import assert from 'node:assert/strict'
import test from 'node:test'
import { groupHomeTournaments } from '../src/lib/homeTournaments.ts'

const tournament = (id, overrides = {}) => ({
  id,
  name: 'Turnier',
  status: 'registration',
  is_test: false,
  tournament_mode: 'bracket_only',
  bracket_start: null,
  group_phase_start: null,
  registration_end: '2026-11-28T15:00:00Z',
  updated_at: '2026-11-23T14:00:00Z',
  ...overrides,
})

test('Laufende Turniere bleiben aktuell, egal wie alt ihr letzter Datensatz-Stand ist', () => {
  const stale = { status: 'bracket', registration_end: '2026-11-01T17:00:00Z', updated_at: '2026-11-01T17:00:00Z' }
  const modes = [
    tournament(1, { ...stale, tournament_mode: 'bracket_only' }),
    tournament(2, { ...stale, tournament_mode: 'group_stage', status: 'group_phase', group_phase_start: '2026-11-01T18:00:00Z' }),
    tournament(3, { ...stale, tournament_mode: 'league', status: 'group_phase', group_phase_start: '2026-11-01T18:00:00Z' }),
    tournament(4, { ...stale, status: 'checkin' }),
  ]
  const { open, past } = groupHomeTournaments(modes)
  assert.deepEqual(open.map((t) => t.id).sort(), [1, 2, 3, 4])
  assert.equal(past.length, 0)
})

test('Abgeschlossene Turniere landen unter den früheren Turnieren', () => {
  const completed = tournament(1, { status: 'completed' })
  const archived = tournament(2, { status: 'archived', updated_at: '2026-11-20T10:00:00Z' })
  const { open, past } = groupHomeTournaments([completed, archived])
  assert.equal(open.length, 0)
  assert.deepEqual(past.map((t) => t.id), [1, 2])
})

test('Ein heute startendes Turnier steht vor einem seit Wochen laufenden', () => {
  const running = tournament(16, { status: 'bracket', registration_end: '2026-11-14T17:00:00Z' })
  const today = tournament(17, { status: 'registration', bracket_start: '2026-11-28T15:00:00Z' })
  const { open } = groupHomeTournaments([running, today], Date.parse('2026-11-28T12:00:00Z'))
  assert.deepEqual(open.map((t) => t.id), [17, 16])
})
