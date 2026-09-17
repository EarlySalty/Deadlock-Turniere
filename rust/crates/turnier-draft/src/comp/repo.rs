use super::{
    normalize_code, token_hash, validate_name, validate_preferences, CompError, CompResult, Member,
    Preference, MAX_PLAYERS,
};
use chrono::{DateTime, Utc};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};
use std::collections::HashSet;
use turnier_db::Pool;

#[derive(Clone, Debug, Serialize)]
pub struct Room {
    pub code: String,
    pub host_member_id: String,
    pub revision: i64,
    pub expires_at: DateTime<Utc>,
    pub members: Vec<Member>,
    pub you: Option<String>,
}

#[derive(Deserialize)]
struct StoredMember {
    #[serde(flatten)]
    member: Member,
    token_hash: String,
}

fn member_id() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

fn room_code() -> String {
    let alphabet = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| alphabet[rng.gen_range(0..alphabet.len())] as char)
        .collect()
}

pub async fn create(pool: &Pool, name: &str, token: &str) -> CompResult<Room> {
    let name = validate_name(name)?;
    let hash = token_hash(token)?;
    // Central migrations are deployment-owned; never create tables at runtime.
    sqlx::query("DELETE FROM turnier.comp_lobbies WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    let id = member_id();
    let mut tx = pool.begin().await?;
    let mut code = room_code();
    let mut inserted = false;
    for _ in 0..8 {
        if sqlx::query("INSERT INTO turnier.comp_lobbies(code, host_member_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(&code).bind(&id).execute(&mut *tx).await?.rows_affected() == 1 {
            inserted = true;
            break;
        }
        code = room_code();
    }
    if !inserted {
        return Err(CompError::Invalid(
            "Es konnte kein freier Lobby-Code erzeugt werden. Bitte erneut versuchen.".into(),
        ));
    }
    insert_member(&mut tx, &code, &id, &name, &hash).await?;
    tx.commit().await?;
    get(pool, &code, Some(token)).await
}

async fn insert_member(
    tx: &mut Transaction<'_, Postgres>,
    code: &str,
    id: &str,
    name: &str,
    hash: &str,
) -> CompResult<()> {
    sqlx::query("INSERT INTO turnier.comp_members(id, lobby_code, name, token_hash) VALUES ($1, $2, $3, $4)")
        .bind(id).bind(code).bind(name).bind(hash).execute(&mut **tx).await?;
    bump(tx, code).await
}

/// A single SQL statement gives a consistent lobby+members snapshot. No hash or
/// credential is serialized into the response, including for the room host.
pub async fn get(pool: &Pool, code: &str, token: Option<&str>) -> CompResult<Room> {
    let code = normalize_code(code)?;
    let hash = token.map(token_hash).transpose()?;
    let row = sqlx::query(
        "SELECT l.code, l.host_member_id, l.revision, l.expires_at,
            COALESCE((SELECT jsonb_agg(to_jsonb(m) ORDER BY m.joined_at, m.id)
                      FROM turnier.comp_members m WHERE m.lobby_code=l.code), '[]'::jsonb) AS members
         FROM turnier.comp_lobbies l WHERE l.code=$1 AND l.expires_at > now()"
    ).bind(&code).fetch_optional(pool).await?.ok_or(CompError::NotFound)?;
    let stored: Vec<StoredMember> = serde_json::from_value(row.try_get("members")?)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let you = stored
        .iter()
        .find(|m| hash.as_deref() == Some(m.token_hash.as_str()))
        .map(|m| m.member.id.clone());
    Ok(Room {
        code: row.try_get("code")?,
        host_member_id: row.try_get("host_member_id")?,
        revision: row.try_get("revision")?,
        expires_at: row.try_get("expires_at")?,
        members: stored.into_iter().map(|m| m.member).collect(),
        you,
    })
}

/// Every mutation takes the SAME room lock: joins cannot overfill a lobby,
/// concurrent edits cannot undo a leave, and host transfer is atomic.
async fn lock_room(tx: &mut Transaction<'_, Postgres>, code: &str) -> CompResult<String> {
    sqlx::query_scalar("SELECT host_member_id FROM turnier.comp_lobbies WHERE code=$1 AND expires_at > now() FOR UPDATE")
        .bind(code).fetch_optional(&mut **tx).await?.ok_or(CompError::NotFound)
}

async fn actor(tx: &mut Transaction<'_, Postgres>, code: &str, hash: &str) -> CompResult<String> {
    sqlx::query_scalar("SELECT id FROM turnier.comp_members WHERE lobby_code=$1 AND token_hash=$2")
        .bind(code)
        .bind(hash)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(CompError::Unauthorized)
}

async fn bump(tx: &mut Transaction<'_, Postgres>, code: &str) -> CompResult<()> {
    sqlx::query("UPDATE turnier.comp_lobbies SET revision=revision+1 WHERE code=$1")
        .bind(code)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn join(pool: &Pool, code: &str, name: &str, token: &str) -> CompResult<Room> {
    let code = normalize_code(code)?;
    let name = validate_name(name)?;
    let hash = token_hash(token)?;
    let mut tx = pool.begin().await?;
    lock_room(&mut tx, &code).await?;
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM turnier.comp_members WHERE lobby_code=$1 AND token_hash=$2",
    )
    .bind(&code)
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;
    if existing.is_none() {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM turnier.comp_members WHERE lobby_code=$1")
                .bind(&code)
                .fetch_one(&mut *tx)
                .await?;
        if count >= MAX_PLAYERS as i64 {
            return Err(CompError::Full);
        }
        insert_member(&mut tx, &code, &member_id(), &name, &hash).await?;
    }
    tx.commit().await?;
    get(pool, &code, Some(token)).await
}

pub async fn save_preferences(
    pool: &Pool,
    code: &str,
    token: &str,
    revision: i64,
    preferences: &[Preference],
    allowed: &HashSet<String>,
) -> CompResult<Room> {
    let code = normalize_code(code)?;
    let hash = token_hash(token)?;
    validate_preferences(preferences, allowed)?;
    let mut tx = pool.begin().await?;
    lock_room(&mut tx, &code).await?;
    let id = actor(&mut tx, &code, &hash).await?;
    let changed = sqlx::query("UPDATE turnier.comp_members SET preferences=$1, revision=revision+1 WHERE id=$2 AND revision=$3")
        .bind(serde_json::json!(preferences)).bind(&id).bind(revision).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(CompError::Stale);
    }
    bump(&mut tx, &code).await?;
    tx.commit().await?;
    get(pool, &code, Some(token)).await
}

