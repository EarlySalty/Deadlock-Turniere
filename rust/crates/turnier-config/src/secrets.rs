//! Mehrschichtiger Resolver für Settings/Secrets.
//!
//! Priorität (wie im Python-Original `config.py`):
//! 1. `NAME_FILE` → Inhalt der referenzierten Datei
//! 2. Secret-Dateien in `CREDENTIALS_DIRECTORY`/`SECRETS_DIRECTORY`/`VAULT_SECRETS_DIR`
//!    (Dateiname `NAME`, `name`, oder `name-mit-bindestrich`)
//! 3. Umgebungsvariable `NAME`
//! 4. Default
//!
//! Keyring (Windows/Dev-Fallback im Original) ist auf Headless-Linux bewusst
//! nicht implementiert — dort liefern Datei/Env alle Werte.

use std::env;
use std::fs;
use std::path::PathBuf;

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
pub fn resolve_first(names: &[&str]) -> Option<String> {
    for name in names {
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

/// Integer-Setting mit Default; ungültige Werte fallen (mit Warnung) auf Default.
pub fn get_int(name: &str, default: i64) -> i64 {
    match resolve_first(&[name]) {
        Some(raw) => raw.parse::<i64>().unwrap_or_else(|_| {
            tracing::warn!("Ungültiger Integer für {name}={raw:?}, nutze Default {default}");
            default
        }),
        None => default,
    }
}

/// Boolean-Setting mit Default. Akzeptiert `1/true/yes/on` bzw. `0/false/no/off`.
pub fn get_bool(name: &str, default: bool) -> bool {
    match resolve_first(&[name]) {
        Some(raw) => match raw.trim().to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            other => {
                tracing::warn!("Ungültiger Boolean für {name}={other:?}, nutze Default {default}");
                default
            }
        },
        None => default,
    }
}
