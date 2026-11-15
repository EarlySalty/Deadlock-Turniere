use chrono::{DateTime, Utc};
use sqlx::postgres::PgRow;
use sqlx::PgConnection;
use turnier_core::{RecruitmentStatus, TeamRole};

use crate::engine::naming::name_key;

pub struct NewTeamMember<'a> {
    pub team_id: i64,
    pub discord_id: i64,
    pub discord_name: Option<&'a str>,
    pub steam_id: Option<&'a str>,
    pub rank: Option<&'a str>,
    pub rank_score: i64,
    pub role: TeamRole,
}

pub struct NewSignup<'a> {
    pub tournament_id: i64,
    pub discord_id: i64,
    pub discord_name: Option<&'a str>,
    pub steam_id: Option<&'a str>,
    pub rank: Option<&'a str>,
    pub rank_score: i64,
    pub team_id: Option<i64>,
}

pub async fn lock_tournament_row(
    conn: &mut PgConnection,
    tournament_id: i64,
) -> sqlx::Result<Option<PgRow>> {
    sqlx::query(r#"SELECT * FROM turnier."tournaments" WHERE id = $1 FOR UPDATE"#)
        .bind(tournament_id)
        .fetch_optional(conn)
        .await
}

pub async fn insert_team(
    conn: &mut PgConnection,
    tournament_id: i64,
    name: &str,
    captain_discord_id: i64,
    recruitment_status: RecruitmentStatus,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        r#"INSERT INTO turnier."teams"
         (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status)
         VALUES ($1, $2, $3, $4, now(), $5) RETURNING id"#,
    )
    .bind(tournament_id)
    .bind(name)
    .bind(name_key(name))
    .bind(captain_discord_id)
    .bind(recruitment_status)
    .fetch_one(conn)
    .await
}

pub async fn insert_team_member(
    conn: &mut PgConnection,
    member: &NewTeamMember<'_>,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        r#"INSERT INTO turnier."team_members"
         (team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now()) RETURNING id"#,
    )
    .bind(member.team_id)
    .bind(member.discord_id)
    .bind(member.discord_name)
    .bind(member.steam_id)
    .bind(member.rank)
    .bind(member.rank_score)
    .bind(member.role)
    .fetch_one(conn)
    .await
}

pub async fn insert_signup(
    conn: &mut PgConnection,
    signup: &NewSignup<'_>,
) -> sqlx::Result<(i64, DateTime<Utc>)> {
    sqlx::query_as(
        r#"INSERT INTO turnier."tournament_signups"
         (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id, signed_up_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now()) RETURNING id, signed_up_at"#,
    )
    .bind(signup.tournament_id)
    .bind(signup.discord_id)
    .bind(signup.discord_name)
    .bind(signup.steam_id)
    .bind(signup.rank)
    .bind(signup.rank_score)
    .bind(signup.team_id)
    .fetch_one(conn)
    .await
}
