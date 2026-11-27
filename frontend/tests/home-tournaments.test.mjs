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

test('Abgeschlossene Turniere landen unter den früheren Turnieren', () => {
  const completed = tournament(1, { status: 'completed' })
  const archived = tournament(2, { status: 'archived', updated_at: '2026-11-20T10:00:00Z' })
  const { open, past } = groupHomeTournaments([completed, archived], NOW)
  assert.equal(open.length, 0)
  assert.deepEqual(past.map((t) => t.id), [1, 2])
})

test('Laufende Liga bleibt aktuell, auch wenn ihr Datensatz lange nicht bearbeitet wurde', () => {
  const league = tournament(3, {
    status: 'group_phase',
    tournament_mode: 'league',
    group_phase_start: '2026-11-01T18:00:00Z',
    updated_at: '2026-11-01T18:00:00Z',
  })
  const { open, past } = groupHomeTournaments([league], NOW)
  assert.deepEqual(open.map((t) => t.id), [3])
  assert.equal(past.length, 0)
})
