import assert from 'node:assert/strict'
import test from 'node:test'
import { reachesMiniGroup } from '../src/lib/miniGroupReach.ts'

const match = (id, round, position, extra = {}) => ({
  id, round, position, bracket_type: 'winners', mini_group_id: null,
  source_match1_id: null, source_match2_id: null, loser_to_match_id: null, ...extra,
})

test('Runde 1 erreicht die Finalgruppe über Runde 2', () => {
  const matches = [
    match(1, 1, 0), match(2, 1, 1),
    match(3, 2, 0, { source_match1_id: 1, source_match2_id: 2 }),
    match(4, 3, 0, { mini_group_id: 9, source_match1_id: 3 }),
  ]
  assert.equal(reachesMiniGroup(matches[0], matches), true)
  assert.equal(reachesMiniGroup(matches[2], matches), true)
})

test('Reines K.-o.-Bracket bleibt korrigierbar', () => {
  const matches = [
    match(1, 1, 0), match(2, 1, 1),
    match(3, 2, 0, { source_match1_id: 1, source_match2_id: 2 }),
  ]
  assert.equal(reachesMiniGroup(matches[0], matches), false)
})

test('Ohne Quellverweise folgt die Prüfung Runde und Position wie der Server', () => {
  const matches = [match(1, 1, 1), match(2, 2, 0), match(3, 3, 0, { mini_group_id: 4, source_match1_id: 2 })]
  assert.equal(reachesMiniGroup(matches[0], matches), true)
})

test('Verliererpfad in eine Mini-Gruppe zählt', () => {
  const matches = [match(1, 1, 0, { loser_to_match_id: 2 }), match(2, 1, 0, { bracket_type: 'losers', mini_group_id: 5 })]
  assert.equal(reachesMiniGroup(matches[0], matches), true)
})

test('Zyklische Verweise enden ohne Endlosschleife', () => {
  const matches = [match(1, 1, 0, { source_match1_id: 2 }), match(2, 1, 1, { source_match1_id: 1 })]
  assert.equal(reachesMiniGroup(matches[0], matches), false)
})
