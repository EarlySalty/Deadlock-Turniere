import assert from 'node:assert/strict'
import { existsSync, readFileSync, statSync } from 'node:fs'
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

test('Schriften werden ausschließlich aus lokalen Build-Assets geladen', () => {
  const indexCss = readFileSync(new URL('../src/index.css', import.meta.url), 'utf8')
  const brandCss = readFileSync(new URL('../src/brand-tokens.css', import.meta.url), 'utf8')
  const indexHtml = readFileSync(new URL('../index.html', import.meta.url), 'utf8')
  const fontLicense = readFileSync(new URL('../public/fonts/OFL.txt', import.meta.url), 'utf8')
  const fontFiles = [
    'manrope-latin-ext.woff2',
    'manrope-latin.woff2',
    'sora-latin-ext.woff2',
    'sora-latin.woff2',
  ]

  assert.ok(!indexCss.includes('fonts.googleapis.com'))
  assert.ok(!brandCss.includes('@import url('))
  assert.ok(!indexHtml.includes('fonts.googleapis.com'))
  assert.ok(!indexHtml.includes('fonts.gstatic.com'))
  assert.ok(fontLicense.includes('The Manrope Project Authors'))
  assert.ok(fontLicense.includes('The Sora Project Authors'))
  for (const fontFile of fontFiles) {
    const fontPath = fileURLToPath(new URL(`../src/assets/fonts/${fontFile}`, import.meta.url))
    assert.ok(indexCss.includes(`./assets/fonts/${fontFile}`))
    assert.ok(existsSync(fontPath))
    assert.ok(statSync(fontPath).size > 10_000)
  }
})

test('HTML bindet kein nicht pinbares Cloudflare-Skript ein', () => {
  const indexHtml = readFileSync(new URL('../index.html', import.meta.url), 'utf8')

  assert.ok(!indexHtml.toLowerCase().includes('static.cloudflareinsights.com'))
})
