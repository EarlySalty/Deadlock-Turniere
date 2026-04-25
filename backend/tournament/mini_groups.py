from __future__ import annotations

import json
from typing import Any

from tournament.engine import _propagate_resolved_entry


def _extract_point_diff(match_row: dict[str, Any], team_id: int) -> int:
    raw_stats = match_row.get("match_stats")
    if not raw_stats:
        winner_id = match_row.get("winner_id")
        if winner_id is None:
            return 0
        return 1 if int(winner_id) == team_id else -1

    try:
        parsed = raw_stats if isinstance(raw_stats, dict) else json.loads(str(raw_stats))
    except (TypeError, ValueError):
        parsed = None

    if not isinstance(parsed, dict):
        winner_id = match_row.get("winner_id")
        if winner_id is None:
            return 0
        return 1 if int(winner_id) == team_id else -1

    score_pairs = (
        ("team1_score", "team2_score"),
        ("score_team1", "score_team2"),
        ("team1_points", "team2_points"),
        ("points_team1", "points_team2"),
    )
    for key1, key2 in score_pairs:
        if key1 in parsed and key2 in parsed:
            try:
                team1_score = int(parsed[key1])
                team2_score = int(parsed[key2])
            except (TypeError, ValueError):
                break
            if int(match_row["team1_id"]) == team_id:
                return team1_score - team2_score
            return team2_score - team1_score

    nested_score = parsed.get("score")
    if isinstance(nested_score, dict):
        try:
            team1_score = int(nested_score.get("team1"))
            team2_score = int(nested_score.get("team2"))
        except (TypeError, ValueError):
            team1_score = None
            team2_score = None
        if team1_score is not None and team2_score is not None:
            if int(match_row["team1_id"]) == team_id:
                return team1_score - team2_score
            return team2_score - team1_score

    winner_id = match_row.get("winner_id")
    if winner_id is None:
        return 0
    return 1 if int(winner_id) == team_id else -1


def _head_to_head_winner(
    head_to_head: dict[tuple[int, int], int],
    left_team_id: int,
    right_team_id: int,
) -> int | None:
    winner = head_to_head.get((left_team_id, right_team_id))
    if winner == left_team_id:
        return left_team_id
    if winner == right_team_id:
        return right_team_id
    return None


def _break_two_way_tie(
    team_ids: list[int],
    *,
    wins: dict[int, int],
    point_diff: dict[int, int],
    seed_order: dict[int, int],
    head_to_head: dict[tuple[int, int], int],
) -> int:
    left_team_id, right_team_id = team_ids
    direct_winner = _head_to_head_winner(head_to_head, left_team_id, right_team_id)
    if direct_winner is not None:
        return direct_winner
    if point_diff[left_team_id] != point_diff[right_team_id]:
        return left_team_id if point_diff[left_team_id] > point_diff[right_team_id] else right_team_id
    if seed_order[left_team_id] != seed_order[right_team_id]:
        return left_team_id if seed_order[left_team_id] < seed_order[right_team_id] else right_team_id
    return min(left_team_id, right_team_id)


