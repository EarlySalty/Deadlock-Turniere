import fs from 'node:fs';

const [basePath, headPath] = process.argv.slice(2);
if (!basePath || !headPath) {
  console.error('usage: npm-audit-regression.mjs <base.json> <head.json>');
  process.exit(64);
}

const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
const severities = new Set(['info', 'low', 'moderate', 'high', 'critical']);
const risk = (severity) => ({ high: 1, critical: 2 })[severity] || 0;
const read = (path) => {
  const report = JSON.parse(fs.readFileSync(path, 'utf8'));
  if (!object(report) || Object.hasOwn(report, 'error') || report.auditReportVersion !== 2 ||
      !object(report.vulnerabilities) || !object(report.metadata?.vulnerabilities)) {
    throw new Error('missing or unsuccessful npm audit v2 report');
  }
  const counts = { info: 0, low: 0, moderate: 0, high: 0, critical: 0, total: 0 };
  for (const vulnerability of Object.values(report.vulnerabilities)) {
    if (!object(vulnerability) || !severities.has(vulnerability.severity) ||
        !Array.isArray(vulnerability.via)) {
      throw new Error('invalid vulnerability in npm audit report');
    }
    for (const entry of vulnerability.via) {
      if (typeof entry === 'string') continue;
      if (!object(entry) || !severities.has(entry.severity) ||
          !(entry.url || entry.source || entry.title)) {
        throw new Error('invalid advisory in npm audit report');
      }
    }
    counts[vulnerability.severity] += 1;
    counts.total += 1;
  }
  for (const [name, count] of Object.entries(counts)) {
    if (report.metadata.vulnerabilities[name] !== count) {
      throw new Error('incomplete or inconsistent npm audit report');
    }
  }
  return report;
};

const risky = (report) => {
  const result = new Map();
  for (const [name, vulnerability] of Object.entries(report.vulnerabilities)) {
    const level = risk(vulnerability.severity);
    if (!level) continue;
    const advisories = vulnerability.via
      .filter((entry) => object(entry) && risk(entry.severity));
    if (advisories.length === 0) {
      result.set(name, level);
    } else {
      for (const entry of advisories) {
        const key = name + ':' + (entry.url || entry.source || entry.title);
        result.set(key, Math.max(result.get(key) || 0, risk(entry.severity)));
      }
    }
  }
  return result;
};

try {
  const base = risky(read(basePath));
  const head = risky(read(headPath));
  const added = [...head]
    .filter(([key, level]) => level > (base.get(key) || 0))
    .map(([key]) => key);
  if (added.length) {
    console.error('New or escalated HIGH/CRITICAL npm audit findings compared with main:');
    for (const item of added) console.error('  ' + item);
    process.exitCode = 1;
  } else {
    console.log(`No new HIGH/CRITICAL npm audit findings compared with main. head=${head.size}, base=${base.size}`);
  }
} catch (error) {
  console.error('Invalid npm audit input:', error.message);
  process.exitCode = 1;
}
