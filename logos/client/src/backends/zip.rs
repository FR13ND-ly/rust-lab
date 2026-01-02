use crate::backend::StorageBackend;
use common::FileMetadata;
use anyhow::{Result, anyhow, Context};
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

    fn sanitize_path(path: &str) -> String {
        path.replace("\\", "/")
    }

    fn zip_time_to_unix(dt: zip::DateTime) -> u64 {
        let year = dt.year() as u64;
        let month = dt.month() as u64;
        let day = dt.day() as u64;
        let hour = dt.hour() as u64;
        let min = dt.minute() as u64;
        let sec = dt.second() as u64;

        (year * 31536000) + (month * 2592000) + (day * 86400) + (hour * 3600) + (min * 60) + sec
    }
}

#[async_trait]
impl StorageBackend for ZipBackend {
    fn get_id(&self) -> String {
        self.file_path.to_string_lossy().to_string()
    }

    fn is_read_only(&self) -> bool { true }

    async fn list_files(&self) -> Result<Vec<FileMetadata>> {
        let archive = self.archive.clone();
        tokio::task::spawn_blocking(move || {
            let mut zip = archive.lock().unwrap();
            let mut files = Vec::new();
            for i in 0..zip.len() {
                if let Ok(file) = zip.by_index(i) {
                    if file.is_file() {
                        let path = Self::sanitize_path(file.name());
                        let modified = Self::zip_time_to_unix(file.last_modified());

                        files.push(FileMetadata {
                            path,
                            size: file.size(),
                            modified, 
                            version: 0,
                            hash: String::new(),
                            is_deleted: false,
                            last_modified_by: None,
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
            let mut buffer = Vec::new();
            
            if let Ok(mut file) = zip.by_name(&path) {
                file.read_to_end(&mut buffer)?;
                return Ok(buffer);
            }

            let win_path = path.replace("/", "\\");
            let mut file = zip.by_name(&win_path).context(format!("File not found in zip: {}", path))?;
            
            file.read_to_end(&mut buffer)?;
            Ok(buffer)
        }).await?
    }

    async fn write_file(&self, _path: &str, _content: &[u8]) -> Result<()> {
        Err(anyhow!("ZIP archives are Read-Only."))
    }

    async fn delete_file(&self, _path: &str) -> Result<()> {
        Err(anyhow!("ZIP archives are Read-Only."))
    }
}