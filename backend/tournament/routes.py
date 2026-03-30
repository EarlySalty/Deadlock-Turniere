"""Öffentliche und authentifizierte Tournament + Team Routes."""
from __future__ import annotations

import json
import re

from fastapi import APIRouter, Depends, HTTPException, status

from auth.permissions import require_auth
from db import get_db
from rank_reader import get_player_rank_profile
from tournament.models import (
    BracketMatch,
    Group,
    GroupMatch,
    GroupTeam,
    Team,
    TeamMember,
    Tournament,
    TournamentDetail,
    TournamentSignup,
    UserSession,
)

router = APIRouter(prefix="/api", tags=["tournaments"])


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

async def _enrich_rank_data(row: dict) -> dict:  # noqa: ANN001
    """Ergänzt Rangdaten mit Discord-first und DB-Fallback."""
    profile = await get_player_rank_profile(str(row["discord_id"]))
    if not profile:
        return row

    enriched = dict(row)
    if profile.get("steam_id"):
        enriched["steam_id"] = profile.get("steam_id")
    if profile.get("rank"):
        enriched["rank"] = profile.get("rank")
    if profile.get("rank_score") is not None:
        enriched["rank_score"] = profile.get("rank_score") or 0
    return enriched


async def _audit(db, action: str, user_id: str | None, details: str) -> None:  # noqa: ANN001
    await db.execute(
        "INSERT INTO audit_log (action, user_id, details) VALUES (?, ?, ?)",
        (action, user_id, details),
    )

async def _load_teams_for_tournament(db, tournament_id: int) -> list[Team]:  # noqa: ANN001
    """Lädt alle Teams eines Turniers inkl. Members."""
    cursor = await db.execute(
        "SELECT * FROM teams WHERE tournament_id = ?",
        (tournament_id,),
    )
    team_rows = await cursor.fetchall()
    teams: list[Team] = []
    for t in team_rows:
        cursor = await db.execute(
            "SELECT tm.id, tm.team_id, tm.discord_id, "
            "COALESCE(NULLIF(tm.discord_name, ''), NULLIF(s.discord_name, '')) AS discord_name, "
            "tm.steam_id, tm.rank, tm.rank_score, tm.role, tm.joined_at "
            "FROM team_members tm "
            "LEFT JOIN ("
            "    SELECT discord_id, MAX(discord_name) AS discord_name "
            "    FROM sessions "
            "    WHERE discord_name IS NOT NULL AND discord_name != '' "
            "    GROUP BY discord_id"
            ") s ON s.discord_id = tm.discord_id "
            "WHERE tm.team_id = ? "
            "ORDER BY tm.joined_at",
            (t["id"],),
        )
        member_rows = await cursor.fetchall()
        members = [TeamMember(**await _enrich_rank_data(dict(m))) for m in member_rows]
        teams.append(Team(**{**dict(t), "members": members}))
    return teams


async def _load_groups_for_tournament(db, tournament_id: int) -> list[Group]:  # noqa: ANN001
    """Lädt alle Gruppen eines Turniers inkl. Teams und Matches."""
    cursor = await db.execute(
        "SELECT * FROM groups WHERE tournament_id = ? ORDER BY seeding_order",
        (tournament_id,),
    )
    group_rows = await cursor.fetchall()
    groups: list[Group] = []
    for g in group_rows:
        # Group Teams
        cursor = await db.execute(
            "SELECT gt.*, t.name as team_name FROM group_teams gt "
            "JOIN teams t ON gt.team_id = t.id WHERE gt.group_id = ?",
            (g["id"],),
        )
        gt_rows = await cursor.fetchall()
        group_teams = [GroupTeam(**dict(gt)) for gt in gt_rows]

        # Group Matches
        cursor = await db.execute(
            "SELECT * FROM group_matches WHERE group_id = ?",
            (g["id"],),
        )
        gm_rows = await cursor.fetchall()
        group_matches = [GroupMatch(**dict(gm)) for gm in gm_rows]

        groups.append(
            Group(**{**dict(g), "teams": group_teams, "matches": group_matches})
        )
    return groups


