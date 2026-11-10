# The Android build

The repository builds an Android app from the same core and interface: Kiln Home, Kiln Write, Kiln Grid and Kiln Deck in one window. Its manifest sets the minimum to Android 8.0 (API level 26).

[Building from source](building.md#android) makes the APK, and `tools/verify-apk.sh` checks its badging, manifest filters, signature, contents and size. The platform-free logic has unit tests. The app has been built and checked, and has not been run on a device.

The phone layout itself is tested in the browser at phone sizes: see [Kiln on a phone or tablet](mobile.md).

## What the code does

- **Opening.** Open shows Android's document picker and keeps access to what you open, for Recent files. Word, Excel, PowerPoint, OpenDocument, CSV and Kiln documents are registered for Open with and Share ▸ Kiln. A document lent for reading only opens as a copy, and Save asks where to keep it.
- **Saving.** Save writes back to where the document came from; Save As shows Android's save dialog. AutoSave, Document Recovery, version history, recent files and settings live in the app's own storage.
- **Printing.** File ▸ Print hands the laid-out pages to Android's print dialog.
- **Back.** Back closes what is open first, then the document, asking about unsaved changes as Close does. On Kiln Home, Kiln goes to the background.
- **Slide shows.** Presenter View's second window is on the desktop app.
