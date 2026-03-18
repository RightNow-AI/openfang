//! Minimal `.env` file loader/saver for `~/.openfang/.env`.
//!
//! No external crate needed — hand-rolled for simplicity.
//! Format: `KEY=VALUE` lines, `#` comments, optional quotes.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

/// Get the OpenFang home directory, respecting OPENFANG_HOME env var.
fn dotenv_openfang_home() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("OPENFANG_HOME") {
        return Some(PathBuf::from(home));
    }
    dirs::home_dir().map(|h| h.join(".openfang"))
}

/// Return the path to `~/.openfang/.env`.
pub fn env_file_path() -> Option<PathBuf> {
    dotenv_openfang_home().map(|h| h.join(".env"))
}

/// Load `~/.openfang/.env` and `~/.openfang/secrets.env` into `std::env`.
///
/// System env vars take priority — existing vars are NOT overridden.
/// `secrets.env` is loaded second so dashboard/API-managed secrets override
/// stale `.env` values while still yielding to system env vars that were
/// already present before startup.
/// Silently does nothing if the files don't exist.
pub fn load_dotenv() {
    let protected_keys: HashSet<String> = std::env::vars().map(|(key, _)| key).collect();
    load_env_file(env_file_path(), &protected_keys);
    // Also load secrets.env (written by dashboard "Set API Key" button)
    load_env_file(secrets_env_path(), &protected_keys);
}

/// Return the path to `~/.openfang/secrets.env`.
pub fn secrets_env_path() -> Option<PathBuf> {
    dotenv_openfang_home().map(|h| h.join("secrets.env"))
}

fn load_env_file(path: Option<PathBuf>, protected_keys: &HashSet<String>) {
    let path = match path {
        Some(p) => p,
        None => return,
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = parse_env_line(trimmed) {
            if !protected_keys.contains(&key) {
                std::env::set_var(&key, &value);
            }
        }
    }
}

/// Upsert a key in `~/.openfang/.env`.
///
/// Creates the file if missing. Sets 0600 permissions on Unix.
/// Also sets the key in the current process environment.
pub fn save_env_key(key: &str, value: &str) -> Result<(), String> {
    let path = env_file_path().ok_or("Could not determine home directory")?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {e}"))?;
    }

    let mut entries = read_env_file(&path);
    entries.insert(key.to_string(), value.to_string());
    write_env_file(&path, &entries)?;

    // Also set in current process
    std::env::set_var(key, value);

    Ok(())
}

/// Remove a key from `~/.openfang/.env`.
///
/// Also removes it from the current process environment.
pub fn remove_env_key(key: &str) -> Result<(), String> {
    let path = env_file_path().ok_or("Could not determine home directory")?;

    let mut entries = read_env_file(&path);
    entries.remove(key);
    write_env_file(&path, &entries)?;

    std::env::remove_var(key);

    Ok(())
}

/// List key names (without values) from `~/.openfang/.env`.
#[allow(dead_code)]
pub fn list_env_keys() -> Vec<String> {
    let path = match env_file_path() {
        Some(p) => p,
        None => return Vec::new(),
    };

    read_env_file(&path).into_keys().collect()
}

/// Check if the `.env` file exists.
#[allow(dead_code)]
pub fn env_file_exists() -> bool {
    env_file_path().map(|p| p.exists()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse a single `KEY=VALUE` line. Handles optional quotes.
fn parse_env_line(line: &str) -> Option<(String, String)> {
    let eq_pos = line.find('=')?;
    let key = line[..eq_pos].trim().to_string();
    let mut value = line[eq_pos + 1..].trim().to_string();

    if key.is_empty() {
        return None;
    }

    // Strip matching quotes
    if ((value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\'')))
        && value.len() >= 2
    {
        value = value[1..value.len() - 1].to_string();
    }

    Some((key, value))
}

/// Read all key-value pairs from the .env file.
fn read_env_file(path: &PathBuf) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return map,
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = parse_env_line(trimmed) {
            map.insert(key, value);
        }
    }

    map
}

