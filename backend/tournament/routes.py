"""Öffentliche und authentifizierte Tournament + Team Routes."""
from __future__ import annotations

import json
import logging
import re
from datetime import datetime, timezone

from fastapi import APIRouter, Depends, HTTPException, status

from auth.permissions import require_auth
from db import get_db
from rank_reader import get_player_rank_profile
from notifications.discord_notifier import notify_users
from tournament.models import (
    BracketMiniGroup,
    BracketMatch,
    Group,
    GroupMatch,
    GroupTeam,
    InviteMode,
    RecruitmentStatus,
    Team,
    TeamApplication,
    TeamInvitation,
    TeamMember,
    TeamMemberPublic,
    TeamPublic,
    Tournament,
    TournamentDetailPublic,
    TournamentSignup,
    TournamentSignupPublic,
    UserSession,
)

router = APIRouter(prefix="/api", tags=["tournaments"])
_CURRENT_CONSENT_VERSION = 2
logger = logging.getLogger(__name__)


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


def _parse_timestamp(value: str | None) -> datetime | None:
    """Parst Zeitstempel robust für UTC-Vergleiche."""
    if not value:
        return None

    normalized = value.strip().replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError:
        return None

    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def _looks_like_discord_id(value: str | None) -> bool:
    if not value:
        return False
    stripped = value.strip()
    return stripped.isdigit() and 16 <= len(stripped) <= 21


def _sanitize_discord_name(
    discord_name: str | None,
    *,
    discord_id: str | None = None,
) -> str | None:
    if not discord_name:
        return None
    stripped = discord_name.strip()
    if not stripped:
        return None
    if discord_id and stripped == str(discord_id).strip():
        return None
    if _looks_like_discord_id(stripped):
        return None
    return stripped


def _preferred_discord_name(
    *candidates: str | None,
    discord_id: str | None = None,
) -> str | None:
    for candidate in candidates:
        sanitized = _sanitize_discord_name(candidate, discord_id=discord_id)
        if sanitized:
            return sanitized
    return None


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
            "SELECT tm.id, tm.team_id, tm.discord_id, tm.discord_name AS team_member_discord_name, "
            "s.discord_name AS session_discord_name, p.display_name AS profile_display_name, "
            "tm.steam_id, tm.rank, tm.rank_score, tm.role, tm.joined_at "
            "FROM team_members tm "
            "LEFT JOIN ("
            "    SELECT discord_id, MAX(discord_name) AS discord_name "
            "    FROM sessions "
            "    WHERE discord_name IS NOT NULL AND discord_name != '' "
            "    GROUP BY discord_id"
            ") s ON s.discord_id = tm.discord_id "
            "LEFT JOIN user_profiles p ON p.discord_id = tm.discord_id "
            "WHERE tm.team_id = ? "
            "ORDER BY tm.joined_at",
            (t["id"],),
        )
        member_rows = await cursor.fetchall()
        members: list[TeamMember] = []
        for member_row in member_rows:
            member_data = dict(member_row)
            member_data["discord_name"] = _preferred_discord_name(
                member_data.pop("profile_display_name", None),
                member_data.pop("session_discord_name", None),
                member_data.pop("team_member_discord_name", None),
                discord_id=member_data["discord_id"],
            )
            members.append(TeamMember(**await _enrich_rank_data(member_data)))
        teams.append(Team(**{**dict(t), "members": members}))
    return teams


async def _load_teams_public(db, tournament_id: int) -> list[TeamPublic]:  # noqa: ANN001
    """Lädt Teams ohne captain_discord_id/discord_id in Members."""
    cursor = await db.execute(
        "SELECT * FROM teams WHERE tournament_id = ?",
        (tournament_id,),
    )
    team_rows = await cursor.fetchall()
    teams: list[TeamPublic] = []
    for t in team_rows:
        cursor = await db.execute(
            "SELECT tm.id, tm.team_id, tm.discord_id, tm.discord_name AS team_member_discord_name, "
            "s.discord_name AS session_discord_name, p.display_name AS profile_display_name, "
            "tm.steam_id, tm.rank, tm.rank_score, tm.role, tm.joined_at "
            "FROM team_members tm "
            "LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM sessions "
            "WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s "
            "ON s.discord_id = tm.discord_id "
            "LEFT JOIN user_profiles p ON p.discord_id = tm.discord_id "
            "WHERE tm.team_id = ? ORDER BY tm.joined_at",
            (t["id"],),
        )
        member_rows = await cursor.fetchall()
        members: list[TeamMemberPublic] = []
        for member_row in member_rows:
            member_data = dict(member_row)
            member_data["discord_name"] = _preferred_discord_name(
                member_data.pop("profile_display_name", None),
                member_data.pop("session_discord_name", None),
                member_data.pop("team_member_discord_name", None),
                discord_id=member_data.pop("discord_id", None),
            )
            members.append(TeamMemberPublic(**member_data))
        cursor2 = await db.execute(
            "SELECT COUNT(*) FROM team_applications WHERE team_id = ? AND status = 'pending'",
            (t["id"],),
        )
        app_count = (await cursor2.fetchone())[0]
        teams.append(TeamPublic(**{
            **dict(t),
            "members": members,
            "has_pending_applications": (
                t["recruitment_status"] == RecruitmentStatus.application.value
                and app_count > 0
            ),
        }))
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


