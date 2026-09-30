# Koordination der TOML-Abnahme

Im bestehenden Feature-Worktree wurden während der Fortsetzung parallele Änderungen an den Startvorlagen und laufende Tests festgestellt. Seine Arbeit bleibt erhalten.

Die weitere Abnahme dieser Session läuft deshalb isoliert unter `/home/nathanael/.worktrees/turniere-global-toml-abnahme-20260920` auf `feat/turniere-global-toml-abnahme-20260920`. Grundlage ist a721693 und ein konsistenter Snapshot der vorhandenen TOML-Arbeit einschließlich Tests und Startvorlagen. Keine Änderungen anderer Bots.

Vor Merge und Deploy den aktuellen Produktions-HEAD erneut prüfen. Den ursprünglichen Worktree nicht löschen und nicht als bereinigt melden. Aktive Datei `config/bot.toml` ist lokal und ignoriert, `config/bot.example.toml` eine nicht automatisch geladene Vorlage.
