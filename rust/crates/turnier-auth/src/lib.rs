//! `turnier-auth` — Auth-Logik unterhalb der HTTP-Schicht.
//!
//! Diese Crate bündelt drei Zuständigkeiten des delegierten Discord-Auth-Flows:
//!
//! - **RBAC** ([`roles`]): leitet `is_admin`/`is_mod` aus den Discord-Rollen
//!   gegen die einmal geparsten Config-Rollen-Mengen ab (Modell: User < Mod <
//!   Admin; Admin impliziert Mod).
//! - **Sessions** ([`session`]): erzeugt opake Zufalls-Tokens (KEIN JWT),
//!   persistiert sie in der Tabelle `sessions` und löst ein Token zur
//!   [`turnier_core::UserSession`] auf (mit Ablaufprüfung + Opportunistic-Cleanup).
//! - **OAuth-Client** ([`oauth`]): spricht den internen Master-Broker an
//!   (`initiate`/`consume-result`) und liefert die Discord-Identität.
//!
//! Die HTTP-Routen (`/auth/discord/login|complete|logout`) und der axum-Extractor
//! `get_current_user` leben in turnier-api und bauen auf diesen Bausteinen auf.

pub mod error;
pub mod oauth;
pub mod roles;
pub mod session;

pub use error::{AuthError, AuthResult};
pub use oauth::{DiscordIdentity, OAuthClient};
pub use roles::{RoleFlags, RoleSets};
pub use session::{
    cleanup_expired, create_session, create_session_with_lifetime, delete_session, generate_token,
    resolve_session, SESSION_LIFETIME_DAYS,
};
