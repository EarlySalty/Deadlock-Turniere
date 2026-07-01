//! Consent-Router — Datenschutz-Einwilligung und Nutzer-Profil inkl.
//! Avatar-Upload/-Auslieferung (portiert `tournament/consent_routes.py`).
//!
//! - `/api/consent` (GET/POST/DELETE): Einwilligungsstatus lesen, setzen
//!   (Versionierung), widerrufen (gesperrt bei aktiver Turnierteilnahme).
//! - `/api/profile` (GET/PUT): eigenes Profil lesen/aktualisieren.
//! - `/api/profile/avatar` (POST): Avatar als Multipart hochladen, Magic-Byte-
//!   Erkennung, alte Dateien aufräumen, in `AVATAR_DIR` ablegen.
//! - `/api/avatars/{discord_id}` und `/api/avatars/by-name/{discord_name}`:
//!   gespeicherte Datei ausliefern oder auf den Discord-Avatar weiterleiten.

use std::path::{Path as FsPath, PathBuf};

use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::header::{CONTENT_TYPE, LOCATION};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use sqlx::QueryBuilder;

use turnier_core::{ConsentCreate, ConsentStatus, UserProfile, UserProfileUpdate, UserSession};

use crate::db;
use crate::error::{WebError, WebResult};
use crate::extract::AuthUser;
use crate::state::AppState;

/// Aktuell geforderte Einwilligungsversion (wie `_CURRENT_CONSENT_VERSION`).
const CURRENT_CONSENT_VERSION: i64 = 2;
/// Turnierstatus, die eine aktive Teilnahme darstellen (wie `_ACTIVE_TOURNAMENT_STATUSES`).
const ACTIVE_TOURNAMENT_STATUSES: [&str; 4] = ["registration", "checkin", "group_phase", "bracket"];
/// Maximale Avatar-Größe in Bytes (2 MB, wie `_MAX_AVATAR_SIZE`).
const MAX_AVATAR_SIZE: usize = 2 * 1024 * 1024;

/// Router der Consent-/Profil-/Avatar-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/consent",
            get(get_consent).post(set_consent).delete(revoke_consent),
        )
        .route("/api/profile", get(get_my_profile).put(update_my_profile))
        .route("/api/profile/avatar", post(upload_profile_avatar))
        .route("/api/avatars/{discord_id}", get(get_avatar))
        .route(
            "/api/avatars/by-name/{discord_name}",
            get(get_avatar_by_name),
        )
}

// --- Hilfsfunktionen ---

/// Normalisiert einen Display-Namen (wie `_normalize_display_name`): trimmen,
/// nicht leer, höchstens 32 Zeichen, keine Steuerzeichen (< U+0020).
fn normalize_display_name(display_name: &str) -> WebResult<String> {
    let value = display_name.trim();
    if value.is_empty() {
        return Err(WebError::bad_request("Display Name darf nicht leer sein"));
    }
    // `len()` im Original zählt Unicode-Codepoints, daher `chars().count()`.
    if value.chars().count() > 32 {
        return Err(WebError::bad_request(
            "Display Name darf maximal 32 Zeichen lang sein",
        ));
    }
    if value.chars().any(|c| (c as u32) < 32) {
        return Err(WebError::bad_request(
            "Display Name enthält ungültige Steuerzeichen",
        ));
    }
    Ok(value.to_string())
}

/// Erkennt das Avatar-Format anhand der Magic Bytes (wie `_detect_avatar_format`).
/// Liefert `(Dateiendung, MIME-Typ)`.
fn detect_avatar_format(data: &[u8]) -> WebResult<(&'static str, &'static str)> {
    if data.starts_with(&[0xff, 0xd8, 0xff]) {
        return Ok((".jpg", "image/jpeg"));
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok((".png", "image/png"));
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Ok((".webp", "image/webp"));
    }
    Err(WebError::bad_request(
        "Erlaubt sind nur JPG, PNG oder WEBP Bilder",
    ))
}

