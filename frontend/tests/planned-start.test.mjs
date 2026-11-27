import assert from 'node:assert/strict'
import test from 'node:test'
import { plannedTournamentStart } from '../src/lib/league.ts'

const tournament = (overrides) => ({
  status: 'registration',
  tournament_mode: 'bracket_only',
  tournament_mode_auto: false,
  group_phase_start: '2026-12-05T17:00:00Z',
  bracket_start: '2026-12-05T20:00:00Z',
  registration_end: '2026-12-05T16:00:00Z',
  ...overrides,
})

test('undecided auto mode shows the earliest possible start', () => {
  assert.equal(plannedTournamentStart(tournament({ tournament_mode_auto: true })), '2026-12-05T17:00:00Z')
  assert.equal(
    plannedTournamentStart(tournament({ tournament_mode_auto: true, bracket_start: '2026-12-05T15:00:00Z' })),
    '2026-12-05T15:00:00Z',
  )
})

test('explicit and resolved modes keep their own start', () => {
  assert.equal(plannedTournamentStart(tournament({})), '2026-12-05T20:00:00Z')
  assert.equal(plannedTournamentStart(tournament({ tournament_mode: 'group_stage' })), '2026-12-05T17:00:00Z')
  assert.equal(plannedTournamentStart(tournament({ tournament_mode_auto: true, status: 'bracket' })), '2026-12-05T20:00:00Z')
  assert.equal(plannedTournamentStart(tournament({ tournament_mode_auto: true, tournament_mode: 'league' })), '2026-12-05T17:00:00Z')
})

test('undecided auto mode without starts falls back to the registration end', () => {
  assert.equal(
    plannedTournamentStart(tournament({ tournament_mode_auto: true, group_phase_start: null, bracket_start: null })),
    '2026-12-05T16:00:00Z',
  )
})
