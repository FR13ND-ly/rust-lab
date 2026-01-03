mod args;
mod backend;
mod backends;
mod config;

use args::{Args, Location};
use backend::StorageBackend;
use backends::folder::FolderBackend;
use backends::ftp::FtpBackend;
use backends::sftp::SftpBackend;
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
use anyhow::{Result, Context, anyhow};

enum TransferState {
    Idle,
    ExpectingBinary { path: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    let config_path = args.config.clone().unwrap_or_else(|| "logos_config.json".to_string());
    let mut config = config::AppConfig::load(&config_path).await;

    let client_name = if let Some(name) = &config.client_name {
        name.clone()
    } else {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Client".to_string())
    };

    let loc_input = if !args.locations.is_empty() {
        args.locations.first().cloned()
    } else {
        config.location.clone()
    };

    let loc_raw = loc_input.ok_or_else(|| anyhow!("No location provided. Pass as arg or set in config."))?;
    let loc_clean: String = loc_raw.trim().chars().filter(|c| !c.is_control()).collect();

    let loc_str = if (loc_clean.starts_with("ftp://") || loc_clean.starts_with("sftp://")) && loc_clean.contains('#') {
        if let Some(idx) = loc_clean.rfind('@') {
            let (creds, rest) = loc_clean.split_at(idx);
            if creds.contains('#') {
                format!("{}{}", creds.replace('#', "%23"), rest)
            } else {
                loc_clean
            }
        } else {
            loc_clean
        }
    } else {
        loc_clean
    };

    let backend: Arc<Box<dyn StorageBackend>> = if loc_str.starts_with("sftp://") || loc_str.starts_with("ssh://") {
         println!("[*] Initializing SFTP backend...");
         match SftpBackend::new(&loc_str).await {
             Ok(sftp) => Arc::new(Box::new(sftp)),
             Err(e) => {
                 eprintln!("[!] SFTP Error: {}", e);
                 std::process::exit(1);
             }
         }
    } else if loc_str.starts_with("ftp://") {
        println!("[*] Initializing FTP backend...");
        match FtpBackend::new(&loc_str) {
            Ok(ftp) => Arc::new(Box::new(ftp)),
            Err(e) => {
                eprintln!("[!] FTP Error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        match Location::parse(&loc_str) {
            Ok(Location::Folder(path)) => Arc::new(Box::new(FolderBackend::new(path))),
            Ok(Location::Ftp(url)) => {
                match FtpBackend::new(url.as_str()) {
                    Ok(ftp) => Arc::new(Box::new(ftp)),
                    Err(e) => {
                        eprintln!("[!] FTP Init Error: {}", e);
                        std::process::exit(1);
                    },
                }
            },
            Ok(Location::Zip(path)) => {
                match ZipBackend::new(path) {
                    Ok(z) => Arc::new(Box::new(z)),
                    Err(e) => {
                         eprintln!("[!] ZIP Error: {}", e);
                         std::process::exit(1);
                    }
                }
            },
            Err(e) => {
                eprintln!("[!] Invalid location: {}", e);
                std::process::exit(1);
            },
        }
    };

    println!("[+] Client started: {}", client_name);

    let server_url = "ws://localhost:3000/ws/client";
    let (ws_stream, _) = connect_async(server_url).await.context("Failed to connect to server")?;
    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_write.send(msg).await.is_err() { break; }
        }
    });

    let mut initial_files = Vec::new();

    if let Some(target_id) = &config.storage_id {
        println!("[*] Auto-joining storage: {}", target_id);
        tx.send(WsMessage::Text(serde_json::to_string(&Message::JoinStorage { 
            storage_id: target_id.clone(),
            client_name: client_name.clone()
        })?)).map_err(|_| anyhow!("Channel closed"))?;
    } else {
        tx.send(WsMessage::Text(serde_json::to_string(&Message::RequestStorageList)?)).map_err(|_| anyhow!("Channel closed"))?;
    }

