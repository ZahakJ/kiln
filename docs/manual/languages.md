# Languages and right-to-left

## The interface language

Kiln's menus, ribbon, dialogs, panes, status bar and messages are in English, German (Deutsch) or
Arabic (العربية). Choose one in **File ▸ Options ▸ Language ▸ Interface language**:

- **System** follows the languages of your browser or operating system, and English when none
  of them is German or Arabic.
- **English**, **Deutsch** and **العربية** use that language whatever the system says.

The window changes language at once, and Kiln keeps the choice for the next time it starts.

The interface language changes only the interface. Documents keep their own text and language;
Excel's function names stay as Excel spells them (`SUM`, `VLOOKUP`) in every language; and cell
values keep their own number formats.

**Default language for new documents**, in the same section, is the language of each new Kiln
Write document, Grid workbook and Deck presentation. Until you pick one it matches the interface
language: with the interface in Arabic, a new workbook is مصنف1 with its sheet ورقة1 running from
the right; in German it is Mappe1 with Tabelle1. Names, spelling and hyphenation follow it, the
status bar shows it, and in a language written from right to left, such as Arabic or Hebrew,
paragraphs start at the right. Documents you open keep their own language and direction.

## Numbers and dates in the interface

Numbers and dates in the interface follow the interface language: the status bar's Sum, Average
and Count, the sample in Format Cells, page counts, file sizes and the dates in Kiln Home and
Version History. In German, 2,234.5 reads 2.234,5.

## Arabic: the interface from right to left

In Arabic the whole interface is mirrored, as Office is:

- The ribbon starts at the right with **File**; Quick Access and the title bar run from the
  right; task panes open at the left; the status bar has the counts at the right and the zoom at
  the left.
- Backstage, Kiln Home, dialogs and context menus read from the right, and submenus open to the
  left.
- Icons that point along the line are mirrored: Undo and Redo, bullets and numbering, indents,
  previous and next, and the arrows. Icons that show a side, such as Align Left, are not.
- The arrow keys follow what you see: in the ribbon, tabs, galleries and lists, Left Arrow moves
  to the next item. Key Tips and F6 follow the mirrored layout.

The document canvas is not mirrored. A document, sheet or slide keeps its own direction: an
English document reads from the left in an Arabic interface, and an Arabic paragraph reads from
the right in an English one. For right-to-left paragraphs see [Kiln Write](write.md); for
right-to-left sheets see [Kiln Grid](grid.md).

Arabic interface text is set in Noto Sans Arabic, which Kiln bundles.
