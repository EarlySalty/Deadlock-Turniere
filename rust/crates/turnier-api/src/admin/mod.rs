//! Admin-/Mod-Router (portiert `tournament/admin_routes.py`, der grösste
//! Monolith des Backends, ~3330 Zeilen).
//!
//! Aufgeteilt entlang der fachlichen Cluster in thematische Submodule:
//!
//! - [`tournaments`] — Turnier-CRUD + Status-Maschine (Statuswechsel über
//!   [`turnier_scheduler::advance_tournament_status`] mit `source="manual"`).
//! - [`phases`] — Check-in finalisieren + Solo-Spieler zufällig verteilen.
//! - [`teams`] — Team-/Mitglieder-/Bewerbungs-/Signup-Verwaltung.
//! - [`brackets`] — Gruppen-/Bracket-Generierung.
//! - [`matches`] — Bracket-Match-Leitstand (Ergebnis, Serien, Steam-Lobby,
//!   ConVars/Event-Presets).
//! - [`group_matches`] — Group-Match-Leitstand (DRY mit [`matches`] über
//!   [`steam_ops`] und `MatchKind::Group`).
//! - [`casters`] — Caster-Verwaltung (Turnier-Ebene; Match-Ebene deprecated).
//! - [`voice`] — Discord-Voice-Steuerung.
//!
//! Geteilte Bausteine: [`helpers`] (Audit/Loader/Invarianten/Tree-Lösch-SQL),
//! [`loaders`] (Detail-Lese-Loader), [`tournament_row`] (Turnier-DTO-Mapping),
//! [`steam_ops`] (parametrisierte Steam-Lobby-Operationen).

use axum::Router;

use crate::state::AppState;

mod brackets;
mod casters;
mod group_matches;
mod helpers;
mod loaders;
mod matches;
mod phases;
mod steam_ops;
mod teams;
mod tournament_row;
mod tournaments;
mod voice;

/// Router aller Admin-Endpunkte (alle Submodule gemerged).
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(tournaments::router())
        .merge(phases::router())
        .merge(teams::router())
        .merge(brackets::router())
        .merge(matches::router())
        .merge(group_matches::router())
        .merge(casters::router())
        .merge(voice::router())
}
