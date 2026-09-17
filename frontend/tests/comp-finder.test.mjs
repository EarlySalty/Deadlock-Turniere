import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { normalizeCompCode, compTokenKey, compShareUrl, nextPriority, preferenceList, preferenceMap, newestRoom, compositionText } from '../src/hooks/compState.ts'

test('Comp-Codes und Einladungslinks werden ohne Token normalisiert', () => {
  assert.equal(normalizeCompCode(' abcd2345 '), 'ABCD2345')
  assert.equal(normalizeCompCode('https://example.com/turnier/comp/abcd2345'), 'ABCD2345')
  for (const input of ['ABCD0000', 'ABCD1234', 'javascript:alert(1)', 'ABC', '/draft/ABCD2345']) assert.equal(normalizeCompCode(input), null)
  assert.equal(compShareUrl('https://example.com', '/turnier/', 'abcd2345'), 'https://example.com/turnier/comp/ABCD2345')
  assert.equal(compTokenKey('abcd2345'), 'comp-player:ABCD2345')
})

test('Nicht ausgewählt und null Punkte sind verschiedene Zustände', () => {
  assert.equal(nextPriority(undefined), 0)
  assert.equal(nextPriority(0), 1)
  assert.equal(nextPriority(1), 2)
  assert.equal(nextPriority(2), undefined)
  const preferences = [{ hero_name: 'Warden', priority: 0 }, { hero_name: 'Abrams', priority: 2 }]
  assert.deepEqual(preferenceList(preferenceMap(preferences)), [preferences[1], preferences[0]])
})

test('Verzögerte Antworten überschreiben keinen neueren Lobby-Stand', () => {
  const current = { code: 'ABCD2345', revision: 5 }
  assert.equal(newestRoom(current, { ...current, revision: 4 }), current)
  const next = { ...current, revision: 6 }
  assert.equal(newestRoom(current, next), next)
  const other = { code: 'WXYZ2345', revision: 1 }
  assert.equal(newestRoom(current, other), other)
})

test('Aufstellung wird mit Spielerzuordnung und Wunschpunkten kopiert', () => {
  const room = { members: [{ name: 'Spieler A' }], results: { max_score: 2 } }
  const composition = { score: 0, assignments: [{ player_index: 0, hero_name: 'Warden', priority: 0 }] }
  assert.match(compositionText(room, composition, 1), /Spieler A: Warden \(0 P\.\)/)
  assert.match(compositionText(room, composition, 1), /keine Meta- oder Synergie-Bewertung/)
})

test('Comp-Finder ist im Router, Draft-Moduswechsel und der Hauptnavigation erreichbar', () => {
  const read = path => readFileSync(new URL(path, import.meta.url), 'utf8')
  const app = read('../src/App.tsx')
  assert.match(app, /path="comp" element=\{<CompFinder \/>\}/)
  assert.match(app, /path="comp\/:code" element=\{<CompFinder \/>\}/)
  assert.match(read('../src/pages/DraftLobbyNeu.tsx'), /<TeamModeNav \/>/)
  assert.match(read('../src/components/layout/Header.tsx'), /label: 'Comp-Finder', path: '\/comp'/)
  assert.match(read('../src/hooks/useCompFinder.ts'), /'X-Comp-Token': options.token/)
})
