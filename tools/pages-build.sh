#!/usr/bin/env bash
# Builds the GitHub Pages artefact: the landing page and the manual from docs/, and the web
# app under app/, as https://zahakj.github.io/kiln/ serves them (the site is mounted at /kiln/):
#
#   target/pages/site/              the landing page, the manual and their media from docs/, without
#                                   the engineering notes (adr/, design/, qa/, the top-level .md files)
#   target/pages/site/app/          `vite build --mode pages`: base /kiln/app/, the service worker
#                                   (precache, offline, COOP/COEP for the threads core), the manifest
#
# Then it checks the artefact and the budgets (tools/pages-check.mjs: first-load chrome JS < 450 KB
# gzipped, an app window's scripts < 820 KB, each wasm module < 4,500 KB gzipped, the manifest
# installable, the worker in place) and writes
# target/pages/report.json.
#
# Usage: tools/pages-build.sh [--wasm] [--lighthouse]
#   --wasm        build the WebAssembly modules first (`cargo xtask wasm --release`); without it the
#                 modules in apps/web/src/wasm/ must exist (CI builds them in an earlier step)
#   --lighthouse  also run Lighthouse against the local Pages emulation (e2e/perf/lighthouse.mjs:
#                 4G, installability, time to interactive) and fail outside the budgets
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
wasm=0
lighthouse=0
for arg in "$@"; do
  case "$arg" in
    --wasm) wasm=1 ;;
    --lighthouse) lighthouse=1 ;;
    *) echo "pages-build: unknown option $arg" >&2; exit 2 ;;
  esac
done

if [ "$wasm" = 1 ]; then
  cargo xtask wasm --release
fi
for module in write/kiln_bridge_wasm grid/kiln_bridge_wasm deck/kiln_bridge_wasm \
  grid-threads/kiln_bridge_wasm collab/kiln_collab_wasm; do
  if [ ! -f "apps/web/src/wasm/${module}_bg.wasm" ]; then
    echo "pages-build: apps/web/src/wasm/${module}_bg.wasm is missing; run cargo xtask wasm --release (or pass --wasm)" >&2
    exit 1
  fi
done
# The page's numbers match their reports, and every internal link leads somewhere.
cargo xtask site --check
python3 tools/check-links.py

# B12 on the modules as built (fails when one reaches its budget).
cargo xtask wasm --release --size

echo "pages-build: building the web app under /kiln/app/"
rm -rf apps/web/dist-pages
pnpm --filter @kiln/web run build:pages

out="target/pages"
site="$out/site"
rm -rf "$out"
mkdir -p "$site"
cp -r docs/. "$site/"
rm -rf "$site/adr" "$site/qa" "$site/design" "$site"/*.md
mkdir -p "$site/app"
cp -r apps/web/dist-pages/. "$site/app/"

node tools/pages-check.mjs "$site"
# Vite's manifest only served the check; source maps stay for debugging the deployed site.
rm -rf "$site/app/.vite"

if [ "$lighthouse" = 1 ]; then
  node e2e/perf/lighthouse.mjs --site "$site"
fi
echo "pages-build: $site ($(du -sh "$site" | cut -f1))"
