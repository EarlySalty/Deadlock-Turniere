import test from 'node:test'
import assert from 'node:assert/strict'
import { sortByStart, tournamentStart } from '../src/lib/adminTournaments.ts'

const t = (id, overrides = {}) => ({
  id,
  name: 'Turnier',
  status: 'registration',
  is_test: false,
  tournament_mode: 'bracket_only',
  bracket_start: null,
  group_phase_start: null,
  registration_end: '2026-11-28T15:00:00Z',
  checkin_start: '2026-11-28T14:30:00Z',
  updated_at: '2026-11-23T19:30:00Z',
  ...overrides,
})

test('Im Admin bleibt ein lange laufendes Turnier nach Start sortiert statt nach Alter des Datensatzes', () => {
  const running = t(16, { status: 'bracket', registration_end: '2026-11-14T17:00:00Z', updated_at: '2026-11-14T17:16:45Z' })
  const upcoming = t(17)
  assert.deepEqual(sortByStart([upcoming, running]).map((item) => item.id), [16, 17])
})

test('Startanzeige nutzt Anmeldeschluss als Ersatz für fehlenden Bracket-Start', () => {
  assert.equal(tournamentStart(t(1)), '2026-11-28T15:00:00Z')
  assert.equal(tournamentStart(t(2, { bracket_start: '2026-11-29T18:00:00Z' })), '2026-11-29T18:00:00Z')
})
