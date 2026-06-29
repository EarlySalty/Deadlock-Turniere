//! Interner OAuth-Client: delegierter Discord-Login über den Master-Broker.
//!
//! Der eigentliche OAuth-Tanz (Authorize-URL, State, Code→Token, Userinfo +
//! Guild-Rollen) läuft zentral beim Deadlock-Bots-Service. Dieses Backend ruft
//! nur dessen interne API auf:
//!   - `POST /internal/v1/discord/initiate`        → liefert `authorize_url`
//!   - `POST /internal/v1/discord/consume-result`  → liefert die Discord-Identität
//!
//! Auth via Header `X-Internal-Token`. Ein EINZELNER, geteilter `reqwest::Client`
//! (Keep-Alive, Connection-Pooling) ersetzt den Per-Call-Client des Originals;
//! Redirects werden bewusst NICHT gefolgt (`redirect::Policy::none()`), Timeouts:
//! 20 s gesamt, 5 s Connect.
//!
//! **Sicherheits-Invariante (extern):** Der CSRF-/Replay-Schutz hängt vollständig
//! daran, dass der Broker `state_id` bei `consume-result` genau einmal einlöst
//! (Single-Use + ablaufend). Diese Crate erzwingt das nicht selbst — sie ist auf
//! das Broker-Verhalten angewiesen (Map-Befund `needs-decision`).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use turnier_config::Config;

use crate::error::{AuthError, AuthResult};

const INTERNAL_TOKEN_HEADER: &str = "X-Internal-Token";
const INITIATE_PATH: &str = "/internal/v1/discord/initiate";
const CONSUME_RESULT_PATH: &str = "/internal/v1/discord/consume-result";

const OAUTH_SCOPE: &str = "identify guilds.members.read";
const REQUESTING_SERVICE: &str = "turnier";

/// Client für den delegierten Discord-OAuth-Flow über den Master-Broker.
#[derive(Debug, Clone)]
pub struct OAuthClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
    /// `<TURNIER_PUBLIC_URL>` — Basis für die Complete-Redirect-URL.
    public_url: String,
    /// `<DISCORD_GUILD_ID>` — geht als Metadata an `initiate`.
    guild_id: String,
}

/// Metadata-Block der `initiate`-Anfrage (verschachtelt — daher eigenes Struct
/// statt der `dict[str, str]`-Typ-Lüge des Originals).
#[derive(Debug, Serialize)]
struct InitiateMetadata {
    guild_id: String,
}

/// Request-Body für `initiate`.
#[derive(Debug, Serialize)]
struct InitiateRequest {
    scope: String,
    redirect_after: String,
    requesting_service: String,
    metadata: InitiateMetadata,
}

/// Antwort von `initiate`.
#[derive(Debug, Deserialize)]
struct InitiateResponse {
    #[serde(default)]
    authorize_url: Option<String>,
}

/// Request-Body für `consume-result`.
#[derive(Debug, Serialize)]
struct ConsumeRequest {
    state_id: String,
}

/// Antwort von `consume-result`: die vom Broker gelieferte Discord-Identität.
#[derive(Debug, Deserialize)]
struct ConsumeResponse {
    #[serde(default)]
    discord_id: Option<String>,
    #[serde(default)]
    discord_name: Option<String>,
    #[serde(default)]
    discord_avatar: Option<String>,
    #[serde(default)]
    discord_roles: Vec<serde_json::Value>,
}

/// Die aus `consume-result` extrahierte, validierte Discord-Identität.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordIdentity {
    pub discord_id: String,
    pub discord_name: String,
    pub discord_avatar: String,
    pub roles: Vec<String>,
}

impl OAuthClient {
    /// Baut den Client aus der Konfiguration.
    ///
    /// Der `reqwest::Client` wird EINMAL erzeugt und wiederverwendet (Map-Befund
    /// „safe": kein Per-Call-Client mehr).
    pub fn new(config: &Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest::Client mit Standard-TLS lässt sich immer bauen");

        Self {
            http,
            base_url: config.discord_oauth_internal_api_base_url.clone(),
            token: config.discord_oauth_internal_api_token.clone(),
            public_url: config.turnier_public_url.clone(),
            guild_id: config.discord_guild_id.clone(),
        }
    }

    /// Basis-URL ohne Trailing-Slash; 503, wenn nicht konfiguriert.
    fn base_url(&self) -> AuthResult<&str> {
        let base = self.base_url.trim_end_matches('/');
        if base.is_empty() {
            return Err(AuthError::ServiceUnavailable(
                "Deadlock-Bots OAuth-Service ist nicht konfiguriert",
            ));
        }
        Ok(base)
    }

    /// Auth-Header-Wert; 503, wenn der Token fehlt.
    fn token(&self) -> AuthResult<&str> {
        let token = self.token.trim();
        if token.is_empty() {
            return Err(AuthError::ServiceUnavailable(
                "Deadlock-Bots OAuth-Service ist nicht authentifiziert",
            ));
        }
        Ok(token)
    }

