import test from 'node:test'
import assert from 'node:assert/strict'
import { sortAdminLive, tournamentStart } from '../src/lib/adminTournaments.ts'

const NOW = Date.parse('2026-11-23T20:00:00Z')
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

test('Im Admin wird ein aktueller Cup vor einem veralteten PUG ausgewählt', () => {
  const oldPug = t(16, {
    status: 'bracket',
    registration_end: '2026-11-14T17:00:00Z',
    updated_at: '2026-11-14T17:16:45Z',
  })
  const newCup = t(17)
  assert.deepEqual(sortAdminLive([oldPug, newCup], NOW).map((item) => item.id), [17, 16])
})

test('Ein aktives Turnier mit neuer Aktivität bleibt vor archivbedürftigen Turnieren', () => {
  const newActive = t(9, { status: 'bracket', registration_end: '2026-11-10T18:00:00Z' })
  const inactive = t(8, { status: 'bracket', registration_end: '2026-11-10T18:00:00Z', updated_at: '2026-11-10T21:00:00Z' })
  assert.deepEqual(sortAdminLive([inactive, newActive], NOW).map((item) => item.id), [9, 8])
})

test('Startanzeige nutzt Anmeldeschluss als Ersatz für fehlenden Bracket-Start', () => {
  assert.equal(tournamentStart(t(1)), '2026-11-28T15:00:00Z')
  assert.equal(tournamentStart(t(2, { bracket_start: '2026-11-29T18:00:00Z' })), '2026-11-29T18:00:00Z')
})
