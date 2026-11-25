//! MCP-Adapter für Turniere. Sicherheitsgrenze: Kein Modellparameter kann
//! eine Identität, Sitzung oder Rolle setzen. Geschützte Aktionen verwenden
//! ausschließlich die vorhandene User-Session UND den aktuellen Discord-Stand.
//! Die Geschäftslogik und die Eigentümer-/Consent-/Statusprüfungen verbleiben
//! in den regulären HTTP-Handlern.
use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::http::header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Map, Value};
use tower::ServiceExt;
use turnier_steam::{DiscordMemberClient, ReqwestDiscordClient};

use crate::state::AppState;

const MAX_RESULT_BYTES: usize = 128 * 1024;
const MAX_PAYLOAD_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Access {
    Public,
    Member,
    Mod,
    Admin,
}

#[derive(Clone, Copy)]
enum RouteKind {
    List,
    One,
    Bracket,
    Groups,
    MyStatus,
    Signup,
    CancelSignup,
    Checkin,
    CreateTeam,
    JoinTeam,
    LeaveTeam,
    AdminList,
    AdminOne,
    CreateTournament,
    UpdateTournament,
    OpenCheckin,
    Advance,
}

#[derive(Clone, Copy)]
struct Tool {
    name: &'static str,
    description: &'static str,
    access: Access,
    kind: RouteKind,
    has_id: bool,
    has_team: bool,
    has_payload: bool,
}

const TOOLS: &[Tool] = &[
    Tool { name: "turnier_list", description: "Öffentliche Turniere auflisten.", access: Access::Public, kind: RouteKind::List, has_id: false, has_team: false, has_payload: false },
    Tool { name: "turnier_get", description: "Öffentliche Turnierdetails ansehen.", access: Access::Public, kind: RouteKind::One, has_id: true, has_team: false, has_payload: false },
    Tool { name: "turnier_bracket", description: "Öffentlichen Turnierbaum lesen.", access: Access::Public, kind: RouteKind::Bracket, has_id: true, has_team: false, has_payload: false },
    Tool { name: "turnier_groups", description: "Öffentliche Turniergruppen lesen.", access: Access::Public, kind: RouteKind::Groups, has_id: true, has_team: false, has_payload: false },
    Tool { name: "turnier_my_status", description: "Eigenen Anmeldestatus abfragen; niemals für andere Nutzer.", access: Access::Member, kind: RouteKind::MyStatus, has_id: true, has_team: false, has_payload: false },
    Tool { name: "turnier_signup", description: "Sich selbst für ein Turnier anmelden. Consent und Registrierung werden geprüft.", access: Access::Member, kind: RouteKind::Signup, has_id: true, has_team: false, has_payload: false },
    Tool { name: "turnier_cancel_signup", description: "Eigene Solo-Anmeldung zurückziehen.", access: Access::Member, kind: RouteKind::CancelSignup, has_id: true, has_team: false, has_payload: false },
    Tool { name: "turnier_checkin", description: "Eigenen Turnier-Check-in durchführen.", access: Access::Member, kind: RouteKind::Checkin, has_id: true, has_team: false, has_payload: false },
    Tool { name: "turnier_team_create", description: "Ein Team erstellen, der eingeloggte Nutzer wird Captain.", access: Access::Member, kind: RouteKind::CreateTeam, has_id: true, has_team: false, has_payload: true },
    Tool { name: "turnier_team_join", description: "Selbst einem Team beitreten.", access: Access::Member, kind: RouteKind::JoinTeam, has_id: true, has_team: true, has_payload: false },
    Tool { name: "turnier_team_leave", description: "Selbst ein Team verlassen.", access: Access::Member, kind: RouteKind::LeaveTeam, has_id: true, has_team: true, has_payload: false },
    Tool { name: "turnier_admin_list", description: "Mit mindestens Moderatorenrolle auch Entwürfe und Testturniere auflisten.", access: Access::Mod, kind: RouteKind::AdminList, has_id: false, has_team: false, has_payload: false },
    Tool { name: "turnier_admin_get", description: "Vollständige Turnierdaten mit mindestens Moderatorenrolle lesen.", access: Access::Mod, kind: RouteKind::AdminOne, has_id: true, has_team: false, has_payload: false },
    Tool { name: "turnier_create", description: "Neues Turnier erstellen. Erfordert Moderatorenrechte; Nutzereingaben vorher bestätigen.", access: Access::Mod, kind: RouteKind::CreateTournament, has_id: false, has_team: false, has_payload: true },
    Tool { name: "turnier_update", description: "Turnier ändern. Erfordert Moderatorenrechte; Änderungen vorher bestätigen.", access: Access::Mod, kind: RouteKind::UpdateTournament, has_id: true, has_team: false, has_payload: true },
    Tool { name: "turnier_open_checkin", description: "Turnier-Check-in öffnen. Erfordert Moderatorenrechte.", access: Access::Mod, kind: RouteKind::OpenCheckin, has_id: true, has_team: false, has_payload: false },
    Tool { name: "turnier_advance", description: "Turnierstatus voranstellen. Erfordert Moderatorenrechte; explizite Bestätigung einholen.", access: Access::Mod, kind: RouteKind::Advance, has_id: true, has_team: false, has_payload: false },
];