    /// Sendet einen POST an den Broker und gibt den geparsten Antwort-Typ zurück.
    ///
    /// Fehler-Mapping 1:1 zum Original (`_post_internal_api`):
    ///   - Netzwerk/HTTP-Fehler                          → 502
    ///   - Non-200 mit JSON-`error`/`detail`             → 502 mit dieser Meldung
    ///   - Non-200 mit Plaintext-Body                    → 502 mit diesem Text
    ///   - Non-200 ohne verwertbaren Body                → 502 „Fehler"
    ///   - 200, aber kein JSON                           → 502 „lieferte kein JSON"
    ///   - 200, aber kein JSON-Objekt                    → 502 „ungültiges Payload"
    async fn post<B, R>(&self, path: &str, body: &B) -> AuthResult<R>
    where
        B: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url()?, path);
        let token = self.token()?.to_string();

        let response = self
            .http
            .post(&url)
            .header(INTERNAL_TOKEN_HEADER, token)
            .json(body)
            .send()
            .await
            .map_err(|exc| {
                AuthError::BadGateway(format!(
                    "Deadlock-Bots OAuth-Service nicht erreichbar: {}",
                    reqwest_error_kind(&exc)
                ))
            })?;

        let status = response.status();
        // Body als Text lesen — wir brauchen ihn sowohl für Fehler-Details als
        // auch für das JSON-Parsen, und reqwest gibt den Body nur einmal her.
        let text = response.text().await.map_err(|exc| {
            AuthError::BadGateway(format!(
                "Deadlock-Bots OAuth-Service nicht erreichbar: {}",
                reqwest_error_kind(&exc)
            ))
        })?;

        if !status.is_success() {
            return Err(AuthError::BadGateway(error_detail_from_body(&text)));
        }

        // Erst auf JSON-Objekt prüfen (Original: `isinstance(data, dict)`).
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) if value.is_object() => serde_json::from_value::<R>(value).map_err(|_| {
                AuthError::BadGateway(
                    "Deadlock-Bots OAuth-Service lieferte ein ungültiges Payload".to_string(),
                )
            }),
            Ok(_) => Err(AuthError::BadGateway(
                "Deadlock-Bots OAuth-Service lieferte ein ungültiges Payload".to_string(),
            )),
            Err(_) => Err(AuthError::BadGateway(
                "Deadlock-Bots OAuth-Service lieferte kein JSON".to_string(),
            )),
        }
    }

    /// Startet den OAuth-Flow: holt die `authorize_url` vom Broker.
    ///
    /// `redirect_after` ist `<TURNIER_PUBLIC_URL>/auth/discord/complete`. Gibt die
    /// URL zurück, auf die turnier-api den Browser redirecten soll; 502, wenn der
    /// Broker keine liefert.
    pub async fn initiate_login(&self) -> AuthResult<String> {
        let complete_url = format!(
            "{}/auth/discord/complete",
            self.public_url.trim_end_matches('/')
        );

        let request = InitiateRequest {
            scope: OAUTH_SCOPE.to_string(),
            redirect_after: complete_url,
            requesting_service: REQUESTING_SERVICE.to_string(),
            metadata: InitiateMetadata {
                guild_id: self.guild_id.clone(),
            },
        };

        let data: InitiateResponse = self.post(INITIATE_PATH, &request).await?;
        let authorize_url = data.authorize_url.unwrap_or_default();
        let authorize_url = authorize_url.trim();
        if authorize_url.is_empty() {
            return Err(AuthError::BadGateway(
                "Deadlock-Bots OAuth-Service lieferte keine Authorize-URL".to_string(),
            ));
        }
        Ok(authorize_url.to_string())
    }

    /// Schließt den OAuth-Flow ab: löst `state_id` beim Broker ein und gibt die
    /// validierte Discord-Identität zurück.
    ///
    /// 400, wenn `state_id` leer ist; 502, wenn der Broker keine `discord_id`
    /// liefert. Strings werden wie im Original getrimmt, Rollen auf nicht-leere
    /// String-Werte normalisiert.
    pub async fn complete_login(&self, state_id: &str) -> AuthResult<DiscordIdentity> {
        if state_id.is_empty() {
            return Err(AuthError::BadRequest("Fehlender state_id"));
        }

        let request = ConsumeRequest {
            state_id: state_id.to_string(),
        };
        let data: ConsumeResponse = self.post(CONSUME_RESULT_PATH, &request).await?;

        let discord_id = data.discord_id.unwrap_or_default();
        let discord_id = discord_id.trim();
        if discord_id.is_empty() {
            return Err(AuthError::BadGateway(
                "Deadlock-Bots OAuth-Service lieferte keine Discord-ID".to_string(),
            ));
        }

        let discord_name = data.discord_name.unwrap_or_default().trim().to_string();
        let discord_avatar = data.discord_avatar.unwrap_or_default().trim().to_string();
        let roles = normalize_roles(&data.discord_roles);

        Ok(DiscordIdentity {
            discord_id: discord_id.to_string(),
            discord_name,
            discord_avatar,
            roles,
        })
    }
}

