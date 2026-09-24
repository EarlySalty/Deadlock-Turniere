import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'

const source = readFileSync(new URL('../src/components/admin/adminPhase.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText
const { defaultPhaseFor } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`)

for (const [status, groups, bracket, expected] of [
  ['draft', false, false, 'participants'],
  ['registration', false, false, 'participants'],
  ['checkin', false, false, 'checkin'],
  ['group_phase', true, false, 'group_phase'],
  ['group_phase', false, false, 'participants'],
  ['bracket', true, true, 'bracket'],
  ['bracket', true, false, 'participants'],
  ['completed', true, true, 'bracket'],
  ['completed', true, false, 'group_phase'],
  ['completed', false, false, 'setup'],
  ['archived', true, true, 'bracket'],
  ['archived', true, false, 'group_phase'],
  ['archived', false, false, 'setup'],
]) {
  test(`Admin-Phase ${status}, Gruppen=${groups}, Bracket=${bracket}: ${expected}`, () => {
    assert.equal(defaultPhaseFor(status, groups, bracket), expected)
  })
}
