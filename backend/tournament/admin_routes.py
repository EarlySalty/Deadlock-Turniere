"""Admin/Mod-only Routes — Tournament-Management und Ergebnis-Eintragung."""
from __future__ import annotations

import json
import logging

from fastapi import APIRouter, Depends, HTTPException, Query, status
from pydantic import BaseModel, Field

from auth.permissions import require_admin, require_mod
from config import settings
from db import get_db
from match import manager as match_manager
from match.series_manager import ensure_game_exists, get_series_games, record_game_result
from notifications.discord_notifier import (
    delete_match_channel,
    get_voice_channel_members,
    get_role_members,
    move_users_to_voice_channel,
    notify_users,
)
from match.result_processor import (
    MatchNotFoundError,
    MatchResultError,
    MatchStateError,
    SteamTaskError,
    apply_bracket_match_result,
)
from tournament.engine import (
    _build_seeded_bracket,
    CheckinSnapshotMismatchError,
    VALID_STATUS_TRANSITIONS,
    assign_random_teams,
    determine_tournament_mode,
    finalize_checkin,
    generate_bracket,
    generate_group_matches,
    generate_groups,
)
from tournament.models import (
    ApplicationStatus,
    RecruitmentStatus,
    LobbySettingsPreset,
    Team,
    TeamApplication,
    TeamMember,
    TournamentDetail,
    Tournament,
    TournamentCreate,
    TournamentMode,
    TournamentSignup,
    TournamentUpdate,
    UserSession,
)
from tournament.routes import (
    _enrich_rank_data,
    _load_bracket_matches,
    _load_groups_for_tournament,
    _load_signups_for_tournament,
    _load_teams_for_tournament,
    _preferred_discord_name,
)
from tournament.scheduler import advance_tournament_status

router = APIRouter(prefix="/api/admin", tags=["admin"])
logger = logging.getLogger(__name__)


class VoiceMoveRequest(BaseModel):
    discord_id: str
    channel_id: int


class GameResultRequest(BaseModel):
    winner_team: int = Field(ge=1, le=2)
    duration_s: int | None = Field(default=None, ge=0)


class ManualLobbyCodeRequest(BaseModel):
    party_code: str
    steam_party_id: str | None = None


class CasterAssignRequest(BaseModel):
    discord_id: str


class MatchCasterOut(BaseModel):
    discord_id: str
    display_name: str | None = None
    assigned_at: str | None = None
    assigned_by: str | None = None


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

async def _audit(db, action: str, user_id: str, details: str) -> None:  # noqa: ANN001
    """Schreibt einen Eintrag in den Audit-Log."""
    await db.execute(
        "INSERT INTO audit_log (action, user_id, details) VALUES (?, ?, ?)",
        (action, user_id, details),
    )


async def _load_all_profile_ids(db) -> list[str]:  # noqa: ANN001
    cursor = await db.execute("SELECT DISTINCT discord_id FROM user_profiles")
    rows = await cursor.fetchall()
    return [str(row["discord_id"]) for row in rows if row["discord_id"]]


def _serialize_reminder_offsets(offsets: list[int] | None) -> str:
    cleaned = sorted({int(offset) for offset in (offsets or [1440, 120, 15]) if int(offset) >= 0}, reverse=True)
    return json.dumps(cleaned or [1440, 120, 15])


async def _load_assigned_casters(db, match_type: str, match_id: int) -> list[MatchCasterOut]:  # noqa: ANN001
    display_names: dict[str, str] = {}
    try:
        members = await get_role_members(int(settings.DISCORD_GUILD_ID), settings.DISCORD_CASTER_ROLE_ID)
        for member in members:
            discord_id = str(member.get("user_id") or member.get("id") or "").strip()
            if not discord_id:
                continue
            display_names[discord_id] = str(
                member.get("display_name")
                or member.get("global_name")
                or member.get("username")
                or discord_id
            )
    except Exception:
        logger.exception("Caster-Rollenmitglieder konnten nicht geladen werden")

    cursor = await db.execute(
        "SELECT discord_id, assigned_at, assigned_by FROM match_casters "
        "WHERE match_type = ? AND match_id = ? ORDER BY assigned_at, discord_id",
        (match_type, match_id),
    )
    rows = await cursor.fetchall()
    casters: list[MatchCasterOut] = []
    for row in rows:
        discord_id = str(row["discord_id"])
        casters.append(
            MatchCasterOut(
                discord_id=discord_id,
                display_name=display_names.get(discord_id, discord_id),
                assigned_at=row["assigned_at"],
                assigned_by=row["assigned_by"],
            )
        )
    return casters


async def _ensure_bracket_match_exists(db, tournament_id: int, match_id: int) -> None:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT 1 FROM bracket_matches WHERE id = ? AND tournament_id = ?",
        (match_id, tournament_id),
    )
    if not await cursor.fetchone():
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Match nicht gefunden")


async def _reset_match_record(
    *,
    db,  # noqa: ANN001
    select_sql: str,
    select_params: tuple,
    update_sql: str,
    update_params: tuple,
    match_id: int,
) -> bool:
    cursor = await db.execute(select_sql, select_params)
    row = await cursor.fetchone()
    if not row:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Match nicht gefunden")
    if row["status"] in {"completed", "forfeit", "cancelled"}:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Abgeschlossene oder abgebrochene Matches können nicht zurückgesetzt werden",
        )
    if row["discord_channel_id"]:
        try:
            await delete_match_channel(row["discord_channel_id"])
        except Exception:
            logger.exception("Discord-Channel konnte beim Match-Reset nicht gelöscht werden")
    result = await db.execute(update_sql, update_params)
    return result.rowcount > 0


_LOBBY_SETTINGS_PRESET_MAP: dict[LobbySettingsPreset, dict[str, object] | None] = {
    LobbySettingsPreset.standard: None,
    LobbySettingsPreset.fast_mode: {
        "citadel_enable_fast_cooldowns": 1,
    },
    LobbySettingsPreset.high_damage: {
        "citadel_dps_multiplier": 2,
    },
    # Niedrige Schwerkraft — Spieler fliegen höher und weiter
    LobbySettingsPreset.low_gravity: {
        "sv_gravity": 200,
    },
    # Alle rennen schnell + kurze Cooldowns
    LobbySettingsPreset.speed_mode: {
        "citadel_player_move_speed_scale": 2.0,
        "citadel_enable_fast_cooldowns": 1,
    },
    # Hoher Schaden — jeder stirbt sofort
    LobbySettingsPreset.glass_cannon: {
        "citadel_weapon_damage_multiplier": 5,
        "citadel_dps_multiplier": 3,
        "citadel_melee_damage_scale": 3.0,
    },
    # Alle starten reich — sofort viele Items möglich
    LobbySettingsPreset.rich_start: {
        "citadel_player_starting_gold": 10000,
    },
    # Chaos: alles auf einmal leicht verrückt
    LobbySettingsPreset.chaos_mode: {
        "sv_gravity": 400,
        "citadel_player_move_speed_scale": 1.5,
        "citadel_weapon_damage_multiplier": 2,
        "citadel_enable_fast_cooldowns": 1,
        "citadel_player_starting_gold": 5000,
        "citadel_trooper_gold_reward": 200,
    },
    # Alle spielen denselben Helden
    LobbySettingsPreset.all_same_hero: {
        "citadel_allow_duplicate_heroes": 1,
    },
    # Niemand stirbt
    LobbySettingsPreset.immortal: {
        "citadel_enable_no_hero_death": 1,
    },
}


def _serialize_lobby_settings(
    preset: LobbySettingsPreset,
    custom_settings: dict | None,
) -> str | None:
    if preset == LobbySettingsPreset.custom:
        if custom_settings is None:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Für lobby_settings_preset=custom ist lobby_settings erforderlich",
            )
        if not isinstance(custom_settings, dict):
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="lobby_settings muss ein JSON-Objekt sein",
            )
        try:
            return json.dumps(custom_settings)
        except (TypeError, ValueError) as exc:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="lobby_settings enthält nicht serialisierbare Werte",
            ) from exc

    if custom_settings is not None:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="lobby_settings ist nur mit lobby_settings_preset=custom erlaubt",
        )

    preset_payload = _LOBBY_SETTINGS_PRESET_MAP.get(preset)
    if preset_payload is None:
        return None

    try:
        return json.dumps(preset_payload)
    except (TypeError, ValueError) as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Vordefinierte lobby_settings konnten nicht serialisiert werden",
        ) from exc


ACTIVE_TOURNAMENT_STATUSES = ("draft", "registration", "checkin", "group_phase", "bracket")


async def _ensure_single_active_tournament(
    db,
    *,
    ignore_tournament_id: int | None = None,
) -> None:  # noqa: ANN001
    """Stellt sicher, dass nur ein aktives Turnier existiert."""
    query = (
        "SELECT id, name, status FROM tournaments "
        f"WHERE status IN ({', '.join('?' for _ in ACTIVE_TOURNAMENT_STATUSES)})"
    )
    params: list = list(ACTIVE_TOURNAMENT_STATUSES)
    if ignore_tournament_id is not None:
        query += " AND id != ?"
        params.append(ignore_tournament_id)

    cursor = await db.execute(query, params)
    existing = await cursor.fetchone()
    if existing:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=(
                f"Es gibt bereits ein aktives Turnier: "
                f"#{existing['id']} {existing['name']} ({existing['status']})"
            ),
        )


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


async def _load_team_application_or_404(db, team_id: int, application_id: int):  # noqa: ANN001
    cursor = await db.execute(
        "SELECT * FROM team_applications WHERE id = ? AND team_id = ?",
        (application_id, team_id),
    )
    application = await cursor.fetchone()
    if not application:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Bewerbung nicht gefunden",
        )
    return application


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


