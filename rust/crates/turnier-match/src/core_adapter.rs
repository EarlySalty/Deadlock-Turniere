//! Legacy Bracket/Group conventions translated to the pure match core.

use turnier_match_core::{resolve_result, CanonicalResult, CanonicalTeams, TeamSlot, Winner};

use crate::error::{MatchError, MatchResult};

/// Translates the existing Bracket convention (`0 = team1`, `1 = team2`) into
/// the canonical team0/team1 result.
pub fn resolve_bracket_result(
    team1_id: i64,
    team2_id: i64,
    winning_team: Option<i64>,
    winner_id: Option<i64>,
) -> MatchResult<CanonicalResult<i64>> {
    if winning_team.is_none() && winner_id.is_none() {
        return Err(MatchError::invalid(
            "winner_id oder winning_team ist erforderlich",
        ));
    }
    let teams = canonical_teams(team1_id, team2_id);
    let slot = winning_team.map(bracket_slot).transpose()?;
    let resolved = match (winner_id, slot) {
        (Some(winner_id), _) => resolve_result(&teams, Winner::TeamId(winner_id)),
        (None, Some(slot)) => resolve_result(&teams, Winner::Slot(slot)),
        (None, None) => {
            return Err(MatchError::invalid(
                "winner_id oder winning_team ist erforderlich",
            ))
        }
    }
    .map_err(|_| {
        MatchError::invalid(format!(
            "winner_id {} gehört nicht zu diesem Match",
            winner_id.unwrap_or_default()
        ))
    })?;

    if let Some(slot) = slot {
        if slot != resolved.winner_slot {
            return Err(MatchError::invalid(
                "winning_team passt nicht zum übergebenen winner_id",
            ));
        }
    }
    Ok(resolved)
}

/// Translates the existing Group convention (`1 = team1`, `2 = team2`) into
/// the canonical result. Existing behavior is retained: an explicit winner ID
/// takes precedence over a simultaneously supplied slot.
pub fn resolve_group_result(
    team1_id: i64,
    team2_id: i64,
    winning_team: Option<i64>,
    winner_id: Option<i64>,
) -> MatchResult<CanonicalResult<i64>> {
    let teams = canonical_teams(team1_id, team2_id);
    if team1_id == team2_id {
        let winner_id = winner_id.or(match winning_team {
            Some(1 | 2) => Some(team1_id),
            _ => None,
        });
        return winner_id
            .filter(|winner| *winner == team1_id)
            .ok_or_else(|| {
                MatchError::invalid("winner_id muss eines der beiden Teams im Match sein")
            })
            .and_then(|winner| {
                resolve_result(&teams, Winner::TeamId(winner)).map_err(|_| {
                    MatchError::invalid("winner_id muss eines der beiden Teams im Match sein")
                })
            });
    }
    let winner = match winner_id {
        Some(winner_id) => Winner::TeamId(winner_id),
        None => Winner::Slot(group_slot(winning_team.ok_or_else(|| {
            MatchError::invalid("winner_id muss eines der beiden Teams im Match sein")
        })?)?),
    };
    resolve_result(&teams, winner)
        .map_err(|_| MatchError::invalid("winner_id muss eines der beiden Teams im Match sein"))
}

pub const fn bracket_winning_team(slot: TeamSlot) -> i64 {
    slot.index() as i64
}

pub const fn group_winning_team(slot: TeamSlot) -> i64 {
    slot.index() as i64 + 1
}

fn canonical_teams(team1_id: i64, team2_id: i64) -> CanonicalTeams<i64> {
    CanonicalTeams::new(team1_id, team2_id)
}

fn bracket_slot(value: i64) -> MatchResult<TeamSlot> {
    match value {
        0 => Ok(TeamSlot::Team0),
        1 => Ok(TeamSlot::Team1),
        other => Err(MatchError::invalid(format!(
            "Ungültiger winning_team-Wert: {other}"
        ))),
    }
}

fn group_slot(value: i64) -> MatchResult<TeamSlot> {
    match value {
        1 => Ok(TeamSlot::Team0),
        2 => Ok(TeamSlot::Team1),
        _ => Err(MatchError::invalid(
            "winner_id muss eines der beiden Teams im Match sein",
        )),
    }
}
