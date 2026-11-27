use std::collections::HashMap;
use std::net::SocketAddr;

use axum::body::to_bytes;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::header::CACHE_CONTROL;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;
use serde_json::{json, Map, Value};

use turnier_core::{
    is_guest_participant, BracketFormat, BracketMatch, BracketType, Group, MatchStatus,
    RecruitmentStatus, Tournament, TournamentGameMode, TournamentMode, TournamentStatus,
};

use crate::error::WebResult;
use crate::internal_scrims::require_internal_boundary;
use crate::public::helpers;
use crate::state::AppState;

pub const ROUTE: &str = "/internal/turnier/v1/mcp";
const PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_BODY_BYTES: usize = 64 * 1024;
const NOT_FOUND: &str = "Turnier nicht gefunden";
const UNAVAILABLE: &str = "Turnierdaten gerade nicht verfügbar";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Query {
    List,
    Detail(i64),
    Bracket(i64),
    Groups(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolKind {
    List,
    Detail,
    Bracket,
    Groups,
}

struct Tool {
    name: &'static str,
    description: &'static str,
    kind: ToolKind,
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "turnier_list",
        description:
            "Öffentliche Turniere der Community, neueste zuerst, ohne Entwürfe und Testturniere.",
        kind: ToolKind::List,
    },
    Tool {
        name: "turnier_get",
        description:
            "Öffentliche Eckdaten eines Turniers mit Teams und Anzahl der Solo-Anmeldungen.",
        kind: ToolKind::Detail,
    },
    Tool {
        name: "turnier_bracket",
        description:
            "Öffentlicher Turnierbaum eines Turniers mit Paarungen, Serienstand und Siegern.",
        kind: ToolKind::Bracket,
    },
    Tool {
        name: "turnier_groups",
        description: "Öffentliche Gruppentabellen und Gruppenspiele eines Turniers.",
        kind: ToolKind::Groups,
    },
];

pub fn router() -> Router<AppState> {
    Router::new().route(ROUTE, post(mcp))
}

async fn mcp(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
) -> Response {
    if let Err(error) = require_internal_boundary(peer, request.headers(), &state) {
        return error.into_response();
    }
    let Ok(body) = to_bytes(request.into_body(), MAX_BODY_BYTES).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        return reply(rpc_error(Value::Null, -32700, "Ungültiges JSON"));
    };
    if is_notification(&message) {
        return (StatusCode::ACCEPTED, [(CACHE_CONTROL, "no-store")]).into_response();
    }
    reply(dispatch(&state, &message).await)
}

fn reply(value: Value) -> Response {
    (StatusCode::OK, [(CACHE_CONTROL, "no-store")], Json(value)).into_response()
}

fn is_notification(message: &Value) -> bool {
    message.is_object() && message.get("id").is_none()
}

fn valid_id(id: &Value) -> bool {
    match id {
        Value::String(text) => text.len() <= 128,
        Value::Number(_) | Value::Null => true,
        _ => false,
    }
}

async fn dispatch(state: &AppState, request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if !valid_id(&id) {
        return rpc_error(Value::Null, -32600, "Ungültige Request-ID");
    }
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return rpc_error(id, -32600, "Ungültiger JSON-RPC-Request");
    }
    match request.get("method").and_then(Value::as_str) {
        Some("initialize") => rpc_ok(id, initialize()),
        Some("ping") => rpc_ok(id, json!({})),
        Some("tools/list") => rpc_ok(id, json!({ "tools": definitions() })),
        Some("tools/call") => call(state, id, request).await,
        Some(_) => rpc_error(id, -32601, "Methode nicht verfügbar"),
        None => rpc_error(id, -32600, "Methode fehlt"),
    }
}

fn initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {"name": "deadlock-turnier-mcp", "version": env!("CARGO_PKG_VERSION")}
    })
}