async def _load_bracket_matches(db, tournament_id: int) -> list[BracketMatch]:  # noqa: ANN001
    """Lädt alle Bracket-Matches eines Turniers."""
    cursor = await db.execute(
        "SELECT * FROM bracket_matches WHERE tournament_id = ? ORDER BY round, position",
        (tournament_id,),
    )
    rows = await cursor.fetchall()
    return [BracketMatch(**dict(r)) for r in rows]


async def _load_signups_for_tournament(db, tournament_id: int) -> list[TournamentSignup]:  # noqa: ANN001
    """Lädt alle Solo-/Signup-Einträge eines Turniers."""
    cursor = await db.execute(
        "SELECT ts.id, ts.tournament_id, ts.discord_id, "
        "COALESCE(NULLIF(ts.discord_name, ''), NULLIF(s.discord_name, ''), NULLIF(tm.discord_name, '')) AS discord_name, "
        "ts.steam_id, ts.rank, ts.rank_score, ts.team_id, ts.signed_up_at "
        "FROM tournament_signups ts "
        "LEFT JOIN ("
        "    SELECT discord_id, MAX(discord_name) AS discord_name "
        "    FROM sessions "
        "    WHERE discord_name IS NOT NULL AND discord_name != '' "
        "    GROUP BY discord_id"
        ") s ON s.discord_id = ts.discord_id "
        "LEFT JOIN ("
        "    SELECT discord_id, MAX(discord_name) AS discord_name "
        "    FROM team_members "
        "    WHERE discord_name IS NOT NULL AND discord_name != '' "
        "    GROUP BY discord_id"
        ") tm ON tm.discord_id = ts.discord_id "
        "WHERE ts.tournament_id = ? "
        "ORDER BY ts.signed_up_at DESC",
        (tournament_id,),
    )
    rows = await cursor.fetchall()
    return [TournamentSignup(**await _enrich_rank_data(dict(r))) for r in rows]


async def _upsert_signup(
    db,
    tournament_id: int,
    *,
    discord_id: str,
    discord_name: str | None,
    steam_id: str | None,
    rank: str | None,
    rank_score: int,
    team_id: int | None,
) -> None:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
        (tournament_id, discord_id),
    )
    existing = await cursor.fetchone()
    if existing:
        await db.execute(
            "UPDATE tournament_signups SET discord_name = ?, steam_id = ?, rank = ?, "
            "rank_score = ?, team_id = ? WHERE id = ?",
            (
                discord_name,
                steam_id,
                rank,
                rank_score,
                team_id,
                existing["id"],
            ),
        )
        return

    await db.execute(
        "INSERT INTO tournament_signups "
        "(tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id) "
        "VALUES (?, ?, ?, ?, ?, ?, ?)",
        (
            tournament_id,
            discord_id,
            discord_name,
            steam_id,
            rank,
            rank_score,
            team_id,
        ),
    )


# ---------------------------------------------------------------------------
# GET /api/tournaments — Liste aller Turniere (public, nicht-draft)
# ---------------------------------------------------------------------------

