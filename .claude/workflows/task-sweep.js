export const meta = {
  name: 'task-sweep',
  description: 'Find and triage stale, stuck, or orphaned tasks — tasks that have not progressed in >24h',
  phases: [
    { title: 'Fetch', detail: 'Pull tasks and beads from at-bridge' },
    { title: 'Triage', detail: 'Classify each stuck task in parallel' },
    { title: 'Report', detail: 'Produce actionable remediation list' },
  ],
}

const TRIAGE_SCHEMA = {
  type: 'object',
  required: ['task_id', 'classification', 'recommendation'],
  properties: {
    task_id:        { type: 'string' },
    title:          { type: 'string' },
    phase:          { type: 'string' },
    hours_stuck:    { type: 'number' },
    classification: {
      type: 'string',
      enum: ['stuck-coding', 'stuck-qa', 'stuck-planning', 'orphaned', 'rate-limited', 'normal'],
    },
    recommendation: { type: 'string' },
    action:         { type: 'string', enum: ['restart', 'escalate', 'archive', 'monitor', 'none'] },
  },
}

// ── Fetch ──────────────────────────────────────────────────────────────────
phase('Fetch')
const [tasksRaw, beadsRaw] = await parallel([
  () => agent(
    'Run: curl -s http://localhost:9090/api/tasks 2>/dev/null || echo "{}"\n' +
    'Return the raw JSON.',
    { label: 'fetch:tasks', phase: 'Fetch' }
  ),
  () => agent(
    'Run: curl -s http://localhost:9090/api/beads 2>/dev/null || echo "[]"\n' +
    'Return the raw JSON.',
    { label: 'fetch:beads', phase: 'Fetch' }
  ),
])

let tasks = [], beads = []
try { const t = JSON.parse(tasksRaw.trim()); tasks = Array.isArray(t) ? t : Object.values(t) } catch {}
try { beads = JSON.parse(beadsRaw.trim()) } catch {}

const beadMap = Object.fromEntries((beads || []).map(b => [b.id, b]))

// Find tasks that haven't been updated in >24h and aren't complete/failed
const now = Date.now()
const stale = tasks.filter(t => {
  if (['complete', 'failed', 'cancelled'].includes(t.phase)) return false
  const updated = t.updated_at ? new Date(t.updated_at).getTime() : 0
  return updated > 0 && (now - updated) > 24 * 60 * 60 * 1000
})

log(`Found ${stale.length} potentially stuck tasks out of ${tasks.length} total`)

if (stale.length === 0) {
  return { status: 'all-clear', message: 'No stuck tasks found. All tasks are progressing normally.' }
}

// ── Triage (parallel) ──────────────────────────────────────────────────────
phase('Triage')
const triaged = await pipeline(
  stale,
  task => {
    const bead = beadMap[task.bead_id] || {}
    const hoursStuck = Math.round((now - new Date(task.updated_at).getTime()) / 3600000)
    return agent(
      `Triage this stuck at-tundra task:\n` +
      `Task: ${JSON.stringify(task, null, 2)}\n` +
      `Parent bead: ${JSON.stringify(bead, null, 2)}\n` +
      `Hours without update: ${hoursStuck}\n\n` +
      'Classify as one of:\n' +
      '- stuck-coding: in coding phase, no progress\n' +
      '- stuck-qa: QA review loop not completing\n' +
      '- stuck-planning: planning/spec stage stalled\n' +
      '- rate-limited: likely hit API rate limit and paused\n' +
      '- orphaned: bead no longer exists or is archived\n' +
      '- normal: might be slow but not actually stuck\n\n' +
      'Provide a one-line recommendation and suggested action (restart/escalate/archive/monitor/none).',
      { label: `triage:${task.id?.slice(0, 8)}`, phase: 'Triage', schema: TRIAGE_SCHEMA }
    )
  }
)

// ── Report ─────────────────────────────────────────────────────────────────
phase('Report')
const actionable = triaged.filter(Boolean).filter(t => t.action !== 'none')
const summary = await agent(
  `Write a Markdown task sweep report.\n\n` +
  `TRIAGE RESULTS:\n${JSON.stringify(triaged.filter(Boolean), null, 2)}\n\n` +
  'Format:\n' +
  '## Task Sweep Report\n' +
  `**${stale.length} tasks swept, ${actionable.length} need action**\n\n` +
  '### Action Required\n' +
  'For each actionable task: task ID, title, phase, hours stuck, classification, recommendation.\n' +
  'Group by action type (restart / escalate / archive).\n\n' +
  '### To restart a stuck task:\n' +
  '`curl -X POST http://localhost:9090/api/tasks/{id}/execute`\n\n' +
  '### To archive:\n' +
  '`curl -X POST http://localhost:9090/api/tasks/{id}/archive`',
  { label: 'report', phase: 'Report' }
)

return { stale: stale.length, actionable: actionable.length, triaged: triaged.filter(Boolean), summary }
