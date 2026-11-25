import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'

const read = (relative) => readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8')
const builder = read('../src/components/admin/TeamBuilderPanel.tsx')
const admin = read('../src/pages/Admin.tsx')
const header = read('../src/components/layout/Header.tsx')
const tournament = read('../src/pages/Tournament.tsx')
const manager = read('../src/components/admin/TournamentManager.tsx')

test('Team-Builder zeigt Teams über den Solo-Spielern und Namen ohne Abschneiden', () => {
  const teamsSection = builder.indexOf('<section aria-label="Teams"')
  const soloSection = builder.indexOf('<section aria-label="Solo-Spieler"')
  assert.ok(teamsSection >= 0 && soloSection > teamsSection)
  assert.match(builder, /break-words font-medium text-foreground/)
  assert.match(builder, /Solo-Spieler ohne Team/)
})

test('Erweiterte Teamverwaltung ist nicht mehr eine zweite offene Vollansicht', () => {
  assert.match(admin, /<details className="min-w-0 rounded-xl/)
  assert.match(admin, /Weitere Teamaktionen/)
})

test('Admin-Link ist auch im mobilen Menü erreichbar', () => {
  assert.match(header, /to="\/admin"\s+onClick=\{\(\) => setMenuOpen\(false\)\}/)
})
test('Öffentliche Teams zeigen Spielernamen vollständig und Solo-Spieler separat darunter', () => {
  const teamView = tournament.indexOf('aria-label={`Mitglieder von ${team.name}`}')
  const solos = tournament.indexOf('<section aria-label="Solo-Spieler ohne Team"')
  assert.ok(teamView >= 0 && solos > teamView)
  assert.match(tournament, /team\.members\.map\(\(m, i\)/)
  assert.match(tournament, /safePublicName\(m\.discord_name\)/)
  assert.doesNotMatch(tournament, /team\.members\.slice\(0, 3\)/)
})

test('Setup versteckt lange Formulare und Sonderaktionen, ohne sie zu entfernen', () => {
  assert.match(manager, /aria-expanded=\{settingsOpen\}/)
  assert.match(manager, /Weitere Verwaltungsaktionen \(mit Vorsicht\)/)
  assert.match(manager, /Check-in öffnen/)
  assert.match(manager, /Solo-Spieler automatisch zuweisen/)
})
