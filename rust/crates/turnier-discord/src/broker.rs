//! Broker-Transport: HTTP-POST und -GET an die internen Master-Broker-Endpunkte
//! mit `X-Internal-Token`, plus die Fehlerübersetzung; fehlende Konfiguration
//! und jeder Transport-, Status- und Formatfehler werden geloggt.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{BrokerError, BrokerResult};

/// Header-Name des internen Auth-Tokens.
const INTERNAL_TOKEN_HEADER: &str = "X-Internal-Token";

/// Client gegen den Discord-Master-Broker. Hält EINEN wiederverwendbaren
/// `reqwest::Client` (das Python-Original baute pro Request einen neuen
/// `httpx.AsyncClient`).
#[derive(Clone)]
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
        Self::with_network(base_url, token, &turnier_config::NetworkConfig::default())
    }

    fn with_network(base_url: &str, token: &str, network: &turnier_config::NetworkConfig) -> Self {
        // Timeouts identisch zum Original: 20 s gesamt, 5 s connect, keine Redirects.
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(
                network.broker_request_seconds,
            ))
            .connect_timeout(std::time::Duration::from_secs(
                network.broker_connect_seconds,
            ))
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
        Self::with_network(
            &config.discord_master_broker_base_url,
            &config.discord_master_broker_token,
            &config.network,
        )
    }

    /// Die effektive Basis-URL oder ein [`BrokerError::Unconfigured`].
    fn base_url(&self) -> BrokerResult<&str> {
        if self.base_url.is_empty() {
            tracing::warn!("Discord-Master-Broker ohne Basis-URL aufgerufen");
            return Err(BrokerError::Unconfigured(
                "Discord-Master-Broker ist nicht konfiguriert",
            ));
        }
        Ok(&self.base_url)
    }

    /// Das Auth-Token oder ein [`BrokerError::Unconfigured`].
    fn token(&self) -> BrokerResult<&str> {
        if self.token.is_empty() {
            tracing::warn!("Discord-Master-Broker ohne Token aufgerufen");
            return Err(BrokerError::Unconfigured(
                "Discord-Master-Broker ist nicht authentifiziert",
            ));
        }
        Ok(&self.token)
    }

    /// POST `payload` an `path` und deserialisiert die Antwort nach `T`.
    ///
    /// Fehlerübersetzung:
    /// - Transportfehler → [`BrokerError::Unreachable`].
    /// - Non-200: `detail` aus [`http_error_detail`].
    /// - 200 ohne (Objekt-)JSON → [`BrokerError::BadJson`].
    pub async fn post_internal<T, P>(&self, path: &str, payload: &P) -> BrokerResult<T>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.base_url()?, path);
        let token = self.token()?.to_string();
        let request = self
            .http
            .post(&url)
            .header(INTERNAL_TOKEN_HEADER, token)
            .json(payload);
        send_and_decode(request, "POST", path).await
    }

    pub async fn get_internal<T>(&self, path: &str, query: &[(&str, String)]) -> BrokerResult<T>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url()?, path);
        let token = self.token()?.to_string();
        let request = self
            .http
            .get(&url)
            .header(INTERNAL_TOKEN_HEADER, token)
            .query(query);
        send_and_decode(request, "GET", path).await
    }
}

async fn send_and_decode<T>(
    request: reqwest::RequestBuilder,
    method: &'static str,
    path: &str,
) -> BrokerResult<T>
where
    T: DeserializeOwned,
{
    let response = request.send().await.map_err(|err| {
        tracing::warn!(method, path, error = %err, "Discord-Broker nicht erreichbar");
        BrokerError::Unreachable(err)
    })?;

    let status = response.status();
    let body_text = response.text().await.map_err(|err| {
        tracing::warn!(method, path, status = status.as_u16(), error = %err, "Discord-Broker-Antwort nicht lesbar");
        BrokerError::Unreachable(err)
    })?;

    if status.as_u16() != 200 {
        let detail = http_error_detail(&body_text, DEFAULT_DETAIL, status.as_u16());
        tracing::warn!(
            method,
            path,
            status = status.as_u16(),
            detail = %detail,
            "Discord-Broker antwortete mit Fehler"
        );
        return Err(BrokerError::Http {
            status: status.as_u16(),
            detail,
        });
    }

    let value: serde_json::Value = serde_json::from_str(&body_text).map_err(|_| {
        tracing::warn!(method, path, "Discord-Broker lieferte kein JSON");
        BrokerError::BadJson("Discord-Broker lieferte kein JSON")
    })?;
    if !value.is_object() {
        tracing::warn!(method, path, "Discord-Broker lieferte kein JSON-Objekt");
        return Err(BrokerError::BadJson(
            "Discord-Broker lieferte ein ungültiges Payload",
        ));
    }
    serde_json::from_value(value).map_err(|err| {
        tracing::warn!(method, path, error = %err, "Discord-Broker-Payload passt nicht zum erwarteten Format");
        BrokerError::BadJson("Discord-Broker lieferte ein ungültiges Payload")
    })
}

const DEFAULT_DETAIL: &str = "Discord-Broker Fehler";

pub fn http_error_detail(body_text: &str, default: &str, status: u16) -> String {
    detail_from_body(body_text).unwrap_or_else(|| format!("{default} (HTTP {status})"))
}

fn detail_from_body(body_text: &str) -> Option<String> {
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(body_text)
    {
        return ["error", "detail"]
            .iter()
            .filter_map(|key| map.get(*key).and_then(|v| v.as_str()))
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(str::to_string);
    }
    Some(body_text.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

impl std::fmt::Debug for BrokerClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerClient").finish_non_exhaustive()
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
    fn http_error_detail_liest_error_vor_detail() {
        assert_eq!(
            http_error_detail(r#"{"error":"  kaputt  "}"#, DEFAULT_DETAIL, 500),
            "kaputt"
        );
        assert_eq!(
            http_error_detail(r#"{"detail":"nope"}"#, DEFAULT_DETAIL, 500),
            "nope"
        );
        assert_eq!(
            http_error_detail(r#"{"error":" ","detail":"b"}"#, DEFAULT_DETAIL, 500),
            "b"
        );
    }

    #[test]
    fn http_error_detail_faellt_mit_status_auf_default() {
        assert_eq!(
            http_error_detail(r#"{"other":1}"#, DEFAULT_DETAIL, 502),
            "Discord-Broker Fehler (HTTP 502)"
        );
        assert_eq!(
            http_error_detail("   ", DEFAULT_DETAIL, 404),
            "Discord-Broker Fehler (HTTP 404)"
        );
        assert_eq!(
            http_error_detail("  Bad Gateway  ", DEFAULT_DETAIL, 502),
            "Bad Gateway"
        );
    }
}