async def _load_bracket_match_for_tournament_or_404(
    db,
    tournament_id: int,
    match_id: int,
):  # noqa: ANN001
    cursor = await db.execute(
        "SELECT * FROM bracket_matches WHERE id = ? AND tournament_id = ?",
        (match_id, tournament_id),
    )
    match = await cursor.fetchone()
    if not match:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Bracket-Match nicht gefunden",
        )
    return match


async def _count_team_members(db, team_id: int) -> int:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT COUNT(*) AS cnt FROM team_members WHERE team_id = ?",
        (team_id,),
    )
    row = await cursor.fetchone()
    return int(row["cnt"])


async def _ensure_team_has_capacity(db, team_id: int, team_size: int) -> None:  # noqa: ANN001
    member_count = await _count_team_members(db, team_id)
    if member_count >= team_size:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Team ist bereits voll",
        )


async def _reassign_or_clear_captain(db, team_id: int) -> None:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT discord_id FROM team_members WHERE team_id = ? ORDER BY joined_at LIMIT 1",
        (team_id,),
    )
    next_member = await cursor.fetchone()
    next_captain = next_member["discord_id"] if next_member else ""

    await db.execute(
        "UPDATE team_members SET role = 'member' WHERE team_id = ?",
        (team_id,),
    )
    if next_captain:
        await db.execute(
            "UPDATE team_members SET role = 'captain' WHERE team_id = ? AND discord_id = ?",
            (team_id, next_captain),
        )
    await db.execute(
        "UPDATE teams SET captain_discord_id = ? WHERE id = ?",
        (next_captain, team_id),
    )


async def _upsert_signup_from_member(
    db,
    tournament_id: int,
    member_row,
) -> None:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
        (tournament_id, member_row["discord_id"]),
    )
    existing = await cursor.fetchone()

    if existing:
        await db.execute(
            "UPDATE tournament_signups SET discord_name = ?, steam_id = ?, rank = ?, rank_score = ?, team_id = NULL "
            "WHERE id = ?",
            (
                member_row["discord_name"],
                member_row["steam_id"],
                member_row["rank"],
                member_row["rank_score"],
                existing["id"],
            ),
        )
        return

    await db.execute(
        "INSERT INTO tournament_signups "
        "(tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id) "
        "VALUES (?, ?, ?, ?, ?, ?, NULL)",
        (
            tournament_id,
            member_row["discord_id"],
            member_row["discord_name"],
            member_row["steam_id"],
            member_row["rank"],
            member_row["rank_score"],
        ),
    )


async def _upsert_signup_for_team(
    db,
    tournament_id: int,
    *,
    discord_id: str,
    discord_name: str | None,
    steam_id: str | None,
    rank: str | None,
    rank_score: int,
    team_id: int,
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


async def _ensure_team_not_locked(db, team_id: int) -> None:  # noqa: ANN001
    """Verhindert destruktive Team-Löschung bei bestehender Turnier-Historie."""
    checks = [
        ("group_teams", "team_id"),
        ("group_matches", "team1_id"),
        ("group_matches", "team2_id"),
        ("group_matches", "winner_id"),
        ("bracket_matches", "team1_id"),
        ("bracket_matches", "team2_id"),
        ("bracket_matches", "winner_id"),
        ("match_results", "winning_team"),
        ("checkins", "team_id"),
    ]
    for table_name, column_name in checks:
        cursor = await db.execute(
            f"SELECT 1 FROM {table_name} WHERE {column_name} = ? LIMIT 1",  # noqa: S608
            (team_id,),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Team kann nicht gelöscht werden, weil es bereits in Turnierdaten verwendet wird",
            )


async def _load_team_detail(db, team_id: int):  # noqa: ANN001
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
    members = await cursor.fetchall()
    member_models: list[TeamMember] = []
    for member in members:
        member_data = dict(member)
        member_data["discord_name"] = _preferred_discord_name(
            member_data.pop("profile_display_name", None),
            member_data.pop("session_discord_name", None),
            member_data.pop("team_member_discord_name", None),
            discord_id=member_data["discord_id"],
        )
        member_models.append(TeamMember(**await _enrich_rank_data(member_data)))
    return Team(
        **dict(team_row),
        members=member_models,
    )


async def _delete_tournament_tree(db, tournament_id: int) -> None:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT id FROM groups WHERE tournament_id = ?",
        (tournament_id,),
    )
    group_ids = [row["id"] for row in await cursor.fetchall()]

    if group_ids:
        placeholders = ", ".join("?" for _ in group_ids)
        await db.execute(
            f"DELETE FROM match_results WHERE group_match_id IN ("  # noqa: S608
            f"SELECT id FROM group_matches WHERE group_id IN ({placeholders}))",
            group_ids,
        )
        await db.execute(
            f"DELETE FROM checkins WHERE match_type = 'group' AND match_id IN ("  # noqa: S608
            f"SELECT id FROM group_matches WHERE group_id IN ({placeholders}))",
            group_ids,
        )
        await db.execute(
            f"DELETE FROM group_matches WHERE group_id IN ({placeholders})",  # noqa: S608
            group_ids,
        )
        await db.execute(
            f"DELETE FROM group_teams WHERE group_id IN ({placeholders})",  # noqa: S608
            group_ids,
        )
        await db.execute(
            f"DELETE FROM groups WHERE id IN ({placeholders})",  # noqa: S608
            group_ids,
        )

    await db.execute(
        "DELETE FROM match_results WHERE bracket_match_id IN "
        "(SELECT id FROM bracket_matches WHERE tournament_id = ?)",
        (tournament_id,),
    )
    await db.execute(
        "DELETE FROM checkins WHERE match_type = 'bracket' AND match_id IN "
        "(SELECT id FROM bracket_matches WHERE tournament_id = ?)",
        (tournament_id,),
    )
    await db.execute(
        "DELETE FROM bracket_matches WHERE tournament_id = ?",
        (tournament_id,),
    )
    await db.execute(
        "DELETE FROM team_members WHERE team_id IN (SELECT id FROM teams WHERE tournament_id = ?)",
        (tournament_id,),
    )
    await db.execute(
        "DELETE FROM tournament_checkins WHERE tournament_id = ?",
        (tournament_id,),
    )
    await db.execute(
        "DELETE FROM tournament_signups WHERE tournament_id = ?",
        (tournament_id,),
    )
    await db.execute(
        "DELETE FROM teams WHERE tournament_id = ?",
        (tournament_id,),
    )
    await db.execute(
        "DELETE FROM tournaments WHERE id = ?",
        (tournament_id,),
    )


async def _delete_group_phase_tree(db, tournament_id: int) -> None:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT id FROM groups WHERE tournament_id = ?",
        (tournament_id,),
    )
    group_ids = [row["id"] for row in await cursor.fetchall()]

    if not group_ids:
        return

    placeholders = ", ".join("?" for _ in group_ids)
    await db.execute(
        f"DELETE FROM match_results WHERE group_match_id IN ("  # noqa: S608
        f"SELECT id FROM group_matches WHERE group_id IN ({placeholders}))",
        group_ids,
    )
    await db.execute(
        f"DELETE FROM checkins WHERE match_type = 'group' AND match_id IN ("  # noqa: S608
        f"SELECT id FROM group_matches WHERE group_id IN ({placeholders}))",
        group_ids,
    )
    await db.execute(
        f"DELETE FROM group_matches WHERE group_id IN ({placeholders})",  # noqa: S608
        group_ids,
    )
    await db.execute(
        f"DELETE FROM group_teams WHERE group_id IN ({placeholders})",  # noqa: S608
        group_ids,
    )
    await db.execute(
        f"DELETE FROM groups WHERE id IN ({placeholders})",  # noqa: S608
        group_ids,
    )


async def _group_phase_has_played_matches(db, tournament_id: int) -> bool:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT 1 FROM group_matches gm "
        "JOIN groups g ON gm.group_id = g.id "
        "WHERE g.tournament_id = ? "
        "AND (gm.status != 'pending' OR gm.winner_id IS NOT NULL OR gm.played_at IS NOT NULL) "
        "LIMIT 1",
        (tournament_id,),
    )
    return await cursor.fetchone() is not None


def _ensure_participant_management_allowed(tournament_status: str) -> None:
    if tournament_status not in ACTIVE_TOURNAMENT_STATUSES:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Teilnehmerverwaltung ist nur für aktive Turniere möglich",
        )


