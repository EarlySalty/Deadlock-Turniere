import assert from 'node:assert/strict'
import test from 'node:test'
import { groupHomeTournaments, isOverdueOpenTournament } from '../src/lib/homeTournaments.ts'

const NOW = Date.parse('2026-11-23T17:40:00Z')
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

test('Überfälliges PUG-Bracket wird nicht als aktuelles Turnier beworben', () => {
  const pug = tournament(16, {
    status: 'bracket',
    registration_end: '2026-11-14T17:00:00Z',
    updated_at: '2026-11-14T17:16:45Z',
  })
  const cup = tournament(17)
  const { open, past, overdueIds } = groupHomeTournaments([pug, cup], NOW)
  assert.deepEqual(open.map((t) => t.id), [17])
  assert.deepEqual(past.map((t) => t.id), [16])
  assert.equal(overdueIds.has(16), true)
})

test('Länger laufendes, aber kürzlich bearbeitetes Turnier bleibt sichtbar', () => {
  const active = tournament(2, {
    status: 'group_phase',
    tournament_mode: 'group_stage',
    group_phase_start: '2026-11-01T18:00:00Z',
  })
  assert.equal(isOverdueOpenTournament(active, NOW), false)
})

test('Abgeschlossene und Test-Turniere werden richtig gruppiert', () => {
  const completed = tournament(1, { status: 'completed' })
  const testTournament = tournament(2, { is_test: true })
  const draft = tournament(3, { status: 'draft' })
  const { open, past } = groupHomeTournaments([completed, testTournament, draft], NOW)
  assert.equal(open.length, 0)
  assert.deepEqual(past.map((t) => t.id), [1])
})
