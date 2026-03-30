from __future__ import annotations

import sys
import unittest
from pathlib import Path

BACKEND_DIR = Path(__file__).resolve().parents[1]
if str(BACKEND_DIR) not in sys.path:
    sys.path.insert(0, str(BACKEND_DIR))

from config import Settings, settings
from main import app
from starlette.middleware.trustedhost import TrustedHostMiddleware


class SettingsSecurityTests(unittest.TestCase):
    def test_allowed_hosts_cover_local_and_frontend_hosts(self) -> None:
        configured = Settings()

        self.assertIn("turnier.earlysalty.com", configured.allowed_hosts)
        self.assertIn("localhost", configured.allowed_hosts)
        self.assertIn("127.0.0.1", configured.allowed_hosts)
        self.assertIn("::1", configured.allowed_hosts)

    def test_allowed_hosts_merge_custom_hosts(self) -> None:
        configured = Settings()
        configured.FRONTEND_URL = "https://frontend.example.org/"
        configured.DISCORD_REDIRECT_URI = "https://auth.example.org/callback"
        configured.BACKEND_HOST = "127.0.0.1"
        configured.BACKEND_ALLOWED_HOSTS = "api.example.org,*.example.net"

        self.assertIn("frontend.example.org", configured.allowed_hosts)
        self.assertIn("auth.example.org", configured.allowed_hosts)
        self.assertIn("api.example.org", configured.allowed_hosts)
        self.assertIn("*.example.net", configured.allowed_hosts)

    def test_cors_allowed_origins_follow_frontend_url(self) -> None:
        configured = Settings()
        configured.FRONTEND_URL = "https://frontend.example.org/"

        self.assertEqual(
            configured.cors_allowed_origins,
            ["http://localhost:5173", "https://frontend.example.org"],
        )


class AppSecurityTests(unittest.TestCase):
    def test_trusted_host_middleware_is_enabled(self) -> None:
        middleware = [entry for entry in app.user_middleware if entry.cls is TrustedHostMiddleware]

        self.assertEqual(len(middleware), 1)
        self.assertEqual(set(middleware[0].kwargs["allowed_hosts"]), set(settings.allowed_hosts))


if __name__ == "__main__":
    unittest.main()