async fn call(state: &AppState, id: Value, request: &Value) -> Value {
    let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
        return rpc_error(id, -32602, "Tool-Name fehlt");
    };
    let Some(tool) = TOOLS.iter().find(|tool| tool.name == name) else {
        return rpc_error(id, -32602, "Unbekanntes Tool");
    };
    let empty = Value::Object(Map::new());
    let arguments = match request.pointer("/params/arguments") {
        None | Some(Value::Null) => &empty,
        Some(arguments) => arguments,
    };
    let query = match parse_arguments(tool.kind, arguments) {
        Ok(query) => query,
        Err(message) => return rpc_ok(id, tool_error(message)),
    };
    let result = match run(state, query).await {
        Ok(Some(data)) => tool_ok(data),
        Ok(None) => tool_error(NOT_FOUND),
        Err(_) => {
            tracing::warn!(tool = tool.name, "turnier mcp read failed");
            tool_error(UNAVAILABLE)
        }
    };
    rpc_ok(id, result)
}

fn definitions() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": schema(tool.kind),
                "annotations": {"readOnlyHint": true, "destructiveHint": false, "openWorldHint": false}
            })
        })
        .collect()
}

fn schema(kind: ToolKind) -> Value {
    if kind == ToolKind::List {
        return json!({"type": "object", "additionalProperties": false, "properties": {}});
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {"tournament_id": {"type": "integer", "minimum": 1}},
        "required": ["tournament_id"]
    })
}

fn parse_arguments(kind: ToolKind, arguments: &Value) -> Result<Query, &'static str> {
    let object = arguments
        .as_object()
        .ok_or("Argumente müssen ein Objekt sein")?;
    if kind == ToolKind::List {
        return if object.is_empty() {
            Ok(Query::List)
        } else {
            Err("Unerlaubte Parameter")
        };
    }
    if object.len() != 1 {
        return Err("Genau tournament_id erforderlich");
    }
    let tournament_id = object
        .get("tournament_id")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or("tournament_id muss eine positive Zahl sein")?;
    Ok(match kind {
        ToolKind::List => Query::List,
        ToolKind::Detail => Query::Detail(tournament_id),
        ToolKind::Bracket => Query::Bracket(tournament_id),
        ToolKind::Groups => Query::Groups(tournament_id),
    })
}

async fn run(state: &AppState, query: Query) -> WebResult<Option<Value>> {
    let tournament_id = match query {
        Query::List => return list(state).await.map(Some),
        Query::Detail(id) | Query::Bracket(id) | Query::Groups(id) => id,
    };
    let Some(tournament) = visible_tournament(state, tournament_id).await? else {
        return Ok(None);
    };
    let teams = load_teams(state, tournament_id).await?;
    let value = match query {
        Query::Bracket(_) => bracket(state, tournament_id, &teams).await?,
        Query::Groups(_) => groups(state, tournament_id, &teams).await?,
        Query::List | Query::Detail(_) => detail(state, tournament, teams).await?,
    };
    Ok(Some(value))
}

async fn list(state: &AppState) -> WebResult<Value> {
    let tournaments = helpers::list_tournament_dtos(
        &state.pool,
        "WHERE status != 'draft' AND is_test = false ORDER BY created_at DESC LIMIT 50",
    )
    .await?;
    let summaries: Vec<TournamentSummary> = tournaments
        .into_iter()
        .map(TournamentSummary::from)
        .collect();
    Ok(json!({ "tournaments": summaries }))
}

async fn visible_tournament(state: &AppState, tournament_id: i64) -> WebResult<Option<Tournament>> {
    let row = sqlx::query(&format!("{} WHERE id = $1", helpers::tournament_select()))
        .bind(tournament_id)
        .fetch_optional(&state.pool)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let tournament = helpers::tournament_from_row(&row)?;
    Ok(is_public(&tournament).then_some(tournament))
}

fn is_public(tournament: &Tournament) -> bool {
    tournament.status != TournamentStatus::Draft && !tournament.is_test
}

#[derive(sqlx::FromRow)]
struct TeamRow {
    id: i64,
    name: String,
    captain_discord_id: i64,
    recruitment_status: RecruitmentStatus,
    member_count: i64,
}

#[derive(Serialize)]
struct TeamSummary {
    id: i64,
    name: String,
    member_count: i64,
    recruitment_status: RecruitmentStatus,
}

async fn load_teams(state: &AppState, tournament_id: i64) -> WebResult<Vec<TeamSummary>> {
    let rows: Vec<TeamRow> = sqlx::query_as(
        r#"SELECT t.id, t.name, t.captain_discord_id, t.recruitment_status,
                (SELECT COUNT(*) FROM turnier."team_members" tm WHERE tm.team_id = t.id) AS member_count
         FROM turnier."teams" t WHERE t.tournament_id = $1 ORDER BY t.id"#,
    )
    .bind(tournament_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows.into_iter().map(team_summary).collect())
}

