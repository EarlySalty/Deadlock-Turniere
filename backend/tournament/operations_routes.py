"""Operative Endpoints fürs Turnier-Tagesgeschäft.

Deckt das Selbst-Melden von Off-Stream-Ergebnissen durch Captains, die
Admin-Bestätigung, den Leitstand ("was braucht jetzt Aktion") und das
Umschalten des Stream-Markers ab. Arbeitet auf Bracket-Matches.
"""
from __future__ import annotations

import json
import logging
from datetime import datetime, timedelta

from fastapi import APIRouter, Body, Depends, HTTPException, status
from pydantic import BaseModel

from auth.permissions import require_auth, require_mod
from db import get_db
from match.result_processor import (
    MatchNotFoundError,
    MatchResultError,
    MatchStateError,
    apply_bracket_match_result,
)
from notifications.discord_notifier import notify_users
from tournament.models import MatchResultReport, MatchResultReportCreate, UserSession

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api", tags=["operations"])

_OPEN_REPORT_STATUS = "pending"
_FINISHED_MATCH_STATUSES = {"completed", "cancelled", "forfeit"}


async def _audit(db, action: str, user_id: str | None, details: str) -> None:  # noqa: ANN001
    await db.execute(
        "INSERT INTO audit_log (action, user_id, details) VALUES (?, ?, ?)",
        (action, user_id, details),
    )


async def _load_bracket_match(db, tournament_id: int, match_id: int):  # noqa: ANN001
    cursor = await db.execute(
        "SELECT id, tournament_id, round, team1_id, team2_id, winner_id, status, on_stream "
        "FROM bracket_matches WHERE id = ? AND tournament_id = ?",
        (match_id, tournament_id),
    )
    row = await cursor.fetchone()
    if row is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Match nicht gefunden")
    return row


async def _team_captains(db, team_ids: list[int]) -> dict[int, str]:  # noqa: ANN001
    captains: dict[int, str] = {}
    for team_id in team_ids:
        if team_id is None:
            continue
        cursor = await db.execute(
            "SELECT captain_discord_id FROM teams WHERE id = ?",
            (team_id,),
        )
        row = await cursor.fetchone()
        if row is not None:
            captains[team_id] = str(row["captain_discord_id"])
    return captains


async def _team_member_ids(db, team_id: int) -> list[str]:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT discord_id FROM team_members WHERE team_id = ?",
        (team_id,),
    )
    return [str(row["discord_id"]) for row in await cursor.fetchall() if row["discord_id"]]


def _parse_db_timestamp(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.strip().replace("Z", ""))
    except ValueError:
        return None


# ---------------------------------------------------------------------------
# Captain meldet ein Off-Stream-Ergebnis
# ---------------------------------------------------------------------------

@router.post(
    "/tournaments/{tournament_id}/matches/{match_id}/report-result",
    response_model=MatchResultReport,
    status_code=201,
)
async def report_match_result(
    tournament_id: int,
    match_id: int,
    body: MatchResultReportCreate,
    user: UserSession = Depends(require_auth),
) -> MatchResultReport:
    """Ein Captain meldet Sieger + Deadlock-Match-ID (oder einen No-Show).

    Die Meldung bleibt 'pending', bis ein Admin sie bestätigt.
    """
    async with get_db() as db:
        match = await _load_bracket_match(db, tournament_id, match_id)
        if match["status"] in _FINISHED_MATCH_STATUSES:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail=f"Match {match_id} ist bereits abgeschlossen ({match['status']})",
            )
        team1_id, team2_id = match["team1_id"], match["team2_id"]
        if team1_id is None or team2_id is None:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Match hat noch nicht beide Teams gesetzt",
            )

        captains = await _team_captains(db, [team1_id, team2_id])
        is_captain = user.discord_id in captains.values()
        is_mod = bool(getattr(user, "is_mod", False) or getattr(user, "is_admin", False))
        if not is_captain and not is_mod:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail="Nur ein Team-Captain dieses Matches darf ein Ergebnis melden",
            )

        if body.is_no_show:
            if body.no_show_team_id not in (team1_id, team2_id):
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="no_show_team_id muss eines der beiden Teams sein",
                )
        else:
            if body.winner_team_id not in (team1_id, team2_id):
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="winner_team_id muss eines der beiden Teams sein",
                )

        # Vorherige offene Meldung desselben Melders für dieses Match ersetzen
        await db.execute(
            "DELETE FROM match_result_reports "
            "WHERE match_type = 'bracket' AND match_id = ? AND reported_by = ? AND status = ?",
            (match_id, user.discord_id, _OPEN_REPORT_STATUS),
        )
        cursor = await db.execute(
            "INSERT INTO match_result_reports "
            "(match_type, match_id, tournament_id, reported_by, winner_team_id, "
            "deadlock_match_id, is_no_show, no_show_team_id, status) "
            "VALUES ('bracket', ?, ?, ?, ?, ?, ?, ?, 'pending')",
            (
                match_id,
                tournament_id,
                user.discord_id,
                body.winner_team_id,
                body.deadlock_match_id,
                1 if body.is_no_show else 0,
                body.no_show_team_id,
            ),
        )
        report_id = int(cursor.lastrowid)
        await _audit(
            db,
            "match_result_reported",
            user.discord_id,
            json.dumps(
                {
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "report_id": report_id,
                    "is_no_show": body.is_no_show,
                }
            ),
        )

        if body.is_no_show and body.no_show_team_id is not None:
            missing_ids = await _team_member_ids(db, body.no_show_team_id)
        else:
            missing_ids = []

        await db.commit()

        cursor = await db.execute(
            "SELECT * FROM match_result_reports WHERE id = ?",
            (report_id,),
        )
        report_row = await cursor.fetchone()

    if missing_ids:
        try:
            await notify_users(
                missing_ids,
                "match_start",
                "Hey, dein Gegner wartet im Turnier auf dich — komm bitte in die Lobby, "
                "sonst wird das Match als Walkover gewertet.",
            )
        except Exception:
            logger.exception("No-Show-Hinweis-DM fehlgeschlagen (match=%s)", match_id)

    return MatchResultReport(**dict(report_row))


