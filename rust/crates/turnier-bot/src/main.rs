//! `turnier-bot` — Composition-Root und Binary des Turnier-Backends.
//!
//! Verdrahtet die Crates zu einem laufenden Dienst: Config laden → Pool öffnen →
//! Migrationen anwenden → [`AppState`] bauen → Scheduler-Loop starten → axum
//! servieren. Mit `--check` bootet der Prozess vollständig (inkl. AppState +
//! Router), serviert aber nicht — für CI/Smoke-Tests.

use std::path::Path;
use std::sync::Arc;

use turnier_config::Config;
use turnier_discord::{BrokerClient, DiscordNotifier};
use turnier_scheduler::{start_scheduler, Scheduler};
use turnier_api::{build_router, AppState};

/// Anzahl Pool-Verbindungen auf die Turnier-DB.
const DB_MAX_CONNECTIONS: u32 = 16;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let check_only = std::env::args().any(|a| a == "--check");
    let config = Arc::new(Config::from_env());

    ensure_dirs(&config);

    // Pool + Migrationen (idempotent gegen die geteilte Live-DB).
    let pool = turnier_db::connect_str(&config.database_path, DB_MAX_CONNECTIONS).await?;
    turnier_db::run_migrations(&pool).await?;
    tracing::info!(db = %config.database_path, "Datenbank verbunden, Migrationen angewendet");

    let state = AppState::build(pool.clone(), config.clone()).await?;

    // Scheduler-Loop (Phasenübergänge + Reminder) als Hintergrund-Task.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let scheduler = Scheduler::new(
        pool.clone(),
        state.match_manager.clone(),
        scheduler_notifier(&config, &pool),
    );
    let scheduler_handle = tokio::spawn(start_scheduler(scheduler, shutdown_rx));

    let app = build_router(state);

    if check_only {
        tracing::info!("--check: AppState + Router gebaut, Boot erfolgreich — kein Servieren");
        let _ = shutdown_tx.send(true);
        let _ = scheduler_handle.await;
        return Ok(());
    }

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

/// Legt DB-Verzeichnis und Avatar-Verzeichnis an (wie `init_db` im Original).
fn ensure_dirs(config: &Config) {
    if let Some(parent) = Path::new(&config.database_path).parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            tracing::warn!(error = %err, "DB-Verzeichnis konnte nicht angelegt werden");
        }
    }
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
