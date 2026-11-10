#!/usr/bin/env bash
# Download the real-world Office files of the fidelity corpus into
# fixtures/external/<format>/, a gitignored directory. fixtures/corpus-manifest.json lists every
# file with its URL, SHA-256 and licence; sources are pinned to one commit so every machine gets
# the same bytes.
#
#   bash tools/fetch-corpus.sh            download what is missing
#   bash tools/fetch-corpus.sh --force    download everything again
#   bash tools/fetch-corpus.sh --verify   download nothing; check every file's SHA-256
#
# The downloads are data: this script only fetches and hashes them and never runs anything from
# them. A file whose hash differs from the manifest is deleted and counts as a failure.
set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$root/fixtures/corpus-manifest.json"
mode=fetch
case "${1:-}" in
  "") ;;
  --force) mode=force ;;
  --verify) mode=verify ;;
  *) echo "usage: fetch-corpus.sh [--force|--verify]" >&2; exit 64 ;;
esac

ok=0 failed=0
while IFS=$'\t' read -r file url want; do
  out="$root/fixtures/$file"
  if [ "$mode" = verify ]; then
    if [ ! -f "$out" ]; then
      echo "fetch-corpus: missing $file" >&2
      failed=$((failed + 1))
    elif [ "$(sha256sum "$out" | cut -d' ' -f1)" = "$want" ]; then
      ok=$((ok + 1))
    else
      echo "fetch-corpus: SHA-256 mismatch: $file" >&2
      failed=$((failed + 1))
    fi
    continue
  fi
  if [ -s "$out" ] && [ "$mode" = fetch ] && [ "$(sha256sum "$out" | cut -d' ' -f1)" = "$want" ]; then
    ok=$((ok + 1))
    continue
  fi
  mkdir -p "$(dirname "$out")"
  tmp="$out.part"
  if curl -fsSL --retry 2 --connect-timeout 15 --max-time 120 -o "$tmp" "$url" \
    && [ "$(sha256sum "$tmp" | cut -d' ' -f1)" = "$want" ]; then
    mv "$tmp" "$out"
    ok=$((ok + 1))
  else
    rm -f "$tmp"
    echo "fetch-corpus: failed $file (download failed or SHA-256 mismatch)" >&2
    failed=$((failed + 1))
  fi
done < <(jq -r '.files[] | [.file, .url, .sha256] | @tsv' "$manifest")

echo "fetch-corpus: $ok files good, $failed failed"
[ "$failed" = 0 ]