    while let Some(Ok(msg)) = ws_read.next().await {
        if let WsMessage::Text(text) = msg && let Ok(parsed) = serde_json::from_str::<Message>(&text) {
            match parsed {
                Message::StorageList { storages } => {
                    println!("\nAvailable Storages:");
                    let mut options: Vec<String> = storages.iter()
                        .map(|s| format!("{} [{}]", s.name, s.id))
                        .collect();
                    options.push("Create New".to_string());
                    options.push("Refresh".to_string());

                    let selection = Select::with_theme(&ColorfulTheme::default())
                        .with_prompt("Select Action")
                        .default(0)
                        .items(&options)
                        .interact()?;

                    if selection < storages.len() {
                        let selected = &storages[selection];
                        tx.send(WsMessage::Text(serde_json::to_string(&Message::JoinStorage { 
                            storage_id: selected.id.clone(),
                            client_name: client_name.clone()
                        })?)).map_err(|_| anyhow!("Channel closed"))?;
                    } else if selection == storages.len() {
                        let name: String = Input::with_theme(&ColorfulTheme::default())
                            .with_prompt("Storage Name")
                            .interact_text()?;
                        tx.send(WsMessage::Text(serde_json::to_string(&Message::CreateStorage { name: name.trim().to_string() })?)).map_err(|_| anyhow!("Channel closed"))?;
                    } else {
                        tx.send(WsMessage::Text(serde_json::to_string(&Message::RequestStorageList)?)).map_err(|_| anyhow!("Channel closed"))?;
                    }
                },
                Message::Welcome { storage_id: sid, files } => {
                    println!("[+] Joined storage successfully");
                    initial_files = files;
                    config.client_name = Some(client_name.clone());
                    config.location = Some(loc_raw.clone());
                    config.storage_id = Some(sid);
                    config.save(&config_path).await;
                    break;
                },
                Message::Error { message } => {
                    eprintln!("[!] Server Error: {}", message);
                    if config.storage_id.is_some() {
                        config.storage_id = None;
                    }
                    tx.send(WsMessage::Text(serde_json::to_string(&Message::RequestStorageList)?)).map_err(|_| anyhow!("Channel closed"))?;
                }
                _ => {}
            }
        }
    }

    println!("[*] Starting synchronization...");
    let synced_hashes = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let pending_deletes = Arc::new(Mutex::new(HashSet::<String>::new()));
    let mut transfer_state = TransferState::Idle;

    if let Ok(local_files) = backend.list_files().await {
        println!("[*] Found {} local files", local_files.len());

        for local in &local_files {
            let remote = initial_files.iter().find(|f| f.path == local.path);
            let needs_upload = match remote {
                None => true, 
                Some(r) => local.modified > r.modified
            };

            if needs_upload {
                if let Ok(content) = backend.read_file(&local.path).await {
                     let hash = calculate_hash(&content);
                     if let Ok(mut guard) = synced_hashes.lock() {
                        guard.insert(local.path.clone(), hash);
                     }

                     let header = Message::StartTransfer { 
                        path: local.path.clone(), 
                        size: content.len() as u64, 
                        target_version: 0 
                    };
                    tx.send(WsMessage::Text(serde_json::to_string(&header)?)).map_err(|_| anyhow!("Channel closed"))?;
                    tx.send(WsMessage::Binary(content)).map_err(|_| anyhow!("Channel closed"))?;
                    println!("[^] Uploading: {}", local.path);
                }
            } else if let Some(r) = remote {
                let will_download = !r.is_deleted && r.modified > local.modified;
                if !will_download && !r.is_deleted
                    && let Ok(content) = backend.read_file(&local.path).await {
                        let hash = calculate_hash(&content);
                        if let Ok(mut guard) = synced_hashes.lock() {
                             guard.insert(local.path.clone(), hash);
                        }
                    }
            }
        }

        if !backend.is_read_only() {
            for remote in &initial_files {
                if remote.is_deleted { continue; }
                let local = local_files.iter().find(|f| f.path == remote.path);
                if local.is_none() || remote.modified > local.unwrap().modified {
                    println!("[v] Requesting download: {}", remote.path);
                    let msg = Message::RequestFile { path: remote.path.clone() };
                    tx.send(WsMessage::Text(serde_json::to_string(&msg)?)).map_err(|_| anyhow!("Channel closed"))?;
                }
            }
        }
    }

    let mut _watcher: Option<RecommendedWatcher> = None;

