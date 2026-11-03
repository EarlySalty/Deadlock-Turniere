use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use turnier_engine::reschedule_league_plan_in_tx;

use crate::error::{WebError, WebResult};
use crate::extract::ModUser;
use crate::state::AppState;

use super::helpers::audit;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/api/admin/tournaments/{tournament_id}/league/schedule",
        post(reschedule_plan),
    )
}

#[derive(Debug, Deserialize)]
struct ScheduleBody {
    start: DateTime<Utc>,
    slot_minutes: i64,
}

async fn reschedule_plan(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
    Json(body): Json<ScheduleBody>,
) -> WebResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    turnier_scheduler::acquire_tournament_schedule_lock(&mut tx).await?;
    let row = sqlx::query(
        r#"SELECT status, tournament_mode FROM turnier."tournaments" WHERE id = $1 FOR UPDATE"#,
    )
    .bind(tournament_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| WebError::not_found("Turnier nicht gefunden"))?;
    let status: String = row.get("status");
    let mode: String = row.get("tournament_mode");
    if mode != "league" {
        return Err(WebError::bad_request(
            "Nur Turniere im Format Jeder gegen jeden haben einen Rundenplan",
        ));
    }
    if status != "group_phase" {
        return Err(WebError::bad_request(
            "Der Rundenplan lässt sich nur ändern, solange das Turnier läuft",
        ));
    }
    let updated =
        reschedule_league_plan_in_tx(&mut tx, tournament_id, body.start, body.slot_minutes).await?;
    if updated == 0 {
        return Err(WebError::bad_request(
            "Für dieses Turnier gibt es noch keinen Rundenplan",
        ));
    }
    audit(
        &mut *tx,
        "league_schedule_update",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "start": body.start,
            "slot_minutes": body.slot_minutes,
            "matches": updated,
        }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "matches_updated": updated })))
}
