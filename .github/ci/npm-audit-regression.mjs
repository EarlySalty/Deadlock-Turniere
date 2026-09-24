import fs from 'node:fs';

const [basePath, headPath] = process.argv.slice(2);
if (!basePath || !headPath) {
  console.error('usage: npm-audit-regression.mjs <base.json> <head.json>');
  process.exit(64);
}

const levels = ['info', 'low', 'moderate', 'high', 'critical'];
const record = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
const read = (path) => {
  const invalid = (reason) => {
    throw new Error(`Invalid npm audit report (${reason}): ${path}`);
  };
  let report;
  try {
    report = JSON.parse(fs.readFileSync(path, 'utf8'));
  } catch {
    // Registry error responses can contain connection details. Do not echo them.
    invalid('unreadable JSON');
  }
  if (!record(report) || Object.hasOwn(report, 'error') || report.auditReportVersion !== 2) {
    invalid('missing version 2 report or audit error');
  }
  if (!record(report.vulnerabilities) || !record(report.metadata?.vulnerabilities)) {
    invalid('missing vulnerability data');
  }
  const actual = Object.fromEntries(levels.map((level) => [level, 0]));
  for (const vulnerability of Object.values(report.vulnerabilities)) {
    if (!record(vulnerability) || !levels.includes(vulnerability.severity) ||
        !Array.isArray(vulnerability.via) || vulnerability.via.length === 0) {
      invalid('malformed vulnerability');
    }
    for (const via of vulnerability.via) {
      if (typeof via === 'string' && via.length > 0) continue;
      if (!record(via) || !levels.includes(via.severity) ||
          !((typeof via.url === 'string' && via.url.length > 0) ||
            Number.isSafeInteger(via.source) ||
            (typeof via.source === 'string' && via.source.length > 0) ||
            (typeof via.title === 'string' && via.title.length > 0))) {
        invalid('malformed advisory');
      }
    }
    actual[vulnerability.severity] += 1;
  }
  const counts = report.metadata.vulnerabilities;
  actual.total = Object.keys(report.vulnerabilities).length;
  for (const level of [...levels, 'total']) {
    if (!Number.isSafeInteger(counts[level]) || counts[level] < 0 || counts[level] !== actual[level]) {
      invalid('inconsistent vulnerability counts');
    }
  }
  return report;
};
const risky = (report) => {
  const result = new Set();
  for (const [name, vulnerability] of Object.entries(report.vulnerabilities || {})) {
    const severity = String(vulnerability?.severity || '').toLowerCase();
    if (!['high', 'critical'].includes(severity)) continue;
    const advisoryKeys = (vulnerability.via || [])
      .filter((entry) => entry && typeof entry === 'object')
      .filter((entry) => ['high', 'critical'].includes(String(entry.severity || severity).toLowerCase()))
      .map((entry) => {
        const key = entry.url || entry.source || entry.title;
        return key ? `${severity}:${entry.severity || severity}:${key}` : null;
      })
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
