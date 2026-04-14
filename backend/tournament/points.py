"""Punkte-Berechnung fuer globale Rangliste."""
from __future__ import annotations

from datetime import datetime, timezone

import aiosqlite

PLACEMENT_POINTS = {1: 10, 2: 6, 3: 3, 4: 3}
PARTICIPATION_POINTS = 1
WIN_POINTS_PER_MATCH = 0.5


async def recalculate_player_points(
    db: aiosqlite.Connection,
    tournament_id: int,
) -> None:
    """Berechnet Punkte fuer alle Teilnehmer eines abgeschlossenen Turniers neu."""
    cursor = await db.execute(
        "SELECT tm.discord_id, t.id as team_id FROM team_members tm "
        "JOIN teams t ON tm.team_id = t.id WHERE t.tournament_id = ?",
        (tournament_id,),
    )
    participants = await cursor.fetchall()

    cursor = await db.execute(
        "SELECT team1_id, team2_id, winner_id, round FROM bracket_matches "
        "WHERE tournament_id = ? AND status = 'completed' ORDER BY round DESC",
        (tournament_id,),
    )
    bracket_matches = await cursor.fetchall()

    team_placements: dict[int, int] = {}
    if bracket_matches:
        max_round = max(match["round"] for match in bracket_matches)
        final = next(
            (match for match in bracket_matches if match["round"] == max_round),
            None,
        )
        if final and final["winner_id"]:
            loser_id = (
                final["team1_id"]
                if final["winner_id"] == final["team2_id"]
                else final["team2_id"]
            )
            team_placements[final["winner_id"]] = 1
            if loser_id:
                team_placements[loser_id] = 2
            semifinals = [
                match for match in bracket_matches if match["round"] == max_round - 1
            ]
            for semifinal in semifinals:
                if semifinal["winner_id"]:
                    semifinal_loser = (
                        semifinal["team1_id"]
                        if semifinal["winner_id"] == semifinal["team2_id"]
                        else semifinal["team2_id"]
                    )
                    if semifinal_loser and semifinal_loser not in team_placements:
                        team_placements[semifinal_loser] = 3

    cursor = await db.execute(
        "SELECT bm.winner_id, bm.team1_id, bm.team2_id "
        "FROM bracket_matches bm WHERE bm.tournament_id = ? AND bm.status = 'completed'",
        (tournament_id,),
    )
    bm_rows = await cursor.fetchall()
    team_wins: dict[int, int] = {}
    for bracket_match in bm_rows:
        if bracket_match["winner_id"]:
            winner_id = bracket_match["winner_id"]
            team_wins[winner_id] = team_wins.get(winner_id, 0) + 1

    now = datetime.now(timezone.utc).isoformat()
    for participant in participants:
        discord_id = participant["discord_id"]
        team_id = participant["team_id"]
        placement = team_placements.get(team_id)
        wins = team_wins.get(team_id, 0)

        pts = PARTICIPATION_POINTS
        pts += PLACEMENT_POINTS.get(placement, 0) if placement else 0
        pts += int(wins * WIN_POINTS_PER_MATCH)

        cursor = await db.execute(
            "SELECT * FROM player_points WHERE discord_id = ?",
            (discord_id,),
        )
        existing = await cursor.fetchone()

        if existing:
            new_total = existing["total_points"] + pts
            new_tournaments = existing["tournaments_played"] + 1
            new_matches = existing["matches_played"] + len(bm_rows)
            new_wins = existing["matches_won"] + wins
            new_best = (
                min(placement, existing["best_placement"])
                if placement and existing["best_placement"]
                else (placement or existing["best_placement"])
            )
            await db.execute(
                "UPDATE player_points SET total_points=?, tournaments_played=?, "
                "matches_played=?, matches_won=?, best_placement=?, updated_at=? "
                "WHERE discord_id=?",
                (
                    new_total,
                    new_tournaments,
                    new_matches,
                    new_wins,
                    new_best,
                    now,
                    discord_id,
                ),
            )
        else:
            await db.execute(
                "INSERT INTO player_points (discord_id, total_points, tournaments_played, matches_played, matches_won, best_placement, updated_at) "
                "VALUES (?, ?, ?, ?, ?, ?, ?)",
                (discord_id, pts, 1, len(bm_rows), wins, placement, now),
            )
