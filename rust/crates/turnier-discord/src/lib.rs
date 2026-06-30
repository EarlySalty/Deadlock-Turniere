//! `turnier-discord` — Broker-Client-Schicht zwischen Turnier-Backend und dem
//! zentralen "Discord-Master-Broker" (separater interner HTTP-Dienst).
//!
//! Kapselt alle Discord-seitigen Effekte (Match-Channel anlegen/löschen,
//! Lobby-/Stats-Embeds posten, DMs an Spieler/Caster, User in Voice-Kanäle
//! ziehen, Rollen-/Voice-Mitglieder abfragen). Versand erfolgt nie direkt an die
//! Discord-API, sondern als POST an interne Broker-Endpunkte mit
//! `X-Internal-Token`. Ein Teil der Operationen wird zusätzlich in der Tabelle
//! `discord_tasks` als Lifecycle-Job protokolliert; DM-Versand respektiert die
//! Opt-out-Flags aus `user_profiles` und `tournament_dm_optout`.
//!
//! Diese Crate ist BEWUSST ENTKOPPELT von axum/FastAPI: kein `HTTPException`,
//! sondern ein eigener [`error::BrokerError`]. Sie portiert
//! `backend/notifications/discord_notifier.py` 1:1 im Verhalten.

pub mod broker;
pub mod channel_name;
pub mod embed;
pub mod error;
pub mod event;
pub mod ids;
pub mod notifier;
pub mod tasks;

// Flache Re-Exports der zentralen Typen.
pub use broker::BrokerClient;
pub use embed::{Embed, Field};
pub use error::{BrokerError, BrokerResult};
pub use event::{NotificationEvent, NOTIFY_DM_DEFAULT};
pub use notifier::{
    CasterNotifyResult, DiscordNotifier, FailedId, MoveVoiceResult, NotifyUsersResult, PlayerStat,
};
