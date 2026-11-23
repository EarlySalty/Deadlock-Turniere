import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const source = (file) => readFileSync(new URL(file, import.meta.url), 'utf8')

test('Solo-Pool und vollständige Teams stehen nebeneinander', () => {
  const manager = source('../src/components/admin/ParticipantManager.tsx')
  assert.ok(manager.includes('data-testid="teilnehmer-arbeitsflaeche"'))
  assert.ok(manager.includes('xl:grid-cols-[minmax(300px,0.9fr)_minmax(0,2.1fr)]'))
  assert.ok(manager.includes('aria-label="Solo-Spieler"'))
  assert.ok(manager.includes('aria-label="Teams und Mitglieder"'))
})

test('Teamkarten zeigen den kompletten Kader unmittelbar an', () => {
  const manager = source('../src/components/admin/ParticipantManager.tsx')
  assert.ok(manager.includes('Kader auf einen Blick'))
  assert.ok(manager.includes('team.members.map((member)'))
  assert.ok(manager.includes('team.captain_discord_id'))
  assert.ok(manager.includes('playerName(member.discord_name)'))
  assert.ok(manager.includes('teamSize - team.members.length'))
})

test('öffentliche Turnierseite zeigt alle Teammitglieder und offene Solo-Spieler', () => {
  const tournament = source('../src/pages/Tournament.tsx')
  assert.ok(tournament.includes('aria-label="Solo-Spieler ohne Team"'))
  assert.ok(tournament.includes('aria-label={`Mitglieder von ${team.name}`}'))
  assert.ok(tournament.includes('team.members.map((m, i)'))
  assert.ok(tournament.includes('playerName(m.discord_name)'))
  assert.ok(!tournament.includes('team.members.slice(0, 3)'))
})

test('schwere Seiten werden nur bei Bedarf geladen, statt den Draft-Start zu blockieren', () => {
  const app = source('../src/App.tsx')
  assert.ok(app.includes("lazy(() => import('@/pages/Tournament'))"))
  assert.ok(app.includes("lazy(() => import('@/pages/Admin'))"))
  assert.ok(app.includes("lazy(() => import('@/pages/DraftBoard'))"))
  assert.ok(app.includes('<Suspense fallback='))
})
