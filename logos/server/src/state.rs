use common::{FileMetadata, Message};
use dashmap::DashMap;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::db;

pub type ClientSender = mpsc::UnboundedSender<axum::extract::ws::Message>;

pub struct AppState {
    pub file_index: DashMap<String, FileMetadata>,
    pub clients: DashMap<String, ClientSender>,
    pub db: Pool<Postgres>,
}

impl AppState {
    pub async fn new(db_url: &str) -> Self {
        let pool = Pool::<Postgres>::connect(db_url).await.expect("Failed to connect to DB");
        
        db::init_db(&pool).await.expect("Failed to init DB schema");

        let loaded_files = db::load_state(&pool).await.expect("Failed to load state");
        tracing::info!("💾 Loaded {} files from Database.", loaded_files.len());

        let index = DashMap::new();
        for (k, v) in loaded_files {
            index.insert(k, v);
        }

        Self {
            file_index: index,
            clients: DashMap::new(),
            db: pool,
        }
    }

    pub async fn process_update(&self, incoming: FileMetadata) -> Option<FileMetadata> {
        if let Some(existing) = self.file_index.get(&incoming.path) {
            
            if existing.is_deleted && !incoming.is_deleted && incoming.version < existing.version {
                return None; 
            }

            if incoming.version <= existing.version {
                return None;
            }
        }

        let new_state = incoming.clone();
        
        if let Err(e) = db::save_file(&self.db, &new_state).await {
            tracing::error!("🔥 DB Error: {}", e);
            return None;
        }
        self.file_index.insert(new_state.path.clone(), new_state.clone());
        
        Some(new_state)
    }

    pub fn broadcast(&self, sender_id: &str, msg: axum::extract::ws::Message) {
        for client in self.clients.iter() {
            if client.key() != sender_id {
                let _ = client.value().send(msg.clone());
            }
        }
    }
}

pub type SharedState = Arc<AppState>;