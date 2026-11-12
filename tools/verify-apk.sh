#!/usr/bin/env bash
# Checks an Android APK of Kiln without a device:
#
#   badging    aapt2 dump badging: package io.github.zahakj.kiln, label Kiln, minSdk 26,
#              targetSdk 36, a version code from the suite's version, the ABI's native code
#   manifest   aapt2 dump xmltree: VIEW and EDIT by MIME type and by extension, SEND by MIME type,
#              for every document type the desktop registers; adjustResize; the FileProvider
#   signature  apksigner verify --print-certs (v2 or v3 scheme); says whether the key is the
#              release key or the debug key; with --release, the debug key fails
#   contents   unzip -l: lib/<abi>/libkiln_desktop.so, the adaptive launcher icon (foreground,
#              background, monochrome), the web interface embedded in the library (index.html)
#   size       under 40 MB (B13, arm64)
#
# Usage: tools/verify-apk.sh [--release] [apk]   (default: the newest release APK under gen/android)
#   --release   the APK is for a release: a debug signer (the "Android Debug" certificate) fails
# Needs the Android build tools (aapt2, apksigner: ANDROID_HOME) and Java for apksigner.
# Prints every result, then exits 1 if any check failed.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
project="$root/apps/desktop/src-tauri/gen/android"
release=0
if [ "${1:-}" = "--release" ]; then
  release=1
  shift
fi
apk="${1:-}"
if [ -z "$apk" ]; then
  apk="$(find "$project/app/build/outputs/apk" -name '*-release.apk' -printf '%T@ %p\n' 2>/dev/null \
    | sort -nr | head -n1 | cut -d' ' -f2-)"
fi
[ -n "$apk" ] && [ -f "$apk" ] || {
  echo "verify-apk: no APK (build one with: pnpm tauri android build --apk --target aarch64 --split-per-abi)"
  exit 1
}

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
tools="$(find "$sdk/build-tools" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n1)"
aapt2="$tools/aapt2"
apksigner="$tools/apksigner"
[ -x "$aapt2" ] && [ -x "$apksigner" ] || { echo "verify-apk: no build tools under $sdk/build-tools"; exit 1; }
command -v unzip >/dev/null || { echo "verify-apk: unzip is missing"; exit 1; }

# The document types (apps/desktop/src-tauri/src/files.rs) and their extensions.
types=(
  docx:application/vnd.openxmlformats-officedocument.wordprocessingml.document
  dotx:application/vnd.openxmlformats-officedocument.wordprocessingml.template
  docm:application/vnd.ms-word.document.macroEnabled.12
  odt:application/vnd.oasis.opendocument.text
  kwrite:application/x-kiln-write
  xlsx:application/vnd.openxmlformats-officedocument.spreadsheetml.sheet
  xltx:application/vnd.openxmlformats-officedocument.spreadsheetml.template
  xlsm:application/vnd.ms-excel.sheet.macroEnabled.12
  csv:text/csv
  ods:application/vnd.oasis.opendocument.spreadsheet
  kgrid:application/x-kiln-grid
  pptx:application/vnd.openxmlformats-officedocument.presentationml.presentation
  potx:application/vnd.openxmlformats-officedocument.presentationml.template
  pptm:application/vnd.ms-powerpoint.presentation.macroEnabled.12
  odp:application/vnd.oasis.opendocument.presentation
  kdeck:application/x-kiln-deck
)

fail=0
ok() { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*"; fail=1; }

size=$(wc -c <"$apk")
echo "verify-apk: $apk ($(( (size + 1048575) / 1048576 )) MB)"

# ----- badging ---------------------------------------------------------------------------------
echo "badging"
badging="$("$aapt2" dump badging "$apk")"
grep -q "^package: name='io.github.zahakj.kiln'" <<<"$badging" \
  && ok "package io.github.zahakj.kiln" || bad "package is not io.github.zahakj.kiln"
version="$(sed -n "s/^package: .*versionCode='\([0-9]*\)'.*versionName='\([^']*\)'.*/\1 \2/p" <<<"$badging")"
[ -n "$version" ] && ok "version code ${version% *}, name ${version#* }" || bad "no version"
grep -q "^application-label:'Kiln'" <<<"$badging" && ok "label Kiln" || bad "label is not Kiln"
grep -q "^minSdkVersion:'26'" <<<"$badging" && ok "minSdk 26" || bad "minSdk is not 26"
grep -q "^targetSdkVersion:'36'" <<<"$badging" && ok "targetSdk 36" || bad "targetSdk is not 36"
abis="$(sed -n "s/^native-code: //p" <<<"$badging" | tr -d "'")"
[ -n "$abis" ] && ok "native code: $abis" || bad "no native code"
grep -q "^launchable-activity: name='io.github.zahakj.kiln.MainActivity'" <<<"$badging" \
  && ok "launcher activity io.github.zahakj.kiln.MainActivity" || bad "no launcher activity"

