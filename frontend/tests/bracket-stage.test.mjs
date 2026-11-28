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

function slotSizes(entries) {
  const slotCount = Math.floor(entries / 2)
  const sizes = Array(slotCount).fill(Math.floor(entries / slotCount))
  for (let offset = 0; offset < entries % slotCount; offset += 1) sizes[sizes.length - 1 - offset] += 1
  return sizes
}

function roundMatches(round, sizes, nextGroupId) {
  return sizes.flatMap((size) => {
    if (size === 2) return [{ bracket_type: 'winners', round, mini_group_id: null }]
    const groupId = nextGroupId()
    return Array.from({ length: (size * (size - 1)) / 2 }, () => ({ bracket_type: 'winners', round, mini_group_id: groupId }))
  })
}

function engineBracket(teams) {
  let groupId = 0
  const nextGroupId = () => ++groupId
  const rounds = []
  let entries = teams
  while (entries > 1) {
    const powerOfTwo = (entries & (entries - 1)) === 0
    const sizes = powerOfTwo ? Array(entries / 2).fill(2) : slotSizes(entries)
    rounds.splice(rounds.length, 0, roundMatches(rounds.length + 1, sizes, nextGroupId))
    entries = sizes.length
  }
  return rounds
}

function roundLabels(teams) {
  const rounds = engineBracket(teams)
  const matches = rounds.flat()
  return rounds.map((round) => bracketStageLabel(round[0], matches))
}

test('Rundennamen passen für 3 bis 16 Teams zur Bracket-Struktur', () => {
  const expected = {
    3: ['Finalgruppe'],
    4: ['Halbfinale', 'Finale'],
    5: ['Halbfinale', 'Finale'],
    6: ['Runde 1', 'Finalgruppe'],
    7: ['Runde 1', 'Finalgruppe'],
    8: ['Viertelfinale', 'Halbfinale', 'Finale'],
    9: ['Viertelfinale', 'Halbfinale', 'Finale'],
    10: ['Runde 1', 'Halbfinale', 'Finale'],
    11: ['Runde 1', 'Halbfinale', 'Finale'],
    12: ['Runde 1', 'Runde 2', 'Finalgruppe'],
    13: ['Runde 1', 'Runde 2', 'Finalgruppe'],
    14: ['Runde 1', 'Runde 2', 'Finalgruppe'],
    15: ['Runde 1', 'Runde 2', 'Finalgruppe'],
    16: ['Runde 1', 'Viertelfinale', 'Halbfinale', 'Finale'],
  }
  for (const [teams, labels] of Object.entries(expected)) {
    assert.deepEqual(roundLabels(Number(teams)), labels, `${teams} Teams`)
  }
})
