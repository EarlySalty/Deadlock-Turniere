//! The same fail-closed decision is used in the Required PR Gate and its tests.
use std::{env, process::ExitCode};

const REQUIRED: &[&str] = &[
    "FRONTEND",
    "RUST",
    "RUST_DEPS",
    "GITLEAKS",
    "NPM",
    "SEMGREP",
    "CODEQL",
    "TRIVY",
    "WORKFLOWS",
];

fn validate(results: &[Option<String>], cancelled: Option<&str>) -> Result<(), String> {
    if cancelled != Some("false") {
        return Err("Workflow cancellation state is absent or not false".into());
    }
    if results.len() != REQUIRED.len() {
        return Err("Mandatory result inventory is incomplete".into());
    }
    for (name, result) in REQUIRED.iter().zip(results) {
        if result.as_deref() != Some("success") {
            return Err(format!("{name}: expected success, got {result:?}"));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let results: Vec<_> = REQUIRED
        .iter()
        .map(|name| env::var(format!("{name}_RESULT")).ok())
        .collect();
    for (name, result) in REQUIRED.iter().zip(&results) {
        println!("{name}: {}", result.as_deref().unwrap_or("MISSING"));
    }
    match validate(&results, env::var("WORKFLOW_CANCELLED").ok().as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("Required PR Gate: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn green() -> Vec<Option<String>> {
        REQUIRED.iter().map(|_| Some("success".into())).collect()
    }

    #[test]
    fn accepts_only_complete_success() {
        assert!(validate(&green(), Some("false")).is_ok());
    }

    #[test]
    fn every_failure_cancel_skip_and_missing_result_blocks() {
        let statuses = [
            None,
            Some(""),
            Some("failure"),
            Some("cancelled"),
            Some("skipped"),
            Some("neutral"),
            Some("timed_out"),
            Some("action_required"),
            Some("Success"),
        ];
        for index in 0..REQUIRED.len() {
            for status in statuses {
                let mut results = green();
                results[index] = status.map(String::from);
                assert!(
                    validate(&results, Some("false")).is_err(),
                    "{}: {status:?}",
                    REQUIRED[index]
                );
            }
        }
    }

    #[test]
    fn empty_truncated_and_cancelled_workflows_block() {
        assert!(validate(&[], Some("false")).is_err());
        assert!(validate(&green()[1..], Some("false")).is_err());
        for cancellation in [None, Some(""), Some("true")] {
            assert!(validate(&green(), cancellation).is_err());
        }
    }
}
