#!/usr/bin/env bash
# Rust-Cutover-Launcher: spiegelt run_turniere_backend.sh (gleiche Infisical-
# Secret-Ladung, gleiche Env, gleicher cwd = backend/), startet aber das Rust-
# Binary turnier-bot (ex tb-app) statt uvicorn. Enthaelt selbst KEINE Secrets.
# Rollback: systemd-Drop-in 30-rust-cutover.conf entfernen, daemon-reload, restart.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEPLOY_PREFLIGHT="${DEPLOY_PREFLIGHT:-$HOME/Documents/Admin-Scripts/deploy-preflight.sh}"
CONFIG_FILE="${TURNIERE_CONFIG_FILE:-$HOME/.config/deadlock-turniere/turniere.env}"
INFISICAL_CONFIG_FILE="${INFISICAL_CONFIG_FILE:-$HOME/.config/deadlock-bots/infisical.conf}"
INFISICAL_LOADER="${INFISICAL_LOADER:-/home/naniadm/.local/bin/dl-infisical-env}"

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

if [[ "${DL_INFISICAL_READY:-0}" != "1" ]]; then
  if [[ ! -x "$INFISICAL_LOADER" ]]; then
    echo "Infisical loader nicht gefunden oder nicht ausführbar: $INFISICAL_LOADER" >&2
    exit 1
  fi
  export DL_INFISICAL_READY=1
  exec "$INFISICAL_LOADER" --profile all -- "$0" "$@"
fi
unset DL_INFISICAL_READY
unset INFISICAL_SERVICE_TOKEN

_dp_parent_comm="$(ps -o comm= -p "$PPID" 2>/dev/null || true)"
if [[ "$_dp_parent_comm" == "systemd" ]]; then
  if [[ ! -x "$DEPLOY_PREFLIGHT" ]]; then
    echo "FEHLER: deploy-preflight fehlt unter systemd-Start, breche ab: $DEPLOY_PREFLIGHT" >&2
    exit 1
  fi
  DEPLOY_PREFLIGHT_SYSTEMD_PARENT=1 "$DEPLOY_PREFLIGHT" "$ROOT_DIR" main "deadlock-turniere"
elif [[ -x "$DEPLOY_PREFLIGHT" ]]; then
  "$DEPLOY_PREFLIGHT" "$ROOT_DIR" main "deadlock-turniere"
fi

export DISCORD_BOT_TOKEN="${DISCORD_BOT_TOKEN:-${DISCORD_TOKEN:-}}"

if [[ -z "${DEADLOCK_CENTRAL_DSN:-}" ]]; then
  echo "DEADLOCK_CENTRAL_DSN nicht gesetzt — zentrale Turnier-DB ist Pflicht fuer Rust." >&2
  exit 1
fi

# cwd = backend/, damit der AVATAR_DIR-Default (data/avatars) wie bei Python auf
# backend/data/avatars zeigt. DATABASE_PATH ist Rust-seitig Legacy/ignoriert; die
# Turnier-Fachdaten kommen aus DEADLOCK_CENTRAL_DSN.
cd "$ROOT_DIR/backend"
exec "$ROOT_DIR/rust/target/release/turnier-bot"
