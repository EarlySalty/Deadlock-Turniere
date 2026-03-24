"""Oeffentliche und authentifizierte Tournament + Team Routes."""
from __future__ import annotations

import json

from fastapi import APIRouter, Depends, HTTPException, status

from auth.permissions import require_auth
from db import get_db
from steam.reader import get_steam_link
from tournament.models import (
    BracketMatch,
    Group,
    GroupMatch,
    GroupTeam,
    Team,
    TeamMember,
    Tournament,
    TournamentDetail,
    UserSession,
)

router = APIRouter(prefix="/api", tags=["tournaments"])


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

async def _load_teams_for_tournament(db, tournament_id: int) -> list[Team]:  # noqa: ANN001
    """Laedt alle Teams eines Turniers inkl. Members."""
    cursor = await db.execute(
        "SELECT * FROM teams WHERE tournament_id = ?",
        (tournament_id,),
    )
    team_rows = await cursor.fetchall()
    teams: list[Team] = []
    for t in team_rows:
        cursor = await db.execute(
            "SELECT * FROM team_members WHERE team_id = ?",
            (t["id"],),
        )
        member_rows = await cursor.fetchall()
        members = [TeamMember(**dict(m)) for m in member_rows]
        teams.append(Team(**{**dict(t), "members": members}))
    return teams


async def _load_groups_for_tournament(db, tournament_id: int) -> list[Group]:  # noqa: ANN001
    """Laedt alle Gruppen eines Turniers inkl. Teams und Matches."""
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
    """Laedt alle Bracket-Matches eines Turniers."""
    cursor = await db.execute(
        "SELECT * FROM bracket_matches WHERE tournament_id = ? ORDER BY round, position",
        (tournament_id,),
    )
    rows = await cursor.fetchall()
    return [BracketMatch(**dict(r)) for r in rows]


# ---------------------------------------------------------------------------
# GET /api/tournaments — Liste aller Turniere (public, nicht-draft)
# ---------------------------------------------------------------------------

@router.get("/tournaments", response_model=list[Tournament])
async def list_tournaments() -> list[Tournament]:
    """Alle oeffentlichen Turniere (nicht im Draft-Status)."""
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

        tournament_data = dict(row)
        teams = await _load_teams_for_tournament(db, tournament_id)
        groups = await _load_groups_for_tournament(db, tournament_id)
        bracket_matches = await _load_bracket_matches(db, tournament_id)

    return TournamentDetail(
        **tournament_data,
        teams=teams,
        groups=groups,
        bracket_matches=bracket_matches,
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
        # Turnier pruefen
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
                detail="Anmeldung ist nicht geoeffnet",
            )

        # Name-Einzigartigkeit pruefen (casefold)
        cursor = await db.execute(
            "SELECT id FROM teams WHERE tournament_id = ? AND name_key = ?",
            (tournament_id, name_key),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Ein Team mit diesem Namen existiert bereits",
            )

        # Pruefen ob User bereits in einem Team dieses Turniers ist
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
        steam_data = await get_steam_link(user.discord_id)
        steam_id = steam_data["steam_id"] if steam_data else None
        rank = steam_data["rank"] if steam_data else None
        score = steam_data["rank_score"] if steam_data else 0

        # Captain als erstes Mitglied
        await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
            "VALUES (?, ?, ?, ?, ?, ?, 'captain')",
            (team_id, user.discord_id, user.discord_name, steam_id, rank, score),
        )
        await db.commit()

        # Team zurueckladen
        cursor = await db.execute("SELECT * FROM teams WHERE id = ?", (team_id,))
        team_row = await cursor.fetchone()
        cursor = await db.execute("SELECT * FROM team_members WHERE team_id = ?", (team_id,))
        members = [TeamMember(**dict(m)) for m in await cursor.fetchall()]

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
        # Turnier pruefen
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
                detail="Anmeldung ist nicht geoeffnet",
            )

        # Team pruefen
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

        # Team-Groesse pruefen
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

        # User bereits in einem Team dieses Turniers?
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
        steam_data = await get_steam_link(user.discord_id)
        steam_id = steam_data["steam_id"] if steam_data else None
        rank = steam_data["rank"] if steam_data else None
        score = steam_data["rank_score"] if steam_data else 0

        # Mitglied hinzufuegen
        cursor = await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
            "VALUES (?, ?, ?, ?, ?, ?, 'member')",
            (team_id, user.discord_id, user.discord_name, steam_id, rank, score),
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
    """Solo-Anmeldung — User wird spaeter zufaellig einem Team zugewiesen."""
    async with get_db() as db:
        # Turnier pruefen
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
                detail="Anmeldung ist nicht geoeffnet",
            )

        # Bereits angemeldet?
        cursor = await db.execute(
            "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, user.discord_id),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Du bist bereits fuer dieses Turnier angemeldet",
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
        steam_data = await get_steam_link(user.discord_id)
        steam_id = steam_data["steam_id"] if steam_data else None
        rank = steam_data["rank"] if steam_data else None
        score = steam_data["rank_score"] if steam_data else 0

        await db.execute(
            "INSERT INTO tournament_signups (tournament_id, discord_id, steam_id, rank, rank_score) "
            "VALUES (?, ?, ?, ?, ?)",
            (tournament_id, user.discord_id, steam_id, rank, score),
        )
        await db.commit()

    return {"status": "angemeldet", "tournament_id": tournament_id}


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
