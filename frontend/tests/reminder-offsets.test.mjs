import assert from 'node:assert/strict'
import test from 'node:test'
import {
  addOffset,
  formatOffset,
  offsetsOrDefault,
  removeOffset,
  toMinutes,
} from '../src/components/admin/reminderOffsets.ts'

test('Einheiten werden in Minuten umgerechnet', () => {
  assert.equal(toMinutes(15, 'minutes'), 15)
  assert.equal(toMinutes(2, 'hours'), 120)
  assert.equal(toMinutes(1, 'days'), 1440)
  assert.equal(toMinutes(2, 'weeks'), 20160)
  for (const amount of [0, -3, 1.5, Number.NaN]) assert.equal(toMinutes(amount, 'hours'), null)
})

test('Offsets werden in der größten glatten Einheit angezeigt', () => {
  assert.equal(formatOffset(1440), '1 Tag vorher')
  assert.equal(formatOffset(2880), '2 Tage vorher')
  assert.equal(formatOffset(120), '2 Std vorher')
  assert.equal(formatOffset(60), '1 Std vorher')
  assert.equal(formatOffset(90), '90 Min vorher')
  assert.equal(formatOffset(10080), '1 Woche vorher')
  assert.equal(formatOffset(20160), '2 Wochen vorher')
  assert.equal(formatOffset(0), 'Zum Start')
})

test('Auswahl bleibt eindeutig und absteigend sortiert', () => {
  assert.deepEqual(addOffset([15, 1440], 120), [1440, 120, 15])
  assert.deepEqual(addOffset([1440, 120], 120), [1440, 120])
  assert.deepEqual(removeOffset([1440, 120, 15], 120), [1440, 15])
  assert.deepEqual(offsetsOrDefault([], [1440, 60]), [1440, 60])
  assert.deepEqual(offsetsOrDefault([60, 1440], [15]), [1440, 60])
})
