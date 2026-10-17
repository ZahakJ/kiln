export const meta = {
  name: 'kiln-v0.1-rust',
  description: 'Build Kiln v0.1 on the Rust core (WP-50..WP-98): implement -> adversarial review -> fix, DAG-scheduled, 2 at a time',
  phases: [
    { title: 'Build', detail: 'implement each work package as dependencies complete' },
    { title: 'Review', detail: 'independent reviewer runs acceptance criteria, inspects renders' },
    { title: 'Fix', detail: 'address blocking review issues (max 2 rounds)' },
  ],
}

const WPS = args.wps.map(w => ({ ...w, d: w.d.filter(x => !x.startsWith('path:')) }))
const PRE_DONE = new Set(args.completed || [])
const MAX = 2
const EXCLUSIVE = new Set(['WP-50', 'WP-97', 'WP-98'])
const LAST = new Set(['WP-51'])

const COMMON = `
You are one engineer in an autonomous team building **Kiln** — an open-source (AGPL-3.0-or-later) office suite that replaces Microsoft Office: **Kiln Write** (Word), **Kiln Grid** (Excel), **Kiln Deck** (PowerPoint), **Kiln Notes** (OneNote), **Kiln Home** (hub). Architecture: a native **Rust core** (Cargo workspace of kiln-* crates: models, formula engine, text/layout, Scene display list, file formats, Loro CRDT, storage) running natively via **Tauri 2** (Linux/Windows/macOS/Android) and as **WebAssembly** for the web/PWA; a thin, strictly typed TypeScript/React UI chrome with NO business logic. The user's bar: better than Microsoft Office and every competitor in every way — fidelity, efficiency (enforced performance budgets), and a premium, distinctive look with its own personality (never a cheap Office clone) while Office users feel instantly at home. Production-grade only: no stubs presented as done, real tests, idiomatic Rust (clippy -D warnings clean, no unjustified unsafe, no panics on user input).

Repo: the current working directory, a clone of GitHub ZahakJ/kiln (branch main; the original machine used /home/muth/OfficeSuite). The plan is PLAN.md (~2,500 lines — do NOT read it whole). Read §0, your work package (full JSON in plan/work-packages.json — extract by id with python3/jq), and every PLAN.md section your task references (grep -n '^## \\|^### ' PLAN.md). Always read §4.7 (concurrency, locks, heavy builds) and follow it exactly. The brand kit (visual source of truth for anything user-facing, once imported by WP-51) is in /home/muth/kiln-brand/final/ and later in the repo.

Environment rules:
- Toolchains (installed by WP-50): rustup/cargo in ~/.cargo/bin, Node 24 in ~/.local/opt/node/bin. export PATH="$HOME/.cargo/bin:$HOME/.local/opt/node/bin:$HOME/.local/bin:$PATH". Android SDK/NDK in ~/Android. LibreOffice /usr/bin/soffice is a test oracle only. IPv6 is broken: use curl -4.
- Machine: 4 CPUs, 7 GB RAM, ONE other agent may work in this same tree on another work package. Respect the heavy-build lock and job limits from §4.7. Never use \`pkill -f\` (kill by PID). No sudo; only WP-50 may use pkexec.
- File ownership: edit only what your work package OWNS plus the shared append-only files listed in §4.7. Never revert or rewrite another work package's code. Never use git stash, git reset --hard, git checkout -- ., git clean, or git add without a pathspec.
- Git: identity user.name "ZahakJ", user.email "77855241+ZahakJ@users.noreply.github.com". Conventional commits ending with: Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>. Use the locked git workflow from §4.7 (pull --rebase then push to origin main). After pushing, watch CI (gh run list / gh run watch --exit-status); fix failures your change caused.
- Verify EVERY acceptance criterion by actually running it. For anything visual, render it (kiln-raster PNGs and/or Playwright screenshots of the web build, light + dark, desktop + phone where relevant), LOOK at the images with the Read tool, and iterate until they genuinely match the brand kit mockups and look premium.
- Once the parity tracker exists (WP-59), update rows your work implements.
`