async def _load_mini_groups_for_tournament(db, tournament_id: int) -> list[BracketMiniGroup]:  # noqa: ANN001
    cursor = await db.execute(
        """
        SELECT id, tournament_id, round, position, advances_to_match_id, advances_to_slot
        FROM bracket_mini_groups
        WHERE tournament_id = ?
        ORDER BY round, position, id
        """,
        (tournament_id,),
    )
    mini_group_rows = await cursor.fetchall()
    mini_groups: list[BracketMiniGroup] = []
    for row in mini_group_rows:
        mini_group_id = int(row["id"])
        cursor = await db.execute(
            """
            SELECT team_id
            FROM bracket_mini_group_teams
            WHERE mini_group_id = ? AND team_id IS NOT NULL
            ORDER BY seed_order, id
            """,
            (mini_group_id,),
        )
        team_ids = [int(team_row["team_id"]) for team_row in await cursor.fetchall()]
        cursor = await db.execute(
            """
            SELECT id
            FROM bracket_matches
            WHERE mini_group_id = ?
            ORDER BY round, position, id
            """,
            (mini_group_id,),
        )
        match_ids = [int(match_row["id"]) for match_row in await cursor.fetchall()]
        mini_groups.append(
            BracketMiniGroup(
                **dict(row),
                team_ids=team_ids,
                match_ids=match_ids,
            )
        )
    return mini_groups


async def _load_signups_for_tournament(db, tournament_id: int) -> list[TournamentSignup]:  # noqa: ANN001
    """Lädt alle Solo-/Signup-Einträge eines Turniers."""
    cursor = await db.execute(
        "SELECT ts.id, ts.tournament_id, ts.discord_id, "
        "ts.discord_name AS signup_discord_name, s.discord_name AS session_discord_name, "
        "tm.discord_name AS team_member_discord_name, p.display_name AS profile_display_name, "
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
        "LEFT JOIN user_profiles p ON p.discord_id = ts.discord_id "
        "WHERE ts.tournament_id = ? "
        "ORDER BY ts.signed_up_at DESC",
        (tournament_id,),
    )
    rows = await cursor.fetchall()
    signups: list[TournamentSignup] = []
    for row in rows:
        signup_data = dict(row)
        signup_data["discord_name"] = _preferred_discord_name(
            signup_data.pop("profile_display_name", None),
            signup_data.pop("session_discord_name", None),
            signup_data.pop("signup_discord_name", None),
            signup_data.pop("team_member_discord_name", None),
            discord_id=signup_data["discord_id"],
        )
        signups.append(TournamentSignup(**await _enrich_rank_data(signup_data)))
    return signups


async def _load_signups_public(db, tournament_id: int) -> list[TournamentSignupPublic]:  # noqa: ANN001
    """Lädt Signups ohne discord_id."""
    cursor = await db.execute(
        "SELECT ts.id, ts.tournament_id, ts.discord_id, "
        "ts.discord_name AS signup_discord_name, s.discord_name AS session_discord_name, "
        "tm.discord_name AS team_member_discord_name, p.display_name AS profile_display_name, "
        "ts.steam_id, ts.rank, ts.rank_score, ts.team_id, ts.signed_up_at "
        "FROM tournament_signups ts "
        "LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM sessions "
        "WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s "
        "ON s.discord_id = ts.discord_id "
        "LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM team_members "
        "WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) tm "
        "ON tm.discord_id = ts.discord_id "
        "LEFT JOIN user_profiles p ON p.discord_id = ts.discord_id "
        "WHERE ts.tournament_id = ? ORDER BY ts.signed_up_at DESC",
        (tournament_id,),
    )
    rows = await cursor.fetchall()
    signups: list[TournamentSignupPublic] = []
    for row in rows:
        signup_data = dict(row)
        signup_data["discord_name"] = _preferred_discord_name(
            signup_data.pop("profile_display_name", None),
            signup_data.pop("session_discord_name", None),
            signup_data.pop("signup_discord_name", None),
            signup_data.pop("team_member_discord_name", None),
            discord_id=signup_data.pop("discord_id", None),
        )
        signups.append(TournamentSignupPublic(**signup_data))
    return signups


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


