from __future__ import annotations

import sys
import unittest
from pathlib import Path

from fastapi.testclient import TestClient

BACKEND_DIR = Path(__file__).resolve().parents[1]
if str(BACKEND_DIR) not in sys.path:
    sys.path.insert(0, str(BACKEND_DIR))

from config import Settings, settings
from main import app
from starlette.middleware.trustedhost import TrustedHostMiddleware


class SettingsSecurityTests(unittest.TestCase):
    def test_discord_oauth_delegates_to_deadlock_bots_by_default(self) -> None:
        configured = Settings()

        self.assertEqual(
            configured.DISCORD_OAUTH_INTERNAL_API_BASE_URL,
            "http://127.0.0.1:8766",
        )

    def test_allowed_hosts_cover_local_and_frontend_hosts(self) -> None:
        configured = Settings()

        self.assertIn("turnier.deutsche-deadlock-community.de", configured.allowed_hosts)
        self.assertIn("localhost", configured.allowed_hosts)
        self.assertIn("127.0.0.1", configured.allowed_hosts)
        self.assertIn("::1", configured.allowed_hosts)

    def test_allowed_hosts_merge_custom_hosts(self) -> None:
        configured = Settings()
        configured.FRONTEND_URL = "https://frontend.example.org/"
        configured.TURNIER_PUBLIC_URL = "https://auth.example.org/turnier"
        configured.BACKEND_HOST = "127.0.0.1"
        configured.BACKEND_ALLOWED_HOSTS = "api.example.org,*.example.net"

        self.assertIn("frontend.example.org", configured.allowed_hosts)
        self.assertIn("auth.example.org", configured.allowed_hosts)
        self.assertIn("api.example.org", configured.allowed_hosts)
        self.assertIn("*.example.net", configured.allowed_hosts)

    def test_tournament_admin_role_is_always_allowed(self) -> None:
        configured = Settings()
        configured.DISCORD_ADMIN_ROLE_IDS = "111,222"
        configured.DISCORD_TOURNAMENT_ADMIN_ROLE_IDS = "1494120177577754747"

        self.assertEqual(
            configured.admin_role_ids,
            {"111", "222", "1494120177577754747"},
        )

    def test_cors_allowed_origins_follow_frontend_url(self) -> None:
        configured = Settings()
        configured.FRONTEND_URL = "https://frontend.example.org/"

        self.assertEqual(
            configured.cors_allowed_origins,
            ["http://localhost:5173", "https://frontend.example.org"],
        )

    def test_api_docs_are_disabled_by_default(self) -> None:
        configured = Settings()

        self.assertFalse(configured.EXPOSE_API_DOCS)
        self.assertIsNone(configured.docs_url)
        self.assertIsNone(configured.redoc_url)
        self.assertIsNone(configured.openapi_url)

    def test_api_docs_can_be_enabled_explicitly(self) -> None:
        configured = Settings()
        configured.EXPOSE_API_DOCS = True

        self.assertEqual(configured.docs_url, "/docs")
        self.assertEqual(configured.redoc_url, "/redoc")
        self.assertEqual(configured.openapi_url, "/openapi.json")


class AppSecurityTests(unittest.TestCase):
    def test_trusted_host_middleware_is_enabled(self) -> None:
        middleware = [entry for entry in app.user_middleware if entry.cls is TrustedHostMiddleware]

        self.assertEqual(len(middleware), 1)
        self.assertEqual(set(middleware[0].kwargs["allowed_hosts"]), set(settings.allowed_hosts))

    def test_invalid_host_header_is_rejected(self) -> None:
        client = TestClient(app)

        response = client.get("/api/health")

        self.assertEqual(response.status_code, 400)
        self.assertEqual(response.text, "Invalid host header")

    def test_common_probe_paths_return_not_found_without_sensitive_content(self) -> None:
        client = TestClient(app, base_url="https://turnier.deutsche-deadlock-community.de")
        probe_paths = (
            "/wp-login.php",
            "/xmlrpc.php",
            "/.env",
            "/.git/config",
            "/etc/passwd?raw??",
            "/%40fs/etc/passwd?import&raw??",
            "/_ignition/execute-solution",
        )

        for path in probe_paths:
            with self.subTest(path=path):
                response = client.get(path)
                body = response.text.lower()

                self.assertEqual(response.status_code, 404)
                self.assertNotIn("root:x:", body)
                self.assertNotIn("discord_client_secret", body)
                self.assertNotIn("jwt_secret", body)
                self.assertNotIn("traceback", body)
                self.assertNotIn("[core]", body)

    def test_api_docs_endpoints_are_not_public_by_default(self) -> None:
        client = TestClient(app, base_url="https://turnier.deutsche-deadlock-community.de")

        for path in ("/docs", "/redoc", "/openapi.json"):
            with self.subTest(path=path):
                response = client.get(path)
                self.assertEqual(response.status_code, 404)


if __name__ == "__main__":
    unittest.main()
