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


test('öffentliche Turnierdetails vermeiden N+1-Mitglieds- und Bewerbungsabfragen', () => {
  const helpers = read('../../rust/crates/turnier-api/src/public/helpers.rs')
  const routes = read('../../rust/crates/turnier-api/src/public/tournaments.rs')
  const start = helpers.indexOf('pub async fn load_teams_public(')
  const end = helpers.indexOf('pub async fn load_team_response(', start)
  const teamLoader = helpers.slice(start, end)
  assert.ok(teamLoader.includes('load_member_rows_batch(pool, &team_ids)'))
  assert.ok(teamLoader.includes('pending_team_ids'))
  assert.ok(teamLoader.includes('members_by_team.remove(&t.id)'))
  assert.ok(!teamLoader.includes('load_member_rows(pool, t.id).await?'))
  assert.ok(helpers.includes('tm.team_id = ANY($1) ORDER BY tm.team_id, tm.joined_at'))
  assert.ok(routes.includes('tokio::try_join!'))
})

test('der Lasttest ist streng auf bekannte lesende Endpunkte begrenzt', () => {
  const benchmark = read('../../scripts/perf/bench-readonly.mjs')
  assert.ok(benchmark.includes("method: 'GET'"))
  assert.ok(benchmark.includes("concurrency: 8, requests: 96"))
  assert.ok(benchmark.includes("'/api/health'"))
  assert.ok(benchmark.includes("'/api/draft/heroes'"))
  assert.ok(benchmark.includes("'/api/tournaments/17'"))
  assert.ok(benchmark.includes("Stopping for errors:"))
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
