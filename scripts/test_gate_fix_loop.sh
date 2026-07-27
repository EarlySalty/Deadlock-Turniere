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

fake_bin="$tmp_dir/fake-bin"
git_log="$tmp_dir/git.log"
mkdir -p "$fake_bin"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'printf "%s\n" "$*" >>"$GIT_LOG"' \
  'case "$1 $2 $3" in' \
  '  "rev-parse --verify origin/main") exit 0 ;;' \
  '  "rev-parse HEAD ") printf "deadbeef\n"; exit 0 ;;' \
  '  "status --porcelain ")' \
  '    [[ -n "${FMT_DIRTY_MARKER:-}" && -e "$FMT_DIRTY_MARKER" ]] && printf " M formatted-file\n"' \
  '    printf "%b" "${GIT_STATUS_OUTPUT:-}"; exit 0 ;;' \
  '  "log --oneline -1") printf "deadbee Testcommit\n"; exit 0 ;;' \
  'esac' \
  'exit 0' >"$fake_bin/git"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'printf "ALLOW: Testfreigabe\n"' >"$fake_bin/python3"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  '[[ -n "${FMT_DIRTY_MARKER:-}" ]] && : >"$FMT_DIRTY_MARKER"' \
  'exit 0' >"$fake_bin/cargo"
chmod +x "$fake_bin/git" "$fake_bin/python3" "$fake_bin/cargo"

mkdir -p "$tmp_dir/allow-repo/rust"
GIT_LOG="$git_log" PATH="$fake_bin:$PATH" REPO="$tmp_dir/allow-repo" \
  LOG_DIR="$tmp_dir/allow-log" MAX_ROUNDS=1 \
  bash "$repo_root/scripts/gate_fix_loop.sh"

# Der Loop soll nach einem ALLOW selbst pushen -- das ist sein Zweck. Er darf dabei
# nur den aktuellen HEAD rausschieben, nie den lokalen main-Zeiger.
if ! grep -Eq '^push origin deadbeef:main$' "$git_log"; then
  printf 'gate loop must push the verified commit after ALLOW\n' >&2
  cat "$git_log" >&2
  exit 1
fi
if grep -Eq '^(add|commit)( |$)' "$git_log"; then
  printf 'gate loop must not create commits of its own on the ALLOW path\n' >&2
  cat "$git_log" >&2
  exit 1
fi

: >"$git_log"
fmt_dirty_marker="$tmp_dir/fmt-dirty"
set +e
GIT_LOG="$git_log" FMT_DIRTY_MARKER="$fmt_dirty_marker" PATH="$fake_bin:$PATH" \
  REPO="$tmp_dir/allow-repo" LOG_DIR="$tmp_dir/fmt-dirty-log" MAX_ROUNDS=1 \
  bash "$repo_root/scripts/gate_fix_loop.sh" >"$tmp_dir/fmt-dirty-output.log" 2>&1
status=$?
set -e

if [[ $status -ne 6 ]]; then
  printf 'expected verification-mutated repository exit 6, got %s\n' "$status" >&2
  cat "$tmp_dir/fmt-dirty-output.log" >&2
  exit 1
fi
if grep -Eq '^push ' "$git_log"; then
  printf 'verification-mutated repository must not be pushed\n' >&2
  cat "$git_log" >&2
  exit 1
fi

: >"$git_log"
dirty_output="$tmp_dir/dirty-output.log"
set +e
GIT_LOG="$git_log" GIT_STATUS_OUTPUT=' M tracked-file\n' PATH="$fake_bin:$PATH" \
  REPO="$tmp_dir/allow-repo" LOG_DIR="$tmp_dir/dirty-log" MAX_ROUNDS=1 \
  bash "$repo_root/scripts/gate_fix_loop.sh" >"$dirty_output" 2>&1
status=$?
set -e

if [[ $status -ne 3 ]]; then
  printf 'expected dirty repository exit 3, got %s\n' "$status" >&2
  exit 1
fi
if ! grep -q 'Arbeitsbaum ist nicht sauber' "$dirty_output"; then
  printf 'dirty repository must report why it aborted\n' >&2
  cat "$dirty_output" >&2
  exit 1
fi
if grep -Eq '^(fetch|rev-parse|add|commit|push)( |$)' "$git_log"; then
  printf 'dirty repository must abort before critic or mutation\n' >&2
  cat "$git_log" >&2
  exit 1
fi
