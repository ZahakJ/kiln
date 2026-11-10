#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Builds the bundled templates: runs every tools/templates/*.jsonl through `kiln-cli control`,
# which writes assets/templates/<app>/<name>.kwrite|.kgrid|.kdeck, then renders each thumbnail
# (assets/templates/<app>/<name>.png) with `kiln-cli render`. Kiln Write's thumbnails are
# rendered by their scripts, in the same session, with spell checking off.
#
#   tools/templates/build.sh                          # builds kiln-cli with cargo
#   KILN_CLI=target/debug/kiln-cli tools/templates/build.sh
#
# Regenerate the scripts after changing the content with `python3 tools/templates/make.py`.
set -euo pipefail

cd "$(dirname "$0")/../.."
if [[ -n "${KILN_CLI:-}" ]]; then
  read -r -a cli <<<"$KILN_CLI"
else
  cli=(cargo run -q -p kiln-cli --)
fi

mkdir -p assets/templates/write assets/templates/grid assets/templates/deck target/templates-tmp
failed=0
for script in tools/templates/*.jsonl; do
  name=$(basename "$script" .jsonl)
  app=${name%%-*}
  stem=${name#*-}
  out=$("${cli[@]}" control <"$script")
  if grep -q '"ok":false' <<<"$out"; then
    echo "error: $script" >&2
    grep '"ok":false' <<<"$out" | head -5 >&2
    failed=1
    continue
  fi
  case "$app" in
    write) ;;
    deck) "${cli[@]}" render "assets/templates/deck/$stem.kdeck" --png "assets/templates/deck/$stem.png" \
            --page 1 --dpi 48 >/dev/null ;;
    grid) "${cli[@]}" render "assets/templates/grid/$stem.kgrid" --png "assets/templates/grid/$stem.png" \
            --page 1 --dpi 64 --no-headings --crop 640x480 >/dev/null ;;
    *) echo "error: $script: unknown app '$app'" >&2; failed=1; continue ;;
  esac
  echo "built $app/$stem"
done
rm -rf target/templates-tmp
exit "$failed"
