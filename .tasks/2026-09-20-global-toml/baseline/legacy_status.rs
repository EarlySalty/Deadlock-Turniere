#[allow(dead_code)]
#[path = "/home/nathanael/repos/Deadlock-Turniere/rust/crates/turnier-config/src/lib.rs"]
mod legacy;

use std::path::Path;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let old = legacy::Config::from_env();
    let text = std::fs::read_to_string("/home/nathanael/.worktrees/turniere-global-toml-20260920/config/bot.toml")?;
    let candidate = turnier_config::Config::parse_file(&text, Path::new("/home/nathanael/repos/Deadlock-Turniere/config/bot.toml"))?;
    let new: serde_json::Value = serde_json::from_str(&candidate.safe_status()?)?;
    let mut old_values = serde_json::Map::new();
    old_values.insert("discord_oauth_internal_api_base_url".into(), json!(old.discord_oauth_internal_api_base_url));
    old_values.insert("discord_master_broker_base_url".into(), json!(old.discord_master_broker_base_url));
    old_values.insert("discord_match_channel_category_id".into(), json!(old.discord_match_channel_category_id));
    old_values.insert("discord_match_channel_delete_delay_seconds".into(), json!(old.discord_match_channel_delete_delay_seconds));
    old_values.insert("discord_sammelpunkt_channel_id".into(), json!(old.discord_sammelpunkt_channel_id));
    old_values.insert("discord_team1_voice_channel_id".into(), json!(old.discord_team1_voice_channel_id));
    old_values.insert("discord_team2_voice_channel_id".into(), json!(old.discord_team2_voice_channel_id));
    old_values.insert("discord_tournament_lobby_channel_id".into(), json!(old.discord_tournament_lobby_channel_id));
    old_values.insert("discord_caster_role_id".into(), json!(old.discord_caster_role_id));
    old_values.insert("discord_caster_voice_channel_id".into(), json!(old.discord_caster_voice_channel_id));
    old_values.insert("turnier_public_url".into(), json!(old.turnier_public_url));
    old_values.insert("discord_guild_id".into(), json!(old.discord_guild_id));
    old_values.insert("discord_admin_role_ids".into(), json!(old.discord_admin_role_ids));
    old_values.insert("discord_tournament_admin_role_ids".into(), json!(old.discord_tournament_admin_role_ids));
    old_values.insert("discord_mod_role_ids".into(), json!(old.discord_mod_role_ids));
    old_values.insert("scrim_guild_id".into(), json!(old.scrim_guild_id));
    old_values.insert("scrim_signup_role_id".into(), json!(old.scrim_signup_role_id));
    old_values.insert("scrim_reserve_role_id".into(), json!(old.scrim_reserve_role_id));
    old_values.insert("scrim_announce_channel_id".into(), json!(old.scrim_announce_channel_id));
    old_values.insert("scrim_substitute_sweep_interval_seconds".into(), json!(old.scrim_substitute_sweep_interval_seconds));
    old_values.insert("avatar_dir".into(), json!(old.avatar_dir));
    old_values.insert("steam_bridge_db_path".into(), json!(old.steam_bridge_db_path));
    old_values.insert("backend_host".into(), json!(old.backend_host));
    old_values.insert("backend_port".into(), json!(old.backend_port));
    old_values.insert("backend_allowed_hosts".into(), json!(old.backend_allowed_hosts));
    old_values.insert("expose_api_docs".into(), json!(old.expose_api_docs));
    old_values.insert("frontend_url".into(), json!(old.frontend_url));
    old_values.insert("steam_bot_base_url".into(), json!(old.steam_bot_base_url));
    old_values.insert("observer_enabled".into(), json!(old.observer_enabled));
    old_values.insert("observer_game_control_enabled".into(), json!(old.observer_game_control_enabled));
    old_values.insert("observer_steam_bot2_base_url".into(), json!(old.observer_steam_bot2_base_url));
    old_values.insert("observer_deadlock_api_base_url".into(), json!(old.observer_deadlock_api_base_url));
    old_values.insert("observer_controller_query".into(), json!(old.observer_controller_query));
    old_values.insert("observer_pawn_query".into(), json!(old.observer_pawn_query));
    old_values.insert("routine_tournaments_enabled".into(), json!(old.routine_tournaments_enabled));
    old_values.insert("routine_proposal_channel_id".into(), json!(old.routine_proposal_channel_id));
    old_values.insert("routine_tournament_preset_id".into(), json!(old.routine_tournament_preset_id));
    old_values.insert("routine_tournament_weekday".into(), json!(old.routine_tournament_weekday));
    old_values.insert("routine_tournament_time_utc".into(), json!(old.routine_tournament_time_utc));
    old_values.insert("routine_tournament_lead_days".into(), json!(old.routine_tournament_lead_days));
    old_values.insert("routine_tournament_checkin_lead_minutes".into(), json!(old.routine_tournament_checkin_lead_minutes));
    old_values.insert("routine_tournament_bracket_delay_minutes".into(), json!(old.routine_tournament_bracket_delay_minutes));
    let mut mismatches = Vec::new();
    for (key, value) in &old_values {
        let equivalent = if key == "avatar_dir" || key == "steam_bridge_db_path" {
            let canonical = |value: &serde_json::Value, old: bool| {
                let path = Path::new(value.as_str().unwrap_or_default());
                let path = if path.is_absolute() { path.to_path_buf() }
                    else if old { Path::new("/home/nathanael/repos/Deadlock-Turniere/backend").join(path) }
                    else { path.to_path_buf() };
                path.canonicalize().unwrap_or(path)
            };
            canonical(value, true) == canonical(&new[key], false)
        } else { value == &new[key] };
        if !equivalent { mismatches.push(key.clone()); }
    }
    let test_mode = std::env::var("TURNIER_ENABLE_TEST_MODE")
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(true);
    let logging_matches = std::env::var("RUST_LOG").map(|level| level.trim() == candidate.logging.level.as_str()).unwrap_or(candidate.logging.level.as_str() == "info");
    println!("{}", serde_json::to_string_pretty(&json!({
        "audit": "turnier-legacy-config-comparison-v1",
        "baseline_sha": "c9fad4346cd0d86fce542dcb2da6f2dc0cadf047",
        "nonsecret_fields_checked": old_values.len(),
        "mismatched_fields": mismatches,
        "test_mode_matches": test_mode == candidate.turnier_enable_test_mode,
        "logging_matches": logging_matches,
        "candidate_fingerprint": candidate.fingerprint()?,
        "database_connections": 0,
        "tournament_actions": 0,
        "secret_values_emitted": 0
    }))?);
    Ok(())
}
