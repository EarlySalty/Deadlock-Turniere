#!/usr/bin/env bash
# Zentrale TOML zuerst prüfen. Die einzige verbleibende ENV-Datei gehört zur
# geschützten Infisical-Bootstrap-Anbindung, nicht zu den Betriebseinstellungen.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
BIN="$ROOT_DIR/rust/target/release/turnier-bot"
DEPLOY_PREFLIGHT="$HOME/Documents/Admin-Scripts/deploy-preflight.sh"
INFISICAL_CONFIG_FILE="$HOME/.config/deadlock-bots/infisical.conf"
INFISICAL_LOADER="$HOME/.local/bin/dl-infisical-env"

if [[ "$#" -lt 2 || "$1" != "--config" || "$2" != /* ]]; then
  echo "Aufruf: run_turniere_backend_rust.sh --config /absoluter/pfad/config/bot.toml [--check|--check-config|--print-config]" >&2
  exit 64
fi
CONFIG_FILE="$2"
shift 2
MODE="${1:-}"
if [[ "$#" -gt 1 ]]; then
  echo "Ungültige Startargumente." >&2
  exit 64
fi
case "$MODE" in
  ""|--check) ;;
  --check-config|--print-config)
    exec "$BIN" --config "$CONFIG_FILE" "$MODE"
    ;;
  *) echo "Ungültiger Prüfmodus." >&2; exit 64 ;;
esac

# Vor Infisical, Dateisystemänderungen, Clients oder Schedulern validieren.
"$BIN" --config "$CONFIG_FILE" --check-config

# Bestehende Infisical-Bootstrap-Anbindung unverändert weiterverwenden.
# Betriebseinstellungen aus turniere.env/bots.env werden nicht mehr gesourct.
if [[ -f "$INFISICAL_CONFIG_FILE" ]]; then
  set -a
  source "$INFISICAL_CONFIG_FILE"
  set +a
fi
if [[ -n "${CREDENTIALS_DIRECTORY:-}" && -f "$CREDENTIALS_DIRECTORY/infisical-token" ]]; then
  INFISICAL_SERVICE_TOKEN="$(<"$CREDENTIALS_DIRECTORY/infisical-token")"
  export INFISICAL_SERVICE_TOKEN
fi
if [[ "${DL_INFISICAL_READY:-0}" != "1" ]]; then
  if [[ -z "${INFISICAL_SERVICE_TOKEN:-}" || ! -x "$INFISICAL_LOADER" ]]; then
    echo "Geschützte Infisical-Anbindung ist nicht bereit." >&2
    exit 1
  fi
  export DL_INFISICAL_READY=1
  if [[ -n "$MODE" ]]; then
    exec "$INFISICAL_LOADER" --profile all -- "$0" --config "$CONFIG_FILE" "$MODE"
  fi
  exec "$INFISICAL_LOADER" --profile all -- "$0" --config "$CONFIG_FILE"
fi
unset DL_INFISICAL_READY
unset INFISICAL_SERVICE_TOKEN

_dp_parent_comm="$(ps -o comm= -p "$PPID" 2>/dev/null || true)"
if [[ "$_dp_parent_comm" == "systemd" ]]; then
  if [[ ! -x "$DEPLOY_PREFLIGHT" ]]; then
    echo "Deploy-Preflight fehlt unter systemd; Start abgebrochen." >&2
    exit 1
  fi
  DEPLOY_PREFLIGHT_SYSTEMD_PARENT=1 "$DEPLOY_PREFLIGHT" "$ROOT_DIR" main "deadlock-turniere"
elif [[ -x "$DEPLOY_PREFLIGHT" ]]; then
  "$DEPLOY_PREFLIGHT" "$ROOT_DIR" main "deadlock-turniere"
fi

if [[ -z "${DEADLOCK_CENTRAL_DSN:-}" ]]; then
  echo "DEADLOCK_CENTRAL_DSN fehlt in der Secret-Anbindung." >&2
  exit 1
fi

# Vorhandener expliziter One-shot-Migrationsmarker. Normale Restarts migrieren
# nicht. Keine neue Turnier-/Anmeldungs-/Benachrichtigungsaktion beim Start.
CENTRAL_MIGRATION_MARKER="${XDG_RUNTIME_DIR:-/tmp}/deadlock-turniere-apply-central-migrations-once"
CENTRAL_MIGRATOR_BIN="$(dirname "$ROOT_DIR")/Deadlock-Bots/rust/target/release/dl-central-migrate"
if [[ -f "$CENTRAL_MIGRATION_MARKER" ]]; then
  if [[ ! -x "$CENTRAL_MIGRATOR_BIN" ]]; then
    echo "Zentraler Migrator fehlt oder ist nicht ausführbar." >&2
    exit 1
  fi
  "$CENTRAL_MIGRATOR_BIN"
  rm -f "$CENTRAL_MIGRATION_MARKER"
fi

# Datenpfade stammen aus der validierten TOML, unabhängig vom Arbeitsverzeichnis.
cd "$ROOT_DIR"
if [[ -n "$MODE" ]]; then
  exec "$BIN" --config "$CONFIG_FILE" "$MODE"
fi
exec "$BIN" --config "$CONFIG_FILE"