pub fn router() -> Router<AppState> {
    Router::new().route("/api/mcp", post(mcp))
}

fn schema(tool: Tool) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    if tool.has_id {
        props.insert(
            "tournament_id".into(),
            json!({"type": "integer", "minimum": 1}),
        );
        required.push("tournament_id");
    }
    if tool.has_team {
        props.insert("team_id".into(), json!({"type": "integer", "minimum": 1}));
        required.push("team_id");
    }
    if tool.has_payload {
        props.insert("payload".into(), json!({"type": "object"}));
        required.push("payload");
    }
    json!({"type":"object","additionalProperties":false,"properties":props,"required":required})
}

fn definitions(level: Access) -> Value {
    Value::Array(TOOLS.iter().copied().filter(|t| t.access <= level).map(|tool| {
        json!({"name":tool.name,"description":tool.description,"inputSchema":schema(tool)})
    }).collect())
}

fn rpc_ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

fn rpc_error(id: Value, code: i64, message: &'static str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn tool_error(message: &'static str) -> Value {
    json!({"content":[{"type":"text","text":message}],"isError":true})
}

fn bearer_token(headers: &HeaderMap) -> Result<Option<&str>, &'static str> {
    if headers.get_all(AUTHORIZATION).iter().count() > 1 {
        return Err("Genau ein Authorization-Header erlaubt");
    }
    let Some(header) = headers.get(AUTHORIZATION) else {
        return Ok(None);
    };
    let value = header
        .to_str()
        .map_err(|_| "Ungültiger Authorization-Header")?;
    let token = value
        .strip_prefix("Bearer ")
        .ok_or("Bearer-Session erforderlich")?;
    // Eigene Sessions enthalten 48 zufällige Bytes, base64url-kodiert zu 64 Zeichen.
    if token.len() != 64
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err("Ungültige Bearer-Session");
    }
    Ok(Some(token))
}

fn role_level(is_mod: bool, is_admin: bool) -> Access {
    if is_admin {
        Access::Admin
    } else if is_mod {
        Access::Mod
    } else {
        Access::Member
    }
}

// Die im Session-Snapshot erfassten Rollen sind die Obergrenze.
// Zusätzlich muss die Rolle aktuell auf Discord noch vorhanden sein.
fn effective_level(
    session: &turnier_core::UserSession,
    live_roles: &[String],
    role_sets: &turnier_auth::RoleSets,
) -> Access {
    let flags = role_sets.flags(live_roles);
    role_level(
        session.is_mod && flags.is_mod,
        session.is_admin && flags.is_admin,
    )
}

