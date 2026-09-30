export const meta = {
  name: 'bead-audit',
  description: 'Audit all backlog beads for quality: title clarity, description completeness, lane assignment',
  phases: [
    { title: 'Fetch', detail: 'Pull current bead list from at-bridge' },
    { title: 'Audit', detail: 'Parallel quality review per bead' },
    { title: 'Report', detail: 'Summarise issues and emit patch suggestions' },
  ],
}

const AUDIT_SCHEMA = {
  type: 'object',
  required: ['id', 'verdict', 'issues'],
  properties: {
    id:          { type: 'string' },
    verdict:     { type: 'string', enum: ['ok', 'needs-update'] },
    issues:      { type: 'array', items: { type: 'string' } },
    suggested_title:       { type: 'string' },
    suggested_description: { type: 'string' },
  },
}

// ── Fetch ──────────────────────────────────────────────────────────────────
phase('Fetch')
const beadsRaw = await agent(
  'Run: curl -s http://localhost:9090/api/beads 2>/dev/null || echo "[]"\n' +
  'Return the raw JSON string exactly — do not summarise.',
  { label: 'fetch:beads', phase: 'Fetch' }
)

let beads
try {
  beads = JSON.parse(beadsRaw.trim())
} catch {
  beads = []
}

const targets = beads.filter(b => ['backlog', 'hooked'].includes(b.status))
log(`Auditing ${targets.length} active beads (${beads.length} total)`)

if (targets.length === 0) {
  return { status: 'no-beads', message: 'No backlog/hooked beads to audit.' }
}

// ── Audit (parallel) ───────────────────────────────────────────────────────
phase('Audit')
const audits = await pipeline(
  targets,
  bead => agent(
    `Audit this auto-tundra bead for quality:\n${JSON.stringify(bead, null, 2)}\n\n` +
    'Evaluate:\n' +
    '1. Title: is it a clear, action-oriented verb phrase? (e.g. "Add OAuth2 login" not "login stuff")\n' +
    '2. Description: does it exist and explain WHY, not just WHAT?\n' +
    '3. Priority: does the priority (1=highest) feel right for a kanban card?\n' +
    '4. Lane: Standard vs Critical — is it appropriate?\n\n' +
    'If all good → verdict: "ok". Otherwise → verdict: "needs-update" with specific issues array\n' +
    'and suggested_title / suggested_description (only the fields that need changing).',
    { label: `audit:${bead.id?.slice(0, 8)}`, phase: 'Audit', schema: AUDIT_SCHEMA }
  )
)

// ── Report ─────────────────────────────────────────────────────────────────
phase('Report')
const issues = audits.filter(Boolean).filter(a => a.verdict === 'needs-update')
const clean  = audits.filter(Boolean).filter(a => a.verdict === 'ok').length

const report = await agent(
  `Produce a concise Markdown bead quality report.\n\n` +
  `RESULTS:\n` +
  `- Clean: ${clean}/${targets.length} beads passed\n` +
  `- Need updates: ${JSON.stringify(issues, null, 2)}\n\n` +
  'Format:\n' +
  '## Bead Quality Audit\n' +
  '**N/M beads need attention**\n\n' +
  'For each issue: bead ID (first 8 chars), current title → suggested title, bullet list of issues.\n' +
  'End with: "To apply: POST /api/beads/{id} with the suggested fields."',
  { label: 'report', phase: 'Report' }
)

return { clean, issues: issues.length, report }
