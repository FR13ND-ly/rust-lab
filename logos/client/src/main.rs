mod args;
mod backend;
mod backends;
mod config;

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
    ExpectingBinary { path: String },
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    
    let config_path = args.config.clone().unwrap_or_else(|| "logos_config.json".to_string());
    let mut config = config::AppConfig::load(&config_path).await;

    let client_name = if let Some(name) = &config.client_name {
        name.clone()
    } else {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Unknown Client".to_string())
    };

    let loc_str = if !args.locations.is_empty() {
        args.locations.first().cloned()
    } else {
        config.location.clone()
    };

    let loc_str = loc_str.expect("❌ No storage location provided. Use args (folder:./path) or config file.");

    let backend: Arc<Box<dyn StorageBackend>> = match Location::parse(&loc_str) {
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

    println!("🚀 Logos Client started as '{}'", client_name);

    let url = "ws://localhost:3000/ws/client";
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect to Server");
    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_write.send(msg).await.is_err() { break; }
        }
    });

    let mut initial_files = Vec::new();

    if let Some(target_id) = &config.storage_id {
        println!("⚙️ Auto-joining storage from config: {}", target_id);
        tx.send(WsMessage::Text(serde_json::to_string(&Message::JoinStorage { 
            storage_id: target_id.clone(),
            client_name: client_name.clone()
        }).unwrap())).unwrap();
    } else {
        tx.send(WsMessage::Text(serde_json::to_string(&Message::RequestStorageList).unwrap())).unwrap();
    }

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
                            tx.send(WsMessage::Text(serde_json::to_string(&Message::JoinStorage { 
                                storage_id: selected.id.clone(),
                                client_name: client_name.clone()
                            }).unwrap())).unwrap();
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
                        initial_files = files;

                        println!("💾 Updating configuration...");
                        config.client_name = Some(client_name.clone());
                        config.location = Some(loc_str.clone());
                        config.storage_id = Some(sid);
                        config.save(&config_path).await;

                        break;
                    },
                    Message::Error { message } => {
                        println!("❌ Error: {}", message);
                        if config.storage_id.is_some() {
                            println!("⚠️ Auto-join failed. Clearing saved storage ID.");
                            config.storage_id = None;
                        }
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
        println!("ℹ️  Found {} files in local backend.", local_files.len());

        for local in &local_files {
            let remote = initial_files.iter().find(|f| f.path == local.path);
            let needs_upload = match remote {
                None => true, 
                Some(r) => local.modified > r.modified
            };

            if needs_upload {
                match backend.read_file(&local.path).await {
                     Ok(content) => {
                         if local.modified > 40_000_000_000 {
                            println!("⚠️  Warning: Local file '{}' has a very large timestamp ({}). This might prevent future updates.", local.path, local.modified);
                         }

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
                     },
                     Err(e) => {
                         eprintln!("⚠️ Failed to read file for upload '{}': {}", local.path, e);
                     }
                }
            } else if let Some(r) = remote {
                println!("⏹️  Skipping '{}': Local (v{}) <= Remote (v{}) [Deleted: {}]", local.path, local.modified, r.modified, r.is_deleted);
                let will_download = !r.is_deleted && r.modified > local.modified;
                if !will_download && !r.is_deleted {
                    if let Ok(content) = backend.read_file(&local.path).await {
                        let hash = calculate_hash(&content);
                        synced_hashes.lock().unwrap().insert(local.path.clone(), hash);
                    }
                }
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
    } else {
        eprintln!("⚠️ Failed to list local files. Is the path correct?");
    }

    let mut _watcher_guard: Option<RecommendedWatcher> = None;

    if !backend.is_read_only() {
        if let Ok(Location::Folder(raw_path)) = Location::parse(&loc_str) {
            let tx_watcher = tx.clone();
            let backend_watcher = backend.clone();
            let hashes_watcher = synced_hashes.clone();
            let deletes_watcher = pending_deletes.clone();
            
            let abs_root = std::fs::canonicalize(&raw_path).unwrap_or(raw_path);
            let folder_path = abs_root.clone();
            
            let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();
            let mut watcher = RecommendedWatcher::new(move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res { notify_tx.send(event).ok(); }
            }, notify::Config::default()).unwrap();

            if let Err(e) = watcher.watch(&folder_path, RecursiveMode::Recursive) {
                eprintln!("❌ Watcher failed to start: {}", e);
            }
            _watcher_guard = Some(watcher);

            let value = synced_hashes.clone();
            tokio::spawn(async move {
                let to_relative = |sys_path: &Path| -> Option<String> {
                     sys_path.strip_prefix(&abs_root).ok()
                        .map(|p| p.to_string_lossy().replace("\\", "/"))
                        .filter(|s| !s.is_empty())
                };

                while let Some(event) = notify_rx.recv().await {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            if event.paths.len() == 2 {
                                let old_path = &event.paths[0];
                                let new_path = &event.paths[1];

                                if let Some(relative_old) = to_relative(old_path) {
                                     let mut files_to_delete = Vec::new();
                                     {
                                        let mut guard = hashes_watcher.lock().unwrap();
                                        if guard.contains_key(&relative_old) { files_to_delete.push(relative_old.clone()); }
                                        let dir_prefix = format!("{}/", relative_old);
                                        for key in guard.keys() {
                                            if key.starts_with(&dir_prefix) { files_to_delete.push(key.clone()); }
                                        }
                                        for f in &files_to_delete { guard.remove(f); }
                                     }
                                     for f in files_to_delete {
                                         let msg = Message::DeleteFile { path: f };
                                         tx_watcher.send(WsMessage::Text(serde_json::to_string(&msg).unwrap())).ok();
                                     }
                                }

                                if let Some(relative_new) = to_relative(new_path) {
                                    if new_path.is_file() {
                                        if let Ok(content) = backend_watcher.read_file(&relative_new).await {
                                            let hash = calculate_hash(&content);
                                            value.lock().unwrap().insert(relative_new.clone(), hash);
                                            let header = Message::StartTransfer { 
                                                path: relative_new.clone(), 
                                                size: content.len() as u64, 
                                                target_version: 0 
                                            };
                                            tx_watcher.send(WsMessage::Text(serde_json::to_string(&header).unwrap())).ok();
                                            tx_watcher.send(WsMessage::Binary(content)).ok();
                                            println!("📤 Uploading (Renamed): {}", relative_new);
                                        }
                                    }
                                }
                                continue;
                            }

                            for path in event.paths {
                                if let Some(relative) = to_relative(&path) {
                                    let mut attempts = 0;
                                    while attempts < 5 {
                                        if let Ok(content) = backend_watcher.read_file(&relative).await {
                                            let hash = calculate_hash(&content);
                                            
                                            {
                                                let mut guard = hashes_watcher.lock().unwrap();
                                                if let Some(known_hash) = guard.get(&relative) {
                                                    if known_hash == &hash { break; }
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
                                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                    }
                                }
                            }
                        }
                        
                        EventKind::Remove(_) => {
                             for path in event.paths {
                                if let Some(relative) = to_relative(&path) {
                                    let mut files_to_delete = Vec::new();
                                    {
                                        let mut guard = hashes_watcher.lock().unwrap();
                                        
                                        if guard.contains_key(&relative) {
                                            files_to_delete.push(relative.clone());
                                        }

                                        let dir_prefix = format!("{}/", relative);
                                        for key in guard.keys() {
                                            if key.starts_with(&dir_prefix) {
                                                files_to_delete.push(key.clone());
                                            }
                                        }
                                        
                                        for f in &files_to_delete {
                                            guard.remove(f);
                                        }
                                    }

                                    for f in files_to_delete {
                                        {
                                            let mut d_guard = deletes_watcher.lock().unwrap();
                                            if d_guard.contains(&f) {
                                                d_guard.remove(&f);
                                                continue; 
                                            }
                                        }

                                        let msg = Message::DeleteFile { path: f.clone() };
                                        tx_watcher.send(WsMessage::Text(serde_json::to_string(&msg).unwrap())).ok();
                                        println!("🗑️ Deleting: {}", f);
                                    }
                                }
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
                        Message::StartTransfer { path, size: _, target_version: _ } => {
                            if backend.is_read_only() {
                                println!("🔒 Ignoring update for Read-Only backend: {}", path);
                            } else {
                                transfer_state = TransferState::ExpectingBinary { path };
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
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            WsMessage::Binary(data) => {
                if let TransferState::ExpectingBinary { path } = transfer_state {
                    println!("📥 Downloading: {}", path);
                    let hash = calculate_hash(&data);
                    synced_hashes.lock().unwrap().insert(path.clone(), hash);
                    
                    let normalized_path = path.replace("/", std::path::MAIN_SEPARATOR_STR);
                    if let Err(e) = backend.write_file(&normalized_path, &data).await {
                        eprintln!("❌ Failed to write file {}: {}", path, e);
                    }
                    transfer_state = TransferState::Idle;
                }
            }
            _ => {}
        }
    }
    
    send_task.abort();
    println!("❌ Disconnected");
}