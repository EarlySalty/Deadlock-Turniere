from __future__ import annotations

import asyncio
import tempfile
import unittest
from pathlib import Path
import sys

from fastapi.testclient import TestClient

BACKEND_DIR = Path(__file__).resolve().parents[1]
if str(BACKEND_DIR) not in sys.path:
    sys.path.insert(0, str(BACKEND_DIR))

from auth.permissions import require_auth
from config import settings
from db import get_db, init_db
from main import app
from tournament.models import UserSession


class ProfileRouteTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)

        self.original_db_path = settings.DATABASE_PATH
        self.original_avatar_dir = settings.AVATAR_DIR
        self.original_dependency_overrides = dict(app.dependency_overrides)
        settings.DATABASE_PATH = str(Path(self.tempdir.name) / "tournament.db")
        settings.AVATAR_DIR = str(Path(self.tempdir.name) / "avatars")

        async def _seed_db() -> None:
            await init_db()

        asyncio.run(_seed_db())

        async def _auth_override() -> UserSession:
            return UserSession(
                discord_id="123456789012345678",
                discord_name="OriginalName",
                discord_avatar="https://cdn.discordapp.com/avatars/123/abc.png",
            )

        app.dependency_overrides[require_auth] = _auth_override
        self.client = TestClient(app)

    def tearDown(self) -> None:
        settings.DATABASE_PATH = self.original_db_path
        settings.AVATAR_DIR = self.original_avatar_dir
        app.dependency_overrides = self.original_dependency_overrides
        self.client.close()

    def test_profile_update_persists_display_name_and_notifications(self) -> None:
        response = self.client.put(
            "/api/profile",
            json={
                "display_name": "Nani Admin",
                "bio": "Bio",
                "notify_match_start": False,
                "notify_checkin": True,
                "notify_team_invite": False,
                "notify_tournament_news": True,
            },
        )

        self.assertEqual(response.status_code, 200)
        payload = response.json()
        self.assertEqual(payload["display_name"], "Nani Admin")
        self.assertFalse(payload["notify_match_start"])
        self.assertTrue(payload["notify_checkin"])
        self.assertFalse(payload["notify_team_invite"])
        self.assertTrue(payload["notify_tournament_news"])

        get_response = self.client.get("/api/profile")
        self.assertEqual(get_response.status_code, 200)
        self.assertEqual(get_response.json()["display_name"], "Nani Admin")

    def test_avatar_upload_and_public_fetch_work(self) -> None:
        png_bytes = (
            b"\x89PNG\r\n\x1a\n"
            b"\x00\x00\x00\rIHDR"
            b"\x00\x00\x00\x01\x00\x00\x00\x01"
            b"\x08\x02\x00\x00\x00"
            b"\x90wS\xde"
            b"\x00\x00\x00\x0cIDAT"
            b"\x08\xd7c\xf8\xff\xff?\x00\x05\xfe\x02\xfeA\x8c\x8d\xcb"
            b"\x00\x00\x00\x00IEND\xaeB`\x82"
        )

        response = self.client.post(
            "/api/profile/avatar",
            files={"avatar": ("avatar.png", png_bytes, "image/png")},
        )

        self.assertEqual(response.status_code, 200)
        payload = response.json()
        self.assertEqual(payload["avatar_filename"], "123456789012345678.png")

        avatar_path = Path(settings.AVATAR_DIR) / payload["avatar_filename"]
        self.assertTrue(avatar_path.exists())

        fetch_response = self.client.get("/api/avatars/123456789012345678")
        self.assertEqual(fetch_response.status_code, 200)
        self.assertEqual(fetch_response.headers["content-type"], "image/png")
        self.assertTrue(fetch_response.content.startswith(b"\x89PNG\r\n\x1a\n"))

    def test_avatar_route_redirects_to_discord_cdn_if_no_file_exists(self) -> None:
        async def _seed_session() -> None:
            async with get_db() as db:
                await db.execute(
                    "INSERT INTO sessions (token, discord_id, discord_name, discord_avatar, discord_roles, expires_at) "
                    "VALUES (?, ?, ?, ?, ?, datetime('now', '+1 day'))",
                    (
                        "token",
                        "123456789012345678",
                        "OriginalName",
                        "https://cdn.discordapp.com/avatars/123/abc.png",
                        "",
                    ),
                )
                await db.commit()

        asyncio.run(_seed_session())

        response = self.client.get("/api/avatars/123456789012345678", follow_redirects=False)
        self.assertEqual(response.status_code, 302)
        self.assertEqual(response.headers["location"], "https://cdn.discordapp.com/avatars/123/abc.png")


if __name__ == "__main__":
    unittest.main()
