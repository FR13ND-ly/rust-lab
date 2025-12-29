use crate::backend::StorageBackend;
use common::FileMetadata;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct ZipBackend {
    file_path: PathBuf,
    archive: Arc<Mutex<zip::ZipArchive<File>>>,
}

impl ZipBackend {
    pub fn new(path: PathBuf) -> Result<Self> {
        let file = File::open(&path)?;
        let archive = zip::ZipArchive::new(file)?;
        Ok(Self {
            file_path: path,
            archive: Arc::new(Mutex::new(archive)),
        })
    }
}

#[async_trait]
impl StorageBackend for ZipBackend {
    fn get_id(&self) -> String {
        self.file_path.to_string_lossy().to_string()
    }

    async fn list_files(&self) -> Result<Vec<FileMetadata>> {
        let archive = self.archive.clone();
        
        tokio::task::spawn_blocking(move || {
            let mut zip = archive.lock().unwrap();
            let mut files = Vec::new();
            
            for i in 0..zip.len() {
                if let Ok(file) = zip.by_index(i) {
                    if file.is_file() {
                        files.push(FileMetadata {
                            path: file.name().to_string(),
                            size: file.size(),
                            modified: 0,
                            version: 0,
                            hash: String::new(),
                            is_deleted: false,
                        });
                    }
                }
            }
            Ok(files)
        }).await?
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let archive = self.archive.clone();
        let path = path.to_string();
        
        tokio::task::spawn_blocking(move || {
            let mut zip = archive.lock().unwrap();
            let mut file = zip.by_name(&path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            Ok(buffer)
        }).await?
    }

    async fn write_file(&self, _path: &str, _content: &[u8]) -> Result<()> {
        Err(anyhow!("ZIP archives are Read-Only in Logos."))
    }

    async fn delete_file(&self, _path: &str) -> Result<()> {
        Err(anyhow!("ZIP archives are Read-Only in Logos."))
    }
}