@router.get("/tournaments", response_model=list[Tournament])
async def list_tournaments_admin(
    user: UserSession = Depends(require_mod),
) -> list[Tournament]:
    """Alle Turniere für Admin/Mods inklusive Drafts."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments ORDER BY created_at DESC"
        )
        rows = await cursor.fetchall()
    return [Tournament(**dict(r)) for r in rows]


@router.get("/tournaments/{tournament_id}", response_model=TournamentDetail)
async def get_tournament_admin(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> TournamentDetail:
    """Turnier-Detail für Admin/Mods inklusive Drafts."""
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
        signups = await _load_signups_for_tournament(db, tournament_id)

    return TournamentDetail(
        **tournament_data,
        teams=teams,
        groups=groups,
        bracket_matches=bracket_matches,
        signups=signups,
    )


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments — Turnier erstellen
# ---------------------------------------------------------------------------

@router.post("/tournaments", response_model=Tournament, status_code=201)
async def create_tournament(
    body: TournamentCreate,
    user: UserSession = Depends(require_mod),
) -> Tournament:
    """Neues Turnier erstellen (Mod+)."""
    async with get_db() as db:
        await _ensure_single_active_tournament(db)
        profile_ids = await _load_all_profile_ids(db)
        lobby_settings = _serialize_lobby_settings(
            body.lobby_settings_preset,
            body.lobby_settings,
        )
        # Auto Tournament Mode bestimmen: >= 12 Teams = group_stage, else = bracket_only
        # Admin kann mit force_tournament_mode überschreiben
        tournament_mode = determine_tournament_mode(
            team_count=body.team_size,  # Fallback zur Team-Größe, nicht ideal aber praktisch
            force_mode=body.force_tournament_mode,
        )
        cursor = await db.execute(
            "INSERT INTO tournaments "
            "(name, description, team_size, bracket_format, registration_start, "
            "registration_end, checkin_start, group_phase_start, bracket_start, "
            "created_by, invite_mode, invite_window_start, invite_window_end, tournament_mode, "
            "exclude_from_leaderboard, reminder_offsets) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                body.name,
                body.description,
                body.team_size,
                body.bracket_format.value,
                body.registration_start,
                body.registration_end,
                body.checkin_start,
                body.group_phase_start,
                body.bracket_start,
                user.discord_id,
                body.invite_mode.value,
                body.invite_window_start,
                body.invite_window_end,
                tournament_mode.value,
                1 if body.exclude_from_leaderboard else 0,
                _serialize_reminder_offsets(body.reminder_offsets),
            ),
        )
        await db.execute(
            "UPDATE tournaments SET lobby_settings = ? WHERE id = ?",
            (lobby_settings, cursor.lastrowid),
        )
        tournament_id = cursor.lastrowid

        await _audit(
            db,
            "tournament_create",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "name": body.name}),
        )
        await db.commit()

        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        row = await cursor.fetchone()

    try:
        await notify_users(
            profile_ids,
            "tournament_news",
            f"Ein neues Turnier wurde angelegt: `{body.name}`.",
        )
    except Exception:
        logger.exception("Tournament news notification failed for tournament %s", tournament_id)

    return Tournament(**dict(row))


# ---------------------------------------------------------------------------
# PUT /api/admin/tournaments/{id} — Turnier bearbeiten
# ---------------------------------------------------------------------------

@router.put("/tournaments/{tournament_id}", response_model=Tournament)
async def update_tournament(
    tournament_id: int,
    body: TournamentUpdate,
    user: UserSession = Depends(require_mod),
) -> Tournament:
    """Turnier-Daten aktualisieren (Mod+). Status-Übergänge werden validiert."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        existing = await cursor.fetchone()
        if not existing:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )

        # Status-Übergang validieren
        if body.status is not None and body.status.value != existing["status"]:
            current_status = existing["status"]
            allowed = VALID_STATUS_TRANSITIONS.get(current_status, [])
            if body.status.value not in allowed:
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail=f"Ungültiger Status-Übergang: {current_status} -> {body.status.value}. "
                    f"Erlaubt: {', '.join(allowed) if allowed else 'keine'}",
                )
            if current_status == "registration" and body.status.value == "checkin":
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="Check-in bitte über den dedizierten Endpoint öffnen",
                )
            if current_status == "checkin" and body.status.value == "group_phase":
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="Check-in bitte über finalize-checkin abschließen",
                )
            if body.status.value in ACTIVE_TOURNAMENT_STATUSES:
                await _ensure_single_active_tournament(
                    db,
                    ignore_tournament_id=tournament_id,
                )

        # Nur gesetzte Felder updaten
        update_data = body.model_dump(exclude_unset=True)

        mode_changed_to_bracket_only = False

        # Tournament Mode: in Draft immer, in Check-in oder ungespielter Gruppenphase ebenfalls
        if "force_tournament_mode" in update_data:
            if existing["status"] not in {"draft", "checkin", "group_phase"}:
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="Turnier-Modus kann nur in Draft-, Check-in- oder ungespielter Gruppenphase geändert werden",
                )
            if (
                existing["status"] == "group_phase"
                and await _group_phase_has_played_matches(db, tournament_id)
            ):
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="Turnier-Modus kann nach Start der Gruppenmatches nicht mehr geändert werden",
                )
            # force_tournament_mode → tournament_mode (DB-Spalte)
            force_mode = update_data.pop("force_tournament_mode")
            if force_mode is not None:
                update_data["tournament_mode"] = force_mode.value
                mode_changed_to_bracket_only = (
                    force_mode.value == TournamentMode.bracket_only.value
                    and existing["tournament_mode"] != TournamentMode.bracket_only.value
                )

        if body.invite_mode is not None:
            update_data["invite_mode"] = body.invite_mode.value
        if body.exclude_from_leaderboard is not None:
            update_data["exclude_from_leaderboard"] = 1 if body.exclude_from_leaderboard else 0
        if body.reminder_offsets is not None:
            update_data["reminder_offsets"] = _serialize_reminder_offsets(body.reminder_offsets)
        if "invite_window_start" in body.model_fields_set:
            update_data["invite_window_start"] = body.invite_window_start
        if "invite_window_end" in body.model_fields_set:
            update_data["invite_window_end"] = body.invite_window_end
        if "lobby_settings_preset" in body.model_fields_set or "lobby_settings" in body.model_fields_set:
            preset = body.lobby_settings_preset
            custom_settings = body.lobby_settings
            if preset is None and custom_settings is not None:
                preset = LobbySettingsPreset.custom
            if preset is not None:
                update_data["lobby_settings"] = _serialize_lobby_settings(preset, custom_settings)
            elif custom_settings is not None:
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="lobby_settings_preset ist erforderlich, wenn lobby_settings gesetzt wird",
                )

        updates: list[str] = []
        params: list = []
        for field, value in update_data.items():
            if hasattr(value, "value"):
                value = value.value
            updates.append(f"{field} = ?")
            params.append(value)

        if not updates:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Keine Änderungen angegeben",
            )

        updates.append("updated_at = datetime('now')")
        params.append(tournament_id)

        await db.execute(
            f"UPDATE tournaments SET {', '.join(updates)} WHERE id = ?",  # noqa: S608
            params,
        )

        if existing["status"] == "group_phase" and mode_changed_to_bracket_only:
            await _delete_group_phase_tree(db, tournament_id)
            await db.execute(
                "DELETE FROM bracket_matches WHERE tournament_id = ?",
                (tournament_id,),
            )
            cursor = await db.execute(
                "SELECT id FROM teams WHERE tournament_id = ? ORDER BY created_at, id",
                (tournament_id,),
            )
            seeded_entries = [{"team_id": row["id"]} for row in await cursor.fetchall()]
            await _build_seeded_bracket(db, tournament_id, seeded_entries)
            await db.execute(
                "UPDATE tournaments SET status = 'bracket', updated_at = datetime('now') WHERE id = ?",
                (tournament_id,),
            )

        await _audit(
            db,
            "tournament_update",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "changes": update_data}, default=str),
        )
        await db.commit()

        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        row = await cursor.fetchone()

    return Tournament(**dict(row))


# ---------------------------------------------------------------------------
# DELETE /api/admin/tournaments/{id} — Turnier löschen
# ---------------------------------------------------------------------------

@router.delete("/tournaments/{tournament_id}", status_code=200)
async def delete_tournament(
    tournament_id: int,
    user: UserSession = Depends(require_admin),
) -> dict:
    """Turnier löschen (Admin only)."""
    async with get_db() as db:
        existing = await _load_tournament_or_404(db, tournament_id)

        await _delete_tournament_tree(db, tournament_id)

        await _audit(
            db,
            "tournament_delete",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "name": existing["name"]}),
        )
        await db.commit()

    return {"status": "gelöscht", "tournament_id": tournament_id}


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments/{id}/open-checkin — Check-in öffnen
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/open-checkin", response_model=Tournament)
async def open_checkin(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> Tournament:
    """Öffnet die Check-in-Phase manuell."""
    async with get_db() as db:
        existing = await _load_tournament_or_404(db, tournament_id)
        if existing["status"] != "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Check-in kann nur aus der Registration geöffnet werden",
            )
        cursor = await db.execute(
            "SELECT COUNT(*) AS cnt FROM teams WHERE tournament_id = ?",
            (tournament_id,),
        )
        team_count_row = await cursor.fetchone()
        if int(team_count_row["cnt"]) == 0:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Check-in kann erst geöffnet werden, wenn mindestens ein Team existiert",
            )

    try:
        await advance_tournament_status(
            tournament_id,
            current_status="registration",
            next_status="checkin",
            source="manual",
            actor_id=user.discord_id,
        )
    except ValueError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(exc),
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=str(exc),
        ) from exc

    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        row = await cursor.fetchone()

    return Tournament(**dict(row))


