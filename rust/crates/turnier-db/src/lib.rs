//! `turnier-db` — Persistenz-Fundament des Turnier-Backends.
//!
//! Stellt den geteilten `SqlitePool`, das PRAGMA-Setup und die eingebettete,
//! konsolidierte Migration bereit. Domänen-Crates führen ihre eigenen Queries
//! gegen den hier erzeugten Pool aus; diese Crate enthält bewusst keine
//! Geschäftslogik.

pub mod error;
pub mod pool;

pub use error::{DbError, DbResult};
pub use pool::{connect, connect_str, run_migrations, Pool, MIGRATOR};

// sqlx wird re-exportiert, damit Domänen-Crates dieselbe Version nutzen, ohne
// sie einzeln deklarieren zu müssen.
pub use sqlx;
