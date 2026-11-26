# Turnier-Bot – vollständige GitHub-Übergabe
Stand 26.11.2026, 22:11 CET. Diese GitHub-Ablage ist die einzige benötigte Auftragsquelle. Der externe Agent arbeitet in seiner eigenen Umgebung; keine Dateien oder Werkzeuge vom Rechner des Maintainers voraussetzen.

## Einstieg
- [RESTAUFTRAG.md](RESTAUFTRAG.md): vollständige priorisierte To-do-Liste mit Branches und Abnahmekriterien.
- [ARBEITSRAHMEN.md](ARBEITSRAHMEN.md): verbindliche projektspezifische Regeln für externe Contributor.
- [REGISTER.md](REGISTER.md): letzter bekannter Paketstand, Nachweise und Abhängigkeiten, keine Pflicht zum separaten Tracker.
- [RESTPROMPT.md](RESTPROMPT.md): kurzer Gesamtauftrag; [PROMPTS.md](PROMPTS.md): Einzelaufträge.
- [BESTANDSAUFNAHME.md](BESTANDSAUFNAHME.md): historische Rekonstruktion; aktuelle Remote-SHAs und neuere Berichte haben Vorrang.
- [QUELLEN.md](QUELLEN.md): GitHub-Verweise auf historische Aufträge, vorhandene Code-Branches, Repo-Regeln und den alten Admin-Cleanup-Entwurf.

## Pakete
- [01-pug-kern](01-pug-kern/AUFTRAG.md)
- [02-admin-ui](02-admin-ui/AUFTRAG.md)
- [03-draft-performance](03-draft-performance/AUFTRAG.md)
- [04-caddy-cache](04-caddy-cache/AUFTRAG.md)
- [05-turnierformat](05-turnierformat/AUFTRAG.md)
- [06-zeiten-reminder](06-zeiten-reminder/AUFTRAG.md)
- [07-steam-rang](07-steam-rang/AUFTRAG.md)
- [08-lobby-steam](08-lobby-steam/AUFTRAG.md)
- [09-twitch-werbung](09-twitch-werbung/AUFTRAG.md)
- [10-onboarding](10-onboarding/AUFTRAG.md)
- [11-brain-mcp](11-brain-mcp/AUFTRAG.md)
- [12-altlasten](12-altlasten/AUFTRAG.md)
- [13-abnahme-release](13-abnahme-release/AUFTRAG.md)

Agent 01 arbeitet bzw. arbeitete separat am PUG-Kern; zuerst neuesten Stand/Übergabe prüfen. Keine parallele Neuimplementierung. GitHub-Push allein ist weder Review-Freigabe noch Deployment. Änderungen auf Feature-Branches mit PR übergeben, Maintainer entscheidet Merge/Release.
