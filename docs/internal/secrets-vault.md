# Secrets und Vault

Das Turnierbackend ist inzwischen so aufgebaut, dass Secret-Werte nicht fest an eine einzelne Plattform oder einen einzelnen Secret-Store gebunden sind. Entscheidend ist die Reihenfolge der Auflösung:

1. `<NAME>_FILE`
2. Secret-Dateien aus einem Credentials-/Secrets-Verzeichnis
3. normales Environment `<NAME>`
4. optionaler lokaler Keyring-Fallback

Damit kann dasselbe Backend lokal, unter `systemd` oder hinter einem Vault-Agent laufen, ohne dass Codepfade für die eigentlichen Geheimnisse umgeschrieben werden müssen.

## Zielbild für Produktion

Die bevorzugte Produktionsvariante ist:

- Vault bleibt Source of Truth
- ein Agent oder Credential-Mechanismus rendert Secret-Dateien zur Laufzeit
- der Service bekommt nur Dateipfade oder gemountete Credentials

Wichtig ist dabei weniger das konkrete Produkt als das Prinzip: Secret-Inhalt soll nicht als Klartext in versionierten Dateien oder Shell-Kommandos herumliegen.

## Relevante Variablennamen

Für dieses Repo sind vor allem diese Secret-Namen vorgesehen:

- `DISCORD_CLIENT_ID`
- `DISCORD_CLIENT_SECRET`
- `JWT_SECRET`
- `DISCORD_WEBHOOK_URL`
- `STEAM_BRIDGE_DB_PATH`

Nicht geheime Betriebswerte können regulär in die Konfiguration:

- `BACKEND_PORT`
- `FRONTEND_URL`
- `DATABASE_PATH`
- Guild-/Role-Konfiguration
- Reminder- oder Feature-Flags

## Unterstützte Betriebsarten

### 1. `_FILE`-Konvention

Wenn etwa `JWT_SECRET_FILE` gesetzt ist, liest das Backend den Wert aus der referenzierten Datei. Das ist die sauberste Brücke zwischen Secret-Management und Applikation, weil in der Prozessumgebung nur der Dateipfad auftaucht.

### 2. systemd Credentials

Wenn der Dienst per `LoadCredential=` startet, kann das Backend direkt aus dem von `systemd` bereitgestellten Credentials-Verzeichnis lesen. Das reduziert Shell-Leaks und erleichtert Rotation.

### 3. Vault-Agent / Secret-Files

Ein Vault-Agent kann Secret-Dateien in ein Laufzeitverzeichnis rendern. Das Backend konsumiert dann wieder nur die Dateien. Der Vorteil: Rotation und zentrale Policy-Steuerung bleiben außerhalb der App.

## Betriebsregeln

- `JWT_SECRET` muss stabil und persistent bleiben. Ein flüchtiger Zufallswert ist für Produktion ungeeignet.
- Falls Steam-Bridge und Turnierbackend getrennt laufen, sollte die Queue-Anbindung sehr bewusst betrachtet werden. Eine wacklige Remote-Dateifreigabe ist kein gutes Zielbild.
- Secret-Rotation sollte immer so geplant werden, dass Dateiquellen erneuert werden können, ohne den Anwendungscode anzufassen.

## Was in diese Doku nicht gehört

Diese Doku hält absichtlich nur Mechanik und Variablennamen fest. Nicht hier hinein gehören:

- Secret-Werte
- DSN-Strings
- lokale Pfade einer konkreten Maschine
- einmalige Operator-Token

Kurz: Das Backend ist Secret-Store-agnostisch, solange es Werte aus Dateien oder kontrollierten Umgebungen lesen kann. Für Produktion ist dateibasierte Übergabe über `systemd` oder Vault das saubere Ziel.
