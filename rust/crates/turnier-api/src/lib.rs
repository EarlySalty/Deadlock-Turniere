//! `turnier-api` — die HTTP-Schicht (axum) des Turnier-Backends.
//!
//! Verdrahtet die Domänen-Crates zu einer API: [`AppState`] hält die Handles,
//! [`build_router`] baut den Router aus den Router-Modulen, die Extractoren
//! ([`AuthUser`]/[`ModUser`]/[`AdminUser`]) setzen die Auth-Gates durch und
//! [`WebError`] übersetzt Domänenfehler in FastAPI-kompatible Responses.
//!
//! Diese Crate enthält keine Geschäftslogik — sie liest Requests, ruft die
//! Domänen-Crates und formt Responses.

pub mod app;
pub mod error;
pub mod extract;
pub mod state;

// Router-Module (je eine Datei/Ordner — getrennt befüllbar).
pub mod admin;
pub mod auth;
pub mod consent;
pub mod draft;
pub mod leaderboard;
pub mod operations;
pub mod public;
pub mod test_mode;

pub use app::build_router;
pub use error::{WebError, WebResult};
pub use extract::{AdminUser, AuthUser, ModUser, OptionalUser};
pub use state::{AppState, BuildError};
