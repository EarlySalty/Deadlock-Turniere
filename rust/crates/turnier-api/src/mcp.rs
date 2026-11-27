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
enum View {
    Detail,
    Bracket,
    Groups,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Query {
    List,
    Tournament(View, i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolKind {
    List,
    Tournament(View),
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
        kind: ToolKind::Tournament(View::Detail),
    },
    Tool {
        name: "turnier_bracket",
        description:
            "Öffentlicher Turnierbaum eines Turniers mit Paarungen, Serienstand und Siegern.",
        kind: ToolKind::Tournament(View::Bracket),
    },
    Tool {
        name: "turnier_groups",
        description: "Öffentliche Gruppentabellen und Gruppenspiele eines Turniers.",
        kind: ToolKind::Tournament(View::Groups),
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
    let view = match kind {
        ToolKind::List if object.is_empty() => return Ok(Query::List),
        ToolKind::List => return Err("Unerlaubte Parameter"),
        ToolKind::Tournament(view) => view,
    };
    if object.len() != 1 {
        return Err("Genau tournament_id erforderlich");
    }
    let tournament_id = object
        .get("tournament_id")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or("tournament_id muss eine positive Zahl sein")?;
    Ok(Query::Tournament(view, tournament_id))
}

async fn run(state: &AppState, query: Query) -> WebResult<Option<Value>> {
    let (view, tournament_id) = match query {
        Query::List => return list(state).await.map(Some),
        Query::Tournament(view, id) => (view, id),
    };
    let Some(tournament) = visible_tournament(state, tournament_id).await? else {
        return Ok(None);
    };
    let teams = load_teams(state, tournament_id).await?;
    let value = match view {
        View::Detail => detail(state, tournament, teams).await?,
        View::Bracket => bracket(state, tournament_id, &teams).await?,
        View::Groups => groups(state, tournament_id, &teams).await?,
    };
    Ok(Some(value))
}

async fn list(state: &AppState) -> WebResult<Value> {
    let tournaments = helpers::list_tournament_dtos(
        &state.pool,
        "WHERE status != 'draft' AND is_test = false ORDER BY created_at DESC, id DESC LIMIT 50",
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

#[derive(sqlx::FromRow)]
struct MemberRow {
    team_id: i64,
    discord_id: i64,
    discord_name: Option<String>,
}

#[derive(Default)]
struct NameEvidence {
    guest_names: Vec<String>,
    members: HashMap<i64, Vec<(i64, String)>>,
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
    let evidence = load_name_evidence(state, tournament_id).await?;
    Ok(rows
        .into_iter()
        .map(|row| team_summary(&evidence, row))
        .collect())
}

async fn load_name_evidence(state: &AppState, tournament_id: i64) -> WebResult<NameEvidence> {
    let members: Vec<MemberRow> = sqlx::query_as(
        r#"SELECT tm.team_id, tm.discord_id, tm.discord_name
         FROM turnier."team_members" tm JOIN turnier."teams" t ON t.id = tm.team_id
         WHERE t.tournament_id = $1"#,
    )
    .bind(tournament_id)
    .fetch_all(&state.pool)
    .await?;
    let signup_guests: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT discord_name FROM turnier."tournament_signups"
         WHERE tournament_id = $1 AND discord_id < 0"#,
    )
    .bind(tournament_id)
    .fetch_all(&state.pool)
    .await?;
    let mut evidence = NameEvidence::default();
    evidence
        .guest_names
        .extend(signup_guests.into_iter().flatten());
    for member in members {
        let name = member.discord_name.unwrap_or_default();
        if is_guest_participant(member.discord_id) {
            evidence.guest_names.push(name.clone());
        }
        evidence
            .members
            .entry(member.team_id)
            .or_default()
            .push((member.discord_id, name));
    }
    Ok(evidence)
}

fn team_summary(evidence: &NameEvidence, row: TeamRow) -> TeamSummary {
    let members = evidence
        .members
        .get(&row.id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let safe = team_name_is_safe(
        &row.name,
        row.captain_discord_id,
        members,
        &evidence.guest_names,
    );
    TeamSummary {
        name: if safe {
            row.name
        } else {
            format!("Team {}", row.id)
        },
        id: row.id,
        member_count: row.member_count,
        recruitment_status: row.recruitment_status,
    }
}

fn normalized(text: &str) -> String {
    text.trim().to_lowercase()
}

fn derived_base(name: &str) -> Option<&str> {
    let trimmed = name.trim();
    let without_suffix = match trimmed.rsplit_once(" (") {
        Some((head, tail))
            if tail.ends_with(')')
                && tail.len() > 1
                && tail[..tail.len() - 1].chars().all(|c| c.is_ascii_digit()) =>
        {
            head
        }
        _ => trimmed,
    };
    let cut = without_suffix.len().checked_sub(" team".len())?;
    let tail = without_suffix.get(cut..)?;
    tail.eq_ignore_ascii_case(" team")
        .then(|| &without_suffix[..cut])
}

fn may_be_truncated(name: &str) -> bool {
    name.trim().chars().count() + 5 >= turnier_engine::TEAM_NAME_MAX_CHARS
}

fn derived_from_listed_member(name: &str, members: &[(i64, String)]) -> bool {
    let key = normalized(name);
    let base = derived_base(name).map(normalized);
    members
        .iter()
        .filter(|(id, member)| !is_guest_participant(*id) && !member.trim().is_empty())
        .map(|(_, member)| normalized(member))
        .any(|member| {
            base.as_deref() == Some(member.as_str())
                || (may_be_truncated(name) && format!("{member} team").starts_with(&key))
        })
}

fn team_name_is_safe(
    name: &str,
    captain_discord_id: i64,
    members: &[(i64, String)],
    guest_names: &[String],
) -> bool {
    if is_guest_participant(captain_discord_id)
        || members.iter().any(|(id, _)| is_guest_participant(*id))
    {
        return false;
    }
    let key = normalized(name);
    let mentions_guest = guest_names
        .iter()
        .map(|guest| normalized(guest))
        .any(|guest| guest.chars().count() >= 2 && key.contains(&guest));
    if mentions_guest {
        return false;
    }
    if derived_base(name).is_some() || may_be_truncated(name) {
        return derived_from_listed_member(name, members);
    }
    true
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
    rank: usize,
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

fn pair_key(team_a: i64, team_b: i64) -> (i64, i64) {
    (team_a.min(team_b), team_a.max(team_b))
}

fn round_robin_pairs(team_ids: &[i64]) -> Vec<Vec<(i64, i64)>> {
    let mut working: Vec<Option<i64>> = team_ids.iter().copied().map(Some).collect();
    if working.len() % 2 == 1 {
        working.push(None);
    }
    let half = working.len() / 2;
    let mut rounds = Vec::new();
    for _ in 0..working.len().saturating_sub(1) {
        let pairs = working[..half]
            .iter()
            .zip(working.iter().rev())
            .filter_map(|(first, second)| Some(((*first)?, (*second)?)))
            .collect();
        rounds.push(pairs);
        if let Some(last) = working.pop() {
            working.insert(1, last);
        }
    }
    rounds
}

fn derived_rounds(team_ids: &[i64], games: &[(i64, i64, i64)]) -> HashMap<i64, i64> {
    let mut by_id = games.to_vec();
    by_id.sort_by_key(|(id, _, _)| *id);
    let by_pair: HashMap<(i64, i64), i64> = by_id
        .iter()
        .map(|&(id, team1, team2)| (pair_key(team1, team2), id))
        .collect();
    let mut rounds = HashMap::new();
    let mut filled = 0;
    for (index, pairs) in round_robin_pairs(team_ids).iter().enumerate() {
        let found: Vec<i64> = pairs
            .iter()
            .filter_map(|&(team1, team2)| by_pair.get(&pair_key(team1, team2)).copied())
            .collect();
        if !found.is_empty() {
            filled += 1;
        }
        for id in found {
            rounds.insert(id, index as i64 + 1);
        }
    }
    for (id, _, _) in by_id {
        rounds.entry(id).or_insert(filled + 1);
    }
    rounds
}

fn group_games(names: &HashMap<i64, String>, group: &Group) -> Vec<GroupGame> {
    let mut seeded: Vec<(i64, i64)> = group.teams.iter().map(|t| (t.id, t.team_id)).collect();
    seeded.sort_unstable();
    let team_ids: Vec<i64> = seeded.into_iter().map(|(_, team_id)| team_id).collect();
    let pairs: Vec<(i64, i64, i64)> = group
        .matches
        .iter()
        .map(|m| (m.id, m.team1_id, m.team2_id))
        .collect();
    let derived = derived_rounds(&team_ids, &pairs);
    let mut games: Vec<GroupGame> = group
        .matches
        .iter()
        .map(|m| GroupGame {
            id: m.id,
            round: m.round.or_else(|| derived.get(&m.id).copied()),
            status: m.status,
            team1: resolve(names, Some(m.team1_id)),
            team2: resolve(names, Some(m.team2_id)),
            winner: resolve(names, m.winner_id),
            scheduled_at: m.scheduled_at.clone(),
            played_at: m.played_at.clone(),
        })
        .collect();
    games.sort_by_key(|game| (game.round, game.id));
    games
}

fn group_summary(names: &HashMap<i64, String>, mut group: Group, league: bool) -> GroupSummary {
    let matches = group_games(names, &group);
    if !league {
        group.teams.sort_by(|a, b| {
            b.points
                .cmp(&a.points)
                .then(b.wins.cmp(&a.wins))
                .then(a.id.cmp(&b.id))
        });
    }
    GroupSummary {
        id: group.id,
        name: group.name,
        standings: group
            .teams
            .into_iter()
            .enumerate()
            .map(|(index, team)| Standing {
                rank: index + 1,
                team: resolve(names, Some(team.team_id)).unwrap_or_default(),
                wins: team.wins,
                losses: team.losses,
                points: team.points,
            })
            .collect(),
        matches,
    }
}

async fn groups(state: &AppState, tournament_id: i64, teams: &[TeamSummary]) -> WebResult<Value> {
    let names = team_names(teams);
    let league = turnier_match::league::tournament_is_league(&state.pool, tournament_id).await?;
    let groups: Vec<GroupSummary> = helpers::load_groups_for_tournament(&state.pool, tournament_id)
        .await?
        .into_iter()
        .map(|group| group_summary(&names, group, league))
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
            assert!(parse_arguments(ToolKind::Tournament(View::Detail), &arguments).is_err());
        }
        assert_eq!(
            parse_arguments(
                ToolKind::Tournament(View::Bracket),
                &json!({"tournament_id": 12})
            ),
            Ok(Query::Tournament(View::Bracket, 12))
        );
        assert!(parse_arguments(ToolKind::List, &json!({"limit": 500})).is_err());
        assert_eq!(parse_arguments(ToolKind::List, &json!({})), Ok(Query::List));
    }

    #[test]
    fn team_names_that_may_carry_a_guest_name_are_masked() {
        let discord = 910_000_000_000_010_002;
        let nova = [(discord, "Nova".to_string())];
        let guests = ["Jörg".to_string()];
        assert!(!team_name_is_safe("Team Bravo", -42, &[], &[]));
        assert!(!team_name_is_safe(
            "Team Bravo",
            discord,
            &[(discord, "Nova".to_string()), (-3, "Mara".to_string())],
            &[]
        ));
        assert!(!team_name_is_safe("Jörg Team", discord, &nova, &guests));
        assert!(!team_name_is_safe(
            "Die Jörg-Bande",
            discord,
            &nova,
            &guests
        ));
        assert!(!team_name_is_safe("Mara Team", discord, &nova, &[]));
        assert!(!team_name_is_safe("Mara Team (2)", discord, &nova, &[]));
        assert!(!team_name_is_safe(&"M".repeat(32), discord, &nova, &[]));
        assert!(team_name_is_safe("Nova Team", discord, &nova, &guests));
        assert!(team_name_is_safe("nova team (3)", discord, &nova, &guests));
        assert!(team_name_is_safe("Team Bravo", discord, &nova, &guests));
        assert!(team_name_is_safe("Team Bravo", 0, &[], &[]));
    }

    fn group_team(id: i64, team_id: i64, wins: i64, points: i64) -> turnier_core::GroupTeam {
        turnier_core::GroupTeam {
            id,
            group_id: 1,
            team_id,
            team_name: String::new(),
            wins,
            losses: 0,
            points,
        }
    }

    fn group_match(id: i64, team1_id: i64, team2_id: i64) -> turnier_core::GroupMatch {
        turnier_core::GroupMatch {
            id,
            group_id: 1,
            team1_id,
            team2_id,
            winner_id: None,
            status: MatchStatus::Pending,
            steam_party_id: None,
            party_code: None,
            deadlock_match_id: None,
            match_duration_s: None,
            match_stats: None,
            hero_assignments: None,
            scheduled_at: None,
            played_at: None,
            round: None,
        }
    }

    fn group(matches: Vec<turnier_core::GroupMatch>) -> Group {
        Group {
            id: 1,
            tournament_id: 1,
            name: "A".to_string(),
            seeding_order: 0,
            teams: vec![
                group_team(3, 30, 1, 3),
                group_team(1, 10, 1, 3),
                group_team(2, 20, 0, 0),
            ],
            matches,
        }
    }

    fn rounds(summary: &GroupSummary) -> Vec<(i64, Option<i64>)> {
        summary.matches.iter().map(|m| (m.id, m.round)).collect()
    }

    #[test]
    fn group_games_get_the_site_round_robin_rounds() {
        let names = HashMap::new();
        let full = group(vec![
            group_match(9, 20, 30),
            group_match(7, 10, 20),
            group_match(8, 10, 99),
            group_match(5, 30, 10),
            group_match(6, 30, 20),
        ]);
        let summary = group_summary(&names, full, false);
        assert_eq!(
            rounds(&summary),
            [
                (9, Some(1)),
                (5, Some(2)),
                (7, Some(3)),
                (6, Some(4)),
                (8, Some(4))
            ]
        );
        let order: Vec<(usize, &str)> = summary
            .standings
            .iter()
            .map(|s| (s.rank, s.team.as_str()))
            .collect();
        assert_eq!(order, [(1, "Team 10"), (2, "Team 30"), (3, "Team 20")]);

        let first_round_missing = group(vec![
            group_match(8, 10, 99),
            group_match(7, 10, 20),
            group_match(5, 30, 10),
        ]);
        assert_eq!(
            rounds(&group_summary(&names, first_round_missing, false)),
            [(5, Some(2)), (7, Some(3)), (8, Some(3))]
        );

        let mut scheduled = group_match(4, 10, 20);
        scheduled.round = Some(7);
        let kept = group(vec![scheduled]);
        assert_eq!(rounds(&group_summary(&names, kept, true)), [(4, Some(7))]);
    }

    #[test]
    fn derived_names_are_recognised_with_collision_suffix() {
        assert_eq!(derived_base("Mara Team"), Some("Mara"));
        assert_eq!(derived_base("Mara Team (12)"), Some("Mara"));
        assert_eq!(derived_base("Team Alpha"), None);
        assert_eq!(derived_base("Mara Team (x)"), None);
        assert_eq!(derived_base("MARA TEAM"), Some("MARA"));
        assert_eq!(derived_base("Jörg Team"), Some("Jörg"));
        assert_eq!(derived_base("Team"), None);
        assert_eq!(derived_base("üteam"), None);
    }
}
