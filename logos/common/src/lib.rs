use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMetadata {
    pub path: String,
    pub size: u64,
    pub modified: u64,
    pub version: u64,
    pub hash: String,
    pub is_deleted: bool,
    pub last_modified_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub id: String,
    pub name: String,
    pub storage_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Register { client_id: String },
    RegisterDashboard, 

    RequestStorageList,
    StorageList { storages: Vec<StorageInfo> },
    CreateStorage { name: String },
    DeleteStorage { storage_id: String },
    JoinStorage { storage_id: String, client_name: String },
    
    Welcome { storage_id: String, files: Vec<FileMetadata> },
    FileUpdate { meta: FileMetadata },
    StartTransfer { path: String, size: u64, target_version: u64 },
    RequestFile { path: String },
    DeleteFile { path: String },
    ConflictDetected { path: String, server_version: u64 },
    Error { message: String }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DashboardMessage {
    Snapshot { files: Vec<FileMetadata> },
    Log { 
        level: String, 
        message: String, 
        timestamp: u64
    },
    Stats { 
        active_clients: usize, 
        total_files: usize,
        client_details: Vec<ClientInfo>
    }
}

pub fn calculate_hash(content: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}