# ---------------------------------------------------------------------------
# Leitstand — was braucht jetzt Aktion
# ---------------------------------------------------------------------------

@router.get("/admin/tournaments/{tournament_id}/action-items")
async def get_action_items(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Offene Ergebnis-Meldungen + No-Show-Fälle für den Admin-Leitstand."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT no_show_grace_minutes FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if tournament is None:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND, detail="Turnier nicht gefunden"
            )
        grace_minutes = int(tournament["no_show_grace_minutes"] or 10)

        cursor = await db.execute(
            """
            SELECT r.id, r.match_id, r.reported_by, r.winner_team_id, r.deadlock_match_id,
                   r.is_no_show, r.no_show_team_id, r.created_at,
                   bm.round AS match_round, bm.team1_id, bm.team2_id, bm.on_stream,
                   t1.name AS team1_name, t2.name AS team2_name,
                   tw.name AS winner_name, tn.name AS no_show_name
            FROM match_result_reports r
            JOIN bracket_matches bm ON bm.id = r.match_id
            LEFT JOIN teams t1 ON t1.id = bm.team1_id
            LEFT JOIN teams t2 ON t2.id = bm.team2_id
            LEFT JOIN teams tw ON tw.id = r.winner_team_id
            LEFT JOIN teams tn ON tn.id = r.no_show_team_id
            WHERE r.tournament_id = ? AND r.match_type = 'bracket' AND r.status = 'pending'
            ORDER BY r.created_at
            """,
            (tournament_id,),
        )
        rows = await cursor.fetchall()

    now = datetime.utcnow()
    items: list[dict] = []
    for row in rows:
        created = _parse_db_timestamp(row["created_at"])
        grace_expired = False
        if bool(row["is_no_show"]) and created is not None:
            grace_expired = now >= created + timedelta(minutes=grace_minutes)
        items.append(
            {
                "report_id": row["id"],
                "match_id": row["match_id"],
                "match_round": row["match_round"],
                "on_stream": bool(row["on_stream"]),
                "team1_name": row["team1_name"],
                "team2_name": row["team2_name"],
                "reported_by": row["reported_by"],
                "is_no_show": bool(row["is_no_show"]),
                "winner_team_id": row["winner_team_id"],
                "winner_name": row["winner_name"],
                "no_show_team_id": row["no_show_team_id"],
                "no_show_name": row["no_show_name"],
                "deadlock_match_id": row["deadlock_match_id"],
                "created_at": row["created_at"],
                "grace_minutes": grace_minutes,
                "grace_expired": grace_expired,
            }
        )

    return {"pending_reports": items, "no_show_grace_minutes": grace_minutes}


# ---------------------------------------------------------------------------
# Admin bestätigt / verwirft eine Meldung
# ---------------------------------------------------------------------------

async def _load_report_or_404(db, report_id: int):  # noqa: ANN001
    cursor = await db.execute(
        "SELECT * FROM match_result_reports WHERE id = ?",
        (report_id,),
    )
    row = await cursor.fetchone()
    if row is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND, detail="Meldung nicht gefunden"
        )
    return row


