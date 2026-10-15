#!/usr/bin/env bash
# Officina — install Node 24 LTS and pnpm into user space (no root). Idempotent.
# See PLAN.md §4.1. Safe to re-run: skips work that is already done.
set -euo pipefail

NODE_VERSION="${NODE_VERSION:-24.22.0}"
PNPM_VERSION="${PNPM_VERSION:-12.11.2}"
OPT="$HOME/.local/opt"
DIST="node-v${NODE_VERSION}-linux-x64"

mkdir -p "$OPT" "$HOME/.local/bin"

if [ ! -x "$OPT/$DIST/bin/node" ]; then
  echo "==> Downloading Node $NODE_VERSION"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  (
    cd "$tmp"
    curl -fsSLO "https://nodejs.org/dist/v${NODE_VERSION}/${DIST}.tar.xz"
    curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt" | grep " ${DIST}.tar.xz\$" | sha256sum -c -
    tar -xJf "${DIST}.tar.xz" -C "$OPT"
  )
else
  echo "==> Node $NODE_VERSION already present in $OPT/$DIST"
fi
ln -sfn "$OPT/$DIST" "$OPT/node"

line='export PATH="$HOME/.local/opt/node/bin:$PATH"'
for rc in "$HOME/.bashrc" "$HOME/.profile"; do
  touch "$rc"
  grep -q 'local/opt/node/bin' "$rc" || printf '\n%s\n' "$line" >> "$rc"
done
export PATH="$OPT/node/bin:$PATH"

echo "==> node $(node -v)"
if [ "$(pnpm -v 2>/dev/null || true)" != "$PNPM_VERSION" ]; then
  echo "==> Installing pnpm $PNPM_VERSION"
  npm install -g "pnpm@${PNPM_VERSION}" --no-fund --no-audit
fi
echo "==> pnpm $(pnpm -v)"

# Playwright Chromium (PLAN.md §4.3) — only once the workspace has been installed.
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -d "$repo/node_modules" ]; then
  echo "==> Installing Playwright Chromium (into ~/.cache/ms-playwright)"
  (cd "$repo" && pnpm --filter @officina/e2e exec playwright install chromium)
else
  echo "==> Skipping Playwright Chromium (run 'pnpm install' first, then re-run this script)"
fi
