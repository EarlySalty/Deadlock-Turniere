from __future__ import annotations

import json
import logging
import random
from datetime import datetime, timedelta, timezone
from typing import Literal

from fastapi import APIRouter, Depends, HTTPException, status
from pydantic import BaseModel, Field

from auth.permissions import require_mod
from db import get_db
from match import manager as match_manager
from match.result_processor import apply_bracket_match_result
from rank_reader import RANK_NAMES_BY_TIER
from tournament.admin_routes import _audit, _delete_tournament_tree, _load_tournament_or_404
from tournament.engine import finalize_checkin
from tournament.models import TournamentGameMode, UserSession
from tournament.scheduler import advance_tournament_status

router = APIRouter(prefix="/api/admin/test", tags=["admin", "test-mode"])
logger = logging.getLogger(__name__)


class TestUsersCreateRequest(BaseModel):
    count: int = Field(ge=1, le=100)


class TestUserCreated(BaseModel):
    discord_id: str
    display_name: str


class TestUserOut(BaseModel):
    discord_id: str
    display_name: str
    rank: str | None = None


class TestUsersCreateResponse(BaseModel):
    created: list[TestUserCreated]


class TestUsersDeleteResponse(BaseModel):
    deleted: int


class TestTournamentCreateRequest(BaseModel):
    name: str
    team_size: int = Field(ge=1, le=12)
    num_teams: int = Field(ge=2, le=128)
    mode: Literal["bracket_only", "group_then_bracket"]
    tournament_game_mode: TournamentGameMode = TournamentGameMode.standard
    advance_to: Literal["bracket", "group_phase", "checkin"] = "bracket"


class TestTournamentCreateResponse(BaseModel):
    tournament_id: int


class SimulateRoundResponse(BaseModel):
    simulated_matches: int


class TestWipeResponse(BaseModel):
    deleted_tournaments: int
    deleted_users: int


def _utc_now() -> datetime:
    return datetime.now(timezone.utc)


def _test_suffix(existing_ids: set[str]) -> str:
    for _ in range(1000):
        suffix = f"{random.randint(0, 999999):06d}"
        discord_id = f"test_{suffix}"
        if discord_id not in existing_ids:
            existing_ids.add(discord_id)
            return suffix
    raise RuntimeError("Konnte keine eindeutige Test-Discord-ID erzeugen")


def _rank_payload() -> tuple[str, int, int, int]:
    rank_tier = random.randint(1, 11)
    subrank = random.randint(1, 6)
    rank_name = RANK_NAMES_BY_TIER[rank_tier]
    rank_score = rank_tier * 100 + subrank
    return rank_name, rank_tier, subrank, rank_score


async def _create_test_users_in_db(db, count: int) -> list[TestUserCreated]:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT discord_id FROM user_profiles WHERE discord_id LIKE 'test_%'"
    )
    existing_ids = {str(row["discord_id"]) for row in await cursor.fetchall() if row["discord_id"]}
    created: list[TestUserCreated] = []
    now = _utc_now().isoformat()

    for _ in range(count):
        suffix = _test_suffix(existing_ids)
        discord_id = f"test_{suffix}"
        display_name = f"Test User {suffix}"
        rank_name, rank_tier, subrank, rank_score = _rank_payload()
        await db.execute(
            """
            INSERT INTO user_profiles (discord_id, display_name, updated_at)
            VALUES (?, ?, ?)
            """,
            (discord_id, display_name, now),
        )
        await db.execute(
            """
            INSERT INTO rank_cache (discord_id, source, steam_id, rank, rank_tier, subrank, rank_score)
            VALUES (?, 'test_mode', NULL, ?, ?, ?, ?)
            """,
            (discord_id, rank_name, rank_tier, subrank, rank_score),
        )
        created.append(TestUserCreated(discord_id=discord_id, display_name=display_name))

    return created


async def _load_test_users(db) -> list[dict[str, str | None]]:  # noqa: ANN001
    cursor = await db.execute(
        """
        SELECT up.discord_id, up.display_name, rc.rank
        FROM user_profiles up
        LEFT JOIN rank_cache rc ON rc.discord_id = up.discord_id
        WHERE up.discord_id LIKE 'test_%'
        ORDER BY up.display_name, up.discord_id
        """
    )
    return [dict(row) for row in await cursor.fetchall()]


