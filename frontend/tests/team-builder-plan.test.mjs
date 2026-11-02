import assert from 'node:assert/strict'
import test from 'node:test'
import { EMPTY_EDITS, buildView, planSave, snapshotOf } from '../src/lib/teamBuilder.ts'

const member = (id, role = 'member') => ({
  discord_id: id,
  discord_name: `Spieler ${id}`,
  steam_id: null,
  rank: null,
  rank_score: 0,
  role,
  joined_at: '2026-11-01T10:00:00Z',
})

const team = (id, ids) => ({
  id,
  tournament_id: 1,
  name: `Team ${id}`,
  name_key: `team ${id}`,
  captain_discord_id: ids[0] ?? '0',
  members: ids.map((m, i) => member(m, i === 0 ? 'captain' : 'member')),
  created_at: '2026-11-01T10:00:00Z',
  recruitment_status: 'open',
  has_pending_applications: false,
})

const signup = (id, discordId, teamId = null) => ({
  id,
  tournament_id: 1,
  discord_id: discordId,
  discord_name: `Solo ${discordId}`,
  steam_id: null,
  rank: 'Oracle',
  rank_score: 50,
  team_id: teamId,
  signed_up_at: '2026-11-01T09:00:00Z',
})

test('neue Teams entstehen vor den Zuweisungen aus der Solo-Liste', () => {
  const signups = [signup(11, 'a'), signup(12, 'b')]
  const edits = {
    ...EMPTY_EDITS,
    newTeams: [{ key: 'n1', name: 'Neu' }],
    placements: { a: 'n1', b: 'n1' },
  }
  const steps = planSave(snapshotOf([], signups), edits, 6)
  assert.deepEqual(steps.map((s) => s.kind), ['create', 'assign', 'assign'])
  assert.deepEqual(steps.slice(1).map((s) => s.signupId), [11, 12])
})

test('Tausch zwischen zwei vollen Teams löst den Kreis über die Solo-Liste auf', () => {
  const teams = [team(1, ['a', 'b']), team(2, ['c', 'd'])]
  const edits = { ...EMPTY_EDITS, placements: { a: 's2', c: 's1' } }
  const steps = planSave(snapshotOf(teams, []), edits, 2)
  assert.deepEqual(
    steps.map((s) => `${s.kind}:${s.discordId}:${s.teamKey}`),
    ['remove:a:s1', 'move:c:s1', 'assign:a:s2'],
  )
  assert.equal(steps[2].signupId, null)
})

test('Entfernen aus einem Team kommt vor dem Auffüllen desselben Teams', () => {
  const teams = [team(1, ['a', 'b'])]
  const edits = { ...EMPTY_EDITS, placements: { a: null, x: 's1' } }
  const steps = planSave(snapshotOf(teams, [signup(20, 'x')]), edits, 2)
  assert.deepEqual(steps.map((s) => s.kind), ['remove', 'assign'])
  const view = buildView(teams, [signup(20, 'x')], edits)
  assert.deepEqual(view.pool.map((p) => p.discordId), ['a'])
  assert.deepEqual(view.teams[0].members.map((p) => p.discordId), ['b', 'x'])
})
