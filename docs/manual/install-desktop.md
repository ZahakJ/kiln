# Kiln on Linux

The desktop app is the same suite as the [web app](web.md), in a window of its own: Kiln Home, Kiln Write, Kiln Grid and Kiln Deck on the native core, with OpenDocument files and Presenter View in a second window. You build it from source: [Building from source](building.md#the-desktop-app) has the steps.

## What the build makes

| File | What it is |
|---|---|
| `target/dist/kiln` | The application |
| `Kiln_X.Y.Z_amd64.deb` | A Debian package |
| `Kiln_X.Y.Z_amd64.AppImage` | A portable AppImage |

`tools/verify-bundles.sh` checks both packages. The desktop test suite runs `target/dist/kiln` under xvfb on Arch Linux with WebKitGTK.

The `.deb` registers sixteen document types with their icons, so Word, Excel, PowerPoint, OpenDocument, CSV and Kiln files offer Kiln under **Open With**.

## Opening files

- `kiln <file>` in a terminal, **Open With** in the file manager or a double-click on a registered type each open the file in a window of its own.
- A second launch hands its files to the running Kiln, so there is one Kiln however many documents are open.
- New and Open from a document window open their own window. If a document is already open, its window comes to the front.

## Menus and closing

The menu bar follows the app of the window and the interface language. Closing a document window asks about unsaved changes first, and Quit asks about every unsaved document. [Document Recovery](files.md#document-recovery) brings a document back after a crash.
