use crate::backend::StorageBackend;
use common::FileMetadata;
use anyhow::{Result, Context};
use async_trait::async_trait;
use std::path::{PathBuf};
use tokio::fs;
use walkdir::WalkDir;

pub struct FolderBackend {
    root: PathBuf,
}

impl FolderBackend {
    pub fn new(path: PathBuf) -> Self {
        if !path.exists() {
            let _ = std::fs::create_dir_all(&path);
        }
        let root = std::fs::canonicalize(&path).unwrap_or(path);
        Self { root }
    }

    fn resolve(&self, rel: &str) -> PathBuf {
        let clean = rel.replace("/", std::path::MAIN_SEPARATOR_STR);
        self.root.join(clean)
    }
}

#[async_trait]
impl StorageBackend for FolderBackend {
    async fn list_files(&self) -> Result<Vec<FileMetadata>> {
        let root = self.root.clone();
        
        let files = tokio::task::spawn_blocking(move || {
            let mut list = Vec::new();
            for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() && let Ok(meta) = entry.metadata() {
                    let path = entry.path().strip_prefix(&root).unwrap()
                        .to_string_lossy()
                        .replace("\\", "/");
                    
                    if path.starts_with(".git") { continue; }

                    list.push(FileMetadata {
                        path,
                        size: meta.len(),
                        modified: meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
                        version: 0,
                        hash: String::new(),
                        is_deleted: false,
                        last_modified_by: None,
                    });
                }
            }
            list
        }).await?;

        Ok(files)
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        fs::read(self.resolve(path)).await.context("fs read failed")
    }

    async fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let target = self.resolve(path);
        if let Some(p) = target.parent() {
            fs::create_dir_all(p).await?;
        }
        
        match fs::write(&target, content).await {
            Ok(_) => Ok(()),
            Err(e) if e.raw_os_error() == Some(32) => {
                println!("[!] File locked: {:?}. Creating copy.", target);
                let stem = target.file_stem().unwrap().to_string_lossy();
                let ext = target.extension().map(|e| e.to_string_lossy()).unwrap_or_default();
                let copy = target.with_file_name(format!("{}_copy.{}", stem, ext));
                fs::write(&copy, content).await?;
                Ok(())
            },
            Err(e) => Err(e.into())
        }
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let target = self.resolve(path);
        if target.exists() {
            fs::remove_file(target).await?;
        }
        Ok(())
    }
}