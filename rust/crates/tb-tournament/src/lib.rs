//! `tb-tournament` — die Turnier-Engine, das Herz des Backends.
//!
//! Strikte Trennung in zwei Schichten:
//!
//! - **Reine Algorithmen** (DB-frei, voll unit-testbar): [`engine`] (Slots,
//!   Seed-Reihenfolge, Slot-Verteilung, Snake-Draft, Double-Elim-Mathematik,
//!   Team-Namensvergabe), [`status`] (Modus/Status-Übergänge), [`points`]
//!   (Platzierungs-/Punkte-Logik), [`mini_groups`] (Tiebreaker-Kette,
//!   Punkt-Differenz).
//! - **Persistenz** ([`persist`], sqlx): `generate_bracket`, `generate_groups`,
//!   `generate_group_matches`, `finalize_checkin`, `assign_random_teams`,
//!   `advance_bracket_winner`, `complete_mini_group_round_robin`,
//!   `recalculate_player_points`. Jede öffentliche Operation läuft in EINER
//!   Transaktion; der Audit-Log committet NICHT mehr selbst.
//!
//! Funktional 1:1 zum Python-Original (`backend/tournament/*.py`); bewiesen durch
//! die portierten Paritätstests unter `tests/`. Determinismus über einen
//! injizierbaren [`rand::rngs::StdRng`]; das Python-Modulo `-1 % n` ist überall
//! als `rem_euclid` umgesetzt.

pub mod engine;
pub mod error;
pub mod mini_groups;
pub mod points;
pub mod status;

mod persist;

pub use error::{TournamentError, TournamentResult};

// Reine API (DB-frei).
pub use engine::groups::auto_num_groups;
pub use engine::naming::name_key;
pub use engine::slots::BracketSlot;
pub use status::{
    determine_tournament_mode, is_valid_transition, valid_next_statuses,
    AUTO_GROUP_STAGE_THRESHOLD,
};

// Persistenz-API.
pub use persist::{
    advance_bracket_winner, assign_random_teams, build_checkin_snapshot_token,
    complete_mini_group_round_robin, finalize_checkin, generate_bracket, generate_group_matches,
    generate_groups, recalculate_player_points, AddedPlayer, CreatedTeam, FinalizeCheckinParams,
    FinalizeCheckinResult, NoShuffle, RemovedPlayer, RngShuffler, SoloPlayer, SoloShuffler,
    TeamWarning,
};
