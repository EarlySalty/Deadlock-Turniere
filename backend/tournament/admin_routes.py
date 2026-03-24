"""Admin/Mod-only Routes — Tournament-Management und Ergebnis-Eintragung."""
from __future__ import annotations

import json

from fastapi import APIRouter, Depends, HTTPException, status

from auth.permissions import require_admin, require_mod
from db import get_db
from tournament.engine import (
    VALID_STATUS_TRANSITIONS,
    advance_bracket_winner,
    assign_random_teams,
    generate_bracket,
    generate_group_matches,
    generate_groups,
)
from tournament.models import (
    Tournament,
    TournamentCreate,
    TournamentUpdate,
    UserSession,
)

router = APIRouter(prefix="/api/admin", tags=["admin"])


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

async def _audit(db, action: str, user_id: str, details: str) -> None:  # noqa: ANN001
    """Schreibt einen Eintrag in den Audit-Log."""
    await db.execute(
        "INSERT INTO audit_log (action, user_id, details) VALUES (?, ?, ?)",
        (action, user_id, details),
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
        cursor = await db.execute(
            "INSERT INTO tournaments "
            "(name, description, team_size, bracket_format, registration_start, "
            "registration_end, group_phase_start, bracket_start, created_by) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                body.name,
                body.description,
                body.team_size,
                body.bracket_format.value,
                body.registration_start,
                body.registration_end,
                body.group_phase_start,
                body.bracket_start,
                user.discord_id,
            ),
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
    """Turnier-Daten aktualisieren (Mod+). Status-Uebergaenge werden validiert."""
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

        # Status-Uebergang validieren
        if body.status is not None and body.status.value != existing["status"]:
            current_status = existing["status"]
            allowed = VALID_STATUS_TRANSITIONS.get(current_status, [])
            if body.status.value not in allowed:
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail=f"Ungueliger Status-Uebergang: {current_status} -> {body.status.value}. "
                    f"Erlaubt: {', '.join(allowed) if allowed else 'keine'}",
                )

        # Nur gesetzte Felder updaten
        updates: list[str] = []
        params: list = []
        update_data = body.model_dump(exclude_unset=True)
        for field, value in update_data.items():
            if hasattr(value, "value"):
                value = value.value
            updates.append(f"{field} = ?")
            params.append(value)

        if not updates:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Keine Aenderungen angegeben",
            )

        updates.append("updated_at = datetime('now')")
        params.append(tournament_id)

        await db.execute(
            f"UPDATE tournaments SET {', '.join(updates)} WHERE id = ?",  # noqa: S608
            params,
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
# DELETE /api/admin/tournaments/{id} — Turnier loeschen
# ---------------------------------------------------------------------------

@router.delete("/tournaments/{tournament_id}", status_code=200)
async def delete_tournament(
    tournament_id: int,
    user: UserSession = Depends(require_admin),
) -> dict:
    """Turnier loeschen (Admin only). Nur im Draft-Status moeglich."""
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

        if existing["status"] != "draft":
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Nur Turniere im Draft-Status koennen geloescht werden",
            )

        # Kaskadierend loeschen
        await db.execute("DELETE FROM team_members WHERE team_id IN (SELECT id FROM teams WHERE tournament_id = ?)", (tournament_id,))
        await db.execute("DELETE FROM tournament_signups WHERE tournament_id = ?", (tournament_id,))
        await db.execute("DELETE FROM teams WHERE tournament_id = ?", (tournament_id,))
        await db.execute("DELETE FROM tournaments WHERE id = ?", (tournament_id,))

        await _audit(
            db,
            "tournament_delete",
            user.discord_id,
            json.dumps({"tournament_id": tournament_id, "name": existing["name"]}),
        )
        await db.commit()

    return {"status": "geloescht", "tournament_id": tournament_id}


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments/{id}/advance — Phase weiterschalten
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/advance", response_model=Tournament)
async def advance_tournament(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> Tournament:
    """Turnier zur naechsten Phase weiterschalten (Mod+)."""
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
        allowed = VALID_STATUS_TRANSITIONS.get(current_status, [])
        if not allowed:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail=f"Keine weitere Phase moeglich (aktuell: {current_status})",
            )

        next_status = allowed[0]

        await db.execute(
            "UPDATE tournaments SET status = ?, updated_at = datetime('now') WHERE id = ?",
            (next_status, tournament_id),
        )

        await _audit(
            db,
            "tournament_advance",
            user.discord_id,
            json.dumps({
                "tournament_id": tournament_id,
                "from": current_status,
                "to": next_status,
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
# POST /api/admin/tournaments/{id}/assign-random — Solo-Spieler verteilen
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/assign-random", status_code=200)
async def assign_random(
    tournament_id: int,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Solo-Anmeldungen zufaellig auf Teams verteilen (Mod+)."""
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
                detail="Team-Zuweisung nur waehrend der Registration moeglich",
            )

    teams_created = await assign_random_teams(tournament_id, tournament["team_size"])
    return {"status": "ok", "teams_created": teams_created}


# ---------------------------------------------------------------------------
# POST /api/admin/tournaments/{id}/matches/{match_id}/result — Ergebnis
# ---------------------------------------------------------------------------

@router.post("/tournaments/{tournament_id}/matches/{match_id}/result", status_code=200)
async def set_match_result(
    tournament_id: int,
    match_id: int,
    body: dict,
    user: UserSession = Depends(require_mod),
) -> dict:
    """Manuelles Match-Ergebnis eintragen (Mod+)."""
    winner_id = body.get("winner_id")
    if not winner_id or not isinstance(winner_id, int):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="winner_id (int) ist erforderlich",
        )

    async with get_db() as db:
        # Turnier pruefen
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
            # Bracket-Match: winner_id setzen
            if winner_id not in (bracket_match["team1_id"], bracket_match["team2_id"]):
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="winner_id muss eines der beiden Teams im Match sein",
                )

            await db.execute(
                "UPDATE bracket_matches SET winner_id = ?, status = 'completed', "
                "played_at = datetime('now') WHERE id = ?",
                (winner_id, match_id),
            )

            # Match-Result erstellen
            await db.execute(
                "INSERT INTO match_results (bracket_match_id, winning_team, source) "
                "VALUES (?, ?, 'manual')",
                (match_id, winner_id),
            )

            await _audit(
                db,
                "match_result_bracket",
                user.discord_id,
                json.dumps({
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                    "winner_id": winner_id,
                }),
            )
            await db.commit()

            # Winner in naechste Runde propagieren
            await advance_bracket_winner(tournament_id, match_id, winner_id)

            return {"status": "ok", "match_type": "bracket", "match_id": match_id, "winner_id": winner_id}

        # Dann in group_matches suchen
        cursor = await db.execute(
            "SELECT gm.* FROM group_matches gm "
            "JOIN groups g ON gm.group_id = g.id "
            "WHERE gm.id = ? AND g.tournament_id = ?",
            (match_id, tournament_id),
        )
        group_match = await cursor.fetchone()

        if group_match:
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
