# Releasing

Maintainer runbook. A release is a version tag; everything after the tag is
automated except the Linux and Android builds that can also be made locally.

## What happens on a tag

```
git tag -a vX.Y.Z  ->  .github/workflows/release.yml
                         linux        (ubuntu-22.04)  tauri-action, profile dist  -> AppImage, .deb
                                                      tools/verify-bundles.sh
                         windows      (windows)       tauri-action, profile dist  -> NSIS .exe, .msi
                         macos-arm64  (macos)         tauri-action, profile dist  -> .dmg
                         macos-x64    (macos)         tauri-action, profile dist  -> .dmg
                         (signed, and the updater's files made, when the secrets below exist)
                         android      (ubuntu)        pnpm tauri android build --apk --split-per-abi
                                                      -> arm64-v8a, armeabi-v7a, x86_64 APKs
                                                      (release key from the secrets below)
                                                      tools/verify-apk.sh
                         web          (ubuntu)        wasm core + pnpm build      -> kiln-web-X.Y.Z.tar.gz
                         every job asserts the installer size budgets (B13)
                         publish  needs all: SHA256SUMS.txt over every file, upload to the
                                  GitHub Release for the tag (softprops/action-gh-release)
```

The workflow can also be started by hand (**Actions ▸ release ▸ Run
workflow**). Without a tag it builds and uploads workflow artifacts but does not
publish a release.

## A test run of the release workflow

Run this after a change to `release.yml`, the bundle configuration
(`apps/desktop/src-tauri/tauri.conf.json`, `tauri.android.conf.json`), the desktop shell or the
Android project (`apps/desktop/src-tauri/gen/android`), before tagging. It builds every platform
from `main` without publishing anything:

```sh
gh workflow run release.yml --ref main
sleep 10
run="$(gh run list --workflow release.yml --limit 1 --json databaseId -q '.[0].databaseId')"
gh run watch "$run" --exit-status
mkdir -p release-check && cd release-check
gh run download "$run" -n linux -n windows -n macos-arm64 -n macos-x64 -n android
ls -la linux windows macos-arm64 macos-x64 android
```

Check that:

1. the run is green, including each desktop job's **Installer size budget (B13)** step and the
   android job's **Check the APKs (B13)** step (their sizes are also in the run summary);
2. the five artefacts exist: `linux` (`Kiln_X.Y.Z_amd64.AppImage`, `Kiln_X.Y.Z_amd64.deb`),
   `windows` (`Kiln_X.Y.Z_x64-setup.exe`, `Kiln_X.Y.Z_x64_en-US.msi`), `macos-arm64` and
   `macos-x64` (`Kiln_X.Y.Z_aarch64.dmg`, `Kiln_X.Y.Z_x64.dmg`), and `android`
   (`kiln-X.Y.Z-arm64-v8a.apk`, `kiln-X.Y.Z-armeabi-v7a.apk`, `kiln-X.Y.Z-x86_64.apk`);
3. the downloaded files are within B13: AppImage under 130 MB, `.deb`, `.exe` and `.msi` under
   30 MB, `.dmg` and each APK under 35 MB:

   ```sh
   for f in */*; do printf '%5d MB  %s\n' $(( ($(wc -c < "$f") + 1048575) / 1048576 )) "$f"; done
   ```

4. the Linux bundles pass the same checks locally: put them where the script looks and run it
   (it needs `dpkg-deb`):

   ```sh
   mkdir -p bundle/appimage bundle/deb
   cp linux/*.AppImage bundle/appimage/ && cp linux/*.deb bundle/deb/
   bash ../tools/verify-bundles.sh "$PWD/bundle"
   ```

5. each APK passes `tools/verify-apk.sh` locally (it needs the Android build tools,
   `ANDROID_HOME`): package `io.github.zahakj.kiln`, the intent filters, the signature, the
   library and the size. The signer line says `the debug key` until the release key's secrets
   are set (below), and names the release key's certificate after. With the secrets set, check
   as a release does (`--release` fails a debug signer):

   ```sh
   for f in android/*.apk; do bash ../tools/verify-apk.sh --release "$f"; done
   ```

The CI workflow (`ci.yml`) runs the desktop shell's unit tests, the startup probe and the
desktop smoke test (`tauri-driver`) on every push; check it is green for the same commit
(`gh run list --workflow ci.yml --limit 1`).

