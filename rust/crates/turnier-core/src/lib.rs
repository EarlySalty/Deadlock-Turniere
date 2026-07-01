//! `turnier-core` — die reine Domänenschicht des Turnier-Backends.
//!
//! Enthält Enums (das frühere Magic-String-Vokabular), Wire-DTOs (1:1 zu den
//! Pydantic-Modellen) und kleine Grenzkonventions-Helfer. Keine Geschäftslogik,
//! keine Persistenz. Jede andere Crate baut auf diesen Verträgen auf.

pub mod bracket;
pub mod enums;
pub mod group;
pub mod ids;
pub mod json;
pub mod rank;
pub mod result;
pub mod team;
pub mod time;
pub mod tournament;
pub mod user;

// Flache Re-Exports, damit Konsumenten `turnier_core::Tournament` statt
// `turnier_core::tournament::Tournament` schreiben können.
pub use bracket::{BracketMatch, BracketMiniGroup, MatchGame};
pub use enums::*;
pub use group::{Group, GroupMatch, GroupTeam};
pub use ids::{discord_id_to_string, parse_discord_id, DiscordIdParseError};
pub use rank::RankProfile;
pub use result::{CheckIn, MatchResult, MatchResultReport, MatchResultReportCreate};
pub use team::{
    Team, TeamApplication, TeamCreate, TeamInvitation, TeamMember, TeamMemberPublic, TeamPublic,
};
pub use time::now_utc;
pub use tournament::{
    Patch, Tournament, TournamentCreate, TournamentDetail, TournamentDetailPublic,
    TournamentSignup, TournamentSignupPublic, TournamentUpdate,
};
pub use user::{
    ConsentCreate, ConsentStatus, LeaderboardEntry, PlayerProfile, TournamentHistoryEntry,
    UserProfile, UserProfileUpdate, UserSession,
};
