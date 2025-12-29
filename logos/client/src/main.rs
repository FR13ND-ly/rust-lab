mod args;
mod backend;
mod backends;

use args::{Args, Location};
use backend::StorageBackend;
use backends::folder::FolderBackend;
use backends::ftp::FtpBackend;
use backends::zip::ZipBackend;
use clap::Parser;
use common::{Message, FileMetadata, calculate_hash};
use futures_util::{SinkExt, StreamExt};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;

enum TransferState {
    Idle,
    ExpectingBinary { path: String, version: u64 },
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    
    let loc = args.locations.first().expect("No location provided");
    
    let backend: Arc<Box<dyn StorageBackend>> = match Location::parse(loc) {
        Ok(Location::Folder(path)) => Arc::new(Box::new(FolderBackend::new(path))),
        Ok(Location::Ftp(url)) => {
            println!("🔌 Connecting to FTP: {}", url);
            match FtpBackend::new(url) {
                Ok(ftp) => Arc::new(Box::new(ftp)),
                Err(e) => panic!("Failed to connect to FTP: {}", e),
            }
        },
        Ok(Location::Zip(path)) => {
            println!("📦 Opening ZIP Archive: {:?}", path);
            match ZipBackend::new(path) {
                Ok(z) => Arc::new(Box::new(z)),
                Err(e) => panic!("Failed to open ZIP: {}", e),
            }
        },
        Err(e) => panic!("Invalid location: {}", e),
    };

    println!("🚀 Logos Client started for: {}", backend.get_id());

    let url = "ws://localhost:3000/ws/client";
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect to Server");
    let (mut ws_write, mut ws_read) = ws_stream.split();

    let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
    
    let ignore_list = Arc::new(Mutex::new(HashSet::<String>::new()));

    let client_id = backend.get_id();
    let reg_msg = Message::Register { client_id: client_id.clone() };
    ws_write.send(WsMessage::Text(serde_json::to_string(&reg_msg).unwrap())).await.unwrap();

    println!("🔍 Scanning files...");
    if let Ok(files) = backend.list_files().await {
        for file in files {
            let msg = Message::FileUpdate { meta: file };
            tx.send(WsMessage::Text(serde_json::to_string(&msg).unwrap())).unwrap();
        }
    }

    if let Ok(Location::Folder(folder_path)) = Location::parse(loc) {
        let tx_watcher = tx.clone();
        let backend_watcher = backend.clone();
        let ignore_watcher = ignore_list.clone();
        let root_path_str = backend.get_id();
        
        let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();
        let mut watcher = RecommendedWatcher::new(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res { notify_tx.send(event).ok(); }
        }, notify::Config::default()).unwrap();

        watcher.watch(&folder_path, RecursiveMode::Recursive).unwrap();

        tokio::spawn(async move {
            while let Some(event) = notify_rx.recv().await {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        for path in event.paths {
                            let root = Path::new(&root_path_str);
                            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace("\\", "/");

                            if ignore_watcher.lock().unwrap().contains(&relative) {
                                continue; 
                            }

                            if let Ok(content) = backend_watcher.read_file(&relative).await {
                                let _hash = calculate_hash(&content);
                                
                                let header = Message::StartTransfer { 
                                    path: relative.clone(), 
                                    size: content.len() as u64, 
                                    target_version: 0 
                                };
                                tx_watcher.send(WsMessage::Text(serde_json::to_string(&header).unwrap())).ok();
                                tx_watcher.send(WsMessage::Binary(content)).ok();
                                
                                println!("📤 Uploading: {}", relative);
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
    } else {
        println!("ℹ️ Watcher disabled for this backend type (FTP/ZIP). Running in Sync-Only mode.");
    }

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_write.send(msg).await.is_err() { break; }
        }
    });

    let mut transfer_state = TransferState::Idle;

    while let Some(Ok(msg)) = ws_read.next().await {
        match msg {
            WsMessage::Text(text) => {
                if let Ok(parsed) = serde_json::from_str::<Message>(&text) {
                    match parsed {
                        Message::StartTransfer { path, size: _, target_version } => {
                            transfer_state = TransferState::ExpectingBinary { path, version: target_version };
                        }
                        Message::ConflictDetected { path, server_version } => {
                            println!("⚔️ Conflict on {}. Server has v{}. Renaming local...", path, server_version);
                        }
                        _ => {}
                    }
                }
            }
            WsMessage::Binary(data) => {
                if let TransferState::ExpectingBinary { path, version: _ } = transfer_state {
                    println!("📥 Downloading: {} ({} bytes)", path, data.len());
                    
                    ignore_list.lock().unwrap().insert(path.clone());

                    if let Err(e) = backend.write_file(&path, &data).await {
                        eprintln!("❌ Write failed: {}", e);
                    } else {
                        let ignore_clone = ignore_list.clone();
                        let path_clone = path.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            ignore_clone.lock().unwrap().remove(&path_clone);
                        });
                    }

                    transfer_state = TransferState::Idle;
                }
            }
            _ => {}
        }
    }

    send_task.abort();
    println!("❌ Disconnected from Server");
}