## Checklist

1. `main` is green in CI and `cargo xtask verify`, `pnpm verify` and `pnpm e2e`
   pass locally.
2. Set the version everywhere: `[workspace.package] version` in `Cargo.toml`,
   `version` in `apps/desktop/src-tauri/tauri.conf.json`,
   `pnpm -r exec npm version X.Y.Z --no-git-tag-version` and the root
   `package.json`.
3. In `CHANGELOG.md`, rename `Unreleased` to `[X.Y.Z] - YYYY-MM-DD`, add a new
   empty `Unreleased` above it, and update the compare links at the bottom.
4. In `docs/index.html`, set every `data-version` element to `vX.Y.Z`, remove
   `hidden` from each `[data-release-card]` of a platform this release has built
   and run, and give its button (`data-release-file`, `VER` standing for the
   version) its asset's URL,
   `https://github.com/ZahakJ/kiln/releases/download/vX.Y.Z/<file>`. Update the
   Platforms cell of Kiln in `docs/qa/compare-facts.json`, then run
   `cargo xtask site` for the numbers.
5. If the interface changed: `pnpm e2e --grep docs-media --workers=1`, then
   `pnpm media:render --screenshots`, and commit `docs/media/`. If the icon or
   banner changed: `pnpm media:render` too.
6. Commit (`chore(release): vX.Y.Z`), push, and wait for CI:

   ```sh
   git push origin main
   gh run watch "$(gh run list --branch main --limit 1 --json databaseId -q '.[0].databaseId')" --exit-status
   ```

7. Tag and push the tag:

   ```sh
   git tag -a vX.Y.Z -m "Kiln X.Y.Z"
   git push origin vX.Y.Z
   gh run watch "$(gh run list --workflow release.yml --limit 1 --json databaseId -q '.[0].databaseId')" --exit-status
   ```

8. Check the release page: the desktop, Android and web files and `SHA256SUMS.txt`. Then check
   that every download button on <https://zahakj.github.io/kiln/> resolves:

   ```sh
   gh release view vX.Y.Z --json assets -q '.assets[].url' | xargs -n1 curl -sIL -o /dev/null -w '%{http_code} %{url_effective}\n'
   ```

9. Bump to the next development version (`X.Y+1.0-dev`) in a final commit.

## Asset names

Desktop file names are Tauri's defaults for product name `Kiln`; Android and web
names are set by the workflow.

| Platform | File |
|---|---|
| Linux AppImage | `Kiln_X.Y.Z_amd64.AppImage` |
| Linux Debian/Ubuntu | `Kiln_X.Y.Z_amd64.deb` |
| Windows installer | `Kiln_X.Y.Z_x64-setup.exe`, `Kiln_X.Y.Z_x64_en-US.msi` |
| macOS | `Kiln_X.Y.Z_aarch64.dmg`, `Kiln_X.Y.Z_x64.dmg` |
| Android | `kiln-X.Y.Z-arm64-v8a.apk`, `kiln-X.Y.Z-armeabi-v7a.apk`, `kiln-X.Y.Z-x86_64.apk` |
| Web | `kiln-web-X.Y.Z.tar.gz` |
| Checksums | `SHA256SUMS.txt` |

## Building locally

The Linux bundles can be built on a Linux machine with the WebKitGTK
development libraries ([Building from source](manual/building.md)).

```sh
pnpm install --frozen-lockfile
pnpm --filter @kiln/desktop tauri build --bundles appimage,deb -- --profile dist
bash tools/verify-bundles.sh                       # target/dist/bundle
xvfb-run -a target/dist/kiln --startup-probe       # B1 and B2
pnpm tauri android build --apk --target aarch64 --split-per-abi   # one ABI locally (memory)
bash tools/verify-apk.sh                           # the APK just built
```

With the release key's variables set (`KILN_KEYSTORE` and the rest, below), the local APK is
signed like the workflow's; without them, with the debug key.

[Building from source](manual/building.md) has the details and the desktop smoke test.

Attach local files with `gh release upload vX.Y.Z <files> --clobber`; the
workflow's `publish` job recomputes `SHA256SUMS.txt` over its own files.

## Signing