pub async fn leave(pool: &Pool, code: &str, token: &str) -> CompResult<()> {
    let code = normalize_code(code)?;
    let hash = token_hash(token)?;
    let mut tx = pool.begin().await?;
    let host = lock_room(&mut tx, &code).await?;
    let id = actor(&mut tx, &code, &hash).await?;
    sqlx::query("DELETE FROM turnier.comp_members WHERE id=$1")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if id == host {
        let next: Option<String> = sqlx::query_scalar("SELECT id FROM turnier.comp_members WHERE lobby_code=$1 ORDER BY joined_at, id LIMIT 1")
            .bind(&code).fetch_optional(&mut *tx).await?;
        match next {
            Some(next) => {
                sqlx::query("UPDATE turnier.comp_lobbies SET host_member_id=$1 WHERE code=$2")
                    .bind(next)
                    .bind(&code)
                    .execute(&mut *tx)
                    .await?;
            }
            None => {
                sqlx::query("DELETE FROM turnier.comp_lobbies WHERE code=$1")
                    .bind(&code)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    bump(&mut tx, &code).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn remove_member(pool: &Pool, code: &str, token: &str, target: &str) -> CompResult<Room> {
    let code = normalize_code(code)?;
    let hash = token_hash(token)?;
    let mut tx = pool.begin().await?;
    let host = lock_room(&mut tx, &code).await?;
    let id = actor(&mut tx, &code, &hash).await?;
    if id != host || target == host {
        return Err(CompError::Forbidden);
    }
    let deleted = sqlx::query("DELETE FROM turnier.comp_members WHERE lobby_code=$1 AND id=$2")
        .bind(&code)
        .bind(target)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(CompError::NotFound);
    }
    bump(&mut tx, &code).await?;
    tx.commit().await?;
    get(pool, &code, Some(token)).await
}
