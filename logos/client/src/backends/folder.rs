use crate::backend::StorageBackend;
use common::FileMetadata;
use anyhow::{Result, Context};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;
use walkdir::WalkDir;

pub struct FolderBackend {
    root: PathBuf,
}

impl FolderBackend {
    pub fn new(path: PathBuf) -> Self {
        if !path.exists() {
            std::fs::create_dir_all(&path).expect("Failed to create root directory");
        }
    
        let absolute_root = std::fs::canonicalize(&path).unwrap_or(path);

        Self { root: absolute_root }
    }

    fn to_full_path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    async fn safe_write(&self, path: PathBuf, content: &[u8]) -> Result<()> {
        match fs::write(&path, content).await {
            Ok(_) => Ok(()),
            Err(e) => {
                if is_file_locked(&e) {
                    println!("⚠️ File Locked: {:?}. Creating conflict copy.", path);
                    let new_path = generate_conflict_name(&path);
                    fs::write(&new_path, content).await.context("Failed to write conflict copy")?;
                    Ok(())
                } else {
                    Err(e.into())
                }
            }
        }
    }
}

#[async_trait]
impl StorageBackend for FolderBackend {
    fn get_id(&self) -> String {
        self.root.to_string_lossy().to_string()
    }

    async fn list_files(&self) -> Result<Vec<FileMetadata>> {
        let root = self.root.clone();
        
        let files = tokio::task::spawn_blocking(move || {
            let mut results = Vec::new();
            for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    if let Ok(metadata) = entry.metadata() {
                        let path = entry.path().strip_prefix(&root).unwrap().to_string_lossy().replace("\\", "/");
                        
                        if path.starts_with(".git") || path.starts_with("target") { continue; }

                        results.push(FileMetadata {
                            path,
                            size: metadata.len(),
                            modified: metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                                .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
                            version: 0,
                            hash: String::new(),
                            is_deleted: false,
                        });
                    }
                }
            }
            results
        }).await?;

        Ok(files)
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let full_path = self.to_full_path(path);
        fs::read(&full_path).await.context("Read failed")
    }

    async fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let full_path = self.to_full_path(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        self.safe_write(full_path, content).await
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let full_path = self.to_full_path(path);
        if full_path.exists() {
            fs::remove_file(full_path).await?;
        }
        Ok(())
    }
}

fn is_file_locked(err: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::PermissionDenied | ErrorKind::AlreadyExists => true,
        _ => err.raw_os_error() == Some(32)
    }
}

fn generate_conflict_name(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap().to_string_lossy();
    let ext = path.extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let new_name = format!("{} (Logos Copy){}", stem, ext);
    path.with_file_name(new_name)
}