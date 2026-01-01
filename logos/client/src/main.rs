mod args;
mod backend;
mod backends;

use args::{Args, Location};
use backend::StorageBackend;
use backends::folder::FolderBackend;
use backends::ftp::FtpBackend;
use backends::zip::ZipBackend;
use clap::Parser;
use common::{Message, calculate_hash};
use futures_util::{SinkExt, StreamExt};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use dialoguer::{theme::ColorfulTheme, Input, Select};

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

    println!("🚀 Logos Client started.");

    let url = "ws://localhost:3000/ws/client";
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect to Server");
    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_write.send(msg).await.is_err() { break; }
        }
    });

    let mut storage_id = String::new();
    let mut initial_files = Vec::new();

    tx.send(WsMessage::Text(serde_json::to_string(&Message::RequestStorageList).unwrap())).unwrap();

    while let Some(Ok(msg)) = ws_read.next().await {
        if let WsMessage::Text(text) = msg {
            if let Ok(parsed) = serde_json::from_str::<Message>(&text) {
                match parsed {
                    Message::StorageList { storages } => {
                        println!("\n📂 Storage Lobby");
                        
                        let mut options: Vec<String> = storages.iter()
                            .map(|s| format!("{} (ID: {})", s.name, s.id))
                            .collect();
                        options.push("✨ Create New Storage".to_string());
                        options.push("🔄 Refresh List".to_string());

                        let selection = Select::with_theme(&ColorfulTheme::default())
                            .with_prompt("Choose an action")
                            .default(0)
                            .items(&options)
                            .interact()
                            .unwrap();

                        if selection < storages.len() {
                            let selected = &storages[selection];
                            println!("Joining {}...", selected.name);
                            tx.send(WsMessage::Text(serde_json::to_string(&Message::JoinStorage { storage_id: selected.id.clone() }).unwrap())).unwrap();
                        } else if selection == storages.len() {
                            let name: String = Input::with_theme(&ColorfulTheme::default())
                                .with_prompt("Enter new storage name")
                                .interact_text()
                                .unwrap();
                            tx.send(WsMessage::Text(serde_json::to_string(&Message::CreateStorage { name: name.trim().to_string() }).unwrap())).unwrap();
                        } else {
                            tx.send(WsMessage::Text(serde_json::to_string(&Message::RequestStorageList).unwrap())).unwrap();
                        }
                    },
                    Message::Welcome { storage_id: sid, files } => {
                        println!("✅ Successfully joined Storage!");
                        storage_id = sid;
                        initial_files = files;
                        break; 
                    },
                    Message::Error { message } => {
                        println!("❌ Error: {}", message);
                        tx.send(WsMessage::Text(serde_json::to_string(&Message::RequestStorageList).unwrap())).unwrap();
                    }
                    _ => {}
                }
            }
        }
    }

    println!("🔄 Starting Synchronization...");
    
    let synced_hashes = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let pending_deletes = Arc::new(Mutex::new(HashSet::<String>::new()));
    
    let mut transfer_state = TransferState::Idle;

    if let Ok(local_files) = backend.list_files().await {
        for local in &local_files {
            let remote = initial_files.iter().find(|f| f.path == local.path);
            let needs_upload = match remote {
                None => true, 
                Some(r) => !r.is_deleted && local.modified > r.modified
            };

            if needs_upload {
                if let Ok(content) = backend.read_file(&local.path).await {
                     let hash = calculate_hash(&content);
                     synced_hashes.lock().unwrap().insert(local.path.clone(), hash);

                     let header = Message::StartTransfer { 
                        path: local.path.clone(), 
                        size: content.len() as u64, 
                        target_version: 0 
                    };
                    tx.send(WsMessage::Text(serde_json::to_string(&header).unwrap())).unwrap();
                    tx.send(WsMessage::Binary(content)).unwrap();
                    println!("📤 Uploading: {}", local.path);
                }
            } else {
                // to do
            }
        }

        if !backend.is_read_only() {
            for remote in &initial_files {
                if remote.is_deleted { continue; }

                let local = local_files.iter().find(|f| f.path == remote.path);
                let needs_download = match local {
                    None => true, 
                    Some(l) => remote.modified > l.modified 
                };

                if needs_download {
                    println!("📥 Requesting initial download: {}", remote.path);
                    let msg = Message::RequestFile { path: remote.path.clone() };
                    tx.send(WsMessage::Text(serde_json::to_string(&msg).unwrap())).unwrap();
                }
            }
        }
    }

    let mut _watcher_guard: Option<RecommendedWatcher> = None;

    if !backend.is_read_only() {
        if let Ok(Location::Folder(_)) = Location::parse(loc) {
            let tx_watcher = tx.clone();
            let backend_watcher = backend.clone();
            let hashes_watcher = synced_hashes.clone();
            let deletes_watcher = pending_deletes.clone();
            let root_path_str = backend.get_id();
            let folder_path = Path::new(&root_path_str);
            
            let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();
            let mut watcher = RecommendedWatcher::new(move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res { notify_tx.send(event).ok(); }
            }, notify::Config::default()).unwrap();

            if let Err(e) = watcher.watch(folder_path, RecursiveMode::Recursive) {
                eprintln!("❌ Watcher failed to start: {}", e);
            }
            _watcher_guard = Some(watcher);

            tokio::spawn(async move {
                while let Some(event) = notify_rx.recv().await {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            for path in event.paths {
                                let path_str = path.to_string_lossy();
                                let root_str = root_path_str.as_str();
                                let clean_path = path_str.trim_start_matches(r"\\?\");
                                let clean_root = root_str.trim_start_matches(r"\\?\");
                                
                                let relative_str = if clean_path.starts_with(clean_root) {
                                    clean_path[clean_root.len()..].trim_start_matches(std::path::MAIN_SEPARATOR)
                                } else { continue; };

                                let relative = relative_str.replace("\\", "/");
                                if relative.is_empty() { continue; }

                                let mut attempts = 0;
                                while attempts < 5 {
                                    if let Ok(content) = backend_watcher.read_file(&relative).await {
                                        let hash = calculate_hash(&content);
                                        
                                        {
                                            let mut guard = hashes_watcher.lock().unwrap();
                                            if let Some(known_hash) = guard.get(&relative) {
                                                if known_hash == &hash {
                                                    break; 
                                                }
                                            }
                                            guard.insert(relative.clone(), hash);
                                        }

                                        let header = Message::StartTransfer { 
                                            path: relative.clone(), 
                                            size: content.len() as u64, 
                                            target_version: 0 
                                        };
                                        tx_watcher.send(WsMessage::Text(serde_json::to_string(&header).unwrap())).ok();
                                        tx_watcher.send(WsMessage::Binary(content)).ok();
                                        println!("📤 Uploading: {}", relative);
                                        break;
                                    }
                                    attempts += 1;
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                            }
                        }
                        EventKind::Remove(_) => {
                             for path in event.paths {
                                let path_str = path.to_string_lossy();
                                let root_str = root_path_str.as_str();
                                let clean_path = path_str.trim_start_matches(r"\\?\");
                                let clean_root = root_str.trim_start_matches(r"\\?\");
                                let relative_str = if clean_path.starts_with(clean_root) {
                                    clean_path[clean_root.len()..].trim_start_matches(std::path::MAIN_SEPARATOR)
                                } else { continue; };

                                let relative = relative_str.replace("\\", "/");
                                
                                {
                                    let mut guard = deletes_watcher.lock().unwrap();
                                    if guard.contains(&relative) {
                                        guard.remove(&relative);
                                        continue;
                                    }
                                }

                                let msg = Message::DeleteFile { path: relative.clone() };
                                tx_watcher.send(WsMessage::Text(serde_json::to_string(&msg).unwrap())).ok();
                                println!("🗑️ Deleting: {}", relative);
                            }
                        }
                        _ => {}
                    }
                }
            });
        }
    }

    while let Some(Ok(msg)) = ws_read.next().await {
        match msg {
            WsMessage::Text(text) => {
                if let Ok(parsed) = serde_json::from_str::<Message>(&text) {
                    match parsed {
                        Message::StartTransfer { path, size: _, target_version } => {
                            if backend.is_read_only() {
                                println!("🔒 Ignoring update for Read-Only backend: {}", path);
                            } else {
                                transfer_state = TransferState::ExpectingBinary { path, version: target_version };
                            }
                        }
                        Message::DeleteFile { path } => {
                            if !backend.is_read_only() {
                                println!("🗑️ Remote Delete: {}", path);
                                pending_deletes.lock().unwrap().insert(path.clone());
                                let _ = backend.delete_file(&path).await;
                                synced_hashes.lock().unwrap().remove(&path);
                            }
                        }
                        Message::ConflictDetected { path, server_version } => {
                            println!("⚔️ Conflict on {}. Server has v{}. Preserving local...", path, server_version);
                            
                            let p_obj = PathBuf::from(&path);
                            let stem = p_obj.file_stem().unwrap_or_default().to_string_lossy();
                            let ext = p_obj.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                            let conflict_name = format!("{} (Conflict-{}).{}", stem, timestamp, ext);
                            
                            pending_deletes.lock().unwrap().insert(path.clone());

                            if let Ok(content) = backend.read_file(&path).await {
                                if let Ok(_) = backend.write_file(&conflict_name, &content).await {
                                    println!("⚠️ Saved conflict copy to '{}'", conflict_name);
                                    
                                    if let Err(e) = backend.delete_file(&path).await {
                                        eprintln!("❌ Failed to remove original conflict file: {}", e);
                                    } else {
                                        tx.send(WsMessage::Text(serde_json::to_string(&Message::RequestFile { path: path.clone() }).unwrap())).unwrap();
                                    }
                                } else {
                                    eprintln!("❌ Failed to write conflict copy. Keeping original.");
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            WsMessage::Binary(data) => {
                if let TransferState::ExpectingBinary { path, version } = transfer_state {
                    println!("📥 Downloading: {} (v{})", path, version);
                    
                    let hash = calculate_hash(&data);
                    synced_hashes.lock().unwrap().insert(path.clone(), hash);

                    let _ = backend.write_file(&path, &data).await;
                    
                    transfer_state = TransferState::Idle;
                }
            }
            _ => {}
        }
    }
    
    send_task.abort();
    println!("❌ Disconnected");
}