fn team_summary(row: TeamRow) -> TeamSummary {
    TeamSummary {
        name: public_team_name(row.id, &row.name, row.captain_discord_id),
        id: row.id,
        member_count: row.member_count,
        recruitment_status: row.recruitment_status,
    }
}

fn public_team_name(team_id: i64, name: &str, captain_discord_id: i64) -> String {
    if is_guest_participant(captain_discord_id) {
        format!("Team {team_id}")
    } else {
        name.to_string()
    }
}

#[derive(Serialize)]
struct TournamentSummary {
    id: i64,
    name: String,
    status: TournamentStatus,
    registration_open: bool,
    description: Option<String>,
    team_size: i64,
    series_format: i64,
    final_series_format: Option<i64>,
    tournament_mode: TournamentMode,
    tournament_game_mode: TournamentGameMode,
    bracket_format: BracketFormat,
    registration_start: Option<String>,
    registration_end: Option<String>,
    checkin_start: Option<String>,
    group_phase_start: Option<String>,
    bracket_start: Option<String>,
}

impl From<Tournament> for TournamentSummary {
    fn from(t: Tournament) -> Self {
        Self {
            id: t.id,
            name: t.name,
            status: t.status,
            registration_open: t.registration_open,
            description: t.description,
            team_size: t.team_size,
            series_format: t.series_format,
            final_series_format: t.final_series_format,
            tournament_mode: t.tournament_mode,
            tournament_game_mode: t.tournament_game_mode,
            bracket_format: t.played_bracket_format.unwrap_or(t.bracket_format),
            registration_start: t.registration_start,
            registration_end: t.registration_end,
            checkin_start: t.checkin_start,
            group_phase_start: t.group_phase_start,
            bracket_start: t.bracket_start,
        }
    }
}

async fn detail(
    state: &AppState,
    tournament: Tournament,
    teams: Vec<TeamSummary>,
) -> WebResult<Value> {
    let solo_signups: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM turnier."tournament_signups"
         WHERE tournament_id = $1 AND team_id IS NULL"#,
    )
    .bind(tournament.id)
    .fetch_one(&state.pool)
    .await?;
    let rules = tournament.rules.clone();
    Ok(json!({
        "tournament": TournamentSummary::from(tournament),
        "rules": rules,
        "teams": teams,
        "solo_signup_count": solo_signups,
    }))
}

#[derive(Serialize)]
struct MatchSummary {
    id: i64,
    round: i64,
    position: i64,
    bracket_type: BracketType,
    status: MatchStatus,
    team1: Option<String>,
    team2: Option<String>,
    winner: Option<String>,
    series_wins_team1: i64,
    series_wins_team2: i64,
    scheduled_at: Option<String>,
    played_at: Option<String>,
    on_stream: bool,
}

fn team_names(teams: &[TeamSummary]) -> HashMap<i64, String> {
    teams
        .iter()
        .map(|team| (team.id, team.name.clone()))
        .collect()
}

fn resolve(names: &HashMap<i64, String>, team_id: Option<i64>) -> Option<String> {
    team_id.map(|id| {
        names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("Team {id}"))
    })
}

fn match_summary(names: &HashMap<i64, String>, m: BracketMatch) -> MatchSummary {
    MatchSummary {
        id: m.id,
        round: m.round,
        position: m.position,
        bracket_type: m.bracket_type,
        status: m.status,
        team1: resolve(names, m.team1_id),
        team2: resolve(names, m.team2_id),
        winner: resolve(names, m.winner_id),
        series_wins_team1: m.series_wins_team1,
        series_wins_team2: m.series_wins_team2,
        scheduled_at: m.scheduled_at,
        played_at: m.played_at,
        on_stream: m.on_stream,
    }
}

async fn with_series(state: &AppState, mut m: BracketMatch) -> WebResult<BracketMatch> {
    let games = turnier_match::series::get_series_games(&state.pool, m.id).await?;
    let wins = |slot| games.iter().filter(|g| g.winner_team == Some(slot)).count() as i64;
    m.series_wins_team1 = wins(1);
    m.series_wins_team2 = wins(2);
    Ok(m)
}

