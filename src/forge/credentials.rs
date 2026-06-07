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

/// Store a token to both the local file and (best-effort) the OS keyring.
/// The file is the primary store so that subsequent reads never trigger a
/// keychain prompt.
pub(crate) fn store_token(service: &str, token: &str) -> Result<(), String> {
    // Always persist to file so reads never need the keyring.
    store_token_to_file(service, token)?;

    // Best-effort write to keyring for other tools (e.g. gh CLI) to find.
    if let Ok(entry) = keyring_entry(service) {
        let _ = entry.set_password(token);
    }

    Ok(())
}

/// Load a token. Reads from the local file first so the OS keychain is
/// never accessed during normal startup. Falls back to the keyring only
/// when the file is missing (e.g. first run after migrating from an older
/// version that only stored in keyring). If the keyring has the token,
/// it is copied to the file so the keyring is not needed on the next launch.
pub(crate) fn load_token(service: &str) -> Option<String> {
    // Try file first — no keychain prompt.
    if let Some(token) = load_token_from_file(service) {
        return Some(token);
    }

    // Fall back to keyring (may trigger a macOS Keychain prompt).
    if let Ok(entry) = keyring_entry(service)
        && let Ok(token) = entry.get_password()
    {
        // Cache to file so the next launch skips the keyring.
        let _ = store_token_to_file(service, &token);
        return Some(token);
    }

    None
}

fn load_token_from_file(service: &str) -> Option<String> {
    let path = file_path(service);
    let raw = fs::read_to_string(&path).ok()?;
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&raw).ok()?;
    map.get("token")?.as_str().map(|s| s.to_string())
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
