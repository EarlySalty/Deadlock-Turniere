import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const read = (relative) => readFileSync(new URL(relative, import.meta.url), 'utf8')

test('Turnier- und Admin-Abfragen wiederholen sich nicht alle drei Sekunden', () => {
  const hooks = read('../src/hooks/useTournament.ts')
  const adminStart = hooks.indexOf('export function useAdminTournaments()')
  const adminEnd = hooks.indexOf('export function useSoloBalance(', adminStart)
  const admin = hooks.slice(adminStart, adminEnd)

  assert.match(admin, /staleTime: 15_000/)
  assert.match(admin, /refetchInterval: 30_000/)
  assert.match(admin, /refetchIntervalInBackground: false/)
  assert.doesNotMatch(admin, /refetchInterval: 3_000/)
  assert.match(admin, /query\.state\.data\?\.status/)
})

test('Admin-Datenfehler bleiben sichtbar und ausführbare Wiederholungen stehen bereit', () => {
  const admin = read('../src/pages/Admin.tsx')
  assert.ok(admin.includes('Turnierübersicht nicht erreichbar'))
  assert.ok(admin.includes('Turnierdetails nicht geladen'))
  assert.ok(admin.includes('refetchTournaments()'))
  assert.ok(admin.includes('refetchLiveDetail()'))
})

test('Seitenlayouts nutzen die Breite und überschneiden den gemeinsamen Header nicht', () => {
  const layout = read('../src/components/layout/Layout.tsx')
  const draft = read('../src/pages/DraftLobbyNeu.tsx')
  const css = read('../src/draft-pulp.css')
  assert.match(layout, /max-w-\[1680px\]/)
  assert.match(layout, /max-w-\[1560px\]/)
  assert.ok(layout.includes("isDraft"))
  assert.ok(!draft.includes('-my-8'))
  assert.ok(css.includes('width:100%'))
  assert.ok(css.includes('overflow-x:clip'))
})

test('Rust-Routen messen langsame Requests und begrenzen parallele Abfragen', () => {
  const app = read('../../rust/crates/turnier-api/src/app.rs')
  const adminRoute = read('../../rust/crates/turnier-api/src/admin/tournaments.rs')
  const loader = read('../../rust/crates/turnier-api/src/admin/loaders.rs')
  const resolver = read('../../rust/crates/turnier-steam/src/resolver.rs')
  assert.ok(app.includes('server-timing'))
  assert.ok(app.includes('duration_ms > 300.0'))
  assert.ok(adminRoute.includes('tokio::try_join!'))
  assert.ok(loader.includes('.buffered(3)'))
  assert.ok(resolver.includes('.buffer_unordered(4)'))
})
