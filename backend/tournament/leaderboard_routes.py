"""Oeffentliche Ranglisten- und Spieler-Profil Routes."""
from __future__ import annotations

from fastapi import APIRouter, HTTPException

from db import get_db
from tournament.models import LeaderboardEntry, PlayerProfile, TournamentHistoryEntry

router = APIRouter(prefix="/api", tags=["leaderboard"])


@router.get("/leaderboard")
async def get_leaderboard() -> list[LeaderboardEntry]:
    """Globale Rangliste aller Spieler nach Punkte."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT pp.total_points, pp.tournaments_played, "
            "pp.matches_played, pp.matches_won, pp.best_placement, "
            "COALESCE(NULLIF(s.discord_name, ''), NULL) AS discord_name, "
            "rc.rank "
            "FROM player_points pp "
            "LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM sessions "
            "WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s "
            "ON s.discord_id = pp.discord_id "
            "LEFT JOIN rank_cache rc ON rc.discord_id = pp.discord_id "
            "ORDER BY pp.total_points DESC, pp.best_placement ASC NULLS LAST"
        )
        rows = await cursor.fetchall()
    return [
        LeaderboardEntry(
            rank_position=index + 1,
            discord_name=row["discord_name"] or "Unbekannt",
            rank=row["rank"],
            total_points=row["total_points"],
            tournaments_played=row["tournaments_played"],
            matches_played=row["matches_played"],
            matches_won=row["matches_won"],
            best_placement=row["best_placement"],
        )
        for index, row in enumerate(rows)
    ]


@router.get("/players/{discord_name}")
async def get_player_profile(discord_name: str) -> PlayerProfile:
    """Oeffentliches Spieler-Profil (kein Login noetig)."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT discord_id FROM sessions WHERE discord_name = ? LIMIT 1",
            (discord_name,),
        )
        session_row = await cursor.fetchone()
        if not session_row:
            raise HTTPException(status_code=404, detail="Spieler nicht gefunden")
        discord_id = session_row["discord_id"]

        cursor = await db.execute(
            "SELECT discord_avatar FROM sessions WHERE discord_id = ? AND discord_avatar IS NOT NULL LIMIT 1",
            (discord_id,),
        )
        avatar_row = await cursor.fetchone()
        discord_avatar = avatar_row["discord_avatar"] if avatar_row else None

        cursor = await db.execute(
            "SELECT display_name, bio, avatar_filename FROM user_profiles WHERE discord_id = ?",
            (discord_id,),
        )
        profile_row = await cursor.fetchone()
        display_name = profile_row["display_name"] if profile_row else None
        bio = profile_row["bio"] if profile_row else None
        avatar_filename = profile_row["avatar_filename"] if profile_row else None

        cursor = await db.execute(
            "SELECT rank FROM rank_cache WHERE discord_id = ?",
            (discord_id,),
        )
        rank_row = await cursor.fetchone()
        rank = rank_row["rank"] if rank_row else None

        cursor = await db.execute(
            "SELECT * FROM player_points WHERE discord_id = ?",
            (discord_id,),
        )
        points_row = await cursor.fetchone()

        cursor = await db.execute(
            "SELECT t.name AS tournament_name, teams.name AS team_name "
            "FROM team_members tm "
            "JOIN teams ON tm.team_id = teams.id "
            "JOIN tournaments t ON teams.tournament_id = t.id "
            "WHERE tm.discord_id = ? ORDER BY t.created_at DESC",
            (discord_id,),
        )
        history_rows = await cursor.fetchall()
        tournament_history = [
            TournamentHistoryEntry(
                tournament_name=row["tournament_name"],
                team_name=row["team_name"],
            )
            for row in history_rows
        ]

    return PlayerProfile(
        discord_name=discord_name,
        display_name=display_name or discord_name,
        discord_avatar=discord_avatar,
        avatar_filename=avatar_filename,
        bio=bio,
        rank=rank,
        rank_score=points_row["matches_won"] if points_row else 0,
        tournaments_played=points_row["tournaments_played"] if points_row else 0,
        matches_played=points_row["matches_played"] if points_row else 0,
        matches_won=points_row["matches_won"] if points_row else 0,
        best_placement=points_row["best_placement"] if points_row else None,
        total_points=points_row["total_points"] if points_row else 0,
        tournament_history=tournament_history,
    )
