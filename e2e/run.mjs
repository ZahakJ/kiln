// Runs the suite, then the timing budgets on their own: a failure in the first never skips the
// budgets, and the exit code reports both. Arguments go to each run; with --project, one run.
import { spawnSync } from 'node:child_process';

const args = process.argv.slice(2);
const run = (extra) =>
  spawnSync('playwright', ['test', ...extra, ...args], {
    stdio: 'inherit',
    shell: process.platform === 'win32',
  }).status ?? 1;

if (args.some((a) => a === '--project' || a.startsWith('--project='))) process.exit(run([]));
// The desktop suite, then the phone and tablet specs in their own projects, and the
// installable web app against the Pages artefact, and Grid's recovery on check cores.
const suite = run([
  '--project=chromium',
  '--project=pixel-7',
  '--project=ipad',
  '--project=pwa',
  '--project=pwa-check',
]);
const budgets = run(['--project=budgets', '--pass-with-no-tests']);
// The timed budgets measure frame intervals and open times in the browser. On a shared CI runner
// they measure the runner as much as Kiln, so there (KILN_BUDGETS_REPORT_ONLY=1) they are reported
// and do not fail the run; on a known machine, as for a release, they are the gate.
if (budgets && process.env.KILN_BUDGETS_REPORT_ONLY === '1') {
  console.log('::warning::e2e budgets missed on this runner (reported only; see the budgets results above)');
  process.exit(suite);
}
process.exit(suite || budgets);
