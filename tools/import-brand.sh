#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Kiln — import the design team's brand kit into the repository (PLAN.md §1.2, WP-51).
#
#   tools/import-brand.sh                  import from /home/muth/kiln-brand/final
#   KILN_BRAND_KIT=/path/to/final tools/import-brand.sh
#   tools/import-brand.sh --no-kit         regenerate everything from the committed brand/ only
#
# Idempotent: running it twice leaves `git status` unchanged after the first run. Every output is
# either a verbatim copy of a kit file or generated deterministically from one:
#
#   brand/                          the kit, verbatim (licences included; .venv, work/ and caches left out)
#   assets/fonts/brand/<Family>/    the kit's UI fonts with their OFL texts
#   packages/tokens/                tokens.css, theme.css, fonts.css, src/generated/kit.ts  (generate)
#   apps/desktop/src-tauri/icons/   `pnpm tauri icon` from the suite tile + the kit's per-size drawings,
#                                   .icns from the kit's macOS canvas, .ico from the kit PNGs,
#                                   Android adaptive layers, per-document icons write/grid/deck.png
#   apps/web/public/                favicon.svg/.ico, PWA icons (16 … 1024, maskable 192/512), manifest
#   docs/media/                     banners (dark/light), social preview 1280×640, logo/lockups
#   docs/design/mockups/            the mockups (README.md there maps each one to its WP)
#
# PNG sizes the kit ships only as SVG are rendered by tools/render-media.mjs (headless Chromium).
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
export PATH="$HOME/.cargo/bin:$HOME/.local/opt/node/bin:$HOME/.local/bin:$PATH"

kit="${KILN_BRAND_KIT:-/home/muth/kiln-brand/final}"
use_kit=1
if [[ "${1:-}" == "--no-kit" ]]; then use_kit=0; fi

log() { printf 'import-brand: %s\n' "$*"; }
die() { printf 'import-brand: error: %s\n' "$*" >&2; exit 1; }

# Copy only when the bytes differ, so unchanged files keep their mtime (and git sees nothing).
put() {
  local src="$1" dst="$2"
  [[ -f "$src" ]] || die "missing kit file: $src"
  mkdir -p "$(dirname "$dst")"
  if ! cmp -s "$src" "$dst"; then cp "$src" "$dst"; fi
}

# Copy an SVG, adding an accessible <title> after the root tag when it has none (the only change
# made to a kit file; Biome's a11y lint requires it for SVGs inside the JS packages).
put_svg_titled() {
  local src="$1" dst="$2" title="$3" tmp
  [[ -f "$src" ]] || die "missing kit file: $src"
  tmp="$(mktemp)"
  if grep -q '<title>' "$src"; then cp "$src" "$tmp"
  else sed -E "0,/<svg[^>]*>/s//&<title>${title}<\/title>/" "$src" >"$tmp"; fi
  mkdir -p "$(dirname "$dst")"
  if cmp -s "$tmp" "$dst"; then rm -f "$tmp"; else mv "$tmp" "$dst"; chmod 644 "$dst"; fi
}

# ------------------------------------------------------------------------------------------------
# 1. The kit, verbatim, into brand/
# ------------------------------------------------------------------------------------------------
if (( use_kit )); then
  [[ -f "$kit/BRAND.md" ]] || die "no brand kit at $kit (BRAND.md missing); set KILN_BRAND_KIT"
  [[ -f "$kit/tokens.json" ]] || die "$kit/tokens.json missing"
  log "copying $kit -> brand/"
  rsync -a --delete --checksum \
    --exclude '/.venv/' --exclude '/work/' --exclude '__pycache__/' --exclude '*.pyc' \
    --exclude '.DS_Store' \
    "$kit/" brand/
fi
[[ -f brand/tokens.json ]] || die "brand/tokens.json missing; run without --no-kit first"
B="$repo/brand"

# ------------------------------------------------------------------------------------------------
# 2. UI fonts with their licences
# ------------------------------------------------------------------------------------------------
log "fonts -> assets/fonts/brand/"
for family in Figtree Fraunces; do
  put "$B/fonts/$family-VF.ttf" "assets/fonts/brand/$family/$family-VF.ttf"
  put "$B/fonts/OFL-$family.txt" "assets/fonts/brand/$family/OFL.txt"
done
put "$B/fonts/LICENSES.md" "assets/fonts/brand/LICENSES.md"

# ------------------------------------------------------------------------------------------------
# 3. Design tokens (packages/tokens is generated from brand/tokens.json)
# ------------------------------------------------------------------------------------------------
log "generating @kiln/tokens"
pnpm --silent --filter @kiln/tokens generate

