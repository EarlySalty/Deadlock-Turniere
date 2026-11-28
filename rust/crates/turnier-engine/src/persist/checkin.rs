//! Solo-Team-Zuweisung + Check-in-Finalisierung (Persistenz).
//! Portiert `assign_random_teams`, `finalize_checkin`,
//! `_build_checkin_snapshot_token`, `_sync_team_captain`, `_team_name_from_captain`.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sha2_min::Sha256;
use sqlx::{FromRow, Postgres, Row, Transaction};
use turnier_core::{
    discord_id_to_string, now_utc, BracketFormat, InviteMode, RecruitmentStatus, TeamRole,
    TournamentMode, TournamentStatus,
};
use turnier_steam::RankResolver;

use crate::engine::naming::TeamNamePool;
use crate::error::{TournamentError, TournamentResult};

use super::bracket::build_seeded_bracket;
use super::double_elim::build_double_elimination_bracket;
use super::groups::{generate_group_matches_in_tx, generate_groups_in_tx};
use super::{audit, parse_db_discord_id, NewTeamMember};

mod sha2_min {
    //! Minimaler, abhängigkeitsfreier SHA-256, nur für den Snapshot-Token.
    //! Reicht aus, weil der Token rein intern (Dry-Run ↔ Confirm) verglichen wird.
    pub struct Sha256 {
        state: [u32; 8],
        buffer: Vec<u8>,
        length: u64,
    }

    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    impl Sha256 {
        pub fn new() -> Self {
            Self {
                state: [
                    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                    0x1f83d9ab, 0x5be0cd19,
                ],
                buffer: Vec::new(),
                length: 0,
            }
        }

        pub fn update(&mut self, data: &[u8]) {
            self.length += data.len() as u64;
            self.buffer.extend_from_slice(data);
            while self.buffer.len() >= 64 {
                let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
                self.process(&block);
                self.buffer.drain(..64);
            }
        }

        pub fn finalize_hex(mut self) -> String {
            let bit_len = self.length * 8;
            self.buffer.push(0x80);
            while self.buffer.len() % 64 != 56 {
                self.buffer.push(0);
            }
            self.buffer.extend_from_slice(&bit_len.to_be_bytes());
            let buffer = std::mem::take(&mut self.buffer);
            for chunk in buffer.chunks_exact(64) {
                let block: [u8; 64] = chunk.try_into().unwrap();
                self.process(&block);
            }
            let mut out = String::with_capacity(64);
            for word in self.state {
                out.push_str(&format!("{word:08x}"));
            }
            out
        }

        fn process(&mut self, block: &[u8; 64]) {
            let mut w = [0u32; 64];
            for (i, word) in w.iter_mut().enumerate().take(16) {
                *word = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }
            let mut h = self.state;
            for i in 0..64 {
                let s1 = h[4].rotate_right(6) ^ h[4].rotate_right(11) ^ h[4].rotate_right(25);
                let ch = (h[4] & h[5]) ^ ((!h[4]) & h[6]);
                let t1 = h[7]
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = h[0].rotate_right(2) ^ h[0].rotate_right(13) ^ h[0].rotate_right(22);
                let maj = (h[0] & h[1]) ^ (h[0] & h[2]) ^ (h[1] & h[2]);
                let t2 = s0.wrapping_add(maj);
                h[7] = h[6];
                h[6] = h[5];
                h[5] = h[4];
                h[4] = h[3].wrapping_add(t1);
                h[3] = h[2];
                h[2] = h[1];
                h[1] = h[0];
                h[0] = t1.wrapping_add(t2);
            }
            for (state, value) in self.state.iter_mut().zip(h) {
                *state = state.wrapping_add(value);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::Sha256;

        fn hash(input: &str) -> String {
            let mut h = Sha256::new();
            h.update(input.as_bytes());
            h.finalize_hex()
        }

        #[test]
        fn known_vectors() {
            // Bekannte SHA-256-Testvektoren.
            assert_eq!(
                hash(""),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            );
            assert_eq!(
                hash("abc"),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            );
            // > 64 Bytes (mehrere Blöcke).
            assert_eq!(
                hash("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Solo-Team-Zuweisung
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow, Clone)]
struct SoloSignup {
    id: i64,
    discord_id: i64,
    discord_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: Option<i64>,
}

/// Liefert eine Permutation der Indizes `0..len`, nach der die geladenen
/// Solo-Anmeldungen vor dem Chunking umgeordnet werden.
///
/// Injizierbar statt eines globalen RNG (Determinismus). Über Indizes statt des
/// (crate-privaten) Zeilen-Typs; das hält die Trait objektsicher (`dyn`) und
/// exponiert den internen Typ nicht. Spiegelt den Monkeypatch-Punkt des
/// Python-Originals (`random.shuffle`), den auch der Paritätstest no-op
/// überschreibt.
pub trait SoloShuffler: Send {
    /// Permutation der Indizes `0..len` (muss genau `len` Elemente enthalten).
    fn permutation(&mut self, len: usize) -> Vec<usize>;
}

/// Produktiver Shuffler: seedbarer Fisher-Yates über `StdRng` o. Ä.
pub struct RngShuffler<R: rand::RngCore>(pub R);

impl<R: rand::RngCore + Send> SoloShuffler for RngShuffler<R> {
    fn permutation(&mut self, len: usize) -> Vec<usize> {
        use rand::seq::SliceRandom;
        let mut indices: Vec<usize> = (0..len).collect();
        indices.shuffle(&mut self.0);
        indices
    }
}

/// No-op-Shuffler: Identitätspermutation (erhält die Eingabereihenfolge, für
/// deterministische Tests, entspricht dem `lambda values: None`-Monkeypatch).
pub struct NoShuffle;
impl SoloShuffler for NoShuffle {
    fn permutation(&mut self, len: usize) -> Vec<usize> {
        (0..len).collect()
    }
}

pub async fn assign_random_teams(
    pool: &sqlx::Pool<Postgres>,
    resolver: &dyn RankResolver,
    tournament_id: i64,
    shuffler: &mut dyn SoloShuffler,
) -> TournamentResult<i64> {
    let profiles = solo_rank_profiles(pool, resolver, tournament_id).await?;
    let mut tx = pool.begin().await?;
    let Some(locked) = super::lock_tournament_row(&mut tx, tournament_id).await? else {
        return Err(TournamentError::Validation("Turnier nicht gefunden".into()));
    };
    let status: TournamentStatus = locked.try_get("status")?;
    let team_size: i64 = locked.try_get("team_size")?;
    if status != TournamentStatus::Registration {
        return Err(TournamentError::Validation(
            "Team-Zuweisung nur während der Anmeldung möglich".into(),
        ));
    }
    let teams_created =
        assign_solo_signups_in_tx(&mut tx, tournament_id, team_size, &profiles, shuffler).await?;
    tx.commit().await?;
    Ok(teams_created)
}

pub async fn prepare_checkin_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    invite_mode: Option<InviteMode>,
    team_size: i64,
) -> TournamentResult<()> {
    let solo_only = invite_mode == Some(InviteMode::SoloOnly);
    if solo_only && team_size == 1 {
        assign_solo_signups_in_tx(
            tx,
            tournament_id,
            team_size,
            &HashMap::new(),
            &mut NoShuffle,
        )
        .await?;
    }
    let (sql, message) = if solo_only && team_size > 1 {
        (
            r#"SELECT COUNT(*) FROM turnier."tournament_signups" WHERE tournament_id = $1"#,
            "Check-in kann erst geöffnet werden, wenn mindestens eine Anmeldung existiert",
        )
    } else if solo_only {
        (
            r#"SELECT COUNT(*) FROM turnier."teams" WHERE tournament_id = $1"#,
            "Check-in kann erst geöffnet werden, wenn mindestens eine Anmeldung existiert",
        )
    } else {
        (
            r#"SELECT COUNT(*) FROM turnier."teams" WHERE tournament_id = $1"#,
            "Check-in kann erst geöffnet werden, wenn mindestens ein Team existiert",
        )
    };
    let count: i64 = sqlx::query_scalar(sql)
        .bind(tournament_id)
        .fetch_one(&mut **tx)
        .await?;
    if count == 0 {
        return Err(TournamentError::Validation(message.into()));
    }
    if solo_only && team_size == 1 {
        let unassigned: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM turnier.tournament_signups \
             WHERE tournament_id = $1 AND team_id IS NULL",
        )
        .bind(tournament_id)
        .fetch_one(&mut **tx)
        .await?;
        if unassigned > 0 {
            return Err(TournamentError::Validation(
                "Bei 1v1 braucht jeder Spieler vor dem Check-in ein eigenes Team".into(),
            ));
        }
    }
    Ok(())
}

async fn assign_solo_signups_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    team_size: i64,
    profiles: &HashMap<String, turnier_core::RankProfile>,
    shuffler: &mut dyn SoloShuffler,
) -> TournamentResult<i64> {
    let signups: Vec<SoloSignup> = sqlx::query_as::<_, SoloSignup>(
        "SELECT id, discord_id, discord_name, steam_id, rank, rank_score \
         FROM turnier.tournament_signups \
         WHERE tournament_id = $1 AND team_id IS NULL \
         AND NOT EXISTS (SELECT 1 FROM turnier.solo_balance_waitlist w \
                         WHERE w.signup_id = turnier.tournament_signups.id) \
         ORDER BY signed_up_at, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;

    if signups.is_empty() {
        return Ok(0);
    }

    let signups_assigned = signups.len();
    let permutation = shuffler.permutation(signups.len());
    debug_assert_eq!(permutation.len(), signups.len());
    let signups: Vec<SoloSignup> = permutation
        .into_iter()
        .map(|i| signups[i].clone())
        .collect();

    let existing_keys: Vec<(String,)> =
        sqlx::query_as("SELECT name_key FROM turnier.teams WHERE tournament_id = $1")
            .bind(tournament_id)
            .fetch_all(&mut **tx)
            .await?;
    let mut pool_names = TeamNamePool::new(existing_keys.into_iter().map(|(k,)| k));

    let mut teams_created = 0i64;
    let size = formation_team_size(team_size)?;
    for chunk in signups.chunks(size) {
        let captain_discord_id = chunk[0].discord_id;
        let captain_name = captain_team_name(tx, tournament_id, captain_discord_id).await?;
        let team_name = pool_names.from_captain(captain_name.as_deref());
        let team_id = super::insert_team(
            tx,
            tournament_id,
            &team_name,
            captain_discord_id,
            RecruitmentStatus::Open,
        )
        .await?;

        for (i, signup) in chunk.iter().enumerate() {
            let role = if i == 0 {
                TeamRole::Captain
            } else {
                TeamRole::Member
            };
            let rank_data = profiles.get(&discord_id_to_string(signup.discord_id));
            let (steam_id, rank, score) = match rank_data.cloned() {
                Some(profile) => (
                    profile.steam_id.or_else(|| signup.steam_id.clone()),
                    profile.rank.or_else(|| signup.rank.clone()),
                    // bug-preserved: rank_data.rank_score gewinnt immer (auch 0).
                    profile.rank_score,
                ),
                None => (
                    signup.steam_id.clone(),
                    signup.rank.clone(),
                    signup.rank_score.unwrap_or(0),
                ),
            };

            super::insert_team_member(
                tx,
                &NewTeamMember {
                    team_id,
                    discord_id: signup.discord_id,
                    discord_name: signup.discord_name.as_deref(),
                    steam_id: steam_id.as_deref(),
                    rank: rank.as_deref(),
                    rank_score: score,
                    role,
                },
            )
            .await?;

            sqlx::query("UPDATE turnier.tournament_signups SET team_id = $1 WHERE id = $2")
                .bind(team_id)
                .bind(signup.id)
                .execute(&mut **tx)
                .await?;
        }

        teams_created += 1;
    }

    let details = serde_json::json!({
        "tournament_id": tournament_id,
        "teams_created": teams_created,
        "signups_assigned": signups_assigned,
    })
    .to_string();
    audit(tx, "assign_random_teams", None, &details).await?;
    Ok(teams_created)
}

async fn solo_rank_profiles(
    pool: &sqlx::Pool<Postgres>,
    resolver: &dyn RankResolver,
    tournament_id: i64,
) -> TournamentResult<HashMap<String, turnier_core::RankProfile>> {
    let ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT discord_id FROM turnier.tournament_signups \
         WHERE tournament_id = $1 AND team_id IS NULL",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;
    let ids: Vec<String> = ids
        .into_iter()
        .map(|(id,)| discord_id_to_string(id))
        .collect();
    Ok(resolver.rank_profiles(&ids).await?)
}

/// Liefert den (jüngsten, nicht-leeren) Captain-Discord-Namen für die
/// Teamnamens-Ableitung. Portiert den DB-Teil von `_team_name_from_captain`.
async fn captain_team_name(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    captain_discord_id: i64,
) -> TournamentResult<Option<String>> {
    let from_members: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT discord_name FROM turnier.team_members \
         WHERE discord_id = $1 AND discord_name IS NOT NULL AND TRIM(discord_name) != '' \
         ORDER BY joined_at DESC, id DESC LIMIT 1",
    )
    .bind(captain_discord_id)
    .fetch_optional(&mut **tx)
    .await?;

    let name = match from_members {
        Some((Some(n),)) => Some(n),
        _ => {
            let from_signups: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT discord_name FROM turnier.tournament_signups \
                 WHERE tournament_id = $1 AND discord_id = $2 \
                 AND discord_name IS NOT NULL AND TRIM(discord_name) != '' \
                 ORDER BY signed_up_at DESC, id DESC LIMIT 1",
            )
            .bind(tournament_id)
            .bind(captain_discord_id)
            .fetch_optional(&mut **tx)
            .await?;
            match from_signups {
                Some((Some(n),)) => Some(n),
                _ => None,
            }
        }
    };
    Ok(name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()))
}