/// Write key-value pairs back to the .env file with a header comment.
fn write_env_file(path: &PathBuf, entries: &BTreeMap<String, String>) -> Result<(), String> {
    let mut content =
        String::from("# OpenFang environment — managed by `openfang config set-key`\n");
    content.push_str("# Do not edit while the daemon is running.\n\n");

    for (key, value) in entries {
        // Quote values that contain spaces or special characters
        if value.contains(' ') || value.contains('#') || value.contains('"') {
            content.push_str(&format!("{key}=\"{}\"\n", value.replace('"', "\\\"")));
        } else {
            content.push_str(&format!("{key}={value}\n"));
        }
    }

    std::fs::write(path, &content).map_err(|e| format!("Failed to write .env file: {e}"))?;

    // Set 0600 permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    struct EnvVarGuard {
        key: String,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self {
                key: key.to_string(),
                previous,
            }
        }

        fn remove(key: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(ref value) = self.previous {
                std::env::set_var(&self.key, value);
            } else {
                std::env::remove_var(&self.key);
            }
        }
    }

    #[test]
    fn test_parse_env_line_simple() {
        let (k, v) = parse_env_line("FOO=bar").unwrap();
        assert_eq!(k, "FOO");
        assert_eq!(v, "bar");
    }

    #[test]
    fn test_parse_env_line_quoted() {
        let (k, v) = parse_env_line("KEY=\"hello world\"").unwrap();
        assert_eq!(k, "KEY");
        assert_eq!(v, "hello world");
    }

    #[test]
    fn test_parse_env_line_single_quoted() {
        let (k, v) = parse_env_line("KEY='value'").unwrap();
        assert_eq!(k, "KEY");
        assert_eq!(v, "value");
    }

    #[test]
    fn test_parse_env_line_spaces() {
        let (k, v) = parse_env_line("  KEY  =  value  ").unwrap();
        assert_eq!(k, "KEY");
        assert_eq!(v, "value");
    }

    #[test]
    fn test_parse_env_line_no_value() {
        let (k, v) = parse_env_line("KEY=").unwrap();
        assert_eq!(k, "KEY");
        assert_eq!(v, "");
    }

    #[test]
    fn test_parse_env_line_comment() {
        assert!(
            parse_env_line("# comment").is_none()
                || parse_env_line("# comment").unwrap().0.starts_with('#')
        );
        // Comments are filtered before reaching parse_env_line in production code
    }

    #[test]
    fn test_parse_env_line_no_equals() {
        assert!(parse_env_line("NOEQUALS").is_none());
    }

    #[test]
    fn test_parse_env_line_empty_key() {
        assert!(parse_env_line("=value").is_none());
    }

    #[test]
    fn test_load_dotenv_prefers_secrets_env_over_dotenv_file() {
        let _env_lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "OPENFANG_API_KEY=from-dotenv\n").unwrap();
        std::fs::write(
            dir.path().join("secrets.env"),
            "OPENFANG_API_KEY=from-secrets\n",
        )
        .unwrap();

        let _home_guard = EnvVarGuard::set("OPENFANG_HOME", dir.path().to_str().unwrap());
        let _key_guard = EnvVarGuard::remove("OPENFANG_API_KEY");

        load_dotenv();

        assert_eq!(std::env::var("OPENFANG_API_KEY").unwrap(), "from-secrets");
    }

    #[test]
    fn test_load_dotenv_preserves_preexisting_system_env() {
        let _env_lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "OPENFANG_API_KEY=from-dotenv\n").unwrap();
        std::fs::write(
            dir.path().join("secrets.env"),
            "OPENFANG_API_KEY=from-secrets\n",
        )
        .unwrap();

        let _home_guard = EnvVarGuard::set("OPENFANG_HOME", dir.path().to_str().unwrap());
        let _key_guard = EnvVarGuard::set("OPENFANG_API_KEY", "from-system-env");

        load_dotenv();

        assert_eq!(
            std::env::var("OPENFANG_API_KEY").unwrap(),
            "from-system-env"
        );
    }
}