const IMPL_SCHEMA = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['done', 'partial', 'failed'] },
    summary: { type: 'string' },
    acceptance: { type: 'array', items: { type: 'object', properties: { criterion: { type: 'string' }, passed: { type: 'boolean' }, evidence: { type: 'string' } }, required: ['criterion', 'passed', 'evidence'] } },
    commits: { type: 'array', items: { type: 'string' } },
    followups: { type: 'array', items: { type: 'string' } },
    needs_user: { type: 'string', description: 'non-empty only if blocked on something only the human user can do (permission refusal, credentials); describe exactly' },
  },
  required: ['status', 'summary', 'acceptance', 'commits', 'followups'],
}
const REVIEW_SCHEMA = {
  type: 'object',
  properties: {
    verdict: { type: 'string', enum: ['pass', 'pass_with_debt', 'fail'] },
    blocking: { type: 'array', items: { type: 'string' } },
    debt: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
  },
  required: ['verdict', 'blocking', 'debt', 'notes'],
}

function implPrompt(wp) {
  const brandWait = wp.id === 'WP-51' ? `\nFIRST: the brand kit is produced by a separate design team. Wait until /home/muth/kiln-brand/final/BRAND.md and /home/muth/kiln-brand/final/board.png exist AND no file under /home/muth/kiln-brand/final has been modified in the last 10 minutes (poll with short sleeps across several commands; give up after 8 hours and report status failed).` : ''
  return `${COMMON}
## Your assignment: ${wp.id} — ${wp.t}${brandWait}
Dependencies already completed: ${wp.d.join(', ') || 'none'}.
Extract your full task, owned paths and acceptance criteria from plan/work-packages.json (id "${wp.id}") and implement it completely and to a very high standard. Study what dependency work packages built (git log, crates, packages) and build on it. When finished, commit and push, then return the structured report with honest per-criterion results.`
}
function reviewPrompt(wp, impl, round) {
  return `${COMMON}
## You are the INDEPENDENT REVIEWER for ${wp.id} — ${wp.t} (review round ${round}).
Be skeptical: assume nothing works until you have run it yourself. Do NOT modify tracked source files (build/test artifacts are fine). Respect the heavy-build lock.
1. Extract ${wp.id}'s task and acceptance criteria from plan/work-packages.json.
2. Run every acceptance criterion yourself and record results (including performance budgets the WP owns).
3. Read the code the work package added (its commits). Hunt for: stubs/TODOs presented as done, tests that assert nothing or hard-code outputs, unmet parts of the task text, correctness bugs, panics on malformed input, unjustified unsafe, clippy/fmt failures, business logic leaking into the TS chrome, licence problems, edits outside ownership.
4. For anything visual: render and LOOK. Compare against the brand kit mockups (brand/mockups and docs/design/mockups in the repo). Ugly, misaligned, generic-looking or confusing UI is blocking.
5. Check the work is pushed and CI is green.
Implementer's report: ${JSON.stringify(impl).slice(0, 6000)}
Verdict: "pass" = all acceptance met with high quality; "pass_with_debt" = acceptance met, non-blocking improvements in debt; "fail" = any criterion unmet or serious quality problem (each concretely in blocking with paths and repro). If a criterion can only be met by the human user (e.g. a permission refusal), say so explicitly in blocking with the prefix "USER:".`
}
function fixPrompt(wp, review, round) {
  return `${COMMON}
## Fix round ${round} for ${wp.id} — ${wp.t}
An independent reviewer found blocking problems. Fix ALL of them properly, re-run the affected acceptance criteria, commit and push, and return the structured report. Items prefixed "USER:" need the human — do not try to circumvent permission refusals; report them in needs_user.
Blocking issues:
${review.blocking.map((b, i) => `${i + 1}. ${b}`).join('\n')}
Reviewer notes: ${review.notes}
Also address these non-blocking items if cheap: ${review.debt.join(' | ')}`
}