@router.post("/tournaments/{tournament_id}/revert-checkin", response_model=Tournament)
async def revert_checkin(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> Tournament:
    """Setzt die Check-in-Phase zurück auf Registration."""
    async with get_db() as db:
        existing = await _load_tournament_or_404(db, tournament_id)
        if existing["status"] != "checkin":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Check-in kann nur aus der Check-in-Phase zurückgesetzt werden",
            )

        await db.execute(
            "DELETE FROM tournament_checkins WHERE tournament_id = ?",
            (tournament_id,),
        )
        await db.execute(
            "UPDATE tournaments "
            "SET status = ?, registration_end = NULL, checkin_start = NULL, updated_at = datetime('now') "
            "WHERE id = ?",
            ("registration", tournament_id),
        )
        await _audit(
            db,
            "tournament_revert_checkin",
            user.discord_id,
            json.dumps({
                "tournament_id": tournament_id,
                "from_status": "checkin",
                "to_status": "registration",
                "cleared_schedule_fields": ["registration_end", "checkin_start"],
            }),
        )
        await db.commit()

        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        row = await cursor.fetchone()

    return Tournament(**dict(row))


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments/{id}/finalize-checkin — Check-in abschließen
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/finalize-checkin", status_code=200)
async def finalize_checkin_endpoint(
    tournament_id: int,
    body: dict | None = None,
    confirm: bool = Query(False),
    user: UserSession = Depends(require_mod),
) -> dict:
    """Bereinigt Teams anhand der Check-ins und startet optional die Gruppenphase."""
    allowed_team_ids = set()
    snapshot_token = None
    if body and isinstance(body.get("allowed_team_ids"), list):
        allowed_team_ids = {
            int(team_id)
            for team_id in body["allowed_team_ids"]
            if isinstance(team_id, int) or (isinstance(team_id, str) and team_id.isdigit())
        }
    if body and isinstance(body.get("snapshot_token"), str):
        snapshot_token = body["snapshot_token"].strip() or None

    try:
        result = await finalize_checkin(
            tournament_id,
            confirm=confirm,
            allowed_team_ids=allowed_team_ids,
            actor_id=user.discord_id,
            expected_snapshot_token=snapshot_token,
            advance_to_group_phase=confirm,
        )
    except ValueError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(exc),
        ) from exc
    except CheckinSnapshotMismatchError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=str(exc),
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=str(exc),
        ) from exc

    return result


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments/{id}/advance — Phase weiterschalten
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/advance", response_model=Tournament)
async def advance_tournament(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> Tournament:
    """Turnier zur nächsten Phase weiterschalten (Mod+)."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        existing = await cursor.fetchone()
        if not existing:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )

        current_status = existing["status"]
        if current_status == "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Check-in bitte über open-checkin öffnen",
            )
        if current_status == "checkin":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Check-in bitte über finalize-checkin abschließen",
            )
        allowed = VALID_STATUS_TRANSITIONS.get(current_status, [])
        if not allowed:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail=f"Keine weitere Phase möglich (aktuell: {current_status})",
            )

        next_status = allowed[0]
        if next_status in ACTIVE_TOURNAMENT_STATUSES:
            await _ensure_single_active_tournament(
                db,
                ignore_tournament_id=tournament_id,
            )

    try:
        await advance_tournament_status(
            tournament_id,
            current_status=current_status,
            next_status=next_status,
            source="manual",
            actor_id=user.discord_id,
        )
    except ValueError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(exc),
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=str(exc),
        ) from exc

    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        row = await cursor.fetchone()

    return Tournament(**dict(row))


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments/{id}/assign-random — Solo-Spieler verteilen
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/assign-random", status_code=200)
async def assign_random(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Solo-Anmeldungen zufällig auf Teams verteilen (Mod+)."""
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

        if tournament["status"] != "registration":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Team-Zuweisung nur während der Registration möglich",
            )

    teams_created = await assign_random_teams(tournament_id, tournament["team_size"])
    return {"status": "ok", "teams_created": teams_created}


# ---------------------------------------------------------------------------
# Team- und Teilnehmer-Verwaltung
# ---------------------------------------------------------------------------


@router.post("/tournaments/{tournament_id}/teams", response_model=Team, status_code=201)
async def create_team_admin(
    tournament_id: int,
    body: dict,
    user: UserSession = Depends(require_mod),
) -> Team:
    """Leeres Team für Admin-Verwaltung anlegen."""
    name = (body.get("name") or "").strip()
    if len(name) < 2 or len(name) > 32:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Team-Name muss zwischen 2 und 32 Zeichen lang sein",
        )

    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        name_key = name.casefold()
        cursor = await db.execute(
            "SELECT id FROM teams WHERE tournament_id = ? AND name_key = ?",
            (tournament_id, name_key),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Ein Team mit diesem Namen existiert bereits",
            )

        cursor = await db.execute(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, '')",
            (tournament_id, name, name_key),
        )
        team_id = cursor.lastrowid
        await _audit(
            db,
            "team_create_admin",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "team_id": team_id, "name": name}),
        )
        await db.commit()
        return await _load_team_detail(db, team_id)


@router.put("/tournaments/{tournament_id}/teams/{team_id}", response_model=Team)
async def rename_team_admin(
    tournament_id: int,
    team_id: int,
    body: dict,
    user: UserSession = Depends(require_mod),
) -> Team:
    """Team umbenennen."""
    name = (body.get("name") or "").strip()
    if len(name) < 2 or len(name) > 32:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Team-Name muss zwischen 2 und 32 Zeichen lang sein",
        )

    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        await _load_team_or_404(db, tournament_id, team_id)
        name_key = name.casefold()
        cursor = await db.execute(
            "SELECT id FROM teams WHERE tournament_id = ? AND name_key = ? AND id != ?",
            (tournament_id, name_key, team_id),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Ein Team mit diesem Namen existiert bereits",
            )

        await db.execute(
            "UPDATE teams SET name = ?, name_key = ? WHERE id = ?",
            (name, name_key, team_id),
        )
        await _audit(
            db,
            "team_rename_admin",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "team_id": team_id, "name": name}),
        )
        await db.commit()
        return await _load_team_detail(db, team_id)


@router.patch("/tournaments/{tournament_id}/teams/{team_id}/recruiting", status_code=200)
async def update_team_recruitment_status_admin(
    tournament_id: int,
    team_id: int,
    body: dict,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Recruiting-Status eines Teams setzen."""
    raw_status = body.get("recruitment_status")
    if not isinstance(raw_status, str):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="recruitment_status ist erforderlich",
        )

    try:
        recruitment_status = RecruitmentStatus(raw_status)
    except ValueError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Ungültiger recruitment_status",
        ) from exc

    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        await _load_team_or_404(db, tournament_id, team_id)

        await db.execute(
            "UPDATE teams SET recruitment_status = ? WHERE id = ?",
            (recruitment_status.value, team_id),
        )
        await _audit(
            db,
            "team_recruitment_status_admin",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "team_id": team_id,
                    "recruitment_status": recruitment_status.value,
                }
            ),
        )
        await db.commit()

    return {
        "status": "ok",
        "team_id": team_id,
        "recruitment_status": recruitment_status.value,
    }


@router.get(
    "/tournaments/{tournament_id}/teams/{team_id}/applications",
    response_model=list[TeamApplication],
)
async def list_team_applications_admin(
    tournament_id: int,
    team_id: int,
    user: UserSession = Depends(require_mod),
) -> list[TeamApplication]:
    """Alle Bewerbungen für ein Team laden."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        await _load_team_or_404(db, tournament_id, team_id)

        cursor = await db.execute(
            "SELECT id, team_id, discord_name, status, created_at "
            "FROM team_applications WHERE team_id = ? "
            "ORDER BY created_at DESC, id DESC",
            (team_id,),
        )
        rows = await cursor.fetchall()

    return [TeamApplication(**dict(row)) for row in rows]


@router.post(
    "/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/accept",
    status_code=200,
)
async def accept_team_application_admin(
    tournament_id: int,
    team_id: int,
    app_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Nimmt eine Team-Bewerbung an und fügt den Spieler dem Team hinzu."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        target_team = await _load_team_or_404(db, tournament_id, team_id)
        application = await _load_team_application_or_404(db, team_id, app_id)

        if application["status"] != ApplicationStatus.pending.value:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Nur ausstehende Bewerbungen können angenommen werden",
            )

        await _ensure_team_has_capacity(db, team_id, tournament["team_size"])

        cursor = await db.execute(
            "SELECT 1 FROM team_members tm "
            "JOIN teams t ON tm.team_id = t.id "
            "WHERE t.tournament_id = ? AND tm.discord_id = ?",
            (tournament_id, application["discord_id"]),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Spieler ist bereits Mitglied in einem Team dieses Turniers",
            )

        cursor = await db.execute(
            "SELECT discord_name, steam_id, rank, rank_score, team_id "
            "FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, application["discord_id"]),
        )
        signup = await cursor.fetchone()
        if signup and signup["team_id"] not in (None, team_id):
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Spieler ist bereits einem anderen Team zugeordnet",
            )

        target_member_count = await _count_team_members(db, team_id)
        role = "captain" if target_member_count == 0 or not target_team["captain_discord_id"] else "member"
        effective_name = application["discord_name"]
        if signup and signup["discord_name"]:
            effective_name = signup["discord_name"]
        steam_id = signup["steam_id"] if signup else None
        rank = signup["rank"] if signup else None
        rank_score = int(signup["rank_score"] or 0) if signup else 0

        await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                team_id,
                application["discord_id"],
                effective_name,
                steam_id,
                rank,
                rank_score,
                role,
            ),
        )
        await _upsert_signup_for_team(
            db,
            tournament_id,
            discord_id=application["discord_id"],
            discord_name=effective_name,
            steam_id=steam_id,
            rank=rank,
            rank_score=rank_score,
            team_id=team_id,
        )
        if role == "captain":
            await db.execute(
                "UPDATE teams SET captain_discord_id = ? WHERE id = ?",
                (application["discord_id"], team_id),
            )

        await db.execute(
            "UPDATE team_applications SET status = ? WHERE id = ?",
            (ApplicationStatus.accepted.value, app_id),
        )
        await _audit(
            db,
            "team_application_accept_admin",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "team_id": team_id,
                    "application_id": app_id,
                    "discord_id": application["discord_id"],
                }
            ),
        )
        await db.commit()

    return {
        "status": ApplicationStatus.accepted.value,
        "application_id": app_id,
        "team_id": team_id,
    }


