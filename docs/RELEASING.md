# Releasing

Maintainer runbook. A release is a version tag; everything after the tag is
automated except the Linux and Android builds that can also be made locally.

## What happens on a tag

```
git tag -a vX.Y.Z  ->  .github/workflows/release.yml
                         linux        (ubuntu-22.04)  tauri-action, profile dist  -> AppImage, .deb
                         windows      (windows)       tauri-action, profile dist  -> NSIS .exe, .msi
                         macos-arm64  (macos)         tauri-action, profile dist  -> .dmg (unsigned)
                         macos-x64    (macos)         tauri-action, profile dist  -> .dmg (unsigned)
                         android      (ubuntu)        pnpm tauri android build --apk --split-per-abi
                                                      -> arm64-v8a, armeabi-v7a, x86_64 APKs
                         web          (ubuntu)        wasm core + pnpm build      -> kiln-web-X.Y.Z.tar.gz
                         every job asserts the installer size budgets (PLAN.md §3.9, B13)
                         publish  needs all: SHA256SUMS.txt over every file, upload to the
                                  GitHub Release for the tag (softprops/action-gh-release)
```

The workflow can also be started by hand (**Actions ▸ release ▸ Run
workflow**). Without a tag it builds and uploads workflow artifacts but does not
publish a release.

## Checklist

1. `main` is green in CI and `cargo xtask verify`, `pnpm verify` and `pnpm e2e`
   pass locally.
2. Set the version everywhere: `[workspace.package] version` in `Cargo.toml`,
   `version` in `apps/desktop/src-tauri/tauri.conf.json`,
   `pnpm -r exec npm version X.Y.Z --no-git-tag-version` and the root
   `package.json`.
3. In `CHANGELOG.md`, rename `Unreleased` to `[X.Y.Z] - YYYY-MM-DD`, add a new
   empty `Unreleased` above it, and update the compare links at the bottom.
4. In `docs/index.html`, set the `data-version` value to `vX.Y.Z` so the
   download buttons point at the new assets.
5. If the icon or banner changed: `pnpm media:render` and commit the PNGs.
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
development libraries (docs/qa/env.md); never while another heavy build runs on
the shared development machine (PLAN.md §4.7).

```sh
pnpm install --frozen-lockfile
pnpm tauri build --bundles appimage,deb -- --profile dist
ls target/dist/bundle/appimage target/dist/bundle/deb
pnpm tauri android build --apk --target aarch64 --split-per-abi   # one ABI locally (memory)
```

Attach local files with `gh release upload vX.Y.Z <files> --clobber`; the
workflow's `publish` job recomputes `SHA256SUMS.txt` over its own files.

## Signing

- **Android.** The release APK is signed with the key in
  `~/.kiln-keys/release.jks` (never in the repository). CI reads the same
  key from the secrets `ANDROID_KEYSTORE_B64` and `ANDROID_KEYSTORE_PASSWORD`:

  ```sh
  gh secret set ANDROID_KEYSTORE_B64 --body "$(base64 -w0 ~/.kiln-keys/release.jks)"
  gh secret set ANDROID_KEYSTORE_PASSWORD --body "$PW"
  ```

  Without the secrets the workflow signs with a throwaway key and prints a
  warning in the run. Such an APK installs, but it cannot update an install
  signed with the real key: set the secrets before tagging.
- **Windows and macOS** builds are unsigned. Windows SmartScreen and macOS
  Gatekeeper warn on first launch; the manual explains how to proceed. Signing
  is added through tauri-action's signing variables when certificates exist.

## GitHub Pages

Until WP-91 switches the source to GitHub Actions (`pages.yml`, which also
publishes the web app under `/kiln/app/`), Pages serves the `docs/` folder of
`main` at <https://zahakj.github.io/kiln/>
(`docs/.nojekyll` turns off Jekyll, so files are served as they are). The
landing page is `docs/index.html`; the manual is the Markdown in `docs/manual/`,
rendered in the browser by `docs/manual/index.html`. Pages rebuilds a minute or
two after each push to `main`. The social preview image,
`docs/media/social-preview.png`, has to be uploaded by hand under
**Settings ▸ General ▸ Social preview**; GitHub has no API for it.
