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
  assert.ok(manager.includes('playerLabel(member)'))
  assert.ok(manager.includes('teamSize - team.members.length'))
})

test('öffentliche Turnierseite stellt Solo-Spieler und alle Teammitglieder nebeneinander dar', () => {
  const tournament = source('../src/pages/Tournament.tsx')
  assert.ok(tournament.includes('xl:grid-cols-[minmax(250px,320px)_minmax(0,1fr)]'))
  assert.ok(tournament.includes('aria-label="Solo-Spieler"'))
  assert.ok(tournament.includes('aria-label="Gemeldete Teams"'))
  assert.ok(tournament.includes('tournament.signups.filter((signup) => signup.team_id === null)'))
  assert.ok(tournament.includes('team.members.map((member, position)'))
  assert.ok(tournament.includes('Team-Kader'))
  assert.ok(!tournament.includes('team.members.slice(0, 3)'))
})

test('schwere Seiten werden nur bei Bedarf geladen, statt den Draft-Start zu blockieren', () => {
  const app = source('../src/App.tsx')
  assert.ok(app.includes("lazy(() => import('@/pages/Tournament'))"))
  assert.ok(app.includes("lazy(() => import('@/pages/Admin'))"))
  assert.ok(app.includes("lazy(() => import('@/pages/DraftBoard'))"))
  const layout = source('../src/components/layout/Layout.tsx')
  assert.ok(app.includes("lazy(() => import('@/pages/Home'))"))
  assert.ok(app.includes("lazy(() => import('@/pages/DraftLobbyNeu'))"))
  assert.ok(layout.includes('<Suspense fallback='))
  assert.ok(!app.includes('<Suspense fallback='))
})

test('statische Vorschau bleibt getrennt vom regulären Vite/React-Routing', () => {
  const main = source('../src/main.tsx')
  const client = source('../src/api/client.ts')
  const mock = source('../src/hooks/useDraftLobby.ts')
  assert.ok(main.includes('isPreview ? HashRouter : BrowserRouter'))
  assert.ok(client.includes("Vorschau ist schreibgeschützt."))
  assert.ok(client.includes("!['GET', 'HEAD'].includes"))
  assert.ok(mock.includes("import.meta.env.BASE_URL.includes('/_preview/')"))
})