@router.post(
    "/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/reject",
    status_code=200,
)
async def reject_team_application_admin(
    tournament_id: int,
    team_id: int,
    app_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Lehnt eine Team-Bewerbung ab."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        await _load_team_or_404(db, tournament_id, team_id)
        application = await _load_team_application_or_404(db, team_id, app_id)

        if application["status"] != ApplicationStatus.pending.value:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Nur ausstehende Bewerbungen können abgelehnt werden",
            )

        await db.execute(
            "UPDATE team_applications SET status = ? WHERE id = ?",
            (ApplicationStatus.rejected.value, app_id),
        )
        await _audit(
            db,
            "team_application_reject_admin",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "team_id": team_id,
                    "application_id": app_id,
                    "discord_id": application["discord_id"],
                }
            ),
        )
        await db.commit()

    return {
        "status": ApplicationStatus.rejected.value,
        "application_id": app_id,
        "team_id": team_id,
    }


@router.delete("/tournaments/{tournament_id}/teams/{team_id}", status_code=200)
async def delete_team_admin(
    tournament_id: int,
    team_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Team löschen, solange noch keine Turnier-Historie daran hängt."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        team = await _load_team_or_404(db, tournament_id, team_id)
        await _ensure_team_not_locked(db, team_id)

        cursor = await db.execute(
            "SELECT * FROM team_members WHERE team_id = ? ORDER BY joined_at",
            (team_id,),
        )
        members = await cursor.fetchall()
        for member in members:
            await _upsert_signup_from_member(db, tournament_id, member)

        await db.execute(
            "UPDATE tournament_signups SET team_id = NULL WHERE tournament_id = ? AND team_id = ?",
            (tournament_id, team_id),
        )
        await db.execute("DELETE FROM team_members WHERE team_id = ?", (team_id,))
        await db.execute("DELETE FROM teams WHERE id = ?", (team_id,))

        await _audit(
            db,
            "team_delete_admin",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "team_id": team_id, "name": team["name"]}),
        )
        await db.commit()

    return {"status": "gelöscht", "team_id": team_id}


@router.put("/tournaments/{tournament_id}/teams/{team_id}/captain", response_model=Team)
async def change_team_captain_admin(
    tournament_id: int,
    team_id: int,
    body: dict,
    user: UserSession = Depends(require_mod),
) -> Team:
    """Captain innerhalb eines Teams wechseln."""
    discord_id = (body.get("discord_id") or "").strip()
    if not discord_id:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="discord_id ist erforderlich",
        )

    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        await _load_team_or_404(db, tournament_id, team_id)
        cursor = await db.execute(
            "SELECT 1 FROM team_members WHERE team_id = ? AND discord_id = ?",
            (team_id, discord_id),
        )
        if not await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Mitglied nicht im Team gefunden",
            )

        await db.execute("UPDATE team_members SET role = 'member' WHERE team_id = ?", (team_id,))
        await db.execute(
            "UPDATE team_members SET role = 'captain' WHERE team_id = ? AND discord_id = ?",
            (team_id, discord_id),
        )
        await db.execute(
            "UPDATE teams SET captain_discord_id = ? WHERE id = ?",
            (discord_id, team_id),
        )
        await _audit(
            db,
            "team_change_captain_admin",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "team_id": team_id, "discord_id": discord_id}),
        )
        await db.commit()
        return await _load_team_detail(db, team_id)


