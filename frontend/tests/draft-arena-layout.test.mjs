import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const quelle = (pfad) => readFileSync(new URL(pfad, import.meta.url), 'utf8')

test('die Draft-Arena zeigt den Hero-Pool im Hauptlayout statt in einer Bottom-Bar', () => {
  const board = quelle('../src/pages/DraftBoard.tsx')
  const hero = quelle('../src/components/draft/HeroLeiste.tsx')
  const css = quelle('../src/draft.css')

  assert.match(board, /lg:grid-cols-\[minmax\(180px,230px\)_minmax\(0,1fr\)_minmax\(180px,230px\)\]/)
  assert.ok(board.includes('<HeroLeiste'))
  assert.ok(hero.includes('draft-hero-panel'))
  assert.ok(hero.includes('Heldenauswahl'))
  assert.ok(!hero.includes('absolute bottom-4'))
  assert.match(css, /minmax\(76px, 1fr\)/)
  assert.ok(!css.includes('max-height: 180px'))
})

test('Picks und Bans müssen explizit bestätigt werden', () => {
  const board = quelle('../src/pages/DraftBoard.tsx')
  assert.ok(board.includes('Ban bestätigen'))
  assert.ok(board.includes('Pick bestätigen'))
  assert.ok(board.includes('disabled={!amZug || bestaetigtLaeuft}'))
  assert.ok(board.includes('onClick={() => onAuswahl(null)}'))
})

test('die Draft-Reihenfolge ist sichtbar und kennzeichnet den aktiven Schritt', () => {
  const board = quelle('../src/pages/DraftBoard.tsx')
  assert.ok(board.includes('aria-label="Draft-Reihenfolge"'))
  assert.ok(board.includes("aria-current={aktuell ? 'step' : undefined}"))
  assert.ok(board.includes('zustand.sequence.map'))
})

test('Room-Wechsel setzt die Ereignisposition zurück', () => {
  const board = quelle('../src/pages/DraftBoard.tsx')
  assert.ok(board.includes('bekanntRef.current.raum !== s.code'))
  assert.ok(board.includes('bekanntRef.current.anzahl = s.actions.length'))
})

test('die Draft-Startseite verwendet die eigenständige Pulp-Bühne mit echten Hero-Artworks', () => {
  const lobby = quelle('../src/pages/DraftLobbyNeu.tsx')
  const style = quelle('../src/draft-pulp.css')
  const index = quelle('../src/index.css')
  assert.ok(lobby.includes('draft-pulp__poster'))
  assert.ok(lobby.includes('draft-pulp__desk'))
  assert.ok(lobby.includes('heroCardImageUrl'))
  assert.ok(!lobby.includes('heldenListe.slice(0, 40)'))
  assert.ok(style.includes('draft-pulp__poster h1 strong'))
  assert.ok(style.includes('draft-arena-screen'))
  assert.ok(index.includes('@import "./draft-pulp.css"'))
})

test('die Lobby hat keine scheinbar klickbaren, funktionslosen Spielmodus-Pills', () => {
  const lobby = quelle('../src/pages/DraftLobbyNeu.tsx')
  assert.ok(lobby.includes('6 gegen 6'))
  assert.ok(!lobby.includes('onWahl={() => {}}'))
  assert.ok(lobby.includes('Draft-Raum erstellen'))
  assert.ok(!lobby.includes('Live-Vorschau'))
})
