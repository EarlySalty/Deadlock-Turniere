//! `turnier-match` — der Match-Lebenszyklus des Turnier-Backends.
//!
//! Orchestriert Steam-Custom-Lobbys (erstellen/starten/verlassen) über die
//! externe `steam_tasks`-Bridge-DB, Hero-Assignments je Spielmodus, Live-Event-
//! ConVar-Presets, Best-of-N-Serien und die VEREINHEITLICHTE Ergebnisverarbeitung
//! (Bracket + Group) mit Gewinner-Propagation ins Bracket.
//!
//! ## Modul-Schnitt (der Python-Monolith `manager.py` ist aufgebrochen)
//! - [`kind`] — [`MatchKind`] (Enum statt stringly-typed `match_type`).
//! - [`repo`] — Repository-Abstraktion über `bracket_matches`/`group_matches`
//!   (kapselt das SQL, keine Tabellennamen als String).
//! - [`modes`] — Spielmodi/Hero-Assignment (+ Heldenliste aus [`turnier_draft`]).
//! - [`presets`] — Event-ConVar-Presets + Normalisierung.
//! - [`series`] — Best-of-N-Serien.
//! - [`steam_bridge`] — Task-Queue gegen die externe Bridge-DB.
//! - [`lobby`] — Lobby-Lebenszyklus (create/start/leave, Spectator/Ready, Invites).
//! - [`result`] — Ergebnisverarbeitung Bracket+Group VEREINHEITLICHT.
//! - [`auto_lobby`] — Auto-Lobby-Planung.
//!
//! ## Effekt-Schichten als injizierte Abhängigkeiten
//! Discord ([`turnier_discord::DiscordNotifier`]) und die Steam-Bridge
//! ([`steam_bridge::SteamBridge`]) sind im [`MatchManager`] als `Option`
//! gehalten: fehlt der Dienst (z. B. Bridge-DB nicht vorhanden), degradiert der
//! Flow sauber. Discord-Versand ist best-effort wie im Original — Fehler werden
//! geloggt, der Match-Flow läuft weiter.
//!
//! Funktional 1:1 zum Python-Original (`backend/match/*.py`). Die als
//! `behavior-change`/`needs-decision` markierten Befunde sind bewusst erhalten
//! (siehe Modul-Dokus + `bugs_preserved`).

pub mod auto_lobby;
pub mod core_adapter;
pub mod error;
pub mod kind;
pub mod lobby;
pub mod modes;
pub mod presets;
pub mod repo;
pub mod result;
pub mod series;
pub mod steam_bridge;

use turnier_config::Config;
use turnier_db::Pool;
use turnier_discord::DiscordNotifier;

// Flache Re-Exports der zentralen Typen.
pub use error::{MatchError, MatchResult, SteamTaskError, SteamTaskResult};
pub use kind::MatchKind;
pub use modes::{resolve_match_objective, ModeAssignments};
pub use presets::EventPreset;
pub use repo::{MatchRow, Participant};
pub use result::{ApplyBracketParams, ApplyGroupParams, ApplyResultOutcome};
pub use series::{GameResultOutcome, GameStats};
pub use steam_bridge::{BridgeError, InviteResult, SteamBridge, TaskOutcome};

/// Der zentrale Orchestrator des Match-Subsystems.
///
/// Hält den Haupt-DB-Pool, optional den Discord-Notifier und optional die
/// Steam-Bridge sowie die für den Flow benötigten Config-Werte. turnier-api (Welle 5)
/// und turnier-scheduler (Welle 4) bauen EINE Instanz und rufen die hier exponierten
/// Methoden.
pub struct MatchManager {
    pub(crate) pool: Pool,
    /// Discord-Notifier — `None`, wenn Discord-Effekte unterdrückt werden sollen.
    pub(crate) notifier: Option<DiscordNotifier>,
    /// Steam-Bridge — `None`, wenn die Bridge-DB fehlt (sauberes Degradieren).
    pub(crate) bridge: Option<steam_bridge::SteamBridge>,
    /// `DISCORD_CASTER_VOICE_CHANNEL_ID` für den Caster-Voice-Move beim Start.
    pub(crate) caster_voice_channel_id: i64,
    /// `DISCORD_GUILD_ID` als i64 (für den Voice-Move); 0, wenn nicht parsebar.
    pub(crate) guild_id: i64,
    /// `DISCORD_MATCH_CHANNEL_DELETE_DELAY_SECONDS` für das verzögerte Löschen.
    pub(crate) channel_delete_delay_seconds: i64,
    pub(crate) bridge_settings: turnier_config::BridgeConfig,
}

impl MatchManager {
    /// Baut den Manager aus seinen Bausteinen.
    pub fn new(
        pool: Pool,
        notifier: Option<DiscordNotifier>,
        bridge: Option<steam_bridge::SteamBridge>,
        config: &Config,
    ) -> Self {
        Self {
            pool,
            notifier,
            bridge,
            caster_voice_channel_id: config.discord_caster_voice_channel_id,
            guild_id: config.discord_guild_id.parse().unwrap_or(0),
            channel_delete_delay_seconds: config.discord_match_channel_delete_delay_seconds,
            bridge_settings: config.bridge.clone(),
        }
    }

    /// Zugriff auf den Haupt-Pool (z. B. für Aufrufer, die direkt lesen müssen).
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Liefert die Steam-Bridge oder einen Fehler, wenn sie nicht verfügbar ist.
    pub(crate) fn require_bridge(&self) -> SteamTaskResult<&steam_bridge::SteamBridge> {
        self.bridge.as_ref().ok_or_else(|| {
            SteamTaskError::failed(
                "Steam-Bridge nicht verfügbar (STEAM_BRIDGE_DB_PATH nicht konfiguriert)",
            )
        })
    }
}
