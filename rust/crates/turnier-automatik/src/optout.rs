//! DM-Opt-out-Persistenz und reine Empfaenger-Berechnung.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use turnier_core::{discord_id_to_string, now_utc, parse_discord_id};
use turnier_db::Pool;

use crate::error::{AutomatikError, AutomatikResult};
use crate::presets::Category;

/// Opt-out-Geltungsbereich.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum Scope {
    Fun,
    Comp,
    All,
}

impl Scope {
    /// Scope fuer eine Kategorie.
    pub fn from_category(category: Category) -> Self {
        match category {
            Category::Fun => Scope::Fun,
            Category::Comp => Scope::Comp,
        }
    }

    /// True, wenn dieser Scope eine DM der Kategorie unterdrueckt.
    pub fn matches_category(self, category: Category) -> bool {
        self == Scope::All || self == Scope::from_category(category)
    }
}

/// DB-Zeile aus `tournament_dm_optout`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OptOut {
    pub id: i64,
    pub discord_id: String,
    pub scope: Scope,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct OptOutRow {
    pub id: i64,
    pub discord_id: i64,
    pub scope: Scope,
    pub created_at: DateTime<Utc>,
}

impl From<OptOutRow> for OptOut {
    fn from(row: OptOutRow) -> Self {
        Self {
            id: row.id,
            discord_id: discord_id_to_string(row.discord_id),
            scope: row.scope,
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

/// Setzt einen Opt-out-Scope idempotent.
pub async fn set_optout(pool: &Pool, discord_id: &str, scope: Scope) -> AutomatikResult<()> {
    let discord_id = parse_numeric_id(discord_id)?;
    let now = now_utc();
    sqlx::query(
        "INSERT INTO turnier.tournament_dm_optout (discord_id, scope, created_at) \
         VALUES ($1, $2, $3) ON CONFLICT (discord_id, scope) DO NOTHING",
    )
    .bind(discord_id)
    .bind(scope)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Entfernt einen Opt-out-Scope.
pub async fn clear_optout(pool: &Pool, discord_id: &str, scope: Scope) -> AutomatikResult<bool> {
    let discord_id = parse_numeric_id(discord_id)?;
    let res = sqlx::query(
        "DELETE FROM turnier.tournament_dm_optout WHERE discord_id = $1 AND scope = $2",
    )
    .bind(discord_id)
    .bind(scope)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Prueft, ob ein User fuer die Kategorie opt-out ist. `all` gilt immer.
pub async fn is_opted_out(
    pool: &Pool,
    discord_id: &str,
    category: Category,
) -> AutomatikResult<bool> {
    let discord_id = parse_numeric_id(discord_id)?;
    let category_scope = Scope::from_category(category);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.tournament_dm_optout \
         WHERE discord_id = $1 AND scope IN ($2, 'all')",
    )
    .bind(discord_id)
    .bind(category_scope)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Listet Opt-outs eines Users.
pub async fn list_optouts(pool: &Pool, discord_id: &str) -> AutomatikResult<Vec<OptOut>> {
    let discord_id = parse_numeric_id(discord_id)?;
    let rows = sqlx::query_as::<_, OptOutRow>(
        "SELECT * FROM turnier.tournament_dm_optout WHERE discord_id = $1 ORDER BY id",
    )
    .bind(discord_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Reine Empfaenger-Berechnung: Rollenmitglieder minus Kategorie-Opt-out minus
/// globalem Opt-out. Die Reihenfolge der Rollenliste bleibt erhalten.
pub fn compute_recipients(
    role_members: &[String],
    optouts: &[(String, Scope)],
    category: Category,
) -> Vec<String> {
    let excluded: HashSet<&str> = optouts
        .iter()
        .filter(|(_, scope)| scope.matches_category(category))
        .map(|(discord_id, _)| discord_id.as_str())
        .collect();

    let mut seen = HashSet::new();
    let mut recipients = Vec::new();
    for discord_id in role_members {
        if excluded.contains(discord_id.as_str()) {
            continue;
        }
        if seen.insert(discord_id.as_str()) {
            recipients.push(discord_id.clone());
        }
    }
    recipients
}

fn parse_numeric_id(value: &str) -> AutomatikResult<i64> {
    parse_discord_id(value).map_err(|_| AutomatikError::InvalidNumericId(value.to_string()))
}
