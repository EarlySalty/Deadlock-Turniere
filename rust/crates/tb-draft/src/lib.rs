//! `tb-draft` — Pick/Ban-Draft-Subsystem des Turnier-Backends.
//!
//! Strikte Trennung in zwei Schichten:
//!
//! - **Reine Zustandsmaschine** (DB-frei, voll unit-testbar): [`sequence`]
//!   (feste Sequenz aus 6 Bans + 12 Picks, Index-Fortschritt, Abschluss-Erkennung)
//!   und [`heroes`] (statische Heldenliste + O(1)-Validierung).
//! - **Persistenz** ([`repo`], sqlx): [`start_draft`] (idempotenter Session-Start),
//!   [`take_action`] (eine Aktion ausführen, mit optimistischem Compare-and-Swap)
//!   und [`get_draft_state`] (Vollzustand rekonstruieren).
//!
//! Funktional 1:1 zum Python-Original (`backend/draft/*.py`). Die HTTP-Routen
//! (`/api/draft/*`) leben in tb-web (Welle 5) und verdrahten gegen die hier
//! exportierte pub-API.

pub mod error;
pub mod heroes;
pub mod sequence;
pub mod state;

mod repo;

pub use error::{DraftError, DraftResult};
pub use heroes::{is_valid_hero, DEADLOCK_HEROES};
pub use repo::{get_draft_state, start_draft, take_action};
pub use sequence::{
    is_complete, step_at, ActionType, SequenceStep, TeamSlot, DEFAULT_SEQUENCE, SEQUENCE_LEN,
};
pub use state::{ActionOutcome, DraftAction, DraftSession, DraftState};
