#![cfg(feature = "testing")]

//! PG-Smoke fuer die zentrale Test-Harness: `dl-central-db` erzeugt eine
//! Wegwerf-Datenbank und wendet die zentralen Migrationen an.

use sqlx::Row;
use turnier_db::{run_migrations, test_pool};

async fn turnier_table_names(pool: &turnier_db::Pool) -> Vec<String> {
    sqlx::query(
        "SELECT table_name \
         FROM information_schema.tables \
         WHERE table_schema = 'turnier' AND table_type = 'BASE TABLE' \
         ORDER BY table_name",
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|row| row.get::<String, _>("table_name"))
    .collect()
}

#[tokio::test]
async fn zentrale_test_harness_enthaelt_turnier_schema() {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool();

    let tables = turnier_table_names(pool).await;
    assert_eq!(
        tables.len(),
        39,
        "erwarte 39 turnier-Tabellen, bekam {}",
        tables.len()
    );
    for must in [
        "comp_lobbies",
        "comp_members",
        "tournaments",
        "teams",
        "bracket_matches",
        "tournament_dm_optout",
        "draft_sessions",
        "match_result_reports",
    ] {
        assert!(tables.contains(&must.to_string()), "Tabelle {must} fehlt");
    }
}

#[tokio::test]
async fn zentrale_test_harness_enthaelt_core_und_voice_schemas() {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool();

    for schema in ["core", "voice", "turnier"] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS ( \
             SELECT 1 FROM information_schema.schemata WHERE schema_name = $1 \
             )",
        )
        .bind(schema)
        .fetch_one(pool)
        .await
        .expect("schema exists query");
        assert!(exists, "Schema {schema} fehlt");
    }
}

#[tokio::test]
async fn zentrale_turnier_tabellen_nutzen_pg_typen() {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool();

    let rows = sqlx::query(
        "SELECT table_name, column_name, data_type, udt_name \
         FROM information_schema.columns \
         WHERE table_schema = 'turnier' \
           AND (table_name, column_name) IN ( \
             ('tournaments', 'id'), \
             ('tournaments', 'created_at'), \
             ('tournaments', 'lobby_settings'), \
             ('tournaments', 'is_test'), \
             ('tournament_dm_optout', 'discord_id') \
           )",
    )
    .fetch_all(pool)
    .await
    .expect("column metadata");

    let mut columns = Vec::new();
    for row in rows {
        columns.push((
            row.get::<String, _>("table_name"),
            row.get::<String, _>("column_name"),
            row.get::<String, _>("data_type"),
            row.get::<String, _>("udt_name"),
        ));
    }

    assert!(columns.contains(&(
        "tournaments".to_string(),
        "id".to_string(),
        "bigint".to_string(),
        "int8".to_string(),
    )));
    assert!(columns.contains(&(
        "tournaments".to_string(),
        "created_at".to_string(),
        "timestamp with time zone".to_string(),
        "timestamptz".to_string(),
    )));
    assert!(columns.contains(&(
        "tournaments".to_string(),
        "lobby_settings".to_string(),
        "jsonb".to_string(),
        "jsonb".to_string(),
    )));
    assert!(columns.contains(&(
        "tournaments".to_string(),
        "is_test".to_string(),
        "boolean".to_string(),
        "bool".to_string(),
    )));
    assert!(columns.contains(&(
        "tournament_dm_optout".to_string(),
        "discord_id".to_string(),
        "bigint".to_string(),
        "int8".to_string(),
    )));
}

#[tokio::test]
async fn lokale_run_migrations_ist_pg_noop() {
    let db = test_pool().await.expect("central test pool");
    run_migrations(db.pool()).await.expect("migration no-op");
}
