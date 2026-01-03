use crate::backend::StorageBackend;
use common::FileMetadata;
use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zip::ZipArchive;

pub struct ZipBackend {
    zip: Arc<Mutex<ZipArchive<File>>>,
}

impl ZipBackend {
    pub fn new(path: PathBuf) -> Result<Self> {
        let f = File::open(&path)?;
        let z = ZipArchive::new(f)?;
        Ok(Self {
            zip: Arc::new(Mutex::new(z)),
        })
    }
}

#[async_trait]
impl StorageBackend for ZipBackend {
    fn is_read_only(&self) -> bool { true }

    async fn list_files(&self) -> Result<Vec<FileMetadata>> {
        let z = self.zip.clone();
        tokio::task::spawn_blocking(move || {
            let mut archive = z.lock().map_err(|_| anyhow!("Zip mutex poisoned"))?;
            let mut list = Vec::new();
            
            for i in 0..archive.len() {
                if let Ok(f) = archive.by_index(i) && f.is_file() {
                    let name = f.name().replace("\\", "/");
                    let dt = f.last_modified();
                    let ts = (dt.year() as u64).saturating_sub(1970) * 31536000 
                            + (dt.month() as u64) * 2592000 
                            + (dt.day() as u64) * 86400;

                    list.push(FileMetadata {
                        path: name,
                        size: f.size(),
                        modified: ts, 
                        version: 0,
                        hash: String::new(),
                        is_deleted: false,
                        last_modified_by: None,
                    });
                }
            }
            Ok(list)
        }).await?
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let z = self.zip.clone();
        let p = path.to_string();
        
        tokio::task::spawn_blocking(move || {
            let mut archive = z.lock().map_err(|_| anyhow!("Zip mutex poisoned"))?;
            let mut buf = Vec::new();
            
            if let Ok(mut f) = archive.by_name(&p) {
                f.read_to_end(&mut buf)?;
                return Ok(buf);
            }
            
            let win_p = p.replace("/", "\\");
            let mut f = archive.by_name(&win_p).context("File not found")?;
            f.read_to_end(&mut buf)?;
            Ok(buf)
        }).await?
    }

    async fn write_file(&self, _: &str, _: &[u8]) -> Result<()> {
        Err(anyhow!("Zip is read-only"))
    }

    async fn delete_file(&self, _: &str) -> Result<()> {
        Err(anyhow!("Zip is read-only"))
    }
}