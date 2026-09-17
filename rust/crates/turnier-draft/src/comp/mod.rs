//! Standalone Comp-Finder: persisted rooms and exact preference-based matching.
//! This mode never creates a Steam lobby or changes a pick/ban draft.

mod repo;
pub mod solver;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub use repo::{create, get, join, leave, remove_member, save_preferences, Room};

pub const MAX_PLAYERS: usize = 6;
pub const MAX_HEROES: usize = 128;
pub const RESULT_LIMIT: usize = 10;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub hero_name: String,
    pub priority: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Member {
    pub id: String,
    pub name: String,
    pub preferences: Vec<Preference>,
    pub revision: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum CompError {
    #[error("Diese Lobby gibt es nicht oder sie ist abgelaufen.")]
    NotFound,
    #[error("Dein Spielerplatz ist nicht mehr gültig. Bitte erneut beitreten.")]
    Unauthorized,
    #[error("Die Lobby ist mit sechs Spielern voll.")]
    Full,
    #[error("Nur der Gastgeber kann andere Spieler entfernen.")]
    Forbidden,
    #[error("Deine Auswahl wurde inzwischen in einem anderen Tab geändert. Bitte neu laden.")]
    Stale,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub type CompResult<T> = Result<T, CompError>;

pub fn normalize_code(code: &str) -> CompResult<String> {
    let code = code.trim().to_ascii_uppercase();
    if code.len() != 8
        || !code
            .bytes()
            .all(|b| b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(&b))
    {
        return Err(CompError::NotFound);
    }
    Ok(code)
}

pub fn validate_name(name: &str) -> CompResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 32 || name.chars().any(char::is_control) {
        return Err(CompError::Invalid(
            "Dein Name muss 1 bis 32 Zeichen lang sein und darf keine Steuerzeichen enthalten."
                .into(),
        ));
    }
    Ok(name.to_owned())
}

/// A browser generates a fresh cryptographic capability BEFORE create/join.
/// Retrying a join with that same capability is idempotent, even in a full room.
pub fn token_hash(token: &str) -> CompResult<String> {
    if !(32..=128).contains(&token.len())
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(CompError::Unauthorized);
    }
    Ok(format!("{:x}", Sha256::digest(token.as_bytes())))
}

pub fn validate_preferences(
    preferences: &[Preference],
    allowed: &HashSet<String>,
) -> CompResult<()> {
    if preferences.len() > MAX_HEROES {
        return Err(CompError::Invalid("Zu viele Helden ausgewählt.".into()));
    }
    let mut seen = HashSet::new();
    for p in preferences {
        if p.priority > 2 || !allowed.contains(&p.hero_name) || !seen.insert(&p.hero_name) {
            return Err(CompError::Invalid("Ungültige Heldenauswahl: jeder verfügbare Held darf einmal mit 0, 1 oder 2 Punkten ausgewählt werden.".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_preserves_zero_and_rejects_duplicates_unknown_heroes_and_bad_priorities() {
        let allowed = HashSet::from(["A".into()]);
        let p = Preference {
            hero_name: "A".into(),
            priority: 0,
        };
        assert!(validate_preferences(std::slice::from_ref(&p), &allowed).is_ok());
        assert!(validate_preferences(&[p.clone(), p], &allowed).is_err());
        assert!(validate_preferences(
            &[Preference {
                hero_name: "A".into(),
                priority: 3
            }],
            &allowed
        )
        .is_err());
        assert!(validate_preferences(
            &[Preference {
                hero_name: "B".into(),
                priority: 0
            }],
            &allowed
        )
        .is_err());
        assert!(validate_name("  ").is_err());
        assert!(validate_name("a\nb").is_err());
        assert!(validate_name(&"ü".repeat(33)).is_err());
        assert_eq!(validate_name("  Spieler  ").unwrap(), "Spieler");
        assert_eq!(normalize_code(" abcd2345 ").unwrap(), "ABCD2345");
        assert!(normalize_code("ABCD0000").is_err());
        assert!(token_hash("public-player-id").is_err());
        let hash = token_hash(&"x".repeat(32)).unwrap();
        assert_eq!(hash.len(), 64);
        assert_ne!(hash, "x".repeat(32));
    }
}
