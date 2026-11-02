use std::collections::BTreeSet;

use turnier_core::{Group, GroupMatch, MatchStatus};
use turnier_db::Pool;
use turnier_engine::league::{league_table, LeagueGame};
use turnier_match::league::tournament_is_league;

use crate::error::WebResult;

pub async fn apply_league_view(
    pool: &Pool,
    tournament_id: i64,
    groups: &mut [Group],
) -> WebResult<()> {
    if tournament_is_league(pool, tournament_id).await? {
        groups.iter_mut().for_each(arrange_league_group);
    }
    Ok(())
}

fn arrange_league_group(group: &mut Group) {
    assign_rounds(&mut group.matches);
    group.teams.sort_by_key(|team| team.id);
    let team_ids: Vec<i64> = group.teams.iter().map(|team| team.team_id).collect();
    let games: Vec<LeagueGame> = group.matches.iter().filter_map(completed_game).collect();
    let order: Vec<i64> = league_table(&team_ids, &games)
        .into_iter()
        .map(|row| row.team_id)
        .collect();
    group.teams.sort_by_key(|team| {
        order
            .iter()
            .position(|team_id| *team_id == team.team_id)
            .unwrap_or(order.len())
    });
}

fn assign_rounds(matches: &mut [GroupMatch]) {
    matches.sort_by(|a, b| a.scheduled_at.cmp(&b.scheduled_at).then(a.id.cmp(&b.id)));
    let times: BTreeSet<Option<String>> = matches.iter().map(|m| m.scheduled_at.clone()).collect();
    for game in matches.iter_mut() {
        game.round = times
            .iter()
            .position(|time| *time == game.scheduled_at)
            .map(|index| index as i64 + 1);
    }
}

fn completed_game(game: &GroupMatch) -> Option<LeagueGame> {
    (game.status == MatchStatus::Completed).then_some(LeagueGame {
        team1_id: game.team1_id,
        team2_id: game.team2_id,
        winner_id: game.winner_id,
    })
}
