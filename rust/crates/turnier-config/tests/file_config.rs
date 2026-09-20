use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use turnier_config::{Config, ConfigArgs, ConfigError, ConfigMode};

const VALID: &str = include_str!("../../../../config/bot.toml");
const BASE: &str = "/srv/turniere/config/bot.toml";
static COUNTER: AtomicU64 = AtomicU64::new(0);

fn parse(text: &str) -> Result<Config, ConfigError> {
    Config::parse_file(text, Path::new(BASE))
}
fn changed(old: &str, new: &str) -> String {
    assert!(VALID.contains(old), "Testanker fehlt");
    VALID.replacen(old, new, 1)
}
fn temp() -> PathBuf {
    std::env::temp_dir().join(format!(
        "turnier-config-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn valid_config_keeps_operational_features_and_resolves_paths() {
    let config = parse(VALID).unwrap();
    assert!(config.routine_tournaments_enabled);
    assert_eq!(config.routine_tournament_preset_id, 1);
    assert_eq!(config.routine_tournament_weekday, "sunday");
    assert_eq!(
        config.discord_master_broker_base_url,
        "http://127.0.0.1:8770"
    );
    assert_eq!(config.avatar_dir, "/srv/turniere/backend/data/avatars");
    assert_eq!(config.network.broker_request_seconds, 20);
}

#[test]
fn rejects_unknown_fields_at_root_and_in_nested_sections() {
    for text in [
        format!("unexpected = true\n{VALID}"),
        VALID.replace("[network]", "[network]\nunexpected = true"),
        VALID.replace("[scheduler]", "[scheduler]\nunexpected = true"),
    ] {
        assert!(matches!(parse(&text), Err(ConfigError::Parse)));
    }
}

#[test]
fn rejects_missing_required_values_and_wrong_types() {
    for text in [
        changed("schema_version = 1", ""),
        changed("backend_port = 8900", ""),
        changed("backend_port = 8900", "backend_port = \"8900\""),
        changed(
            "routine_tournaments_enabled = true",
            "routine_tournaments_enabled = 1",
        ),
        changed("tick_seconds = 60", "tick_seconds = -1"),
    ] {
        assert!(parse(&text).is_err());
    }
}

#[test]
fn rejects_unsupported_schema() {
    assert!(parse(&changed("schema_version = 1", "schema_version = 2")).is_err());
}

#[test]
fn port_boundaries_are_enforced() {
    for port in [0, -1, 65536] {
        assert!(parse(&changed(
            "backend_port = 8900",
            &format!("backend_port = {port}")
        ))
        .is_err());
    }
    for port in [1, 65535] {
        assert!(parse(&changed(
            "backend_port = 8900",
            &format!("backend_port = {port}")
        ))
        .is_ok());
    }
}

#[test]
fn ids_reject_zero_negative_overflow_and_invalid_csv() {
    for id in ["0", "-1", "9223372036854775808", "abc", "12,,13"] {
        let text = changed(
            "discord_guild_id = \"1289721245281292288\"",
            &format!("discord_guild_id = \"{id}\""),
        );
        assert!(parse(&text).is_err());
    }
    assert!(parse(&changed(
        "scrim_signup_role_id = 1520849762851618817",
        "scrim_signup_role_id = 0"
    ))
    .is_err());
    assert!(parse(&changed(
        "discord_mod_role_ids = \"1474210107255554331\"",
        "discord_mod_role_ids = \"12,,13\""
    ))
    .is_err());
}

#[test]
fn timeout_boundaries_and_connect_budget() {
    for value in [0, 3601] {
        assert!(parse(&changed(
            "broker_request_seconds = 20",
            &format!("broker_request_seconds = {value}")
        ))
        .is_err());
    }
    assert!(parse(&changed(
        "broker_connect_seconds = 5",
        "broker_connect_seconds = 21"
    ))
    .is_err());
    assert!(parse(&changed("tick_seconds = 60", "tick_seconds = 0")).is_err());
    assert!(parse(&changed(
        "observer_evaluate_milliseconds = 350",
        "observer_evaluate_milliseconds = 0"
    ))
    .is_err());
}

#[test]
fn routine_requires_preset_and_valid_utc_time() {
    assert!(parse(&changed(
        "routine_tournament_preset_id = 1",
        "routine_tournament_preset_id = 0"
    ))
    .is_err());
    for time in ["24:00", "18:60", "6:00", "token-sentinel"] {
        assert!(parse(&changed(
            "routine_tournament_time_utc = \"18:00\"",
            &format!("routine_tournament_time_utc = \"{time}\"")
        ))
        .is_err());
    }
}

#[test]
fn parser_and_validation_errors_never_echo_input() {
    let sentinel = "SECRET_SENTINEL_DO_NOT_PRINT";
    let samples = [
        format!("unknown = \"{sentinel}\"\n{VALID}"),
        changed(
            "backend_port = 8900",
            &format!("backend_port = \"{sentinel}\""),
        ),
        changed(
            "backend_host = \"127.0.0.1\"",
            &format!("backend_host = \"{sentinel}\""),
        ),
        format!("{VALID}\nbroken = \"{sentinel}"),
    ];
    for text in samples {
        let err = parse(&text).unwrap_err();
        assert!(!format!("{err:?} {err}").contains(sentinel));
    }
}

#[test]
fn secret_fields_are_rejected_and_never_serialized_or_debugged() {
    let sentinel = "SECRET_SENTINEL_DO_NOT_PRINT";
    for key in [
        "discord_bot_token",
        "discord_webhook_url",
        "jwt_secret",
        "turnier_internal_api_token",
    ] {
        assert!(parse(&format!("{key} = \"{sentinel}\"\n{VALID}")).is_err());
    }
    let mut cfg = parse(VALID).unwrap();
    cfg.discord_bot_token = sentinel.into();
    cfg.discord_webhook_url = sentinel.into();
    cfg.jwt_secret = sentinel.into();
    let status = cfg.safe_status().unwrap();
    assert!(!status.contains(sentinel));
    assert!(!status.contains("discord_bot_token"));
    assert!(!format!("{cfg:?}").contains(sentinel));
}

#[test]
fn rejects_token_bearing_urls() {
    for url in [
        "https://user:secret@example.org",
        "https://example.org?token=secret",
        "https://example.org#secret",
        "file:///etc/passwd",
    ] {
        let text = changed(
            "steam_bot_base_url = \"http://127.0.0.1:8782\"",
            &format!("steam_bot_base_url = \"{url}\""),
        );
        assert!(parse(&text).is_err());
    }
}

#[test]
fn relative_config_path_is_rejected_and_internal_paths_are_cwd_independent() {
    assert_eq!(
        Config::parse_file(VALID, Path::new("config/bot.toml")).unwrap_err(),
        ConfigError::AbsolutePath
    );
    let path = "/srv/independent/config/bot.toml";
    let cfg = Config::parse_file(VALID, Path::new(path)).unwrap();
    assert_eq!(cfg.avatar_dir, "/srv/independent/backend/data/avatars");
}

#[test]
fn missing_file_does_not_create_a_default() {
    let path = temp();
    assert_eq!(Config::load_file(&path).unwrap_err(), ConfigError::Read);
    assert!(!path.exists());
}

#[test]
fn failed_candidate_keeps_the_running_snapshot_and_the_file_unchanged() {
    let dir = temp();
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("bot.toml");
    std::fs::write(&path, VALID).unwrap();
    let active = Arc::new(Config::load_file(&path).unwrap());
    let fingerprint = active.fingerprint().unwrap();
    std::fs::write(&path, "broken = [").unwrap();
    assert!(Config::load_file(&path).is_err());
    assert_eq!(active.fingerprint().unwrap(), fingerprint);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken = [");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cli_requires_explicit_config_and_rejects_unknown_or_duplicate_options() {
    let args = |args: &[&str]| ConfigArgs::parse(args.iter().map(Into::into));
    assert!(args(&[]).is_err());
    assert!(args(&["--config"]).is_err());
    assert!(args(&["--config", "relative"]).is_err());
    assert!(args(&["--config", BASE, "--reload"]).is_err());
    assert!(args(&["--config", BASE, "--check", "--print-config"]).is_err());
    assert_eq!(
        args(&["--config", BASE, "--check-config"]).unwrap().mode,
        ConfigMode::Validate
    );
}

#[test]
fn child_environment_probe() {
    if std::env::var_os("TURNIER_CONFIG_ENV_TEST_CHILD").is_none() {
        return;
    }
    let config = parse(VALID).unwrap();
    assert_eq!(config.backend_port, 8900);
    assert!(config.routine_tournaments_enabled);
    assert_eq!(config.scheduler.tick_seconds, 60);
    assert_eq!(config.discord_guild_id, "1289721245281292288");
    assert!(turnier_config::secrets::resolve_first(&["BACKEND_PORT", "SCRIM_GUILD_ID"]).is_none());
}

#[test]
fn environment_and_dynamic_file_keys_cannot_override_toml() {
    let dir = temp();
    std::fs::create_dir(&dir).unwrap();
    let legacy = dir.join("BACKEND_PORT");
    std::fs::write(&legacy, "1").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_environment_probe", "--nocapture"])
        .current_dir(&dir)
        .env("TURNIER_CONFIG_ENV_TEST_CHILD", "1")
        .env("BACKEND_PORT", "1")
        .env("BACKEND_PORT_FILE", &legacy)
        .env("CREDENTIALS_DIRECTORY", &dir)
        .env("SECRETS_DIRECTORY", &dir)
        .env("VAULT_SECRETS_DIR", &dir)
        .env("DISCORD_GUILD_ID", "7")
        .env("SCRIM_GUILD_ID", "8")
        .env("ROUTINE_TOURNAMENTS_ENABLED", "0")
        .env("TURNIERE_CONFIG_FILE", "missing.env")
        .output()
        .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(
        result.status.success(),
        "isolierter ENV-Test fehlgeschlagen"
    );
}

#[test]
fn runtime_environment_reads_are_limited_to_secrets_and_throwaway_guards() {
    fn walk(path: &Path, sources: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, sources);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                sources.push(path);
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(crates).unwrap() {
        let src = entry.unwrap().path().join("src");
        if src.is_dir() {
            walk(&src, &mut sources);
        }
    }
    assert!(sources.len() > 50);
    for path in sources {
        let text = std::fs::read_to_string(&path).unwrap();
        let runtime = text.split("#[cfg(test)]").next().unwrap();
        assert!(
            !runtime.contains("set_var("),
            "ENV-Brücke: {}",
            path.display()
        );
        assert!(
            !runtime.contains("Config::from_env("),
            "alter Loader: {}",
            path.display()
        );
        let secret_resolver = path.ends_with("turnier-config/src/secrets.rs");
        let throwaway_guard = path.ends_with("turnier-api/src/test_mode.rs");
        for line in runtime.lines().filter(|line| {
            line.contains("env::var")
                || line.contains("getenv(")
                || line.contains("try_from_default_env(")
        }) {
            if secret_resolver {
                continue;
            }
            let approved_guard = throwaway_guard
                && [
                    "\"CENTRAL_TEST_DSN\"",
                    "\"DEADLOCK_CENTRAL_DSN\"",
                    "\"TURNIER_TEST_DB_CONFIRM\"",
                ]
                .iter()
                .any(|key| line.contains(key));
            assert!(
                approved_guard,
                "nicht freigegebener ENV-Leser: {}",
                path.display()
            );
        }
    }
}

#[test]
fn observer_agent_is_optional_but_strict_when_configured() {
    assert!(parse(VALID).unwrap().observer_agent.is_none());
    let agent = "\n[observer_agent]\nserver_base_url = \"http://127.0.0.1:8900\"\nvconsole_address = \"127.0.0.1:29000\"\nbot_account_id = 2\npoll_milliseconds = 250\ngame_control_enabled = false\nrequest_timeout_seconds = 5\n";
    assert!(parse(&format!("{VALID}{agent}")).is_ok());
    assert!(parse(&format!(
        "{VALID}{}",
        agent.replace("bot_account_id = 2", "bot_account_id = 1")
    ))
    .is_err());
    assert!(parse(&format!(
        "{VALID}{}",
        agent.replace("poll_milliseconds = 250", "poll_milliseconds = 149")
    ))
    .is_err());
}