async fn identity(state: &AppState, token: Option<&str>) -> Result<Access, &'static str> {
    let Some(token) = token else {
        return Ok(Access::Public);
    };
    let session = turnier_auth::resolve_session(&state.pool, token, &state.role_sets)
        .await
        .map_err(|_| "Session fehlt oder ist abgelaufen")?;
    // Gäste haben keinen Discord-Guild-Ausweis und können über MCP
    // keine personenbezogenen Operationen vornehmen.
    if session
        .discord_id
        .parse::<u64>()
        .ok()
        .is_none_or(|id| id == 0)
    {
        return Err("Discord-Anmeldung erforderlich");
    }
    let client = ReqwestDiscordClient::from_config(&state.config)
        .ok_or("Discord-Rollenprüfung nicht verfügbar")?;
    let roles = client
        .member_role_ids(&session.discord_id)
        .await
        .map_err(|_| "Discord-Rollenprüfung nicht verfügbar")?
        .ok_or("Discord-Mitgliedschaft nicht bestätigt")?;
    Ok(effective_level(&session, &roles, &state.role_sets))
}

struct Plan {
    method: Method,
    path: String,
    body: Option<Value>,
}

fn positive_id(args: &Map<String, Value>, key: &str) -> Result<i64, &'static str> {
    args.get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or("Positive numerische ID erforderlich")
}

fn plan(tool: Tool, args: &Value) -> Result<Plan, &'static str> {
    let obj = args.as_object().ok_or("Argumente müssen ein Objekt sein")?;
    let expected =
        usize::from(tool.has_id) + usize::from(tool.has_team) + usize::from(tool.has_payload);
    if obj.len() != expected
        || obj.keys().any(|key| {
            !matches!(key.as_str(),
            "tournament_id" if tool.has_id)
                && !matches!(key.as_str(), "team_id" if tool.has_team)
                && !matches!(key.as_str(), "payload" if tool.has_payload)
        })
    {
        return Err("Unerlaubte oder fehlende Parameter");
    }
    let id = if tool.has_id {
        positive_id(obj, "tournament_id")?
    } else {
        0
    };
    let team_id = if tool.has_team {
        positive_id(obj, "team_id")?
    } else {
        0
    };
    let payload = if tool.has_payload {
        let v = obj
            .get("payload")
            .filter(|v| v.is_object())
            .ok_or("payload muss ein Objekt sein")?;
        if serde_json::to_vec(v)
            .map_err(|_| "Ungültiger payload")?
            .len()
            > MAX_PAYLOAD_BYTES
        {
            return Err("payload zu groß");
        }
        Some(v.clone())
    } else {
        None
    };
    let (method, path) = match tool.kind {
        RouteKind::List => (Method::GET, "/api/tournaments".to_string()),
        RouteKind::One => (Method::GET, format!("/api/tournaments/{id}")),
        RouteKind::Bracket => (Method::GET, format!("/api/tournaments/{id}/bracket")),
        RouteKind::Groups => (Method::GET, format!("/api/tournaments/{id}/groups")),
        RouteKind::MyStatus => (Method::GET, format!("/api/tournaments/{id}/me")),
        RouteKind::Signup => (Method::POST, format!("/api/tournaments/{id}/signup")),
        RouteKind::CancelSignup => (Method::DELETE, format!("/api/tournaments/{id}/signup")),
        RouteKind::Checkin => (Method::POST, format!("/api/tournaments/{id}/checkin")),
        RouteKind::CreateTeam => (Method::POST, format!("/api/tournaments/{id}/teams")),
        RouteKind::JoinTeam => (
            Method::POST,
            format!("/api/tournaments/{id}/teams/{team_id}/join"),
        ),
        RouteKind::LeaveTeam => (
            Method::DELETE,
            format!("/api/tournaments/{id}/teams/{team_id}/leave"),
        ),
        RouteKind::AdminList => (Method::GET, "/api/admin/tournaments".to_string()),
        RouteKind::AdminOne => (Method::GET, format!("/api/admin/tournaments/{id}")),
        RouteKind::CreateTournament => (Method::POST, "/api/admin/tournaments".to_string()),
        RouteKind::UpdateTournament => (Method::PUT, format!("/api/admin/tournaments/{id}")),
        RouteKind::OpenCheckin => (
            Method::POST,
            format!("/api/admin/tournaments/{id}/open-checkin"),
        ),
        RouteKind::Advance => (Method::POST, format!("/api/admin/tournaments/{id}/advance")),
    };
    Ok(Plan {
        method,
        path,
        body: payload,
    })
}