- **Android.** The release key signs every APK, and Android installs an update only when it is
  signed with the same key as the app it replaces, so the key is made once and kept for good. It
  lives outside the repository, in `~/.kiln-keys` (only the owner can read it), with a backup
  somewhere safe:

  ```sh
  install -d -m 700 ~/.kiln-keys
  keytool -genkeypair -v -keystore ~/.kiln-keys/release.jks -alias kiln \
    -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=Kiln, O=The Kiln contributors"
  chmod 600 ~/.kiln-keys/release.jks
  # The passwords, for local builds:
  umask 077 && cat > ~/.kiln-keys/release.env <<'END'
  export KILN_KEYSTORE="$HOME/.kiln-keys/release.jks"
  export KILN_KEYSTORE_PASSWORD='the keystore password'
  export KILN_KEY_ALIAS=kiln
  END
  ```

  `keytool` asks for the keystore password and the key's (press Enter to use the same one).
  Gradle (`gen/android/app/build.gradle.kts`) signs the release APK with the key named by
  `KILN_KEYSTORE`, `KILN_KEYSTORE_PASSWORD`, `KILN_KEY_ALIAS` (default `kiln`) and
  `KILN_KEY_PASSWORD` (default: the keystore's password); `source ~/.kiln-keys/release.env`
  before a local build. CI reads the same key from these secrets:

  ```sh
  gh secret set ANDROID_KEYSTORE_B64 --body "$(base64 -w0 ~/.kiln-keys/release.jks)"
  gh secret set ANDROID_KEYSTORE_PASSWORD --body "$PW"
  gh secret set ANDROID_KEY_ALIAS --body kiln                  # optional, kiln by default
  gh secret set ANDROID_KEY_PASSWORD --body "$KEY_PW"          # only when it differs
  ```

  On a tag (`v*`) the android job stops with an error when the keystore secrets are missing,
  and checks each APK with `tools/verify-apk.sh --release`, which fails an APK signed with the
  debug key: a release is never signed with it. A run started by hand without the secrets (a
  test run, a fork) signs with a throwaway debug key and prints a warning; such an APK installs,
  but it cannot update an install signed with the real key. Locally, Gradle stops when
  `KILN_KEYSTORE` names a file that is not there or `KILN_KEYSTORE_PASSWORD` is empty, and uses
  the debug key only when `KILN_KEYSTORE` is not set. `tools/verify-apk.sh` prints the signer's name and the
  certificate's SHA-256 digest, to compare with
  `keytool -list -v -keystore ~/.kiln-keys/release.jks`.
- **macOS.** Without certificates the `.dmg` is unsigned and Gatekeeper warns on
  first launch. With a Developer ID certificate the build is signed, and with an
  Apple ID also notarised:

  ```sh
  gh secret set APPLE_CERTIFICATE --body "$(base64 -w0 DeveloperID.p12)"
  gh secret set APPLE_CERTIFICATE_PASSWORD --body "$PW"
  gh secret set APPLE_SIGNING_IDENTITY --body "Developer ID Application: Name (TEAMID)"
  gh secret set APPLE_ID --body "you@example.com"          # notarisation
  gh secret set APPLE_PASSWORD --body "$APP_SPECIFIC_PASSWORD"
  gh secret set APPLE_TEAM_ID --body "TEAMID"
  ```

- **Windows.** Without a certificate the installers are unsigned and SmartScreen
  warns on first launch. With a code-signing certificate (`.pfx`) the workflow
  imports it and signs the `.exe` and the `.msi` (SHA-256, timestamped):

  ```sh
  gh secret set WINDOWS_CERTIFICATE --body "$(base64 -w0 kiln.pfx)"
  gh secret set WINDOWS_CERTIFICATE_PASSWORD --body "$PW"
  ```

- **Updates.** The desktop app has Tauri's updater configured against this
  repository's releases (`plugins.updater` in `tauri.conf.json`: the endpoint
  `releases/latest/download/latest.json`, an empty `pubkey`). With no key the
  updater stays off. To switch it on, make a key pair once, keep the private key
  out of the repository and give the workflow both halves:

  ```sh
  pnpm --filter @kiln/desktop tauri signer generate -w ~/.kiln-keys/updater.key
  gh secret set TAURI_SIGNING_PRIVATE_KEY --body "$(cat ~/.kiln-keys/updater.key)"
  gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body "$PW"
  gh secret set TAURI_UPDATER_PUBKEY --body "$(cat ~/.kiln-keys/updater.key.pub)"
  ```

  The release builds then embed the public key, make the signed update files
  (`.sig`, the AppImage and macOS archives) and the app registers the updater.
  Add `latest.json` (Tauri's static update manifest, listing each platform's file
  and signature) to the release assets.

