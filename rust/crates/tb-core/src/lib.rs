//! `tb-core` — die reine Domänenschicht des Turnier-Backends.
//!
//! Enthält ausschließlich Typen: Enums (das frühere Magic-String-Vokabular) und
//! Wire-DTOs (1:1 zu den Pydantic-Modellen). Kein I/O, keine Geschäftslogik,
//! keine Persistenz. Jede andere Crate baut auf diesen Verträgen auf.

pub mod bracket;
pub mod enums;
pub mod group;
pub mod json;
pub mod rank;
pub mod result;
pub mod team;
pub mod tournament;
pub mod user;

// Flache Re-Exports, damit Konsumenten `tb_core::Tournament` statt
// `tb_core::tournament::Tournament` schreiben können.
pub use bracket::{BracketMatch, BracketMiniGroup, MatchGame};
pub use enums::*;
pub use group::{Group, GroupMatch, GroupTeam};
pub use rank::RankProfile;
pub use result::{CheckIn, MatchResult, MatchResultReport, MatchResultReportCreate};
pub use team::{
    Team, TeamApplication, TeamCreate, TeamInvitation, TeamMember, TeamMemberPublic, TeamPublic,
};
pub use tournament::{
    Tournament, TournamentCreate, TournamentDetail, TournamentDetailPublic, TournamentSignup,
    TournamentSignupPublic, TournamentUpdate,
};
pub use user::{
    ConsentCreate, ConsentStatus, LeaderboardEntry, PlayerProfile, TournamentHistoryEntry,
    UserProfile, UserProfileUpdate, UserSession,
};
