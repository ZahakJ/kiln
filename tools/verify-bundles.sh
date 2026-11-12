#!/usr/bin/env bash
# Checks the Linux bundles `pnpm --filter @kiln/desktop tauri build` makes:
#
#   deb       dpkg-deb -I reads it; the package is kiln, its binary is /usr/bin/kiln, the desktop
#             file runs `kiln %F` and registers every document type, the MIME database entry and the
#             per-type icons are installed; under 30 MB.
#   AppImage  --appimage-extract unpacks it; the binary inside runs (`kiln --version`), the desktop
#             file registers every document type; under 130 MB (B13 as amended 2026-10-30).
#
# Usage: tools/verify-bundles.sh [bundle dir]   (default target/dist/bundle, then target/release/bundle)
# Needs dpkg-deb (dpkg). Exits 1 on the first failed check, after printing every result so far.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dir="${1:-}"
if [ -z "$dir" ]; then
  for d in "$root/target/dist/bundle" "$root/target/release/bundle"; do
    if [ -d "$d" ]; then dir="$d"; break; fi
  done
fi
[ -n "$dir" ] && [ -d "$dir" ] || { echo "verify-bundles: no bundle directory (build with pnpm --filter @kiln/desktop tauri build)"; exit 1; }
# An absolute path: the AppImage check runs it from a directory of its own.
dir="$(cd "$dir" && pwd)"
command -v dpkg-deb >/dev/null || { echo "verify-bundles: dpkg-deb is missing (install dpkg)"; exit 1; }

# The document types every bundle registers (apps/desktop/src-tauri/src/files.rs).
mimes=(
  application/vnd.openxmlformats-officedocument.wordprocessingml.document
  application/vnd.openxmlformats-officedocument.wordprocessingml.template
  application/vnd.ms-word.document.macroEnabled.12
  application/vnd.oasis.opendocument.text
  application/x-kiln-write
  application/vnd.openxmlformats-officedocument.spreadsheetml.sheet
  application/vnd.openxmlformats-officedocument.spreadsheetml.template
  application/vnd.ms-excel.sheet.macroEnabled.12
  text/csv
  application/vnd.oasis.opendocument.spreadsheet
  application/x-kiln-grid
  application/vnd.openxmlformats-officedocument.presentationml.presentation
  application/vnd.openxmlformats-officedocument.presentationml.template
  application/vnd.ms-powerpoint.presentation.macroEnabled.12
  application/vnd.oasis.opendocument.presentation
  application/x-kiln-deck
)

fail=0
ok() { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*"; fail=1; }
mb() { local s; s=$(wc -c <"$1"); echo $(( (s + 1048575) / 1048576 )); }

# A desktop file: runs kiln with files, names the icon, lists every type.
check_desktop() {
  local file="$1" what="$2"
  [ -f "$file" ] || { bad "$what: no desktop file"; return; }
  grep -q '^Exec=kiln %F$' "$file" && ok "$what: Exec=kiln %F" || bad "$what: Exec is not 'kiln %F'"
  grep -q '^Icon=kiln$' "$file" && ok "$what: Icon=kiln" || bad "$what: Icon is not kiln"
  grep -q '^Name=Kiln$' "$file" && ok "$what: Name=Kiln" || bad "$what: Name is not Kiln"
  local line missing=0
  line="$(grep '^MimeType=' "$file" || true)"
  for m in "${mimes[@]}"; do
    case ";${line#MimeType=}" in *";$m;"*) ;; *) bad "$what: $m not registered"; missing=1 ;; esac
  done
  [ "$missing" = 0 ] && ok "$what: ${#mimes[@]} document types registered"
  if command -v desktop-file-validate >/dev/null; then
    desktop-file-validate "$file" && ok "$what: desktop-file-validate" || bad "$what: desktop-file-validate"
  fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "verify-bundles: $dir"

# --- deb --------------------------------------------------------------------------------------
deb="$(find "$dir/deb" -maxdepth 1 -name '*.deb' -type f 2>/dev/null | sort | tail -n1)"
if [ -z "$deb" ]; then
  bad "deb: no .deb in $dir/deb"
else
  echo "deb: $(basename "$deb")"
  if info="$(dpkg-deb -I "$deb")"; then
    ok "deb: dpkg-deb -I reads it"
    grep -q '^ Package: kiln$' <<<"$info" && ok "deb: package kiln" || bad "deb: the package is not kiln"
    grep -q '^ Depends:.*libwebkit2gtk-4.1' <<<"$info" && ok "deb: depends on libwebkit2gtk-4.1" || bad "deb: no WebKitGTK dependency"
  else
    bad "deb: dpkg-deb -I fails"
  fi
  size=$(mb "$deb")
  [ "$size" -le 30 ] && ok "deb: ${size} MB (B13: under 30 MB)" || bad "deb: ${size} MB (B13: under 30 MB)"
  dpkg-deb -x "$deb" "$work/deb"
  [ -x "$work/deb/usr/bin/kiln" ] && ok "deb: /usr/bin/kiln" || bad "deb: no /usr/bin/kiln"
  check_desktop "$(find "$work/deb/usr/share/applications" -name '*.desktop' | head -n1)" deb
  [ -f "$work/deb/usr/share/mime/packages/kiln.xml" ] && ok "deb: MIME database entry for .kwrite, .kgrid, .kdeck" || bad "deb: no /usr/share/mime/packages/kiln.xml"
  icons=$(find "$work/deb/usr/share/icons/hicolor" -path '*/mimetypes/*.png' | wc -l)
  [ "$icons" -ge $(( ${#mimes[@]} * 7 )) ] && ok "deb: $icons document type icons" || bad "deb: $icons document type icons (want $(( ${#mimes[@]} * 7 )))"
  [ -f "$work/deb/usr/share/icons/hicolor/128x128/apps/kiln.png" ] && ok "deb: app icon" || bad "deb: no app icon"
fi

# --- AppImage ---------------------------------------------------------------------------------
appimage="$(find "$dir/appimage" -maxdepth 1 -name '*.AppImage' -type f 2>/dev/null | sort | tail -n1)"
if [ -z "$appimage" ]; then
  bad "AppImage: no .AppImage in $dir/appimage"
else
  echo "AppImage: $(basename "$appimage")"
  size=$(mb "$appimage")
  [ "$size" -le 130 ] && ok "AppImage: ${size} MB (B13: under 130 MB)" || bad "AppImage: ${size} MB (B13: under 130 MB)"
  chmod +x "$appimage"
  if (cd "$work" && "$appimage" --appimage-extract >/dev/null 2>&1); then
    ok "AppImage: --appimage-extract"
    bin="$work/squashfs-root/usr/bin/kiln"
    [ -x "$bin" ] && ok "AppImage: usr/bin/kiln" || bad "AppImage: no usr/bin/kiln"
    if [ -x "$bin" ]; then
      ver="$("$bin" --version 2>/dev/null || true)"
      [[ "$ver" == Kiln\ * ]] && ok "AppImage: the binary runs ($ver)" || bad "AppImage: the binary does not run"
    fi
    check_desktop "$(find "$work/squashfs-root" -maxdepth 1 -name '*.desktop' | head -n1)" AppImage
  else
    bad "AppImage: --appimage-extract fails"
  fi
fi

if [ "$fail" = 0 ]; then echo "verify-bundles: passed"; else echo "verify-bundles: FAILED"; fi
exit "$fail"