async fn bracket(state: &AppState, tournament_id: i64, teams: &[TeamSummary]) -> WebResult<Value> {
    let names = team_names(teams);
    let mut matches = Vec::new();
    for m in helpers::load_bracket_matches(&state.pool, tournament_id).await? {
        matches.push(match_summary(&names, with_series(state, m).await?));
    }
    Ok(json!({ "tournament_id": tournament_id, "matches": matches }))
}

#[derive(Serialize)]
struct Standing {
    team: String,
    wins: i64,
    losses: i64,
    points: i64,
}

#[derive(Serialize)]
struct GroupGame {
    id: i64,
    round: Option<i64>,
    status: MatchStatus,
    team1: Option<String>,
    team2: Option<String>,
    winner: Option<String>,
    scheduled_at: Option<String>,
    played_at: Option<String>,
}

#[derive(Serialize)]
struct GroupSummary {
    id: i64,
    name: String,
    standings: Vec<Standing>,
    matches: Vec<GroupGame>,
}

fn group_summary(names: &HashMap<i64, String>, group: Group) -> GroupSummary {
    GroupSummary {
        id: group.id,
        name: group.name,
        standings: group
            .teams
            .into_iter()
            .map(|team| Standing {
                team: resolve(names, Some(team.team_id)).unwrap_or_default(),
                wins: team.wins,
                losses: team.losses,
                points: team.points,
            })
            .collect(),
        matches: group
            .matches
            .into_iter()
            .map(|m| GroupGame {
                id: m.id,
                round: m.round,
                status: m.status,
                team1: resolve(names, Some(m.team1_id)),
                team2: resolve(names, Some(m.team2_id)),
                winner: resolve(names, m.winner_id),
                scheduled_at: m.scheduled_at,
                played_at: m.played_at,
            })
            .collect(),
    }
}

async fn groups(state: &AppState, tournament_id: i64, teams: &[TeamSummary]) -> WebResult<Value> {
    let names = team_names(teams);
    let groups: Vec<GroupSummary> = helpers::load_groups_for_tournament(&state.pool, tournament_id)
        .await?
        .into_iter()
        .map(|group| group_summary(&names, group))
        .collect();
    Ok(json!({ "tournament_id": tournament_id, "groups": groups }))
}

fn rpc_ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: Value, code: i64, message: &'static str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_ok(data: Value) -> Value {
    json!({"content": [{"type": "text", "text": data.to_string()}], "structuredContent": data, "isError": false})
}

fn tool_error(message: &'static str) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_list_is_exactly_the_public_read_contract() {
        let names: Vec<&str> = TOOLS.iter().map(|tool| tool.name).collect();
        assert_eq!(
            names,
            [
                "turnier_list",
                "turnier_get",
                "turnier_bracket",
                "turnier_groups"
            ]
        );
        for definition in definitions() {
            assert_eq!(definition["inputSchema"]["additionalProperties"], false);
            assert_eq!(definition["annotations"]["readOnlyHint"], true);
        }
    }

    #[test]
    fn arguments_reject_identity_paths_and_extra_fields() {
        for arguments in [
            json!({"tournament_id": 1, "user_id": "999"}),
            json!({"tournament_id": 1, "roles": ["admin"]}),
            json!({"tournament_id": "../admin"}),
            json!({"tournament_id": -1}),
            json!({"tournament_id": 0}),
            json!({"tournament_id": 1.5}),
            json!({}),
            json!([1]),
        ] {
            assert!(parse_arguments(ToolKind::Detail, &arguments).is_err());
        }
        assert_eq!(
            parse_arguments(ToolKind::Bracket, &json!({"tournament_id": 12})),
            Ok(Query::Bracket(12))
        );
        assert!(parse_arguments(ToolKind::List, &json!({"limit": 500})).is_err());
        assert_eq!(parse_arguments(ToolKind::List, &json!({})), Ok(Query::List));
    }

    #[test]
    fn guest_captain_team_names_are_masked() {
        assert_eq!(public_team_name(7, "Team Jörg", -42), "Team 7");
        assert_eq!(
            public_team_name(8, "Team Bravo", 910_000_000_000_010_002),
            "Team Bravo"
        );
    }
}