async def _load_tournament_or_404(db, tournament_id: int):  # noqa: ANN001
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
    return tournament


async def _load_team_or_404(db, tournament_id: int, team_id: int):  # noqa: ANN001
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
    return team


def _ensure_registration_open(tournament) -> None:  # noqa: ANN001
    if tournament["status"] not in ("registration", "checkin"):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Anmeldung ist nicht geöffnet",
        )


def _is_mod_user(user: UserSession) -> bool:
    return user.is_mod or user.is_admin


def _ensure_captain(user: UserSession, team) -> None:  # noqa: ANN001
    if user.discord_id != team["captain_discord_id"]:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Nur der Captain darf diese Aktion ausführen",
        )


def _ensure_captain_or_mod(user: UserSession, team) -> None:  # noqa: ANN001
    if user.discord_id == team["captain_discord_id"] or _is_mod_user(user):
        return
    raise HTTPException(
        status_code=status.HTTP_403_FORBIDDEN,
        detail="Nur Captain oder Mod dürfen diese Aktion ausführen",
    )


def _ensure_invites_enabled(tournament) -> str | None:  # noqa: ANN001
    mode = tournament["invite_mode"] or InviteMode.always.value
    if mode == InviteMode.never.value:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Einladungen sind für dieses Turnier deaktiviert",
        )

    if mode == InviteMode.window.value:
        now = datetime.now(timezone.utc)
        start = _parse_timestamp(tournament["invite_window_start"])
        end = _parse_timestamp(tournament["invite_window_end"])
        if start is None or end is None or now < start or now > end:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="Einladungen sind aktuell nicht erlaubt",
            )
        return tournament["invite_window_end"]

    return None


async def _count_team_members(db, team_id: int) -> int:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT COUNT(*) AS cnt FROM team_members WHERE team_id = ?",
        (team_id,),
    )
    row = await cursor.fetchone()
    return int(row["cnt"])


async def _ensure_team_has_capacity(db, team_id: int, team_size: int) -> None:  # noqa: ANN001
    if await _count_team_members(db, team_id) >= team_size:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Team ist bereits voll",
        )


async def _ensure_user_not_in_tournament_team(db, tournament_id: int, discord_id: str) -> None:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT tm.id FROM team_members tm "
        "JOIN teams t ON tm.team_id = t.id "
        "WHERE t.tournament_id = ? AND tm.discord_id = ?",
        (tournament_id, discord_id),
    )
    if await cursor.fetchone():
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Spieler ist bereits in einem Team",
        )


async def _resolve_discord_name(
    db,
    discord_id: str,
    *,
    preferred_name: str | None = None,
) -> str:  # noqa: ANN001
    sanitized_preferred = _sanitize_discord_name(preferred_name, discord_id=discord_id)
    if sanitized_preferred:
        return sanitized_preferred

    cursor = await db.execute(
        "SELECT discord_name FROM sessions "
        "WHERE discord_id = ? AND discord_name IS NOT NULL AND discord_name != '' "
        "ORDER BY id DESC LIMIT 1",
        (discord_id,),
    )
    row = await cursor.fetchone()
    if row:
        sanitized_session_name = _sanitize_discord_name(
            row["discord_name"],
            discord_id=discord_id,
        )
        if sanitized_session_name:
            return sanitized_session_name

    cursor = await db.execute(
        "SELECT discord_name FROM team_members "
        "WHERE discord_id = ? AND discord_name IS NOT NULL AND discord_name != '' "
        "ORDER BY joined_at DESC LIMIT 1",
        (discord_id,),
    )
    row = await cursor.fetchone()
    if row:
        sanitized_member_name = _sanitize_discord_name(
            row["discord_name"],
            discord_id=discord_id,
        )
        if sanitized_member_name:
            return sanitized_member_name

    cursor = await db.execute(
        "SELECT display_name FROM user_profiles "
        "WHERE discord_id = ? AND display_name IS NOT NULL AND display_name != '' "
        "ORDER BY updated_at DESC LIMIT 1",
        (discord_id,),
    )
    row = await cursor.fetchone()
    if row:
        sanitized_profile_name = _sanitize_discord_name(
            row["display_name"],
            discord_id=discord_id,
        )
        if sanitized_profile_name:
            return sanitized_profile_name

    return discord_id