async def _ensure_test_user_deletion_allowed(db) -> None:  # noqa: ANN001
    cursor = await db.execute(
        """
        SELECT 1
        FROM team_members tm
        JOIN teams t ON t.id = tm.team_id
        JOIN tournaments tr ON tr.id = t.tournament_id
        WHERE tm.discord_id LIKE 'test_%'
          AND tr.status IN ('registration', 'checkin', 'group_phase', 'bracket')
        LIMIT 1
        """
    )
    if await cursor.fetchone():
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Test-User sind noch in laufenden Turnieren referenziert",
        )

    cursor = await db.execute(
        """
        SELECT 1
        FROM tournament_signups ts
        JOIN tournaments tr ON tr.id = ts.tournament_id
        WHERE ts.discord_id LIKE 'test_%'
          AND tr.status IN ('registration', 'checkin', 'group_phase', 'bracket')
        LIMIT 1
        """
    )
    if await cursor.fetchone():
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Test-User sind noch in laufenden Turnieren referenziert",
        )

    cursor = await db.execute(
        """
        SELECT 1
        FROM tournament_checkins tc
        JOIN tournaments tr ON tr.id = tc.tournament_id
        WHERE tc.discord_id LIKE 'test_%'
          AND tr.status IN ('registration', 'checkin', 'group_phase', 'bracket')
        LIMIT 1
        """
    )
    if await cursor.fetchone():
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Test-User sind noch in laufenden Turnieren referenziert",
        )


async def _delete_test_users(db) -> int:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT COUNT(*) AS cnt FROM user_profiles WHERE discord_id LIKE 'test_%'"
    )
    row = await cursor.fetchone()
    deleted = int(row["cnt"] or 0) if row else 0
    await db.execute("DELETE FROM sessions WHERE discord_id LIKE 'test_%'")
    await db.execute("DELETE FROM user_consents WHERE discord_id LIKE 'test_%'")
    await db.execute("DELETE FROM player_points WHERE discord_id LIKE 'test_%'")
    await db.execute("DELETE FROM rank_cache WHERE discord_id LIKE 'test_%'")
    await db.execute("DELETE FROM user_profiles WHERE discord_id LIKE 'test_%'")
    return deleted


async def _ensure_test_tournament(db, tournament_id: int):  # noqa: ANN001
    tournament = await _load_tournament_or_404(db, tournament_id)
    if not bool(tournament["is_test"]):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Nur Test-Turniere dürfen über diesen Endpoint verändert werden",
        )
    return tournament


def _normalize_test_mode(mode: str) -> str:
    return "group_stage" if mode == "group_then_bracket" else "bracket_only"


def _team_name(base_name: str, existing_keys: set[str]) -> tuple[str, str]:
    candidate = f"{base_name} Team".strip()
    if not candidate:
        candidate = "Test Team"
    team_name = candidate
    suffix = 1
    while team_name.casefold() in existing_keys:
        suffix += 1
        team_name = f"{candidate} ({suffix})"
    name_key = team_name.casefold()
    existing_keys.add(name_key)
    return team_name, name_key


@router.post("/users", response_model=TestUsersCreateResponse)
async def create_test_users(
    body: TestUsersCreateRequest,
    user: UserSession = Depends(require_mod),
) -> TestUsersCreateResponse:
    async with get_db() as db:
        created = await _create_test_users_in_db(db, body.count)
        await _audit(
            db,
            "test_users_create",
            user.discord_id,
            json.dumps({"count": body.count, "created_ids": [item.discord_id for item in created]}),
        )
        await db.commit()
    return TestUsersCreateResponse(created=created)


@router.get("/users", response_model=list[TestUserOut])
async def list_test_users(
    user: UserSession = Depends(require_mod),
) -> list[TestUserOut]:
    del user
    async with get_db() as db:
        rows = await _load_test_users(db)
    return [TestUserOut(**row) for row in rows]


@router.delete("/users", response_model=TestUsersDeleteResponse)
async def delete_test_users(
    user: UserSession = Depends(require_mod),
) -> TestUsersDeleteResponse:
    async with get_db() as db:
        await _ensure_test_user_deletion_allowed(db)
        deleted = await _delete_test_users(db)
        await _audit(
            db,
            "test_users_delete",
            user.discord_id,
            json.dumps({"deleted": deleted}),
        )
        await db.commit()
    return TestUsersDeleteResponse(deleted=deleted)


