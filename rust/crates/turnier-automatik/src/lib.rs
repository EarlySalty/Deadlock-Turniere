//! `turnier-automatik` — Fundament der automatischen Turnierplanung.
//!
//! Diese Crate ist in Phase 1a bewusst eine reine Bibliothek: DB-Zugriffe ueber
//! den geteilten [`turnier_db::Pool`], plus reine, unit-testbare Logik fuer
//! Proposal-Zustaende und Empfaenger-Berechnung. Discord-, Broker-, Scheduler-
//! und HTTP-Verdrahtung folgen erst in spaeteren Phasen.

pub mod error;
pub mod optout;
pub mod presets;
pub mod proposals;
pub mod signals;

pub use error::{AutomatikError, AutomatikResult};
