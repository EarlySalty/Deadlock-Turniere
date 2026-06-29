//! Integrationstest: die eingebettete Migration läuft über sqlx sauber durch —
//! sowohl auf einer frischen DB als auch (idempotent) auf einer Kopie der echten
//! Live-DB.

use std::path::PathBuf;

use sqlx::Row;
use turnier_db::{connect, run_migrations};

fn temp_db(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("tb_db_{}_{}.db", tag, std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

async fn table_names(pool: &turnier_db::Pool) -> Vec<String> {
    sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations' ORDER BY name")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect()
}

#[tokio::test]
async fn migration_baut_frische_db_vollstaendig_auf() {
    let path = temp_db("fresh");
    let pool = connect(&path, 2).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");

    let tables = table_names(&pool).await;
    assert_eq!(tables.len(), 37, "erwarte 37 Tabellen, bekam {}", tables.len());
    for must in ["tournaments", "teams", "bracket_matches", "draft_sessions", "match_result_reports"] {
        assert!(tables.contains(&must.to_string()), "Tabelle {must} fehlt");
    }

    pool.close().await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn migration_ist_idempotent_auf_live_db_kopie() {
    // Die echte Live-DB liegt relativ zum Crate-Manifest. Fehlt sie (z. B. in CI),
    // wird der Test übersprungen statt zu scheitern.
    let live = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../backend/data/tournament.db");
    if !live.exists() {
        eprintln!("Live-DB nicht vorhanden ({}), Test übersprungen", live.display());
        return;
    }

    let path = temp_db("livecopy");
    std::fs::copy(&live, &path).expect("copy live db");

    let pool = connect(&path, 2).await.expect("connect");
    // Darf auf der bestehenden DB nicht scheitern (alle CREATEs sind IF NOT EXISTS).
    run_migrations(&pool).await.expect("migrate on live copy");
    // Zweiter Lauf = garantiert No-op.
    run_migrations(&pool).await.expect("migrate idempotent");

    let tables = table_names(&pool).await;
    assert_eq!(tables.len(), 37);

    pool.close().await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn migration_erzwingt_automatik_enum_checks_und_tournament_source_not_null() {
    let path = temp_db("constraints");
    let pool = connect(&path, 2).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");

    let bad_category = sqlx::query(
        "INSERT INTO tournament_presets (name, category, created_by) \
         VALUES ('Bad Preset', 'xxx', 'tester')",
    )
    .execute(&pool)
    .await;
    assert!(bad_category.is_err(), "ungueltige Preset-Kategorie wurde akzeptiert");

    let missing_source = sqlx::query(
        "INSERT INTO tournaments (name, status, created_by, source) \
         VALUES ('Bad Cup', 'draft', 'tester', NULL)",
    )
    .execute(&pool)
    .await;
    assert!(missing_source.is_err(), "NULL fuer tournaments.source wurde akzeptiert");

    let row: (String,) = sqlx::query_as(
        "INSERT INTO tournaments (name, status, created_by) \
         VALUES ('Default Cup', 'draft', 'tester') RETURNING source",
    )
    .fetch_one(&pool)
    .await
    .expect("Tournament mit Default-Source anlegen");
    assert_eq!(row.0, "manual");

    pool.close().await;
    let _ = std::fs::remove_file(&path);
}
