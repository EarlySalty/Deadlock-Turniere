//! `turnier-bot` — Composition-Root und Binary des Turnier-Backends.
//!
//! Verdrahtet die Crates zu einem laufenden Dienst: Config laden → Pool öffnen →
//! [`AppState`] bauen → Scheduler-Loop starten → axum servieren. Mit `--check`
//! bootet der Prozess bis AppState/Scheduler/Router, serviert aber nicht und
//! startet keine Scheduler-Checks — für CI/Smoke-Tests.

use std::sync::Arc;

use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_discord::{BrokerClient, DiscordNotifier};
use turnier_scheduler::{start_scheduler, Scheduler};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let check_only = std::env::args().any(|a| a == "--check");
    let config = Arc::new(Config::from_env());

    ensure_dirs(&config);

    // Zentrale PG-DB: DSN kommt aus DEADLOCK_CENTRAL_DSN; Wert niemals loggen.
    let pool = turnier_db::connect_central().await?;
    tracing::info!("central DB pool opened");

    let state = AppState::build(pool.clone(), config.clone()).await?;
    let scheduler = Scheduler::new(
        pool.clone(),
        state.match_manager.clone(),
        scheduler_notifier(&config, &pool),
        &config,
    )?;
    let app = build_router(state);

    if check_only {
        tracing::info!("--check: AppState + Scheduler + Router gebaut, kein Servieren");
        return Ok(());
    }

    // Scheduler-Loop (Phasenübergänge + Reminder) als Hintergrund-Task.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let scheduler_handle = tokio::spawn(start_scheduler(scheduler, shutdown_rx));

    let addr = format!("{}:{}", config.backend_host, config.backend_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "Turnier-Backend lauscht");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await?;

    let _ = scheduler_handle.await;
    Ok(())
}

/// Baut den Notifier für den Scheduler (eigene Instanz, teilt Pool/Broker).
fn scheduler_notifier(config: &Config, pool: &turnier_db::Pool) -> DiscordNotifier {
    let broker = BrokerClient::from_config(config);
    DiscordNotifier::new(broker, pool.clone(), config)
}

/// Legt das Avatar-Verzeichnis an (wie `init_db` im Original).
fn ensure_dirs(config: &Config) {
    if let Err(err) = std::fs::create_dir_all(&config.avatar_dir) {
        tracing::warn!(error = %err, "Avatar-Verzeichnis konnte nicht angelegt werden");
    }
}

/// Initialisiert das Tracing-Subscriber (Env-Filter, Default `info`).
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();
}

/// Wartet auf Ctrl-C und signalisiert dann den Shutdown an den Scheduler.
async fn shutdown_signal(shutdown_tx: tokio::sync::watch::Sender<bool>) {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("Shutdown-Signal empfangen");
    let _ = shutdown_tx.send(true);
}
