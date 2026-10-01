//! Controls for the exact jq filter and invocation used by the CodeQL job.
use std::{
    error::Error,
    io::Write,
    process::{Command, ExitCode, Stdio},
};

fn check(name: &str, report: &str, expected_success: bool) -> Result<(), Box<dyn Error>> {
    let mut child = Command::new("jq")
        .args(["--slurp", "--exit-status", include_str!("codeql-policy.jq")])
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

fn report(tool: &str, results: &str, invocations: &str) -> String {
    format!(
        r#"{{"version":"2.1.0","runs":[{{"tool":{tool},"results":{results},"invocations":{invocations}}}]}}"#
    )
}

fn run() -> Result<(), Box<dyn Error>> {
    let driver = r#"{"driver":{"name":"CodeQL","rules":[{"id":"js/test","properties":{"security-severity":"6.9"}}]}}"#;
    let extensions = r#"{"driver":{"name":"CodeQL","rules":[]},"extensions":[{"name":"codeql/javascript-queries","rules":[{"id":"js/test","properties":{"security-severity":"6.9"}}]}]}"#;
    let invocation = r#"[{"executionSuccessful":true}]"#;
    let finding = r#"[{"ruleId":"js/test"}]"#;
    let clean = report(driver, "[]", invocation);
    let medium = report(driver, finding, invocation);
    let extension_clean = report(extensions, "[]", invocation);
    let extension_medium = report(extensions, finding, invocation);
    check("complete clean driver report", &clean, true)?;
    check("documented below-HIGH driver finding", &medium, true)?;
    check("complete clean extension report", &extension_clean, true)?;
    check(
        "documented below-HIGH extension finding",
        &extension_medium,
        true,
    )?;
    check(
        "numeric below-HIGH severity",
        &medium.replace("\"6.9\"", "6.9"),
        true,
    )?;
    check(
        "non-error extraction notification",
        &clean.replace(
            "\"executionSuccessful\":true",
            "\"executionSuccessful\":true,\"toolExecutionNotifications\":[{\"level\":\"none\"}]",
        ),
        true,
    )?;
    for (name, invalid) in [
        ("HIGH driver finding", medium.replace("6.9", "7.0")),
        ("CRITICAL driver finding", medium.replace("6.9", "9.8")),
        ("HIGH extension finding", extension_medium.replace("6.9", "7.0")),
        ("CRITICAL extension finding", extension_medium.replace("6.9", "9.8")),
        ("numeric HIGH severity", medium.replace("\"6.9\"", "7")),
        ("unknown severity", medium.replace("6.9", "unknown")),
        ("NaN severity", medium.replace("6.9", "NaN")),
        ("negative severity", medium.replace("6.9", "-1")),
        ("null severity", medium.replace("\"6.9\"", "null")),
        ("missing severity", medium.replace("\"security-severity\":\"6.9\"", "\"unrelated\":true")),
        ("unresolved rule", medium.replace("\"ruleId\":\"js/test\"", "\"ruleId\":\"unknown\"")),
        ("missing rule identifier", medium.replace("\"ruleId\":\"js/test\"", "\"message\":\"unknown\"")),
        ("missing results", clean.replace("\"results\":[]", "\"unrelated\":[]")),
        ("null results", clean.replace("\"results\":[]", "\"results\":null")),
        ("empty rule inventory", report(r#"{"driver":{"name":"CodeQL","rules":[]}}"#, "[]", invocation)),
        ("malformed extensions", clean.replace("\"driver\":", "\"extensions\":false,\"driver\":")),
        ("ambiguous rules", extension_medium.replace("\"rules\":[]", "\"rules\":[{\"id\":\"js/test\",\"properties\":{\"security-severity\":\"1.0\"}}]")),
        ("wrong scanner", clean.replace("CodeQL", "unknown")),
        ("failed scanner invocation", clean.replace("\"executionSuccessful\":true", "\"executionSuccessful\":false")),
        ("missing scanner invocations", clean.replace("\"invocations\":", "\"unrelated\":")),
        ("empty scanner invocations", report(driver, "[]", "[]")),
        ("execution error notification", clean.replace("\"executionSuccessful\":true", "\"executionSuccessful\":true,\"toolExecutionNotifications\":[{\"level\":\"error\"}]")),
        ("configuration error notification", clean.replace("\"executionSuccessful\":true", "\"executionSuccessful\":true,\"toolConfigurationNotifications\":[{\"level\":\"error\"}]")),
        ("wrong SARIF version", clean.replace("2.1.0", "0.0.0")),
        ("empty run inventory", r#"{"version":"2.1.0","runs":[]}"#.into()),
        ("empty report", String::new()),
        ("null report", "null".into()),
        ("malformed JSON", "{".into()),
        ("failed document followed by clean document", format!("null\n{clean}")),
        ("multiple clean JSON documents", format!("{clean}\n{clean}")),
    ] {
        check(name, &invalid, false)?;
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
