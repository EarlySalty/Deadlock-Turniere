import fs from 'node:fs';

const [basePath, headPath] = process.argv.slice(2);
if (!basePath || !headPath) {
  console.error('usage: npm-audit-regression.mjs <base.json> <head.json>');
  process.exit(64);
}

const read = (path) => JSON.parse(fs.readFileSync(path, 'utf8'));
const risky = (report) => {
  const result = new Set();
  for (const [name, vulnerability] of Object.entries(report.vulnerabilities || {})) {
    const severity = String(vulnerability?.severity || '').toLowerCase();
    if (!['high', 'critical'].includes(severity)) continue;
    const advisoryKeys = (vulnerability.via || [])
      .filter((entry) => entry && typeof entry === 'object')
      .filter((entry) => ['high', 'critical'].includes(String(entry.severity || severity).toLowerCase()))
      .map((entry) => entry.url || entry.source || entry.title)
      .filter(Boolean);
    if (advisoryKeys.length === 0) {
      result.add(name + ':' + severity);
    } else {
      for (const key of advisoryKeys) result.add(name + ':' + key);
    }
  }
  return result;
};

const base = risky(read(basePath));
const head = risky(read(headPath));
const added = [...head].filter((item) => !base.has(item));
if (added.length) {
  console.error('New HIGH/CRITICAL npm audit findings compared with main:');
  for (const item of added) console.error('  ' + item);
  process.exit(1);
}
console.log(`No new HIGH/CRITICAL npm audit findings compared with main. head=${head.size}, base=${base.size}`);
