#!/usr/bin/env python3
"""Abnahmenachweis mit vorhandenem Wegwerf-DB-Harness, ohne Produktivzugangsdaten."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
COMMAND = [
    "/usr/bin/bash",
    "/home/nathanael/Documents/Deadlock-Bots/rust/scripts/central_test_db.sh",
    "/home/nathanael/.cargo/bin/cargo", "test", "--locked",
    "--manifest-path", str(ROOT / "rust/Cargo.toml"),
    "--workspace", "--features", "testing", "--no-fail-fast", "-j", "2",
    "--", "--include-ignored",
]

def snapshot():
    result = {}
    for tree in ["rust/crates", "scripts", "config"]:
        for path in (ROOT / tree).rglob("*"):
            if path.is_file() and path.suffix in {".rs", ".toml", ".sh"} and path.name != "bot.toml":
                result[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    for name in ["rust/Cargo.toml", "rust/Cargo.lock"]:
        result[name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    return result

def main():
    environment = {
        "PATH": "/home/nathanael/.cargo/bin:/usr/local/bin:/usr/bin:/bin",
        "HOME": "/home/nathanael", "LANG": "C.UTF-8", "CARGO_BUILD_JOBS": "2",
        "XDG_RUNTIME_DIR": f"/run/user/{os.getuid()}",
    }
    before = snapshot()
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    print("Fachtests: Wegwerf-Postgres, alle Workspace-Tests inklusive ignorierter DB-Tests.", flush=True)
    log = OUT / "test-final.log"
    with log.open("w") as stream:
        run = subprocess.run(COMMAND, cwd=ROOT, env=environment, stdout=stream, stderr=subprocess.STDOUT)
    text = log.read_text(errors="replace")
    rows = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored", text)
    report = {
        "command": COMMAND, "started_utc": started,
        "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "exit_code": run.returncode,
        "passed": sum(int(row[1]) for row in rows),
        "failed": sum(int(row[2]) for row in rows),
        "ignored": sum(int(row[3]) for row in rows),
        "test_binaries_reported": len(rows),
        "failed_tests": re.findall(r"^test (.+?) \.\.\. FAILED$", text, re.M),
        "source_unchanged_during_tests": before == snapshot(),
        "secret_environment": "cleared before throwaway harness",
        "log": log.name,
    }
    (OUT / "test-final-source-snapshot.json").write_text(json.dumps(before, indent=2) + "\n")
    (OUT / "test-final-summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False, indent=2), flush=True)
    return run.returncode if report["source_unchanged_during_tests"] else 2

if __name__ == "__main__":
    sys.exit(main())
