//! Öffentlicher/Teilnehmer-Router — portiert `tournament/routes.py` vollständig.
//!
//! Der große Router rund um die Turnier-Teilnahme: Turnierliste/-detail abrufen,
//! Teams erstellen/beitreten/verlassen, Solo-Anmeldung, Captain-Einladungen
//! (per signup_id oder direkt), Bewerbungen, Player-Check-in samt Status sowie
//! Lesezugriff auf Bracket und Gruppen. Ergebnis-Selbstmeldung liegt bewusst in
//! [`crate::operations`] — hier nur Bracket/Groups read-only.
//!
//! Aufteilung in thematische Submodule:
//! - [`helpers`] — Name-Resolution, gebatchte Rang-Anreicherung, Read-Modell-
//!   Loader, Guards, Signup-Sync, Turnier-Row→DTO-Mapping (DRY-Konsolidierung
//!   der im Original 4–5-fach kopierten Queries/Guards).
//! - [`tournaments`] — öffentliche Lese-Routen + `/me`.
//! - [`teams`] — Team-Lifecycle.
//! - [`invitations`] — Einladungs-/Bewerbungs-Flow.
//! - [`signups`] — Solo-Anmeldung + Check-in.

use axum::Router;

use crate::state::AppState;

pub mod helpers;
pub mod invitations;
pub mod signups;
pub mod teams;
pub mod tournaments;

/// Router aller öffentlichen + Teilnehmer-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(tournaments::router())
        .merge(teams::router())
        .merge(invitations::router())
        .merge(signups::router())
}
