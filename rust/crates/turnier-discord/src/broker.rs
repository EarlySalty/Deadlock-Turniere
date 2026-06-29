//! Broker-Transport: HTTP-POST an die internen Master-Broker-Endpunkte mit
//! `X-Internal-Token`, plus die exakt nachgebildete Fehlerübersetzung.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{BrokerError, BrokerResult};

/// Header-Name des internen Auth-Tokens.
const INTERNAL_TOKEN_HEADER: &str = "X-Internal-Token";

/// Client gegen den Discord-Master-Broker. Hält EINEN wiederverwendbaren
/// `reqwest::Client` (das Python-Original baute pro Request einen neuen
/// `httpx.AsyncClient`).
#[derive(Debug, Clone)]
pub struct BrokerClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl BrokerClient {
    /// Baut den Client aus der aufgelösten Config. `base_url` wird (wie im
    /// Original) am Ende von `/` befreit; ob Basis-URL/Token gesetzt sind, wird
    /// erst beim Aufruf geprüft (Lazy-Validierung wie in `_broker_base_url`).
    pub fn new(base_url: &str, token: &str) -> Self {
        // Timeouts identisch zum Original: 20 s gesamt, 5 s connect, keine Redirects.
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .connect_timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest-Client baut immer (statische Konfiguration)");

        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.trim().to_string(),
        }
    }

    /// Komfort-Konstruktor aus [`turnier_config::Config`].
    pub fn from_config(config: &turnier_config::Config) -> Self {
        Self::new(&config.discord_master_broker_base_url, &config.discord_master_broker_token)
    }

    /// Die effektive Basis-URL oder ein [`BrokerError::Unconfigured`].
    fn base_url(&self) -> BrokerResult<&str> {
        if self.base_url.is_empty() {
            return Err(BrokerError::Unconfigured("Discord-Master-Broker ist nicht konfiguriert"));
        }
        Ok(&self.base_url)
    }

    /// Das Auth-Token oder ein [`BrokerError::Unconfigured`].
    fn token(&self) -> BrokerResult<&str> {
        if self.token.is_empty() {
            return Err(BrokerError::Unconfigured("Discord-Master-Broker ist nicht authentifiziert"));
        }
        Ok(&self.token)
    }

    /// POST `payload` an `path` und deserialisiert die Antwort nach `T`.
    ///
    /// Repliziert die Fehlerübersetzung von `_post_internal_api` EXAKT:
    /// - Transportfehler → [`BrokerError::Unreachable`].
    /// - Non-200: `detail` = JSON-`error` ∨ JSON-`detail` (getrimmt, falls
    ///   nicht leer), sonst getrimmter Response-Body, sonst `"Discord-Broker
    ///   Fehler"`.
    /// - 200 ohne (Objekt-)JSON → [`BrokerError::BadJson`].
    pub async fn post_internal<T, P>(&self, path: &str, payload: &P) -> BrokerResult<T>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.base_url()?, path);
        let token = self.token()?.to_string();

        let response = self
            .http
            .post(&url)
            .header(INTERNAL_TOKEN_HEADER, token)
            .json(payload)
            .send()
            .await
            .map_err(BrokerError::Unreachable)?;

        let status = response.status();
        // Body wird in jedem Fall als Text gelesen (für die Non-200-Detailfindung).
        let body_text = response.text().await.map_err(BrokerError::Unreachable)?;

        if status.as_u16() != 200 {
            return Err(BrokerError::Http { status: status.as_u16(), detail: error_detail(&body_text) });
        }

        // 200: erst als JSON-Objekt validieren, dann nach T deserialisieren.
        let value: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|_| BrokerError::BadJson("Discord-Broker lieferte kein JSON"))?;
        if !value.is_object() {
            return Err(BrokerError::BadJson("Discord-Broker lieferte ein ungültiges Payload"));
        }
        serde_json::from_value(value)
            .map_err(|_| BrokerError::BadJson("Discord-Broker lieferte ein ungültiges Payload"))
    }
}

/// Ermittelt die Fehler-`detail`-Meldung aus dem Non-200-Body — exakt nach der
/// Reihenfolge im Python-Original.
fn error_detail(body_text: &str) -> String {
    const DEFAULT: &str = "Discord-Broker Fehler";

    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(body_text) {
        let from_field = map
            .get("error")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| map.get("detail").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()));
        if let Some(err) = from_field {
            return err.trim().to_string();
        }
        // JSON-Objekt vorhanden, aber kein nutzbares error/detail → Default
        // (das Original fällt NICHT auf response.text zurück, wenn body ein dict ist).
        return DEFAULT.to_string();
    }

    let trimmed = body_text.trim();
    if trimmed.is_empty() {
        DEFAULT.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_wird_normalisiert() {
        let c = BrokerClient::new("http://host:8766///", "tok");
        assert_eq!(c.base_url().unwrap(), "http://host:8766");
    }

    #[test]
    fn fehlende_konfiguration_meldet_unconfigured() {
        let c = BrokerClient::new("", "tok");
        assert!(matches!(c.base_url(), Err(BrokerError::Unconfigured(_))));
        let c2 = BrokerClient::new("http://x", "   ");
        assert!(matches!(c2.token(), Err(BrokerError::Unconfigured(_))));
    }

    #[test]
    fn error_detail_aus_json_error_feld() {
        assert_eq!(error_detail(r#"{"error":"  kaputt  "}"#), "kaputt");
    }

    #[test]
    fn error_detail_aus_json_detail_feld() {
        assert_eq!(error_detail(r#"{"detail":"nope"}"#), "nope");
    }

    #[test]
    fn error_detail_json_ohne_felder_gibt_default() {
        // dict ohne error/detail → Default (kein Fallback auf body-text).
        assert_eq!(error_detail(r#"{"other":1}"#), "Discord-Broker Fehler");
        // leeres error → Default.
        assert_eq!(error_detail(r#"{"error":"   "}"#), "Discord-Broker Fehler");
    }

    #[test]
    fn error_detail_aus_plaintext() {
        assert_eq!(error_detail("  Bad Gateway  "), "Bad Gateway");
    }

    #[test]
    fn error_detail_leer_gibt_default() {
        assert_eq!(error_detail("   "), "Discord-Broker Fehler");
        assert_eq!(error_detail(""), "Discord-Broker Fehler");
    }
}
