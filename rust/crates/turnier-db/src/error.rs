//! Fehler-Typ der Persistenzschicht.

use thiserror::Error;

/// Fehler beim zentralen DB-Zugriff.
#[derive(Debug, Error)]
pub enum DbError {
    /// Ein Fehler aus der zentralen DB-Infrastruktur.
    #[error(transparent)]
    Central(#[from] dl_central_db::CentralDbError),

    /// Ein Fehler aus sqlx (Query, Verbindung, Decode …).
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    /// Ein Fehler beim Anwenden von Migrationen in Tests/Altpfaden.
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

/// Bequemer Result-Alias für die Persistenzschicht.
pub type DbResult<T> = Result<T, DbError>;
