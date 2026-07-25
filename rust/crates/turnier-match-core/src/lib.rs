//! Storage-independent result mappings shared by match adapters.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamSlot {
    Team0,
    Team1,
}

impl TeamSlot {
    pub const fn index(self) -> usize {
        match self {
            Self::Team0 => 0,
            Self::Team1 => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTeams<T> {
    pub team0: T,
    pub team1: T,
}

impl<T: Eq> CanonicalTeams<T> {
    pub fn new(team0: T, team1: T) -> Self {
        Self { team0, team1 }
    }

    pub fn slot_of(&self, team_id: &T) -> Option<TeamSlot> {
        if team_id == &self.team0 {
            Some(TeamSlot::Team0)
        } else if team_id == &self.team1 {
            Some(TeamSlot::Team1)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Winner<T> {
    Slot(TeamSlot),
    TeamId(T),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finality {
    Provisional,
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalResult<T> {
    pub winner_id: T,
    pub winner_slot: TeamSlot,
    pub finality: Finality,
}

pub fn resolve_result<T: Clone + Eq>(
    teams: &CanonicalTeams<T>,
    winner: Winner<T>,
) -> Result<CanonicalResult<T>, MatchCoreError> {
    let (winner_id, winner_slot) = match winner {
        Winner::Slot(TeamSlot::Team0) => (teams.team0.clone(), TeamSlot::Team0),
        Winner::Slot(TeamSlot::Team1) => (teams.team1.clone(), TeamSlot::Team1),
        Winner::TeamId(team_id) => {
            let slot = teams
                .slot_of(&team_id)
                .ok_or(MatchCoreError::UnknownWinner)?;
            (team_id, slot)
        }
    };
    Ok(CanonicalResult {
        winner_id,
        winner_slot,
        finality: Finality::Final,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MatchCoreError {
    #[error("winner is not a participant")]
    UnknownWinner,
}
