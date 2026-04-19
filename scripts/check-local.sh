#!/usr/bin/env bash
# Lokale Code-Qualitaet und Security-Pruefung
# Spiegelt lint-and-typecheck.yml + security.yml (Python-Teil)
# Voraussetzung: pip install ruff mypy bandit pytest pytest-cov

set -euo pipefail

cd "$(dirname "$0")/.."

fail=0

run_check() {
  local name="$1"
  shift
  echo
  echo "=== $name ==="
  if "$@"; then
    echo "OK: $name"
  else
    echo "FEHLGESCHLAGEN: $name"
    fail=1
  fi
}

run_check "Ruff Lint" \
  ruff check backend/ --target-version=py311

run_check "Ruff Format" \
  ruff format backend/ --check --target-version=py311

run_check "mypy Type Check" \
  mypy backend/ --ignore-missing-imports --no-error-summary --show-column-numbers

run_check "Bandit Security SAST" \
  bandit -r backend -ll -ii

run_check "pytest" \
  pytest -q backend/tests/ --cov=backend --cov-branch --cov-fail-under=40

echo
if [ "$fail" -eq 0 ]; then
  echo "Alle lokalen Checks bestanden."
else
  echo "Einige Checks sind fehlgeschlagen."
  exit 1
fi