    if !backend.is_read_only() {
        if let Ok(Location::Folder(raw_path)) = Location::parse(&loc_str) {
            let tx_w = tx.clone();
            let backend_w = backend.clone();
            let hashes_w = synced_hashes.clone();
            let deletes_w = pending_deletes.clone();
            
            let abs_root = std::fs::canonicalize(&raw_path).unwrap_or(raw_path);
            let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();
            
            let mut watcher = RecommendedWatcher::new(move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res { notify_tx.send(event).ok(); }
            }, notify::Config::default()).context("Failed to create watcher")?;
            
            if let Err(e) = watcher.watch(&abs_root, RecursiveMode::Recursive) {
                eprintln!("[!] Watcher error: {}", e);
            }
            _watcher = Some(watcher);
            
            tokio::spawn(async move {
                let to_relative = |sys_path: &Path| -> Option<String> {
                     sys_path.strip_prefix(&abs_root).ok()
                        .map(|p| p.to_string_lossy().replace("\\", "/"))
                        .filter(|s| !s.is_empty())
                };

                while let Some(event) = notify_rx.recv().await {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            for path in event.paths {
                                if let Some(rel) = to_relative(&path) {
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                    
                                    if let Ok(content) = backend_w.read_file(&rel).await {
                                        let hash = calculate_hash(&content);
                                        
                                        let should_upload = if let Ok(mut guard) = hashes_w.lock() {
                                            if let Some(h) = guard.get(&rel) {
                                                if h == &hash { 
                                                    false 
                                                } else {
                                                    guard.insert(rel.clone(), hash);
                                                    true
                                                }
                                            } else {
                                                guard.insert(rel.clone(), hash);
                                                true
                                            }
                                        } else {
                                            eprintln!("[!] Mutex poisoned");
                                            false
                                        };

                                        if !should_upload { continue; }

                                        let header = Message::StartTransfer { 
                                            path: rel.clone(), 
                                            size: content.len() as u64, 
                                            target_version: 0 
                                        };
                                        if let Ok(json) = serde_json::to_string(&header) {
                                            let _ = tx_w.send(WsMessage::Text(json));
                                            let _ = tx_w.send(WsMessage::Binary(content));
                                            println!("[^] Uploading: {}", rel);
                                        }
                                    }
                                }
                            }
                        }
                        EventKind::Remove(_) => {
                             for path in event.paths {
                                if let Some(rel) = to_relative(&path) {
                                    if let Ok(mut d_guard) = deletes_w.lock() && d_guard.contains(&rel) { 
                                        d_guard.remove(&rel); 
                                        continue; 
                                    }
                                    if let Ok(mut guard) = hashes_w.lock() {
                                        guard.remove(&rel);
                                    }
                                    let msg = Message::DeleteFile { path: rel.clone() };
                                    if let Ok(json) = serde_json::to_string(&msg) {
                                        let _ = tx_w.send(WsMessage::Text(json));
                                        println!("[x] Deleting: {}", rel);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            });
        } else {
             println!("[*] Starting remote polling (10s interval)");
             let tx_poll = tx.clone();
             let backend_poll = backend.clone();
             let hashes_poll = synced_hashes.clone();
             
             tokio::spawn(async move {
                 loop {
                     tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                     if let Ok(files) = backend_poll.list_files().await {
                         for file in files {
                             let should_upload = {
                                 if let Ok(guard) = hashes_poll.lock() {
                                     !guard.contains_key(&file.path)
                                 } else {
                                     false
                                 }
                             };

                             if should_upload && let Ok(content) = backend_poll.read_file(&file.path).await {
                                 let hash = calculate_hash(&content);
                                 if let Ok(mut guard) = hashes_poll.lock() {
                                     if let Some(h) = guard.get(&file.path) && h == &hash { continue; }
                                     guard.insert(file.path.clone(), hash);
                                 }
                                 
                                 let header = Message::StartTransfer { 
                                    path: file.path.clone(), 
                                    size: content.len() as u64, 
                                    target_version: 0 
                                 };
                                 if let Ok(json) = serde_json::to_string(&header) && tx_poll.send(WsMessage::Text(json)).is_ok() {
                                     let _ = tx_poll.send(WsMessage::Binary(content));
                                     println!("[^] Uploading (Poll): {}", file.path);
                                 }
                             }
                         }
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
                        Message::StartTransfer { path, .. } => {
                            if backend.is_read_only() {
                                println!("[!] Skipped update for read-only backend: {}", path);
                            } else {
                                transfer_state = TransferState::ExpectingBinary { path };
                            }
                        }
                        Message::DeleteFile { path } => {
                            if !backend.is_read_only() {
                                println!("[x] Remote delete: {}", path);
                                if let Ok(mut guard) = pending_deletes.lock() {
                                    guard.insert(path.clone());
                                }
                                let _ = backend.delete_file(&path).await;
                                if let Ok(mut guard) = synced_hashes.lock() {
                                    guard.remove(&path);
                                }
                            }
                        }
                        Message::ConflictDetected { path, server_version } => {
                            println!("[!] Conflict detected: {} (v{}). Saving local copy.", path, server_version);
                            let p_obj = PathBuf::from(&path);
                            let stem = p_obj.file_stem().unwrap_or_default().to_string_lossy();
                            let ext = p_obj.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                            let conflict_path = format!("{}_conflict_{}{}", stem, ts, ext);
                            
                            if let Ok(mut guard) = pending_deletes.lock() {
                                guard.insert(path.clone());
                            }
                            if let Ok(content) = backend.read_file(&path).await 
                                && backend.write_file(&conflict_path, &content).await.is_ok() {
                                    println!("[*] Saved conflict to {}", conflict_path);
                                    if backend.delete_file(&path).await.is_ok() {
                                        let _ = tx.send(WsMessage::Text(serde_json::to_string(&Message::RequestFile { path: path.clone() })?));
                                    }
                            }
                        }
                        _ => {}
                    }
                }
            }
            WsMessage::Binary(data) => {
                if let TransferState::ExpectingBinary { path } = transfer_state {
                    println!("[v] Downloading: {}", path);
                    let hash = calculate_hash(&data);
                    if let Ok(mut guard) = synced_hashes.lock() {
                        guard.insert(path.clone(), hash);
                    }
                    
                    if let Err(e) = backend.write_file(&path, &data).await {
                        eprintln!("[!] Write error for {}: {}", path, e);
                    }
                    transfer_state = TransferState::Idle;
                }
            }
            _ => {}
        }
    }
    
    send_task.abort();
    println!("[!] Disconnected from server.");
    Ok(())
}