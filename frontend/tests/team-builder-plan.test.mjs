import assert from 'node:assert/strict'
import test from 'node:test'
import {
  EMPTY_EDITS,
  buildView,
  captainChanges,
  planSave,
  rebaseEdits,
  snapshotOf,
} from '../src/lib/teamBuilder.ts'

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

const named = (id, name, ids = []) => ({ ...team(id, ids), name, name_key: name.toLowerCase() })

test('Namenstausch läuft über einen freien Zwischennamen', () => {
  const teams = [named(1, 'Alpha'), named(2, 'Beta')]
  const edits = { ...EMPTY_EDITS, names: { s1: 'Beta', s2: 'Alpha' } }
  const steps = planSave(snapshotOf(teams, []), edits, 6)
  assert.deepEqual(
    steps.map((s) => `${s.teamKey}:${s.name}`),
    ['s1:Umbenennung 1', 's2:Alpha', 's1:Beta'],
  )
})

test('Ein Name wird erst frei, dann neu vergeben', () => {
  const teams = [named(1, 'Alpha'), named(2, 'Straße')]
  const edits = {
    ...EMPTY_EDITS,
    newTeams: [
      { key: 'n1', name: 'ALPHA' },
      { key: 'n2', name: 'Strasse' },
    ],
    names: { s1: 'Gamma', s2: 'Delta' },
  }
  const steps = planSave(snapshotOf(teams, []), edits, 6)
  assert.deepEqual(
    steps.map((s) => `${s.kind}:${s.teamKey}`),
    ['rename:s1', 'create:n1', 'rename:s2', 'create:n2'],
  )
})

test('Der zuerst eingesetzte Spieler bleibt Captain, auch nach dem Speichern', () => {
  const signups = [signup(11, 'a'), signup(12, 'b')]
  const edits = { ...EMPTY_EDITS, newTeams: [{ key: 'n1', name: 'Neu' }], placements: { b: 'n1', a: 'n1' } }
  const snapshot = snapshotOf([], signups)
  const view = buildView([], signups, edits)
  assert.equal(view.teams[0].captainId, 'b')
  const steps = planSave(snapshot, edits, 6)
  assert.deepEqual(captainChanges(view, snapshot, edits, steps), [{ teamKey: 'n1', discordId: 'b' }])
})

test('Nach einem Teilfehler wird ein schon angelegtes Team nicht erneut angelegt', () => {
  const edits = {
    ...EMPTY_EDITS,
    newTeams: [{ key: 'n1', name: 'Neu' }],
    placements: { b: 'n1', a: 'n1' },
    captains: { n1: 'b' },
  }
  const rebased = rebaseEdits(edits, new Map([['n1', 5]]))
  const teams = [{ ...named(5, 'Neu', ['a']) }]
  const signups = [signup(11, 'a', 5), signup(12, 'b')]
  const snapshot = snapshotOf(teams, signups)
  const steps = planSave(snapshot, rebased, 6)
  assert.deepEqual(
    steps.map((s) => `${s.kind}:${s.discordId}:${s.teamKey}`),
    ['assign:b:s5'],
  )
  const view = buildView(teams, signups, rebased)
  assert.equal(view.teams.length, 1)
  assert.equal(view.teams[0].captainId, 'b')
  assert.deepEqual(captainChanges(view, snapshot, rebased, steps), [{ teamKey: 's5', discordId: 'b' }])
})
