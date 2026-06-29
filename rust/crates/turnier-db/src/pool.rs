//! Aufbau des geteilten `SqlitePool` und Anwenden der Migration.
//!
//! Gegenüber dem Python-Original (eine frische Verbindung pro Request, ohne
//! `busy_timeout`) nutzt der Port EINEN Pool mit gesetztem Busy-Timeout — das
//! beseitigt das Lock-Risiko unter Last (`SQLITE_BUSY`).

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::SqlitePool;

use crate::error::DbResult;

/// Geteilter Verbindungspool auf die Turnier-SQLite-Datenbank.
pub type Pool = SqlitePool;

/// Eingebettete, konsolidierte Migration (siehe `migrations/0001_initial.sql`).
/// `sqlx::migrate!` liest die Dateien zur Compile-Zeit ein — kein DB-Zugriff zum
/// Bauen nötig.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Öffnet den Pool auf die Datei unter `db_path` und konfiguriert die PRAGMAs
/// (WAL, Foreign Keys an, Busy-Timeout, `synchronous=NORMAL`). Legt die Datei
/// bei Bedarf an.
pub async fn connect(db_path: &Path, max_connections: u32) -> DbResult<Pool> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections.max(1))
        .connect_with(options)
        .await?;

    Ok(pool)
}

/// Variante, die den Pfad als String (z. B. aus der Config) entgegennimmt.
pub async fn connect_str(db_path: &str, max_connections: u32) -> DbResult<Pool> {
    let options = SqliteConnectOptions::from_str(db_path)
        .unwrap_or_else(|_| SqliteConnectOptions::new().filename(db_path))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections.max(1))
        .connect_with(options)
        .await?;

    Ok(pool)
}

/// Wendet alle ausstehenden Migrationen idempotent an. Auf der bestehenden
/// Live-DB ist das ein No-op (alle Tabellen existieren bereits).
pub async fn run_migrations(pool: &Pool) -> DbResult<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}
