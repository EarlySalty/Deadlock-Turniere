import assert from 'node:assert/strict'
import test from 'node:test'
import { bracketStageLabel } from '../src/lib/bracketStage.ts'

const stage = (bracket_type, round) => ({ bracket_type, round })

test('Double Elimination: Grand Final und Reset heißen so, unabhängig von der Rundennummer', () => {
  const matches = [
    stage('winners', 1), stage('winners', 2), stage('winners', 3),
    stage('losers', 1), stage('losers', 2), stage('losers', 3), stage('losers', 4),
    stage('grand_final', 4), stage('grand_final', 5),
  ]
  const labels = matches.map((m) => bracketStageLabel(m, matches))
  assert.deepEqual(labels, [
    'WB-Viertelfinale', 'WB-Halbfinale', 'WB-Finale',
    'LB Runde 1', 'LB Runde 2', 'LB Runde 3', 'LB-Finale',
    'Grand Final', 'Grand Final Reset',
  ])
})

test('Single Elimination behält Finale, Halbfinale und Viertelfinale', () => {
  const matches = [stage('winners', 1), stage('winners', 2), stage('winners', 3), stage('winners', 4)]
  const labels = matches.map((m) => bracketStageLabel(m, matches))
  assert.deepEqual(labels, ['Runde 1', 'Viertelfinale', 'Halbfinale', 'Finale'])
})