async fn invoke(state: AppState, token: Option<&str>, plan: Plan) -> Value {
    // Nur statisch definierte Routen und Methoden werden an bestehende
    // Handler geschickt. Kein generischer HTTP-, SQL- oder Discord-Proxy.
    let router = Router::new()
        .merge(crate::public::router())
        .merge(crate::admin::router())
        .with_state(state);
    let mut builder = Request::builder()
        .method(plan.method)
        .uri(plan.path)
        .header(CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let body = plan
        .body
        .map_or_else(Vec::new, |v| serde_json::to_vec(&v).unwrap_or_default());
    let request = match builder.body(Body::from(body)) {
        Ok(req) => req,
        Err(_) => return tool_error("Anfrage konnte nicht erstellt werden"),
    };
    let response = match router.oneshot(request).await {
        Ok(response) => response,
        Err(_) => return tool_error("Turnier-API nicht verfügbar"),
    };
    let status = response.status();
    let bytes = match to_bytes(response.into_body(), MAX_RESULT_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return tool_error("Turnier-Antwort überschreitet das Größenlimit"),
    };
    let body: Value =
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"message":"Keine JSON-Antwort"}));
    json!({
        "content":[{"type":"text","text":json!({"status":status.as_u16(),"data":body}).to_string()}],
        "isError":!status.is_success()
    })
}

async fn dispatch(state: AppState, headers: HeaderMap, request: Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return rpc_error(id, -32600, "Ungültiger JSON-RPC-Request");
    }
    let method = match request.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => return rpc_error(id, -32600, "Methode fehlt"),
    };
    match method {
        "initialize" => rpc_ok(
            id,
            json!({
                "protocolVersion":"2025-06-18",
                "capabilities":{"tools":{"listChanged":false}},
                "serverInfo":{"name":"deadlock-turnier-mcp","version":env!("CARGO_PKG_VERSION")}
            }),
        ),
        "ping" => rpc_ok(id, json!({})),
        "tools/list" | "tools/call" => {
            // Ohne Bearer nur Public Data. Cookies oder Modellparameter können
            // keinen authentifizierten Nutzer identifizieren.
            let token = match bearer_token(&headers) {
                Ok(token) => token,
                Err(_) => return rpc_error(id, -32001, "Ungültige Authentifizierung"),
            };
            let level = match identity(&state, token).await {
                Ok(level) => level,
                Err(_) => {
                    return rpc_error(id, -32001, "Discord-Authentifizierung nicht bestätigt")
                }
            };
            if method == "tools/list" {
                return rpc_ok(id, json!({"tools":definitions(level)}));
            }
            let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
                return rpc_error(id, -32602, "Tool-Name fehlt");
            };
            let Some(tool) = TOOLS.iter().copied().find(|tool| tool.name == name) else {
                return rpc_error(id, -32602, "Unbekanntes Tool");
            };
            if tool.access > level {
                return rpc_ok(id, tool_error("Keine Berechtigung für dieses Tool"));
            }
            let arguments = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let plan = match plan(tool, &arguments) {
                Ok(plan) => plan,
                Err(message) => return rpc_ok(id, tool_error(message)),
            };
            rpc_ok(id, invoke(state, token, plan).await)
        }
        _ => rpc_error(id, -32601, "Methode nicht verfügbar"),
    }
}

