//! Opake Server-Sessions: Token-Erzeugung, Persistenz und Auflösung.
//!
//! Sessions sind KEINE JWTs (trotz `JWT_SECRET` in der Config) — sie sind reine
//! Zufalls-Tokens in der DB-Tabelle `sessions`. Diese Crate erzeugt das Token
//! wie das Python-Original (`secrets.token_urlsafe(48)` = 48 Zufalls-Bytes,
//! base64url ohne Padding), persistiert die Zeile mit 7-Tage-Ablauf und löst sie
//! beim Request wieder auf.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use tb_core::UserSession;
use tb_db::Pool;

use crate::error::{AuthError, AuthResult};
use crate::roles::RoleSets;

/// Lebensdauer einer frisch erzeugten Session (7 Tage, wie im Original).
pub const SESSION_LIFETIME_DAYS: i64 = 7;

/// Anzahl Zufalls-Bytes hinter einem Token (`secrets.token_urlsafe(48)`).
const TOKEN_BYTES: usize = 48;

/// Eine roh aus der DB gelesene Session-Zeile.
///
/// Die `is_admin`/`is_mod`-Flags werden NICHT gespeichert, sondern beim Auflösen
/// frisch aus `discord_roles` gegen die Config-Rollen berechnet — exakt wie im
/// Original.
#[derive(Debug, Clone, sqlx::FromRow)]
struct SessionRow {
    discord_id: String,
    discord_name: Option<String>,
    discord_avatar: Option<String>,
    discord_roles: Option<String>,
    expires_at: String,
}

/// Erzeugt ein neues opakes Session-Token (base64url ohne Padding).
///
/// Entspricht `secrets.token_urlsafe(48)`: 48 kryptografisch zufällige Bytes,
/// URL-safe Base64-kodiert, kein Padding.
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Parst die kommaseparierte Rollen-Spalte in eine getrimmte, leere-Werte-freie
/// Liste (wie `[r.strip() for r in s.split(",") if r.strip()]`).
fn parse_roles(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or("")
        .split(',')
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .collect()
}

/// Legt eine neue Session an (Login-Completion). Gibt das erzeugte Token zurück.
///
/// `roles` wird kommasepariert gespeichert (un-normalisiert wie im Original —
/// Map-Befund `needs-decision`: Rollen werden als CSV-String eingefroren). Der
/// Ablauf wird als RFC3339-UTC geschrieben.
pub async fn create_session(
    pool: &Pool,
    discord_id: &str,
    discord_name: &str,
    discord_avatar: &str,
    roles: &[String],
) -> AuthResult<String> {
    let token = generate_token();
    let expires_at = Utc::now() + Duration::days(SESSION_LIFETIME_DAYS);
    let roles_csv = roles.join(",");

    sqlx::query(
        "INSERT INTO sessions \
         (token, discord_id, discord_name, discord_avatar, discord_roles, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&token)
    .bind(discord_id)
    .bind(discord_name)
    .bind(discord_avatar)
    .bind(roles_csv)
    .bind(expires_at.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(token)
}

/// Löst ein Token zur aufgelösten [`UserSession`] auf.
///
/// Ablauf wie `get_current_user` im Original: SELECT der Zeile, RFC3339-Parse von
/// `expires_at`, Ablaufprüfung, Rollen-Split, Flag-Berechnung gegen [`RoleSets`].
///
/// Abweichung (Map-Befund „safe", behebt Zombie-Zeilen): Eine als abgelaufen
/// erkannte Session wird zusätzlich aus der DB gelöscht (Opportunistic-Cleanup),
/// bevor der 401 entsteht. Verhalten gegenüber dem Aufrufer bleibt identisch
/// (abgelaufen ⇒ nicht authentifiziert).
///
/// Gibt `Err(AuthError::Unauthorized)` bei fehlender/abgelaufener Session zurück.
pub async fn resolve_session(
    pool: &Pool,
    token: &str,
    role_sets: &RoleSets,
) -> AuthResult<UserSession> {
    let row: Option<SessionRow> = sqlx::query_as(
        "SELECT discord_id, discord_name, discord_avatar, discord_roles, expires_at \
         FROM sessions WHERE token = ?",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Err(AuthError::Unauthorized("Session ungültig oder abgelaufen"));
    };

    let expires_at = parse_expires_at(&row.expires_at)?;
    if Utc::now() > expires_at {
        // Opportunistisch aufräumen — der Original-Code ließ die Zeile stehen.
        let _ = delete_session(pool, token).await;
        return Err(AuthError::Unauthorized("Session abgelaufen"));
    }

    let roles = parse_roles(row.discord_roles.as_deref());
    let flags = role_sets.flags(&roles);

    Ok(UserSession {
        discord_id: row.discord_id,
        discord_name: row.discord_name,
        discord_avatar: row.discord_avatar,
        roles,
        is_admin: flags.is_admin,
        is_mod: flags.is_mod,
    })
}

/// Parst `expires_at` als RFC3339-UTC.
///
/// Das eigene Schema schreibt ausschließlich tz-aware ISO (`to_rfc3339()`), daher
/// gibt es — anders als der defensive naive-Fallback im Original — hier keinen
/// Pfad für tz-naive Werte. Ein unparsbarer Wert ist ein echter Datenfehler und
/// wird als ungültige Session (401) behandelt, nicht stillschweigend als UTC
/// angenommen.
fn parse_expires_at(raw: &str) -> AuthResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| AuthError::Unauthorized("Session ungültig oder abgelaufen"))
}

/// Löscht die Session-Zeile zu einem Token (Logout / Opportunistic-Cleanup).
pub async fn delete_session(pool: &Pool, token: &str) -> AuthResult<()> {
    sqlx::query("DELETE FROM sessions WHERE token = ?")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

/// Löscht alle abgelaufenen Sessions. Für einen periodischen Cleanup-Task.
///
/// Behebt den Map-Befund „safe" (Tabelle wächst sonst monoton, kein Cleanup im
/// Original). Vergleich als String-Vergleich auf RFC3339 ist nur korrekt, wenn
/// alle Zeilen denselben Offset (`+00:00`) tragen — was `create_session`
/// garantiert. Deshalb vergleichen wir gegen `Utc::now().to_rfc3339()`.
pub async fn cleanup_expired(pool: &Pool) -> AuthResult<u64> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < ?")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_ist_url_safe_und_lang_genug() {
        let token = generate_token();
        // 48 Bytes base64url-ohne-Padding = 64 Zeichen.
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn token_ist_zufaellig() {
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn rollen_parsen_trimmt_und_filtert_leer() {
        assert_eq!(parse_roles(Some(" a, b ,, c ")), vec!["a", "b", "c"]);
        assert!(parse_roles(Some("")).is_empty());
        assert!(parse_roles(None).is_empty());
    }

    #[test]
    fn expires_parse_rfc3339() {
        let dt = parse_expires_at("2030-01-01T00:00:00+00:00").unwrap();
        assert_eq!(dt.to_rfc3339(), "2030-01-01T00:00:00+00:00");
    }

    #[test]
    fn expires_parse_ungueltig_ist_401() {
        let err = parse_expires_at("nicht-iso").unwrap_err();
        assert_eq!(err.status_code(), 401);
    }
}
