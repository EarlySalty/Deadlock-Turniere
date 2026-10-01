//! Resolver für echte Secrets aus der bestehenden Infisical-/Credentials-Kette.
//! Nicht geheime Betriebsschlüssel werden vor jedem dynamischen Dateizugriff abgewiesen.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};
use zeroize::Zeroizing;

static SNAPSHOT: OnceLock<BTreeMap<String, Zeroizing<String>>> = OnceLock::new();
static START: Mutex<()> = Mutex::new(());

/// Einmaliger privater Start-Snapshot neben der validierten normalen TOML.
pub fn load_snapshot(config_source: &std::path::Path) -> Result<(), &'static str> {
    let _guard = START
        .lock()
        .map_err(|_| "Private Infisical-Startgrenze ist nicht verfügbar.")?;
    if SNAPSHOT.get().is_some() {
        return Err("Private Infisical-Momentaufnahme wurde bereits geladen.");
    }
    let values = dl_token_secrets::private_values(&config_source.with_file_name("infisical.json"))
        .map_err(|_| "Private Infisical-Momentaufnahme konnte nicht geladen werden.")?;
    SNAPSHOT
        .set(values.into_iter().collect())
        .map_err(|_| "Private Infisical-Momentaufnahme wurde bereits geladen.")
}

/// Poolaufbau akzeptiert nur den bereits installierten privaten Snapshot.
pub fn snapshot_value(name: &str) -> Option<&'static str> {
    SNAPSHOT
        .get()?
        .get(name)
        .map(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
}

/// Liest eine UTF-8-Datei und trimmt Whitespace. Leerer Inhalt → `None`.
fn read_file(path: &std::path::Path) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(v) => {
            let trimmed = v.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(_) => None,
    }
}

fn credential_dirs() -> Vec<PathBuf> {
    [
        "CREDENTIALS_DIRECTORY",
        "SECRETS_DIRECTORY",
        "VAULT_SECRETS_DIR",
    ]
    .iter()
    .filter_map(|name| {
        let raw = env::var(name).ok()?;
        let raw = raw.trim();
        if raw.is_empty() {
            None
        } else {
            Some(PathBuf::from(raw))
        }
    })
    .collect()
}

fn name_candidates(name: &str) -> [String; 3] {
    [
        name.to_string(),
        name.to_lowercase(),
        name.to_lowercase().replace('_', "-"),
    ]
}

/// Versucht, `name` aus einer Datei aufzulösen (`NAME_FILE` oder Credentials-Dirs).
fn file_backed(name: &str) -> Option<String> {
    if let Ok(explicit) = env::var(format!("{name}_FILE")) {
        let explicit = explicit.trim();
        if !explicit.is_empty() {
            return read_file(std::path::Path::new(explicit));
        }
    }
    for dir in credential_dirs() {
        for candidate in name_candidates(name) {
            if let Some(value) = read_file(&dir.join(candidate)) {
                return Some(value);
            }
        }
    }
    None
}

/// Löst den ersten nicht-leeren Wert aus der Alias-Liste auf (Datei → Env je Name).
const ALLOWED_SECRET_NAMES: &[&str] = &[
    "DEADLOCK_CENTRAL_DSN",
    "TURNIER_INTERNAL_API_TOKEN",
    "MASTER_BROKER_TOKEN",
    "MAIN_BOT_INTERNAL_TOKEN",
    "TWITCH_INTERNAL_API_TOKEN",
    "DISCORD_MASTER_BROKER_TOKEN",
    "DISCORD_BOT_TOKEN",
    "DISCORD_TOKEN",
    "BOT_TOKEN",
    "JWT_SECRET",
    "DISCORD_WEBHOOK_URL",
    "STEAM_BOT_INTERNAL_TOKEN",
    "SCRIM_OBSERVER_AGENT_TOKEN",
    "OBSERVER_AGENT_TOKEN",
];

pub fn resolve_first(names: &[&str]) -> Option<String> {
    for name in names {
        if !ALLOWED_SECRET_NAMES.contains(name) {
            continue;
        }
        if let Some(snapshot) = SNAPSHOT.get() {
            if let Some(value) = snapshot.get(*name).filter(|value| !value.trim().is_empty()) {
                return Some(value.trim().to_owned());
            }
            continue;
        }
        if let Some(value) = file_backed(name) {
            return Some(value);
        }
        if let Ok(value) = env::var(name) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Wie [`resolve_first`] für einen einzelnen Namen, mit Default.
pub fn get_string(name: &str, default: &str) -> String {
    resolve_first(&[name]).unwrap_or_else(|| default.to_string())
}

/// Wie [`resolve_first`] über mehrere Aliase, mit Default.
pub fn get_first_string(names: &[&str], default: &str) -> String {
    resolve_first(names).unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        os::unix::process::CommandExt,
        process::{Command, Stdio},
    };

    #[test]
    fn real_fifo_snapshot_is_single_read_and_aliases_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("infisical.json"),
            br#"{"secret_values_fd":3,"project_id":"fixture","environment":"fixture","secret_path":"/","socket_path":"/nonexistent","database_secret":"DEADLOCK_CENTRAL_DSN"}"#).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "secrets::tests::child_fifo_contract",
            ])
            .current_dir(directory.path())
            .stdin(Stdio::piped());
        // SAFETY: Command owns the piped stdin FD0 until exec; dup2 is
        // async-signal-safe and creates the designated inherited private FD3.
        unsafe {
            command.pre_exec(|| {
                if libc::dup2(0, 3) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        child.stdin.take().unwrap().write_all(br#"{"DEADLOCK_CENTRAL_DSN":"synthetic-dsn","MASTER_BROKER_TOKEN":"synthetic-token"}"#).unwrap();
        assert!(child.wait().unwrap().success());
    }

    #[test]
    #[ignore = "isolierter Kindprozess mit echter privater FIFO"]
    fn child_fifo_contract() {
        let config = std::env::current_dir().unwrap().join("bot.toml");
        load_snapshot(&config).unwrap();
        assert_eq!(
            snapshot_value("DEADLOCK_CENTRAL_DSN"),
            Some("synthetic-dsn")
        );
        assert_eq!(
            resolve_first(&["TURNIER_INTERNAL_API_TOKEN", "MASTER_BROKER_TOKEN"]).as_deref(),
            Some("synthetic-token")
        );
        assert!(resolve_first(&["DISCORD_TOKEN"]).is_none());
        assert!(resolve_first(&["RUST_LOG"]).is_none());
        assert!(load_snapshot(&config).is_err());
    }
}
