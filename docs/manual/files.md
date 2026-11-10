# Files, AutoSave and recovery

Kiln opens and saves Office files directly, and has native files of its own. While you work it keeps a recovery copy of every changed document, so a crash, a closed window or tab, or a reload does not take your work with it. On the desktop each change is on the disk a moment after you make it, so a power cut does not take it either.

## Kiln's own files

Kiln Write, Kiln Grid and Kiln Deck each have a native file type: `.kwrite`, `.kgrid` and `.kdeck`. Choose them in **File ▸ Save As** (**Kiln Write Document**, **Kiln Grid Workbook**, **Kiln Deck Presentation**). A native file holds:

- the document exactly as Kiln edits it;
- its version history (Kiln Write and Kiln Deck; see below), so the versions travel with the file;
- the Office file it was opened from, when there was one, with a note of which of its parts your edits touched. Saving the native file as `.docx`, `.xlsx` or `.pptx` again writes exactly what saving the Office file directly would: the parts you changed from the document, and every other part (macros, custom XML, settings and styles with markup Kiln does not show) byte for byte as it was. If that original cannot be read (the native file was damaged), the Compatibility Checker lists it as a loss and AutoSave stays off for the Office format.

A native file is a zip archive with a `manifest.json` that says what is inside and which version of the format wrote it. A file written by a newer version of Kiln than yours is not opened; Kiln says which version it needs.

The command line converts both ways:

```sh
kiln-cli convert Report.docx Report.kwrite
kiln-cli convert Report.kwrite Report.docx
```

## AutoSave

The chip after the document's name in the title bar is the AutoSave switch. With AutoSave on, Kiln saves the file two seconds after you stop typing, the way **Save** does, and the chip says **Saved**. Select the chip to turn AutoSave on or off for the document.

AutoSave needs a file: the switch appears once a new document is saved. It stays off, and the chip's tooltip says why, when the document is read-only or when saving in the file's format would lose something that **Save** would warn you about: macros in a document saved as `.docx` rather than `.docm`, every sheet but the active one in a `.csv`, the formatting of a `.txt`. If an edit makes the format lose something while AutoSave is on (a second sheet in a `.csv`), AutoSave turns itself off and says why; save in a format that keeps everything (**File ▸ Save As**) to turn it on again.