@router.delete("/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}", response_model=Team)
async def remove_team_member_admin(
    tournament_id: int,
    team_id: int,
    discord_id: str,
    user: UserSession = Depends(require_mod),
) -> Team:
    """Spieler aus Team entfernen und als Solo-Signup zurücklegen."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        team = await _load_team_or_404(db, tournament_id, team_id)

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

        await _upsert_signup_from_member(db, tournament_id, member)
        await db.execute(
            "DELETE FROM team_members WHERE team_id = ? AND discord_id = ?",
            (team_id, discord_id),
        )
        if team["captain_discord_id"] == discord_id:
            await _reassign_or_clear_captain(db, team_id)

        await _audit(
            db,
            "team_remove_member_admin",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "team_id": team_id, "discord_id": discord_id}),
        )
        await db.commit()
        return await _load_team_detail(db, team_id)


@router.post("/tournaments/{tournament_id}/teams/{team_id}/members/move", response_model=Team)
async def move_team_member_admin(
    tournament_id: int,
    team_id: int,
    body: dict,
    user: UserSession = Depends(require_mod),
) -> Team:
    """Spieler zwischen Teams verschieben."""
    from_team_id = body.get("from_team_id")
    discord_id = (body.get("discord_id") or "").strip()
    if not isinstance(from_team_id, int) or not discord_id:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="from_team_id und discord_id sind erforderlich",
        )
    if from_team_id == team_id:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Quelle und Ziel dürfen nicht identisch sein",
        )

    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        source_team = await _load_team_or_404(db, tournament_id, from_team_id)
        target_team = await _load_team_or_404(db, tournament_id, team_id)
        await _ensure_team_has_capacity(db, team_id, tournament["team_size"])

        cursor = await db.execute(
            "SELECT * FROM team_members WHERE team_id = ? AND discord_id = ?",
            (from_team_id, discord_id),
        )
        member = await cursor.fetchone()
        if not member:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Mitglied nicht im Quell-Team gefunden",
            )

        cursor = await db.execute(
            "SELECT 1 FROM team_members WHERE team_id = ? AND discord_id = ?",
            (team_id, discord_id),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Spieler ist bereits im Ziel-Team",
            )

        target_member_count = await _count_team_members(db, team_id)
        new_role = "captain" if target_member_count == 0 or not target_team["captain_discord_id"] else "member"
        await db.execute(
            "UPDATE team_members SET team_id = ?, role = ? WHERE team_id = ? AND discord_id = ?",
            (team_id, new_role, from_team_id, discord_id),
        )
        await db.execute(
            "UPDATE tournament_signups SET team_id = ? WHERE tournament_id = ? AND discord_id = ?",
            (team_id, tournament_id, discord_id),
        )

        if new_role == "captain":
            await db.execute(
                "UPDATE teams SET captain_discord_id = ? WHERE id = ?",
                (discord_id, team_id),
            )

        if source_team["captain_discord_id"] == discord_id:
            await _reassign_or_clear_captain(db, from_team_id)

        await _audit(
            db,
            "team_move_member_admin",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "discord_id": discord_id,
                    "from_team_id": from_team_id,
                    "to_team_id": team_id,
                }
            ),
        )
        await db.commit()
        return await _load_team_detail(db, team_id)


@router.post("/tournaments/{tournament_id}/teams/{team_id}/signups/assign", response_model=Team)
async def assign_signup_to_team_admin(
    tournament_id: int,
    team_id: int,
    body: dict,
    user: UserSession = Depends(require_mod),
) -> Team:
    """Solo-Signup einem Team zuweisen."""
    signup_id = body.get("signup_id")
    if not isinstance(signup_id, int):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="signup_id ist erforderlich",
        )

    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
        target_team = await _load_team_or_404(db, tournament_id, team_id)
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
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Signup ist bereits einem Team zugewiesen",
            )

        cursor = await db.execute(
            "SELECT 1 FROM team_members tm JOIN teams t ON tm.team_id = t.id "
            "WHERE t.tournament_id = ? AND tm.discord_id = ?",
            (tournament_id, signup["discord_id"]),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Spieler ist bereits Mitglied in einem Team dieses Turniers",
            )

        target_member_count = await _count_team_members(db, team_id)
        role = "captain" if target_member_count == 0 or not target_team["captain_discord_id"] else "member"
        await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                team_id,
                signup["discord_id"],
                signup["discord_name"],
                signup["steam_id"],
                signup["rank"],
                signup["rank_score"],
                role,
            ),
        )
        await db.execute(
            "UPDATE tournament_signups SET team_id = ? WHERE id = ?",
            (team_id, signup_id),
        )
        if role == "captain":
            await db.execute(
                "UPDATE teams SET captain_discord_id = ? WHERE id = ?",
                (signup["discord_id"], team_id),
            )

        await _audit(
            db,
            "team_assign_signup_admin",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "team_id": team_id, "signup_id": signup_id}),
        )
        await db.commit()
        return await _load_team_detail(db, team_id)


@router.post("/tournaments/{tournament_id}/teams/{team_id}/add-member", response_model=Team)
async def add_team_member_admin(
    tournament_id: int,
    team_id: int,
    body: dict,
    user: UserSession = Depends(require_admin),
) -> Team:
    """Fügt einen Ersatzspieler direkt einem Team hinzu."""
    discord_id = (body.get("discord_id") or "").strip()
    discord_name = (body.get("discord_name") or "").strip()
    if not discord_id or not discord_name:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="discord_id und discord_name sind erforderlich",
        )

    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        if tournament["status"] not in {"group_phase", "bracket"}:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Ersatzspieler können erst ab der Gruppenphase hinzugefügt werden",
            )

        target_team = await _load_team_or_404(db, tournament_id, team_id)
        await _ensure_team_has_capacity(db, team_id, tournament["team_size"])

        cursor = await db.execute(
            "SELECT 1 FROM team_members tm JOIN teams t ON tm.team_id = t.id "
            "WHERE t.tournament_id = ? AND tm.discord_id = ?",
            (tournament_id, discord_id),
        )
        if await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Spieler ist bereits Mitglied in einem Team dieses Turniers",
            )

        cursor = await db.execute(
            "SELECT discord_name, steam_id, rank, rank_score, team_id "
            "FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
            (tournament_id, discord_id),
        )
        existing_signup = await cursor.fetchone()
        if existing_signup and existing_signup["team_id"] not in (None, team_id):
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Spieler ist bereits einem anderen Team zugeordnet",
            )

        target_member_count = await _count_team_members(db, team_id)
        role = "captain" if target_member_count == 0 or not target_team["captain_discord_id"] else "member"
        steam_id = existing_signup["steam_id"] if existing_signup else None
        rank = existing_signup["rank"] if existing_signup else None
        rank_score = int(existing_signup["rank_score"] or 0) if existing_signup else 0
        effective_name = discord_name or (
            existing_signup["discord_name"] if existing_signup else discord_id
        )

        await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                team_id,
                discord_id,
                effective_name,
                steam_id,
                rank,
                rank_score,
                role,
            ),
        )
        await _upsert_signup_for_team(
            db,
            tournament_id,
            discord_id=discord_id,
            discord_name=effective_name,
            steam_id=steam_id,
            rank=rank,
            rank_score=rank_score,
            team_id=team_id,
        )
        if role == "captain":
            await db.execute(
                "UPDATE teams SET captain_discord_id = ? WHERE id = ?",
                (discord_id, team_id),
            )

        await _audit(
            db,
            "team_add_member_admin",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "team_id": team_id,
                    "discord_id": discord_id,
                }
            ),
        )
        await db.commit()
        return await _load_team_detail(db, team_id)


@router.delete("/tournaments/{tournament_id}/signups/{signup_id}", response_model=TournamentSignup)
async def delete_signup_admin(
    tournament_id: int,
    signup_id: int,
    user: UserSession = Depends(require_mod),
) -> TournamentSignup:
    """Solo-Signup löschen."""
    async with get_db() as db:
        tournament = await _load_tournament_or_404(db, tournament_id)
        _ensure_participant_management_allowed(tournament["status"])
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
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Nur nicht zugewiesene Solo-Signups können gelöscht werden",
            )

        await db.execute("DELETE FROM tournament_signups WHERE id = ?", (signup_id,))
        await _audit(
            db,
            "signup_delete_admin",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "signup_id": signup_id, "discord_id": signup["discord_id"]}),
        )
        await db.commit()
        return TournamentSignup(**dict(signup))


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments/{id}/matches/{match_id}/result — Ergebnis
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/matches/{match_id}/result", status_code=200)
async def set_match_result(
    tournament_id: int,
    match_id: int,
    body: dict,
    force: bool = Query(False),
    user: UserSession = Depends(require_mod),
) -> dict:
    """Manuelles Match-Ergebnis eintragen (Mod+)."""
    winner_id = body.get("winner_id")
    if winner_id is None or not isinstance(winner_id, int):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="winner_id (int) ist erforderlich",
        )

    async with get_db() as db:
        # Turnier prüfen
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        if not await cursor.fetchone():
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Turnier nicht gefunden",
            )

        # Zuerst in bracket_matches suchen
        cursor = await db.execute(
            "SELECT * FROM bracket_matches WHERE id = ? AND tournament_id = ?",
            (match_id, tournament_id),
        )
        bracket_match = await cursor.fetchone()

        if bracket_match:
            try:
                result = await apply_bracket_match_result(
                    tournament_id,
                    match_id,
                    winner_id=winner_id,
                    source="manual",
                    force=force,
                )
            except MatchNotFoundError as exc:
                raise HTTPException(
                    status_code=status.HTTP_404_NOT_FOUND,
                    detail=str(exc),
                ) from exc
            except MatchStateError as exc:
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail=str(exc),
                ) from exc
            except MatchResultError as exc:
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail=str(exc),
                ) from exc

            await _audit(
                db,
                "match_result_bracket",
                user.discord_id,
                json.dumps(
                    {
                        "tournament_id": tournament_id,
                        "match_id": match_id,
                        "winner_id": result["winner_id"],
                        "winning_team": result["winning_team"],
                        "source": "manual",
                    }
                ),
            )
            await db.commit()

            return {
                "status": "ok",
                "match_type": "bracket",
                "match_id": result["match_id"],
                "winner_id": result["winner_id"],
                "winning_team": result["winning_team"],
            }

        # Dann in group_matches suchen
        cursor = await db.execute(
            "SELECT gm.* FROM group_matches gm "
            "JOIN groups g ON gm.group_id = g.id "
            "WHERE gm.id = ? AND g.tournament_id = ?",
            (match_id, tournament_id),
        )
        group_match = await cursor.fetchone()

        if group_match:
            if group_match["status"] in {"completed", "cancelled", "forfeit"}:
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail=f"Group-Match {match_id} kann aus Status {group_match['status']} nicht verarbeitet werden",
                )
            if winner_id not in (group_match["team1_id"], group_match["team2_id"]):
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="winner_id muss eines der beiden Teams im Match sein",
                )

            await db.execute(
                "UPDATE group_matches SET winner_id = ?, status = 'completed', "
                "played_at = datetime('now') WHERE id = ?",
                (winner_id, match_id),
            )

            # Gruppen-Standings updaten
            loser_id = (
                group_match["team2_id"]
                if winner_id == group_match["team1_id"]
                else group_match["team1_id"]
            )
            await db.execute(
                "UPDATE group_teams SET wins = wins + 1, points = points + 3 "
                "WHERE group_id = ? AND team_id = ?",
                (group_match["group_id"], winner_id),
            )
            await db.execute(
                "UPDATE group_teams SET losses = losses + 1 "
                "WHERE group_id = ? AND team_id = ?",
                (group_match["group_id"], loser_id),
            )

            # Match-Result erstellen
            await db.execute(
                "INSERT INTO match_results (group_match_id, winning_team, source) "
                "VALUES (?, ?, 'manual')",
                (match_id, winner_id),
            )

            await _audit(
                db,
                "match_result_group",
                user.discord_id,
                json.dumps({
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "winner_id": winner_id,
                    "group_id": group_match["group_id"],
                }),
            )
            await db.commit()
            return {"status": "ok", "match_type": "group", "match_id": match_id, "winner_id": winner_id}

        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Match nicht gefunden",
        )


@router.post("/tournaments/{tournament_id}/matches/{match_id}/games/{game_number}/start", status_code=200)
async def start_series_game(
    tournament_id: int,
    match_id: int,
    game_number: int,
    user: UserSession = Depends(require_admin),
) -> dict:
    """Stellt sicher dass Spiel N existiert und gibt aktuelle Spiel-IDs zurück."""
    del user
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT id, status FROM bracket_matches WHERE id = ? AND tournament_id = ?",
            (match_id, tournament_id),
        )
        bm = await cursor.fetchone()
    if not bm:
        raise HTTPException(status_code=404, detail="Match nicht gefunden oder gehört nicht zu diesem Turnier")
    if bm["status"] in ("completed", "forfeit", "cancelled"):
        raise HTTPException(status_code=400, detail="Match bereits abgeschlossen")

    game_id = await ensure_game_exists(match_id, game_number)
    games = await get_series_games(match_id)
    return {"game_id": game_id, "game_number": game_number, "games": games}


@router.post("/tournaments/{tournament_id}/matches/{match_id}/games/{game_number}/result", status_code=200)
async def submit_series_game_result(
    tournament_id: int,
    match_id: int,
    game_number: int,
    body: GameResultRequest,
    user: UserSession = Depends(require_admin),
) -> dict:
    """Trägt Ergebnis für Spiel N ein. Wenn Serie entschieden: Bracket-Winner wird gesetzt."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT id, status FROM bracket_matches WHERE id = ? AND tournament_id = ?",
            (match_id, tournament_id),
        )
        bm = await cursor.fetchone()
    if not bm:
        raise HTTPException(status_code=404, detail="Match nicht gefunden oder gehört nicht zu diesem Turnier")
    if bm["status"] in ("completed", "forfeit", "cancelled"):
        raise HTTPException(status_code=400, detail="Match bereits abgeschlossen")

    await ensure_game_exists(match_id, game_number)
    series_result = await record_game_result(
        match_id,
        game_number,
        winner_team=body.winner_team,
        duration_s=body.duration_s,
    )

    if series_result["series_done"]:
        async with get_db() as db:
            match_row = await _load_bracket_match_for_tournament_or_404(db, tournament_id, match_id)
            winner_team = int(series_result["series_winner_team"])
            winner_id = match_row["team1_id"] if winner_team == 1 else match_row["team2_id"]

            await _audit(
                db,
                "match_result_series",
                user.discord_id,
                json.dumps(
                    {
                        "tournament_id": tournament_id,
                        "match_id": match_id,
                        "game_number": game_number,
                        "winner_id": winner_id,
                        "series_winner_team": winner_team,
                        "wins_team1": series_result["wins_team1"],
                        "wins_team2": series_result["wins_team2"],
                        "source": "series_manual",
                    }
                ),
            )
            await db.commit()

        await apply_bracket_match_result(
            tournament_id,
            match_id,
            winning_team=winner_team - 1,
            winner_id=winner_id,
            duration_s=body.duration_s,
            players=None,
            source="series_manual",
        )

    return series_result


@router.post("/tournaments/{tournament_id}/matches/{match_id}/create-lobby", status_code=200)
async def create_match_lobby(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Erstellt eine Steam-Custom-Lobby für ein Bracket-Match."""
    try:
        result = await match_manager.create_lobby(tournament_id, match_id)
    except MatchNotFoundError as exc:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=str(exc),
        ) from exc
    except MatchStateError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(exc),
        ) from exc
    except SteamTaskError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Erstellen der Lobby nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc

    async with get_db() as db:
        await _audit(
            db,
            "match_create_lobby",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "party_id": result.get("party_id"),
                    "party_code": result.get("party_code"),
                    "join_code": result.get("join_code"),
                }
            ),
        )
        await db.commit()

    return {
        "success": True,
        "party_id": result.get("party_id"),
        "party_code": result.get("party_code"),
        "join_code": result.get("join_code"),
    }


@router.post("/tournaments/{tournament_id}/matches/{match_id}/start", status_code=200)
async def start_match_via_steam(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Startet ein Custom-Match über den Steam-Bot."""
    try:
        result = await match_manager.start_match(tournament_id, match_id)
    except MatchNotFoundError as exc:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=str(exc),
        ) from exc
    except MatchStateError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(exc),
        ) from exc
    except SteamTaskError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Match-Start nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc

    async with get_db() as db:
        await _audit(
            db,
            "match_start_steam",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "deadlock_match_id": result.get("match_id"),
                }
            ),
        )
        await db.commit()

    return {
        "success": True,
        "match_id": result.get("match_id"),
    }


