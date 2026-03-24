from __future__ import annotations

import secrets
from pathlib import Path

from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    """Zentrale Konfiguration — alle Werte kommen aus Environment-Variablen."""

    # --- Datenbank ---
    DATABASE_PATH: str = "data/tournament.db"

    # --- Discord OAuth ---
    DISCORD_CLIENT_ID: str = ""
    DISCORD_CLIENT_SECRET: str = ""
    DISCORD_REDIRECT_URI: str = "https://turnier.earlysalty.com/auth/discord/callback"

    # --- Discord Guild / Rollen ---
    DISCORD_GUILD_ID: str = ""
    DISCORD_ADMIN_ROLE_IDS: str = ""  # Komma-separiert
    DISCORD_MOD_ROLE_IDS: str = ""    # Komma-separiert

    # --- JWT ---
    JWT_SECRET: str = ""

    # --- Steam Bridge (read-only Zugriff auf Discord Bot DB) ---
    STEAM_BRIDGE_DB_PATH: str = ""

    # --- Server ---
    BACKEND_PORT: int = 8900
    FRONTEND_URL: str = "https://turnier.earlysalty.com"

    # --- Notifications ---
    DISCORD_WEBHOOK_URL: str = ""

    model_config = {"env_file": ".env", "env_file_encoding": "utf-8"}

    # --- Helfer ---

    @property
    def jwt_secret_key(self) -> str:
        """Gibt JWT_SECRET zurueck oder generiert einen zufaelligen Key."""
        if self.JWT_SECRET:
            return self.JWT_SECRET
        return secrets.token_hex(32)

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
