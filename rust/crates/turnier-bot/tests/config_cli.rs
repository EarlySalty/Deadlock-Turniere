use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use turnier_config::{Config, ConfigError, CONFIG_ANCHOR};

const VALID: &str = include_str!("../../../../config/bot.example.toml");
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
    config: PathBuf,
}
impl Fixture {
    fn new(text: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "turnier-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::create_dir_all(root.join("unrelated-cwd")).unwrap();
        let config = root.join("config/bot.toml");
        std::fs::write(&config, text).unwrap();
        Self { root, config }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_turnier-bot"));
        command
            .env_clear()
            .current_dir(self.root.join("unrelated-cwd"))
            .arg("--config")
            .arg(&self.config);
        command
    }
    fn run(&self, mode: &str) -> Output {
        self.command().arg(mode).output().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn real_binary_checks_without_secrets_database_scheduler_or_directory_creation() {
    let fixture = Fixture::new(VALID);
    let result = fixture.run("--check-config");
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains(CONFIG_ANCHOR));
    assert!(result.stderr.is_empty());
    assert!(!fixture.root.join("backend").exists());
    assert_eq!(std::fs::read_to_string(&fixture.config).unwrap(), VALID);
}
#[test]
fn real_binary_prints_effective_file_values_despite_legacy_environment() {
    let fixture = Fixture::new(VALID);
    let legacy = fixture.root.join("legacy-value");
    std::fs::write(&legacy, "1").unwrap();
    let result = fixture
        .command()
        .arg("--print-config")
        .env("BACKEND_PORT", "1")
        .env("BACKEND_PORT_FILE", &legacy)
        .env("TURNIER_ENABLE_TEST_MODE", "0")
        .env("RUST_LOG", "trace")
        .env("ROUTINE_TOURNAMENTS_ENABLED", "0")
        .output()
        .unwrap();
    assert!(result.status.success());
    let actual: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let expected: serde_json::Value = serde_json::from_str(
        &Config::load_file(&fixture.config)
            .unwrap()
            .safe_status()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);
    for key in [
        "discord_bot_token",
        "discord_webhook_url",
        "jwt_secret",
        "observer_agent_token",
    ] {
        assert!(actual.get(key).is_none());
    }
    assert!(!fixture.root.join("backend").exists());
}
#[test]
fn real_binary_rejects_invalid_config_before_live_check_initialization() {
    let fixture = Fixture::new("unrecognized = \"INPUT_MUST_NOT_BE_ECHOED\"");
    for mode in [
        "--check-config",
        "--print-config",
        "--check",
        "--check-broker",
    ] {
        let result = fixture.run(mode);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains(&ConfigError::Parse.to_string()));
        assert!(!error.contains("INPUT_MUST_NOT_BE_ECHOED"));
        assert!(!fixture.root.join("backend").exists());
    }
}
#[test]
fn real_binary_does_not_create_missing_config_and_has_no_reload_mode() {
    let fixture = Fixture::new(VALID);
    assert!(!fixture.run("--reload").status.success());
    std::fs::remove_file(&fixture.config).unwrap();
    assert!(!fixture.run("--check-config").status.success());
    assert!(!fixture.config.exists());
}
