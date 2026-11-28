use std::collections::BTreeSet;

use turnier_core::{Group, GroupMatch, MatchStatus};
use turnier_db::Pool;
use turnier_engine::league::LeagueGame;
use turnier_engine::standings::{rank_standings, Standing};
use turnier_match::league::tournament_is_league;

use crate::error::WebResult;

pub async fn apply_league_view<L>(
    pool: &Pool,
    tournament_id: i64,
    groups: &mut [Group<L>],
) -> WebResult<()> {
    let league = tournament_is_league(pool, tournament_id).await?;
    for group in groups.iter_mut() {
        if league {
            assign_rounds(&mut group.matches);
        }
        rank_group_teams(group);
    }
    Ok(())
}

fn rank_group_teams<L>(group: &mut Group<L>) {
    group.teams.sort_by_key(|team| team.id);
    let standings: Vec<Standing> = group
        .teams
        .iter()
        .map(|team| Standing {
            team_id: team.team_id,
            wins: team.wins,
            points: team.points,
        })
        .collect();
    let games: Vec<LeagueGame> = group.matches.iter().filter_map(completed_game).collect();
    let ranked = rank_standings(&standings, &games);
    group.teams.sort_by_key(|team| {
        ranked
            .iter()
            .position(|row| row.team_id == team.team_id)
            .unwrap_or(ranked.len())
    });
    for team in group.teams.iter_mut() {
        team.seed_tiebreak = ranked
            .iter()
            .any(|row| row.team_id == team.team_id && row.seed_tiebreak);
    }
}

fn assign_rounds<L>(matches: &mut [GroupMatch<L>]) {
    matches.sort_by(|a, b| a.scheduled_at.cmp(&b.scheduled_at).then(a.id.cmp(&b.id)));
    let times: BTreeSet<Option<String>> = matches.iter().map(|m| m.scheduled_at.clone()).collect();
    for game in matches.iter_mut() {
        game.round = times
            .iter()
            .position(|time| *time == game.scheduled_at)
            .map(|index| index as i64 + 1);
    }
}

fn completed_game<L>(game: &GroupMatch<L>) -> Option<LeagueGame> {
    (game.status == MatchStatus::Completed).then_some(LeagueGame {
        team1_id: game.team1_id,
        team2_id: game.team2_id,
        winner_id: game.winner_id,
    })
}