@router.get("/tournaments", response_model=list[Tournament])
async def list_tournaments() -> list[Tournament]:
    """Alle öffentlichen Turniere (nicht im Draft-Status)."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE status != 'draft' ORDER BY created_at DESC"
        )
        rows = await cursor.fetchall()
    return [Tournament(**dict(r)) for r in rows]


# ---------------------------------------------------------------------------
# GET /api/tournaments/{id} — Turnier Detail
# ---------------------------------------------------------------------------

@router.get("/tournaments/{tournament_id}", response_model=TournamentDetail)
async def get_tournament(tournament_id: int) -> TournamentDetail:
    """Turnier-Detail mit Teams, Gruppen und Bracket."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        row = await cursor.fetchone()
        if not row:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        if row["status"] == "draft":
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )

        tournament_data = dict(row)
        teams = await _load_teams_for_tournament(db, tournament_id)
        groups = await _load_groups_for_tournament(db, tournament_id)
        bracket_matches = await _load_bracket_matches(db, tournament_id)
        signups = await _load_signups_for_tournament(db, tournament_id)

    return TournamentDetail(
        **tournament_data,
        teams=teams,
        groups=groups,
        bracket_matches=bracket_matches,
        signups=signups,
    )


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/teams — Team erstellen
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/teams", response_model=Team, status_code=201)
async def create_team(
    tournament_id: int,
    body: dict,
    user: UserSession = Depends(require_auth),
) -> Team:
    """Neues Team erstellen. Der aktuelle User wird Captain."""
    name: str = (body.get("name") or "").strip()

    # Validierung: Name 2-32 Zeichen
    if len(name) < 2 or len(name) > 32:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Team-Name muss zwischen 2 und 32 Zeichen lang sein",
        )

    name_key = name.casefold()

    async with get_db() as db:
        # Turnier prüfen
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )

        if tournament["status"] != "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Anmeldung ist nicht geöffnet",
            )

        # Name-Einzigartigkeit prüfen (casefold)
        cursor = await db.execute(
            "SELECT id FROM teams WHERE tournament_id = ? AND name_key = ?",
            (tournament_id, name_key),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Ein Team mit diesem Namen existiert bereits",
            )

        # Prüfen ob User bereits in einem Team dieses Turniers ist
        cursor = await db.execute(
            "SELECT tm.id FROM team_members tm "
            "JOIN teams t ON tm.team_id = t.id "
            "WHERE t.tournament_id = ? AND tm.discord_id = ?",
            (tournament_id, user.discord_id),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Du bist bereits in einem Team dieses Turniers",
            )

        # Team erstellen
        cursor = await db.execute(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) "
            "VALUES (?, ?, ?, ?)",
            (tournament_id, name, name_key, user.discord_id),
        )
        team_id = cursor.lastrowid

        # Steam-Link laden
        rank_data = await get_player_rank_profile(user.discord_id)
        steam_id = rank_data.get("steam_id") if rank_data else None
        rank = rank_data.get("rank") if rank_data else None
        score = rank_data.get("rank_score", 0) if rank_data else 0

        # Captain als erstes Mitglied
        await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
            "VALUES (?, ?, ?, ?, ?, ?, 'captain')",
            (team_id, user.discord_id, user.discord_name, steam_id, rank, score),
        )
        await _upsert_signup(
            db,
            tournament_id,
            discord_id=user.discord_id,
            discord_name=user.discord_name,
            steam_id=steam_id,
            rank=rank,
            rank_score=score,
            team_id=team_id,
        )
        await db.commit()

        # Team zurückladen
        cursor = await db.execute("SELECT * FROM teams WHERE id = ?", (team_id,))
        team_row = await cursor.fetchone()
        cursor = await db.execute(
            "SELECT tm.id, tm.team_id, tm.discord_id, "
            "COALESCE(NULLIF(tm.discord_name, ''), NULLIF(s.discord_name, '')) AS discord_name, "
            "tm.steam_id, tm.rank, tm.rank_score, tm.role, tm.joined_at "
            "FROM team_members tm "
            "LEFT JOIN ("
            "    SELECT discord_id, MAX(discord_name) AS discord_name "
            "    FROM sessions "
            "    WHERE discord_name IS NOT NULL AND discord_name != '' "
            "    GROUP BY discord_id"
            ") s ON s.discord_id = tm.discord_id "
            "WHERE tm.team_id = ? "
            "ORDER BY tm.joined_at",
            (team_id,),
        )
        member_rows = await cursor.fetchall()
        members = [TeamMember(**await _enrich_rank_data(dict(m))) for m in member_rows]

    return Team(**{**dict(team_row), "members": members})


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/teams/{team_id}/join — Team beitreten
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/teams/{team_id}/join", response_model=TeamMember, status_code=200)
async def join_team(
    tournament_id: int,
    team_id: int,
    user: UserSession = Depends(require_auth),
) -> TeamMember:
    """Einem bestehenden Team beitreten."""
    async with get_db() as db:
        # Turnier prüfen
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        if tournament["status"] != "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Anmeldung ist nicht geöffnet",
            )

        # Team prüfen
        cursor = await db.execute(
            "SELECT * FROM teams WHERE id = ? AND tournament_id = ?",
            (team_id, tournament_id),
        )
        team = await cursor.fetchone()
        if not team:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Team nicht gefunden",
            )

        # Team-Größe prüfen
        cursor = await db.execute(
            "SELECT COUNT(*) as cnt FROM team_members WHERE team_id = ?",
            (team_id,),
        )
        count_row = await cursor.fetchone()
        if count_row["cnt"] >= tournament["team_size"]:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Team ist bereits voll",
            )

        # User bereits in einem Team dieses Turniers? → automatisch austreten
        cursor = await db.execute(
            "SELECT tm.*, t.captain_discord_id FROM team_members tm "
            "JOIN teams t ON tm.team_id = t.id "
            "WHERE t.tournament_id = ? AND tm.discord_id = ?",
            (tournament_id, user.discord_id),
        )
        existing_membership = await cursor.fetchone()
        if existing_membership:
            old_team_id = existing_membership["team_id"]
            is_captain = user.discord_id == existing_membership["captain_discord_id"]

            # Mitgliederanzahl des alten Teams
            cursor = await db.execute(
                "SELECT COUNT(*) as cnt FROM team_members WHERE team_id = ?",
                (old_team_id,),
            )
            old_count_row = await cursor.fetchone()
            old_member_count = old_count_row["cnt"]

            if is_captain and old_member_count > 1:
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="Übergib zuerst die Captain-Rolle oder löse das Team auf",
                )

            if is_captain and old_member_count == 1:
                # Mitglied-Daten VOR dem Löschen sichern
                cursor = await db.execute(
                    "SELECT * FROM team_members WHERE team_id = ? AND discord_id = ?",
                    (old_team_id, user.discord_id),
                )
                old_captain_member = await cursor.fetchone()

                # Altes Team komplett auflösen
                await db.execute("DELETE FROM team_members WHERE team_id = ?", (old_team_id,))
                await db.execute("DELETE FROM teams WHERE id = ?", (old_team_id,))

                # tournament_signups prüfen und ggf. erstellen
                cursor = await db.execute(
                    "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
                    (tournament_id, user.discord_id),
                )
                existing_old_signup = await cursor.fetchone()
                if existing_old_signup:
                    await db.execute(
                        "UPDATE tournament_signups SET team_id = NULL WHERE tournament_id = ? AND discord_id = ?",
                        (tournament_id, user.discord_id),
                    )
                elif old_captain_member:
                    await db.execute(
                        "INSERT INTO tournament_signups "
                        "(tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id) "
                        "VALUES (?, ?, ?, ?, ?, ?, NULL)",
                        (
                            tournament_id,
                            user.discord_id,
                            old_captain_member["discord_name"],
                            old_captain_member["steam_id"],
                            old_captain_member["rank"],
                            old_captain_member["rank_score"],
                        ),
                    )
            else:
                # Normales Verlassen
                await db.execute(
                    "DELETE FROM team_members WHERE team_id = ? AND discord_id = ?",
                    (old_team_id, user.discord_id),
                )
                cursor = await db.execute(
                    "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
                    (tournament_id, user.discord_id),
                )
                if await cursor.fetchone():
                    await db.execute(
                        "UPDATE tournament_signups SET team_id = NULL WHERE tournament_id = ? AND discord_id = ?",
                        (tournament_id, user.discord_id),
                    )

        # Steam-Link laden
        rank_data = await get_player_rank_profile(user.discord_id)
        steam_id = rank_data.get("steam_id") if rank_data else None
        rank = rank_data.get("rank") if rank_data else None
        score = rank_data.get("rank_score", 0) if rank_data else 0

        # Mitglied hinzufügen
        cursor = await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
            "VALUES (?, ?, ?, ?, ?, ?, 'member')",
            (team_id, user.discord_id, user.discord_name, steam_id, rank, score),
        )

        await _upsert_signup(
            db,
            tournament_id,
            discord_id=user.discord_id,
            discord_name=user.discord_name,
            steam_id=steam_id,
            rank=rank,
            rank_score=score,
            team_id=team_id,
        )

        await db.commit()

        member_id = cursor.lastrowid
        cursor = await db.execute("SELECT * FROM team_members WHERE id = ?", (member_id,))
        member_row = await cursor.fetchone()

    return TeamMember(**dict(member_row))


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/signup — Solo-Anmeldung
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/signup", status_code=201)
async def solo_signup(
    tournament_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Solo-Anmeldung — User wird später zufällig einem Team zugewiesen."""
    async with get_db() as db:
        # Turnier prüfen
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        if tournament["status"] != "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Anmeldung ist nicht geöffnet",
            )

        # Bereits angemeldet?
        cursor = await db.execute(
            "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, user.discord_id),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Du bist bereits für dieses Turnier angemeldet",
            )

        # Bereits in einem Team?
        cursor = await db.execute(
            "SELECT tm.id FROM team_members tm "
            "JOIN teams t ON tm.team_id = t.id "
            "WHERE t.tournament_id = ? AND tm.discord_id = ?",
            (tournament_id, user.discord_id),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Du bist bereits in einem Team dieses Turniers",
            )

        # Steam-Link laden
        rank_data = await get_player_rank_profile(user.discord_id)
        steam_id = rank_data.get("steam_id") if rank_data else None
        rank = rank_data.get("rank") if rank_data else None
        score = rank_data.get("rank_score", 0) if rank_data else 0

        await db.execute(
            "INSERT INTO tournament_signups (tournament_id, discord_id, discord_name, steam_id, rank, rank_score) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            (tournament_id, user.discord_id, user.discord_name, steam_id, rank, score),
        )
        await db.commit()

    return {"status": "angemeldet", "tournament_id": tournament_id}


# ---------------------------------------------------------------------------
# DELETE /api/tournaments/{tournament_id}/signup — Solo-Austragen
# ---------------------------------------------------------------------------

@router.delete("/tournaments/{tournament_id}/signup", status_code=200)
async def cancel_solo_signup(
    tournament_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Solo-Anmeldung zurückziehen."""
    async with get_db() as db:
        # Turnier prüfen
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        if tournament["status"] != "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Anmeldung ist nicht geöffnet",
            )

        # Signup suchen
        cursor = await db.execute(
            "SELECT * FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, user.discord_id),
        )
        signup = await cursor.fetchone()
        if not signup:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Keine Anmeldung gefunden",
            )
        if signup["team_id"] is not None:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Du bist bereits in einem Team — verlasse zuerst das Team",
            )

        await db.execute(
            "DELETE FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, user.discord_id),
        )
        await db.commit()

    return {"status": "abgemeldet", "tournament_id": tournament_id}


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/checkin — Spieler-Check-in
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/checkin", status_code=200)
async def checkin_player(
    tournament_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Checkt den aktuellen Spieler für das Turnier ein."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        if tournament["status"] != "checkin":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Check-in ist aktuell nicht geöffnet",
            )

        cursor = await db.execute(
            "SELECT * FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, user.discord_id),
        )
        signup = await cursor.fetchone()

        if not signup:
            cursor = await db.execute(
                "SELECT tm.discord_name, tm.steam_id, tm.rank, tm.rank_score, tm.team_id "
                "FROM team_members tm "
                "JOIN teams t ON t.id = tm.team_id "
                "WHERE t.tournament_id = ? AND tm.discord_id = ?",
                (tournament_id, user.discord_id),
            )
            membership = await cursor.fetchone()
            if not membership:
                raise HTTPException(
                    status_code=status.HTTP_403_FORBIDDEN,
                    detail="Du bist für dieses Turnier nicht angemeldet",
                )
            await _upsert_signup(
                db,
                tournament_id,
                discord_id=user.discord_id,
                discord_name=user.discord_name or membership["discord_name"],
                steam_id=membership["steam_id"],
                rank=membership["rank"],
                rank_score=membership["rank_score"] or 0,
                team_id=membership["team_id"],
            )

        cursor = await db.execute(
            "SELECT checked_in_at FROM tournament_checkins WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, user.discord_id),
        )
        existing_checkin = await cursor.fetchone()
        already_checked_in = existing_checkin is not None
        if not already_checked_in:
            await db.execute(
                "INSERT INTO tournament_checkins (tournament_id, discord_id) VALUES (?, ?)",
                (tournament_id, user.discord_id),
            )
        await _audit(
            db,
            "tournament_checkin",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "discord_id": user.discord_id,
                    "already_checked_in": already_checked_in,
                }
            ),
        )
        await db.commit()

    return {"checked_in": True, "already_checked_in": already_checked_in}


