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
with a separator line. A footnote too long for what is left of its page starts under its
reference and goes on at the foot of the next page, under a separator line as wide as the column,
as in Word. The **Footnote and Endnote**
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

Pictures (**Insert ▸ Pictures**) go in the text where the caret is. **Insert ▸ Shapes ▾**
(rectangles, rounded rectangles, ovals, triangles, arrows, stars, callouts and more) and **Insert
▸ Text Box ▾ ▸ Draw Text Box** turn the pointer into a crosshair: drag on the page to draw the
shape or box at that size (a click places it at its usual size; Esc cancels). The **Text Box**
gallery also has three ready-made designs placed for you: **Simple Text Box**, **Sidebar** (a
tall tinted panel at the right margin) and **Pull Quote** (large centred text in an accent
frame, with text above and below it). Type into a shape or text box to give it text.

**Insert ▸ Icons** opens a gallery of icons (people, communication, business, nature, travel,
technology, signs and more) with a search box and a choice of colour. **Insert** puts the icon
in the text as a picture an inch square, named after the icon, with the icon's name as alt text.

Click an object to select it: a frame with handles appears, and the **Picture Format** or
**Shape Format** tab shows on the ribbon.

- **Drag** a floating object to move it; Kiln anchors it to the paragraph you drop it beside, as
  Word does. **Drag a handle** to resize it; corner handles keep a picture's proportions (hold
  Shift to stretch it), and the size is shown while you drag.
- **Layout ▸ Position ▾** puts it in line with the text or at one of nine places on the page (Top
  Left … Bottom Right) with square wrapping.
- **Layout ▸ Wrap Text ▾**: In Line with Text, Square, Tight, Through, Top and Bottom, Behind Text,
  In Front of Text.
- **Bring Forward ▾** / **Send Backward ▾** (and to front or back) change which object is on top.
- **Shift+click** (or Ctrl+click) more floating objects to select several; each gets a thin frame.
- **Align ▾** lines objects up. With several selected (**Align Selected Objects**, the default),
  they line up with each other by their left, centre or right, or top, middle or bottom, and
  **Distribute Horizontally** or **Vertically** spaces them evenly. One object lines up with the
  margins. **Align to Page** and **Align to Margin** in the same menu make every selected object line
  up with the page edges or the margins instead. **Rotate ▾** turns objects 90° either way or flips
  them.
- **Group ▾ ▸ Group** makes the selected pictures, shapes and text boxes one object that moves,
  resizes and wraps as one; **Ungroup** separates them again, all still selected. Groups are saved
  as Word groups, and groups in Word files show with all their shapes, pictures and text.
- **More Layout Options…** opens the **Layout** dialog: the exact position and what it is measured
  from (page, margin, column or paragraph), the wrapping style, the size, rotation and "Lock aspect
  ratio", and on the **Alt Text** tab the description read to people who cannot see the picture,
  or "Mark as decorative".
- The **Selection pane** (Layout ▸ Selection Pane) lists every object on the pages, front first:
  click one to select it (Shift+click to add it to the selection), double-click its name to rename
  it, click the eye to hide or show it (**Show All**, **Hide All**), and use the arrows to change the
  order. A hidden object is not shown or printed, and Word sees it hidden too.

Moving and resizing are not recorded as tracked changes.

### Picture Format

- **Crop ▾ ▸ Crop…** opens the **Crop** tab of the Layout dialog: how much to cut from the left,
  right, top and bottom, in per cent. The picture keeps its scale, so its frame gets smaller
  with what you cut. **Crop ▾ ▸ Aspect Ratio** crops to 1:1, 2:3, 3:4, 3:5, 4:5, 3:2, 4:3, 5:3,
  5:4, 16:9 or 16:10, keeping the middle of the picture. Cropping is saved in the file, so Word
  shows the same crop.
- **Reset Picture ▾**: **Reset Picture** removes the cropping, rotation and flips; **Reset Picture
  & Size** also gives the picture its own size again (at 96 pixels per inch, no wider than the
  text column).
- **Change Picture** puts another picture file in its place: the position, wrapping, width and
  alt text stay, and the height follows the new picture's proportions.
- **Height** and **Width** in the Size group set the size in points (a picture keeps its
  proportions). A picture you insert is selected, so Picture Format shows straight away.
- **Crop ▾ ▸ Crop to Shape** cuts the picture to an oval, rounded rectangle, triangle, star, heart,
  speech bubble or other shape (Rectangle goes back to a plain picture); the shape is saved with the
  picture, so Word shows the same.
- **Corrections ▾** makes the picture brighter or darker, raises or lowers its contrast, or recolours
  it (greyscale, sepia, washout); **Picture Corrections Options…** sets brightness and contrast to
  the per cent. The picture in the file stays as it was; the correction is saved as Word's.
- Under Track Changes, an inserted or deleted picture gets a frame in the reviewer's colour.

### Shape Format

- **Shape Fill** and **Shape Outline** colour a shape or text box (or take the colour away with
  **No Fill** / **No Outline**); **Weight** sets the outline's thickness, from 0.25 to 6 pt.
- **Align Text** puts the text at the top, middle or bottom of the shape. **Text Box Options…**
  opens the Layout dialog's **Text Box** tab: the vertical alignment and the internal margins.
- The **Shape Styles** gallery colours a shape the way Word's styles do (subtle to intense, in each
  theme colour, with white text on the darker ones). **Gradient ▾** shades the fill from light to
  dark, down, across or diagonally; **Shape Effects ▾** adds a shadow, a glow or soft edges.
- **Text Direction ▾** turns the text in a shape 90° or 270°.
- Height and Width are in the Size group, as for pictures.

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
