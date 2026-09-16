import test from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(dirname(fileURLToPath(import.meta.url)))
const header = readFileSync(join(root, 'src/components/layout/Header.tsx'), 'utf8')
const home = readFileSync(join(root, 'src/pages/Home.tsx'), 'utf8')
const html = readFileSync(join(root, 'index.html'), 'utf8')

test('Turnier-Shell nutzt die Deutsche-Deadlock-Community-Marke', () => {
  assert.match(header, /Deutsche Deadlock Community/)
  assert.match(header, /brand\/deadlock-d-logo\.png/)
  assert.doesNotMatch(header, />\s*Deadlock\s*<\/Link>/)
  assert.ok(existsSync(join(root, 'public/brand/deadlock-d-logo.png')))
})

test('Navigation bleibt auf Desktop und Mobil erreichbar', () => {
  assert.match(header, /Arena/)
  assert.match(header, /Rangliste/)
  assert.match(header, /Regelwerk/)
  assert.match(header, /turnier-mobile-navigation/)
})

test('Landing und Favicon folgen dem gemeinsamen Markenmuster', () => {
  assert.match(home, /Turniere der Deutschen Deadlock Community/)
  assert.match(home, /from-primary to-accent/)
  assert.match(html, /\/turnier\/brand\/deadlock-d-logo\.png/)
})