// ---------------------------------------------------------------------------
// Snapshot-Token
// ---------------------------------------------------------------------------

/// Baut den deterministischen Snapshot-Token (sha256 über sortiertes, kompaktes
/// JSON). Spiegelt `_build_checkin_snapshot_token`. Öffentlich, damit Aufrufer
/// (turnier-api) ihn vorab berechnen können.
pub async fn build_checkin_snapshot_token(
    pool: &sqlx::Pool<Postgres>,
    tournament_id: i64,
    tournament_status: TournamentStatus,
) -> TournamentResult<String> {
    let mut tx = pool.begin().await?;
    let token = snapshot_token_in_tx(&mut tx, tournament_id, tournament_status).await?;
    tx.commit().await?;
    Ok(token)
}

#[derive(sqlx::FromRow)]
struct SnapshotCheckin {
    discord_id: i64,
    checked_in_at: Option<DateTime<Utc>>,
}
#[derive(sqlx::FromRow)]
struct SnapshotMember {
    team_id: i64,
    discord_id: i64,
    joined_at: Option<DateTime<Utc>>,
    role: String,
    name: String,
    created_at: Option<DateTime<Utc>>,
}
#[derive(sqlx::FromRow)]
struct SnapshotSignup {
    id: i64,
    discord_id: i64,
    team_id: Option<i64>,
    signed_up_at: Option<DateTime<Utc>>,
}

