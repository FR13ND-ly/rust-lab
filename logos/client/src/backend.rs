use common::FileMetadata;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn list_files(&self) -> Result<Vec<FileMetadata>>;

    async fn read_file(&self, path: &str) -> Result<Vec<u8>>;

    async fn write_file(&self, path: &str, content: &[u8]) -> Result<()>;

    async fn delete_file(&self, path: &str) -> Result<()>;

    fn get_id(&self) -> String;
}