# ---------------------------------------------------------------------------
# GET /api/tournaments/{tournament_id}/checkin-status — Statusübersicht
# ---------------------------------------------------------------------------

@router.get("/tournaments/{tournament_id}/checkin-status", status_code=200)
async def get_checkin_status(tournament_id: int) -> dict:
    """Gibt den aktuellen Check-in-Stand zurück."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT id FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        if not await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )

        cursor = await db.execute(
            "SELECT discord_id FROM tournament_signups WHERE tournament_id = ?",
            (tournament_id,),
        )
        registered_ids = {row["discord_id"] for row in await cursor.fetchall()}

        cursor = await db.execute(
            "SELECT tm.discord_id FROM team_members tm "
            "JOIN teams t ON t.id = tm.team_id WHERE t.tournament_id = ?",
            (tournament_id,),
        )
        registered_ids.update(row["discord_id"] for row in await cursor.fetchall())

        cursor = await db.execute(
            "SELECT discord_id FROM tournament_checkins "
            "WHERE tournament_id = ? ORDER BY checked_in_at, id",
            (tournament_id,),
        )
        checked_in_ids = [row["discord_id"] for row in await cursor.fetchall()]

    return {
        "total_registered": len(registered_ids),
        "total_checked_in": len(checked_in_ids),
        "checked_in_discord_ids": checked_in_ids,
    }


# ---------------------------------------------------------------------------
# DELETE /api/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}
# — Captain kickt Mitglied
# ---------------------------------------------------------------------------

@router.delete("/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}", status_code=200)
async def kick_team_member(
    tournament_id: int,
    team_id: int,
    discord_id: str,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Captain kickt ein Mitglied aus dem Team."""
    if not re.match(r'^\d{17,19}$', discord_id):
        raise HTTPException(status_code=400, detail="Ungültige Discord-ID")
    async with get_db() as db:
        # Turnier prüfen
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        if tournament["status"] != "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Anmeldung ist nicht geöffnet",
            )

        # Team prüfen
        cursor = await db.execute(
            "SELECT * FROM teams WHERE id = ? AND tournament_id = ?",
            (team_id, tournament_id),
        )
        team = await cursor.fetchone()
        if not team:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Team nicht gefunden",
            )

        # Nur Captain darf kicken
        if user.discord_id != team["captain_discord_id"]:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="Nur der Captain darf Mitglieder entfernen",
            )

        # Captain kann sich nicht selbst kicken
        if discord_id == user.discord_id:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Du kannst dich nicht selbst kicken",
            )

        # Mitglied suchen
        cursor = await db.execute(
            "SELECT * FROM team_members WHERE team_id = ? AND discord_id = ?",
            (team_id, discord_id),
        )
        member = await cursor.fetchone()
        if not member:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Mitglied nicht gefunden",
            )

        # Mitglied aus team_members entfernen
        await db.execute(
            "DELETE FROM team_members WHERE team_id = ? AND discord_id = ?",
            (team_id, discord_id),
        )

        # tournament_signups aktualisieren oder neu erstellen
        cursor = await db.execute(
            "SELECT * FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, discord_id),
        )
        existing_signup = await cursor.fetchone()
        if existing_signup:
            await db.execute(
                "UPDATE tournament_signups SET team_id = NULL WHERE tournament_id = ? AND discord_id = ?",
                (tournament_id, discord_id),
            )
        else:
            await db.execute(
                "INSERT INTO tournament_signups "
                "(tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id) "
                "VALUES (?, ?, ?, ?, ?, ?, NULL)",
                (
                    tournament_id,
                    discord_id,
                    member["discord_name"],
                    member["steam_id"],
                    member["rank"],
                    member["rank_score"],
                ),
            )

        await db.commit()

    return {"status": "mitglied_entfernt", "discord_id": discord_id, "team_id": team_id}


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/teams/{team_id}/invite/{target_discord_id}
# — Captain lädt Solo-Spieler ein
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/teams/{team_id}/invite/{target_discord_id}", response_model=Team, status_code=200)
async def invite_to_team(
    tournament_id: int,
    team_id: int,
    target_discord_id: str,
    user: UserSession = Depends(require_auth),
) -> Team:
    """Captain lädt einen Solo-Spieler direkt ins Team ein."""
    if not re.match(r'^\d{17,19}$', target_discord_id):
        raise HTTPException(status_code=400, detail="Ungültige Discord-ID")
    async with get_db() as db:
        # Turnier prüfen
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        if tournament["status"] != "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Anmeldung ist nicht geöffnet",
            )

        # Team prüfen
        cursor = await db.execute(
            "SELECT * FROM teams WHERE id = ? AND tournament_id = ?",
            (team_id, tournament_id),
        )
        team = await cursor.fetchone()
        if not team:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Team nicht gefunden",
            )

        # Nur Captain darf einladen
        if user.discord_id != team["captain_discord_id"]:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="Nur der Captain darf Spieler einladen",
            )

        # Prüfe zuerst: Spieler nicht bereits in einem Team
        cursor = await db.execute(
            "SELECT tm.id FROM team_members tm "
            "JOIN teams t ON tm.team_id = t.id "
            "WHERE t.tournament_id = ? AND tm.discord_id = ?",
            (tournament_id, target_discord_id),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Spieler ist bereits in einem Team",
            )

        # Prüfe: Ziel-Spieler ist solo angemeldet
        cursor = await db.execute(
            "SELECT * FROM tournament_signups WHERE tournament_id = ? AND discord_id = ? AND team_id IS NULL",
            (tournament_id, target_discord_id),
        )
        solo_signup = await cursor.fetchone()
        if not solo_signup:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Spieler ist nicht als Solo-Spieler angemeldet",
            )

        # Team-Größe prüfen
        cursor = await db.execute(
            "SELECT COUNT(*) as cnt FROM team_members WHERE team_id = ?",
            (team_id,),
        )
        count_row = await cursor.fetchone()
        if count_row["cnt"] >= tournament["team_size"]:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Team ist bereits voll",
            )

        # discord_name aus früherer team_members Mitgliedschaft ermitteln, Fallback: discord_id
        cursor = await db.execute(
            "SELECT discord_name FROM team_members WHERE discord_id = ? AND discord_name != '' LIMIT 1",
            (target_discord_id,),
        )
        name_row = await cursor.fetchone()
        discord_name = name_row["discord_name"] if name_row else target_discord_id

        # Spieler zum Team hinzufügen
        await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
            "VALUES (?, ?, ?, ?, ?, ?, 'member')",
            (team_id, target_discord_id, discord_name, solo_signup["steam_id"], solo_signup["rank"], solo_signup["rank_score"]),
        )

        await _upsert_signup(
            db,
            tournament_id,
            discord_id=target_discord_id,
            discord_name=discord_name,
            steam_id=solo_signup["steam_id"],
            rank=solo_signup["rank"],
            rank_score=solo_signup["rank_score"] or 0,
            team_id=team_id,
        )

        await db.commit()

        # Aktualisiertes Team zurückgeben
        cursor = await db.execute("SELECT * FROM teams WHERE id = ?", (team_id,))
        team_row = await cursor.fetchone()
        cursor = await db.execute("SELECT * FROM team_members WHERE team_id = ?", (team_id,))
        members = [TeamMember(**dict(m)) for m in await cursor.fetchall()]

    return Team(**{**dict(team_row), "members": members})