@router.post("/tournaments/{tournament_id}/matches/{match_id}/fetch-result", status_code=200)
async def fetch_match_result_via_steam(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Lädt das Match-Ergebnis aus Deadlock und übernimmt es ins Bracket."""
    try:
        result = await match_manager.fetch_match_result(tournament_id, match_id)
    except MatchNotFoundError as exc:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=str(exc),
        ) from exc
    except MatchStateError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(exc),
        ) from exc
    except SteamTaskError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Abrufen des Match-Ergebnisses nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc

    async with get_db() as db:
        await _audit(
            db,
            "match_fetch_result_steam",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "winner_id": result.get("winner_id"),
                    "duration_s": result.get("duration_s"),
                }
            ),
        )
        await db.commit()

    return result


@router.post("/tournaments/{tournament_id}/matches/{match_id}/leave-lobby", status_code=200)
async def leave_match_lobby(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Lässt den Steam-Bot die Match-Lobby verlassen."""
    try:
        result = await match_manager.leave_lobby(tournament_id, match_id)
    except MatchNotFoundError as exc:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=str(exc),
        ) from exc
    except MatchStateError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(exc),
        ) from exc
    except SteamTaskError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Verlassen der Lobby nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc

    async with get_db() as db:
        await _audit(
            db,
            "match_leave_lobby",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "match_id": match_id}),
        )
        await db.commit()

    return result


@router.post("/tournaments/{tournament_id}/matches/{match_id}/reset", status_code=200)
async def reset_match(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_admin),
) -> dict:
    async with get_db() as db:
        await _reset_match_record(
            db=db,
            select_sql="SELECT discord_channel_id, status FROM bracket_matches WHERE id = ? AND tournament_id = ?",
            select_params=(match_id, tournament_id),
            update_sql=(
                "UPDATE bracket_matches "
                "SET status = 'pending', steam_party_id = NULL, party_code = NULL, "
                "deadlock_match_id = NULL, discord_channel_id = NULL "
                "WHERE id = ? AND tournament_id = ?"
            ),
            update_params=(match_id, tournament_id),
            match_id=match_id,
        )
        await _audit(
            db,
            "match_reset",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "match_id": match_id}),
        )
        await db.commit()
    return {"success": True}


@router.post("/tournaments/{tournament_id}/matches/{match_id}/manual-lobby", status_code=200)
async def set_manual_match_lobby(
    tournament_id: int,
    match_id: int,
    body: ManualLobbyCodeRequest,
    user: UserSession = Depends(require_mod),
) -> dict:
    party_code = body.party_code.strip()
    if not party_code:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="party_code ist erforderlich")

    async with get_db() as db:
        cursor = await db.execute(
            "UPDATE bracket_matches "
            "SET party_code = ?, steam_party_id = ?, status = 'lobby_created' "
            "WHERE id = ? AND tournament_id = ? AND status NOT IN ('completed', 'forfeit', 'cancelled')",
            (party_code, body.steam_party_id.strip() if body.steam_party_id else None, match_id, tournament_id),
        )
        if cursor.rowcount == 0:
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Match kann nicht manuell gesetzt werden")
        await _audit(
            db,
            "match_manual_lobby",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "match_id": match_id, "party_code": party_code}),
        )
        await db.commit()
    return {"success": True, "party_code": party_code}


@router.post("/tournaments/{tournament_id}/group-matches/{match_id}/create-lobby", status_code=200)
async def create_group_match_lobby(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Erstellt eine Steam-Custom-Lobby fuer ein Group-Match."""
    try:
        result = await match_manager.create_group_lobby(tournament_id, match_id)
    except MatchNotFoundError as exc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=str(exc)) from exc
    except MatchStateError as exc:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc
    except SteamTaskError as exc:
        raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Erstellen der Gruppen-Lobby nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc

    async with get_db() as db:
        await _audit(
            db,
            "group_match_create_lobby",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "party_id": result.get("party_id"),
                    "party_code": result.get("party_code"),
                    "join_code": result.get("join_code"),
                }
            ),
        )
        await db.commit()

    return {
        "success": True,
        "party_id": result.get("party_id"),
        "party_code": result.get("party_code"),
        "join_code": result.get("join_code"),
    }


@router.post("/tournaments/{tournament_id}/group-matches/{match_id}/start", status_code=200)
async def start_group_match_via_steam(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Startet ein Group-Match ueber den Steam-Bot."""
    try:
        result = await match_manager.start_group_match(tournament_id, match_id)
    except MatchNotFoundError as exc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=str(exc)) from exc
    except MatchStateError as exc:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc
    except SteamTaskError as exc:
        raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Start des Gruppen-Matches nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc

    async with get_db() as db:
        await _audit(
            db,
            "group_match_start_steam",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "deadlock_match_id": result.get("match_id"),
                }
            ),
        )
        await db.commit()

    return {"success": True, "match_id": result.get("match_id")}


@router.post("/tournaments/{tournament_id}/group-matches/{match_id}/fetch-result", status_code=200)
async def fetch_group_match_result_via_steam(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Laedt das Ergebnis eines Group-Matches aus Deadlock."""
    try:
        result = await match_manager.fetch_group_match_result(tournament_id, match_id)
    except MatchNotFoundError as exc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=str(exc)) from exc
    except MatchStateError as exc:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc
    except SteamTaskError as exc:
        raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Abrufen des Gruppen-Ergebnisses nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc

    async with get_db() as db:
        await _audit(
            db,
            "group_match_fetch_result_steam",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "winner_id": result.get("winner_id"),
                    "duration_s": result.get("duration_s"),
                }
            ),
        )
        await db.commit()

    return result


@router.post("/tournaments/{tournament_id}/group-matches/{match_id}/leave-lobby", status_code=200)
async def leave_group_match_lobby(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Laesst den Steam-Bot die Gruppen-Lobby verlassen."""
    try:
        result = await match_manager.leave_group_lobby(tournament_id, match_id)
    except MatchNotFoundError as exc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=str(exc)) from exc
    except MatchStateError as exc:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc
    except SteamTaskError as exc:
        raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Verlassen der Gruppen-Lobby nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail=str(exc)) from exc

    async with get_db() as db:
        await _audit(
            db,
            "group_match_leave_lobby",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "match_id": match_id}),
        )
        await db.commit()

    return result


@router.post("/tournaments/{tournament_id}/group-matches/{match_id}/reset", status_code=200)
async def reset_group_match(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_admin),
) -> dict:
    async with get_db() as db:
        await _reset_match_record(
            db=db,
            select_sql=(
                "SELECT gm.discord_channel_id, gm.status "
                "FROM group_matches gm "
                "JOIN groups g ON g.id = gm.group_id "
                "WHERE gm.id = ? AND g.tournament_id = ?"
            ),
            select_params=(match_id, tournament_id),
            update_sql=(
                "UPDATE group_matches "
                "SET status = 'pending', steam_party_id = NULL, party_code = NULL, "
                "deadlock_match_id = NULL, discord_channel_id = NULL "
                "WHERE id = ? AND group_id IN (SELECT id FROM groups WHERE tournament_id = ?)"
            ),
            update_params=(match_id, tournament_id),
            match_id=match_id,
        )
        await _audit(
            db,
            "group_match_reset",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "match_id": match_id}),
        )
        await db.commit()
    return {"success": True}


@router.post("/tournaments/{tournament_id}/group-matches/{match_id}/manual-lobby", status_code=200)
async def set_manual_group_match_lobby(
    tournament_id: int,
    match_id: int,
    body: ManualLobbyCodeRequest,
    user: UserSession = Depends(require_mod),
) -> dict:
    party_code = body.party_code.strip()
    if not party_code:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="party_code ist erforderlich")

    async with get_db() as db:
        cursor = await db.execute(
            "UPDATE group_matches "
            "SET party_code = ?, steam_party_id = ?, status = 'lobby_created' "
            "WHERE id = ? AND group_id IN (SELECT id FROM groups WHERE tournament_id = ?) "
            "AND status NOT IN ('completed', 'forfeit', 'cancelled')",
            (party_code, body.steam_party_id.strip() if body.steam_party_id else None, match_id, tournament_id),
        )
        if cursor.rowcount == 0:
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Gruppen-Match kann nicht manuell gesetzt werden")
        await _audit(
            db,
            "group_match_manual_lobby",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "match_id": match_id, "party_code": party_code}),
        )
        await db.commit()
    return {"success": True, "party_code": party_code}


@router.get("/casters", response_model=list[MatchCasterOut])
async def list_available_casters(
    user: UserSession = Depends(require_mod),
) -> list[MatchCasterOut]:
    del user
    members = await get_role_members(int(settings.DISCORD_GUILD_ID), settings.DISCORD_CASTER_ROLE_ID)
    casters: list[MatchCasterOut] = []
    for member in members:
        discord_id = str(member.get("user_id") or member.get("id") or "").strip()
        if not discord_id:
            continue
        casters.append(
            MatchCasterOut(
                discord_id=discord_id,
                display_name=str(
                    member.get("display_name")
                    or member.get("global_name")
                    or member.get("username")
                    or discord_id
                ),
            )
        )
    return casters