## GitHub Pages

The site at <https://zahakj.github.io/kiln/> is one artefact built by `tools/pages-build.sh`:
the landing page (`docs/index.html`), the manual (`docs/manual/`, rendered in the browser by
`docs/manual/index.html`) and the installable web app under `/kiln/app/` (`vite build --mode
pages`: the base path, the service worker, the manifest). `.github/workflows/pages.yml` builds it
on every push to `main` that touches the site, on releases and by hand, and deploys it with
`actions/upload-pages-artifact` and `actions/deploy-pages`. The social preview image,
`docs/media/social-preview.png`, has to be uploaded by hand under **Settings ▸ General ▸ Social
preview**; GitHub has no API for it.

### Building and checking it locally

```sh
cargo xtask wasm --release             # the cores (or pass --wasm below)
bash tools/pages-build.sh              # target/pages/site, then the checks
node tools/pages-serve.mjs             # http://127.0.0.1:4180/kiln/, served as Pages serves it
pnpm --filter @kiln/e2e exec playwright test --project=pwa --workers=1
bash tools/pages-build.sh --lighthouse # also Lighthouse on 4G (time to interactive < 2.5 s)
```

`tools/pages-build.sh` fails when a budget is broken: the page's first scripts over 450 KB
gzipped, an app window's scripts over 820 KB, a core module over 4,500 KB (the collaboration module over 2,000 KB), or a manifest that
is not installable (`tools/pages-check.mjs`; the numbers go to `target/pages/report.json`).
`pages-serve.mjs` sends no COOP or COEP headers, like Pages; the service worker adds them, which
is what turns on the Grid core's threads module.

### Switching the Pages source to GitHub Actions

The deploy job of `pages.yml` runs only when the repository variable `PAGES_WORKFLOW` is `1`, so
a push before the source is switched does not fail. The steps, with `gh` signed in as the
repository's owner:

1. Switch the source:
   `gh api -X PUT repos/ZahakJ/kiln/pages -f build_type=workflow`, then check it:
   `gh api repos/ZahakJ/kiln/pages -q .build_type` prints `workflow`.
2. Push `main`, or run the workflow: `gh workflow run pages.yml --ref main`.
3. Watch it: `gh run watch --exit-status $(gh run list --workflow pages.yml --limit 1 --json databaseId -q '.[0].databaseId')`.
   With `PAGES_WORKFLOW` unset the deploy job is skipped: set it for this first deploy with
   `gh variable set PAGES_WORKFLOW --body 1`, or remove the guard first (step 6).
4. Check the app is served: `curl -4 -sI https://zahakj.github.io/kiln/app/` answers `200`, and the
   landing page is served: `curl -4 -s https://zahakj.github.io/kiln/` contains "Kiln Write".
5. Run the pwa e2e against the deployed site (it boots the WebAssembly core, goes offline and
   back, and checks that the service worker makes the page cross-origin isolated and Grid runs
   on its threads module):
   `KILN_E2E_BASE=https://zahakj.github.io/kiln/app/ pnpm --filter @kiln/e2e exec playwright test --project=pwa --workers=1`.
   The tests that deploy are skipped there (`pwa-builds.spec.ts` and the update notice's need the
   local server's test deployments).
6. Once the deploy has succeeded, remove the guard: delete the `if: vars.PAGES_WORKFLOW == '1'`
   line from the deploy job of `pages.yml` and the sentence about it in its header comment, then
   `gh variable delete PAGES_WORKFLOW`.
7. Check the `release:` trigger can deploy. A release runs `pages.yml` on its tag, and the
   `github-pages` environment normally allows only the default branch, so that deploy can be
   refused while pushes to `main` succeed. List the rules:
   `gh api repos/ZahakJ/kiln/environments/github-pages/deployment-branch-policies`. If there is no
   rule for tags, add one:
   `gh api -X POST repos/ZahakJ/kiln/environments/github-pages/deployment-branch-policies -f name='v*' -f type=tag`.
   The site is the same either way when the tag is on `main`, which has deployed it already.
