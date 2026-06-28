#!/usr/bin/env bash
# Rust-Cutover-Launcher: spiegelt run_turniere_backend.sh (gleiche Infisical-
# Secret-Ladung, gleiche Env, gleicher cwd = backend/), startet aber das Rust-
# Binary turnier-bot (ex tb-app) statt uvicorn. Enthaelt selbst KEINE Secrets.
# Rollback: systemd-Drop-in 30-rust-cutover.conf entfernen, daemon-reload, restart.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_FILE="${TURNIERE_CONFIG_FILE:-$HOME/.config/deadlock-turniere/turniere.env}"
INFISICAL_CONFIG_FILE="${INFISICAL_CONFIG_FILE:-$HOME/.config/deadlock-bots/infisical.conf}"

if [[ ! -f "$CONFIG_FILE" ]]; then
  echo "Missing Turniere config: $CONFIG_FILE" >&2
  exit 1
fi

set -a
source "$CONFIG_FILE"
set +a

if [[ -f "$INFISICAL_CONFIG_FILE" ]]; then
  set -a
  source "$INFISICAL_CONFIG_FILE"
  set +a
fi

if [[ -n "${CREDENTIALS_DIRECTORY:-}" && -f "$CREDENTIALS_DIRECTORY/infisical-token" ]]; then
  INFISICAL_SERVICE_TOKEN="$(<"$CREDENTIALS_DIRECTORY/infisical-token")"
  export INFISICAL_SERVICE_TOKEN
fi

if [[ -z "${INFISICAL_SERVICE_TOKEN:-}" ]]; then
  echo "INFISICAL_SERVICE_TOKEN nicht gesetzt — weder in $INFISICAL_CONFIG_FILE noch via systemd-creds." >&2
  exit 1
fi

# Python nur fuer den Infisical-Export-Helfer (kein Laufzeit-Python mehr danach).
if [[ -x "$ROOT_DIR/.venv/bin/python" ]]; then
  PYTHON_BIN="${PYTHON_BIN:-$ROOT_DIR/.venv/bin/python}"
else
  PYTHON_BIN="${PYTHON_BIN:-python3}"
fi

INFISICAL_RETRY_DELAY="${INFISICAL_RETRY_DELAY:-5}"
INFISICAL_MAX_ATTEMPTS="${INFISICAL_MAX_ATTEMPTS:-0}"
attempt=0

while true; do
  if INFISICAL_EXPORT="$("$PYTHON_BIN" /home/naniadm/Documents/Deadlock-Bots/scripts/export_infisical_env.py --format shell)"; then
    eval "$INFISICAL_EXPORT"
    break
  fi

  attempt=$((attempt + 1))
  if [[ "$INFISICAL_MAX_ATTEMPTS" -gt 0 && "$attempt" -ge "$INFISICAL_MAX_ATTEMPTS" ]]; then
    echo "Infisical secrets could not be loaded after $attempt attempt(s)." >&2
    exit 1
  fi

  echo "Infisical not ready for Turniere Backend (Rust), retrying in ${INFISICAL_RETRY_DELAY}s (attempt $attempt)." >&2
  sleep "$INFISICAL_RETRY_DELAY"
done

export DISCORD_BOT_TOKEN="${DISCORD_BOT_TOKEN:-${DISCORD_TOKEN:-}}"

# cwd = backend/, damit der DATABASE_PATH-/AVATAR_DIR-Default (data/...) wie bei
# Python auf backend/data/* zeigt (geteilte DB).
cd "$ROOT_DIR/backend"
exec "$ROOT_DIR/rust/target/release/turnier-bot"