# ------------------------------------------------------------------------------------------------
# 4. Desktop and Android app icons
# ------------------------------------------------------------------------------------------------
icons="apps/desktop/src-tauri/icons"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# Tauri's generator from the hero tile (1024 px) with the kit's adaptive layers for Android.
cat >"$scratch/icon.json" <<JSON
{
  "default": "$B/icons/png/suite-1024.png",
  "bg_color": "#FFF6E8",
  "android_bg": "$B/android/suite-background.png",
  "android_fg": "$B/android/suite-foreground.png",
  "android_fg_scale": 100,
  "android_monochrome": "$B/android/suite-monochrome.png"
}
JSON
log "pnpm tauri icon (suite tile) -> $icons/"
if ! pnpm --silent tauri icon "$scratch/icon.json" -o "$repo/$icons" >"$scratch/tauri-icon.log" 2>&1; then
  cat "$scratch/tauri-icon.log" >&2
  die "pnpm tauri icon failed"
fi
# icon.icns (from the kit's macOS canvas) and icon.ico (from the kit's per-size drawings) are rebuilt
# by tools/render-media.mjs below, reproducibly; tauri's own .icns differs from run to run.
rm -f "$icons/source.png" # the old placeholder input; the source is now the kit.

# Small sizes use the kit's own drawings (flat tile 20–40 px, 48-level 40–96 px), never a downscale.
put "$B/icons/png/suite-32.png" "$icons/32x32.png"
put "$B/icons/png/suite-64.png" "$icons/64x64.png"
put "$B/icons/png/suite-128.png" "$icons/128x128.png"
put "$B/icons/png/suite-256.png" "$icons/128x128@2x.png"
put "$B/icons/png/suite-512.png" "$icons/icon.png"
# Per-document-type icons for file associations (.docx → Write, .xlsx → Grid, .pptx → Deck).
for app in write grid deck notes; do
  put "$B/icons/png/$app-256.png" "$icons/$app.png"
done

# ------------------------------------------------------------------------------------------------
# 5. Web / PWA icons
# ------------------------------------------------------------------------------------------------
pub="apps/web/public"
log "web icons -> $pub/"
put_svg_titled "$B/favicon.svg" "$pub/favicon.svg" "Kiln"
put "$B/favicon.ico" "$pub/favicon.ico"
for size in 16 24 32 48 64 128 256 512 1024; do
  put "$B/icons/png/suite-$size.png" "$pub/icons/icon-$size.png"
done
for app in write grid deck notes; do
  put "$B/icons/png/$app-256.png" "$pub/icons/$app-256.png"
done
rm -f "$pub"/icons/*-mono.svg

# ------------------------------------------------------------------------------------------------
# 6. Banners, social preview, marks for the README and the landing page
# ------------------------------------------------------------------------------------------------
log "banners -> docs/media/"
for f in banner.svg banner.png banner-light.svg banner-light.png social-preview.svg social-preview.png \
  logo.svg logo-512.png lockup.svg lockup-light.svg wordmark.svg wordmark-light.svg favicon.svg; do
  put "$B/$f" "docs/media/$f"
done
rm -f docs/media/icon.svg docs/media/icon.png # placeholders from WP-50
# The landing page (docs/index.html) serves the brand fonts and the app tiles itself (same origin).
for family in Figtree Fraunces; do
  put "$B/fonts/$family-VF.ttf" "docs/media/fonts/$family-VF.ttf"
  put "$B/fonts/OFL-$family.txt" "docs/media/fonts/OFL-$family.txt"
done
for app in suite write grid deck notes; do
  put "$B/icons/svg/$app-48.svg" "docs/media/tiles/$app-48.svg"
done

# ------------------------------------------------------------------------------------------------
# 7. Mockups for the UI work packages
# ------------------------------------------------------------------------------------------------
log "mockups -> docs/design/mockups/"
mkdir -p docs/design/mockups
for f in "$B"/mockups/*.png; do put "$f" "docs/design/mockups/$(basename "$f")"; done
put "$B/board.png" "docs/design/mockups/board.png"
put "$B/construction.png" "docs/design/mockups/construction.png"
put "$B/icons/size-check.png" "docs/design/mockups/size-check.png"

# ------------------------------------------------------------------------------------------------
# 8. The ui-kit brand glyph set (app marks, spark, flame) and tiles
# ------------------------------------------------------------------------------------------------
node packages/ui-kit/src/icons/brand/build.mjs

# ------------------------------------------------------------------------------------------------
# 9. Raster sizes the kit ships only as SVG, the macOS .icns and the Windows .ico
# ------------------------------------------------------------------------------------------------
node tools/render-media.mjs

log "done"
