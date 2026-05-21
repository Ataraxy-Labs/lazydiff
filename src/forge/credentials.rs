use std::{env, fs, path::PathBuf};

const KEYRING_USERNAME: &str = "default";

fn keyring_entry(service: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(service, KEYRING_USERNAME)
}

pub(crate) fn data_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lazydiff")
}

fn file_path(service: &str) -> PathBuf {
    let forge_name = service.strip_prefix("lazydiff-").unwrap_or(service);
    data_dir().join(format!("{forge_name}-auth.json"))
}

/// Store a token in the OS keyring, falling back to a JSON file if the
/// keyring is unavailable.
pub(crate) fn store_token(service: &str, token: &str) -> Result<(), String> {
    if let Ok(entry) = keyring_entry(service) {
        if entry.set_password(token).is_ok() {
            return Ok(());
        }
    }
    store_token_to_file(service, token)
}

/// Load a token, trying the OS keyring first and then the JSON file.
/// If the token is found in the file but not in the keyring, it is
/// migrated into the keyring and stripped from the file.
pub(crate) fn load_token(service: &str) -> Option<String> {
    // Try keyring first.
    if let Ok(entry) = keyring_entry(service) {
        if let Ok(token) = entry.get_password() {
            return Some(token);
        }
    }

    // Fall back to file.
    let path = file_path(service);
    let raw = fs::read_to_string(&path).ok()?;
    let mut map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&raw).ok()?;
    let token = map.get("token")?.as_str()?.to_string();

    // Attempt to migrate the token into the keyring.
    if let Ok(entry) = keyring_entry(service) {
        if entry.set_password(&token).is_ok() {
            map.remove("token");
            if map.is_empty() {
                let _ = fs::remove_file(&path);
            } else if let Ok(json) = serde_json::to_string_pretty(&map) {
                let _ = fs::write(&path, json);
            }
        }
    }

    Some(token)
}

/// Delete a token from both the OS keyring and the JSON file.
/// Returns `true` if something was actually removed.
pub(crate) fn delete_token(service: &str) -> Result<bool, String> {
    let mut removed = false;

    if let Ok(entry) = keyring_entry(service) {
        match entry.delete_credential() {
            Ok(()) => removed = true,
            Err(keyring::Error::NoEntry) => {}
            Err(_) => {}
        }
    }

    match fs::remove_file(file_path(service)) {
        Ok(()) => removed = true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }

    Ok(removed)
}

fn store_token_to_file(service: &str, token: &str) -> Result<(), String> {
    let path = file_path(service);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::json!({ "token": token });
    fs::write(
        path,
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