async function runWP(wp) {
  let impl = await agent(implPrompt(wp), { label: `${wp.id} build`, phase: 'Build', schema: IMPL_SCHEMA })
  if (!impl) return { id: wp.id, ok: false, reason: 'implementer died' }
  let review = null
  for (let round = 1; round <= 3; round++) {
    review = await agent(reviewPrompt(wp, impl, round), { label: `${wp.id} review ${round}`, phase: 'Review', schema: REVIEW_SCHEMA })
    if (!review) return { id: wp.id, ok: false, reason: 'reviewer died', impl }
    log(`${wp.id} review ${round}: ${review.verdict} (${review.blocking.length} blocking, ${review.debt.length} debt)`)
    if (review.verdict !== 'fail' || round === 3) break
    if (review.blocking.length && review.blocking.every(b => b.startsWith('USER:'))) break
    const fixed = await agent(fixPrompt(wp, review, round), { label: `${wp.id} fix ${round}`, phase: 'Fix', schema: IMPL_SCHEMA })
    if (fixed) impl = fixed
  }
  const userOnly = review.verdict === 'fail' && review.blocking.length > 0 && review.blocking.every(b => b.startsWith('USER:'))
  // USER-only blockers don't stop dependents: the code is done, only a human action remains
  const ok = review.verdict !== 'fail' || userOnly
  if (userOnly) log(`${wp.id} needs the user: ${review.blocking.join(' | ')}`)
  return { id: wp.id, ok, verdict: review.verdict, blocking: review.blocking, debt: review.debt, summary: impl.summary, followups: impl.followups, needs_user: impl.needs_user || (userOnly ? review.blocking.join(' | ') : '') }
}

const results = {}
const state = {}
for (const wp of WPS) state[wp.id] = PRE_DONE.has(wp.id) ? 'done' : 'pending'
const running = new Map()

while (true) {
  for (const wp of WPS) {
    if (state[wp.id] === 'pending' && wp.d.some(d => state[d] === 'failed' || state[d] === 'skipped')) {
      state[wp.id] = 'skipped'
      log(`${wp.id} skipped: a dependency failed`)
    }
  }
  const ready = WPS.filter(wp => state[wp.id] === 'pending' && wp.d.every(d => state[d] === 'done'))
  // brand-dependent work last among ready items so engine work proceeds while the kit is finished
  ready.sort((a, b) => (LAST.has(a.id) ? 1 : 0) - (LAST.has(b.id) ? 1 : 0))
  const exclusiveRunning = [...running.keys()].some(id => EXCLUSIVE.has(id))
  for (const wp of ready) {
    if (running.size >= MAX || exclusiveRunning) break
    if (EXCLUSIVE.has(wp.id) && running.size > 0) continue
    if (wp.h && [...running.keys()].some(id => WPS.find(x => x.id === id).h)) continue
    if (LAST.has(wp.id) && ready.some(o => !LAST.has(o.id) && state[o.id] === 'pending')) continue
    state[wp.id] = 'running'
    log(`starting ${wp.id}: ${wp.t}`)
    running.set(wp.id, runWP(wp).catch(e => ({ id: wp.id, ok: false, reason: String(e) })))
    if (EXCLUSIVE.has(wp.id)) break
  }
  if (running.size === 0) {
    // nothing running: if WP-51 was deferred only for priority, start it now
    const deferred = ready.filter(wp => state[wp.id] === 'pending')
    if (!deferred.length) break
    const wp = deferred[0]
    state[wp.id] = 'running'
    log(`starting ${wp.id}: ${wp.t}`)
    running.set(wp.id, runWP(wp).catch(e => ({ id: wp.id, ok: false, reason: String(e) })))
  }
  const r = await Promise.race([...running.values()])
  running.delete(r.id)
  results[r.id] = r
  state[r.id] = r.ok ? 'done' : 'failed'
  const nDone = Object.values(state).filter(s => s === 'done').length
  log(`${r.id} ${r.ok ? 'DONE' : 'FAILED'} — ${nDone}/${WPS.length} complete`)
}

return {
  state,
  failed: Object.values(results).filter(r => !r.ok),
  needs_user: Object.values(results).filter(r => r.needs_user).map(r => `${r.id}: ${r.needs_user}`),
  debt: Object.values(results).flatMap(r => (r.debt || []).map(d => `${r.id}: ${d}`)),
  followups: Object.values(results).flatMap(r => (r.followups || []).map(f => `${r.id}: ${f}`)),
}
