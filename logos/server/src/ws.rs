use crate::state::SharedState;
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


enum TransferState {
    Idle,
    ExpectingBinary { path: String, meta: FileMetadata },
}

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut sender, mut receiver) = socket.split();


    let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();


    let mut client_id = String::new();
    while let Some(Ok(msg)) = receiver.next().await {
        if let WsMessage::Text(text) = msg {
            if let Ok(Message::Register { client_id: id }) = serde_json::from_str(&text) {
                client_id = id;
                tracing::info!("🔌 Client Connected: {}", client_id);
                state.clients.insert(client_id.clone(), tx.clone());
                

                for entry in state.file_index.iter() {
                    let meta = entry.value().clone();
                    if !meta.is_deleted {
                        let _ = tx.send(WsMessage::Text(serde_json::to_string(&Message::FileUpdate { meta }).unwrap()));
                    }
                }
                break;
            }
        }
    }

    if client_id.is_empty() { return; }


    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() { break; }
        }
    });


    let mut transfer_state = TransferState::Idle;
    let upload_dir = PathBuf::from("uploads");
    fs::create_dir_all(&upload_dir).await.ok();

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {

            WsMessage::Text(text) => {
                if let Ok(parsed) = serde_json::from_str::<Message>(&text) {
                    match parsed {
                        Message::StartTransfer { path, size, target_version } => {
                            let meta = FileMetadata {
                                path: path.clone(),
                                size,
                                modified: chrono::Utc::now().timestamp() as u64,
                                version: target_version,
                                hash: String::new(),
                                is_deleted: false,
                            };
                            transfer_state = TransferState::ExpectingBinary { path, meta };
                        }
                        Message::DeleteFile { path } => {

                            let meta = FileMetadata {
                                path: path.clone(),
                                size: 0,
                                modified: chrono::Utc::now().timestamp() as u64,
                                version: 0,
                                hash: String::new(),
                                is_deleted: true,
                            };

                            if let Some(updated) = state.process_update(meta).await {
                                let json = serde_json::to_string(&Message::DeleteFile { path: updated.path }).unwrap();
                                state.broadcast(&client_id, WsMessage::Text(json));
                            }
                        }
                        _ => {}
                    }
                }
            }
            

            WsMessage::Binary(data) => {
                if let TransferState::ExpectingBinary { path, mut meta } = transfer_state {
                    tracing::info!("📥 Received Binary for: {} ({} bytes)", path, data.len());
                    
                    meta.hash = common::calculate_hash(&data);

                    if let Some(updated_meta) = state.process_update(meta).await {
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
                        
                        state.broadcast(&client_id, WsMessage::Text(json));
                        state.broadcast(&client_id, WsMessage::Binary(data));
                    } else {
                        if let Some(current) = state.file_index.get(&path) {
                            let err = Message::ConflictDetected { 
                                path: path.clone(), 
                                server_version: current.version 
                            };
                            let _ = state.clients.get(&client_id).unwrap().send(WsMessage::Text(serde_json::to_string(&err).unwrap()));
                        }
                    }

                    transfer_state = TransferState::Idle;
                }
            }
            _ => {}
        }
    }

    tracing::info!("🔌 Client Disconnected: {}", client_id);
    state.clients.remove(&client_id);
    send_task.abort();
}