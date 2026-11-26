#![cfg(feature = "testing")]
// Agent07: characterization of the pinned central test schema, not a fix to the original count test.
#[tokio::test]
async fn agent07_solo_balance_schema() {
    let db = turnier_db::test_pool().await.expect("isolated central test pool");
    let tables: Vec<String> = sqlx::query_scalar("SELECT table_name FROM information_schema.tables WHERE table_schema='turnier' AND table_type='BASE TABLE' ORDER BY table_name")
        .fetch_all(db.pool()).await.unwrap();
    assert_eq!(tables.len(),41);
    for must in ["solo_balance_runs","solo_balance_waitlist","comp_lobbies","comp_members","tournaments","teams","bracket_matches","tournament_dm_optout","draft_sessions","match_result_reports"] {
        assert!(tables.iter().any(|name| name==must), "missing {must}");
    }
    println!("synthetic schema only: {}", tables.join(","));
}
