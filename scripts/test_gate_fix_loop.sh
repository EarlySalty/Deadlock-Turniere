#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tmp_dir=$(mktemp -d)
trap 'rm -rf -- "$tmp_dir"' EXIT

mkdir -p "$tmp_dir/repo/rust" "$tmp_dir/log"
printf '%s\n' '#!/usr/bin/env bash' 'exit 97' >"$tmp_dir/test-db"
chmod +x "$tmp_dir/test-db"

cargo() { return 0; }
say() { :; }

REPO="$tmp_dir/repo"
TEST_DB="$tmp_dir/test-db"
LOG_DIR="$tmp_dir/log"
KNOWN_RED="invalid_proposal_transition_returns_conflict"
source <(awk '/^verify\(\)/,/^}/' "$repo_root/scripts/gate_fix_loop.sh")

set +e
verify
status=$?
set -e

if [[ $status -ne 1 ]]; then
  printf 'expected verify exit 1 for test runner exit 97, got %s\n' "$status" >&2
  exit 1
fi
