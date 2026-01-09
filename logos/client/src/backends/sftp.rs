use crate::backend::StorageBackend;
use common::FileMetadata;
use anyhow::{Result, Context, anyhow};
use async_trait::async_trait;
use url::Url;
use std::sync::Arc;
use russh::*;
use russh_sftp::client::SftpSession;
use russh_keys::*;
use percent_encoding::percent_decode_str;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct ClientHandler;

#[async_trait]
impl client::Handler for ClientHandler {
   type Error = russh::Error;
   async fn check_server_key(&mut self, _key: &key::PublicKey) -> Result<bool, Self::Error> {
       Ok(true) 
   }
}

pub struct SftpBackend {
    sftp: SftpSession,
    root_path: String,
}

impl SftpBackend {
    pub async fn new(url_str: &str) -> Result<Self> {
        let url = Url::parse(url_str.trim()).context("Invalid URL")?;

        if url.scheme() != "sftp" && url.scheme() != "ssh" {
            return Err(anyhow!("Scheme must be sftp or ssh"));
        }

        let host = url.host_str().context("No host provided")?;
        let port = url.port().unwrap_or(22);
        
        let user = percent_decode_str(url.username()).decode_utf8_lossy().to_string();
        let pass = percent_decode_str(url.password().unwrap_or("")).decode_utf8_lossy().to_string();
        
        let raw_path = url.path().to_string();
        let root_path = raw_path.replace('\\', "/");

        let config = Arc::new(client::Config::default());
        let sh = ClientHandler;

        let mut session = client::connect(config, (host, port), sh).await
            .context("SSH Connection failed")?;

        if !user.is_empty() && !session.authenticate_password(&user, &pass).await? {
             return Err(anyhow!("Authentication failed for user: {}", user));
        }

        let channel = session.channel_open_session().await.context("Failed to open SSH channel")?;
        channel.request_subsystem(true, "sftp").await.context("Failed to request SFTP subsystem")?;
        let sftp = SftpSession::new(channel.into_stream()).await.context("SFTP session initialization failed")?;

        if sftp.metadata(&root_path).await.is_err() {
            println!("[*] Remote root '{}' missing, attempting to create...", root_path);
            
            let parts: Vec<&str> = root_path.split('/').collect();
            let mut cur = String::new();
            
            for part in parts {
                if part.is_empty() { 
                    if cur.is_empty() { cur.push('/'); }
                    continue; 
                }
                
                if !cur.ends_with('/') && !cur.is_empty() {
                    cur.push('/');
                }
                cur.push_str(part);
                
                if cur != "/" && sftp.metadata(&cur).await.is_err() {
                    sftp.create_dir(&cur).await.context(format!("Failed to create root directory segment: {}", cur))?;
                }
            }
            
            if sftp.metadata(&root_path).await.is_err() {
                return Err(anyhow!("Could not ensure remote root directory exists: {}", root_path));
            }
        }

        Ok(Self {
            sftp,
            root_path,
        })
    }
}

#[async_trait]
impl StorageBackend for SftpBackend {
    async fn list_files(&self) -> Result<Vec<FileMetadata>> {
        let paths = self.sftp.read_dir(&self.root_path).await.context("Failed to list remote directory")?;
        let mut files = Vec::new();

        for file in paths {
            let name = file.file_name();
            if name == "." || name == ".." { continue; }

            let meta = file.metadata();
            if !meta.is_regular() { continue; }

            files.push(FileMetadata {
                path: name,
                size: meta.size.unwrap_or(0), 
                modified: meta.mtime.unwrap_or(0) as u64,
                version: 0,
                hash: String::new(),
                is_deleted: false,
                last_modified_by: None,
            });
        }
        Ok(files)
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let clean = path.replace('\\', "/");
        let target = if self.root_path.ends_with('/') {
            format!("{}{}", self.root_path, clean)
        } else {
            format!("{}/{}", self.root_path, clean)
        };

        let mut file = self.sftp.open(&target).await.context(format!("Failed to open {} for reading", target))?;
        let size = file.metadata().await?.size.unwrap_or(0);
        
        let mut buf = Vec::with_capacity(size as usize);
        file.read_to_end(&mut buf).await?;
        
        Ok(buf)
    }

    async fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let clean = path.replace('\\', "/");
        let target = if self.root_path.ends_with('/') {
            format!("{}{}", self.root_path, clean)
        } else {
            format!("{}/{}", self.root_path, clean)
        };

        let parts: Vec<&str> = target.split('/').collect();
        let mut cur = String::new();

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 { break; }
            
            if part.is_empty() {
                if i == 0 { cur.push('/'); }
                continue;
            }
            
            if !cur.ends_with('/') && !cur.is_empty() {
                cur.push('/');
            }
            cur.push_str(part);

            if cur != "/" {
                if self.sftp.metadata(&cur).await.is_err() {
                    let _ = self.sftp.create_dir(&cur).await;
                }
            }
        }

        let mut file = self.sftp.create(&target).await
            .context(format!("SFTP 'create' failed for target: {}. Check folder permissions.", target))?;
            
        file.write_all(content).await.context("Failed to write content to remote file")?;
        file.flush().await?;
        file.shutdown().await?;
        
        Ok(())
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let clean = path.replace('\\', "/");
        let target = if self.root_path.ends_with('/') {
            format!("{}{}", self.root_path, clean)
        } else {
            format!("{}/{}", self.root_path, clean)
        };

        if self.sftp.metadata(&target).await.is_ok() {
            self.sftp.remove_file(&target).await.context("Failed to delete remote file")?;
        }
        Ok(())
    }
}