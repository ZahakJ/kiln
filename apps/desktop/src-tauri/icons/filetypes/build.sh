#!/usr/bin/env bash
# Renders the document icons of Kiln's file types (a page with the app's tile) from the brand
# kit's tiles (the flat tile below 48 px): <app>.svg and <app>-{16,24,32,48,64,128,256}.png. Needs rsvg-convert (librsvg).
# Run from anywhere: bash apps/desktop/src-tauri/icons/filetypes/build.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
brand="$(cd "$here/../../../../../brand/icons/svg" && pwd)"
# page <app> <tile svg> <out>: the page, its fold and, at 48 px and up, three lines of text.
page() {
  local app="$1" tilesrc="$2" out="$3" small="$4"
  local tile lines tx ty ts
  tile="$(sed -e 's/<?xml[^>]*?>//' "$tilesrc")"
  if [ "$small" = 1 ]; then
    lines=''; tx=40; ty=78; ts=176
  else
    lines='<g fill="#D8C8B0"><rect x="68" y="44" width="80" height="8" rx="4"/><rect x="68" y="64" width="120" height="8" rx="4"/><rect x="68" y="84" width="104" height="8" rx="4"/></g>'
    tx=60; ty=102; ts=136
  fi
  local svg="$out"
  # The tile's own ids stay unique inside the page.
  cat > "$svg" <<SVG
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <defs>
    <linearGradient id="page" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#FFFCF6"/>
      <stop offset="1" stop-color="#F6EBDA"/>
    </linearGradient>
    <linearGradient id="fold" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#E9DBC5"/>
      <stop offset="1" stop-color="#FFF6E8"/>
    </linearGradient>
    <filter id="shadow" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="3" stdDeviation="3.5" flood-color="#2A1A10" flood-opacity="0.28"/>
    </filter>
  </defs>
  <path d="M52 12 H170 L220 62 V236 a8 8 0 0 1 -8 8 H52 a8 8 0 0 1 -8 -8 V20 a8 8 0 0 1 8 -8 Z"
        fill="url(#page)" stroke="#CDBBA2" stroke-width="2" filter="url(#shadow)"/>
  <path d="M170 12 V54 a8 8 0 0 0 8 8 H220 Z" fill="url(#fold)" stroke="#CDBBA2" stroke-width="2" stroke-linejoin="round"/>
  $lines
  <svg x="$tx" y="$ty" width="$ts" height="$ts" viewBox="0 0 512 512">
    $(printf '%s' "$tile" | sed -e 's/^<svg[^>]*>//' -e 's/<\/svg>\s*$//')
  </svg>
</svg>
SVG
}
for app in write grid deck; do
  page "$app" "$brand/$app-48.svg" "$here/$app.svg" 0
  page "$app" "$brand/$app-32.svg" "$here/.$app-small.svg" 1
  for size in 16 24 32; do
    rsvg-convert -w "$size" -h "$size" "$here/.$app-small.svg" -o "$here/$app-$size.png"
  done
  for size in 48 64 128 256; do
    rsvg-convert -w "$size" -h "$size" "$here/$app.svg" -o "$here/$app-$size.png"
  done
  rm -f "$here/.$app-small.svg"
done
echo "wrote $here/{write,grid,deck}{.svg,-*.png}"
