#!/usr/bin/env bash
# Officina — install Android cmdline-tools, platform-tools, platform 36 and build-tools 36.0.0
# into ~/Android (user space, no root). Idempotent. See PLAN.md §4.4.
#
# Why cmdline-tools 22.0 (build 15859902) and not the newer 16111833 named in PLAN.md §4.4:
# from 16111833 on, `sdkmanager` is a deprecated shim over the new native "Android CLI", whose
# downloader connects over IPv6 only and hangs forever on hosts with broken IPv6 (as on the dev
# machine, see docs/qa/env.md). 22.0 is the last Java-based sdkmanager; it honours
# -Djava.net.preferIPv4Stack and installs the same packages. Override with CMDLINE_TOOLS_BUILD.
set -euo pipefail

CMDLINE_TOOLS_BUILD="${CMDLINE_TOOLS_BUILD:-15859902}"
CMDLINE_TOOLS_REVISION="${CMDLINE_TOOLS_REVISION:-22.0}"
export SDKMANAGER_OPTS="${SDKMANAGER_OPTS:--Djava.net.preferIPv4Stack=true}"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"

if ! "$JAVA_HOME/bin/java" -version 2>&1 | grep -q 'version "21'; then
  echo "error: Java 21 not found at JAVA_HOME=$JAVA_HOME" >&2
  exit 1
fi

mkdir -p "$ANDROID_HOME/cmdline-tools"
latest="$ANDROID_HOME/cmdline-tools/latest"
if [ ! -x "$latest/bin/sdkmanager" ] \
  || ! grep -qx "Pkg.Revision=$CMDLINE_TOOLS_REVISION" "$latest/source.properties" 2>/dev/null; then
  echo "==> Downloading Android command-line tools $CMDLINE_TOOLS_REVISION ($CMDLINE_TOOLS_BUILD)"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  zip="commandlinetools-linux-${CMDLINE_TOOLS_BUILD}_latest.zip"
  curl -4 -fsSL -o "$tmp/$zip" "https://dl.google.com/android/repository/$zip"
  unzip -q -o "$tmp/$zip" -d "$tmp/ct"
  rm -rf "$latest"
  mv "$tmp/ct/cmdline-tools" "$latest"
else
  echo "==> cmdline-tools already installed"
fi

# Accept licences (yes exits with SIGPIPE when sdkmanager closes stdin; ignore that).
(yes || true) | sdkmanager --sdk_root="$ANDROID_HOME" --licenses >/dev/null 2>&1 || true

need=()
[ -d "$ANDROID_HOME/platform-tools" ] || need+=("platform-tools")
[ -d "$ANDROID_HOME/platforms/android-36" ] || need+=("platforms;android-36")
[ -d "$ANDROID_HOME/build-tools/36.0.0" ] || need+=("build-tools;36.0.0")
if [ "${#need[@]}" -gt 0 ]; then
  echo "==> Installing: ${need[*]}"
  sdkmanager --sdk_root="$ANDROID_HOME" "${need[@]}"
else
  echo "==> SDK packages already installed"
fi

# Persist environment (idempotent guards).
block='export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
export ANDROID_HOME="$HOME/Android" ANDROID_SDK_ROOT="$HOME/Android"
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"'
for rc in "$HOME/.bashrc" "$HOME/.profile"; do
  touch "$rc"
  grep -q 'ANDROID_HOME="$HOME/Android"' "$rc" || printf '\n# Officina Android SDK\n%s\n' "$block" >> "$rc"
done

sdkmanager --sdk_root="$ANDROID_HOME" --list_installed
