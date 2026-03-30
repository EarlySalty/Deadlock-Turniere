"""Deadlock Tournament Platform configuration.

Supports multiple secret sources so the backend can run on Windows, Linux,
systemd credentials, or files rendered by Vault Agent.
"""
from __future__ import annotations

import logging
import os
import secrets
from pathlib import Path
from urllib.parse import urlsplit

log = logging.getLogger(__name__)


def _read_file(path: str | Path) -> str | None:
    """Read a UTF-8 text file and strip surrounding whitespace."""
    try:
        value = Path(path).expanduser().read_text(encoding="utf-8").strip()
        return value or None
    except FileNotFoundError:
        return None
    except Exception as exc:
        log.warning("Secret file could not be read from %s: %s", path, exc)
        return None


def _get_keyring_value(service: str, key: str) -> str | None:
    """Read a value from the local keyring backend if available."""
    try:
        import keyring

        value = keyring.get_password(service, key)
        return value or None
    except Exception as exc:
        log.debug("Keyring unavailable for %s/%s: %s", service, key, exc)
        return None


def _credential_dir_candidates() -> list[Path]:
    """Directories that may contain runtime-mounted secret files."""
    candidates: list[Path] = []
    for env_name in ("CREDENTIALS_DIRECTORY", "SECRETS_DIRECTORY", "VAULT_SECRETS_DIR"):
        raw = os.getenv(env_name, "").strip()
        if raw:
            candidates.append(Path(raw).expanduser())
    return candidates


def _secret_name_candidates(name: str) -> list[str]:
    return [
        name,
        name.lower(),
        name.lower().replace("_", "-"),
    ]


def _get_file_backed_value(name: str) -> str | None:
    """Resolve a secret from NAME_FILE or well-known credentials directories."""
    explicit_file = os.getenv(f"{name}_FILE", "").strip()
    if explicit_file:
        return _read_file(explicit_file)

    for directory in _credential_dir_candidates():
        for candidate_name in _secret_name_candidates(name):
            value = _read_file(directory / candidate_name)
            if value is not None:
                return value

    return None


def _get_string(
    name: str,
    *,
    default: str = "",
    keyring_service: str | None = None,
    keyring_key: str | None = None,
) -> str:
    """Resolve a string setting from secure sources first, then plain env."""
    file_value = _get_file_backed_value(name)
    if file_value is not None:
        return file_value

    env_value = os.getenv(name, "").strip()
    if env_value:
        return env_value

    if keyring_service and keyring_key:
        keyring_value = _get_keyring_value(keyring_service, keyring_key)
        if keyring_value is not None:
            return keyring_value

    return default


def _get_first_string(
    names: list[str],
    *,
    default: str = "",
    keyring_service: str | None = None,
    keyring_keys: list[str] | None = None,
) -> str:
    """Resolve the first non-empty value from multiple aliases."""
    for name in names:
        file_value = _get_file_backed_value(name)
        if file_value is not None:
            return file_value

        env_value = os.getenv(name, "").strip()
        if env_value:
            return env_value

    if keyring_service and keyring_keys:
        for keyring_key in keyring_keys:
            keyring_value = _get_keyring_value(keyring_service, keyring_key)
            if keyring_value is not None:
                return keyring_value

    return default


def _get_int(name: str, *, default: int) -> int:
    raw = _get_string(name, default="")
    if not raw:
        return default
    try:
        return int(raw)
    except ValueError:
        log.warning("Invalid integer for %s=%r, using default %s", name, raw, default)
        return default


def _get_hostname(value: str) -> str | None:
    """Extract a normalized hostname from a URL or host string."""
    candidate = value.strip()
    if not candidate:
        return None

    if "://" in candidate:
        parsed = urlsplit(candidate)
        hostname = parsed.hostname
    else:
        hostname = candidate
        if hostname.startswith("[") and hostname.endswith("]"):
            hostname = hostname[1:-1]
        if ":" in hostname and hostname.count(":") == 1:
            hostname = hostname.split(":", 1)[0]

    if not hostname:
        return None

    return hostname.strip().lower() or None


def _ensure_jwt_secret() -> str:
    """Resolve a stable JWT secret or create one in keyring on Windows/dev."""
    configured = _get_string(
        "JWT_SECRET",
        keyring_service="DeadlockTurniere",
        keyring_key="JWT_SECRET",
    )
    if configured:
        return configured

    try:
        import keyring

        new_secret = secrets.token_hex(32)
        keyring.set_password("DeadlockTurniere", "JWT_SECRET", new_secret)
        log.info("Generated a new JWT secret and stored it in keyring")
        return new_secret
    except Exception:
        log.warning("No persistent JWT secret configured; using ephemeral fallback")
        return secrets.token_hex(32)


