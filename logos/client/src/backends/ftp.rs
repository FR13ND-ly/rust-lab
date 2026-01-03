use crate::backend::StorageBackend;
use common::FileMetadata;
use anyhow::{Result, Context, anyhow};
use async_trait::async_trait;
use suppaftp::FtpStream;
use std::sync::{Arc, Mutex};
use url::Url;
use std::io::Cursor;

pub struct FtpBackend {
    conn: Arc<Mutex<FtpStream>>,
    root: String,
}

impl FtpBackend {
    pub fn new(raw_url: &str) -> Result<Self> {
        let url = Url::parse(raw_url.trim()).context("Invalid URL")?;
        if url.scheme() != "ftp" { return Err(anyhow!("Scheme must be ftp")); }

        let host = url.host_str().context("No host")?;
        let port = url.port().unwrap_or(21);
        let user = url.username();
        let pass = url.password().unwrap_or("anonymous");
        let root = url.path().to_string();

        let mut stream = FtpStream::connect(format!("{}:{}", host, port))?;
        if !user.is_empty() {
            stream.login(user, pass)?;
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(stream)),
            root,
        })
    }
}

#[async_trait]
impl StorageBackend for FtpBackend {
    async fn list_files(&self) -> Result<Vec<FileMetadata>> {
        let c = self.conn.clone();
        let r = self.root.clone();

        tokio::task::spawn_blocking(move || {
            let mut ftp = c.lock().map_err(|_| anyhow!("FTP Mutex poisoned"))?;
            if !r.is_empty() && r != "/" {
                let _ = ftp.cwd(&r);
            }

            let names = ftp.nlst(None)?;
            let mut files = Vec::new();

            for n in names {
                if n == "." || n == ".." { continue; }
                
                let size = ftp.size(&n).unwrap_or(0) as u64;
                let mod_time = ftp.mdtm(&n).map(|t| t.and_utc().timestamp() as u64).unwrap_or(0);

                files.push(FileMetadata {
                    path: n,
                    size, 
                    modified: mod_time,
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
        let c = self.conn.clone();
        let p = path.to_string();

        tokio::task::spawn_blocking(move || {
            let mut ftp = c.lock().map_err(|_| anyhow!("FTP Mutex poisoned"))?;
            let mut buf = Cursor::new(Vec::new());
            ftp.retr(&p, |mut r| {
                std::io::copy(&mut r, &mut buf).map_err(suppaftp::FtpError::ConnectionError)?;
                Ok(())
            })?;
            Ok(buf.into_inner())
        }).await?
    }

    async fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let c = self.conn.clone();
        let p = path.to_string();
        let d = content.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut ftp = c.lock().map_err(|_| anyhow!("FTP Mutex poisoned"))?;
            let mut r = Cursor::new(d);
            ftp.put_file(&p, &mut r)?;
            Ok(())
        }).await?
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let c = self.conn.clone();
        let p = path.to_string();

        tokio::task::spawn_blocking(move || {
            let mut ftp = c.lock().map_err(|_| anyhow!("FTP Mutex poisoned"))?;
            ftp.rm(&p)?;
            Ok(())
        }).await?
    }
}