async fn mcp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    // JSON-RPC-Notifications besitzen keine id und erwarten keine Antwort.
    if request.get("id").is_none()
        && request
            .get("method")
            .and_then(Value::as_str)
            .is_some_and(|method| method.starts_with("notifications/"))
    {
        return (StatusCode::ACCEPTED, [(CACHE_CONTROL, "no-store")]).into_response();
    }
    // Keine Session-Cookies für Tool-Rechte: Schutz vor Cross-Site-Requests.
    // Nur eine bereits authentifizierte Host-Anwendung darf den Bearer senden.
    let output = dispatch(state, headers, request).await;
    (StatusCode::OK, [(CACHE_CONTROL, "no-store")], Json(output)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> Tool {
        TOOLS
            .iter()
            .copied()
            .find(|tool| tool.name == name)
            .unwrap()
    }

    #[test]
    fn public_tools_never_expose_mutations() {
        let defined = definitions(Access::Public);
        let names: Vec<_> = defined
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v["name"].as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "turnier_list",
                "turnier_get",
                "turnier_bracket",
                "turnier_groups"
            ]
        );
        assert!(!names
            .iter()
            .any(|name| name.contains("admin") || name.contains("create")));
    }

    #[test]
    fn level_mapping_is_monotone_and_fail_closed() {
        assert_eq!(role_level(false, false), Access::Member);
        assert_eq!(role_level(true, false), Access::Mod);
        assert_eq!(role_level(false, true), Access::Admin);
        assert!(tool("turnier_create").access > Access::Member);
        assert!(tool("turnier_team_create").access <= Access::Member);
    }

    #[test]
    fn role_revocation_downgrades_existing_admin_session() {
        let roles = turnier_auth::RoleSets::new(
            ["admin".to_string()].into_iter().collect(),
            ["mod".to_string()].into_iter().collect(),
        );
        let admin = turnier_core::UserSession {
            discord_id: "123".into(),
            discord_name: Some("fixture".into()),
            discord_avatar: None,
            roles: vec!["admin".into()],
            is_admin: true,
            is_mod: true,
        };
        assert_eq!(
            effective_level(&admin, &["admin".into()], &roles),
            Access::Admin
        );
        assert_eq!(
            effective_level(&admin, &["mod".into()], &roles),
            Access::Mod
        );
        assert_eq!(effective_level(&admin, &[], &roles), Access::Member);
        let normal = turnier_core::UserSession {
            is_admin: false,
            is_mod: false,
            ..admin
        };
        assert_eq!(
            effective_level(&normal, &["admin".into()], &roles),
            Access::Member
        );
    }

    #[test]
    fn extra_parameters_cannot_spoof_permissions_or_paths() {
        for args in [
            json!({"tournament_id":1, "user_id":"999"}),
            json!({"tournament_id":1, "roles":["admin"]}),
            json!({"tournament_id":"../admin"}),
            json!({"tournament_id":-1}),
            json!({"tournament_id":1,"payload":{"name":"Test"}}),
        ] {
            assert!(plan(tool("turnier_get"), &args).is_err());
        }
        let valid = plan(
            tool("turnier_update"),
            &json!({"tournament_id":12,"payload":{"name":"Test"}}),
        )
        .unwrap();
        assert_eq!(valid.path, "/api/admin/tournaments/12");
        assert_eq!(valid.method, Method::PUT);
    }

    #[test]
    fn bearer_is_not_derived_from_cookies_or_model_arguments() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", "session_token=fake".parse().unwrap());
        assert_eq!(bearer_token(&headers), Ok(None));
        headers.insert(AUTHORIZATION, "Bearer fake".parse().unwrap());
        assert!(bearer_token(&headers).is_err());
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", "a".repeat(64)).parse().unwrap(),
        );
        assert_eq!(
            bearer_token(&headers).unwrap(),
            Some("a".repeat(64).as_str())
        );
    }

    #[test]
    fn all_tool_names_unique_and_schemas_closed() {
        let mut names = std::collections::BTreeSet::new();
        for tool in TOOLS {
            assert!(names.insert(tool.name));
            assert_eq!(schema(*tool)["additionalProperties"], false);
            assert!(tool.name.starts_with("turnier_"));
            if tool.access == Access::Public {
                assert!(!tool.has_payload);
            }
        }
    }
}