@router.post("/tournaments", response_model=TestTournamentCreateResponse)
async def create_test_tournament(
    body: TestTournamentCreateRequest,
    user: UserSession = Depends(require_mod),
) -> TestTournamentCreateResponse:
    if body.mode == "bracket_only" and body.advance_to == "group_phase":
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="group_phase ist für bracket_only nicht verfügbar",
        )

    required_users = body.team_size * body.num_teams
    registration_start = _utc_now()
    registration_end = registration_start + timedelta(hours=1)
    tournament_id: int | None = None

    async with get_db() as db:
        existing_users = await _load_test_users(db)
        missing_users = required_users - len(existing_users)
        if missing_users > 0:
            await _create_test_users_in_db(db, missing_users)

        cursor = await db.execute(
            """
            SELECT up.discord_id, up.display_name, rc.rank, rc.rank_score
            FROM user_profiles up
            LEFT JOIN rank_cache rc ON rc.discord_id = up.discord_id
            WHERE up.discord_id LIKE 'test_%'
            ORDER BY up.discord_id
            LIMIT ?
            """,
            (required_users,),
        )
        user_rows = [dict(row) for row in await cursor.fetchall()]
        if len(user_rows) < required_users:
            raise HTTPException(
                status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
                detail="Test-User-Pool konnte nicht vollständig erzeugt werden",
            )

        random.shuffle(user_rows)
        cursor = await db.execute(
            """
            INSERT INTO tournaments (
                name, status, description, team_size, registration_start, registration_end,
                checkin_start, group_phase_start, bracket_start, bracket_format, created_by,
                tournament_mode, tournament_game_mode, auto_lobby_enabled,
                exclude_from_leaderboard, is_test, reminder_offsets
            )
            VALUES (?, 'checkin', ?, ?, ?, ?, ?, ?, ?, 'single_elimination', ?, ?, ?, 0, 1, 1, ?)
            """,
            (
                body.name,
                "Autogenerated test tournament",
                body.team_size,
                registration_start.isoformat(),
                registration_end.isoformat(),
                registration_start.isoformat(),
                registration_start.isoformat(),
                registration_start.isoformat(),
                user.discord_id,
                _normalize_test_mode(body.mode),
                body.tournament_game_mode.value,
                json.dumps([1440, 120, 15]),
            ),
        )
        tournament_id = int(cursor.lastrowid)

        existing_keys: set[str] = set()
        for team_index in range(body.num_teams):
            members = user_rows[team_index * body.team_size : (team_index + 1) * body.team_size]
            captain = members[0]
            team_name, name_key = _team_name(captain.get("display_name") or captain["discord_id"], existing_keys)
            cursor = await db.execute(
                "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?)",
                (tournament_id, team_name, name_key, captain["discord_id"]),
            )
            team_id = int(cursor.lastrowid)

            for member_index, member in enumerate(members):
                role = "captain" if member_index == 0 else "member"
                display_name = str(member.get("display_name") or member["discord_id"])
                await db.execute(
                    """
                    INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role)
                    VALUES (?, ?, ?, NULL, ?, ?, ?)
                    """,
                    (
                        team_id,
                        member["discord_id"],
                        display_name,
                        member.get("rank"),
                        member.get("rank_score") or 0,
                        role,
                    ),
                )
                await db.execute(
                    """
                    INSERT INTO tournament_signups (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id)
                    VALUES (?, ?, ?, NULL, ?, ?, ?)
                    """,
                    (
                        tournament_id,
                        member["discord_id"],
                        display_name,
                        member.get("rank"),
                        member.get("rank_score") or 0,
                        team_id,
                    ),
                )
                await db.execute(
                    "INSERT INTO tournament_checkins (tournament_id, discord_id) VALUES (?, ?)",
                    (tournament_id, member["discord_id"]),
                )

        await _audit(
            db,
            "test_tournament_create",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "name": body.name,
                    "team_size": body.team_size,
                    "num_teams": body.num_teams,
                    "mode": body.mode,
                    "advance_to": body.advance_to,
                }
            ),
        )
        await db.commit()

    assert tournament_id is not None

    try:
        if body.advance_to in {"group_phase", "bracket"}:
            preview = await finalize_checkin(tournament_id, confirm=False)
            await finalize_checkin(
                tournament_id,
                confirm=True,
                actor_id=user.discord_id,
                expected_snapshot_token=preview["snapshot_token"],
                advance_to_group_phase=True,
            )

        if body.advance_to == "bracket":
            async with get_db() as db:
                tournament = await _ensure_test_tournament(db, tournament_id)
            if tournament["status"] == "group_phase":
                await advance_tournament_status(
                    tournament_id,
                    current_status="group_phase",
                    next_status="bracket",
                    source="manual",
                    actor_id=user.discord_id,
                )
    except Exception:
        logger.exception("Test-Turnier konnte nicht vollständig vorbereitet werden (tournament=%s)", tournament_id)
        async with get_db() as db:
            await _delete_tournament_tree(db, tournament_id)
            await db.commit()
        raise

    return TestTournamentCreateResponse(tournament_id=tournament_id)


