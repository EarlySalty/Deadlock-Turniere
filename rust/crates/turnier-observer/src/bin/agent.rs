use std::time::Duration;

use anyhow::Context;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use turnier_observer::{AgentAck, AgentHeartbeat, CameraAction, CameraCommand};
use turnier_observer::vconsole::{VConsoleClient, VConsoleError};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
struct Settings {
    server: String,
    token: String,
    vconsole_addr: String,
    bot_account_id: i16,
    poll_ms: u64,
    game_control_enabled: bool,
}

impl Settings {
    fn from_env() -> anyhow::Result<Self> {
        let server = std::env::var("OBSERVER_SERVER_BASE_URL")
            .context("OBSERVER_SERVER_BASE_URL fehlt")?
            .trim_end_matches('/')
            .to_string();
        let token = std::env::var("OBSERVER_AGENT_TOKEN").context("OBSERVER_AGENT_TOKEN fehlt")?;
        if token.trim().len() < 24 {
            anyhow::bail!("OBSERVER_AGENT_TOKEN ist zu kurz");
        }
        let bot_account_id = std::env::var("OBSERVER_BOT_ACCOUNT_ID")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(2);
        if bot_account_id != 2 {
            anyhow::bail!("dieser Agent ist fuer den Test explizit auf Steam Bot 2 begrenzt");
        }
        Ok(Self {
            server,
            token,
            vconsole_addr: std::env::var("OBSERVER_VCONSOLE_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:29000".to_string()),
            bot_account_id,
            poll_ms: std::env::var("OBSERVER_POLL_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .map(|value: u64| value.clamp(150, 2_000))
                .unwrap_or(250),
            game_control_enabled: std::env::var("OBSERVER_GAME_CONTROL_ENABLED")
                .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
                .unwrap_or(false),
        })
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }
}

#[derive(Deserialize)]
struct CommandsResponse {
    commands: Vec<CameraCommand>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let settings = Settings::from_env()?;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    info!(
        bot_account_id = settings.bot_account_id,
        game_control_enabled = settings.game_control_enabled,
        "Deadlock Observer Agent startet"
    );
    if !settings.game_control_enabled {
        info!("Safe Mode aktiv: keine Verbindung zu VConsole und keine automatisierten Spieleingaben");
    }

    let mut vconsole = if settings.game_control_enabled {
        connect_vconsole(&settings).await
    } else {
        None
    };
    let mut game_connected = false;
    let mut last_command_id = 0_i64;
    let mut current_action: Option<CameraAction> = None;
    let mut heartbeat_counter = 0_u8;
    let mut ticker = tokio::time::interval(Duration::from_millis(settings.poll_ms));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        heartbeat_counter = heartbeat_counter.wrapping_add(1);
        if heartbeat_counter % 20 == 0 {
            game_connected = match vconsole.as_ref() {
                Some(client) => match client.probe_game_connected().await {
                    Ok(connected) => connected,
                    Err(err) => {
                        warn!(error = %err, "Observer-Game-Status konnte nicht bestätigt werden");
                        false
                    }
                },
                None => false,
            };
            if let Err(err) = heartbeat(
                &http,
                &settings,
                vconsole.is_some(),
                game_connected,
                current_action.clone(),
                (last_command_id > 0).then_some(last_command_id),
            )
            .await
            {
                warn!(error = %err, "Observer-Heartbeat fehlgeschlagen");
            }
        }

