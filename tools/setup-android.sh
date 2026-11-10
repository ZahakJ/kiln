#!/usr/bin/env bash
# Kiln: the Android toolchain for `pnpm tauri android build`, in user space, no root.
# Idempotent: what is there already stays.
#
#   - Android command-line tools, platform-tools, platform 36, build-tools 36.0.0 and an NDK
#     (r29, 29.0.14206865, unless one of r27 or later is installed or NDK_VERSION names another);
#   - the Rust targets aarch64-linux-android, armv7-linux-androideabi, x86_64-linux-android and
#     i686-linux-android;
#   - with --persist, the environment (JAVA_HOME, ANDROID_HOME, NDK_HOME) in ~/.profile.
#
# Usage: tools/setup-android.sh [--check] [--persist]
#   --check    reports what is installed and what is missing, installs nothing (exit 1 if missing)
#
# Where things are: ANDROID_HOME if set, else ~/Android/Sdk if it exists, else ~/Android; Java 21
# from JAVA_HOME, else ~/Android/jdk-21*, else /usr/lib/jvm/java-21-openjdk-amd64.
#
# cmdline-tools 22.0 (build 15859902): from 16111833 on, `sdkmanager` is a shim over the native
# "Android CLI", whose downloader connects over IPv6 only and hangs on hosts with broken IPv6 (as
# on the development machine). 22.0 is the last Java sdkmanager; it honours
# -Djava.net.preferIPv4Stack and installs the same packages. Override with CMDLINE_TOOLS_BUILD.
set -euo pipefail

check=0 persist=0
for arg in "$@"; do
  case "$arg" in
    --check) check=1 ;;
    --persist) persist=1 ;;
    *) echo "usage: $0 [--check] [--persist]" >&2; exit 2 ;;
  esac
done

CMDLINE_TOOLS_BUILD="${CMDLINE_TOOLS_BUILD:-15859902}"
CMDLINE_TOOLS_REVISION="${CMDLINE_TOOLS_REVISION:-22.0}"
export SDKMANAGER_OPTS="${SDKMANAGER_OPTS:--Djava.net.preferIPv4Stack=true}"

if [ -z "${ANDROID_HOME:-}" ]; then
  if [ -d "$HOME/Android/Sdk" ]; then ANDROID_HOME="$HOME/Android/Sdk"; else ANDROID_HOME="$HOME/Android"; fi
fi
export ANDROID_HOME ANDROID_SDK_ROOT="$ANDROID_HOME"

if [ -z "${JAVA_HOME:-}" ] || ! "$JAVA_HOME/bin/java" -version 2>&1 | grep -q 'version "21'; then
  JAVA_HOME=""
  for j in "$HOME"/Android/jdk-21* /usr/lib/jvm/java-21-openjdk-amd64 /usr/lib/jvm/java-21-openjdk; do
    if [ -x "$j/bin/java" ] && "$j/bin/java" -version 2>&1 | grep -q 'version "21'; then JAVA_HOME="$j"; break; fi
  done
fi
[ -n "$JAVA_HOME" ] || { echo "error: no Java 21 (set JAVA_HOME; Gradle needs 21)" >&2; exit 1; }
export JAVA_HOME
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"

# The NDK: NDK_VERSION if set, else the newest r27 or later already installed, else r29.
if [ -z "${NDK_VERSION:-}" ]; then
  NDK_VERSION="$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' 2>/dev/null \
    | awk -F. '$1 >= 27' | sort -V | tail -n1)"
  NDK_VERSION="${NDK_VERSION:-29.0.14206865}"
fi
export NDK_HOME="$ANDROID_HOME/ndk/$NDK_VERSION"

targets=(aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android)
missing=()
[ -x "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" ] || missing+=("cmdline-tools")
[ -d "$ANDROID_HOME/platform-tools" ] || missing+=("platform-tools")
[ -d "$ANDROID_HOME/platforms/android-36" ] || missing+=("platforms;android-36")
[ -d "$ANDROID_HOME/build-tools/36.0.0" ] || missing+=("build-tools;36.0.0")
[ -f "$NDK_HOME/source.properties" ] || missing+=("ndk;$NDK_VERSION")
installed_targets="$(rustup target list --installed 2>/dev/null || true)"
for t in "${targets[@]}"; do
  grep -qx "$t" <<<"$installed_targets" || missing+=("rust:$t")
done

echo "==> JAVA_HOME=$JAVA_HOME"
echo "==> ANDROID_HOME=$ANDROID_HOME"
echo "==> NDK_HOME=$NDK_HOME"
if [ "$check" = 1 ]; then
  if [ "${#missing[@]}" -eq 0 ]; then echo "==> everything is installed"; exit 0; fi
  echo "==> missing: ${missing[*]}"
  exit 1
fi

latest="$ANDROID_HOME/cmdline-tools/latest"
if [ ! -x "$latest/bin/sdkmanager" ]; then
  echo "==> Downloading Android command-line tools $CMDLINE_TOOLS_REVISION ($CMDLINE_TOOLS_BUILD)"
  mkdir -p "$ANDROID_HOME/cmdline-tools"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  zip="commandlinetools-linux-${CMDLINE_TOOLS_BUILD}_latest.zip"
  curl -4 -fsSL -o "$tmp/$zip" "https://dl.google.com/android/repository/$zip"
  unzip -q -o "$tmp/$zip" -d "$tmp/ct"
  rm -rf "$latest"
  mv "$tmp/ct/cmdline-tools" "$latest"
fi

sdk=()
for m in "${missing[@]}"; do
  case "$m" in cmdline-tools | rust:*) ;; *) sdk+=("$m") ;; esac
done
if [ "${#sdk[@]}" -gt 0 ]; then
  # Accept the licences (yes exits with SIGPIPE when sdkmanager closes stdin; ignore that).
  (yes || true) | sdkmanager --sdk_root="$ANDROID_HOME" --licenses >/dev/null 2>&1 || true
  echo "==> Installing: ${sdk[*]}"
  sdkmanager --sdk_root="$ANDROID_HOME" "${sdk[@]}"
fi

echo "==> Rust Android targets"
rustup target add "${targets[@]}"

if [ "$persist" = 1 ]; then
  rc="$HOME/.profile"
  touch "$rc"
  grep -q '# Android (Kiln)' "$rc" || printf '\n# Android (Kiln)\nexport JAVA_HOME="%s"\nexport ANDROID_HOME="%s" ANDROID_SDK_ROOT="%s"\nexport NDK_HOME="%s"\n' \
    "$JAVA_HOME" "$ANDROID_HOME" "$ANDROID_HOME" "$NDK_HOME" >>"$rc"
  echo "==> environment written to $rc"
else
  echo "==> for the build, export:"
  printf '    export JAVA_HOME="%s" ANDROID_HOME="%s" NDK_HOME="%s"\n' "$JAVA_HOME" "$ANDROID_HOME" "$NDK_HOME"
fi
