use std::time::Duration;

use anyhow::Context;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use turnier_observer::vconsole::{VConsoleClient, VConsoleError};
use turnier_observer::{AgentAck, AgentHeartbeat, CameraAction, CameraCommand};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
struct Settings {
    server: String,
    token: String,
    vconsole_addr: String,
    bot_account_id: i16,
    poll_ms: u64,
    game_control_enabled: bool,
    request_timeout_seconds: u64,
    heartbeat_every_polls: u8,
    game_connection_attempts: u32,
    game_connection_poll_milliseconds: u64,
    vconsole_ack_milliseconds: u64,
}

impl Settings {
    fn from_config(config: &turnier_config::Config) -> anyhow::Result<Self> {
        let agent = config
            .observer_agent
            .as_ref()
            .context("observer_agent fehlt in der zentralen TOML")?;
        let token = turnier_config::secrets::resolve_first(&["OBSERVER_AGENT_TOKEN"])
            .context("OBSERVER_AGENT_TOKEN fehlt in der Secret-Anbindung")?;
        if token.trim().len() < 24 {
            anyhow::bail!("OBSERVER_AGENT_TOKEN ist zu kurz");
        }
        Ok(Self {
            server: agent.server_base_url.trim_end_matches('/').to_owned(),
            token,
            vconsole_addr: agent.vconsole_address.clone(),
            bot_account_id: agent.bot_account_id,
            poll_ms: agent.poll_milliseconds,
            game_control_enabled: agent.game_control_enabled,
            request_timeout_seconds: agent.request_timeout_seconds,
            heartbeat_every_polls: agent.heartbeat_every_polls,
            game_connection_attempts: agent.game_connection_attempts,
            game_connection_poll_milliseconds: agent.game_connection_poll_milliseconds,
            vconsole_ack_milliseconds: agent.vconsole_ack_milliseconds,
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
    let args = turnier_config::ConfigArgs::parse(std::env::args_os().skip(1))?;
    let config = turnier_config::Config::load_file(&args.path)?;
    if config.observer_agent.is_none() {
        anyhow::bail!("observer_agent fehlt in der zentralen TOML");
    }
    match args.mode {
        turnier_config::ConfigMode::Validate | turnier_config::ConfigMode::Check => {
            println!("{}: gültig", turnier_config::CONFIG_ANCHOR);
            return Ok(());
        }
        turnier_config::ConfigMode::Print => {
            println!("{}", config.safe_status()?);
            return Ok(());
        }
        turnier_config::ConfigMode::BrokerCheck => {
            anyhow::bail!("--check-broker ist nur beim Backend verfügbar")
        }
        _ => {}
    }
    let filter = EnvFilter::new(config.logging.level.as_str());
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let settings = Settings::from_config(&config)?;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(settings.request_timeout_seconds))
        .build()?;

    info!(
        bot_account_id = settings.bot_account_id,
        game_control_enabled = settings.game_control_enabled,
        "Deadlock Observer Agent startet"
    );
    if !settings.game_control_enabled {
        info!(
            "Safe Mode aktiv: keine Verbindung zu VConsole und keine automatisierten Spieleingaben"
        );
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
        if heartbeat_counter % settings.heartbeat_every_polls == 0 {
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
                                game_connected = wait_for_game_connection(client, &settings).await;
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

async fn wait_for_game_connection(client: &VConsoleClient, settings: &Settings) -> bool {
    for _ in 0..settings.game_connection_attempts {
        match client.probe_game_connected().await {
            Ok(true) => return true,
            Ok(false) => {}
            Err(err) => warn!(error = %err, "Observer wartet auf aktive Game-Session"),
        }
        tokio::time::sleep(Duration::from_millis(
            settings.game_connection_poll_milliseconds,
        ))
        .await;
    }
    false
}

async fn connect_vconsole(settings: &Settings) -> Option<VConsoleClient> {
    match VConsoleClient::with_ack_timeout(
        &settings.vconsole_addr,
        Duration::from_millis(settings.vconsole_ack_milliseconds),
    )
    .await
    {
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