        let response = http
            .get(format!(
                "{}/api/observer/agent/commands?after_id={last_command_id}",
                settings.server
            ))
            .header(AUTHORIZATION, settings.bearer())
            .send()
            .await;
        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                warn!(status = %response.status(), "Observer-Command-Poll abgelehnt");
                continue;
            }
            Err(err) => {
                warn!(error = %err, "Observer-Command-Poll fehlgeschlagen");
                continue;
            }
        };
        let body: CommandsResponse = match response.json().await {
            Ok(body) => body,
            Err(err) => {
                warn!(error = %err, "Observer-Command-Antwort ungueltig");
                continue;
            }
        };

        for command in body.commands {
            if command.id <= last_command_id {
                continue;
            }
            if command.expires_at < chrono::Utc::now() {
                let _ = ack(&http, &settings, command.id, false, Some("expired")).await;
                last_command_id = command.id;
                continue;
            }
            if !settings.game_control_enabled {
                let _ = ack(
                    &http,
                    &settings,
                    command.id,
                    false,
                    Some("game_control_disabled_safe_mode"),
                )
                .await;
                warn!(
                    command_id = command.id,
                    action = ?command.action,
                    "Kameraaktion im Safe Mode blockiert"
                );
                last_command_id = command.id;
                continue;
            }
            if vconsole.is_none() {
                vconsole = connect_vconsole(&settings).await;
            }
            let result = match vconsole.as_ref() {
                Some(client) => match &command.action {
                    CameraAction::SpectateLobby { .. } => {
                        match client.apply_camera_action(&command.action).await {
                            Ok(()) => {
                                game_connected = wait_for_game_connection(client).await;
                                if game_connected {
                                    Ok(())
                                } else {
                                    Err(VConsoleError::Protocol(
                                        "spectate command was sent, but no active game session was confirmed".into(),
                                    ))
                                }
                            }
                            Err(err) => Err(err),
                        }
                    }
                    CameraAction::HeroChase { .. } | CameraAction::PlayerView { .. } => {
                        if !game_connected {
                            game_connected = client.probe_game_connected().await.unwrap_or(false);
                        }
                        if !game_connected {
                            Err(VConsoleError::Protocol(
                                "camera action blocked: no active game session confirmed".into(),
                            ))
                        } else {
                            client.apply_camera_action(&command.action).await
                        }
                    }
                    CameraAction::Directed => client.apply_camera_action(&command.action).await,
                },
                None => {
                    let _ = ack(
                        &http,
                        &settings,
                        command.id,
                        false,
                        Some("vconsole_unavailable"),
                    )
                    .await;
                    last_command_id = command.id;
                    continue;
                }
            };
            match result {
                Ok(()) => {
                    current_action = Some(command.action.clone());
                    let _ = ack(&http, &settings, command.id, true, None).await;
                    info!(
                        command_id = command.id,
                        reason = %command.reason,
                        action = ?command.action,
                        "Observer-Kamera angewendet"
                    );
                }
                Err(err) => {
                    warn!(command_id = command.id, error = %err, "VConsole-Kameraaktion fehlgeschlagen");
                    let detail = format!("vconsole: {err}");
                    let _ = ack(&http, &settings, command.id, false, Some(&detail)).await;
                    vconsole = None;
                }
            }
            last_command_id = command.id;
        }
    }
}

async fn wait_for_game_connection(client: &VConsoleClient) -> bool {
    for _ in 0..20 {
        match client.probe_game_connected().await {
            Ok(true) => return true,
            Ok(false) => {}
            Err(err) => warn!(error = %err, "Observer wartet auf aktive Game-Session"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    false
}

async fn connect_vconsole(settings: &Settings) -> Option<VConsoleClient> {
    match VConsoleClient::connect(&settings.vconsole_addr).await {
        Ok(client) => {
            info!(addr = %settings.vconsole_addr, "VConsole verbunden");
            Some(client)
        }
        Err(err) => {
            warn!(addr = %settings.vconsole_addr, error = %err, "VConsole noch nicht erreichbar");
            None
        }
    }
}

async fn heartbeat(
    http: &reqwest::Client,
    settings: &Settings,
    vconsole_connected: bool,
    game_connected: bool,
    current_action: Option<CameraAction>,
    last_command_id: Option<i64>,
) -> anyhow::Result<()> {
    let body = AgentHeartbeat {
        agent_version: VERSION.to_string(),
        bot_account_id: settings.bot_account_id,
        vconsole_connected,
        game_connected,
        current_action,
        last_command_id,
    };
    let response = http
        .post(format!("{}/api/observer/agent/heartbeat", settings.server))
        .header(AUTHORIZATION, settings.bearer())
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await?;
    if !response.status().is_success() {
        anyhow::bail!("heartbeat status {}", response.status());
    }
    Ok(())
}

async fn ack(
    http: &reqwest::Client,
    settings: &Settings,
    command_id: i64,
    ok: bool,
    detail: Option<&str>,
) -> anyhow::Result<()> {
    let response = http
        .post(format!(
            "{}/api/observer/agent/commands/{command_id}/ack",
            settings.server
        ))
        .header(AUTHORIZATION, settings.bearer())
        .json(&AgentAck {
            ok,
            detail: detail.map(ToOwned::to_owned),
        })
        .send()
        .await?;
    if !response.status().is_success() {
        anyhow::bail!("ack status {}", response.status());
    }
    Ok(())
}
