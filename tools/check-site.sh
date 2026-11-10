#!/usr/bin/env bash
# Builds target/pages-check/site: the Pages artefact (target/pages/site, `tools/pages-build.sh`)
# with Kiln Grid's cores swapped for check cores (`cargo xtask wasm --journal-check`: every journal
# write is replayed onto a shadow copy of the workbook and compared, and a difference stops the
# core). The e2e project `pwa-check` runs Grid's recovery spec on it. Only the Grid modules
# differ; their bindings are the same, so the release artefact's scripts load them.
#
# Usage: tools/check-site.sh   (rebuilds only when the release site or the sources are newer)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
release=target/pages/site
check=target/pages-check/site
stamp=target/pages-check/.built
[ -d "$release" ] || { echo "check-site: $release is missing; run tools/pages-build.sh" >&2; exit 1; }
if [ -f "$stamp" ] && [ -z "$(find "$release" crates -newer "$stamp" -type f -print -quit)" ]; then
  echo "check-site: $check is up to date"
  exit 0
fi
cargo xtask wasm --release --journal-check --app grid --out-dir target/wasm-check
cargo xtask wasm --release --journal-check --app grid-threads --out-dir target/wasm-check
rm -rf target/pages-check
mkdir -p target/pages-check
cp -r "$release" "$check"
swapped=0
for app in grid grid-threads; do
  shipped="apps/web/src/wasm/$app/kiln_bridge_wasm_bg.wasm"
  for f in "$check"/app/assets/kiln_bridge_wasm_bg-*.wasm; do
    if cmp -s "$f" "$shipped"; then
      cp "target/wasm-check/$app/kiln_bridge_wasm_bg.wasm" "$f"
      swapped=$((swapped + 1))
    fi
  done
done
[ "$swapped" -eq 2 ] || { echo "check-site: swapped $swapped Grid modules, not 2 (is $release built from apps/web/src/wasm?)" >&2; exit 1; }
grep -qa "journal check: the records" "$check"/app/assets/kiln_bridge_wasm_bg-*.wasm \
  || { echo "check-site: no check core in the site" >&2; exit 1; }
touch "$stamp"
echo "check-site: $check (Grid check cores)"
