//! DM-Opt-out-Persistenz und reine Empfaenger-Berechnung.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use turnier_db::Pool;

use crate::error::AutomatikResult;
use crate::presets::Category;

/// Opt-out-Geltungsbereich.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
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

/// Setzt einen Opt-out-Scope idempotent.
pub async fn set_optout(pool: &Pool, discord_id: &str, scope: Scope) -> AutomatikResult<()> {
    sqlx::query("INSERT OR IGNORE INTO tournament_dm_optout (discord_id, scope) VALUES (?, ?)")
        .bind(discord_id)
        .bind(scope)
        .execute(pool)
        .await?;
    Ok(())
}

/// Entfernt einen Opt-out-Scope.
pub async fn clear_optout(pool: &Pool, discord_id: &str, scope: Scope) -> AutomatikResult<bool> {
    let res = sqlx::query("DELETE FROM tournament_dm_optout WHERE discord_id = ? AND scope = ?")
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
    let category_scope = Scope::from_category(category);
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM tournament_dm_optout \
         WHERE discord_id = ? AND scope IN (?, 'all')",
    )
    .bind(discord_id)
    .bind(category_scope)
    .fetch_one(pool)
    .await?;
    Ok(row.0 > 0)
}

/// Listet Opt-outs eines Users.
pub async fn list_optouts(pool: &Pool, discord_id: &str) -> AutomatikResult<Vec<OptOut>> {
    let rows = sqlx::query_as::<_, OptOut>(
        "SELECT * FROM tournament_dm_optout WHERE discord_id = ? ORDER BY id",
    )
    .bind(discord_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
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