@router.post("/tournaments/{tournament_id}/simulate-round", response_model=SimulateRoundResponse)
async def simulate_test_tournament_round(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> SimulateRoundResponse:
    async with get_db() as db:
        tournament = await _ensure_test_tournament(db, tournament_id)

        cursor = await db.execute(
            """
            SELECT id, round, team1_id, team2_id
            FROM bracket_matches
            WHERE tournament_id = ?
              AND team1_id IS NOT NULL
              AND team2_id IS NOT NULL
              AND status NOT IN ('completed', 'forfeit', 'cancelled')
            ORDER BY round, position, id
            """,
            (tournament_id,),
        )
        bracket_rows = [dict(row) for row in await cursor.fetchall()]

        cursor = await db.execute(
            """
            SELECT gm.id, gm.team1_id, gm.team2_id
            FROM group_matches gm
            JOIN groups g ON g.id = gm.group_id
            WHERE g.tournament_id = ?
              AND gm.team1_id IS NOT NULL
              AND gm.team2_id IS NOT NULL
              AND gm.status NOT IN ('completed', 'forfeit', 'cancelled')
            ORDER BY gm.id
            """,
            (tournament_id,),
        )
        group_rows = [dict(row) for row in await cursor.fetchall()]

    simulated_matches = 0

    if group_rows and tournament["status"] == "group_phase":
        for row in group_rows:
            winner_id = random.choice([int(row["team1_id"]), int(row["team2_id"])])
            await match_manager._apply_group_match_result(  # noqa: SLF001
                tournament_id,
                int(row["id"]),
                winner_id=winner_id,
                duration_s=0,
                players=[],
                source="manual",
            )
            simulated_matches += 1
    elif bracket_rows:
        current_round = min(int(row["round"]) for row in bracket_rows)
        for row in bracket_rows:
            if int(row["round"]) != current_round:
                continue
            winner_id = random.choice([int(row["team1_id"]), int(row["team2_id"])])
            await apply_bracket_match_result(
                tournament_id,
                int(row["id"]),
                winner_id=winner_id,
                duration_s=0,
                players=[],
                source="manual",
            )
            simulated_matches += 1

    async with get_db() as db:
        await _audit(
            db,
            "test_tournament_simulate_round",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "simulated_matches": simulated_matches}),
        )
        await db.commit()

    return SimulateRoundResponse(simulated_matches=simulated_matches)


@router.delete("/wipe", response_model=TestWipeResponse)
async def wipe_test_data(
    user: UserSession = Depends(require_mod),
) -> TestWipeResponse:
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT id FROM tournaments WHERE is_test = 1 ORDER BY id"
        )
        tournament_ids = [int(row["id"]) for row in await cursor.fetchall()]
        for tournament_id in tournament_ids:
            await _delete_tournament_tree(db, tournament_id)

        deleted_users = await _delete_test_users(db)
        await _audit(
            db,
            "test_data_wipe",
            user.discord_id,
            json.dumps(
                {
                    "deleted_tournaments": len(tournament_ids),
                    "deleted_users": deleted_users,
                }
            ),
        )
        await db.commit()

    return TestWipeResponse(
        deleted_tournaments=len(tournament_ids),
        deleted_users=deleted_users,
    )
