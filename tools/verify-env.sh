#!/usr/bin/env bash
# Kiln — verify the development environment (PLAN.md §4). Exits non-zero with a clear message
# listing every missing or wrong tool. Run from anywhere: bash tools/verify-env.sh
set -uo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/opt/node/bin:$HOME/.local/bin:$PATH"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
want_pnpm="$(sed -n 's/.*"packageManager": *"pnpm@\([^"]*\)".*/\1/p' "$repo/package.json" 2>/dev/null)"
want_pnpm="${want_pnpm:-12.11.2}"

fail=0
if [ -t 1 ]; then G=$'\033[32m' R=$'\033[31m' Y=$'\033[33m' N=$'\033[0m'; else G='' R='' Y='' N=''; fi
ok() { printf '  %sok%s    %-22s %s\n' "$G" "$N" "$1" "$2"; }
bad() { printf '  %sFAIL%s  %-22s %s\n' "$R" "$N" "$1" "$2" >&2; fail=1; }
warn() { printf '  %swarn%s  %-22s %s\n' "$Y" "$N" "$1" "$2"; }

echo "Kiln environment check"

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
  bad "playwright chromium" "not in $pw_cache — run: pnpm --filter @kiln/e2e exec playwright install chromium"
fi

# Rust toolchain (PLAN.md §4.1)
want_rust="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' "$repo/rust-toolchain.toml" 2>/dev/null)"
want_rust="${want_rust:-1.99.1}"
if command -v rustc >/dev/null 2>&1; then
  v="$(cd "$repo" && rustc -V 2>/dev/null)"
  case "$v" in
    "rustc $want_rust "*) ok rustc "$v" ;;
    *) bad rustc "${v:-unusable} — need $want_rust: run tools/setup-rust.sh" ;;
  esac
  targets="$(cd "$repo" && rustup target list --installed 2>/dev/null)"
  for t in wasm32-unknown-unknown aarch64-linux-android armv7-linux-androideabi x86_64-linux-android; do
    printf '%s\n' "$targets" | grep -qx "$t" || bad "rust target" "$t missing — run tools/setup-rust.sh"
  done
else
  bad rustc "not found — run tools/setup-rust.sh and export PATH=\"\$HOME/.cargo/bin:\$PATH\""
fi
if command -v cargo-nextest >/dev/null 2>&1; then
  ok cargo-nextest "$(cargo-nextest nextest --version 2>/dev/null | head -n1)"
else
  bad cargo-nextest "not found — run tools/setup-rust.sh"
fi
if command -v sccache >/dev/null 2>&1; then
  ok sccache "$(sccache --version)"
else
  warn sccache "not found — builds work but are slower; run tools/setup-rust.sh"
fi
# wasm-bindgen-cli must match the wasm-bindgen crate in Cargo.lock exactly.
lock_wb="$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/, ""); print; exit}' "$repo/Cargo.lock" 2>/dev/null)"
if command -v wasm-bindgen >/dev/null 2>&1; then
  cli_wb="$(wasm-bindgen --version | awk '{print $2}')"
  if [ -z "$lock_wb" ]; then
    warn wasm-bindgen "$cli_wb (Cargo.lock has no wasm-bindgen entry yet)"
  elif [ "$cli_wb" = "$lock_wb" ]; then
    ok wasm-bindgen "$cli_wb (matches Cargo.lock)"
  else
    bad wasm-bindgen "CLI $cli_wb != crate $lock_wb in Cargo.lock — cargo install wasm-bindgen-cli --version $lock_wb --locked"
  fi
else
  bad wasm-bindgen "wasm-bindgen-cli not found — run tools/setup-rust.sh"
fi
if command -v wasm-opt >/dev/null 2>&1; then
  ok wasm-opt "$(wasm-opt --version)"
else
  bad wasm-opt "not found — run tools/setup-rust.sh"
fi
for tool in cargo-deny cargo-about; do
  if command -v "$tool" >/dev/null 2>&1; then ok "$tool" "$("$tool" --version)"; else bad "$tool" "not found — run tools/setup-rust.sh"; fi
done
# Tauri Linux libraries (only kiln-desktop needs them; CI builds the desktop bundles)
if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
  ok webkit2gtk-4.1 "$(pkg-config --modversion webkit2gtk-4.1)"
else
  warn webkit2gtk-4.1 "missing — kiln-desktop cannot be built locally (see docs/qa/env.md)"
fi
# Android NDK for Tauri Android builds
ndk="${NDK_HOME:-$HOME/Android/ndk/29.0.14206865}"
if [ -f "$ndk/source.properties" ]; then
  ok "android ndk" "$ndk ($(sed -n 's/^Pkg.Revision *= *//p' "$ndk/source.properties"))"
  [ -n "${NDK_HOME:-}" ] || warn NDK_HOME "not exported in this shell (tools/setup-android.sh adds it to ~/.bashrc and ~/.profile)"
else
  bad "android ndk" "not found at $ndk — run tools/setup-android.sh"
fi
# Disk space: the shared target/ directory, sccache and Android builds need room.
free_gb="$(df -Pk "$repo" | awk 'NR==2 {print int($4 / 1048576)}')"
if [ "${free_gb:-0}" -ge 15 ]; then ok "free disk" "${free_gb} GB"; else warn "free disk" "${free_gb} GB (< 15 GB recommended)"; fi

# Informational (not required by every work package)
if [ -x "$HOME/Android/cmdline-tools/latest/bin/sdkmanager" ] && [ -d "$HOME/Android/platforms/android-36" ] \
  && [ -d "$HOME/Android/build-tools/36.0.0" ]; then
  ok "android sdk" "$HOME/Android (platform 36, build-tools 36.0.0)"
else
  warn "android sdk" "incomplete in ~/Android — run tools/setup-android.sh (needed by WP-89)"
fi
if [ -d "$repo/node_modules" ]; then ok "workspace" "dependencies installed"; else warn "workspace" "run pnpm install"; fi

if [ "$fail" -ne 0 ]; then
  echo "Environment check FAILED — fix the items marked FAIL above." >&2
  exit 1
fi
echo "Environment OK."
