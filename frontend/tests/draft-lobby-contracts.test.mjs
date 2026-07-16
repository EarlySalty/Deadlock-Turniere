import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath, URL } from 'node:url'

const helpersUrl = new URL('../src/hooks/draftLobbyState.ts', import.meta.url)

async function loadHelpers() {
  assert.ok(
    existsSync(fileURLToPath(helpersUrl)),
    'die testbaren Draft-Lobby-Zustandshelfer fehlen',
  )
  return import(helpersUrl.href)
}

test('der Countdown wird aus Server-Deadline und aktueller Zeit abgeleitet', async () => {
  const { remainingSeconds } = await loadHelpers()

  assert.equal(remainingSeconds(undefined, 1_000), null)
  assert.equal(remainingSeconds('1970-01-01T00:00:02.500Z', 1_000), 2)
  assert.equal(remainingSeconds('1970-01-01T00:00:00.500Z', 1_000), 0)
})

test('der URL-Token gewinnt und der gespeicherte Token bleibt der Fallback', async () => {
  const { selectCaptainToken } = await loadHelpers()

  assert.equal(selectCaptainToken(undefined, 'url', 'gespeichert'), null)
  assert.equal(selectCaptainToken('ABC123', 'url', 'gespeichert'), 'url')
  assert.equal(selectCaptainToken('ABC123', null, 'gespeichert'), 'gespeichert')
  assert.equal(selectCaptainToken('ABC123', '', 'gespeichert'), 'gespeichert')
})

test('leere Hero-Bildadressen werden nicht als Bildquelle verwendet', async () => {
  const { heroImageUrl } = await loadHelpers()

  assert.equal(heroImageUrl(''), null)
  assert.equal(heroImageUrl('   '), null)
  assert.equal(heroImageUrl(' https://example.invalid/hero.webp '), 'https://example.invalid/hero.webp')
})

test('die Hero-Kachel reicht keine ungeprüfte Bildadresse an img weiter', () => {
  const board = readFileSync(new URL('../src/pages/DraftBoard.tsx', import.meta.url), 'utf8')

  assert.ok(board.includes('heroImageUrl(held.image_url)'))
  assert.ok(!board.includes('<img src={held.image_url}'))
})

test('externe Schriftimporte stehen vor allen erzeugenden CSS-Importen', () => {
  const indexCss = readFileSync(new URL('../src/index.css', import.meta.url), 'utf8')
  const brandCss = readFileSync(new URL('../src/brand-tokens.css', import.meta.url), 'utf8')

  assert.ok(indexCss.trimStart().startsWith('@import url('))
  assert.ok(!brandCss.includes('@import url('))
})
