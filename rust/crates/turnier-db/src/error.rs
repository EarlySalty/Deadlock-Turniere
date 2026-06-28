//! Fehler-Typ der Persistenzschicht.

use thiserror::Error;

/// Fehler beim DB-Zugriff oder beim Anwenden der Migration.
#[derive(Debug, Error)]
pub enum DbError {
    /// Ein Fehler aus sqlx (Query, Verbindung, Decode …).
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    /// Ein Fehler beim Anwenden der eingebetteten Migration.
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

/// Bequemer Result-Alias für die Persistenzschicht.
pub type DbResult<T> = Result<T, DbError>;
