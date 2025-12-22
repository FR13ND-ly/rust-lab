use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMetadata {
    pub path: String,
    pub size: u64,
    pub modified: u64,
    
    pub version: u64,
    
    pub hash: String,

    pub is_deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Register { client_id: String },

    FileUpdate { meta: FileMetadata },

    StartTransfer { 
        path: String, 
        size: u64,
        target_version: u64 
    },
    
    DeleteFile { path: String },

    ConflictDetected { 
        path: String, 
        server_version: u64 
    },

    Error { message: String }
}

pub fn calculate_hash(content: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}