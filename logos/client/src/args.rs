use clap::Parser;
use std::path::PathBuf;
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    pub locations: Vec<String>,

    #[arg(long, default_value = "default_secret")]
    pub secret: String,

    #[arg(short, long)]
    pub config: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Location {
    Folder(PathBuf),
    Ftp(Url),
    Zip(PathBuf),
}

impl Location {
    pub fn parse(input: &str) -> Result<Self, String> {
        if let Some((type_str, path_str)) = input.split_once(':') {
            match type_str {
                "folder" => Ok(Location::Folder(PathBuf::from(path_str))),
                "zip" => Ok(Location::Zip(PathBuf::from(path_str))),
                "ftp" => {
                    let url = Url::parse(input).map_err(|e| e.to_string())?;
                    Ok(Location::Ftp(url))
                },
                _ => Err(format!("Unknown location type: {}", type_str)),
            }
        } else {
            Err(format!("Invalid format. Expected type:path, got {}", input))
        }
    }
}