use common::{FileMetadata, DashboardMessage, Message, ClientInfo};
use dashmap::DashMap;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::db;

pub type ClientSender = mpsc::UnboundedSender<axum::extract::ws::Message>;
pub type DashboardSender = mpsc::UnboundedSender<axum::extract::ws::Message>;

pub struct StorageRoom {
    pub files: DashMap<String, FileMetadata>,
    pub clients: DashMap<String, ClientSender>,
    pub client_names: DashMap<String, String>,
}

impl StorageRoom {
    pub fn new() -> Self {
        Self {
            files: DashMap::new(),
            clients: DashMap::new(),
            client_names: DashMap::new(),
        }
    }
}

pub struct AppState {
    pub rooms: DashMap<String, Arc<StorageRoom>>, 
    pub dashboards: DashMap<usize, DashboardSender>,
    pub db: Pool<Postgres>,
}

impl AppState {
    pub async fn new(db_url: &str) -> Self {
        let pool = Pool::<Postgres>::connect(db_url).await.expect("Failed to connect to DB");
        db::init_db(&pool).await.expect("Failed to init DB schema");
        
        Self {
            rooms: DashMap::new(),
            dashboards: DashMap::new(),
            db: pool,
        }
    }

    pub async fn get_or_load_room(&self, storage_id: &str) -> Arc<StorageRoom> {
        if let Some(room) = self.rooms.get(storage_id) {
            return room.clone();
        }

        let files = db::load_storage_files(&self.db, storage_id).await.unwrap_or_default();
        let room = Arc::new(StorageRoom::new());
        for (k, v) in files {
            room.files.insert(k, v);
        }

        self.rooms.insert(storage_id.to_string(), room.clone());
        room
    }

    pub async fn process_update(&self, storage_id: &str, incoming: FileMetadata) -> Option<FileMetadata> {
        let room = self.get_or_load_room(storage_id).await;

        if let Some(existing) = room.files.get(&incoming.path) {
            if existing.is_deleted && !incoming.is_deleted && incoming.version < existing.version { return None; }
            if incoming.version <= existing.version { return None; }
        }

        let new_state = incoming.clone();
        if let Err(e) = db::save_file(&self.db, storage_id, &new_state).await {
            tracing::error!("🔥 DB Error: {}", e);
            return None;
        }

        room.files.insert(new_state.path.clone(), new_state.clone());
        self.emit_log("info", &format!("File updated in {}: {}", storage_id, new_state.path));
        
        self.emit_stats();
        
        Some(new_state)
    }

    pub async fn broadcast(&self, storage_id: &str, sender_id: &str, msg: axum::extract::ws::Message) {
        if let Some(room) = self.rooms.get(storage_id) {
            for client in room.clients.iter() {
                if client.key() != sender_id {
                    let _ = client.value().send(msg.clone());
                }
            }
        }
    }

    pub async fn emit_storage_list(&self) {
        if let Ok(list) = db::list_storages(&self.db).await {
             let resp = Message::StorageList { storages: list };
             let json = serde_json::to_string(&resp).unwrap();
             self.broadcast_dashboard(axum::extract::ws::Message::Text(json));
        }
    }

    pub fn emit_log(&self, level: &str, message: &str) {
        let msg = DashboardMessage::Log {
            level: level.to_string(),
            message: message.to_string(),
            timestamp: chrono::Utc::now().timestamp() as u64,
        };
        let json = serde_json::to_string(&msg).unwrap();
        self.broadcast_dashboard(axum::extract::ws::Message::Text(json));
    }

    pub fn emit_stats(&self) {
        let active_clients = self.rooms.iter().map(|r| r.clients.len()).sum();
        let total_files = self.rooms.iter().map(|r| r.files.len()).sum();
        
        let mut client_details = Vec::new();
        for room_entry in self.rooms.iter() {
            let storage_id = room_entry.key();
            let room = room_entry.value();
            
            for client_entry in room.client_names.iter() {
                client_details.push(ClientInfo {
                    id: client_entry.key().clone(),
                    name: client_entry.value().clone(),
                    storage_id: storage_id.clone(),
                });
            }
        }

        let msg = DashboardMessage::Stats {
            active_clients,
            total_files,
            client_details,
        };
        let json = serde_json::to_string(&msg).unwrap();
        self.broadcast_dashboard(axum::extract::ws::Message::Text(json));
    }

    fn broadcast_dashboard(&self, msg: axum::extract::ws::Message) {
        self.dashboards.retain(|_, tx| tx.send(msg.clone()).is_ok());
    }
}

pub type SharedState = Arc<AppState>;