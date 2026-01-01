use crate::state::SharedState;
use crate::db;
use axum::{
    extract::{ws::{Message as WsMessage, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
};
use common::{Message, FileMetadata};
use futures::{sink::SinkExt, stream::StreamExt};
use tokio::sync::mpsc;
use std::path::PathBuf;
use tokio::fs;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

enum SessionState {
    Lobby,
    Synced { storage_id: String },
}

enum TransferState {
    Idle,
    ExpectingBinary { path: String, meta: FileMetadata },
}

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() { break; }
        }
    });

    let mut session = SessionState::Lobby;
    let mut transfer_state = TransferState::Idle;
    let client_id = uuid::Uuid::new_v4().to_string();

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            WsMessage::Text(text) => {
                if let Ok(parsed) = serde_json::from_str::<Message>(&text) {
                    match parsed {
                        Message::RequestStorageList => {
                            if let Ok(list) = db::list_storages(&state.db).await {
                                let resp = Message::StorageList { storages: list };
                                tx.send(WsMessage::Text(serde_json::to_string(&resp).unwrap())).ok();
                            }
                        },
                        Message::CreateStorage { name } => {
                            match db::create_storage(&state.db, &name).await {
                                Ok(info) => {
                                    if let Ok(list) = db::list_storages(&state.db).await {
                                        let resp = Message::StorageList { storages: list };
                                        tx.send(WsMessage::Text(serde_json::to_string(&resp).unwrap())).ok();
                                    }
                                }
                                Err(e) => {
                                    let err = Message::Error { message: format!("Create failed: {}", e) };
                                    tx.send(WsMessage::Text(serde_json::to_string(&err).unwrap())).ok();
                                }
                            }
                        },
                        Message::JoinStorage { storage_id } => {
                            let room = state.get_or_load_room(&storage_id).await;
                            
                            room.clients.insert(client_id.clone(), tx.clone());
                            
                            let mut files = Vec::new();
                            for entry in room.files.iter() {
                                files.push(entry.value().clone());
                            }
                            let welcome = Message::Welcome { storage_id: storage_id.clone(), files };
                            tx.send(WsMessage::Text(serde_json::to_string(&welcome).unwrap())).ok();
                            
                            state.emit_log("info", &format!("Client {} joined storage {}", client_id, &storage_id));
                            session = SessionState::Synced { storage_id };
                        },
                        
                        Message::StartTransfer { path, size, target_version } => {
                            if let SessionState::Synced { storage_id } = &session {
                                let room = state.get_or_load_room(storage_id).await;
                                
                                let effective_version = if target_version == 0 {
                                    room.files.get(&path)
                                        .map(|existing| existing.version + 1)
                                        .unwrap_or(1)
                                } else {
                                    target_version
                                };

                                let meta = FileMetadata {
                                    path: path.clone(),
                                    size,
                                    modified: chrono::Utc::now().timestamp() as u64,
                                    version: effective_version,
                                    hash: String::new(),
                                    is_deleted: false,
                                };
                                transfer_state = TransferState::ExpectingBinary { path, meta };
                            }
                        },
                        Message::RequestFile { path } => {
                            if let SessionState::Synced { storage_id } = &session {
                                let upload_dir = PathBuf::from("uploads").join(storage_id);
                                let file_path = upload_dir.join(&path);
                                let room = state.get_or_load_room(storage_id).await;

                                if let Some(meta) = room.files.get(&path) {
                                    if let Ok(content) = fs::read(&file_path).await {
                                        let header = Message::StartTransfer { 
                                            path: path.clone(), 
                                            size: meta.size, 
                                            target_version: meta.version 
                                        };
                                        tx.send(WsMessage::Text(serde_json::to_string(&header).unwrap())).ok();
                                        tx.send(WsMessage::Binary(content)).ok();
                                        state.emit_log("info", &format!("Serving conflict recovery for {}", path));
                                    }
                                }
                            }
                        },
                        Message::DeleteFile { path } => {
                            if let SessionState::Synced { storage_id } = &session {
                                let room = state.get_or_load_room(storage_id).await;
                                let version = room.files.get(&path).map(|m| m.version + 1).unwrap_or(1);

                                let meta = FileMetadata {
                                    path: path.clone(),
                                    size: 0,
                                    modified: chrono::Utc::now().timestamp() as u64,
                                    version,
                                    hash: String::new(),
                                    is_deleted: true,
                                };

                                if let Some(updated) = state.process_update(storage_id, meta).await {
                                    let json = serde_json::to_string(&Message::DeleteFile { path: updated.path }).unwrap();
                                    state.broadcast(storage_id, &client_id, WsMessage::Text(json)).await;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            },
            WsMessage::Binary(data) => {
                if let SessionState::Synced { storage_id } = &session {
                    if let TransferState::ExpectingBinary { path, mut meta } = transfer_state {
                        meta.hash = common::calculate_hash(&data);

                        if let Some(updated_meta) = state.process_update(storage_id, meta).await {
                            let upload_dir = PathBuf::from("uploads").join(storage_id);
                            let file_path = upload_dir.join(&path);
                            
                            if let Some(parent) = file_path.parent() {
                                fs::create_dir_all(parent).await.ok();
                            }
                            fs::write(&file_path, &data).await.ok();

                            let header = Message::StartTransfer { 
                                path: updated_meta.path.clone(), 
                                size: updated_meta.size, 
                                target_version: updated_meta.version 
                            };
                            let json = serde_json::to_string(&header).unwrap();
                            
                            state.broadcast(storage_id, &client_id, WsMessage::Text(json)).await;
                            state.broadcast(storage_id, &client_id, WsMessage::Binary(data)).await;
                        } else {
                            let room = state.get_or_load_room(storage_id).await;
                            if let Some(current) = room.files.get(&path) {
                                let err = Message::ConflictDetected { 
                                    path: path.clone(), 
                                    server_version: current.version 
                                };
                                tx.send(WsMessage::Text(serde_json::to_string(&err).unwrap())).ok();
                            }
                        }
                        transfer_state = TransferState::Idle;
                    }
                }
            }
            _ => {}
        }
    }

    if let SessionState::Synced { storage_id } = &session {
         if let Some(room) = state.rooms.get(storage_id) {
             room.clients.remove(&client_id);
         }
    }
    send_task.abort();
}