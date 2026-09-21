use crate::models::{Email, GoogleAccount, TempEmail};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct StoredData {
    pub temp_email: Vec<TempEmail>,
    pub google_accounts: Vec<GoogleAccount>,
    pub emails: HashMap<String, Vec<Email>>,
}

// Windows: %APPDATA%/mailbox/state.json.
// Linux/macOS: there's no APPDATA, so this falls back to ./mailbox/state.json
// in whatever folder the app was started from. That folder is in .gitignore
// because it contains account tokens and passwords, never commit it.
fn storage_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    base.join("mailbox").join("state.json")
}

pub fn load() -> StoredData {
    let path = storage_path();
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(_) => return StoredData::default(),
    };

    serde_json::from_str(&contents).unwrap_or_else(|error| {
        eprintln!("Failed to read saved mailbox state: {error}");
        StoredData::default()
    })
}

pub fn save(data: &StoredData) {
    let path = storage_path();
    let Some(parent) = path.parent() else {
        return;
    };

    if let Err(error) = fs::create_dir_all(parent) {
        eprintln!("Failed to create mailbox storage directory: {error}");
        return;
    }

    match serde_json::to_string_pretty(data) {
        Ok(contents) => {
            if let Err(error) = fs::write(path, contents) {
                eprintln!("Failed to save mailbox state: {error}");
            }
        }
        Err(error) => eprintln!("Failed to serialize mailbox state: {error}"),
    }
}

pub fn clear() {
    let path = storage_path();
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!("Failed to delete mailbox state: {error}");
        }
    }
}