async def _add_user_to_team(
    db,
    tournament_id: int,
    team_id: int,
    *,
    discord_id: str,
    discord_name: str | None,
    steam_id: str | None,
    rank: str | None,
    rank_score: int,
) -> None:  # noqa: ANN001
    resolved_name = await _resolve_discord_name(
        db,
        discord_id,
        preferred_name=discord_name,
    )
    await db.execute(
        "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
        "VALUES (?, ?, ?, ?, ?, ?, 'member')",
        (team_id, discord_id, resolved_name, steam_id, rank, rank_score),
    )
    await _upsert_signup(
        db,
        tournament_id,
        discord_id=discord_id,
        discord_name=resolved_name,
        steam_id=steam_id,
        rank=rank,
        rank_score=rank_score,
        team_id=team_id,
    )


async def _load_team_response(db, team_id: int) -> Team:  # noqa: ANN001
    cursor = await db.execute("SELECT * FROM teams WHERE id = ?", (team_id,))
    team_row = await cursor.fetchone()
    cursor = await db.execute(
        "SELECT tm.id, tm.team_id, tm.discord_id, tm.discord_name AS team_member_discord_name, "
        "s.discord_name AS session_discord_name, p.display_name AS profile_display_name, "
        "tm.steam_id, tm.rank, tm.rank_score, tm.role, tm.joined_at "
        "FROM team_members tm "
        "LEFT JOIN ("
            "    SELECT discord_id, MAX(discord_name) AS discord_name "
            "    FROM sessions "
            "    WHERE discord_name IS NOT NULL AND discord_name != '' "
            "    GROUP BY discord_id"
        ") s ON s.discord_id = tm.discord_id "
        "LEFT JOIN user_profiles p ON p.discord_id = tm.discord_id "
        "WHERE tm.team_id = ? "
        "ORDER BY tm.joined_at",
        (team_id,),
    )
    member_rows = await cursor.fetchall()
    members: list[TeamMember] = []
    for member_row in member_rows:
        member_data = dict(member_row)
        member_data["discord_name"] = _preferred_discord_name(
            member_data.pop("profile_display_name", None),
            member_data.pop("session_discord_name", None),
            member_data.pop("team_member_discord_name", None),
            discord_id=member_data["discord_id"],
        )
        members.append(TeamMember(**await _enrich_rank_data(member_data)))
    return Team(**{**dict(team_row), "members": members})


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

@router.get("/tournaments/{tournament_id}", response_model=TournamentDetailPublic)
async def get_tournament(tournament_id: int) -> TournamentDetailPublic:
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
        teams = await _load_teams_public(db, tournament_id)
        groups = await _load_groups_for_tournament(db, tournament_id)
        bracket_matches = await _load_bracket_matches(db, tournament_id)
        mini_groups = await _load_mini_groups_for_tournament(db, tournament_id)
        signups = await _load_signups_public(db, tournament_id)

    return TournamentDetailPublic(
        **tournament_data,
        teams=teams,
        groups=groups,
        bracket_matches=bracket_matches,
        mini_groups=mini_groups,
        signups=signups,
    )


# ---------------------------------------------------------------------------
# GET /api/tournaments/{tournament_id}/me — Eigener Status im Turnier
# ---------------------------------------------------------------------------

