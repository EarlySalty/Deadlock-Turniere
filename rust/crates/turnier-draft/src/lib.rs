//! `turnier-draft` — Pick/Ban-Draft-Subsystem des Turnier-Backends.
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
//! (`/api/draft/*`) leben in turnier-api (Welle 5) und verdrahten gegen die hier
//! exportierte pub-API.

pub mod comp;
pub mod error;
pub mod heroes;
pub mod heroes_provider;
pub mod sequence;
pub mod state;

mod repo;

pub use error::{DraftError, DraftResult};
pub use heroes::{is_valid_hero, DEADLOCK_HEROES};
pub use heroes_provider::{load_heroes, Hero, HeroFetcher, HeroesProvider, ReqwestHeroFetcher};
pub use repo::{
    claim_room, create_lobby, create_room, get_draft_state, get_state_by_code, leave_room,
    rematch_room, retry_lobby_request, room_ready, start_draft, take_action, take_lobby_action,
    ClaimOutcome, CreateLobbyOptions, CreateRoomOptions, LobbyCredentials, ReadyOutcome,
};
pub use sequence::{
    is_complete, preset, sequence_for_bans, step_at, ActionType, SequenceStep, TeamSlot,
    COMPETITIVE_1BAN, DEFAULT_SEQUENCE, QUICK_NO_BAN, SEQUENCE_LEN,
};
pub use state::{ActionOutcome, DraftAction, DraftSession, DraftState};

pub use repo::{
    get_state_by_code_with_heroes, take_action_with_heroes, take_lobby_action_with_heroes,
};
