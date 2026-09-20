#!/usr/bin/env bash
# Isolierte Vertragsprüfung des Startskripts. Keine Dienste oder Secrets laden.
# Der Binary-Stub prüft nur die Argumentweitergabe. Die echte TOML-Prüfung wird
# separat durch die Rust-CLI-Tests belegt.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
TEST_ROOT="$TMP/repo"
mkdir -p "$TEST_ROOT/scripts" "$TEST_ROOT/rust/target/release" "$TEST_ROOT/config"
cp "$ROOT/scripts/run_turniere_backend_rust.sh" "$TEST_ROOT/scripts/"
LAUNCHER="$TEST_ROOT/scripts/run_turniere_backend_rust.sh"
CONFIG="$TEST_ROOT/config/bot.toml"
printf 'synthetic-fixture\n' > "$CONFIG"

cat > "$TEST_ROOT/rust/target/release/turnier-bot" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)"
printf '%s\n' "$@" >> "$ROOT/calls"
[[ "$#" == 3 && "$1" == --config && "$2" == /* ]] || exit 91
if [[ -f "$ROOT/reject-config" ]]; then exit 78; fi
case "$3" in
  --check-config) printf 'config-check-ok\n' ;;
  --print-config) printf '{"schema_version":1}\n' ;;
  *) exit 92 ;;
esac
STUB
chmod +x "$TEST_ROOT/rust/target/release/turnier-bot"

# Ein etwaiger Bootstrap-Zugriff wäre hier sichtbar, ohne echte Secrets.
mkdir -p "$TMP/home/.config/deadlock-bots"
printf 'touch "$HOME/bootstrap-touched"\n' > "$TMP/home/.config/deadlock-bots/infisical.conf"
run() { HOME="$TMP/home" bash "$LAUNCHER" "$@"; }
expect_status() {
  local expected="$1" actual=0
  shift
  run "$@" > "$TMP/output" 2>&1 || actual=$?
  if [[ "$actual" != "$expected" ]]; then
    printf 'Fehlgeschlagen: erwarteter Exit %s, tatsächlicher Exit %s\n' "$expected" "$actual" >&2
    exit 1
  fi
}

expect_status 64
expect_status 64 --config relative.toml
expect_status 64 --config "$CONFIG" --reload
expect_status 64 --config "$CONFIG" --check --print-config
[[ ! -e "$TEST_ROOT/calls" ]]

expect_status 0 --config "$CONFIG" --check-config
printf '%s\n' --config "$CONFIG" --check-config > "$TMP/expected"
cmp -s "$TEST_ROOT/calls" "$TMP/expected"
[[ ! -e "$TMP/home/bootstrap-touched" ]]

rm "$TEST_ROOT/calls"
expect_status 0 --config "$CONFIG" --print-config
printf '%s\n' --config "$CONFIG" --print-config > "$TMP/expected"
cmp -s "$TEST_ROOT/calls" "$TMP/expected"
[[ ! -e "$TMP/home/bootstrap-touched" ]]

# Auch der normale Start bricht bei ungültiger Config vor dem Bootstrap ab.
rm "$TEST_ROOT/calls"
touch "$TEST_ROOT/reject-config"
expect_status 78 --config "$CONFIG"
printf '%s\n' --config "$CONFIG" --check-config > "$TMP/expected"
cmp -s "$TEST_ROOT/calls" "$TMP/expected"
[[ ! -e "$TMP/home/bootstrap-touched" ]]

bash -n "$ROOT/scripts/run_turniere_backend_rust.sh"
bash -n "$ROOT/scripts/test_config_launcher.sh"
printf 'Startskript-Vertrag: 7 Fälle bestanden; keine Dienst- oder Secret-Zugriffe.\n'
