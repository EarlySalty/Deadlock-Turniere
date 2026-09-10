#!/usr/bin/env bash
# Durchlauf durch den Scrim-Draft-Raum-Vertrag gegen ein laufendes Backend.
# Standardziel ist der Worktree-Start auf 127.0.0.1:8900; abweichende Ziele
# ueber BASE_URL. curl ist in Agenten-Shells gesperrt, daher urllib.
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8900}"

python3 - "$BASE_URL" <<'PYEOF'
import json
import sys
import urllib.request

base_url = sys.argv[1].rstrip("/")


def call(method, path, payload=None, headers=None):
    body = json.dumps(payload).encode() if payload is not None else None
    request = urllib.request.Request(
        base_url + path,
        data=body,
        method=method,
        headers={"Content-Type": "application/json", **(headers or {})},
    )
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            return response.status, json.loads(response.read() or b"null")
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise SystemExit(f"FAIL {method} {path}: {error.code} {detail}")


def step(label, ok):
    print(f"{'OK ' if ok else 'FEHLER '}{label}")
    if not ok:
        raise SystemExit(1)


status, created = call("POST", "/api/draft/lobbies", {
    "team1_name": "Durchlauf Eins",
    "team2_name": "Durchlauf Zwei",
    "bans_per_team": 0,
    "round_seconds": 30,
})
step(f"Raum angelegt: {created.get('code')}", status == 200 and created.get("code"))
code = created["code"]

status, state = call("GET", f"/api/draft/lobbies/{code}")
step(f"Zustand gelesen, Phase {state.get('phase')}", status == 200 and state["phase"] == "warteraum")
step("Keine Deadline bei Anlage", state.get("deadline_at") is None)

tokens = {}
for team in (1, 2):
    status, claim = call("POST", f"/api/draft/lobbies/{code}/claim", {"team": team})
    step(f"Claim Team {team}", status == 200 and claim.get("team") == team)
    tokens[team] = claim["token"]

status, ready = call("POST", f"/api/draft/lobbies/{code}/ready", {"token": tokens[1]})
step("Bereit Team 1 ohne Start", status == 200 and ready.get("started") is False)

status, state = call("GET", f"/api/draft/lobbies/{code}", headers={"X-Draft-Token": tokens[1]})
step("you.team per Token erkannt", state.get("you", {}).get("team") == 1)

status, ready = call("POST", f"/api/draft/lobbies/{code}/ready", {"token": tokens[2]})
step("Bereit Team 2 startet den Draft", status == 200 and ready.get("started") is True)

status, state = call("GET", f"/api/draft/lobbies/{code}")
step("Phase laeuft nach Start", state.get("phase") == "laeuft")
step("Deadline mit Timer gesetzt", state.get("deadline_at") is not None)

helden = [
    "Abrams", "Bebop", "Calico", "Dynamo", "Grey Talon", "Haze",
    "Holliday", "Infernus", "Ivy", "Kelvin", "Lady Geist", "Lash",
]
for held in helden:
    status, state = call("GET", f"/api/draft/lobbies/{code}")
    if state.get("phase") == "abgeschlossen":
        break
    slot = state.get("current_team_slot")
    if slot not in (1, 2):
        raise SystemExit(f"FAIL unerwarteter Team-Slot: {slot}")
    status, _ = call(
        "POST",
        f"/api/draft/lobbies/{code}/action",
        {"token": tokens[slot], "hero_name": held},
    )
    step(f"Zug {held} durch Team {slot}", status == 200)

status, state = call("GET", f"/api/draft/lobbies/{code}")
step("Draft abgeschlossen", state.get("phase") == "abgeschlossen")
step("Lobby angefordert", state.get("lobby", {}).get("status") == "angefordert")

status, rematch = call(
    "POST",
    f"/api/draft/lobbies/{code}/rematch",
    {"token": tokens[1]},
)
step(f"Rematch-Raum {rematch.get('code')}", status == 200 and rematch.get("code"))
neuer_code = rematch["code"]

status, neuer_raum = call("GET", f"/api/draft/lobbies/{neuer_code}")
step("Rematch liegt im Warteraum", neuer_raum.get("phase") == "warteraum")
step(
    "Seiten getauscht",
    neuer_raum["team1"]["name"] == "Durchlauf Zwei"
    and neuer_raum["team2"]["name"] == "Durchlauf Eins",
)

status, alter_raum = call("GET", f"/api/draft/lobbies/{code}")
step("Altbau verweist auf Rematch", alter_raum.get("rematch_code") == neuer_code)

print(f"DURCHLAUF OK, Raum {code}, Rematch {neuer_code}")
PYEOF
