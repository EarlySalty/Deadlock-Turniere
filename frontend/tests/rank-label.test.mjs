import test from 'node:test'
import assert from 'node:assert/strict'
import { rankLabel } from '../src/lib/rankLabel.ts'

test('Rangwert wird als Rangname mit Stufe angezeigt', () => {
  assert.equal(rankLabel(7, null), 'Initiate 1')
  assert.equal(rankLabel(12, null), 'Initiate 6')
  assert.equal(rankLabel(63, 'Ascendant'), 'Ascendant 3')
  assert.equal(rankLabel(72, null), 'Eternus 6')
})

test('Ohne gültigen Rangwert bleibt der gespeicherte Name oder „Ohne Rang“', () => {
  assert.equal(rankLabel(3, 'Seeker'), 'Seeker')
  assert.equal(rankLabel(0, null), 'Ohne Rang')
  assert.equal(rankLabel(99, ''), 'Ohne Rang')
})
