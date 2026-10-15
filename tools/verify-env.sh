#!/usr/bin/env bash
# Officina — verify the development environment (PLAN.md §4). Exits non-zero with a clear message
# listing every missing or wrong tool. Run from anywhere: bash tools/verify-env.sh
set -uo pipefail

export PATH="$HOME/.local/opt/node/bin:$HOME/.local/bin:$PATH"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
want_pnpm="$(sed -n 's/.*"packageManager": *"pnpm@\([^"]*\)".*/\1/p' "$repo/package.json" 2>/dev/null)"
want_pnpm="${want_pnpm:-12.11.2}"

fail=0
if [ -t 1 ]; then G=$'\033[32m' R=$'\033[31m' Y=$'\033[33m' N=$'\033[0m'; else G='' R='' Y='' N=''; fi
ok() { printf '  %sok%s    %-22s %s\n' "$G" "$N" "$1" "$2"; }
bad() { printf '  %sFAIL%s  %-22s %s\n' "$R" "$N" "$1" "$2" >&2; fail=1; }
warn() { printf '  %swarn%s  %-22s %s\n' "$Y" "$N" "$1" "$2"; }

echo "Officina environment check"

# Node 24.x
if command -v node >/dev/null 2>&1; then
  v="$(node -v)"
  case "$v" in
    v24.*) ok node "$v ($(command -v node))" ;;
    *) bad node "$v found, need 24.x — run tools/setup-node.sh and export PATH=\"\$HOME/.local/opt/node/bin:\$PATH\"" ;;
  esac
else
  bad node "not found — run tools/setup-node.sh"
fi

# pnpm (version pinned by packageManager)
if command -v pnpm >/dev/null 2>&1; then
  v="$(pnpm -v 2>/dev/null)"
  if [ "$v" = "$want_pnpm" ]; then ok pnpm "$v"; else bad pnpm "$v found, need $want_pnpm — run tools/setup-node.sh"; fi
else
  bad pnpm "not found — run tools/setup-node.sh"
fi

# Java 21
java_bin="${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}/bin/java"
[ -x "$java_bin" ] || java_bin="$(command -v java || true)"
if [ -n "$java_bin" ] && [ -x "$java_bin" ]; then
  v="$("$java_bin" -version 2>&1 | head -n1)"
  if printf '%s' "$v" | grep -q 'version "21'; then ok java "$v"; else bad java "$v — need Java 21 (JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64)"; fi
else
  bad java "not found — need OpenJDK 21"
fi

# LibreOffice (test oracle)
if [ -x /usr/bin/soffice ] || command -v soffice >/dev/null 2>&1; then
  sof="$(command -v soffice || echo /usr/bin/soffice)"
  ok soffice "$("$sof" --version 2>/dev/null | head -n1)"
else
  bad soffice "not found at /usr/bin/soffice — LibreOffice is required as a test oracle"
fi

# Playwright Chromium
pw_cache="${PLAYWRIGHT_BROWSERS_PATH:-$HOME/.cache/ms-playwright}"
chromium="$(find "$pw_cache" -maxdepth 4 \( -path '*/chromium_headless_shell-*/*/chrome-headless-shell' -o -path '*/chromium-*/chrome-linux*/chrome' \) -type f 2>/dev/null | head -n1)"
if [ -n "$chromium" ]; then
  ok "playwright chromium" "${chromium#"$pw_cache"/}"
else
  bad "playwright chromium" "not in $pw_cache — run: pnpm --filter @officina/e2e exec playwright install chromium"
fi

# Informational (not required by every work package)
if [ -x "$HOME/Android/cmdline-tools/latest/bin/sdkmanager" ] && [ -d "$HOME/Android/platforms/android-36" ] \
  && [ -d "$HOME/Android/build-tools/36.0.0" ]; then
  ok "android sdk" "$HOME/Android (platform 36, build-tools 36.0.0)"
else
  warn "android sdk" "incomplete in ~/Android — run tools/setup-android.sh (needed by WP-35)"
fi
if [ -d "$repo/node_modules" ]; then ok "workspace" "dependencies installed"; else warn "workspace" "run pnpm install"; fi

if [ "$fail" -ne 0 ]; then
  echo "Environment check FAILED — fix the items marked FAIL above." >&2
  exit 1
fi
echo "Environment OK."
