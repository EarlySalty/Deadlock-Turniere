//! Scrim domain foundation. Pure decisions are separated from repositories and effects.

pub mod decision;
pub mod dto;
pub mod error;
pub mod model;
pub mod repository;
pub mod service;

pub use error::{ScrimError, ScrimResult};