@router.get("/tournaments/{tournament_id}/me")
async def get_my_tournament_status(
    tournament_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Gibt den eigenen Anmeldestatus im Turnier zurück (kein discord_id im Public-Response)."""
    async with get_db() as db:
        # Team-Mitgliedschaft prüfen
        cursor = await db.execute(
            "SELECT tm.team_id, t.captain_discord_id "
            "FROM team_members tm "
            "JOIN teams t ON tm.team_id = t.id "
            "WHERE t.tournament_id = ? AND tm.discord_id = ?",
            (tournament_id, user.discord_id),
        )
        member_row = await cursor.fetchone()

        # Solo-Signup prüfen (nur wenn nicht in Team)
        cursor2 = await db.execute(
            "SELECT id FROM tournament_signups "
            "WHERE tournament_id = ? AND discord_id = ? AND (team_id IS NULL OR team_id = 0)",
            (tournament_id, user.discord_id),
        )
        signup_row = await cursor2.fetchone()

        # Check-in prüfen
        cursor3 = await db.execute(
            "SELECT id FROM tournament_checkins "
            "WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, user.discord_id),
        )
        checkin_row = await cursor3.fetchone()

    team_id = member_row["team_id"] if member_row else None
    is_captain = (
        member_row is not None
        and member_row["captain_discord_id"] == user.discord_id
    )
    signup_id = signup_row["id"] if signup_row else None

    return {
        "team_id": team_id,
        "signup_id": signup_id,
        "is_captain": is_captain,
        "is_checked_in": checkin_row is not None,
    }


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
        cursor = await db.execute(
            "SELECT consent_version FROM user_consents WHERE discord_id = ?",
            (user.discord_id,),
        )
        consent_row = await cursor.fetchone()
        if not consent_row or int(consent_row["consent_version"]) < _CURRENT_CONSENT_VERSION:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="CONSENT_REQUIRED",
            )

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

        if tournament["status"] not in ("registration", "checkin"):
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

        team = await _load_team_response(db, team_id)

    return team


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
        cursor = await db.execute(
            "SELECT consent_version FROM user_consents WHERE discord_id = ?",
            (user.discord_id,),
        )
        consent_row = await cursor.fetchone()
        if not consent_row or int(consent_row["consent_version"]) < _CURRENT_CONSENT_VERSION:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="CONSENT_REQUIRED",
            )

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
        if tournament["status"] not in ("registration", "checkin"):
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
        cursor = await db.execute(
            "SELECT consent_version FROM user_consents WHERE discord_id = ?",
            (user.discord_id,),
        )
        consent_row = await cursor.fetchone()
        if not consent_row or int(consent_row["consent_version"]) < _CURRENT_CONSENT_VERSION:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="CONSENT_REQUIRED",
            )

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
        if tournament["status"] not in ("registration", "checkin"):
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
# PATCH /api/tournaments/{tournament_id}/teams/{team_id}/recruiting
# ---------------------------------------------------------------------------

@router.patch("/tournaments/{tournament_id}/teams/{team_id}/recruiting", response_model=Team, status_code=200)
async def update_team_recruiting(
    tournament_id: int,
    team_id: int,
    body: dict,
    user: UserSession = Depends(require_auth),
) -> Team:
    """Setzt den Recruiting-Status eines Teams."""
    raw_status = body.get("recruitment_status")
    try:
        recruitment_status = RecruitmentStatus(raw_status)
    except ValueError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Ungültiger Recruiting-Status",
        ) from exc

    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_registration_open(tournament)
        team = await _load_team_or_404(db, tournament_id, team_id)
        _ensure_captain_or_mod(user, team)

        await db.execute(
            "UPDATE teams SET recruitment_status = ? WHERE id = ?",
            (recruitment_status.value, team_id),
        )
        await db.commit()
        return await _load_team_response(db, team_id)


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/teams/{team_id}/invite-by-signup/{signup_id}
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/teams/{team_id}/invite-by-signup/{signup_id}", status_code=200)
async def invite_to_team_by_signup(
    tournament_id: int,
    team_id: int,
    signup_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Captain lädt einen Solo-Signup über signup_id ein."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_registration_open(tournament)
        team = await _load_team_or_404(db, tournament_id, team_id)
        _ensure_captain(user, team)
        await _ensure_team_has_capacity(db, team_id, tournament["team_size"])

        cursor = await db.execute(
            "SELECT * FROM tournament_signups WHERE id = ? AND tournament_id = ?",
            (signup_id, tournament_id),
        )
        signup = await cursor.fetchone()
        if not signup:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Signup nicht gefunden",
            )
        if signup["team_id"] is not None:
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Spieler ist bereits einem Team zugeordnet",
            )

        await _ensure_user_not_in_tournament_team(db, tournament_id, signup["discord_id"])
        expires_at = _ensure_invites_enabled(tournament)

        cursor = await db.execute(
            "SELECT invite_auto_accept FROM user_profiles WHERE discord_id = ?",
            (signup["discord_id"],),
        )
        profile = await cursor.fetchone()
        auto_accept = bool(profile["invite_auto_accept"]) if profile else False

        cursor = await db.execute(
            "SELECT * FROM team_invitations WHERE team_id = ? AND discord_id = ?",
            (team_id, signup["discord_id"]),
        )
        existing_invitation = await cursor.fetchone()

        target_status = "accepted" if auto_accept else "pending"
        if existing_invitation and existing_invitation["status"] == "pending" and not auto_accept:
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Für diesen Spieler existiert bereits eine offene Einladung",
            )

        if existing_invitation:
            await db.execute(
                "UPDATE team_invitations SET tournament_id = ?, signup_id = ?, status = ?, "
                "created_at = datetime('now'), expires_at = ? WHERE id = ?",
                (tournament_id, signup_id, target_status, expires_at, existing_invitation["id"]),
            )
            invite_id = existing_invitation["id"]
        else:
            cursor = await db.execute(
                "INSERT INTO team_invitations (tournament_id, team_id, discord_id, signup_id, status, expires_at) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                (tournament_id, team_id, signup["discord_id"], signup_id, target_status, expires_at),
            )
            invite_id = cursor.lastrowid

        if auto_accept:
            await _add_user_to_team(
                db,
                tournament_id,
                team_id,
                discord_id=signup["discord_id"],
                discord_name=signup["discord_name"],
                steam_id=signup["steam_id"],
                rank=signup["rank"],
                rank_score=signup["rank_score"] or 0,
            )
            await db.execute(
                "UPDATE team_invitations SET status = 'accepted' WHERE id = ?",
                (invite_id,),
            )
            await db.commit()
            return {"status": "auto_accepted"}

        await db.commit()
    try:
        await notify_users(
            [signup["discord_id"]],
            "team_invite",
            f"Du wurdest von `{team['name']}` für `{tournament['name']}` eingeladen.",
        )
    except Exception:
        logger.exception(
            "Signup invite notification failed (tournament=%s team=%s signup=%s)",
            tournament_id,
            team_id,
            signup_id,
        )
    return {"status": "invited"}


# ---------------------------------------------------------------------------
# GET /api/tournaments/{tournament_id}/my-invitations
# ---------------------------------------------------------------------------

@router.get("/tournaments/{tournament_id}/my-invitations", response_model=list[TeamInvitation])
async def get_my_invitations(
    tournament_id: int,
    user: UserSession = Depends(require_auth),
) -> list[TeamInvitation]:
    """Gibt alle offenen Team-Einladungen des aktuellen Users zurück."""
    async with get_db() as db:
        await _load_tournament_or_404(db, tournament_id)
        cursor = await db.execute(
            "SELECT ti.id, ti.tournament_id, ti.team_id, t.name AS team_name, "
            "ti.status, ti.created_at, ti.expires_at "
            "FROM team_invitations ti "
            "JOIN teams t ON t.id = ti.team_id "
            "WHERE ti.tournament_id = ? AND ti.discord_id = ? AND ti.status = 'pending' "
            "ORDER BY ti.created_at DESC",
            (tournament_id, user.discord_id),
        )
        rows = await cursor.fetchall()
        return [TeamInvitation(**dict(row)) for row in rows]


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/invitations/{invite_id}/accept
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/invitations/{invite_id}/accept", status_code=200)
async def accept_team_invitation(
    tournament_id: int,
    invite_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Nimmt eine Team-Einladung an."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_registration_open(tournament)
        cursor = await db.execute(
            "SELECT * FROM team_invitations WHERE id = ? AND tournament_id = ?",
            (invite_id, tournament_id),
        )
        invitation = await cursor.fetchone()
        if not invitation:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Einladung nicht gefunden",
            )
        if invitation["discord_id"] != user.discord_id:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="Diese Einladung gehört nicht zu dir",
            )
        if invitation["status"] != "pending":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Einladung ist nicht mehr offen",
            )

        await _load_team_or_404(db, tournament_id, invitation["team_id"])
        await _ensure_team_has_capacity(db, invitation["team_id"], tournament["team_size"])
        await _ensure_user_not_in_tournament_team(db, tournament_id, user.discord_id)

        cursor = await db.execute(
            "SELECT * FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, user.discord_id),
        )
        signup = await cursor.fetchone()
        if signup:
            steam_id = signup["steam_id"]
            rank = signup["rank"]
            rank_score = signup["rank_score"] or 0
            discord_name = user.discord_name or signup["discord_name"]
        else:
            rank_data = await get_player_rank_profile(user.discord_id)
            steam_id = rank_data.get("steam_id") if rank_data else None
            rank = rank_data.get("rank") if rank_data else None
            rank_score = rank_data.get("rank_score", 0) if rank_data else 0
            discord_name = user.discord_name

        await _add_user_to_team(
            db,
            tournament_id,
            invitation["team_id"],
            discord_id=user.discord_id,
            discord_name=discord_name,
            steam_id=steam_id,
            rank=rank,
            rank_score=rank_score,
        )
        await db.execute(
            "UPDATE team_invitations SET status = 'accepted' WHERE id = ?",
            (invite_id,),
        )
        await db.commit()

    return {"status": "accepted", "invite_id": invite_id}


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/invitations/{invite_id}/reject
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/invitations/{invite_id}/reject", status_code=200)
async def reject_team_invitation(
    tournament_id: int,
    invite_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Lehnt eine Team-Einladung ab."""
    async with get_db() as db:
        await _load_tournament_or_404(db, tournament_id)
        cursor = await db.execute(
            "SELECT * FROM team_invitations WHERE id = ? AND tournament_id = ?",
            (invite_id, tournament_id),
        )
        invitation = await cursor.fetchone()
        if not invitation:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Einladung nicht gefunden",
            )
        if invitation["discord_id"] != user.discord_id:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="Diese Einladung gehört nicht zu dir",
            )
        if invitation["status"] != "pending":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Einladung ist nicht mehr offen",
            )

        await db.execute(
            "UPDATE team_invitations SET status = 'rejected' WHERE id = ?",
            (invite_id,),
        )
        await db.commit()

    return {"status": "rejected", "invite_id": invite_id}


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/teams/{team_id}/apply
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/teams/{team_id}/apply", status_code=200)
async def apply_to_team(
    tournament_id: int,
    team_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Bewirbt den aktuellen User auf ein Team."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_registration_open(tournament)
        team = await _load_team_or_404(db, tournament_id, team_id)
        if team["recruitment_status"] != RecruitmentStatus.application.value:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="Dieses Team nimmt aktuell keine Bewerbungen an",
            )

        await _ensure_user_not_in_tournament_team(db, tournament_id, user.discord_id)

        cursor = await db.execute(
            "SELECT * FROM team_applications WHERE team_id = ? AND discord_id = ?",
            (team_id, user.discord_id),
        )
        existing_application = await cursor.fetchone()
        discord_name = await _resolve_discord_name(
            db,
            user.discord_id,
            preferred_name=user.discord_name,
        )

        if existing_application and existing_application["status"] == "pending":
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Für dieses Team existiert bereits eine offene Bewerbung",
            )

        if existing_application:
            await db.execute(
                "UPDATE team_applications SET discord_name = ?, status = 'pending', created_at = datetime('now') "
                "WHERE id = ?",
                (discord_name, existing_application["id"]),
            )
        else:
            await db.execute(
                "INSERT INTO team_applications (team_id, discord_id, discord_name, status, created_at) "
                "VALUES (?, ?, ?, 'pending', datetime('now'))",
                (team_id, user.discord_id, discord_name),
            )
        await db.commit()

    return {"status": "applied", "team_id": team_id}


