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

fn source_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            source_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn referenced_turnier_tables() -> std::collections::BTreeSet<String> {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates).unwrap() {
        let src = entry.unwrap().path().join("src");
        if src.is_dir() {
            source_files(&src, &mut files);
        }
    }
    let mut tables = std::collections::BTreeSet::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for (at, _) in text.match_indices("turnier.") {
            let rest = text[at + "turnier.".len()..].trim_start_matches(['"', '\\']);
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .collect();
            let after = rest[name.len()..].trim_start_matches(['"', '\\']);
            if !name.is_empty() && !after.starts_with('(') {
                tables.insert(name);
            }
        }
    }
    tables
}

#[tokio::test]
async fn zentrale_test_harness_enthaelt_jede_genutzte_turnier_tabelle() {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool();

    let tables = turnier_table_names(pool).await;
    let referenced = referenced_turnier_tables();
    assert!(
        referenced.len() > 30,
        "Quelltext-Scan fand nur {referenced:?}"
    );
    let missing: Vec<&String> = referenced
        .iter()
        .filter(|name| !tables.contains(name))
        .collect();
    assert!(missing.is_empty(), "Tabellen fehlen im Schema: {missing:?}");
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