# ---------------------------------------------------------------------------
# DELETE /api/tournaments/{tournament_id}/teams/{team_id}/leave — Team verlassen
# ---------------------------------------------------------------------------

@router.delete("/tournaments/{tournament_id}/teams/{team_id}/leave", status_code=200)
async def leave_team(
    tournament_id: int,
    team_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Team verlassen. Captain kann nur austreten wenn er allein ist."""
    async with get_db() as db:
        # Turnier prüfen
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        if tournament["status"] != "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Anmeldung ist nicht geöffnet",
            )

        # Team prüfen
        cursor = await db.execute(
            "SELECT * FROM teams WHERE id = ? AND tournament_id = ?",
            (team_id, tournament_id),
        )
        team = await cursor.fetchone()
        if not team:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Team nicht gefunden",
            )

        # User-Mitglied suchen
        cursor = await db.execute(
            "SELECT * FROM team_members WHERE team_id = ? AND discord_id = ?",
            (team_id, user.discord_id),
        )
        member = await cursor.fetchone()
        if not member:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Du bist kein Mitglied dieses Teams",
            )

        # Mitgliederanzahl
        cursor = await db.execute(
            "SELECT COUNT(*) as cnt FROM team_members WHERE team_id = ?",
            (team_id,),
        )
        count_row = await cursor.fetchone()
        member_count = count_row["cnt"]

        is_captain = user.discord_id == team["captain_discord_id"]

        if is_captain and member_count > 1:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Übergib zuerst die Captain-Rolle oder löse das Team auf",
            )

        if is_captain and member_count == 1:
            # Mitglied-Daten VOR dem Löschen sichern (für ggf. neuen Signup-Eintrag)
            captain_steam_id = member["steam_id"]
            captain_rank = member["rank"]
            captain_rank_score = member["rank_score"]

            # Team komplett auflösen
            await db.execute("DELETE FROM team_members WHERE team_id = ?", (team_id,))
            await db.execute("DELETE FROM teams WHERE id = ?", (team_id,))

            # tournament_signups prüfen
            cursor = await db.execute(
                "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
                (tournament_id, user.discord_id),
            )
            existing_captain_signup = await cursor.fetchone()
            if existing_captain_signup:
                # Eintrag vorhanden — team_id zurücksetzen
                await db.execute(
                    "UPDATE tournament_signups SET team_id = NULL WHERE tournament_id = ? AND discord_id = ?",
                    (tournament_id, user.discord_id),
                )
            else:
                # Kein Eintrag — Captain hat das Team direkt erstellt, neuen Solo-Eintrag anlegen
                await db.execute(
                    "INSERT INTO tournament_signups "
                    "(tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id) "
                    "VALUES (?, ?, ?, ?, ?, ?, NULL)",
                    (
                        tournament_id,
                        user.discord_id,
                        user.discord_name,
                        captain_steam_id,
                        captain_rank,
                        captain_rank_score,
                    ),
                )

            await db.commit()
            return {"status": "team_aufgeloest", "team_id": team_id}

        # Normales Verlassen
        await db.execute(
            "DELETE FROM team_members WHERE team_id = ? AND discord_id = ?",
            (team_id, user.discord_id),
        )

        # tournament_signups aktualisieren oder erstellen
        cursor = await db.execute(
            "SELECT * FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, user.discord_id),
        )
        existing_signup = await cursor.fetchone()
        if existing_signup:
            await db.execute(
                "UPDATE tournament_signups SET team_id = NULL WHERE tournament_id = ? AND discord_id = ?",
                (tournament_id, user.discord_id),
            )
        else:
            await db.execute(
                "INSERT INTO tournament_signups "
                "(tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id) "
                "VALUES (?, ?, ?, ?, ?, ?, NULL)",
                (
                    tournament_id,
                    user.discord_id,
                    member["discord_name"],
                    member["steam_id"],
                    member["rank"],
                    member["rank_score"],
                ),
            )

        await db.commit()

    return {"status": "team_verlassen", "team_id": team_id}


# ---------------------------------------------------------------------------
# GET /api/tournaments/{tournament_id}/bracket — Bracket-Daten
# ---------------------------------------------------------------------------

@router.get("/tournaments/{tournament_id}/bracket", response_model=list[BracketMatch])
async def get_bracket(tournament_id: int) -> list[BracketMatch]:
    """Bracket-Matches eines Turniers."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        if not await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        return await _load_bracket_matches(db, tournament_id)


# ---------------------------------------------------------------------------
# GET /api/tournaments/{tournament_id}/groups — Gruppen-Standings
# ---------------------------------------------------------------------------

@router.get("/tournaments/{tournament_id}/groups", response_model=list[Group])
async def get_groups(tournament_id: int) -> list[Group]:
    """Gruppen-Standings eines Turniers."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        if not await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )
        return await _load_groups_for_tournament(db, tournament_id)
