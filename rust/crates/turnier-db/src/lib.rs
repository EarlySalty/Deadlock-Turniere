//! `turnier-db` — Persistenz-Fundament des Turnier-Backends.
//!
//! Stellt den geteilten `PgPool` zur zentralen Postgres/TimescaleDB bereit.
//! Domänen-Crates führen ihre eigenen Queries gegen den hier erzeugten Pool aus;
//! diese Crate enthält bewusst keine Geschäftslogik.

pub mod dynamic_sql;
pub mod error;
pub mod pool;

pub use error::{DbError, DbResult};
pub use pool::{connect_central, run_migrations, Pool};
#[cfg(feature = "testing")]
pub use pool::{test_pool, TestDb};

// sqlx wird re-exportiert, damit Domänen-Crates dieselbe Version nutzen, ohne
// sie einzeln deklarieren zu müssen.
pub use sqlx;
