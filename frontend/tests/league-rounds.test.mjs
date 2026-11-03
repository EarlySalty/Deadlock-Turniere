import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { URL } from 'node:url'
import { leagueRounds } from '../src/lib/league.ts'

const game = (id, round, team1_id, team2_id, status = 'pending', winner_id = null) => ({
  id,
  round,
  team1_id,
  team2_id,
  status,
  winner_id,
  scheduled_at: `2026-11-07T${17 + round}:00:00Z`,
})

test('Liga-Runden folgen der gelieferten Runde, nicht der Tabellenreihenfolge', () => {
  const before = [game(1, 1, 1, 4), game(2, 1, 2, 3), game(3, 2, 1, 3), game(4, 2, 4, 2)]
  const after = [game(4, 2, 4, 2, 'completed', 4), game(3, 2, 1, 3), game(2, 1, 2, 3, 'completed', 3), game(1, 1, 1, 4)]
  const ids = (matches) => leagueRounds(matches).map((round) => [round.round, round.matches.map((match) => match.id).sort()])
  assert.deepEqual(ids(before), [[1, [1, 2]], [2, [3, 4]]])
  assert.deepEqual(ids(after), ids(before))
})

test('Spielplan und Ergebnisprotokoll lesen in der Liga die Runde des Spiels', () => {
  const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')
  assert.match(read('../src/components/groups/GroupMatchList.tsx'), /league \? leagueRounds\(group\.matches\)/)
  assert.match(read('../src/pages/Tournament.tsx'), /match\.round \? `Runde \$\{match\.round\}`/)
})