def _select_mini_group_winner(
    team_ids: list[int],
    *,
    wins: dict[int, int],
    point_diff: dict[int, int],
    seed_order: dict[int, int],
    head_to_head: dict[tuple[int, int], int],
) -> int:
    max_wins = max(wins[team_id] for team_id in team_ids)
    tied_team_ids = [team_id for team_id in team_ids if wins[team_id] == max_wins]
    if len(tied_team_ids) == 1:
        return tied_team_ids[0]
    if len(tied_team_ids) == 2:
        return _break_two_way_tie(
            tied_team_ids,
            wins=wins,
            point_diff=point_diff,
            seed_order=seed_order,
            head_to_head=head_to_head,
        )

    mini_wins = {team_id: 0 for team_id in tied_team_ids}
    for team_id in tied_team_ids:
        for opponent_id in tied_team_ids:
            if team_id == opponent_id:
                continue
            if _head_to_head_winner(head_to_head, team_id, opponent_id) == team_id:
                mini_wins[team_id] += 1
    max_mini_wins = max(mini_wins.values())
    narrowed_team_ids = [
        team_id for team_id in tied_team_ids if mini_wins[team_id] == max_mini_wins
    ]
    if len(narrowed_team_ids) == 1:
        return narrowed_team_ids[0]
    if len(narrowed_team_ids) == 2:
        return _break_two_way_tie(
            narrowed_team_ids,
            wins=wins,
            point_diff=point_diff,
            seed_order=seed_order,
            head_to_head=head_to_head,
        )

    best_point_diff = max(point_diff[team_id] for team_id in narrowed_team_ids)
    point_diff_winners = [
        team_id for team_id in narrowed_team_ids if point_diff[team_id] == best_point_diff
    ]
    if len(point_diff_winners) == 1:
        return point_diff_winners[0]
    return min(point_diff_winners, key=lambda team_id: (seed_order[team_id], team_id))


async def complete_mini_group_round_robin(db, mini_group_id: int) -> int | None:  # noqa: ANN001
    cursor = await db.execute(
        """
        SELECT id, tournament_id, advances_to_match_id, advances_to_slot
        FROM bracket_mini_groups
        WHERE id = ?
        """,
        (mini_group_id,),
    )
    mini_group = await cursor.fetchone()
    if not mini_group:
        return None

    cursor = await db.execute(
        """
        SELECT team_id, seed_order
        FROM bracket_mini_group_teams
        WHERE mini_group_id = ? AND team_id IS NOT NULL
        ORDER BY seed_order, id
        """,
        (mini_group_id,),
    )
    team_rows = await cursor.fetchall()
    if len(team_rows) < 2:
        return None

    team_ids = [int(row["team_id"]) for row in team_rows]
    seed_order = {int(row["team_id"]): int(row["seed_order"]) for row in team_rows}

    cursor = await db.execute(
        """
        SELECT id, team1_id, team2_id, winner_id, status, match_stats
        FROM bracket_matches
        WHERE mini_group_id = ?
        ORDER BY round, position, id
        """,
        (mini_group_id,),
    )
    match_rows = [dict(row) for row in await cursor.fetchall()]
    if not match_rows or any(row["status"] != "completed" or row["winner_id"] is None for row in match_rows):
        return None

    wins = {team_id: 0 for team_id in team_ids}
    point_diff = {team_id: 0 for team_id in team_ids}
    head_to_head: dict[tuple[int, int], int] = {}

    for match_row in match_rows:
        team1_id = int(match_row["team1_id"])
        team2_id = int(match_row["team2_id"])
        winner_id = int(match_row["winner_id"])
        wins[winner_id] += 1
        point_diff[team1_id] += _extract_point_diff(match_row, team1_id)
        point_diff[team2_id] += _extract_point_diff(match_row, team2_id)
        head_to_head[(team1_id, team2_id)] = winner_id
        head_to_head[(team2_id, team1_id)] = winner_id

    winner_team_id = _select_mini_group_winner(
        team_ids,
        wins=wins,
        point_diff=point_diff,
        seed_order=seed_order,
        head_to_head=head_to_head,
    )

    if mini_group["advances_to_match_id"] is not None and mini_group["advances_to_slot"] in (1, 2):
        target_column = "team1_id" if int(mini_group["advances_to_slot"]) == 1 else "team2_id"
        await db.execute(
            f"UPDATE bracket_matches SET {target_column} = ? WHERE id = ?",  # noqa: S608
            (winner_team_id, mini_group["advances_to_match_id"]),
        )

    await _propagate_resolved_entry(
        db,
        winner_id=winner_team_id,
        source_mini_group_id=int(mini_group_id),
    )
    return winner_team_id
