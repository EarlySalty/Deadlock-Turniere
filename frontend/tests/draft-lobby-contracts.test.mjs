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

test('der Claim-Token wird je Raum-Code unter festem Schlüssel geführt', async () => {
  const { claimSpeicherSchluessel } = await loadHelpers()

  assert.equal(claimSpeicherSchluessel('ABC123'), 'draft-claim:ABC123')
  assert.notEqual(claimSpeicherSchluessel('ABC123'), claimSpeicherSchluessel('XYZ789'))
})

test('Captain-Token wird mit dem Backend-Vertragsheader gesendet', () => {
  const hook = readFileSync(new URL('../src/hooks/useDraftLobby.ts', import.meta.url), 'utf8')

  assert.ok(hook.includes("'X-Draft-Token': claim"))
  assert.ok(!hook.includes("'X-Draft-Claim': claim"))
})

test('die Countdown-Anzeige formatiert Minuten und Sekunden zweistellig', async () => {
  const { countdownText } = await loadHelpers()

  assert.equal(countdownText(null), '')
  assert.equal(countdownText(5), '0:05')
  assert.equal(countdownText(30), '0:30')
  assert.equal(countdownText(90), '1:30')
})

test('die Phasen-Kopfzeile nennt Zugart, Team und Anteil an der Sequenz', async () => {
  const { phasenKopf } = await loadHelpers()
  const sequenz = [
    { index: 0, team: 1, action: 'ban' },
    { index: 1, team: 2, action: 'ban' },
    { index: 2, team: 1, action: 'pick' },
  ]

  assert.deepEqual(phasenKopf(sequenz, 0), {
    label: 'BAN-PHASE',
    anteil: '1/3',
    action: 'ban',
    team: 1,
  })
  assert.deepEqual(phasenKopf(sequenz, 2), {
    label: 'PICK-PHASE',
    anteil: '3/3',
    action: 'pick',
    team: 1,
  })
  assert.equal(phasenKopf(sequenz, 9), null)
})

test('leere Hero-Bildadressen werden nicht als Bildquelle verwendet', async () => {
  const { heroImageUrl } = await loadHelpers()

  assert.equal(heroImageUrl(''), null)
  assert.equal(heroImageUrl('   '), null)
  assert.equal(heroImageUrl(' https://example.invalid/hero.webp '), 'https://example.invalid/hero.webp')
})

test('die Hero-Leiste reicht keine ungeprüfte Bildadresse an img weiter', () => {
  const leiste = readFileSync(new URL('../src/components/draft/HeroLeiste.tsx', import.meta.url), 'utf8')

  assert.ok(leiste.includes('heroImageUrl(held.image_url)'))
  assert.ok(!leiste.includes('<img src={held.image_url}'))
})

test('externe Schriftimporte stehen vor allen erzeugenden CSS-Importen', () => {
  const indexCss = readFileSync(new URL('../src/index.css', import.meta.url), 'utf8')
  const brandCss = readFileSync(new URL('../src/brand-tokens.css', import.meta.url), 'utf8')

  assert.ok(indexCss.trimStart().startsWith('@import url('))
  assert.ok(!brandCss.includes('@import url('))
})
