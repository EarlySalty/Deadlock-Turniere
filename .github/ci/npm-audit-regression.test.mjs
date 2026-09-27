import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = fileURLToPath(new URL('./npm-audit-regression.mjs', import.meta.url));
const levels = ['info', 'low', 'moderate', 'high', 'critical'];
function report(entries = []) {
  const vulnerabilities = {};
  const counts = Object.fromEntries(levels.map((level) => [level, 0]));
  for (const [name, severity, advisory = 'example'] of entries) {
    vulnerabilities[name] = {
      name, severity, isDirect: true, via: [{ name, severity,
        title: `Test advisory ${advisory}`, url: `https://example.invalid/${advisory}` }],
      effects: [], range: '*', nodes: [`node_modules/${name}`], fixAvailable: false,
    };
    counts[severity] += 1;
  }
  counts.total = entries.length;
  return { auditReportVersion: 2, vulnerabilities, metadata: { vulnerabilities: counts } };
}
function compare(base, head) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'npm-audit-gate-test-'));
  try {
    const files = ['base.json', 'head.json'].map((name) => path.join(directory, name));
    for (const [index, value] of [base, head].entries()) {
      fs.writeFileSync(files[index], typeof value === 'string' ? value : JSON.stringify(value));
    }
    const result = spawnSync(process.execPath, [script, ...files], { encoding: 'utf8', timeout: 5000 });
    assert.ifError(result.error);
    return result;
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

test('Valid clean reports pass', () => assert.equal(compare(report(), report()).status, 0));
test('Findings reproduced in the real baseline remain comparable', () => {
  const known = report([['existing', 'high']]);
  assert.equal(compare(known, known).status, 0);
});
test('Removing a known finding passes', () => {
  assert.equal(compare(report([['existing', 'high']]), report()).status, 0);
});
for (const severity of ['high', 'critical']) {
  test(`A new ${severity} advisory blocks`, () => {
    assert.equal(compare(report(), report([['new-package', severity]])).status, 1);
  });
}
test('A different advisory in an already vulnerable package blocks', () => {
  assert.equal(compare(report([['same-package', 'high', 'old']]), report([['same-package', 'high', 'new']])).status, 1);
});
test('Escalation of the same advisory from high to critical blocks', () => {
  assert.equal(compare(report([['same-package', 'high']]), report([['same-package', 'critical']])).status, 1);
});
for (const [label, malformed] of [
  ['registry failure', { error: { code: 'ECONNREFUSED', summary: 'Unavailable test registry' } }],
  ['empty object', {}],
  ['null', null],
  ['array', []],
  ['invalid JSON', '{'],
  ['wrong format version', { ...report(), auditReportVersion: 1 }],
  ['missing metadata', { auditReportVersion: 2, vulnerabilities: {} }],
  ['invalid vulnerability collection', { ...report(), vulnerabilities: [] }],
  ['error accompanying partial results', { ...report(), error: { code: 'ENOAUDIT' } }],
]) {
  test(`Malformed head audit blocks: ${label}`, () => {
    assert.equal(compare(report(), malformed).status, 1);
  });
  test(`Malformed baseline audit blocks: ${label}`, () => {
    assert.equal(compare(malformed, report()).status, 1);
  });
}
test('Unknown severity cannot be silently ignored', () => {
  const invalid = report([['package', 'high']]);
  invalid.vulnerabilities.package.severity = 'unknown';
  assert.equal(compare(report(), invalid).status, 1);
});
test('Missing advisory data blocks', () => {
  const invalid = report([['package', 'high']]);
  invalid.vulnerabilities.package.via = [];
  assert.equal(compare(report(), invalid).status, 1);
});
test('Malformed advisory data blocks', () => {
  const invalid = report([['package', 'high']]);
  invalid.vulnerabilities.package.via = [{}];
  assert.equal(compare(report(), invalid).status, 1);
});
test('Inconsistent vulnerability counts block', () => {
  const invalid = report([['package', 'high']]);
  invalid.metadata.vulnerabilities.high = 0;
  assert.equal(compare(report(), invalid).status, 1);
});
test('Invalid audit responses are not echoed into logs', () => {
  const marker = 'do-not-log-audit-response';
  const result = compare(report(), { error: { summary: marker } });
  assert.equal(result.status, 1);
  assert.ok(!result.stderr.includes(marker));
  assert.ok(!result.stdout.includes(marker));
});