@router.get("/tournaments/{tournament_id}/matches/{match_id}/casters", response_model=list[MatchCasterOut])
async def list_match_casters(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> list[MatchCasterOut]:
    del user
    async with get_db() as db:
        await _ensure_bracket_match_exists(db, tournament_id, match_id)
        return await _load_assigned_casters(db, "bracket", match_id)


@router.post("/tournaments/{tournament_id}/matches/{match_id}/casters", response_model=list[MatchCasterOut])
async def assign_match_caster(
    tournament_id: int,
    match_id: int,
    body: CasterAssignRequest,
    user: UserSession = Depends(require_mod),
) -> list[MatchCasterOut]:
    async with get_db() as db:
        await _ensure_bracket_match_exists(db, tournament_id, match_id)
        await db.execute(
            "INSERT OR IGNORE INTO match_casters (match_id, match_type, discord_id, assigned_by) VALUES (?, 'bracket', ?, ?)",
            (match_id, body.discord_id, user.discord_id),
        )
        await _audit(
            db,
            "match_caster_assign",
            user.discord_id,
            json.dumps({"match_id": match_id, "discord_id": body.discord_id, "match_type": "bracket"}),
        )
        await db.commit()
        return await _load_assigned_casters(db, "bracket", match_id)


@router.delete("/tournaments/{tournament_id}/matches/{match_id}/casters/{discord_id}", response_model=list[MatchCasterOut])
async def remove_match_caster(
    tournament_id: int,
    match_id: int,
    discord_id: str,
    user: UserSession = Depends(require_mod),
) -> list[MatchCasterOut]:
    async with get_db() as db:
        await _ensure_bracket_match_exists(db, tournament_id, match_id)
        await db.execute(
            "DELETE FROM match_casters WHERE match_id = ? AND match_type = 'bracket' AND discord_id = ?",
            (match_id, discord_id),
        )
        await _audit(
            db,
            "match_caster_remove",
            user.discord_id,
            json.dumps({"match_id": match_id, "discord_id": discord_id, "match_type": "bracket"}),
        )
        await db.commit()
        return await _load_assigned_casters(db, "bracket", match_id)


@router.get("/tournaments/{tournament_id}/matches/{match_id}/event-presets", status_code=200)
async def get_match_event_presets(
    tournament_id: int,
    match_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Liefert Live-Event-Presets fuer das Admin-Panel."""
    del user
    try:
        match = await match_manager._get_bracket_match(tournament_id, match_id)  # noqa: SLF001
    except MatchNotFoundError as exc:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=str(exc),
        ) from exc

    presets = await match_manager.list_match_event_presets()
    return {
        "success": True,
        "match_id": match_id,
        "party_id": match.get("steam_party_id"),
        "party_code": match.get("party_code"),
        "presets": presets,
    }


@router.post("/tournaments/{tournament_id}/matches/{match_id}/apply-convars", status_code=200)
async def apply_match_convars(
    tournament_id: int,
    match_id: int,
    body: dict,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Wendet frei definierte Match-ConVars auf eine laufende Lobby an."""
    convars = body.get("convars")
    try:
        result = await match_manager.apply_match_convars(tournament_id, match_id, convars)
    except MatchNotFoundError as exc:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=str(exc),
        ) from exc
    except MatchStateError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(exc),
        ) from exc
    except SteamTaskError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Anwenden der Match-ConVars nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc

    async with get_db() as db:
        await _audit(
            db,
            "match_apply_convars",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "party_id": result.get("party_id"),
                    "applied_convars": result.get("applied_convars"),
                },
                default=str,
            ),
        )
        await db.commit()

    return result


@router.post("/tournaments/{tournament_id}/matches/{match_id}/apply-event-preset", status_code=200)
async def apply_match_event_preset(
    tournament_id: int,
    match_id: int,
    body: dict,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Wendet ein Event-Preset auf eine laufende Match-Lobby an."""
    preset_key = str(body.get("preset_key") or "").strip()
    enabled = bool(body.get("enabled", True))
    if not preset_key:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="preset_key ist erforderlich",
        )

    try:
        result = await match_manager.apply_match_event_preset(
            tournament_id,
            match_id,
            preset_key,
            enabled=enabled,
        )
    except MatchNotFoundError as exc:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=str(exc),
        ) from exc
    except MatchStateError as exc:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=str(exc),
        ) from exc
    except SteamTaskError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc
    except TimeoutError as exc:
        raise HTTPException(
            status_code=status.HTTP_504_GATEWAY_TIMEOUT,
            detail="Steam Bot hat beim Anwenden des Match-Events nicht rechtzeitig geantwortet",
        ) from exc
    except RuntimeError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=str(exc),
        ) from exc

    async with get_db() as db:
        await _audit(
            db,
            "match_apply_event_preset",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "preset_key": result.get("preset_key"),
                    "enabled": result.get("enabled"),
                    "party_id": result.get("party_id"),
                    "applied_convars": result.get("applied_convars"),
                },
                default=str,
            ),
        )
        await db.commit()

    return result


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments/{id}/groups/generate — Gruppen generieren
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/groups/generate", status_code=200)
async def generate_groups_endpoint(
    tournament_id: int,
    body: dict | None = None,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Gruppen generieren mit Snake-Draft Seeding (Mod+)."""
    num_groups = 4
    if body and "num_groups" in body:
        num_groups = int(body["num_groups"])
        num_groups = max(2, min(8, num_groups))

    try:
        group_ids = await generate_groups(tournament_id, num_groups)
    except ValueError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))

    # Gruppen-Matches generieren
    match_count = await generate_group_matches(tournament_id)

    async with get_db() as db:
        await _audit(
            db,
            "groups_generate",
            user.discord_id,
            json.dumps({
                "tournament_id": tournament_id,
                "groups": len(group_ids),
                "matches": match_count,
            }),
        )
        await db.commit()

    return {"groups_created": len(group_ids), "matches_created": match_count}


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments/{id}/bracket/generate — Bracket generieren
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/bracket/generate", status_code=200)
async def generate_bracket_endpoint(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Bracket generieren aus Gruppen-Ergebnissen oder direkt aus Teams (Mod+)."""
    try:
        match_count = await generate_bracket(tournament_id)
    except ValueError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))

    async with get_db() as db:
        await _audit(
            db,
            "bracket_generate",
            user.discord_id,
            json.dumps({
                "tournament_id": tournament_id,
                "matches": match_count,
            }),
        )
        await db.commit()

    return {"bracket_matches_created": match_count}


@router.post("/tournaments/{tournament_id}/voice/move-teams")
async def voice_move_teams(
    tournament_id: int,
    match_id: int = Query(..., description="ID des Bracket-Matches"),
    user: UserSession = Depends(require_admin),
) -> dict:
    """Verschiebt Team1 → VC1, Team2 → VC2 basierend auf dem aktuellen Match."""
    del user
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT team1_id, team2_id FROM bracket_matches WHERE id = ? AND tournament_id = ?",
            (match_id, tournament_id),
        )
        match = await cursor.fetchone()
    if not match:
        raise HTTPException(status_code=404, detail="Match nicht gefunden")

    async def _load_discord_ids(team_id: int | None) -> list[str]:
        if team_id is None:
            return []
        async with get_db() as db:
            cursor = await db.execute(
                "SELECT discord_id FROM team_members WHERE team_id = ? AND discord_id IS NOT NULL",
                (team_id,),
            )
            rows = await cursor.fetchall()
        return [str(r["discord_id"]) for r in rows if r["discord_id"]]

    guild_id = int(settings.DISCORD_GUILD_ID)
    team1_ids = await _load_discord_ids(match["team1_id"])
    team2_ids = await _load_discord_ids(match["team2_id"])
    result1 = await move_users_to_voice_channel(
        team1_ids,
        settings.DISCORD_TEAM1_VOICE_CHANNEL_ID,
        guild_id=guild_id,
    )
    result2 = await move_users_to_voice_channel(
        team2_ids,
        settings.DISCORD_TEAM2_VOICE_CHANNEL_ID,
        guild_id=guild_id,
    )
    return {"team1": result1, "team2": result2}


@router.post("/tournaments/{tournament_id}/voice/move-sammelpunkt")
async def voice_move_sammelpunkt(
    tournament_id: int,
    user: UserSession = Depends(require_admin),
) -> dict:
    """Verschiebt alle Turnier-Teilnehmer zurück in den Sammelpunkt-VC."""
    del user
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT DISTINCT tm.discord_id
            FROM team_members tm
            JOIN teams t ON t.id = tm.team_id
            WHERE t.tournament_id = ? AND tm.discord_id IS NOT NULL
            """,
            (tournament_id,),
        )
        rows = await cursor.fetchall()
    all_ids = [str(r["discord_id"]) for r in rows if r["discord_id"]]
    guild_id = int(settings.DISCORD_GUILD_ID)
    result = await move_users_to_voice_channel(
        all_ids,
        settings.DISCORD_SAMMELPUNKT_CHANNEL_ID,
        guild_id=guild_id,
    )
    return result


@router.post("/voice/move-user")
async def voice_move_user(
    body: VoiceMoveRequest,
    user: UserSession = Depends(require_admin),
) -> dict:
    """Verschiebt einen einzelnen User manuell in einen Voice-Kanal."""
    del user
    guild_id = int(settings.DISCORD_GUILD_ID)
    result = await move_users_to_voice_channel([body.discord_id], body.channel_id, guild_id=guild_id)
    return result


@router.get("/voice/channel-members/{channel_id}")
async def voice_get_channel_members(
    channel_id: int,
    user: UserSession = Depends(require_admin),
) -> dict:
    """Gibt zurück, wer aktuell in einem Voice-Kanal ist."""
    del user
    members = await get_voice_channel_members(channel_id)
    return {"channel_id": channel_id, "members": members}
