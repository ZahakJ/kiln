#!/usr/bin/env bash
# Shows that pwa-chaos catches a planted bug of the recovery reviews in the web store
# (apps/web/src/host/store.ts, `VITE_KILN_PLANT`): review 3's W0, a write-ahead tail that drops the
# copies of records (`tail-drops-records`, the default), review 4's W1, a tail whose last record
# entry is lost when it is read (`tail-loses-last`), or round 6's W2, a record's copy taken off the
# tail as soon as its write starts (`tail-removed-early`). Builds the web
# app with the plant into target/pages-planted/site (the WebAssembly modules as they are in
# apps/web/src/wasm/), serves it on a port of its own, runs pwa-chaos against it and succeeds only
# when the spec fails. Normal builds never have the plant.
#
# Usage: tools/chaos-planted.sh [seeds] [plant]   (16 seeds and tail-drops-records by default;
#        KILN_RANDOM_FIRST_SEED sets the first seed)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
seeds="${1:-16}"
plant="${2:-tail-drops-records}"

rm -rf apps/web/dist-pages-planted target/pages-planted
VITE_KILN_PLANT="$plant" pnpm --filter @kiln/web exec vite build --mode pages \
  --outDir dist-pages-planted
mkdir -p target/pages-planted/site
cp -r docs/. target/pages-planted/site/
rm -rf target/pages-planted/site/adr target/pages-planted/site/qa target/pages-planted/site/design
rm -f target/pages-planted/site/*.md
mkdir -p target/pages-planted/site/app
cp -r apps/web/dist-pages-planted/. target/pages-planted/site/app/
rm -rf apps/web/dist-pages-planted
grep -rq "$plant" target/pages-planted/site/app/assets \
  || { echo "chaos-planted: the plant is not in the build" >&2; exit 2; }

cd e2e
set +e
KILN_PAGES_PORT=4181 KILN_PAGES_ROOT=target/pages-planted/site KILN_RANDOM_SEEDS="$seeds" \
  pnpm exec playwright test --project=pwa specs/pwa-chaos.spec.ts --workers=1 \
  --reporter=line
status=$?
set -e
if [ "$status" -eq 0 ]; then
  echo "chaos-planted: pwa-chaos passed on the planted build: it does not see the tail" >&2
  exit 1
fi
echo "chaos-planted: pwa-chaos failed on the planted build, as it must"
