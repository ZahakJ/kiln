#!/usr/bin/env bash
# Download real-world Office files for fidelity testing (PLAN.md §5) into
# fixtures/external/<format>/ — a gitignored directory; nothing here is committed.
#
#   bash tools/fetch-corpus.sh [--force]
#
# Source: the Apache POI test corpus (test-data/, Apache License 2.0), pinned to one commit so every
# machine downloads the same bytes. These are real files written by Word, Excel and PowerPoint (plus a
# few by other producers, and one ISO 29500 Strict workbook) collected by the POI project from bug
# reports. Files that fail to download are skipped with a warning; the script exits 0 as long as at
# least one file per format arrived.
set -uo pipefail

POI_COMMIT="ebf602f1bc748f9ec9b29553070ff62170e3243b"
BASE="https://raw.githubusercontent.com/apache/poi/${POI_COMMIT}/test-data"

root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/fixtures/external"
force=0
[ "${1:-}" = "--force" ] && force=1

DOCX=(
  document/SampleDoc.docx
  document/Styles.docx
  document/Headers.docx
  document/footnotes.docx
  document/endnotes.docx
  document/Numbering.docx
  document/ComplexNumberedLists.docx
  document/FieldCodes.docx
  document/deep-table-cell.docx
  document/headerPic.docx
)
XLSX=(
  spreadsheet/SampleSS.xlsx
  spreadsheet/SampleSS.strict.xlsx
  spreadsheet/Formatting.xlsx
  spreadsheet/styles.xlsx
  spreadsheet/ExcelTables.xlsx
  spreadsheet/WithChart.xlsx
  spreadsheet/WithConditionalFormatting.xlsx
  spreadsheet/comments.xlsx
  spreadsheet/shared_formulas.xlsx
  spreadsheet/Themes.xlsx
)
PPTX=(
  slideshow/SampleShow.pptx
  slideshow/sample.pptx
  slideshow/shapes.pptx
  slideshow/layouts.pptx
  slideshow/backgrounds.pptx
  slideshow/table_test.pptx
  slideshow/bar-chart.pptx
  slideshow/SmartArt.pptx
  slideshow/with_japanese.pptx
  slideshow/pptx2svg.pptx
)

fetch_set() {
  local fmt="$1"
  shift
  local dir="$dest/$fmt" ok=0 failed=0
  mkdir -p "$dir"
  for rel in "$@"; do
    local name out tmp
    name="$(basename "$rel")"
    out="$dir/poi-$name"
    if [ -s "$out" ] && [ "$force" = 0 ]; then
      ok=$((ok + 1))
      continue
    fi
    tmp="$out.part"
    if curl -fsSL --retry 2 --connect-timeout 15 --max-time 120 -o "$tmp" "$BASE/$rel" && [ "$(head -c 2 "$tmp")" = "PK" ]; then
      mv "$tmp" "$out"
      ok=$((ok + 1))
    else
      rm -f "$tmp"
      echo "fetch-corpus: skipped $rel (download failed or not a zip)" >&2
      failed=$((failed + 1))
    fi
  done
  echo "$fmt: $ok files in $dir${failed:+ ($failed skipped)}"
  [ "$ok" -gt 0 ]
}

status=0
fetch_set docx "${DOCX[@]}" || status=1
fetch_set xlsx "${XLSX[@]}" || status=1
fetch_set pptx "${PPTX[@]}" || status=1
exit "$status"