@router.post("/admin/result-reports/{report_id}/confirm")
async def confirm_result_report(
    report_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Bestätigt eine gemeldete Ergebnis-/No-Show-Meldung und wertet das Match."""
    async with get_db() as db:
        report = await _load_report_or_404(db, report_id)
        if report["status"] != "pending":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail=f"Meldung ist bereits {report['status']}",
            )
        match = await _load_bracket_match(db, report["tournament_id"], report["match_id"])
        team1_id, team2_id = match["team1_id"], match["team2_id"]

        if bool(report["is_no_show"]):
            no_show_team = report["no_show_team_id"]
            winner_id = team1_id if no_show_team == team2_id else team2_id
            result_source = "no_show"
            force = True
        else:
            winner_id = report["winner_team_id"]
            result_source = "self_report"
            force = False

        if winner_id not in (team1_id, team2_id):
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Sieger der Meldung passt nicht mehr zu den Teams des Matches",
            )

    try:
        result = await apply_bracket_match_result(
            report["tournament_id"],
            report["match_id"],
            winner_id=winner_id,
            source=result_source,
            force=force,
        )
    except MatchNotFoundError as exc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=str(exc)) from exc
    except (MatchStateError, MatchResultError) as exc:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(exc)) from exc

    async with get_db() as db:
        # Diese Meldung bestätigen, konkurrierende Meldungen desselben Matches verwerfen
        await db.execute(
            "UPDATE match_result_reports "
            "SET status = 'confirmed', resolved_at = datetime('now'), resolved_by = ? "
            "WHERE id = ?",
            (user.discord_id, report_id),
        )
        await db.execute(
            "UPDATE match_result_reports "
            "SET status = 'rejected', resolved_at = datetime('now'), resolved_by = ? "
            "WHERE match_type = 'bracket' AND match_id = ? AND status = 'pending' AND id != ?",
            (user.discord_id, report["match_id"], report_id),
        )
        if report["deadlock_match_id"]:
            await db.execute(
                "UPDATE bracket_matches SET deadlock_match_id = ? WHERE id = ?",
                (str(report["deadlock_match_id"]), report["match_id"]),
            )
        await _audit(
            db,
            "match_result_report_confirmed",
            user.discord_id,
            json.dumps(
                {
                    "report_id": report_id,
                    "match_id": report["match_id"],
                    "winner_id": winner_id,
                    "source": result_source,
                }
            ),
        )
        await db.commit()

    return {
        "status": "ok",
        "report_id": report_id,
        "match_id": result["match_id"],
        "winner_id": result["winner_id"],
        "winning_team": result["winning_team"],
        "source": result_source,
    }


@router.post("/admin/result-reports/{report_id}/reject")
async def reject_result_report(
    report_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Verwirft eine Meldung, ohne das Match zu werten (z.B. Team will doch spielen)."""
    async with get_db() as db:
        report = await _load_report_or_404(db, report_id)
        if report["status"] != "pending":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail=f"Meldung ist bereits {report['status']}",
            )
        await db.execute(
            "UPDATE match_result_reports "
            "SET status = 'rejected', resolved_at = datetime('now'), resolved_by = ? "
            "WHERE id = ?",
            (user.discord_id, report_id),
        )
        await _audit(
            db,
            "match_result_report_rejected",
            user.discord_id,
            json.dumps({"report_id": report_id, "match_id": report["match_id"]}),
        )
        await db.commit()

    return {"status": "ok", "report_id": report_id}


# ---------------------------------------------------------------------------
# Stream-Marker pro Match umschalten
# ---------------------------------------------------------------------------

@router.patch("/admin/tournaments/{tournament_id}/matches/{match_id}/stream")
async def set_match_stream_flag(
    tournament_id: int,
    match_id: int,
    on_stream: bool = Body(..., embed=True),
    user: UserSession = Depends(require_mod),
) -> dict:
    """Markiert ein Bracket-Match als 'auf Stream' oder 'parallel/off-stream'."""
    async with get_db() as db:
        await _load_bracket_match(db, tournament_id, match_id)
        await db.execute(
            "UPDATE bracket_matches SET on_stream = ? WHERE id = ? AND tournament_id = ?",
            (1 if on_stream else 0, match_id, tournament_id),
        )
        await _audit(
            db,
            "match_stream_flag",
            user.discord_id,
            json.dumps({"match_id": match_id, "on_stream": on_stream}),
        )
        await db.commit()

    return {"status": "ok", "match_id": match_id, "on_stream": on_stream}
