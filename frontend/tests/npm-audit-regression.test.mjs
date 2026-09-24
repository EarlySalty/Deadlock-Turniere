import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execPath } from 'node:process';
import { test } from 'node:test';
import { fileURLToPath, URL } from 'node:url';

const checker = fileURLToPath(new URL('../../.github/ci/npm-audit-regression.mjs', import.meta.url));

function report(severity) {
  const counts = { info: 0, low: 0, moderate: 0, high: 0, critical: 0, total: 0 };
  const vulnerabilities = {};
  if (severity) {
    counts[severity] = 1;
    counts.total = 1;
    vulnerabilities.example = {
      severity,
      via: [{ severity, source: 123, url: 'https://example.invalid/advisory/123' }],
    };
  }
  return { auditReportVersion: 2, vulnerabilities, metadata: { vulnerabilities: counts } };
}

function check(base, head) {
  const dir = mkdtempSync(join(tmpdir(), 'turniere-audit-regression-'));
  try {
    const basePath = join(dir, 'base.json');
    const headPath = join(dir, 'head.json');
    writeFileSync(basePath, JSON.stringify(base));
    writeFileSync(headPath, JSON.stringify(head));
    const result = spawnSync(execPath, [checker, basePath, headPath], { encoding: 'utf8' });
    assert.equal(result.error, undefined);
    assert.equal(result.signal, null);
    return result;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('vollständige leere Audit-Berichte werden akzeptiert', () => {
  assert.equal(check(report(), report()).status, 0);
});

test('neue HIGH-Funde blockieren und identische bestätigte Funde bleiben vergleichbar', () => {
  assert.equal(check(report(), report('high')).status, 1);
  assert.equal(check(report('high'), report('high')).status, 0);
  assert.equal(check(report('high'), report()).status, 0);
});

test('Eskalation auf CRITICAL blockiert auch bei unveränderter Advisory-ID', () => {
  assert.equal(check(report('high'), report('critical')).status, 1);
  assert.equal(check(report('critical'), report('high')).status, 0);
});

test('Scannerfehler in Head oder Basis können nicht als leerer Audit bestehen', () => {
  const failure = { error: { code: 'ENOAUDIT', summary: 'isolated negative control' } };
  for (const [base, head] of [[report(), failure], [failure, report()], [failure, failure]]) {
    const result = check(base, head);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Invalid npm audit input/);
  }
});

test('fehlende und unvollständige Audit-Strukturen blockieren', () => {
  const incomplete = report('high');
  incomplete.vulnerabilities = {};
  for (const invalid of [null, [], {}, { vulnerabilities: {} }, incomplete]) {
    assert.equal(check(report(), invalid).status, 1);
    assert.equal(check(invalid, report()).status, 1);
  }
});

test('unbekannte Schweregrade und fehlerhafte Advisory-Daten blockieren', () => {
  const invalidSeverity = report('high');
  invalidSeverity.vulnerabilities.example.severity = 'unknown';
  const invalidAdvisory = report('high');
  invalidAdvisory.vulnerabilities.example.via = [null];
  for (const invalid of [invalidSeverity, invalidAdvisory]) {
    assert.equal(check(report(), invalid).status, 1);
  }
});
