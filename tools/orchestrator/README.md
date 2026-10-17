# Orchestrator

How the Kiln build is driven by Claude Code agents. Start with
[docs/STATUS.md](../../docs/STATUS.md).

- `kiln-v0-1.workflow.js`: the Claude Code workflow that schedules the v0.1
  work packages (dependency order, two at a time, build → review → up to two
  fix rounds). It needs `args.wps` (the package list with dependencies, `d`,
  and the flags `g` = gate/run alone and `h` = heavy) and `args.completed`.
- `resume-args.json`: the packages finished when the build paused on
  2026-10-17. WP-66, WP-71, WP-78 and WP-87 were interrupted; their files are
  on the branch `wip/paused-2026-10-17`.
- `devmon.sh`: the laptop health monitor used during the build (disk pruning of
  Rust build output below 20 GB free, thermal-throttle, memory and swap alerts).

## Resuming in a new Claude Code session

1. `cd ~/OfficeSuite`, then bring back the interrupted work:
   `git checkout wip/paused-2026-10-17 -- .` and `git reset` (files stay
   modified in the tree, `main` stays checked out).
2. Free disk space first: `rm -rf target/debug/incremental` and remove stale
   worktrees (`git worktree list`).
3. In Claude Code, say: *"Resume the Kiln build: read docs/STATUS.md and
   tools/orchestrator/README.md, then run tools/orchestrator/kiln-v0-1.workflow.js
   as a workflow with resume-args.json as its args. Keep the laptop monitor running and continue
   with the parity rounds of PLAN.md §7 after v0.1."*