Kiln never writes over a change someone else made. Before each save of the file (AutoSave's and your own) it checks that the file is as Kiln last left it; if another program, or Kiln in another browser tab, changed it in the meantime, nothing is written: AutoSave pauses, the chip's tooltip says so, and Kiln asks whether to reload the file (see below). In the browser, AutoSave is on for a file in one tab at a time; turning it on in a second tab with the same file says it is on in the other.

AutoSave saves do not add versions to the version history; saves you make do.

## Closing a document

Closing a document with changes that are not saved asks, as Office does: **Save** saves it and closes it, **Don't Save** closes it and drops the changes (and their recovery copy), **Cancel** keeps it open. With AutoSave on, closing writes the last changes first and asks nothing; if they cannot be written, the document stays open with the reason.

## Document Recovery

Kiln keeps a recovery copy of each document from the first change on: the document as it was, then every change as you make it, undo and redo included, even an undo or redo that changes no cell. A change that is on your screen is in the recovery copy, apart from the few that take a moment longer (below): reload the page, close the tab or lose the page to a crash the moment after, and it comes back, while the copy starts again too. A key that has not reached the screen when the page goes, which a busy workbook can take a moment over, is not part of what comes back.

The document comes back with what you had entered, and nothing open for editing. If you were typing in a Kiln Grid cell, the message bar under the ribbon names the cell and the text, and **Put it back** opens the cell with that text so you can enter it; the cell keeps its old value until you do. **Put it back** finds the cell on its sheet even after the sheet is renamed or moved; if the sheet has been deleted, the message says so and keeps the text, with **Copy**. The message takes the keys when it appears, with **Put it back** focused, and F6 comes back to it. **Copy** says when the text is on the clipboard, or that it could not copy it.

Everything comes back as it was on screen (cells, formulas, formats, merges, notes, comments, names, conditional formats, validation, tables, filters, charts, pictures and the order and names of the sheets), except the values of cells that hold `RAND`, `NOW`, `TODAY` and the like and what depends on them: as when a workbook opens, they are worked out afresh. A sort, filter, fill, paste or Remove Duplicates that went by them keeps the rows, the order and the values you saw.

A few changes take a moment longer to reach the copy: in a browser, a single change over 2 MB (a very large paste); and anywhere, a change made while others edit a shared document, and the first changes to a file over 256 KB, until its first copy is written. In a browser Kiln asks before the page goes while they do. If Kiln closes in that moment, the document comes back as it was before that change, and the message bar names the change and how many came after it; the same message names a change Kiln could not repeat, should one ever fail to replay. If nothing of the document can come back, the Document Recovery pane lists it with the reason. A power cut or a crash of the whole browser can also take the last few seconds of changes: the browser writes its storage to the disk on its own schedule.

Every 200 changes the copy starts again from the document as it is; the new copy is written beside the old one and takes over only once it is complete, and changes made meanwhile go on being kept, so a crash at any moment leaves a whole copy with every change in it, up to the limits above. Saving marks the copy as saved, unless you went on typing while the file was written. Closing a document whose changes are saved, or closing it with **Don't Save**, removes the copy; closing it any other way keeps it for Document Recovery.

If Kiln cannot write the recovery copy (the disk or the browser's storage for the site is full), the message bar says so; save the document to keep your work. What was written before stays recoverable, and the next change writes a whole new copy once writing works again.

When Kiln starts after it closed before a document was saved (a crash, a closed browser tab, a reload), the **Document Recovery** pane opens on the left of the window and lists those documents, newest first, each with the time of its recovery copy and where its file is (**Never saved to a file** for a new document). For each one:

- **Open** opens it as it was at the moment Kiln closed, marked as not saved. Save it to keep it. From then on its recovery copy goes on in this window, so a second crash keeps everything again.
- **Save As…** opens it and asks where to save it.
- **Delete** removes the recovery copy.

Documents you neither open nor delete stay in the list for the next time. A recovery copy written by a newer version of Kiln is listed with the reason it cannot be opened, and can be deleted. A document open in another window (another browser tab, or another Kiln on the desktop) is never listed, and if another window opens or deletes one while the list is showing, **Open** and **Delete** say so and the list is read again.

On the desktop, each change is written to its recovery copy as it is made, in Kiln's data folder, which every Kiln window and process uses; every write is flushed to the disk before the next one starts, and a copy that is replaced is written beside the old one and takes its place only once it is on the disk. In the browser they go to the site's own storage.

## Version history of your files

Kiln Write and Kiln Deck documents keep versions of themselves: one each time you save them and one for every ten minutes of editing. Open **Review ▸ Version History** to see them, newest first:

- **Select a version** to show it read-only in place of the document; **Back to the document** returns.
- **Compare** (Kiln Write) shows the words added and removed since that version.
- In Kiln Deck each version shows its slides as thumbnails, the new and changed ones marked.
- **Name** gives the version a name. Named versions are always kept.
- **Restore** makes the document that version again, as a new change: save it to keep it. The restored state is a version too, so nothing is lost.

A document keeps up to 200 versions; past that, automatic versions are thinned out where they are closest together, then the oldest unnamed ones. A native file keeps its versions inside the file. Any other file (`.docx`, `.pptx` and the rest) keeps them on this device, and they follow the document: open the file again and its versions are there; move or rename it with another program, or pick it again in the browser, and Kiln finds them by the file's contents; **Save As** takes them along to the new name. A different document saved at the same place by another program starts a history of its own, so a version shown or restored is always one of the document you have open.

Versions kept on the device take at most 512 MB together; past that the oldest unnamed versions of all files go first (each file's latest and named versions stay). When Kiln starts, it removes the versions of files that have been gone for 30 days. Shared documents keep their versions with the shared document instead (see [Collaboration and version history](collaboration.md)).

## Recent and pinned files

Kiln Home and **File ▸ Open** list the files you opened or saved, most recent first. Select the pin beside a file to keep it at the top; pinned files are also under **Pinned**. Kiln keeps the 50 most recent files besides the pinned ones.

In the browser, a file you picked from your device opens again from the list; the browser may ask once whether Kiln may read it. When the browser needs your permission again before Kiln can save to it (after a reload), saving stops with a message and a button, **Allow Kiln to save to** the file, which asks the browser and then saves.

## A file changed by another program

When another program changes a file you have open, Kiln asks whether to reload it: on the desktop as soon as the file changes, in the browser when Kiln is about to save it. **Reload** reads the file again and drops the changes made in Kiln since the last save; **Keep my version** goes on with yours and saves it, writing over the other change. Until you answer, nothing is written to the file and AutoSave waits; closing the question answers nothing. Kiln's own saves never ask.

## Spelling dictionary

Words you add with **Add to Dictionary** are kept for every document on this device.
