# Releasing

Maintainer runbook. A release is a version tag; everything after the tag is
automated except the Linux and Android builds that can also be made locally.

## What happens on a tag

```
git tag -a vX.Y.Z  ->  .github/workflows/release.yml
                         linux    (ubuntu)   pnpm release:desktop --linux   -> AppImage, .deb
                         windows  (windows)  pnpm release:desktop --win     -> NSIS installer x64, portable .exe
                         macos    (macos)    pnpm release:desktop --mac     -> .dmg arm64 and x64, unsigned
                         android  (ubuntu)   pnpm release:android           -> signed release APK
                         publish  needs all four: SHA256SUMS.txt over every file,
                                  upload to the GitHub Release for the tag
                                  (created if missing, appended to otherwise)
                     ->  README and the landing page link to the release assets;
                         GitHub Pages serves docs/ from main
```

The workflow can also be started by hand (**Actions ▸ release ▸ Run
workflow**). Without a tag it builds and uploads workflow artifacts but does not
publish a release.

## Checklist

1. `main` is green in CI and `pnpm verify` and `pnpm e2e` pass locally.
2. Set the version everywhere:
   `pnpm -r exec npm version X.Y.Z --no-git-tag-version` and the same in the root
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
   git tag -a vX.Y.Z -m "Officina X.Y.Z"
   git push origin vX.Y.Z
   gh run watch "$(gh run list --workflow release.yml --limit 1 --json databaseId -q '.[0].databaseId')" --exit-status
   ```

8. Check the release page: six platform files and `SHA256SUMS.txt`. Then check
   that every download button on <https://zahakj.github.io/officina/> resolves:

   ```sh
   gh release view vX.Y.Z --json assets -q '.assets[].url' | xargs -n1 curl -sIL -o /dev/null -w '%{http_code} %{url_effective}\n'
   ```

9. Bump to the next development version (`X.Y+1.0-dev`) in a final commit.

## Asset names

| Platform | File |
|---|---|
| Linux AppImage | `officina-X.Y.Z-linux-x86_64.AppImage` |
| Linux Debian/Ubuntu | `officina_X.Y.Z_amd64.deb` |
| Windows installer | `Officina-Setup-X.Y.Z.exe` |
| Windows portable | `Officina-X.Y.Z-portable.exe` |
| macOS Apple silicon | `Officina-X.Y.Z-arm64.dmg` |
| macOS Intel | `Officina-X.Y.Z-x64.dmg` |
| Android | `officina-X.Y.Z-android.apk` |
| Checksums | `SHA256SUMS.txt` |

## Building locally and uploading

The Linux and Android builds can be made on a Linux machine and attached to the
release before or instead of the workflow's copies (the workflow overwrites
files of the same name).

```sh
pnpm install --frozen-lockfile
pnpm release:desktop --linux      # -> apps/desktop/dist/officina-X.Y.Z-linux-x86_64.AppImage, officina_X.Y.Z_amd64.deb
pnpm release:android                 # -> apps/android/dist/officina-X.Y.Z-android.apk
sha256sum apps/desktop/dist/officina[-_]X.Y.Z* apps/android/dist/officina-X.Y.Z-android.apk > SHA256SUMS-local.txt

gh release create vX.Y.Z --title "Officina X.Y.Z" --notes-file notes.md \
  apps/desktop/dist/officina-X.Y.Z-linux-x86_64.AppImage \
  apps/desktop/dist/officina_X.Y.Z_amd64.deb \
  apps/android/dist/officina-X.Y.Z-android.apk
# or, if the release exists already:
gh release upload vX.Y.Z <files> --clobber
```

Create the release before pushing the tag if you want the local files in it:
the workflow's `publish` job appends to an existing release and recomputes
`SHA256SUMS.txt` over its own files.

## Signing

- **Android.** The release APK is signed with the key in
  `~/.officina-keys/release.jks` (never in the repository). CI reads the same
  key from the secrets `ANDROID_KEYSTORE_B64` and `ANDROID_KEYSTORE_PASSWORD`:

  ```sh
  gh secret set ANDROID_KEYSTORE_B64 --body "$(base64 -w0 ~/.officina-keys/release.jks)"
  gh secret set ANDROID_KEYSTORE_PASSWORD --body "$PW"
  ```

  Without the secrets the workflow signs with a throwaway key and names the file
  `…-unsigned-key.apk`, so nobody mistakes it for the real one. Such an APK
  installs, but it cannot update an install signed with the real key: set the
  secrets before tagging, or delete that asset and upload the locally signed
  APK.
- **Windows and macOS** builds are unsigned. Windows SmartScreen and macOS
  Gatekeeper warn on first launch; the manual explains how to proceed. The
  release workflow honours `CSC_LINK`/`CSC_KEY_PASSWORD` and the Apple
  notarisation variables if they are ever added as secrets.

## GitHub Pages

Pages serves the `docs/` folder of `main` at <https://zahakj.github.io/officina/>
(`docs/.nojekyll` turns off Jekyll, so files are served as they are). The
landing page is `docs/index.html`; the manual is the Markdown in `docs/manual/`,
rendered in the browser by `docs/manual/index.html`. Pages rebuilds a minute or
two after each push to `main`. The social preview image,
`docs/media/social-preview.png`, has to be uploaded by hand under
**Settings ▸ General ▸ Social preview**; GitHub has no API for it.
