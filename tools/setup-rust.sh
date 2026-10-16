#!/usr/bin/env bash
# Kiln — install the Rust toolchain and Rust build tools into user space (no root). Idempotent.
# Implements PLAN.md §4.1 exactly: rustup + Rust 1.99.1 (minimal profile, rustfmt, clippy, rust-src,
# wasm32 + Android targets), prebuilt cargo-nextest / sccache / wasm-opt, and cargo-installed
# wasm-bindgen-cli / cargo-deny / cargo-about. IPv6 is broken on the dev machine: every download uses curl -4.
set -euo pipefail

RUST_VERSION="${RUST_VERSION:-1.99.1}"
SCCACHE_VERSION="${SCCACHE_VERSION:-0.18.0}"
BINARYEN_VERSION="${BINARYEN_VERSION:-133}"
WASM_BINDGEN_VERSION="${WASM_BINDGEN_VERSION:-0.2.129}"
CARGO_DENY_VERSION="${CARGO_DENY_VERSION:-0.20.2}"
CARGO_ABOUT_VERSION="${CARGO_ABOUT_VERSION:-0.9.2}"
TARGETS=(wasm32-unknown-unknown aarch64-linux-android armv7-linux-androideabi x86_64-linux-android)

export CARGO_HTTP_MULTIPLEXING=false
export PATH="$HOME/.cargo/bin:$PATH"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 1. rustup + the pinned toolchain
if ! command -v rustup >/dev/null 2>&1; then
  echo "==> Installing rustup with Rust $RUST_VERSION"
  target_args=()
  for t in "${TARGETS[@]}"; do target_args+=(--target "$t"); done
  curl -4 --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path \
    --default-toolchain "$RUST_VERSION" --profile minimal \
    --component rustfmt --component clippy --component rust-src "${target_args[@]}"
else
  echo "==> rustup present; ensuring Rust $RUST_VERSION"
  rustup toolchain install "$RUST_VERSION" --profile minimal \
    --component rustfmt --component clippy --component rust-src
  rustup default "$RUST_VERSION"
  rustup target add --toolchain "$RUST_VERSION" "${TARGETS[@]}"
fi

line='export PATH="$HOME/.cargo/bin:$PATH"'
for rc in "$HOME/.bashrc" "$HOME/.profile"; do
  touch "$rc"
  grep -q '.cargo/bin' "$rc" || printf '\n%s\n' "$line" >> "$rc"
done
echo "==> $(rustc -V)"

# 2. Prebuilt binaries (no ten-minute cargo installs)
if ! command -v cargo-nextest >/dev/null 2>&1; then
  echo "==> Installing cargo-nextest (prebuilt)"
  curl -4 -LsSf https://get.nexte.st/latest/linux | tar zxf - -C "$HOME/.cargo/bin"
fi
if [ "$(sccache --version 2>/dev/null | awk '{print $2}')" != "$SCCACHE_VERSION" ]; then
  echo "==> Installing sccache $SCCACHE_VERSION (prebuilt)"
  curl -4 -LsSf "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
    | tar xz --strip-components=1 -C "$HOME/.cargo/bin" --wildcards '*/sccache' || echo "warn: sccache download failed" >&2
fi
if ! wasm-opt --version 2>/dev/null | grep -q "version $BINARYEN_VERSION"; then
  echo "==> Installing wasm-opt (binaryen version_$BINARYEN_VERSION, prebuilt)"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  curl -4 -LsSf "https://github.com/WebAssembly/binaryen/releases/download/version_${BINARYEN_VERSION}/binaryen-version_${BINARYEN_VERSION}-x86_64-linux.tar.gz" \
    | tar xz -C "$tmp"
  # wasm-opt links against libbinaryen from ../lib; keep the layout under ~/.local/opt/binaryen.
  rm -rf "$HOME/.local/opt/binaryen"
  mkdir -p "$HOME/.local/opt"
  mv "$tmp"/binaryen-version_"$BINARYEN_VERSION" "$HOME/.local/opt/binaryen"
  ln -sfn "$HOME/.local/opt/binaryen/bin/wasm-opt" "$HOME/.cargo/bin/wasm-opt"
fi

# 3. cargo-installed tools (pinned; --locked)
cargo_install() { # <crate> <version> <binary> [cargo install options...]
  local crate="$1" version="$2" binary="$3"
  shift 3
  if [ "$("$binary" --version 2>/dev/null | awk '{print $NF}')" != "$version" ]; then
    echo "==> cargo install $crate $version"
    cargo install "$crate" --version "$version" --locked "$@"
  else
    echo "==> $crate $version already installed"
  fi
}
# wasm-bindgen-cli MUST equal the wasm-bindgen crate version in Cargo.lock (tools/verify-env.sh checks).
cargo_install wasm-bindgen-cli "$WASM_BINDGEN_VERSION" wasm-bindgen
cargo_install cargo-deny "$CARGO_DENY_VERSION" cargo-deny
# cargo-about only builds its binary with the `cli` feature.
cargo_install cargo-about "$CARGO_ABOUT_VERSION" cargo-about --features cli

# 4. sccache is optional: if it is absent, drop the rustc-wrapper line from .cargo/config.toml.
if ! command -v sccache >/dev/null 2>&1 && [ -f "$repo/.cargo/config.toml" ]; then
  echo "==> sccache not available; removing rustc-wrapper from .cargo/config.toml"
  sed -i '/^rustc-wrapper *= *"sccache"/d' "$repo/.cargo/config.toml"
fi

echo "==> Rust toolchain ready:"
rustc -V; cargo -V; cargo nextest --version | head -n1; sccache --version 2>/dev/null || true
wasm-opt --version; wasm-bindgen --version; cargo deny --version; cargo about --version
