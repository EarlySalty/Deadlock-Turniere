//! Exercise actual scanner exit codes using synthetic files, never production targets.
use std::{
    env,
    error::Error,
    fs,
    path::PathBuf,
    process::{Command, ExitCode},
};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn expect(command: &mut Command, expected: i32) -> Result<(), Box<dyn Error>> {
    let output = command.output()?;
    let actual = output.status.code();
    if actual != Some(expected) {
        eprintln!(
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(format!("expected scanner exit {expected}, got {actual:?}").into());
    }
    println!("Confirmed scanner exit {expected}");
    Ok(())
}
fn run() -> Result<(), Box<dyn Error>> {
    let scanner = env::args().nth(1).ok_or("scanner name required")?;
    if !["semgrep", "gitleaks", "trivy"].contains(&scanner.as_str()) {
        return Err("unknown scanner".into());
    }
    let dir = env::temp_dir().join(format!("turniere-{scanner}-probe-{}", std::process::id()));
    fs::create_dir(&dir)?;
    let scratch = Scratch(dir);
    let root = &scratch.0;
    let code = root.join("input");
    fs::create_dir(&code)?;
    match scanner.as_str() {
        "semgrep" => {
            let config = root.join("rules.yml");
            fs::write(&config, "rules:\n  - id: synthetic-eval\n    languages: [javascript]\n    severity: ERROR\n    message: Synthetic negative-control finding\n    pattern: eval($X)\n")?;
            let source = code.join("probe.js");
            let command = || {
                let mut c = Command::new("semgrep");
                c.args([
                    "scan",
                    "--error",
                    "--strict",
                    "--metrics=off",
                    "--disable-version-check",
                    "--no-git-ignore",
                    "--config",
                ])
                .arg(&config)
                .arg(&code);
                c
            };
            fs::write(&source, "const value = 42;\n")?;
            expect(&mut command(), 0)?;
            fs::write(&source, "eval(input);\n")?;
            expect(&mut command(), 1)?;
            fs::write(&config, "rules: [\n")?;
            expect(&mut command(), 7)?;
        }
        "gitleaks" => {
            let source = code.join("settings.txt");
            let command = || {
                let mut c = Command::new("gitleaks");
                c.args(["dir", "--redact", "--no-banner", "--exit-code", "23"])
                    .arg(&code);
                c
            };
            fs::write(&source, "theme = blue\n")?;
            expect(&mut command(), 0)?;
            // Generated only in temporary storage; never a working credential.
            fs::write(
                &source,
                format!(
                    "github_token = \"{}{}\"\n",
                    "ghp_", "a8Z3kP9rQ2mN7vB4xC6dF1gH5jL0sT8wY9uI"
                ),
            )?;
            expect(&mut command(), 23)?;
            let mut invalid = command();
            invalid.arg("--config").arg(root.join("missing.toml"));
            expect(&mut invalid, 1)?;
        }
        "trivy" => {
            let source = code.join("Dockerfile");
            let command = || {
                let mut c = Command::new("trivy");
                c.args([
                    "config",
                    "--severity",
                    "HIGH,CRITICAL",
                    "--exit-code",
                    "23",
                    "--format",
                    "json",
                ])
                .arg(&code);
                c
            };
            fs::write(&source, "FROM scratch\nUSER 65532\n")?;
            expect(&mut command(), 0)?;
            fs::write(&source, "FROM scratch\nUSER root\n")?;
            expect(&mut command(), 23)?;
            expect(
                Command::new("trivy")
                    .args(["config", "--config"])
                    .arg(root.join("missing.yaml"))
                    .arg(&code),
                1,
            )?;
        }
        _ => unreachable!(),
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Scanner counterprobe failed: {error}");
            ExitCode::FAILURE
        }
    }
}