# ---------------------------------------------------------------------------
# GET /api/tournaments/{tournament_id}/teams/{team_id}/applications
# ---------------------------------------------------------------------------

@router.get("/tournaments/{tournament_id}/teams/{team_id}/applications", response_model=list[TeamApplication])
async def get_team_applications(
    tournament_id: int,
    team_id: int,
    user: UserSession = Depends(require_auth),
) -> list[TeamApplication]:
    """Gibt alle Bewerbungen eines Teams zurück."""
    async with get_db() as db:
        await _load_tournament_or_404(db, tournament_id)
        team = await _load_team_or_404(db, tournament_id, team_id)
        _ensure_captain_or_mod(user, team)
        cursor = await db.execute(
            "SELECT id, team_id, discord_name, status, created_at "
            "FROM team_applications WHERE team_id = ? ORDER BY created_at DESC",
            (team_id,),
        )
        rows = await cursor.fetchall()
        return [TeamApplication(**dict(row)) for row in rows]


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/accept
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/accept", status_code=200)
async def accept_team_application(
    tournament_id: int,
    team_id: int,
    app_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Nimmt eine Team-Bewerbung an."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_registration_open(tournament)
        team = await _load_team_or_404(db, tournament_id, team_id)
        _ensure_captain_or_mod(user, team)

        cursor = await db.execute(
            "SELECT * FROM team_applications WHERE id = ? AND team_id = ?",
            (app_id, team_id),
        )
        application = await cursor.fetchone()
        if not application:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Bewerbung nicht gefunden",
            )
        if application["status"] != "pending":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Bewerbung ist nicht mehr offen",
            )

        await _ensure_team_has_capacity(db, team_id, tournament["team_size"])
        await _ensure_user_not_in_tournament_team(db, tournament_id, application["discord_id"])

        cursor = await db.execute(
            "SELECT * FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, application["discord_id"]),
        )
        signup = await cursor.fetchone()
        if signup:
            steam_id = signup["steam_id"]
            rank = signup["rank"]
            rank_score = signup["rank_score"] or 0
            discord_name = signup["discord_name"] or application["discord_name"]
        else:
            rank_data = await get_player_rank_profile(application["discord_id"])
            steam_id = rank_data.get("steam_id") if rank_data else None
            rank = rank_data.get("rank") if rank_data else None
            rank_score = rank_data.get("rank_score", 0) if rank_data else 0
            discord_name = application["discord_name"]

        await _add_user_to_team(
            db,
            tournament_id,
            team_id,
            discord_id=application["discord_id"],
            discord_name=discord_name,
            steam_id=steam_id,
            rank=rank,
            rank_score=rank_score,
        )
        await db.execute(
            "UPDATE team_applications SET status = 'accepted' WHERE id = ?",
            (app_id,),
        )
        await db.commit()

    return {"status": "accepted", "application_id": app_id}


