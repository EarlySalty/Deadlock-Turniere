//! Aufbau des geteilten `PgPool` zur zentralen Postgres/TimescaleDB.

use sqlx::PgPool;

use crate::error::DbResult;

/// Geteilter Verbindungspool auf die zentrale Turnier-Postgres-Datenbank.
pub type Pool = PgPool;

#[cfg(feature = "testing")]
pub use dl_central_db::TestDb;

/// Baut den zentralen Pool aus `DEADLOCK_CENTRAL_DSN`.
pub async fn connect_central() -> DbResult<Pool> {
    let dsn = dl_central_db::dsn_from_env()?;
    let pool = dl_central_db::connect_pool(&dsn).await?;
    Ok(pool)
}

/// Baut eine wegwerfbare zentrale Testdatenbank auf und wendet die zentralen
/// Migrationen an.
#[cfg(feature = "testing")]
pub async fn test_pool() -> DbResult<TestDb> {
    let pool = dl_central_db::test_pool().await?;
    Ok(pool)
}

/// Produktive PG-Migrationen werden zentral durch `dl-central-migrate`
/// ausgefuehrt. Dieser Kompatibilitaets-Hook bleibt absichtlich ein No-op, bis
/// die Composition Root in T12 entfernt/umgestellt wird.
pub async fn run_migrations(_pool: &Pool) -> DbResult<()> {
    Ok(())
}