async fn snapshot_token_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    tournament_status: TournamentStatus,
) -> TournamentResult<String> {
    let checkins: Vec<SnapshotCheckin> = sqlx::query_as(
        "SELECT discord_id, checked_in_at FROM turnier.tournament_checkins \
         WHERE tournament_id = $1 ORDER BY checked_in_at, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;

    let members: Vec<SnapshotMember> = sqlx::query_as(
        "SELECT tm.team_id, tm.discord_id, tm.joined_at, tm.role, t.name, t.created_at \
         FROM turnier.team_members tm JOIN turnier.teams t ON t.id = tm.team_id \
         WHERE t.tournament_id = $1 ORDER BY t.created_at, t.id, tm.joined_at, tm.id",
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;

    let signups: Vec<SnapshotSignup> = sqlx::query_as(
        "SELECT id, discord_id, team_id, signed_up_at FROM turnier.tournament_signups \
         WHERE tournament_id = $1 ORDER BY signed_up_at, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;

    let settings: Option<(InviteMode, BracketFormat, TournamentMode, i64, bool)> = sqlx::query_as(
        "SELECT invite_mode, bracket_format, tournament_mode, team_size, tournament_mode_auto \
         FROM turnier.tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_optional(&mut **tx)
    .await?;

    // serde_json::Map ist (ohne preserve_order) ein BTreeMap -> Schlüssel sortiert,
    // exakt wie json.dumps(sort_keys=True). Kompakte Trennzeichen über to_string.
    let payload = serde_json::json!({
        "status": tournament_status,
        "settings": settings,
        "checkins": checkins.iter().map(|c| serde_json::json!({
            "discord_id": c.discord_id, "checked_in_at": c.checked_in_at,
        })).collect::<Vec<_>>(),
        "members": members.iter().map(|m| serde_json::json!({
            "team_id": m.team_id, "discord_id": m.discord_id, "joined_at": m.joined_at,
            "role": m.role, "name": m.name, "created_at": m.created_at,
        })).collect::<Vec<_>>(),
        "signups": signups.iter().map(|s| serde_json::json!({
            "id": s.id, "discord_id": s.discord_id, "team_id": s.team_id, "signed_up_at": s.signed_up_at,
        })).collect::<Vec<_>>(),
    });
    let serialized = serde_json::to_string(&payload).expect("payload serialisierbar");
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    Ok(hasher.finalize_hex())
}

// ---------------------------------------------------------------------------
// Check-in-Finalisierung
// ---------------------------------------------------------------------------

/// Ergebnis von [`finalize_checkin`]: die für den Aufrufer relevanten Felder.
#[derive(Debug, Clone)]
pub struct FinalizeCheckinResult {
    pub warnings: Vec<TeamWarning>,
    pub removed_players: Vec<RemovedPlayer>,
    pub added_players: Vec<AddedPlayer>,
    pub created_teams: Vec<CreatedTeam>,
    pub deleted_team_ids: Vec<i64>,
    pub remaining_solo_players: Vec<SoloPlayer>,
    pub team_plans: Vec<TeamPlan>,
    pub solo_plans: Vec<SoloPlan>,
    pub auto_team_formation: bool,
    pub can_fill_from_solos: bool,
    pub bracket_format: BracketFormat,
    pub effective_bracket_format: BracketFormat,
    pub bracket_entrants: Option<usize>,
    pub final_team_count: usize,
    pub tournament_mode: TournamentMode,
    pub tournament_mode_auto: bool,
    pub dry_run: bool,
    pub snapshot_token: String,
    pub groups_created: Option<i64>,
    pub matches_created: Option<i64>,
    pub advanced_to_group_phase: Option<bool>,
    pub advanced_to_bracket: Option<bool>,
    pub entered_status: Option<TournamentStatus>,
}

#[derive(Debug, Clone)]
pub struct TeamWarning {
    pub team_id: i64,
    pub team_name: String,
    pub current: i64,
    pub required: i64,
}
#[derive(Debug, Clone)]
pub struct RemovedPlayer {
    pub team_id: i64,
    pub team_name: String,
    pub discord_id: String,
    pub discord_name: Option<String>,
}
#[derive(Debug, Clone)]
pub struct AddedPlayer {
    pub team_id: Option<i64>,
    pub team_name: String,
    pub discord_id: String,
    pub discord_name: Option<String>,
    pub source: &'static str,
}
#[derive(Debug, Clone)]
pub struct CreatedTeam {
    pub team_id: Option<i64>,
    pub team_name: String,
    pub members: Vec<SoloPlayer>,
}
#[derive(Debug, Clone)]
pub struct SoloPlayer {
    pub discord_id: String,
    pub discord_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosterChange {
    Stays,
    Removed,
    Added,
}

impl RosterChange {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stays => "stays",
            Self::Removed => "removed",
            Self::Added => "added",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RosterEntry {
    pub discord_id: String,
    pub discord_name: Option<String>,
    pub checked_in: bool,
    pub change: RosterChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamOutcome {
    Complete,
    Incomplete,
    Deleted,
}

impl TeamOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incomplete => "incomplete",
            Self::Deleted => "deleted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TeamPlan {
    pub team_id: i64,
    pub team_name: String,
    pub roster: Vec<RosterEntry>,
    pub checked_in: i64,
    pub required: i64,
    pub outcome: TeamOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoloOutcome {
    JoinsTeam,
    NewTeam,
    StaysSolo,
}

impl SoloOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JoinsTeam => "joins_team",
            Self::NewTeam => "new_team",
            Self::StaysSolo => "stays_solo",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SoloPlan {
    pub discord_id: String,
    pub discord_name: Option<String>,
    pub checked_in: bool,
    pub outcome: SoloOutcome,
    pub team_name: Option<String>,
}

fn solo_plans(
    signup_iter_order: &[i64],
    signups_by_discord: &HashMap<i64, SignupData>,
    checked_in_ids: &HashSet<i64>,
    added_players: &[AddedPlayer],
) -> Vec<SoloPlan> {
    signup_iter_order
        .iter()
        .filter_map(|discord_id| signups_by_discord.get(discord_id))
        .filter(|signup| signup.team_id.is_none())
        .map(|signup| {
            let id = discord_id_to_string(signup.discord_id);
            let added = added_players.iter().find(|p| p.discord_id == id);
            let outcome = match added.map(|p| p.source) {
                Some("solo_pool") => SoloOutcome::JoinsTeam,
                Some(_) => SoloOutcome::NewTeam,
                None => SoloOutcome::StaysSolo,
            };
            SoloPlan {
                discord_id: id,
                discord_name: signup.discord_name.clone(),
                checked_in: checked_in_ids.contains(&signup.discord_id),
                outcome,
                team_name: added.map(|p| p.team_name.clone()),
            }
        })
        .collect()
}

fn team_roster(
    original: &[MemberData],
    kept: &[MemberData],
    checked_in_ids: &HashSet<i64>,
) -> Vec<RosterEntry> {
    let kept_ids: HashSet<i64> = kept.iter().map(|m| m.discord_id).collect();
    let original_ids: HashSet<i64> = original.iter().map(|m| m.discord_id).collect();
    let current = original.iter().map(|m| {
        let change = if kept_ids.contains(&m.discord_id) {
            RosterChange::Stays
        } else {
            RosterChange::Removed
        };
        (m, change)
    });
    let added = kept
        .iter()
        .filter(|m| !original_ids.contains(&m.discord_id))
        .map(|m| (m, RosterChange::Added));
    current
        .chain(added)
        .map(|(m, change)| RosterEntry {
            discord_id: discord_id_to_string(m.discord_id),
            discord_name: m.discord_name.clone(),
            checked_in: checked_in_ids.contains(&m.discord_id),
            change,
        })
        .collect()
}

fn snapshot_token_for_formation(base: String, fill_from_solos: bool) -> String {
    if !fill_from_solos {
        return base;
    }
    let mut hasher = Sha256::new();
    hasher.update(base.as_bytes());
    hasher.update(b":fill_from_solos");
    hasher.finalize_hex()
}

async fn store_resolved_mode(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    mode: TournamentMode,
) -> TournamentResult<()> {
    sqlx::query(
        "UPDATE turnier.tournaments SET tournament_mode = $1, updated_at = $2 \
         WHERE id = $3 AND tournament_mode_auto",
    )
    .bind(mode)
    .bind(now_utc())
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    super::start_gap::ensure_start_gap(tx, tournament_id).await
}

fn formation_team_size(team_size: i64) -> TournamentResult<usize> {
    turnier_core::tournament::check_team_size(team_size).map_err(TournamentError::validation)?;
    usize::try_from(team_size).map_err(|_| TournamentError::validation("Teamgröße ungültig"))
}

#[derive(sqlx::FromRow)]
struct TournamentRow {
    status: TournamentStatus,
    team_size: i64,
    tournament_mode: TournamentMode,
    tournament_mode_auto: bool,
    bracket_format: BracketFormat,
    invite_mode: InviteMode,
}

/// Mitglieds-Sicht im Check-in-Algorithmus. Es werden nur `discord_id` (für die
/// Check-in-Prüfung) und `discord_name` (für Report-Felder) ausgewertet; die
/// übrigen Spalten der Solo-Pool-Einfügung kommen aus `signups_by_discord`, exakt
/// wie im Original.
#[derive(Clone)]
struct MemberData {
    discord_id: i64,
    discord_name: Option<String>,
}

#[derive(Clone)]
struct SignupData {
    id: i64,
    discord_id: i64,
    discord_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: i64,
    team_id: Option<i64>,
    signed_up_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    team_id: i64,
    discord_id: i64,
    discord_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SignupRow {
    id: i64,
    discord_id: i64,
    discord_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: Option<i64>,
    team_id: Option<i64>,
    signed_up_at: Option<DateTime<Utc>>,
}

struct CreatedTeamPlan {
    name: String,
    captain_discord_id: i64,
    members: Vec<SignupData>,
    team_id: Option<i64>,
}

/// Parameter für [`finalize_checkin`] (statt vieler Keyword-Argumente).
#[derive(Default)]
pub struct FinalizeCheckinParams<'a> {
    pub confirm: bool,
    pub allowed_team_ids: HashSet<i64>,
    pub actor_id: Option<&'a str>,
    pub expected_snapshot_token: Option<&'a str>,
    pub advance_to_group_phase: bool,
    pub fill_from_solos: bool,
}

/// Dry-Run/Confirm-Bereinigung der Teams nach Turnier-Check-ins. Portiert
/// `finalize_checkin` 1:1 (inkl. Snapshot-Schutz, Pool-Auffüllung, Team-Bildung,
/// optionalem Vorrücken). Eine Transaktion.
pub async fn finalize_checkin(
    pool: &sqlx::Pool<Postgres>,
    tournament_id: i64,
    params: FinalizeCheckinParams<'_>,
) -> TournamentResult<FinalizeCheckinResult> {
    let mut tx = pool.begin().await?;
    if params.confirm && params.advance_to_group_phase {
        super::start_gap::acquire_tournament_schedule_lock(&mut tx).await?;
    }

    let tournament = super::lock_tournament_row(&mut tx, tournament_id)
        .await?
        .map(|row| TournamentRow::from_row(&row))
        .transpose()?;
    let Some(tournament) = tournament else {
        return Err(TournamentError::validation("Turnier nicht gefunden"));
    };
    if tournament.status != TournamentStatus::Checkin {
        return Err(TournamentError::validation(
            "Check-in kann nur in der Check-in-Phase abgeschlossen werden",
        ));
    }

    let can_fill_from_solos = tournament.invite_mode != InviteMode::SoloOnly;
    let auto_team_formation = can_fill_from_solos && params.fill_from_solos;
    let keep_rosters = can_fill_from_solos && !params.fill_from_solos;

    let snapshot_token = snapshot_token_for_formation(
        snapshot_token_in_tx(&mut tx, tournament_id, tournament.status).await?,
        auto_team_formation,
    );
    if params.confirm {
        match params.expected_snapshot_token {
            None => {
                return Err(TournamentError::validation(
                    "snapshot_token ist für confirm erforderlich",
                ));
            }
            Some(token) if token != snapshot_token => {
                return Err(TournamentError::SnapshotMismatch);
            }
            _ => {}
        }
    }

    let team_size = tournament.team_size;
    formation_team_size(team_size)?;

    // Check-ins (Reihenfolge merken).
    let checkin_rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT discord_id FROM turnier.tournament_checkins \
         WHERE tournament_id = $1 ORDER BY checked_in_at, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut *tx)
    .await?;
    let checked_in_ids: HashSet<i64> = checkin_rows.iter().map(|(d,)| *d).collect();
    let checkin_order: HashMap<i64, usize> = checkin_rows
        .iter()
        .enumerate()
        .map(|(i, (d,))| (*d, i))
        .collect();

    // Teams (in stabiler Reihenfolge).
    let team_rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM turnier.teams WHERE tournament_id = $1 ORDER BY created_at, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut *tx)
    .await?;
    let team_order: Vec<i64> = team_rows.iter().map(|(id, _)| *id).collect();
    let team_name_by_id: HashMap<i64, String> = team_rows
        .iter()
        .map(|(id, name)| (*id, name.clone()))
        .collect();

    // Mitglieder je Team (stabile Reihenfolge). Es werden nur discord_id/
    // discord_name ausgewertet (siehe MemberData-Doku).
    let member_rows: Vec<MemberRow> = sqlx::query_as::<_, MemberRow>(
        "SELECT tm.team_id, tm.discord_id, tm.discord_name \
         FROM turnier.team_members tm JOIN turnier.teams t ON t.id = tm.team_id \
         WHERE t.tournament_id = $1 ORDER BY t.created_at, t.id, tm.joined_at, tm.id",
    )
    .bind(tournament_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut members_by_team: HashMap<i64, Vec<MemberData>> = HashMap::new();
    for row in &member_rows {
        members_by_team
            .entry(row.team_id)
            .or_default()
            .push(MemberData {
                discord_id: row.discord_id,
                discord_name: row.discord_name.clone(),
            });
    }

    // Signups (nach discord_id indexiert; Pool = Solo + eingecheckt).
    let signup_rows: Vec<SignupRow> = sqlx::query_as::<_, SignupRow>(
        "SELECT id, discord_id, discord_name, steam_id, rank, rank_score, team_id, signed_up_at \
         FROM turnier.tournament_signups WHERE tournament_id = $1 \
         AND (team_id IS NOT NULL OR NOT EXISTS (SELECT 1 FROM turnier.solo_balance_waitlist w \
              WHERE w.signup_id = turnier.tournament_signups.id)) ORDER BY signed_up_at, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut signups_by_discord: HashMap<i64, SignupData> = HashMap::new();
    let mut signup_iter_order: Vec<i64> = Vec::new();
    for row in &signup_rows {
        signup_iter_order.push(row.discord_id);
        signups_by_discord.insert(
            row.discord_id,
            SignupData {
                id: row.id,
                discord_id: row.discord_id,
                discord_name: row.discord_name.clone(),
                steam_id: row.steam_id.clone(),
                rank: row.rank.clone(),
                rank_score: row.rank_score.unwrap_or(0),
                team_id: row.team_id,
                signed_up_at: row.signed_up_at,
            },
        );
    }

    // Team-Status: behaltene vs. entfernte Mitglieder.
    let mut team_states: HashMap<i64, Vec<MemberData>> = HashMap::new();
    for team_id in &team_order {
        team_states.insert(
            *team_id,
            members_by_team.get(team_id).cloned().unwrap_or_default(),
        );
    }
    let mut removed_members: Vec<RemovedPlayer> = Vec::new();
    for team_id in &team_order {
        let members = team_states.get(team_id).cloned().unwrap_or_default();
        let mut kept: Vec<MemberData> = Vec::new();
        for member in members {
            if keep_rosters || checked_in_ids.contains(&member.discord_id) {
                kept.push(member);
            } else {
                removed_members.push(RemovedPlayer {
                    team_id: *team_id,
                    team_name: team_name_by_id[team_id].clone(),
                    discord_id: discord_id_to_string(member.discord_id),
                    discord_name: member.discord_name.clone(),
                });
            }
        }
        team_states.insert(*team_id, kept);
    }

    // Solo-Pool: ungebundene, eingecheckte Signups (sortiert nach Check-in-Reihenfolge).
    let mut pool_entries: Vec<SignupData> = Vec::new();
    // Iteration in signups-Query-Reihenfolge (signed_up_at, id), wie .values() in
    // Python (CPython dict bewahrt Insertionsreihenfolge).
    for discord_id in &signup_iter_order {
        let signup = &signups_by_discord[discord_id];
        if signup.team_id.is_some() {
            continue;
        }
        if !checked_in_ids.contains(&signup.discord_id) {
            continue;
        }
        pool_entries.push(signup.clone());
    }
    pool_entries.sort_by(|a, b| {
        let ka = (
            checkin_order
                .get(&a.discord_id)
                .map(|i| *i as i64)
                .unwrap_or(i64::MAX),
            a.signed_up_at,
            a.id,
        );
        let kb = (
            checkin_order
                .get(&b.discord_id)
                .map(|i| *i as i64)
                .unwrap_or(i64::MAX),
            b.signed_up_at,
            b.id,
        );
        ka.cmp(&kb)
    });
    let mut pool: std::collections::VecDeque<SignupData> = pool_entries.into();

    let mut added_players: Vec<AddedPlayer> = Vec::new();
    let mut created_teams: Vec<CreatedTeamPlan> = Vec::new();
    let mut affected_team_ids: HashSet<i64> = HashSet::new();

    // Unvollständige bestehende Teams aus dem Pool auffüllen.
    for team_id in &team_order {
        let mut members = team_states.get(team_id).cloned().unwrap_or_default();
        while auto_team_formation && (members.len() as i64) < team_size {
            let Some(signup) = pool.pop_front() else {
                break;
            };
            members.push(MemberData {
                discord_id: signup.discord_id,
                discord_name: signup.discord_name.clone(),
            });
            affected_team_ids.insert(*team_id);
            added_players.push(AddedPlayer {
                team_id: Some(*team_id),
                team_name: team_name_by_id[team_id].clone(),
                discord_id: discord_id_to_string(signup.discord_id),
                discord_name: signup.discord_name.clone(),
                source: "solo_pool",
            });
        }
        team_states.insert(*team_id, members);
    }

    // Neue Teams aus dem Restpool.
    let existing_keys: Vec<(String,)> =
        sqlx::query_as("SELECT name_key FROM turnier.teams WHERE tournament_id = $1")
            .bind(tournament_id)
            .fetch_all(&mut *tx)
            .await?;
    let mut name_pool = TeamNamePool::new(existing_keys.into_iter().map(|(k,)| k));
    while auto_team_formation && pool.len() >= team_size as usize {
        let chunk: Vec<SignupData> = pool.drain(..team_size as usize).collect();
        let captain_name = captain_team_name(&mut tx, tournament_id, chunk[0].discord_id).await?;
        let team_name = name_pool.from_captain(captain_name.as_deref());
        for signup in &chunk {
            added_players.push(AddedPlayer {
                team_id: None,
                team_name: team_name.clone(),
                discord_id: discord_id_to_string(signup.discord_id),
                discord_name: signup.discord_name.clone(),
                source: "new_team",
            });
        }
        created_teams.push(CreatedTeamPlan {
            name: team_name,
            captain_discord_id: chunk[0].discord_id,
            members: chunk,
            team_id: None,
        });
    }

    // Warnungen + zu löschende leere Teams.
    let mut warnings: Vec<TeamWarning> = Vec::new();
    let mut deleted_team_ids: Vec<i64> = Vec::new();
    let mut team_plans: Vec<TeamPlan> = Vec::new();
    for team_id in &team_order {
        let original = members_by_team
            .get(team_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let kept = team_states.get(team_id).map(Vec::as_slice).unwrap_or(&[]);
        let checked_in = kept
            .iter()
            .filter(|m| checked_in_ids.contains(&m.discord_id))
            .count() as i64;
        let member_count = kept.len() as i64;
        let ready = if keep_rosters {
            checked_in
        } else {
            member_count
        };
        let outcome = if !keep_rosters && member_count == 0 {
            deleted_team_ids.push(*team_id);
            TeamOutcome::Deleted
        } else if ready < team_size {
            warnings.push(TeamWarning {
                team_id: *team_id,
                team_name: team_name_by_id[team_id].clone(),
                current: ready,
                required: team_size,
            });
            TeamOutcome::Incomplete
        } else {
            TeamOutcome::Complete
        };
        team_plans.push(TeamPlan {
            team_id: *team_id,
            team_name: team_name_by_id[team_id].clone(),
            roster: team_roster(original, kept, &checked_in_ids),
            checked_in,
            required: team_size,
            outcome,
        });
    }
    let solo_plans = solo_plans(
        &signup_iter_order,
        &signups_by_discord,
        &checked_in_ids,
        &added_players,
    );

    let final_team_count =
        team_order.len().saturating_sub(deleted_team_ids.len()) + created_teams.len();
    let tournament_mode = crate::status::resolve_tournament_mode(
        tournament.tournament_mode,
        tournament.tournament_mode_auto,
        final_team_count,
    );
    let bracket_entrants =
        crate::engine::double_elim::expected_bracket_entrants(tournament_mode, final_team_count);
    let effective_bracket_format = crate::engine::double_elim::effective_bracket_format(
        tournament.bracket_format,
        bracket_entrants,
    );

    let missing_allowed: Vec<&TeamWarning> = warnings
        .iter()
        .filter(|w| !params.allowed_team_ids.contains(&w.team_id))
        .collect();
    if params.confirm && !missing_allowed.is_empty() {
        let labels = missing_allowed
            .iter()
            .map(|w| format!("{} ({}/{})", w.team_name, w.current, w.required))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(TournamentError::validation(format!(
            "Unvollständige Teams müssen bestätigt werden: {labels}"
        )));
    }

    let mut groups_created = 0i64;
    let mut matches_created = 0i64;
    let mut advanced_to_bracket = false;
    let mut entered_status: Option<TournamentStatus> = None;

    if params.confirm {
        // Entfernte Mitglieder löschen + Signup entkoppeln.
        for removed in &removed_members {
            let removed_discord_id = parse_db_discord_id(&removed.discord_id)?;
            sqlx::query("DELETE FROM turnier.team_members WHERE team_id = $1 AND discord_id = $2")
                .bind(removed.team_id)
                .bind(removed_discord_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "UPDATE turnier.tournament_signups SET team_id = NULL \
                 WHERE tournament_id = $1 AND discord_id = $2",
            )
            .bind(tournament_id)
            .bind(removed_discord_id)
            .execute(&mut *tx)
            .await?;
            affected_team_ids.insert(removed.team_id);
        }

        // Leere Teams löschen.
        for team_id in &deleted_team_ids {
            sqlx::query(
                "UPDATE turnier.tournament_signups SET team_id = NULL \
                 WHERE tournament_id = $1 AND team_id = $2",
            )
            .bind(tournament_id)
            .bind(team_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM turnier.team_members WHERE team_id = $1")
                .bind(team_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM turnier.teams WHERE id = $1")
                .bind(team_id)
                .execute(&mut *tx)
                .await?;
            affected_team_ids.remove(team_id);
        }

        // Solo-Pool-Mitglieder in bestehende Teams einfügen.
        for added in &added_players {
            if added.source != "solo_pool" {
                continue;
            }
            let Some(team_id) = added.team_id else {
                continue;
            };
            let added_discord_id = parse_db_discord_id(&added.discord_id)?;
            let Some(signup) = signups_by_discord.get(&added_discord_id) else {
                continue;
            };
            super::insert_team_member(
                &mut tx,
                &NewTeamMember {
                    team_id,
                    discord_id: signup.discord_id,
                    discord_name: signup.discord_name.as_deref(),
                    steam_id: signup.steam_id.as_deref(),
                    rank: signup.rank.as_deref(),
                    rank_score: signup.rank_score,
                    role: TeamRole::Member,
                },
            )
            .await?;
            sqlx::query("UPDATE turnier.tournament_signups SET team_id = $1 WHERE id = $2")
                .bind(team_id)
                .bind(signup.id)
                .execute(&mut *tx)
                .await?;
        }

        // Neue Teams anlegen.
        for created_team in &mut created_teams {
            let new_team_id = super::insert_team(
                &mut tx,
                tournament_id,
                &created_team.name,
                created_team.captain_discord_id,
                RecruitmentStatus::Open,
            )
            .await?;
            created_team.team_id = Some(new_team_id);

            for (index, signup) in created_team.members.iter().enumerate() {
                let role = if index == 0 {
                    TeamRole::Captain
                } else {
                    TeamRole::Member
                };
                super::insert_team_member(
                    &mut tx,
                    &NewTeamMember {
                        team_id: new_team_id,
                        discord_id: signup.discord_id,
                        discord_name: signup.discord_name.as_deref(),
                        steam_id: signup.steam_id.as_deref(),
                        rank: signup.rank.as_deref(),
                        rank_score: signup.rank_score,
                        role,
                    },
                )
                .await?;
                sqlx::query("UPDATE turnier.tournament_signups SET team_id = $1 WHERE id = $2")
                    .bind(new_team_id)
                    .bind(signup.id)
                    .execute(&mut *tx)
                    .await?;
            }

            for added in &mut added_players {
                if added.source == "new_team" && added.team_name == created_team.name {
                    added.team_id = Some(new_team_id);
                }
            }
        }

        // Captain-Sync je betroffenem Team (sortiert).
        let mut sorted_affected: Vec<i64> = affected_team_ids.iter().copied().collect();
        sorted_affected.sort_unstable();
        for team_id in sorted_affected {
            sync_team_captain(&mut tx, team_id).await?;
        }

        // Optionales Vorrücken.
        if params.advance_to_group_phase {
            if tournament_mode != tournament.tournament_mode {
                store_resolved_mode(&mut tx, tournament_id, tournament_mode).await?;
            }
            let status_changed: u64;
            if tournament_mode == TournamentMode::League {
                let plan = super::league::generate_league_plan_in_tx(
                    &mut tx,
                    tournament_id,
                    crate::league::DEFAULT_SLOT_MINUTES,
                )
                .await?;
                groups_created = 1;
                matches_created = plan.matches;
                entered_status = Some(TournamentStatus::GroupPhase);
                let res = sqlx::query(
                    "UPDATE turnier.tournaments SET status = 'group_phase', updated_at = $1 \
                     WHERE id = $2 AND status = 'checkin'",
                )
                .bind(now_utc())
                .bind(tournament_id)
                .execute(&mut *tx)
                .await?;
                status_changed = res.rows_affected();
            } else if tournament_mode == TournamentMode::GroupStage {
                let group_ids = generate_groups_in_tx(&mut tx, tournament_id, None).await?;
                groups_created = group_ids.len() as i64;
                matches_created = generate_group_matches_in_tx(&mut tx, tournament_id).await?;
                entered_status = Some(TournamentStatus::GroupPhase);
                let res = sqlx::query(
                    "UPDATE turnier.tournaments SET status = 'group_phase', updated_at = $1 \
                     WHERE id = $2 AND status = 'checkin'",
                )
                .bind(now_utc())
                .bind(tournament_id)
                .execute(&mut *tx)
                .await?;
                status_changed = res.rows_affected();
            } else {
                super::clear_bracket_tree(&mut tx, tournament_id).await?;
                let seeded: Vec<(i64,)> = sqlx::query_as(
                    "SELECT id FROM turnier.teams WHERE tournament_id = $1 ORDER BY created_at, id",
                )
                .bind(tournament_id)
                .fetch_all(&mut *tx)
                .await?;
                let entries: Vec<crate::engine::slots::BracketSlot> = seeded
                    .into_iter()
                    .map(|(id,)| crate::engine::slots::BracketSlot::Team(id))
                    .collect();
                if tournament.bracket_format == BracketFormat::DoubleElimination {
                    matches_created = build_double_elimination_bracket(
                        &mut tx,
                        tournament_id,
                        super::bracket::DoubleElimInput::Entries(entries),
                    )
                    .await?;
                } else {
                    matches_created = build_seeded_bracket(&mut tx, tournament_id, entries).await?;
                }
                let res = sqlx::query(
                    "UPDATE turnier.tournaments SET status = 'bracket', updated_at = $1 \
                     WHERE id = $2 AND status = 'checkin'",
                )
                .bind(now_utc())
                .bind(tournament_id)
                .execute(&mut *tx)
                .await?;
                status_changed = res.rows_affected();
                advanced_to_bracket = true;
                entered_status = Some(TournamentStatus::Bracket);
            }
            if status_changed == 0 {
                return Err(TournamentError::StatusConflict);
            }
        }

        // Audit (in derselben Transaktion).
        let details = serde_json::json!({
            "tournament_id": tournament_id,
            "removed_players": removed_members.len(),
            "added_players": added_players.len(),
            "deleted_team_ids": deleted_team_ids,
            "created_teams": created_teams.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
            "created_team_members": created_teams.iter().map(|t| serde_json::json!({
                "team_name": t.name,
                "discord_ids": t.members.iter().map(|m| discord_id_to_string(m.discord_id)).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "auto_team_formation": auto_team_formation,
            "fill_from_solos": params.fill_from_solos,
            "kept_rosters": keep_rosters,
            "effective_bracket_format": effective_bracket_format,
            "tournament_mode": tournament_mode,
            "tournament_mode_auto": tournament.tournament_mode_auto,
            "advanced_to_group_phase": params.advance_to_group_phase,
            "advanced_to_bracket": advanced_to_bracket,
            "groups_created": groups_created,
            "matches_created": matches_created,
        })
        .to_string();
        audit(&mut tx, "finalize_checkin", params.actor_id, &details).await?;
    }

    let result = FinalizeCheckinResult {
        warnings,
        removed_players: removed_members,
        added_players: added_players.clone(),
        created_teams: created_teams
            .iter()
            .map(|t| CreatedTeam {
                team_id: t.team_id,
                team_name: t.name.clone(),
                members: t
                    .members
                    .iter()
                    .map(|m| SoloPlayer {
                        discord_id: discord_id_to_string(m.discord_id),
                        discord_name: m.discord_name.clone(),
                    })
                    .collect(),
            })
            .collect(),
        deleted_team_ids,
        remaining_solo_players: pool
            .iter()
            .map(|s| SoloPlayer {
                discord_id: discord_id_to_string(s.discord_id),
                discord_name: s.discord_name.clone(),
            })
            .collect(),
        team_plans,
        solo_plans,
        auto_team_formation,
        can_fill_from_solos,
        bracket_format: tournament.bracket_format,
        effective_bracket_format,
        bracket_entrants,
        final_team_count,
        tournament_mode,
        tournament_mode_auto: tournament.tournament_mode_auto,
        dry_run: !params.confirm,
        snapshot_token,
        groups_created: if params.confirm && params.advance_to_group_phase {
            Some(groups_created)
        } else {
            None
        },
        matches_created: if params.confirm && params.advance_to_group_phase {
            Some(matches_created)
        } else {
            None
        },
        advanced_to_group_phase: if params.confirm && params.advance_to_group_phase {
            Some(groups_created > 0)
        } else {
            None
        },
        advanced_to_bracket: if params.confirm && params.advance_to_group_phase {
            Some(advanced_to_bracket)
        } else {
            None
        },
        entered_status,
    };

    tx.commit().await?;
    Ok(result)
}

/// Setzt den Captain eines Teams auf das (nach joined_at, id) erste Mitglied.
/// Portiert `_sync_team_captain`.
async fn sync_team_captain(
    tx: &mut Transaction<'_, Postgres>,
    team_id: i64,
) -> TournamentResult<()> {
    let members: Vec<(i64,)> = sqlx::query_as(
        "SELECT discord_id FROM turnier.team_members WHERE team_id = $1 ORDER BY joined_at, id",
    )
    .bind(team_id)
    .fetch_all(&mut **tx)
    .await?;
    let captain_id = members.first().map(|(d,)| *d);

    sqlx::query("UPDATE turnier.team_members SET role = 'member' WHERE team_id = $1")
        .bind(team_id)
        .execute(&mut **tx)
        .await?;
    if let Some(captain_id) = captain_id {
        sqlx::query(
            "UPDATE turnier.team_members SET role = 'captain' WHERE team_id = $1 AND discord_id = $2",
        )
        .bind(team_id)
        .bind(captain_id)
        .execute(&mut **tx)
        .await?;

        sqlx::query("UPDATE turnier.teams SET captain_discord_id = $1 WHERE id = $2")
            .bind(captain_id)
            .bind(team_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
