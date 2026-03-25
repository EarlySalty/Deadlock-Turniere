"""Deadlock Tournament Platform — Konfiguration.

Liest Secrets aus dem Windows Credential Manager (keyring).
Keine .env Datei nötig.
"""
from __future__ import annotations

import logging
import secrets

log = logging.getLogger(__name__)


def _get_keyring_value(service: str, key: str) -> str | None:
    """Liest einen Wert aus dem Windows Credential Manager."""
    try:
        import keyring
        return keyring.get_password(service, key)
    except Exception as e:
        log.warning("Keyring-Fehler für %s/%s: %s", service, key, e)
        return None


def _ensure_jwt_secret() -> str:
    """Holt oder erstellt den JWT Secret im Keyring."""
    try:
        import keyring
        existing = keyring.get_password("DeadlockTurniere", "JWT_SECRET")
        if existing:
            return existing
        new_secret = secrets.token_hex(32)
        keyring.set_password("DeadlockTurniere", "JWT_SECRET", new_secret)
        log.info("Neuen JWT Secret im Keyring gespeichert")
        return new_secret
    except Exception:
        # Fallback: zufälliger Key (geht bei Restart verloren)
        log.warning("Keyring nicht verfügbar, nutze ephemeren JWT Secret")
        return secrets.token_hex(32)


class Settings:
    """Zentrale Konfiguration — Secrets aus Windows Keyring, Rest als Defaults."""

    # --- Discord OAuth (aus DeadlockBot Keyring) ---
    DISCORD_CLIENT_ID: str = _get_keyring_value("DeadlockBot", "DISCORD_OAUTH_CLIENT_ID") or ""
    DISCORD_CLIENT_SECRET: str = _get_keyring_value("DeadlockBot", "DISCORD_OAUTH_CLIENT_SECRET") or ""
    DISCORD_REDIRECT_URI: str = "https://turnier.earlysalty.com/auth/discord/callback"

    # --- Discord Guild & Rollen ---
    DISCORD_GUILD_ID: str = "1289721245281292288"
    DISCORD_ADMIN_ROLE_IDS: str = ",".join([
        "1304169657124782100",
        "1337518124647579661",
        "1411000883155832852",
        "1401891955931222110",
    ])
    DISCORD_MOD_ROLE_IDS: str = "1474210107255554331"

    # --- JWT ---
    JWT_SECRET: str = _ensure_jwt_secret()

    # --- Datenbank ---
    DATABASE_PATH: str = "data/tournament.db"

    # --- Steam Bridge (read-only Zugriff auf Discord Bot DB) ---
    STEAM_BRIDGE_DB_PATH: str = r"C:\Users\Nani-Admin\Documents\Deadlock\service\deadlock.sqlite3"

    # --- Server ---
    BACKEND_PORT: int = 8900
    FRONTEND_URL: str = "https://turnier.earlysalty.com"

    # --- Notifications ---
    DISCORD_WEBHOOK_URL: str = ""

    # --- Helfer ---
    @property
    def jwt_secret_key(self) -> str:
        return self.JWT_SECRET

    @property
    def admin_role_ids(self) -> set[str]:
        if not self.DISCORD_ADMIN_ROLE_IDS:
            return set()
        return {r.strip() for r in self.DISCORD_ADMIN_ROLE_IDS.split(",") if r.strip()}

    @property
    def mod_role_ids(self) -> set[str]:
        if not self.DISCORD_MOD_ROLE_IDS:
            return set()
        return {r.strip() for r in self.DISCORD_MOD_ROLE_IDS.split(",") if r.strip()}


settings = Settings()

# Startup-Log
if settings.DISCORD_CLIENT_ID:
    log.info("Discord OAuth Client ID geladen: %s...", settings.DISCORD_CLIENT_ID[:6])
else:
    log.warning("DISCORD_OAUTH_CLIENT_ID nicht im Keyring gefunden!")
