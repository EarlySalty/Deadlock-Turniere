import assert from 'node:assert/strict'
import test from 'node:test'
import { bracketPlacements, placementLabel } from '../src/lib/bracketPlacements.ts'

const match = (bracket_type, round, team1_id, team2_id, winner_id, status = 'completed') => ({
  bracket_type,
  round,
  team1_id,
  team2_id,
  winner_id,
  status,
})

const places = (matches) => bracketPlacements(matches).map((entry) => [entry.teamId, entry.position, entry.label])

test('Double Elimination: der Sieger des Grand-Final-Resets steht auf Platz 1', () => {
  const matches = [
    match('winners', 1, 1, 2, 1),
    match('winners', 1, 3, 4, 3),
    match('losers', 1, 2, 4, 2),
    match('winners', 2, 1, 3, 1),
    match('losers', 2, 2, 3, 3),
    match('grand_final', 3, 1, 3, 3),
    match('grand_final', 4, 1, 3, 3),
  ]
  assert.deepEqual(places(matches), [
    [3, 1, '1. Platz'],
    [1, 2, '2. Platz'],
    [2, 3, '3. Platz'],
    [4, 4, '4. Platz'],
  ])
})

test('Double Elimination: ohne Reset entscheidet das erste Grand Final', () => {
  const matches = [
    match('winners', 1, 1, 2, 1),
    match('winners', 1, 3, 4, 3),
    match('losers', 1, 2, 4, 2),
    match('winners', 2, 1, 3, 1),
    match('losers', 2, 2, 3, 3),
    match('grand_final', 3, 1, 3, 1),
    match('grand_final', 4, null, null, null, 'cancelled'),
  ]
  assert.deepEqual(places(matches).slice(0, 2), [
    [1, 1, '1. Platz'],
    [3, 2, '2. Platz'],
  ])
})

test('Single Elimination bleibt bei Finale und Halbfinale', () => {
  const matches = [
    match('winners', 1, 1, 2, 1),
    match('winners', 1, 3, 4, 3),
    match('winners', 2, 1, 3, 1),
  ]
  assert.deepEqual(places(matches), [
    [1, 1, '1. Platz'],
    [3, 2, '2. Platz'],
    [2, 3, '3./4. Platz'],
    [4, 3, '3./4. Platz'],
  ])
})

test('Double Elimination: offener Reset vergibt noch keinen Titel', () => {
  const matches = [
    match('winners', 1, 1, 2, 1),
    match('winners', 1, 3, 4, 3),
    match('losers', 1, 2, 4, 2),
    match('winners', 2, 1, 3, 1),
    match('losers', 2, 2, 3, 3),
    match('grand_final', 3, 1, 3, 3),
    match('grand_final', 4, 1, 3, null, 'pending'),
  ]
  assert.deepEqual(places(matches), [
    [2, 3, '3. Platz'],
    [4, 4, '4. Platz'],
  ])
})

test('Ranglisten-Platzierung folgt dem Format des Turniers', () => {
  const shared = places([
    match('winners', 1, 1, 2, 1),
    match('winners', 1, 3, 4, 3),
    match('winners', 2, 1, 3, 1),
  ]).find(([team]) => team === 2)[2]
  assert.equal(placementLabel(3, true, '-'), shared)
  assert.equal(placementLabel(3, false, '-'), '3. Platz')
  assert.equal(placementLabel(4, false, '-'), '4. Platz')
  assert.equal(placementLabel(1, false, '-'), '1. Platz')
  assert.equal(placementLabel(2, true, '-'), '2. Platz')
  assert.equal(placementLabel(3, undefined, '-'), '3. Platz')
  assert.equal(placementLabel(null, true, '-'), '-')
  assert.equal(placementLabel(undefined, false, 'Keine Platzierung'), 'Keine Platzierung')
  assert.equal(placementLabel(0, true, '-'), '-')
})