/// MIME-Typ zu einer (kleingeschriebenen) Avatar-Dateiendung (wie `_AVATAR_MEDIA_TYPES`).
fn media_type_for_extension(ext_lower: &str) -> &'static str {
    match ext_lower {
        ".jpg" | ".jpeg" => "image/jpeg",
        ".png" => "image/png",
        ".webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// Sucht eine vorhandene Avatar-Datei für eine Discord-ID (wie `_avatar_file_path`).
fn avatar_file_path(avatar_dir: &str, discord_id: &str) -> Option<PathBuf> {
    for extension in [".jpg", ".jpeg", ".png", ".webp"] {
        let candidate = FsPath::new(avatar_dir).join(format!("{discord_id}{extension}"));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Eine vollständige `user_profiles`-Zeile.
#[derive(sqlx::FromRow)]
struct ProfileRow {
    discord_id: i64,
    bio: Option<String>,
    invite_auto_accept: bool,
    notify_discord_dm: bool,
    notify_browser: bool,
    display_name: Option<String>,
    avatar_filename: Option<String>,
    notify_match_start: bool,
    notify_checkin: bool,
    notify_team_invite: bool,
    notify_tournament_news: bool,
    notify_registration_reminder: bool,
    updated_at: DateTime<Utc>,
}

/// Baut ein [`UserProfile`] aus einer DB-Zeile (wie `_serialize_profile_row`):
/// ein leerer/fehlender `display_name` fällt auf den Discord-Namen zurück.
fn serialize_profile_row(row: ProfileRow, user: &UserSession) -> UserProfile {
    let display_name = match row.display_name {
        Some(name) if !name.is_empty() => Some(name),
        _ => user.discord_name.clone(),
    };
    UserProfile {
        discord_id: db::discord_id_to_string(row.discord_id),
        bio: row.bio,
        invite_auto_accept: row.invite_auto_accept,
        notify_discord_dm: row.notify_discord_dm,
        notify_browser: row.notify_browser,
        display_name,
        avatar_filename: row.avatar_filename,
        notify_match_start: row.notify_match_start,
        notify_checkin: row.notify_checkin,
        notify_team_invite: row.notify_team_invite,
        notify_tournament_news: row.notify_tournament_news,
        notify_registration_reminder: row.notify_registration_reminder,
        updated_at: Some(db::ts_to_string(row.updated_at)),
    }
}

/// Prüft, ob der Nutzer in einem aktiven Turnier angemeldet ist
/// (wie `_has_active_tournament_participation`).
async fn has_active_tournament_participation(
    pool: &turnier_db::Pool,
    discord_id: &str,
) -> WebResult<bool> {
    let discord_id = db::parse_discord_id(discord_id)?;
    let mut query = QueryBuilder::new(
        r#"SELECT 1::BIGINT FROM turnier."tournament_signups" ts
         JOIN turnier."tournaments" t ON t.id = ts.tournament_id
         WHERE ts.discord_id = "#,
    );
    query.push_bind(discord_id).push(" AND t.status IN (");
    {
        let mut separated = query.separated(", ");
        for status in ACTIVE_TOURNAMENT_STATUSES {
            separated.push_bind(status);
        }
    }
    query.push(") LIMIT 1");
    let row: Option<(i64,)> = query.build_query_as().fetch_optional(pool).await?;
    Ok(row.is_some())
}

/// Lädt das Profil des Nutzers neu und serialisiert es (für PUT/Upload-Antwort).
async fn reload_profile(pool: &turnier_db::Pool, user: &UserSession) -> WebResult<UserProfile> {
    let row: ProfileRow =
        sqlx::query_as(r#"SELECT * FROM turnier."user_profiles" WHERE discord_id = $1"#)
            .bind(db::parse_discord_id(&user.discord_id)?)
            .fetch_one(pool)
            .await?;
    Ok(serialize_profile_row(row, user))
}

// --- Consent-Endpunkte ---

/// `GET /api/consent` — Einwilligungsstatus des eingeloggten Nutzers.
async fn get_consent(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> WebResult<Json<ConsentStatus>> {
    let row: Option<(Option<DateTime<Utc>>, i64)> = sqlx::query_as(
        r#"SELECT consented_at, consent_version FROM turnier."user_consents" WHERE discord_id = $1"#,
    )
    .bind(db::parse_discord_id(&user.discord_id)?)
    .fetch_optional(&state.pool)
    .await?;

    let Some((consented_at, consent_version)) = row else {
        return Ok(Json(ConsentStatus {
            has_consent: false,
            consented_at: None,
            consent_version: None,
        }));
    };

    Ok(Json(ConsentStatus {
        has_consent: consent_version >= CURRENT_CONSENT_VERSION,
        consented_at: consented_at.map(db::ts_to_string),
        consent_version: Some(consent_version),
    }))
}

/// `POST /api/consent` (201) — Einwilligung setzen/aktualisieren.
async fn set_consent(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<ConsentCreate>,
) -> WebResult<(StatusCode, Json<ConsentStatus>)> {
    let now = Utc::now();
    sqlx::query(
        r#"INSERT INTO turnier."user_consents" (discord_id, consented_at, consent_version)
         VALUES ($1, $2, $3)
         ON CONFLICT (discord_id) DO UPDATE
         SET consented_at = EXCLUDED.consented_at,
             consent_version = EXCLUDED.consent_version"#,
    )
    .bind(db::parse_discord_id(&user.discord_id)?)
    .bind(now)
    .bind(body.consent_version)
    .execute(&state.pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ConsentStatus {
            has_consent: true,
            consented_at: Some(db::ts_to_string(now)),
            consent_version: Some(body.consent_version),
        }),
    ))
}

/// `DELETE /api/consent` (204) — Einwilligung widerrufen; 409 bei aktiver
/// Turnierteilnahme.
async fn revoke_consent(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> WebResult<StatusCode> {
    if has_active_tournament_participation(&state.pool, &user.discord_id).await? {
        return Err(WebError::conflict(
            "Ein Widerruf ist nicht möglich, solange du in einem aktiven Turnier angemeldet bist.",
        ));
    }
    sqlx::query(r#"DELETE FROM turnier."user_consents" WHERE discord_id = $1"#)
        .bind(db::parse_discord_id(&user.discord_id)?)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- Profil-Endpunkte ---

/// `GET /api/profile` — eigenes Profil (oder Default-Profil, wenn keine Zeile).
async fn get_my_profile(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> WebResult<Json<UserProfile>> {
    let row: Option<ProfileRow> =
        sqlx::query_as(r#"SELECT * FROM turnier."user_profiles" WHERE discord_id = $1"#)
            .bind(db::parse_discord_id(&user.discord_id)?)
            .fetch_optional(&state.pool)
            .await?;

    match row {
        // Default-Profil wie `UserProfile(discord_id=..., display_name=user.discord_name)`:
        // alle anderen Felder über die Pydantic/serde-Defaults der DTO-Definition.
        None => Ok(Json(UserProfile {
            discord_id: user.discord_id.clone(),
            bio: None,
            invite_auto_accept: false,
            notify_discord_dm: true,
            notify_browser: false,
            display_name: user.discord_name.clone(),
            avatar_filename: None,
            notify_match_start: true,
            notify_checkin: true,
            notify_team_invite: true,
            notify_tournament_news: false,
            notify_registration_reminder: true,
            updated_at: None,
        })),
        Some(row) => Ok(Json(serialize_profile_row(row, &user))),
    }
}

/// `PUT /api/profile` — eigenes Profil aktualisieren (partiell, UPSERT-Logik).
async fn update_my_profile(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<UserProfileUpdate>,
) -> WebResult<Json<UserProfile>> {
    let pool = &state.pool;
    let discord_id = db::parse_discord_id(&user.discord_id)?;

    let existing: Option<(i64,)> =
        sqlx::query_as(r#"SELECT discord_id FROM turnier."user_profiles" WHERE discord_id = $1"#)
            .bind(discord_id)
            .fetch_optional(pool)
            .await?;

    if existing.is_some() {
        // Dynamisches UPDATE mit statischer Spalten-Whitelist; Werte werden gebunden.
        let mut updates: Vec<(&'static str, ProfilePatch)> = Vec::new();

        if let Some(bio) = &body.bio {
            if bio.chars().count() > 1000 {
                return Err(WebError::bad_request(
                    "Bio darf maximal 1000 Zeichen lang sein",
                ));
            }
            updates.push(("bio", ProfilePatch::OptText(Some(bio.clone()))));
        }
        if let Some(v) = body.invite_auto_accept {
            updates.push(("invite_auto_accept", ProfilePatch::Bool(v)));
        }
        if let Some(v) = body.notify_discord_dm {
            updates.push(("notify_discord_dm", ProfilePatch::Bool(v)));
        }
        if let Some(v) = body.notify_browser {
            updates.push(("notify_browser", ProfilePatch::Bool(v)));
        }
        if let Some(display_name) = &body.display_name {
            updates.push((
                "display_name",
                ProfilePatch::OptText(Some(normalize_display_name(display_name)?)),
            ));
        }
        if let Some(avatar_filename) = &body.avatar_filename {
            updates.push((
                "avatar_filename",
                ProfilePatch::OptText(Some(avatar_filename.clone())),
            ));
        }
        if let Some(v) = body.notify_match_start {
            updates.push(("notify_match_start", ProfilePatch::Bool(v)));
        }
        if let Some(v) = body.notify_checkin {
            updates.push(("notify_checkin", ProfilePatch::Bool(v)));
        }
        if let Some(v) = body.notify_team_invite {
            updates.push(("notify_team_invite", ProfilePatch::Bool(v)));
        }
        if let Some(v) = body.notify_tournament_news {
            updates.push(("notify_tournament_news", ProfilePatch::Bool(v)));
        }
        if let Some(v) = body.notify_registration_reminder {
            updates.push(("notify_registration_reminder", ProfilePatch::Bool(v)));
        }

        let mut query = QueryBuilder::new(r#"UPDATE turnier."user_profiles" SET "#);
        {
            let mut separated = query.separated(", ");
            for (column, value) in updates {
                separated.push(format!("{column} = "));
                push_profile_patch(&mut separated, value);
            }
            separated.push("updated_at = now()");
        }
        query.push(" WHERE discord_id = ").push_bind(discord_id);
        query.build().execute(pool).await?;
    } else {
        // INSERT-Pfad: Bio-Limit prüfen, Display-Name normalisieren,
        // Benachrichtigungs-Defaults exakt wie im Original übernehmen.
        let bio = body.bio.clone().unwrap_or_default();
        if bio.chars().count() > 1000 {
            return Err(WebError::bad_request(
                "Bio darf maximal 1000 Zeichen lang sein",
            ));
        }
        let bio_value: Option<String> = if bio.is_empty() { None } else { Some(bio) };
        let display_name = match &body.display_name {
            Some(name) => Some(normalize_display_name(name)?),
            None => None,
        };

        sqlx::query(
            r#"INSERT INTO turnier."user_profiles"
             (discord_id, bio, invite_auto_accept, notify_discord_dm, notify_browser, display_name,
              avatar_filename, notify_match_start, notify_checkin, notify_team_invite,
              notify_tournament_news, notify_registration_reminder, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now())"#,
        )
        .bind(discord_id)
        .bind(bio_value)
        .bind(body.invite_auto_accept.unwrap_or(false))
        // notify_discord_dm: Default an (None oder true -> 1).
        .bind(body.notify_discord_dm.unwrap_or(true))
        .bind(body.notify_browser.unwrap_or(false))
        .bind(display_name)
        .bind(body.avatar_filename.clone())
        .bind(body.notify_match_start.unwrap_or(true))
        .bind(body.notify_checkin.unwrap_or(true))
        .bind(body.notify_team_invite.unwrap_or(true))
        .bind(body.notify_tournament_news.unwrap_or(false))
        .bind(body.notify_registration_reminder.unwrap_or(true))
        .execute(pool)
        .await?;
    }

    Ok(Json(reload_profile(pool, &user).await?))
}

/// `POST /api/profile/avatar` — Avatar als Multipart hochladen.
async fn upload_profile_avatar(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    mut multipart: Multipart,
) -> WebResult<Json<UserProfile>> {
    let pool = &state.pool;

    // Datei-Feld `avatar` aus dem Multipart-Body lesen.
    let mut data: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| WebError::unprocessable("Ungültiger Multipart-Upload"))?
    {
        if field.name() == Some("avatar") {
            let bytes = field
                .bytes()
                .await
                .map_err(|_| WebError::unprocessable("Avatar konnte nicht gelesen werden"))?;
            data = Some(bytes.to_vec());
            break;
        }
    }
    let data = data.ok_or_else(|| WebError::unprocessable("Feld 'avatar' fehlt"))?;

    if data.len() > MAX_AVATAR_SIZE {
        return Err(WebError::bad_request("Avatar darf maximal 2 MB groß sein"));
    }

    let (extension, _media_type) = detect_avatar_format(&data)?;
    let avatar_dir = &state.config.avatar_dir;
    tokio::fs::create_dir_all(avatar_dir)
        .await
        .map_err(|err| WebError::internal(format!("Avatar-Verzeichnis nicht anlegbar: {err}")))?;
    let avatar_path = FsPath::new(avatar_dir).join(format!("{}{extension}", user.discord_id));

    // Alte Avatare desselben Nutzers mit anderer Endung entfernen.
    for old_ext in [".jpg", ".jpeg", ".png", ".webp"] {
        let candidate = FsPath::new(avatar_dir).join(format!("{}{old_ext}", user.discord_id));
        if candidate != avatar_path && candidate.exists() {
            // `missing_ok=True`-Äquivalent: Fehler beim Löschen ignorieren.
            let _ = tokio::fs::remove_file(&candidate).await;
        }
    }

    tokio::fs::write(&avatar_path, &data).await.map_err(|err| {
        WebError::internal(format!("Avatar konnte nicht gespeichert werden: {err}"))
    })?;

    let file_name = avatar_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let discord_id = db::parse_discord_id(&user.discord_id)?;

    let existing: Option<(i64,)> =
        sqlx::query_as(r#"SELECT discord_id FROM turnier."user_profiles" WHERE discord_id = $1"#)
            .bind(discord_id)
            .fetch_optional(pool)
            .await?;

    if existing.is_some() {
        sqlx::query(
            r#"UPDATE turnier."user_profiles" SET avatar_filename = $1, updated_at = now()
             WHERE discord_id = $2"#,
        )
        .bind(&file_name)
        .bind(discord_id)
        .execute(pool)
        .await?;
    } else {
        // INSERT-Pfad mit den abweichenden Defaults aus dem Original
        // (invite_auto_accept=0, notify_discord_dm=1, notify_browser=0,
        //  notify_match_start=1, notify_checkin=1, notify_team_invite=1,
        //  notify_tournament_news=0, notify_registration_reminder=1).
        sqlx::query(
            r#"INSERT INTO turnier."user_profiles"
             (discord_id, avatar_filename, updated_at, invite_auto_accept, notify_discord_dm,
              notify_browser, notify_match_start, notify_checkin, notify_team_invite,
              notify_tournament_news, notify_registration_reminder)
             VALUES ($1, $2, now(), false, true, false, true, true, true, false, true)"#,
        )
        .bind(discord_id)
        .bind(&file_name)
        .execute(pool)
        .await?;
    }

    Ok(Json(reload_profile(pool, &user).await?))
}

// --- Avatar-Auslieferung ---

/// Baut eine 302-Redirect-Response auf den Discord-Avatar-URL.
fn redirect_found(url: &str) -> Response {
    Response::builder()
        .status(StatusCode::FOUND)
        .header(LOCATION, url)
        .body(Body::empty())
        .expect("statische Redirect-Response ist immer gültig")
}

/// Liest eine Avatar-Datei und baut eine 200-Response mit passendem Content-Type.
async fn serve_avatar_file(path: &FsPath) -> WebResult<Response> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|err| WebError::internal(format!("Avatar nicht lesbar: {err}")))?;
    let ext_lower = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();
    let media_type = media_type_for_extension(&ext_lower);
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, media_type)
        .body(Body::from(bytes))
        .expect("gültige Datei-Response"))
}

/// Liefert den gespeicherten Discord-Avatar-URL eines Nutzers (jüngste Session
/// mit nicht-leerem `discord_avatar`).
async fn latest_discord_avatar(
    pool: &turnier_db::Pool,
    discord_id: &str,
) -> WebResult<Option<String>> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        r#"SELECT discord_avatar FROM turnier."sessions" WHERE discord_id = $1
         AND discord_avatar IS NOT NULL AND discord_avatar != ''
         ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(db::parse_discord_id(discord_id)?)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(avatar,)| avatar).filter(|a| !a.is_empty()))
}

/// `GET /api/avatars/{discord_id}` — gespeicherte Datei oder Redirect oder 404.
async fn get_avatar(
    State(state): State<AppState>,
    Path(discord_id): Path<String>,
) -> WebResult<Response> {
    if let Some(path) = avatar_file_path(&state.config.avatar_dir, &discord_id) {
        return serve_avatar_file(&path).await;
    }
    if let Some(url) = latest_discord_avatar(&state.pool, &discord_id).await? {
        return Ok(redirect_found(&url));
    }
    Err(WebError::not_found("Avatar nicht gefunden"))
}

/// `GET /api/avatars/by-name/{discord_name}` — Avatar über den Discord-Namen.
async fn get_avatar_by_name(
    State(state): State<AppState>,
    Path(discord_name): Path<String>,
) -> WebResult<Response> {
    let session: Option<(i64,)> = sqlx::query_as(
        r#"SELECT discord_id FROM turnier."sessions" WHERE discord_name = $1 LIMIT 1"#,
    )
    .bind(&discord_name)
    .fetch_optional(&state.pool)
    .await?;
    let Some((discord_id,)) = session else {
        return Err(WebError::not_found("Spieler nicht gefunden"));
    };
    let discord_id = db::discord_id_to_string(discord_id);

    if let Some(path) = avatar_file_path(&state.config.avatar_dir, &discord_id) {
        return serve_avatar_file(&path).await;
    }
    if let Some(url) = latest_discord_avatar(&state.pool, &discord_id).await? {
        return Ok(redirect_found(&url));
    }
    Err(WebError::not_found("Avatar nicht gefunden"))
}

/// Heterogene Bind-Werte für das dynamische Profil-UPDATE.
enum ProfilePatch {
    Bool(bool),
    OptText(Option<String>),
}

fn push_profile_patch<'a>(
    separated: &mut sqlx::query_builder::Separated<'_, 'a, sqlx::Postgres, &'static str>,
    value: ProfilePatch,
) {
    match value {
        ProfilePatch::Bool(value) => {
            separated.push_bind(value);
        }
        ProfilePatch::OptText(value) => {
            separated.push_bind(value);
        }
    }
}
