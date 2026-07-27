# Abschlussbericht: Result-Fetch-Payload

## Befund und Ursache

Der Befund traf zu. `ResultFetchRequest` akzeptierte `winner_team_id`, `score` und
`notes`. Der Service validierte diese Felder, übergab an das Repository aber nur
`match_id` und `match_id_ref`. Deshalb wurden die Ergebnisdaten weder gespeichert
noch in den Idempotenz-Hash aufgenommen. Ein Retry mit geändertem Ergebnisinhalt
konnte dadurch fälschlich als identische Wiederholung `200 OK` erhalten.

Der bisherige Weg unterstützt diese Ergebnisfelder nicht:

- `Website/builds/backend-rust/src/routes/scrim.rs` enthält keinen Result-Fetch-Weg.
  Der vorhandene Website-Proxy reicht den Body unverändert an den Turniere-Endpunkt
  weiter.
- `Deadlock-Bots/rust/crates/dl-dashboard/src/scrims.rs` setzte beim bisherigen
  Result-Fetch nur den Zustand `result_requested`; er übergab keine manuellen
  Ergebnisdaten.
- Tatsächliche abgerufene Match-Ergebnisse schreibt der Bot in
  `scrim.match_result_refs`.

Damit waren die drei Felder ein neuer, aber nicht implementierter Vertrag. Das war
kein übernommenes Legacy-Verhalten und richtete echten Schaden an, weil der Endpunkt
Erfolg für verworfene Daten meldete.

## Änderung

- `winner_team_id`, `score` und `notes` wurden aus `ResultFetchRequest` entfernt.
- Die dadurch tote Validierung im Service wurde entfernt.
- `#[serde(deny_unknown_fields)]` lehnt diese Felder nun an der HTTP-Grenze mit
  `422 Unprocessable Entity` ab.
- Der weiterhin akzeptierte Payload besteht nur aus `match_id_ref`. Damit deckt der
  bestehende Repository-Hash wieder alle akzeptierten Nutzdaten ab.
- Der Regressionstest prüft jedes entfernte Feld einzeln mit demselben
  Idempotency-Key nach einer erfolgreichen Wiederholung.

Der Result-Fetch-Weg ruft Discord nicht auf. Deshalb gab es in diesem Flow keine
Discord-Fehlergrenze anzupassen und keinen Fehler, der bisher still verschluckt
wurde.

## TDD- und Prüfnachweis

- Rot vor dem Produktionsfix: Der fokussierte Routentest erhielt für den geänderten
  Ergebnis-Payload `200 OK` statt einer Ablehnung.
- Grün nach dem Fix:
  `match_block_and_action_operator_routes_persist_the_canonical_flow` — 1 bestanden,
  0 fehlgeschlagen.
- `cargo fmt --all -- --check` — bestanden.
- `cargo build --release --workspace` — bestanden.
- `cargo clippy --workspace --all-targets -- -D warnings` — bestanden.
- Vollständiger Workspace-Test mit zentraler Postgres-Testinstanz:
  Alle Result-Fetch- und übrigen Tests bestanden, außer dem ausdrücklich als
  vorbestehend benannten
  `invalid_proposal_transition_returns_conflict` (`400` statt `409`).
- Unabhängiger Rust-Diff-Review: keine kritischen, wichtigen oder kleinen Findings.
- `git diff --check` — bestanden.

Es wurde weder committet noch gepusht. Die Änderungen liegen im Arbeitsbaum.
