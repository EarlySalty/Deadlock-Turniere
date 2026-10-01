status: aktiv
Datum: 2026-10-01

# Auftrag: Turniere Frontend-Updates PR23 bis PR27 integrieren

## Ziel

Die fünf offenen Frontend-Dependency-Updates PR23 bis PR27 werden als zusammenhängender, geprüfter Stand auf `main` gebracht und im statischen Turnierfrontend live verifiziert. Es gibt keinen Einzel-Merge und keinen pauschalen TokenDB-Hold.

## Arbeit

1. PR25 auf dem vorgegebenen Arbeitsbranch behalten und ausschließlich die zugehörigen Quellcommits PR23, PR24, PR26 und PR27 einzeln übernehmen. Alte fremde Main-Historie nicht mergen.
2. Die tatsächliche Nutzung und Kompatibilität von framer-motion 13.4.4, lucide-react 1.48.0, globals 17.12.0, @eslint/js 10.0.1 und @types/react-dom 19.3.0 prüfen. Nur notwendige Kompatibilitätskorrekturen vornehmen.
3. Mit Node 24.14.1 ohne Engine-Bypass die bestehende Frontend-Suite, TypeScript-/Vite-Production-Build und ESLint auf dem gemeinsamen Stand prüfen.
4. Den gemeinsamen Freeze für unabhängige Intent-Abnahme und lokales Merge-Gate bereitstellen. BLOCKs ursächlich beheben und erneut prüfen.
5. Nach positiver Abnahme und Gate aktuelle Basis revalidieren, atomar zusammenführen und pushen, das statische Frontend nach bestehendem Deployvertrag ausrollen, sichtbare UI live verifizieren und Branch/Worktree löschen.

## Grenzen

- Arbeitsbranch: `luna/abschluss-pr25-20261001`
- Worktree: `/home/nathanael/.worktrees/luna-abschluss-turniere-pr25-20261001`
- Quellbranches, fremde Worktrees, TokenDB, Backend-Services, Service-Units, Secret-Quellen und Umgebungsdateien bleiben unverändert.
- Keine Unterthreads oder Unteragenten.
- Keine Pull-Requests als Integrations-Gate und keine GitHub-Actions als Gate.

## Fertig-Kriterium

Alle fünf Updates sind gemeinsam auf `main`, der lokale Review-/Merge-Gate und unabhängige Intent-Abnahme haben freigegeben, das Frontend ist nachweislich live unter der Turnier-URL und UI-Ort geprüft, TokenDB blieb unverändert, und Branch sowie Worktree sind entfernt.

## Ausgangsstand

- `origin/main`: `b6fb84412422813c4373bad2c24af9f73029d62b`
- PR23: `e2bd5663d991910610f75e07bd35d0a2143799bd`
- PR24: `25a1da43f91336a8a853afbde59e7c59e7707468`
- PR25: `91bc6c1a993d3cf1d7926ad519fe2366a62ac856`
- PR26: `912f24b5e7da9286125f67fb6e01b910bf4886c4`
- PR27: `0beddeebd70def79dad2af144e272b1290729791`
- Koordination / Callback: T3 `cf1d8ad4-dd63-403d-a2bf-6cc8b1b9fa93`
