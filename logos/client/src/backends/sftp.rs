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
        let root_path = url.path().to_string();

        let config = Arc::new(client::Config::default());
        let sh = ClientHandler;

        let mut session = client::connect(config, (host, port), sh).await
            .context("Connection failed")?;

        if !user.is_empty() && !session.authenticate_password(&user, &pass).await? {
             return Err(anyhow!("Authentication failed"));
        }

        let channel = session.channel_open_session().await.context("Channel open failed")?;
        channel.request_subsystem(true, "sftp").await.context("SFTP subsystem failed")?;
        let sftp = SftpSession::new(channel.into_stream()).await.context("SFTP init failed")?;

        Ok(Self {
            sftp,
            root_path,
        })
    }
}

#[async_trait]
impl StorageBackend for SftpBackend {
    async fn list_files(&self) -> Result<Vec<FileMetadata>> {
        let paths = self.sftp.read_dir(&self.root_path).await.context("ls failed")?;
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

        let mut file = self.sftp.open(&target).await.context("Open failed")?;
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

        for (i, part) in parts.iter().enumerate().take(parts.len().saturating_sub(1)) {
            if part.is_empty() {
                if i == 0 { cur.push('/'); }
                continue;
            }
            if !cur.ends_with('/') && !cur.is_empty() { cur.push('/'); }
            cur.push_str(part);
            
            if cur != "/" && self.sftp.metadata(&cur).await.is_err() {
                 let _ = self.sftp.create_dir(&cur).await;
            }
        }

        let mut file = self.sftp.create(&target).await.context("Create failed")?;
        file.write_all(content).await?;
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

        self.sftp.remove_file(&target).await?;
        Ok(())
    }
}