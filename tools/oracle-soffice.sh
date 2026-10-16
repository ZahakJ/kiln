#!/usr/bin/env bash
# Kiln test oracle: convert a document with LibreOffice headless (PLAN.md §4.5).
#
#   tools/oracle-soffice.sh <input-file> <format> <outdir>
#
# Examples:
#   tools/oracle-soffice.sh book.xlsx csv out/      # LibreOffice-recalculated values (formula oracle)
#   tools/oracle-soffice.sh report.docx pdf out/    # reference rendering; then `pdftoppm -r 60 -png`
#
# Uses a dedicated LibreOffice profile (default /tmp/kiln-lo-profile, override with
# KILN_LO_PROFILE) seeded on first run with "always recalculate on load" for OOXML and ODF files,
# so converting .xlsx -> .csv yields values computed by LibreOffice rather than cached ones.
# Conversions are serialised with flock because a LibreOffice profile cannot be shared concurrently.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <input-file> <format> <outdir>" >&2
  exit 64
fi
in="$1" fmt="$2" outdir="$3"
[ -f "$in" ] || { echo "oracle-soffice: input not found: $in" >&2; exit 66; }

SOFFICE="${SOFFICE:-/usr/bin/soffice}"
[ -x "$SOFFICE" ] || { echo "oracle-soffice: LibreOffice not found at $SOFFICE" >&2; exit 69; }

profile="${KILN_LO_PROFILE:-/tmp/kiln-lo-profile}"
xcu="$profile/user/registrymodifications.xcu"
if [ ! -f "$xcu" ] || ! grep -q 'OOXMLRecalcMode' "$xcu"; then
  mkdir -p "$profile/user"
  cat > "$xcu" <<'XCU'
<?xml version="1.0" encoding="UTF-8"?>
<oor:items xmlns:oor="http://openoffice.org/2001/registry" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<item oor:path="/org.openoffice.Office.Calc/Formula/Load"><prop oor:name="OOXMLRecalcMode" oor:op="fuse"><value>0</value></prop></item>
<item oor:path="/org.openoffice.Office.Calc/Formula/Load"><prop oor:name="ODFRecalcMode" oor:op="fuse"><value>0</value></prop></item>
<item oor:path="/org.openoffice.Office.Common/Misc"><prop oor:name="FirstRun" oor:op="fuse"><value>false</value></prop></item>
<item oor:path="/org.openoffice.Office.Common/Misc"><prop oor:name="ShowTipOfTheDay" oor:op="fuse"><value>false</value></prop></item>
</oor:items>
XCU
fi

mkdir -p "$outdir"
abs_in="$(cd "$(dirname "$in")" && pwd)/$(basename "$in")"
abs_out="$(cd "$outdir" && pwd)"

exec flock "$profile.lock" "$SOFFICE" "-env:UserInstallation=file://$profile" --headless --norestore \
  --convert-to "$fmt" --outdir "$abs_out" "$abs_in"