class Settings:
    """Central application settings with Linux-friendly secret loading."""

    # --- Discord OAuth ---
    DISCORD_CLIENT_ID: str = _get_string(
        "DISCORD_CLIENT_ID",
        keyring_service="DeadlockBot",
        keyring_key="DISCORD_OAUTH_CLIENT_ID",
    )
    DISCORD_CLIENT_SECRET: str = _get_string(
        "DISCORD_CLIENT_SECRET",
        keyring_service="DeadlockBot",
        keyring_key="DISCORD_OAUTH_CLIENT_SECRET",
    )
    DISCORD_BOT_TOKEN: str = _get_first_string(
        ["DISCORD_BOT_TOKEN", "DISCORD_TOKEN", "BOT_TOKEN"],
        keyring_service="DeadlockBot",
        keyring_keys=["DISCORD_BOT_TOKEN", "DISCORD_TOKEN", "BOT_TOKEN"],
    )
    DISCORD_REDIRECT_URI: str = _get_string(
        "DISCORD_REDIRECT_URI",
        default="https://turnier.earlysalty.com/auth/discord/callback",
    )

    # --- Discord guild and roles ---
    DISCORD_GUILD_ID: str = _get_string(
        "DISCORD_GUILD_ID",
        default="1289721245281292288",
    )
    DISCORD_ADMIN_ROLE_IDS: str = _get_string(
        "DISCORD_ADMIN_ROLE_IDS",
        default="1304169657124782100,1337518124647579661,1411000883155832852,1401891955931222110",
    )
    DISCORD_MOD_ROLE_IDS: str = _get_string(
        "DISCORD_MOD_ROLE_IDS",
        default="1474210107255554331",
    )

    # --- JWT ---
    JWT_SECRET: str = _ensure_jwt_secret()

    # --- Databases ---
    DATABASE_PATH: str = _get_string(
        "DATABASE_PATH",
        default="data/tournament.db",
    )
    STEAM_BRIDGE_DB_PATH: str = _get_string(
        "STEAM_BRIDGE_DB_PATH",
        default=r"C:\Users\Nani-Admin\Documents\Deadlock\service\deadlock.sqlite3",
    )

    # --- Server ---
    BACKEND_HOST: str = _get_string("BACKEND_HOST", default="127.0.0.1")
    BACKEND_PORT: int = _get_int("BACKEND_PORT", default=8900)
    BACKEND_ALLOWED_HOSTS: str = _get_string("BACKEND_ALLOWED_HOSTS", default="")
    FRONTEND_URL: str = _get_string(
        "FRONTEND_URL",
        default="https://turnier.earlysalty.com",
    )

    # --- Notifications ---
    DISCORD_WEBHOOK_URL: str = _get_string("DISCORD_WEBHOOK_URL", default="")

    @property
    def jwt_secret_key(self) -> str:
        return self.JWT_SECRET

    @property
    def admin_role_ids(self) -> set[str]:
        if not self.DISCORD_ADMIN_ROLE_IDS:
            return set()
        return {role.strip() for role in self.DISCORD_ADMIN_ROLE_IDS.split(",") if role.strip()}

    @property
    def mod_role_ids(self) -> set[str]:
        if not self.DISCORD_MOD_ROLE_IDS:
            return set()
        return {role.strip() for role in self.DISCORD_MOD_ROLE_IDS.split(",") if role.strip()}

    @property
    def cors_allowed_origins(self) -> list[str]:
        origins = {
            "http://localhost:5173",
        }
        frontend_url = self.FRONTEND_URL.rstrip("/")
        if frontend_url:
            origins.add(frontend_url)
        return sorted(origins)

    @property
    def allowed_hosts(self) -> list[str]:
        hosts = {"127.0.0.1", "localhost", "::1"}

        for candidate in (
            self.FRONTEND_URL,
            self.DISCORD_REDIRECT_URI,
            self.BACKEND_HOST,
        ):
            hostname = _get_hostname(candidate)
            if hostname:
                hosts.add(hostname)

        extra_hosts = [
            host.strip().lower()
            for host in self.BACKEND_ALLOWED_HOSTS.split(",")
            if host.strip()
        ]
        hosts.update(extra_hosts)

        return sorted(hosts)


settings = Settings()

if settings.DISCORD_CLIENT_ID:
    log.info("Discord OAuth client id loaded: %s...", settings.DISCORD_CLIENT_ID[:6])
else:
    log.warning("DISCORD_CLIENT_ID is not configured")
