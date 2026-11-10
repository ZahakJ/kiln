# Kiln in the browser

Kiln Write, Grid and Deck run in a browser at <https://zahakj.github.io/kiln/app/>, with the
same ribbon, commands and file formats as the desktop app. Kiln opens, lays out and saves
documents inside the page itself; a document leaves the device only when you share it (see
[Collaboration and version history](collaboration.md)).

## Installing Kiln

Kiln is an installable web app. In Chrome and Edge, Kiln Home offers **Install Kiln** after a
moment; **Install** adds Kiln to the desktop, the Start menu or the app launcher, and
**Not now** puts the card away for good. The browser's own menu has the same command
(**Install Kiln** in Chrome's ⋮ menu, **Apps ▸ Install this site as an app** in Edge). On Android,
Chrome's menu has **Add to home screen**; on an iPhone or iPad, Safari's **Share ▸ Add to Home
Screen**.

The installed app opens in a window of its own, with its icon in the task bar. Its icon's menu
(a right-click, or a long press on a phone) has **New document**, **New workbook** and **New
presentation**.

## Working offline

The first visit saves Kiln on the device a few seconds after Kiln Home appears: **Kiln works
offline now** says when Write, Grid and Deck are all there. That first download is about 33 MB, and Kiln then
takes about 68 MB of the browser's storage on the device. From then on Kiln starts, and opens,
edits and saves documents, without a connection, and an update keeps it that way: a new version
saves all of its apps on the device before it replaces the old one. While the connection is gone
the corner of the window shows **Offline**, and **Back online** when it returns.

The fonts for Chinese, Japanese and Korean text and for emoji are saved on the device the first
time a document needs them, in the parts its text uses. A document opened offline with such
characters before then says so in one notice above the document, and the characters appear as
soon as the connection is back.

With the browser's data saver on, Kiln saves an app when you first open it and does not save the
others in the background, so **Kiln works offline now** does not appear. A device that already
had every app keeps them through updates.

Documents saved in the browser stay on the device: in the browser's storage for Kiln, or in the
file you chose when the browser lets Kiln save to your files (Chrome and Edge). They are listed
under **Recent** on Kiln Home. AutoSave and Document Recovery work as on the desktop (see
[Files, AutoSave and recovery](files.md)).

## Opening files with Kiln

Once Kiln is installed, the system offers it for documents: **Open with ▸ Kiln** in the file
manager, or Kiln as the default app for a type. The file opens in its app, in the window Kiln is
already running in if there is one, and **Save** writes back to it. Documents already open in
that window stay open, and **Back** returns to them. Kiln opens:

| App | Files |
|---|---|
| Kiln Write | `.docx`, `.dotx`, `.docm`, `.dotm`, `.kwrite`, `.txt`, `.md` |
| Kiln Grid | `.xlsx`, `.xltx`, `.xlsm`, `.csv`, `.kgrid` |
| Kiln Deck | `.pptx`, `.potx`, `.pptm`, `.ppsx`, `.kdeck` |

OpenDocument files (`.odt`, `.ods`, `.odp`) open in the Kiln desktop app.

## Updates

Kiln updates itself. When a new version has arrived, **Kiln has been updated** appears with
**Reload**; the version you are using stays until you reload or close the last Kiln window, and
**Later** puts the notice away. Changes you have not saved come back after the reload in
**Document Recovery**, as after any unexpected close.

With **Keep recovery info** off (**Options ▸ Save**) nothing can bring those changes back, so
**Reload** first asks whether to save each document, and the browser asks before a tab with
unsaved changes closes.

## Speed

In the browser, as on the desktop, Kiln Grid recalculates a large workbook on several processor
cores at once. Each app loads when you first open one of its documents, so Kiln Home appears
quickly.
