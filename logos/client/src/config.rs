use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub client_name: Option<String>,
    pub location: Option<String>,
    pub storage_id: Option<String>,
}

impl AppConfig {
    pub async fn load(path: &str) -> Self {
        if Path::new(path).exists() {
            match fs::read_to_string(path).await {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(cfg) => {
                        println!("📄 Loaded configuration from {}", path);
                        return cfg;
                    },
                    Err(e) => eprintln!("⚠️ Failed to parse config: {}", e),
                },
                Err(e) => eprintln!("⚠️ Failed to read config file: {}", e),
            }
        }
        Self::default()
    }

    pub async fn save(&self, path: &str) {
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = fs::write(path, json).await {
                    eprintln!("⚠️ Failed to save config to {}: {}", path, e);
                } else {
                    println!("💾 Configuration saved to {}", path);
                }
            },
            Err(e) => eprintln!("⚠️ Failed to serialize config: {}", e),
        }
    }
}