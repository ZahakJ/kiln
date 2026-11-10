# Options, document properties and checking before you share

## File ▸ Options

**File ▸ Options** (or **Settings** on Kiln Home) opens the Options dialog. Its sections are listed on the left; every change applies at once and is kept on this device.

### General

- **Show Mini Toolbar on selection**: formatting buttons beside the text you select.
- **Enable Live Preview**: pointing at a style, colour or effect in a gallery shows it on the document before you choose it.
- **Classic Office look**: colourful ribbon icons and a title bar in the colour of the app, as in Office 2019.
- **Theme**: light, dark, high contrast, or as the system is set.
- **User name** and **Initials**: used for comments, tracked changes and document properties.

### Display

- **Ribbon layout**: Classic, Simplified, or automatic (Classic on wide windows).
- **ScreenTip style**: with feature descriptions, without them, or no ScreenTips at all.
- **Show shortcut keys in ScreenTips**.

### Proofing

- **When correcting spelling**: ignore words in UPPERCASE, words that contain numbers, and Internet and file addresses. Turning one off checks those words too, in every open document.
- **Check spelling as you type**.
- **AutoCorrect**: replace text as you type, capitalise sentences and the names of days, correct TWo INitial CApitals, and the list of replacements (add your own or delete built-in ones).
- **AutoFormat as you type**: Markdown shortcuts, smart quotes, dashes, the ellipsis and fraction characters.

### Save

- **Turn on AutoSave when a file opens**: files that can be saved where they are get the title bar's AutoSave switch turned on.
- **Save after a pause of (seconds)**: how long AutoSave waits after your last change before it writes the file.
- **Keep AutoRecover information**: Kiln records every change as you make it, so a crash brings back what was on screen ([Document Recovery](files.md#document-recovery) says exactly what). Applies to documents opened after the change.
- **Save new files in**: Office formats, or OpenDocument formats in the desktop app. The first Save of a new document then asks where to save it in that format.

### Language

- **Interface language**: English, Deutsch or العربية (or the system's).
- **Default language for new documents**: new documents, workbooks and presentations are in this language. **Match display language**, the default, follows the interface language (English (United States) for English, German (Germany) for Deutsch, Arabic (Saudi Arabia) for العربية, or the system's language when the interface follows the system) and names the language it gives; pick a language to keep it whatever the interface is. Their names follow it (Document1, Dokument1, مستند1; Book1, Mappe1, مصنف1; Presentation1, Präsentation1, عرض تقديمي1), and so do the first sheet's name (Sheet1, Tabelle1, ورقة1), spelling and hyphenation. In a right-to-left language (Arabic, Hebrew, Persian, Urdu) paragraphs start at the right, a new workbook's sheet runs from the right and a new presentation's slide master is mirrored, with right-aligned, right-to-left text. Files you open keep their own direction.

### Advanced

- **Allow text to be dragged and dropped** (Kiln Write): drag selected text to move it, or hold Ctrl to copy it.
- **After pressing Enter, move selection** (Kiln Grid), and its **Direction**: down, right, up or left. Shift+Enter goes the other way.
- **Show measurements in units of**: inches, centimetres, millimetres or points.
- **Show this number of Recent Documents**: on Kiln Home and in **File ▸ Open**. Pinned files are always shown.
- In Kiln Grid, **When calculating this workbook ▸ Use 1904 date system**.

### Keyboard

Every command with its shortcuts, searchable by name or by key. Select a command, click **Press new shortcut**, press the keys and choose **Assign**. When the keys already run another command, Kiln says which before you assign them; assigning moves the keys to the new command. **Reset** restores one command's shortcuts, **Reset All** restores Office's.

**Export** saves your shortcuts as `kiln-shortcuts.json`; **Import…** reads such a file, for example on another computer. When an imported file puts two commands on one key, Kiln lists them above the commands with a link to each.

### Privacy

Kiln has no telemetry. Recent files, pinned files and your preferences stay on this device; recovery journals and version history are kept beside your documents. A shared document goes only to the collaboration server named in its link. The writing assistant is off unless you choose one.

## Document properties

**File ▸ Info ▸ Edit Properties** opens **Properties**: title, subject, author, manager, company, category, tags, status and comments, with the document's size (pages and words, worksheets, or slides) and when it was created and modified. Kiln Write, Grid and Deck keep them in Word, Excel and PowerPoint files and in Kiln's own files, where Office and file managers read them.

## Check before you share

**File ▸ Info** has two checks:

- **Inspect Document** looks for what travels with the file: comments (and notes in Kiln Grid), tracked changes, personal information in the properties (author, last modified by, company, manager), hidden worksheets, hidden slides and speaker notes. **Remove All** beside each one takes it out; for tracked changes it accepts them. Undo brings back comments, changes, sheets and slides.
- **Check compatibility** lists, for the format you choose, what will be lost, what will be saved differently, and what was kept as it was read when the file was opened. It is the same check Save As makes before it saves a macro-enabled document as one without macros.
