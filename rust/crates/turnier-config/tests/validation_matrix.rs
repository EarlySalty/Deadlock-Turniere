use std::path::Path;
use turnier_config::{Config, ConfigError, MAX_CONFIG_BYTES};

const VALID: &str = include_str!("../../../../config/bot.example.toml");
const BASE: &str = "/srv/turniere/config/bot.toml";
fn parse(value: &toml::Value) -> Result<Config, ConfigError> {
    Config::parse_file(&toml::to_string(value).unwrap(), Path::new(BASE))
}
fn leaves(value: &toml::Value, prefix: Vec<String>, out: &mut Vec<Vec<String>>) {
    if let Some(table) = value.as_table() {
        for (key, value) in table {
            let mut next = prefix.clone();
            next.push(key.clone());
            leaves(value, next, out);
        }
    } else {
        out.push(prefix);
    }
}
fn parent<'a>(
    value: &'a mut toml::Value,
    path: &[String],
) -> &'a mut toml::map::Map<String, toml::Value> {
    let mut value = value;
    for key in &path[..path.len() - 1] {
        value = value.get_mut(key).unwrap();
    }
    value.as_table_mut().unwrap()
}
fn set(path: &str, new: toml::Value) -> Result<Config, ConfigError> {
    let mut value: toml::Value = toml::from_str(VALID).unwrap();
    let path: Vec<String> = path.split('.').map(str::to_owned).collect();
    *parent(&mut value, &path)
        .get_mut(path.last().unwrap())
        .unwrap() = new;
    parse(&value)
}
#[test]
fn every_required_leaf_rejects_omission_and_every_leaf_rejects_wrong_type() {
    let value: toml::Value = toml::from_str(VALID).unwrap();
    let mut fields = Vec::new();
    leaves(&value, Vec::new(), &mut fields);
    assert!(fields.len() > 100);
    for path in fields {
        let label = path.join(".");
        let mut missing = value.clone();
        let old = parent(&mut missing, &path)
            .remove(path.last().unwrap())
            .unwrap();
        if !matches!(
            label.as_str(),
            "scrim_signup_role_id" | "scrim_reserve_role_id"
        ) {
            assert!(
                parse(&missing).is_err(),
                "fehlender Pflichtwert wurde akzeptiert: {label}"
            );
        }
        let mut wrong = value.clone();
        parent(&mut wrong, &path).insert(
            path.last().unwrap().clone(),
            if old.is_str() {
                toml::Value::Boolean(false)
            } else {
                toml::Value::String("WRONG_TYPE_SENTINEL".into())
            },
        );
        let err = parse(&wrong).unwrap_err();
        assert!(!format!("{err} {err:?}").contains("WRONG_TYPE_SENTINEL"));
    }
}
#[test]
fn every_section_rejects_unknown_keys() {
    let value: toml::Value = toml::from_str(VALID).unwrap();
    for (section, child) in value.as_table().unwrap() {
        if child.is_table() {
            let mut altered = value.clone();
            altered
                .get_mut(section)
                .unwrap()
                .as_table_mut()
                .unwrap()
                .insert("unrecognized".into(), toml::Value::Boolean(true));
            assert!(parse(&altered).is_err(), "{section}");
        }
    }
}
#[test]
fn ids_and_utc_time_must_have_canonical_spelling() {
    for id in ["+1", "01", " 1", "1 ", "1e3", "0"] {
        assert!(set("discord_guild_id", toml::Value::String(id.into())).is_err());
        assert!(set(
            "authorization.proposal_approver_role_ids",
            toml::Value::Array(vec![toml::Value::String(id.into())])
        )
        .is_err());
    }
    for time in ["+1:00", "01:+1", " 1:00", "00:60", "24:00"] {
        assert!(set(
            "routine_tournament_time_utc",
            toml::Value::String(time.into())
        )
        .is_err());
    }
    for time in ["00:00", "23:59"] {
        assert!(set(
            "routine_tournament_time_utc",
            toml::Value::String(time.into())
        )
        .is_ok());
    }
}
#[test]
fn pool_cache_limits_and_intervals_reject_out_of_range_values() {
    for (field, values) in [
        ("database.max_connections", [0, 65]),
        ("database.acquire_timeout_seconds", [0, 301]),
        ("bridge.read_connections", [0, 33]),
        ("bridge.task_connections", [0, 33]),
        ("bridge.poll_milliseconds", [0, 60001]),
        ("bridge.stale_task_milliseconds", [0, 3600001]),
        ("steam.rank_cache_seconds", [0, 604801]),
        ("steam.subrank_role_cache_seconds", [0, 86401]),
        ("assets.heroes_cache_seconds", [0, 604801]),
        ("assets.heroes_fallback_cache_seconds", [0, 86401]),
        ("limits.session_lifetime_days", [0, 31]),
        ("limits.avatar_bytes", [0, 16777217]),
        ("limits.request_body_bytes", [0, 33554433]),
        ("limits.comp_body_bytes", [0, 1048577]),
        ("limits.comp_clients", [0, 100001]),
        ("limits.comp_reads_per_minute", [0, 100001]),
        ("limits.comp_writes_per_minute", [0, 100001]),
        ("limits.comp_creations_per_hour", [0, 1001]),
        ("limits.draft_creations_per_hour", [0, 1001]),
        ("limits.draft_viewer_ttl_seconds", [0, 3601]),
        ("scheduler.reminder_window_minutes", [0, 61]),
        ("scheduler.observer_agent_fresh_seconds", [0, 61]),
        ("scheduler.observer_command_ttl_seconds", [0, 31]),
    ] {
        for value in values {
            assert!(set(field, toml::Value::Integer(value)).is_err(), "{field}");
        }
    }
    for timeout in ["invite", "result", "create", "start", "control", "convars"] {
        assert!(set(
            &format!("bridge.{timeout}_timeout_seconds"),
            toml::Value::Integer(0)
        )
        .is_err());
        assert!(set(
            &format!("bridge.{timeout}_timeout_seconds"),
            toml::Value::Integer(121)
        )
        .is_err());
    }
    assert!(set(
        "scheduler.observer_bootstrap_cooldown_seconds",
        toml::Value::Integer(29)
    )
    .is_err());
    assert!(set("limits.request_body_bytes", toml::Value::Integer(1024)).is_err());
}
#[test]
fn reminder_fallback_and_rank_mapping_are_strict_without_touching_db_rules() {
    for offsets in [
        vec![],
        vec![15, 15],
        vec![15, 120],
        vec![1440, -1],
        vec![525601, 1],
    ] {
        assert!(set(
            "scheduler.default_reminder_offsets_minutes",
            toml::Value::Array(offsets.into_iter().map(toml::Value::Integer).collect())
        )
        .is_err());
    }
    assert!(set(
        "steam.main_rank_role_ids",
        toml::Value::Array(vec![toml::Value::Integer(1); 11])
    )
    .is_err());
    assert!(set(
        "authorization.proposal_approver_role_ids",
        toml::Value::Array(vec![])
    )
    .is_err());
}
#[test]
fn all_configured_service_urls_reject_credentials_and_unusable_addresses() {
    for field in [
        "discord_oauth_internal_api_base_url",
        "discord_master_broker_base_url",
        "turnier_public_url",
        "frontend_url",
        "steam_bot_base_url",
        "observer_steam_bot2_base_url",
        "observer_deadlock_api_base_url",
        "assets.heroes_url",
        "steam.discord_api_base_url",
    ] {
        for value in [
            "https://example.invalid/?credential=sentinel",
            "https://example.invalid/api/webhooks/1/sentinel",
            "http://127.0.0.1:0",
            "ftp://example.invalid/",
        ] {
            assert!(
                set(field, toml::Value::String(value.into())).is_err(),
                "{field}"
            );
        }
    }
}
#[test]
fn oversized_input_is_rejected_without_echoing_it() {
    let text = "x".repeat(MAX_CONFIG_BYTES as usize + 1);
    assert!(matches!(
        Config::parse_file(&text, Path::new(BASE)),
        Err(ConfigError::Invalid(_))
    ));
}
#[cfg(unix)]
#[test]
fn symlink_uses_the_real_file_directory_for_relative_data_paths() {
    let root = std::env::temp_dir().join(format!("turnier-symlink-{}", std::process::id()));
    std::fs::create_dir_all(root.join("real/config")).unwrap();
    std::fs::create_dir_all(root.join("alias")).unwrap();
    let real = root.join("real/config/bot.toml");
    std::fs::write(&real, VALID).unwrap();
    let alias = root.join("alias/bot.toml");
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let config = Config::load_file(&alias).unwrap();
    assert_eq!(
        Path::new(&config.avatar_dir),
        root.join("real/backend/data/avatars")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn observer_director_rejects_nonfinite_scores_and_inconsistent_budgets() {
    for field in [
        "normal_switch_delta",
        "emergency_switch_delta",
        "minimum_interesting_score",
        "player_view_score",
    ] {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 10001.0] {
            assert!(set(
                &format!("observer_director.{field}"),
                toml::Value::Float(value)
            )
            .is_err());
        }
    }
    for field in ["min_hold_milliseconds", "stale_after_milliseconds"] {
        for value in [0, 60001] {
            assert!(set(
                &format!("observer_director.{field}"),
                toml::Value::Integer(value)
            )
            .is_err());
        }
    }
    assert!(set(
        "observer_director.stale_after_milliseconds",
        toml::Value::Integer(349)
    )
    .is_err());
    assert!(set(
        "observer_director.normal_switch_delta",
        toml::Value::Float(26.0)
    )
    .is_err());
    assert!(set(
        "observer_director.minimum_interesting_score",
        toml::Value::Float(59.0)
    )
    .is_err());
    for value in [0, 65537] {
        assert!(set(
            "limits.observer_live_queue_rows",
            toml::Value::Integer(value)
        )
        .is_err());
    }
}
