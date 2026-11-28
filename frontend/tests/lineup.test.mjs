import test from 'node:test'
import assert from 'node:assert/strict'
import { benchFreeLabel, isTeamComplete, lineupOf, teamFormatLabel } from '../src/lib/lineup.ts'
import { teamOverview } from '../src/lib/adminWorkflow.ts'

const roster = (starters, bench) => [
  ...Array.from({ length: starters }, (_, i) => ({ discord_id: `s${i}`, is_bench: false })),
  ...Array.from({ length: bench }, (_, i) => ({ discord_id: `b${i}`, is_bench: true })),
]

test('Ersatzspieler zählen nie zur Vollständigkeit', () => {
  for (const teamSize of [6, 8]) {
    assert.equal(isTeamComplete(roster(teamSize, 1), teamSize), true)
    assert.equal(isTeamComplete(roster(teamSize - 1, 3), teamSize), false)
    assert.deepEqual(lineupOf(roster(teamSize, 0), teamSize, 1), { starters: teamSize, bench: 0, starterFree: 0, benchFree: 1 })
  }
})

test('Unvollständige Teams im Admin-Ablauf zählen nur Stammspieler', () => {
  for (const teamSize of [6, 8]) {
    const overview = teamOverview({
      team_size: teamSize,
      teams: [
        { id: 1, members: roster(teamSize, 1) },
        { id: 2, members: roster(teamSize - 1, 1) },
      ],
      signups: [],
    })
    assert.equal(overview.incompleteTeams, 1)
  }
})

test('Kopfzeile und freie Ersatzplätze nutzen die echten Werte', () => {
  assert.equal(teamFormatLabel(6, 0), '6 gegen 6')
  assert.equal(teamFormatLabel(6, 1), '6 gegen 6 · bis zu 1 Ersatz')
  assert.equal(teamFormatLabel(8, 2), '8 gegen 8 · bis zu 2 Ersatz')
  assert.equal(benchFreeLabel(1), '1 Ersatzplatz frei')
  assert.equal(benchFreeLabel(2), '2 Ersatzplätze frei')
})