# ---------------------------------------------------------------------------
# POST /api/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/reject
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/reject", status_code=200)
async def reject_team_application(
    tournament_id: int,
    team_id: int,
    app_id: int,
    user: UserSession = Depends(require_auth),
) -> dict:
    """Lehnt eine Team-Bewerbung ab."""
    async with get_db() as db:
        await _load_tournament_or_404(db, tournament_id)
        team = await _load_team_or_404(db, tournament_id, team_id)
        _ensure_captain_or_mod(user, team)
        cursor = await db.execute(
            "SELECT * FROM team_applications WHERE id = ? AND team_id = ?",
            (app_id, team_id),
        )
        application = await cursor.fetchone()
        if not application:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Bewerbung nicht gefunden",
            )
        if application["status"] != "pending":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Bewerbung ist nicht mehr offen",
            )

        await db.execute(
            "UPDATE team_applications SET status = 'rejected' WHERE id = ?",
            (app_id,),
        )
        await db.commit()

    return {"status": "rejected", "application_id": app_id}


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
            "SELECT COALESCE("
            "NULLIF(ts.discord_name, ''), "
            "NULLIF(s.discord_name, ''), "
            "NULLIF(tm.discord_name, ''), "
            "'Unbekannt'"
            ") AS discord_name "
            "FROM tournament_checkins tc "
            "LEFT JOIN tournament_signups ts "
            "ON ts.tournament_id = tc.tournament_id AND ts.discord_id = tc.discord_id "
            "LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM sessions "
            "WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s "
            "ON s.discord_id = tc.discord_id "
            "LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM team_members "
            "WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) tm "
            "ON tm.discord_id = tc.discord_id "
            "WHERE tc.tournament_id = ? ORDER BY tc.checked_in_at, tc.id",
            (tournament_id,),
        )
        checked_in_names = [row["discord_name"] for row in await cursor.fetchall()]

    return {
        "total_registered": len(registered_ids),
        "total_checked_in": len(checked_in_names),
        "checked_in_names": checked_in_names,
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

        # Aktualisiertes Team zurückgeben
        cursor = await db.execute("SELECT * FROM teams WHERE id = ?", (team_id,))
        team_row = await cursor.fetchone()
        cursor = await db.execute("SELECT * FROM team_members WHERE team_id = ?", (team_id,))
        members = [TeamMember(**dict(m)) for m in await cursor.fetchall()]

        await db.commit()

    try:
        await notify_users(
            [target_discord_id],
            "team_invite",
            f"Du wurdest zu `{team['name']}` für `{tournament['name']}` eingeladen.",
        )
    except Exception:
        logger.exception(
            "Team invite notification failed (tournament=%s team=%s target=%s)",
            tournament_id,
            team_id,
            target_discord_id,
        )

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
