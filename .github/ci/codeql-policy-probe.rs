//! Negative controls for the exact jq filter used by the mandatory CodeQL step.
use std::{
    error::Error,
    io::Write,
    process::{Command, ExitCode, Stdio},
};

fn check(name: &str, report: &str, expected_success: bool) -> Result<(), Box<dyn Error>> {
    let mut child = Command::new("jq")
        .args(["-e", include_str!("codeql-policy.jq")])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("jq stdin missing")?
        .write_all(report.as_bytes())?;
    let status = child.wait()?;
    if status.success() != expected_success {
        return Err(format!("{name}: unexpected CodeQL policy status {status}").into());
    }
    println!(
        "{name}: correctly {}",
        if expected_success {
            "accepted"
        } else {
            "blocked"
        }
    );
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let clean = r#"{"version":"2.1.0","runs":[{"tool":{"driver":{"rules":[{"id":"js/test","properties":{"security-severity":"6.9"}}]}},"results":[],"invocations":[{"executionSuccessful":true}]}]}"#;
    let medium = clean.replace("\"results\":[]", "\"results\":[{\"ruleId\":\"js/test\"}]");
    check("complete clean report", clean, true)?;
    check("documented below-HIGH finding", &medium, true)?;
    for (name, report) in [
        ("HIGH finding", medium.replace("6.9", "7.0")),
        ("CRITICAL finding", medium.replace("6.9", "9.8")),
        ("unknown severity", medium.replace("6.9", "unknown")),
        (
            "missing severity",
            medium.replace("\"security-severity\":\"6.9\"", "\"unrelated\":true"),
        ),
        (
            "unresolved rule",
            medium.replace("\"ruleId\":\"js/test\"", "\"ruleId\":\"unknown\""),
        ),
        (
            "missing rule identifier",
            medium.replace("\"ruleId\":\"js/test\"", "\"message\":\"unknown\""),
        ),
        (
            "missing results",
            clean.replace("\"results\":[]", "\"unrelated\":[]"),
        ),
        (
            "failed scanner invocation",
            clean.replace(
                "\"executionSuccessful\":true",
                "\"executionSuccessful\":false",
            ),
        ),
        ("wrong SARIF version", clean.replace("2.1.0", "0.0.0")),
        (
            "empty run inventory",
            "{\"version\":\"2.1.0\",\"runs\":[]}".into(),
        ),
        ("empty report", String::new()),
        ("null report", "null".into()),
        ("malformed JSON", "{".into()),
    ] {
        check(name, &report, false)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
