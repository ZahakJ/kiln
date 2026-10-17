# References, pictures and page furniture in Kiln Write

This chapter covers the parts of a document that Kiln Write builds for you or that sit around the
text: tables of contents, footnotes and endnotes, captions and cross-references, pictures, shapes
and text boxes that float beside the text, cover pages, code blocks and callouts, hyphenation and
the document's properties. All of them are saved as Word saves them (fields, notes, DrawingML
objects and content controls), so Word updates and edits them as its own.

## Table of contents

**References ▸ Table of Contents ▾** offers two automatic tables (with the title "Contents" or
"Table of Contents") built from the Heading 1–3 styles, **Custom Table of Contents…** and
**Remove Table of Contents**. **Insert ▸ Table of Contents** is the one-click version.

The **Table of Contents** dialog sets the title, whether page numbers are shown and right-aligned,
the tab leader (dots, dashes, a line or none), whether entries are links in Web Layout and how many
heading levels to show, with a preview of the result.

- **Update Table** (References ▸ Update Table ▾) refreshes the page numbers only, or the entire
  table (new, renamed or removed headings).
- **Add Text ▾** sets the level at which the current paragraph appears in the table (Level 1–3, or
  Do Not Show in Table of Contents) by giving it the matching heading style.
- Every entry is a link: Ctrl+click it to go to the heading.

Page numbers come from Kiln's own page layout, so they match what you see and print.

## Footnotes and endnotes

| To | Do |
|---|---|
| Insert a footnote | **References ▸ Insert Footnote** or Ctrl+Alt+F |
| Insert an endnote | **References ▸ Insert Endnote** or Ctrl+Alt+D |
| Go to the next or previous note | **References ▸ Next Footnote ▾** |
| Go between a reference and its note | **References ▸ Show Notes** |
| Change numbering or convert notes | the Footnotes group's dialog launcher ↘ |

The note's text goes at the foot of the page (footnotes) or at the end of the document (endnotes),
with a separator line. The **Footnote and Endnote**
dialog sets the number format (1, 2, 3; i, ii, iii; I, II, III; a, b, c; A, B, C; *, †, ‡; ①, ②,
③), the starting number and whether numbering restarts on each page or section, and converts all
footnotes to endnotes, all endnotes to footnotes, or swaps them.

## Captions and cross-references

**References ▸ Insert Caption** numbers a figure, table or equation: "Figure 1: Tiles after the
firing". Choose the label (Figure, Table, Equation or one you add with **New Label…**), whether
the caption goes above or below the selected item, and whether the number includes the chapter
(the Heading 1 number), as in "Figure 2-1". The numbers are `SEQ` fields: they renumber when you
add or move captions.

**Insert ▸ Cross-reference** (or **References ▸ Cross-reference**) inserts a reference to a
heading, a bookmark, a footnote or endnote, or a figure, table or equation caption. Choose what to
show: the text, the label and number only, the page number, or "above"/"below". A reference is a
`REF`, `PAGEREF` or `NOTEREF` field that follows its target when it moves or is renumbered, and,
with **Insert as hyperlink**, a link to it (Ctrl+click).

## Pictures, shapes and text boxes

Pictures (**Insert ▸ Pictures**), shapes (**Insert ▸ Shapes ▾**: rectangles, rounded rectangles,
ovals, triangles, arrows, stars, callouts and more) and text boxes (**Insert ▸ Text Box**) go in
the text where the caret is. Click one to select it: a frame with handles appears.

- **Drag** a floating object to move it; Kiln anchors it to the paragraph you drop it beside, as
  Word does. **Drag a handle** to resize it; corner handles keep a picture's proportions (hold
  Shift to stretch it), and the size is shown while you drag.
- **Layout ▸ Position ▾** puts it in line with the text or at one of nine places on the page (Top
  Left … Bottom Right) with square wrapping.
- **Layout ▸ Wrap Text ▾**: In Line with Text, Square, Tight, Through, Top and Bottom, Behind Text,
  In Front of Text.
- **Bring Forward ▾** / **Send Backward ▾** (and to front or back) change which object is on top.
- **Align ▾** lines it up with the left, centre or right of the margins, or their top, middle or
  bottom. **Rotate ▾** turns it 90° either way or flips it.
- **More Layout Options…** opens the **Layout** dialog: the exact position and what it is measured
  from (page, margin, column or paragraph), the wrapping style, the size, rotation and "Lock aspect
  ratio", and on the **Alt Text** tab the description read to people who cannot see the picture,
  or "Mark as decorative".
- The **Selection pane** (Layout ▸ Selection Pane) lists every object on the pages, front first:
  click one to select it, double-click its name to rename it, and use the arrows to change the
  order.

Moving and resizing are not recorded as tracked changes.

## Cover pages

**Insert ▸ Cover Page ▾** has six designs: **Kiln** (a large serif title on a calm page),
**Banded** (a band of colour across the top), **Ledger** (a rule under a left-aligned title),
**Classic** (centred, between rules), **Slab** (a dark slab behind the title) and **Quiet** (a
small title low on the page). The cover takes the title, subtitle and author from the document's
properties and today's date, and ends with a page break. Choosing another design replaces the
cover (Undo takes it away). In Word it is a cover page from the Cover Pages
gallery.

## Code blocks and callouts

**Insert ▸ Code Block ▾** inserts a block in a monospace style for a language (or plain text), with
syntax highlighting for common languages (keywords, strings, comments and numbers in colour).
Typing ```` ``` ```` and Enter starts one too, and Enter on an empty last line ends it.

**Insert ▸ Callout ▾** inserts a shaded, bordered paragraph for a **Note**, **Tip**, **Warning** or
**Important** message. Callouts and code blocks are ordinary paragraphs with borders and shading in
the `.docx` file, so they look the same in Word.

## Hyphenation

**Layout ▸ Hyphenation** turns automatic hyphenation on or off for the document (Word's
`autoHyphenation` setting).

## Document properties

**File ▸ Info ▸ Properties** edits the title, subject, author, tags and comments that Word keeps
with the file, and shows the page, word and character counts; the cover pages use them.
