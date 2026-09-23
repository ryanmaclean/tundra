export const meta = {
  name: 'at-health',
  description: 'Full health check of the running at-tundra daemon: API, KPI, agents, tasks, memory',
  phases: [
    { title: 'Probe', detail: 'Hit all key endpoints in parallel' },
    { title: 'Analyse', detail: 'Interpret results and flag anomalies' },
  ],
}

const PROBE_SCHEMA = {
  type: 'object',
  required: ['endpoint', 'status', 'ok', 'latency_ms'],
  properties: {
    endpoint:   { type: 'string' },
    status:     { type: 'number' },
    ok:         { type: 'boolean' },
    latency_ms: { type: 'number' },
    summary:    { type: 'string' },
    error:      { type: 'string' },
  },
}

const BASE = 'http://localhost:9090'

const ENDPOINTS = [
  { path: '/api/status',    label: 'status' },
  { path: '/api/bootstrap', label: 'bootstrap' },
  { path: '/api/kpi',       label: 'kpi' },
  { path: '/api/agents',    label: 'agents' },
  { path: '/api/beads',     label: 'beads' },
  { path: '/api/tasks',     label: 'tasks' },
  { path: '/api/mcp/servers', label: 'mcp-servers' },
  { path: '/api/settings',  label: 'settings' },
]

// ── Probe (parallel) ───────────────────────────────────────────────────────
phase('Probe')
const probes = await parallel(ENDPOINTS.map(ep => () =>
  agent(
    `Probe ${BASE}${ep.path} and return timing + summary.\n\n` +
    `Run:\n` +
    `  START=$(date +%s%3N)\n` +
    `  RESP=$(curl -s -o /tmp/probe_resp -w "%{http_code}" ${BASE}${ep.path} 2>/dev/null)\n` +
    `  END=$(date +%s%3N)\n` +
    `  BODY=$(cat /tmp/probe_resp 2>/dev/null | head -c 400)\n` +
    `  echo "STATUS=$RESP LATENCY=$((END-START)) BODY=$BODY"\n\n` +
    'Parse: endpoint="${ep.path}", status=HTTP code, ok=(status==200), latency_ms=elapsed.\n' +
    'summary: for /api/kpi summarise counts; for /api/agents count active; ' +
    'for /api/beads count by status; otherwise just "ok" or error text.\n' +
    'Return the structured JSON.',
    { label: `probe:${ep.label}`, phase: 'Probe', schema: PROBE_SCHEMA }
  )
))

// ── Analyse ────────────────────────────────────────────────────────────────
phase('Analyse')
const results = probes.filter(Boolean)
const failed  = results.filter(p => !p.ok)
const slow    = results.filter(p => p.latency_ms > 500)

const report = await agent(
  `Generate a concise Markdown health report for the at-tundra daemon.\n\n` +
  `PROBE RESULTS:\n${JSON.stringify(results, null, 2)}\n\n` +
  `SUMMARY:\n` +
  `- ${results.filter(p => p.ok).length}/${results.length} endpoints healthy\n` +
  `- Failed: ${failed.map(p => p.endpoint).join(', ') || 'none'}\n` +
  `- Slow (>500ms): ${slow.map(p => `${p.endpoint} ${p.latency_ms}ms`).join(', ') || 'none'}\n\n` +
  'Format:\n' +
  '## at-tundra Health Report\n' +
  '🟢 / 🔴 overall status line\n' +
  'Table: endpoint | status | latency | summary\n' +
  'Section: Anomalies (if any)\n' +
  'Section: KPI snapshot (from /api/kpi result)',
  { label: 'report', phase: 'Analyse' }
)

return {
  healthy: failed.length === 0,
  endpoints_ok: results.filter(p => p.ok).length,
  endpoints_total: results.length,
  slow_count: slow.length,
  report,
}
