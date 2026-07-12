//! `turnier-automatik` — Fundament der automatischen Turnierplanung.
//!
//! Diese Crate ist in Phase 1a bewusst eine reine Bibliothek: DB-Zugriffe ueber
//! den geteilten [`turnier_db::Pool`], plus reine, unit-testbare Logik fuer
//! Proposal-Zustaende, Empfaenger-Berechnung und die idempotente Persistenz der
//! Routine-Turniere. Discord-/Scheduler-Orchestrierung bleibt in ihren Crates.

pub mod error;
pub mod optout;
pub mod presets;
pub mod proposals;
pub mod routine;
pub mod signals;

pub use error::{AutomatikError, AutomatikResult};