# ----- manifest --------------------------------------------------------------------------------
echo "manifest"
xml="$("$aapt2" dump xmltree --file AndroidManifest.xml "$apk")"
# One line per intent filter: its actions and categories, MIME types and path patterns.
filters="$(awk '
  function value(line) { match(line, /"[^"]*"/); return substr(line, RSTART + 1, RLENGTH - 2) }
  /E: intent-filter/ { if (f != "") print f " "; f = "filter"; next }
  /E: (activity|activity-alias|provider|application|service|receiver)/ { if (f != "") print f " "; f = "" }
  f != "" && /android:name\(/ { f = f " name=" value($0) }
  f != "" && /android:mimeType\(/ { f = f " mime=" value($0) }
  f != "" && /android:pathPattern\(/ { f = f " path=" value($0) }
  END { if (f != "") print f " " }' <<<"$xml")"
# The filters with `action`, and of those the ones that have `what` (a "mime=..." or "path=...").
takes() { grep -F -- " name=android.intent.action.$1 " <<<"$filters" | grep -F -q -- " $2 "; }
missing=0
for t in "${types[@]}"; do
  ext="${t%%:*}" mime="${t#*:}"
  for action in VIEW EDIT SEND; do
    takes "$action" "mime=$mime" || { bad "$action does not take $mime"; missing=1; }
  done
  pattern="$(grep -F -- " name=android.intent.action.VIEW " <<<"$filters" | grep -o -- " path=[^ ]*\.$ext " | head -n1)"
  [ -n "$pattern" ] || { bad "VIEW does not take .$ext by extension"; missing=1; }
done
[ "$missing" = 0 ] && ok "VIEW, EDIT and SEND take all ${#types[@]} document types (by MIME type; VIEW and EDIT also by extension)"
# windowSoftInputMode: the adjust bits (0xf0) say adjustResize (0x10).
soft="$(sed -n 's/.*android:windowSoftInputMode([^)]*)=\(0x[0-9a-fA-F]*\).*/\1/p' <<<"$xml" | head -n1)"
[ -n "$soft" ] && [ $(( soft & 0xf0 )) -eq $(( 0x10 )) ] && ok "windowSoftInputMode adjustResize" \
  || bad "windowSoftInputMode is not adjustResize"
grep -q 'androidx.core.content.FileProvider' <<<"$xml" && ok "FileProvider for print and share" \
  || bad "no FileProvider"
grep -q 'io.github.zahakj.kiln.fileprovider' <<<"$xml" && ok "FileProvider authority io.github.zahakj.kiln.fileprovider" \
  || bad "FileProvider authority"

# ----- signature -------------------------------------------------------------------------------
echo "signature"
if certs="$("$apksigner" verify --verbose --print-certs "$apk" 2>&1)"; then
  grep -q 'Verified using v2 scheme (APK Signature Scheme v2): true\|Verified using v3 scheme (APK Signature Scheme v3): true' <<<"$certs" \
    && ok "signed (APK signature scheme v2/v3)" || bad "no v2 or v3 signature"
  dn="$(sed -n 's/^Signer #1 certificate DN: //p' <<<"$certs")"
  sha="$(sed -n 's/^Signer #1 certificate SHA-256 digest: //p' <<<"$certs")"
  case "$dn" in
    *"Android Debug"*)
      if [ "$release" = 1 ]; then
        bad "signer: the debug key ($dn); a release is signed with the release key (KILN_KEYSTORE, docs/RELEASING.md)"
      else
        ok "signer: the debug key ($dn); set KILN_KEYSTORE for a release (docs/RELEASING.md)"
      fi
      ;;
    *) ok "signer: $dn" ;;
  esac
  [ -n "$sha" ] && ok "certificate SHA-256 $sha"
else
  bad "apksigner verify failed: $(head -n3 <<<"$certs")"
fi

# ----- contents --------------------------------------------------------------------------------
echo "contents"
listing="$(unzip -l "$apk")"
for abi in $abis; do
  lib="lib/$abi/libkiln_desktop.so"
  if grep -q " $lib\$" <<<"$listing"; then
    ok "$lib ($(( $(awk -v l="$lib" '$4 == l { print $1 }' <<<"$listing") / 1048576 )) MB unpacked)"
    # Tauri embeds the web interface in the library (compressed), keyed by its paths.
    # (grep -c reads the whole library: no early exit for pipefail to report.)
    if [ "$(unzip -p "$apk" "$lib" | grep -a -c -F 'index.html')" -gt 0 ]; then
      ok "web interface embedded in $lib (index.html)"
    else
      bad "no index.html in $lib"
    fi
  else
    bad "no $lib"
  fi
done
[ -n "$abis" ] || bad "no native library"
# Release builds shorten resource paths (res/Gx.png): the icon is looked up in the resource table.
resources="$("$aapt2" dump resources "$apk")"
icon=1
for r in mipmap/ic_launcher mipmap/ic_launcher_round mipmap/ic_launcher_foreground \
  mipmap/ic_launcher_background mipmap/ic_launcher_monochrome; do
  grep -q -- "resource 0x[0-9a-f]* $r\$" <<<"$resources" || { bad "no $r"; icon=0; }
done
# The adaptive form (anydpi; the v26 qualifier goes when minSdk is 26).
grep -A8 -- "resource 0x[0-9a-f]* mipmap/ic_launcher\$" <<<"$resources" | grep -q '(anydpi' \
  || { bad "mipmap/ic_launcher has no adaptive (anydpi) form"; icon=0; }
[ "$icon" = 1 ] && ok "adaptive launcher icon from the brand kit (foreground, background, monochrome)"

# ----- size ------------------------------------------------------------------------------------
echo "size"
limit=$(( 40 * 1048576 ))
if [ "$size" -lt "$limit" ]; then
  ok "$(( (size + 1048575) / 1048576 )) MB, under 40 MB (B13)"
else
  bad "$(( (size + 1048575) / 1048576 )) MB, over 40 MB (B13)"
fi

if [ "$fail" = 0 ]; then echo "verify-apk: all checks passed"; else echo "verify-apk: some checks failed"; fi
exit "$fail"
