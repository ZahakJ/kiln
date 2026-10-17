# Continuing the build on a new machine or account

Everything needed is in this repository; nothing lives only on the original
laptop any more (the brand kit is in `brand/`, the mockups in `brand/mockups/`
and `docs/design/mockups/`, the plan in `PLAN.md` and `plan/work-packages.json`,
the workflow in this folder). Read [docs/STATUS.md](../../docs/STATUS.md) first.

## 1. Machine

Linux x86_64 is what the build was developed on (Ubuntu 24.04). More CPUs and
RAM help a lot: the original was 4 CPUs / 7.7 GB and could only run two agents
at once. Recommended: 8+ CPUs, 16+ GB RAM, 100 GB free disk (Rust build output
alone reaches 30 GB). With more cores, raise `MAX` in
`kiln-v0-1.workflow.js` (concurrent packages) and `jobs` in `.cargo/config.toml`.

## 2. Accounts and access

- GitHub: push access to `ZahakJ/kiln` over SSH, and `gh` logged in
  (`gh auth login`); agents push to `main` and watch CI with `gh run watch`.
- Git identity used so far: `ZahakJ <77855241+ZahakJ@users.noreply.github.com>`.
  Commits end with a `Co-Authored-By:` line for the model.
- Claude Code with a plan that allows long autonomous runs. The v0.1 build
  used about 21 M subagent tokens for its first 25 packages; expect the rest of
  v0.1 to need at least as much again.

## 3. Toolchain

```sh
git clone git@github.com:ZahakJ/kiln.git ~/OfficeSuite && cd ~/OfficeSuite
bash tools/setup-rust.sh       # Rust 1.99.1, wasm/Android targets, nextest, sccache, wasm-opt, wasm-bindgen, cargo-deny, cargo-about
bash tools/setup-node.sh       # Node 24, pnpm, Playwright Chromium
bash tools/setup-android.sh    # Android SDK + NDK (needed from WP-89)
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev libssl-dev patchelf file pkg-config \
  libreoffice poppler-utils imagemagick ffmpeg openjdk-21-jdk xvfb \
  fonts-liberation fonts-crosextra-carlito fonts-crosextra-caladea
bash tools/verify-env.sh       # reports anything missing
pnpm install
cargo xtask verify && pnpm verify
```

Versions and why each tool is there: [docs/qa/env.md](../../docs/qa/env.md).
The original machine had broken IPv6, so the scripts use `curl -4`; that is
harmless elsewhere. On the original machine root went through `pkexec`.

## 4. Bring back the interrupted work

```sh
git fetch origin
git checkout wip/paused-2026-10-17 -- .   # WP-66, WP-78, WP-87 unfinished files
git reset                                 # keep them as uncommitted changes on main
```

`wip/wp64-leftovers` holds nine leftover edits from WP-64's fix round; compare
them with `main` before reusing any of them.

## 5. Resume

Open Claude Code in the repository and say:

> Resume the Kiln build. Read docs/STATUS.md, tools/orchestrator/README.md and
> tools/orchestrator/CONTEXT.md. Run tools/orchestrator/kiln-v0-1.workflow.js as
> a workflow with tools/orchestrator/resume-args.json as its args. Keep a
> disk/thermal/memory monitor running (tools/orchestrator/devmon.sh, adjust its
> paths), resolve plan-wording failures as the plan owner, and after v0.1 run
> the parity rounds of PLAN.md §7 until done.

The orchestrator (the main Claude session) owns the plan: when a review fails
only because a criterion is impossible as written (as happened with WP-58 and
B5), it amends `PLAN.md` and `plan/work-packages.json`, records the decision in
`docs/STATUS.md`, and relaunches with the finished packages in `completed`.
A failed package skips everything that depends on it, so check the journal of
each run and relaunch quickly.

## 6. Lessons from the first machine

- Two agents compiling at once kept a 4-core laptop at load 8–14; turbo boost
  had to be turned off to stop thermal throttling.
- Disk: prune `target/*/incremental` and build artefacts older than an hour;
  agents sometimes create their own worktrees (`~/OfficeSuite-wp*`) that hold
  another 20 GB target each; remove them when the package is on `main`.
- A Grid test once allocated 5 GB (whole-sheet command materialising every
  cell); watch memory when Grid tests run.
- The usage limit stops every running agent; the workflow retries after a reset
  twice, then marks them "implementer died". Relaunch with the finished list.
- Paths in older plan text mention `/home/muth/...`; read them as the repository
  root (`/home/muth/OfficeSuite`) and `brand/` (`/home/muth/kiln-brand/final`).
