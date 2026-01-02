use crate::backend::StorageBackend;
use common::FileMetadata;
use anyhow::{Result, Context};
use async_trait::async_trait;
use suppaftp::FtpStream;
use std::sync::{Arc, Mutex};
use url::Url;
use std::io::Cursor;

pub struct FtpBackend {
    url: Url,
    stream: Arc<Mutex<FtpStream>>,
}

impl FtpBackend {
    pub fn new(url: Url) -> Result<Self> {
        let host = url.host_str().context("Missing host")?;
        let port = url.port().unwrap_or(21);
        let user = url.username();
        let pass = url.password().unwrap_or("anonymous");

        let mut stream = FtpStream::connect(format!("{}:{}", host, port))?;
        if !user.is_empty() {
            stream.login(user, pass)?;
        }

        Ok(Self {
            url,
            stream: Arc::new(Mutex::new(stream)),
        })
    }
}

#[async_trait]
impl StorageBackend for FtpBackend {
    fn get_id(&self) -> String {
        self.url.to_string()
    }

    async fn list_files(&self) -> Result<Vec<FileMetadata>> {
        let stream = self.stream.clone();
        let root = self.url.path().to_string();

        tokio::task::spawn_blocking(move || {
            let mut ftp = stream.lock().unwrap();
            
            if !root.is_empty() && root != "/" {
                let _ = ftp.cwd(&root);
            }

            let filenames = ftp.nlst(None)?;
            
            let mut files = Vec::new();
            for name in filenames {
                if name == "." || name == ".." { continue; }
                
                files.push(FileMetadata {
                    path: name,
                    size: 0, 
                    modified: 0,
                    version: 0,
                    hash: String::new(),
                    is_deleted: false,
                    last_modified_by: None,
                });
            }
            Ok(files)
        }).await?
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let stream = self.stream.clone();
        let path = path.to_string();

        tokio::task::spawn_blocking(move || {
            let mut ftp = stream.lock().unwrap();
            let mut buffer = Cursor::new(Vec::new());
            ftp.retr(&path, |mut data| {
                std::io::copy(&mut data, &mut buffer).map_err(|e| suppaftp::FtpError::ConnectionError(e))?;
                Ok(())
            })?;
            Ok(buffer.into_inner())
        }).await?
    }

    async fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let stream = self.stream.clone();
        let path = path.to_string();
        let data = content.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut ftp = stream.lock().unwrap();
            let mut reader = Cursor::new(data);
            ftp.put_file(&path, &mut reader)?;
            Ok(())
        }).await?
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let stream = self.stream.clone();
        let path = path.to_string();

        tokio::task::spawn_blocking(move || {
            let mut ftp = stream.lock().unwrap();
            ftp.rm(&path)?;
            Ok(())
        }).await?
    }
}