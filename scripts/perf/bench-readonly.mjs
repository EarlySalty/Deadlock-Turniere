#!/usr/bin/env node
// Bounded, read-only production benchmark for the DDC tournament API.
// Only fixed first-party GET endpoints; no arbitrary URLs, writes or stress spikes.
// Usage: node scripts/perf/bench-readonly.mjs [local|public|staging|both]
// Measure total response duration on warmed keep-alive connections; cold TLS is separate.
import http from 'node:http'
import https from 'node:https'
import { performance } from 'node:perf_hooks'

const targets = {
  local: 'http://127.0.0.1:8900',
  staging: 'http://127.0.0.1:8901',
  public: 'https://deutsche-deadlock-community.de/turnier',
}
const routes = [
  ['health', '/api/health'],
  ['heroes', '/api/draft/heroes'],
  ['tournament-17', '/api/tournaments/17'],
]
const stages = [
  { concurrency: 1, requests: 24 },
  { concurrency: 4, requests: 56 },
  { concurrency: 8, requests: 96 },
]
const choice = process.argv[2] || 'both'
if (!['both', 'local', 'public', 'staging'].includes(choice)) {
  console.error('Usage: node scripts/perf/bench-readonly.mjs [local|public|staging|both]')
  process.exit(2)
}
const chosen = choice === 'both' ? ['local', 'public'] : [choice]
const results = []
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const quantile = (sorted, q) => sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * q) - 1)]
const round = n => Math.round(n * 10) / 10

function measure(url, agent) {
  return new Promise(resolve => {
    const start = performance.now()
    const module = url.protocol === 'https:' ? https : http
    let finished = false
    const finish = (sample) => {
      if (!finished) {
        finished = true
        resolve({ ...sample, totalMs: round(performance.now() - start) })
      }
    }
    const req = module.request(url, {
      agent, method: 'GET', headers: { Accept: 'application/json' },
      timeout: 3500,
    }, res => {
      const headMs = round(performance.now() - start)
      let bytes = 0
      res.on('data', chunk => { bytes += chunk.length })
      res.on('end', () => finish({
        status: res.statusCode, headMs, bytes,
        serverTiming: String(res.headers['server-timing'] || ''),
        reusedSocket: req.reusedSocket,
      }))
      res.on('error', e => finish({ error: e.code || e.message }))
    })
    req.on('timeout', () => req.destroy(new Error('timeout')))
    req.on('error', e => finish({ error: e.code || e.message }))
    req.end()
  })
}
function summarize(target, route, stage, samples, elapsed) {
  const ok = samples.filter(s => s.status === 200)
  const sorted = ok.map(s => s.totalMs).sort((a, b) => a - b)
  const ttfbs = ok.map(s => s.headMs).sort((a,b) => a - b)
  const bad = samples.filter(s => s.status !== 200)
  return {
    target, route, concurrency: stage.concurrency, requests: stage.requests,
    ok: ok.length, errors: bad.length,
    p50: sorted.length ? quantile(sorted,.5) : null,
    p95: sorted.length ? quantile(sorted,.95) : null,
    p99: sorted.length ? quantile(sorted,.99) : null,
    ttfbP95: ttfbs.length ? quantile(ttfbs,.95) : null,
    over50: ok.filter(s => s.totalMs > 50).length,
    over100: ok.filter(s => s.totalMs > 100).length,
    over300: ok.filter(s => s.totalMs > 300).length,
    rate: round(samples.length / (elapsed / 1000)),
    serverTiming: ok.find(s => s.serverTiming)?.serverTiming || '',
    sampleErrors: bad.slice(0, 3),
  }
}
for (const target of chosen) {
  const base = targets[target]
  const agent = target === 'public'
    ? new https.Agent({ keepAlive: true, maxSockets: 8, maxFreeSockets: 8 })
    : new http.Agent({ keepAlive: true, maxSockets: 8, maxFreeSockets: 8 })
  for (const [name, path] of routes) {
    const url = new URL(base + path)
    for (let i = 0; i < 8; i++) await measure(url, agent)
    for (const stage of stages) {
      const samples = []
      let cursor = 0
      let stop = false
      const started = performance.now()
      async function worker() {
        while (!stop) {
          const i = cursor++
          if (i >= stage.requests) return
          const result = await measure(url, agent)
          samples.push(result)
          if (result.status !== 200) {
            if (samples.filter(x => x.status !== 200).length >= 3) stop = true
          }
        }
      }
      await Promise.all(Array.from({ length: stage.concurrency }, worker))
      const summary = summarize(target, name, stage, samples, performance.now() - started)
      results.push(summary)
      console.log(JSON.stringify(summary))
      if (summary.errors || (summary.p95 !== null && summary.p95 > 300)) {
        console.error('Stopping for errors: or p95 >300 ms: ' + target + ' ' + name)
        agent.destroy()
        process.exit(1)
      }
      await sleep(200)
    }
  }
  agent.destroy()
}
console.log('FINAL_REPORT=' + JSON.stringify({ date: new Date().toISOString(), results }))