/// Normalisiert die rohen Rollen-Werte: `str(role).strip()`, leere weg.
///
/// Der Broker liefert Snowflake-IDs i. d. R. als Strings, gelegentlich als
/// Zahlen — beide Fälle werden (wie Pythons `str(role)`) in ihre String-Form
/// gebracht.
fn normalize_roles(raw: &[serde_json::Value]) -> Vec<String> {
    raw.iter()
        .map(|v| match v {
            serde_json::Value::String(s) => s.trim().to_string(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Extrahiert die Fehler-Detail-Meldung aus einem Non-200-Body — 1:1 zur
/// Original-Logik: JSON-`error`/`detail` (gestripped, falls non-empty), sonst
/// gestrippter Plaintext, sonst Default.
fn error_detail_from_body(text: &str) -> String {
    const DEFAULT: &str = "Deadlock-Bots OAuth-Service Fehler";
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text) {
        let error = map
            .get("error")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                map.get("detail")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
            });
        if let Some(error) = error {
            return error.trim().to_string();
        }
        // JSON-Objekt ohne verwertbares Feld → Default (Original geht NICHT in
        // den Plaintext-Zweig, wenn der Body bereits ein dict war).
        return DEFAULT.to_string();
    }
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    DEFAULT.to_string()
}

/// Kurzer, stabiler Bezeichner der reqwest-Fehlerklasse (für die Detail-Meldung,
/// analog zu `exc.__class__.__name__` im Original — nie der volle Fehlertext, um
/// keine internen Details zu leaken).
fn reqwest_error_kind(exc: &reqwest::Error) -> &'static str {
    if exc.is_timeout() {
        "Timeout"
    } else if exc.is_connect() {
        "ConnectError"
    } else if exc.is_decode() {
        "DecodeError"
    } else if exc.is_request() {
        "RequestError"
    } else {
        "HTTPError"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fehler_detail_aus_json_error_feld() {
        assert_eq!(
            error_detail_from_body(r#"{"error": "kaputt"}"#),
            "kaputt"
        );
    }

    #[test]
    fn fehler_detail_aus_json_detail_feld() {
        assert_eq!(
            error_detail_from_body(r#"{"detail": "auch kaputt"}"#),
            "auch kaputt"
        );
    }

    #[test]
    fn fehler_detail_error_hat_vorrang_vor_detail() {
        assert_eq!(
            error_detail_from_body(r#"{"error": "a", "detail": "b"}"#),
            "a"
        );
    }

    #[test]
    fn fehler_detail_json_objekt_ohne_feld_ist_default() {
        // Body ist dict, aber ohne brauchbares Feld → Default, NICHT der Roh-Body.
        assert_eq!(
            error_detail_from_body(r#"{"foo": "bar"}"#),
            "Deadlock-Bots OAuth-Service Fehler"
        );
    }

    #[test]
    fn fehler_detail_leeres_error_feld_faellt_auf_default() {
        assert_eq!(
            error_detail_from_body(r#"{"error": "  "}"#),
            "Deadlock-Bots OAuth-Service Fehler"
        );
    }

    #[test]
    fn fehler_detail_plaintext() {
        assert_eq!(error_detail_from_body("  reiner text  "), "reiner text");
    }

    #[test]
    fn fehler_detail_leer_ist_default() {
        assert_eq!(
            error_detail_from_body("   "),
            "Deadlock-Bots OAuth-Service Fehler"
        );
    }

    #[test]
    fn rollen_normalisieren_mischtypen() {
        let raw = vec![
            serde_json::json!("123"),
            serde_json::json!(" 456 "),
            serde_json::json!(""),
            serde_json::json!(789),
            serde_json::Value::Null,
        ];
        assert_eq!(normalize_roles(&raw), vec!["123", "456", "789"]);
    }

    #[test]
    fn fehlende_config_meldet_503() {
        // Leere Basis-URL = nicht konfiguriert (wie der Default-Leerstring im
        // Original). Whitespace-only wäre — wie bei Pythons `if base_url:` nach
        // `rstrip("/")` — KEIN „nicht konfiguriert", daher bewusst echtes "".
        let client = OAuthClient {
            http: reqwest::Client::new(),
            base_url: String::new(),
            token: "tok".to_string(),
            public_url: "https://x".to_string(),
            guild_id: "1".to_string(),
        };
        assert_eq!(client.base_url().unwrap_err().status_code(), 503);
    }

    #[test]
    fn fehlender_token_meldet_503() {
        let client = OAuthClient {
            http: reqwest::Client::new(),
            base_url: "http://x".to_string(),
            token: "".to_string(),
            public_url: "https://x".to_string(),
            guild_id: "1".to_string(),
        };
        assert_eq!(client.token().unwrap_err().status_code(), 503);
    }
}
