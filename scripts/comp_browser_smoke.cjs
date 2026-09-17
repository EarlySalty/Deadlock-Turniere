// Anonymous browser smoke. Creates one temporary lobby, then removes every test seat.
// Run with PLAYWRIGHT_MODULE pointing to an installed playwright package.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { randomUUID } = require('node:crypto');
const assert = require('node:assert/strict');
const fs = require('node:fs');

(async () => {
  const origin = process.env.COMP_SMOKE_ORIGIN || 'https://deutsche-deadlock-community.de';
  const output = process.env.COMP_SMOKE_OUTPUT || '/tmp/comp-browser-test';
  fs.mkdirSync(output, { recursive: true });
  const browser = await chromium.launch({ executablePath: process.env.CHROME_BIN || '/usr/local/bin/google-chrome', headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage'] });
  const contexts = [];
  const tokens = [];
  const errors = [];
  let code;
  let api;
  let success = false;
  try {
    for (let i = 0; i < 6; i++) {
      const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
      contexts.push(context);
      const page = await context.newPage();
      page.on('pageerror', error => errors.push(error.message));
      if (i === 0) {
        await page.goto(`${origin}/turnier/draft`, { waitUntil: 'networkidle' });
        assert.equal(await page.getByRole('navigation', { name: 'Team-Modus', exact: true }).count(), 1);
        await page.getByRole('navigation', { name: 'Team-Modus', exact: true }).getByRole('link', { name: 'Comp-Finder', exact: true }).click();
        await page.getByLabel('Dein Name', { exact: true }).fill('Deploy-Test 1');
        await page.getByRole('button', { name: 'Lobby erstellen', exact: true }).click();
        await page.waitForURL(/\/turnier\/comp\/[A-HJ-NP-Z2-9]{8}$/);
        code = page.url().split('/').pop();
        api = `${origin}/turnier/api/comp/lobbies/${code}`;
      } else {
        await page.goto(`${origin}/turnier/comp/${code}`);
        await page.getByLabel('Dein Name', { exact: true }).fill(`Deploy-Test ${i + 1}`);
        await page.getByRole('button', { name: 'Mitspielen', exact: true }).click();
      }
      await page.getByRole('heading', { name: 'Deine Helden', exact: true }).waitFor();
      tokens.push(await page.evaluate(c => sessionStorage.getItem(`comp-player:${c}`), code));
      const response = await page.request.get(`${origin}/turnier/api/draft/heroes`);
      assert.equal(response.status(), 200);
      const heroes = (await response.json()).heroes;
      assert.ok(heroes.length >= 6);
      const hero = heroes[i].name;
      const button = page.locator('.comp-hero').filter({ hasText: hero }).filter({ has: page.locator('.comp-hero-name', { hasText: hero }) });
      // Exact accessible name avoids overlapping hero names.
      const escaped = hero.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const pick = page.getByRole('button', { name: new RegExp(`^${escaped}:`) });
      assert.equal(await button.count(), 1);
      for (let j = 0; j < 3; j++) await pick.click();
      await page.getByRole('button', { name: 'Auswahl speichern', exact: true }).click();
      await page.getByRole('status').filter({ hasText: 'Deine Auswahl ist gespeichert.' }).waitFor();
      await page.reload();
      await page.getByRole('heading', { name: 'Deine Helden', exact: true }).waitFor();
      assert.equal(await page.evaluate(c => sessionStorage.getItem(`comp-player:${c}`), code), tokens[i]);
    }
    const page = contexts[0].pages()[0];
    const response = await page.request.get(api);
    assert.equal(response.status(), 200);
    assert.equal(response.headers()['cache-control'], 'no-store');
    const room = await response.json();
    assert.equal(room.members.length, 6);
    assert.equal(room.you, null);
    const comp = room.results.compositions[0];
    assert.equal(comp.score, 12);
    assert.equal(new Set(comp.assignments.map(a => a.hero_name)).size, 6);
    assert.equal(comp.assignments.length, 6);
    const overflow = await page.request.post(`${api}/join`, { headers: { 'X-Comp-Token': randomUUID() }, data: { name: 'Overflow-Test' } });
    assert.equal(overflow.status(), 409);
    const unauthorized = await page.request.post(`${api}/preferences`, { data: { revision: 0, preferences: [] } });
    assert.equal(unauthorized.status(), 401);
    await page.reload();
    await page.getByRole('heading', { name: 'Eure Aufstellungen', exact: true }).waitFor();
    await page.screenshot({ path: `${output}/comp-desktop.png`, fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(500);
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), 'mobile horizontal overflow');
    await page.screenshot({ path: `${output}/comp-mobile.png`, fullPage: true });
    assert.deepEqual(errors, []);
    success = true;
    console.log(JSON.stringify({ browser_flow: 'passed', members: 6, score: 12, unique_heroes: 6, reload: 'passed', capacity: 'passed', unauthorized_write: 'rejected', mobile_overflow: false, runtime_errors: errors.length }));
  } finally {
    const cleanupErrors = [];
    if (api && contexts.length) {
      for (const token of tokens) {
        const response = await contexts[0].request.post(`${api}/leave`, { headers: { 'X-Comp-Token': token } });
        if (response.status() !== 200) cleanupErrors.push(response.status());
      }
      const response = await contexts[0].request.get(api);
      if (response.status() !== 404) cleanupErrors.push(response.status());
    }
    await browser.close();
    assert.deepEqual(cleanupErrors, [], 'temporary lobby cleanup');
    if (success) console.log('Temporary lobby fully removed.');
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
