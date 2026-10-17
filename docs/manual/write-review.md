# Reviewing and writing in Kiln Write

This chapter covers the parts of Kiln Write you use while a document is being written and
reviewed: comments, Track Changes, spelling, the Format Painter, the Markdown shortcuts and smart
typography that work as you type, focus mode and word-count goals. Everything here is saved in the
`.docx` file the way Word saves it, so comments and tracked changes made in Kiln appear in Word
with their authors and dates, and the other way round.

## Your name

Comments and tracked changes carry the reviewer's name, set in **File ▸ Options ▸ General ▸ User
name** ("Author" until you set one). Each reviewer gets a colour of their own, the same in the
margin, in the Comments pane and in the Reviewing pane.

## Comments

| To | Do |
|---|---|
| Comment on the selection (or the word at the caret) | **Review ▸ New Comment**, **Insert ▸ Comment** or Ctrl+Alt+M |
| Reply | **Reply** on the card, type, then Ctrl+Enter or **Reply** |
| Edit or delete your comment | the card's **⋯** menu |
| Resolve a thread (and reopen it) | **Resolve** on the card, **Reopen** on a resolved one |
| Go to the previous or next comment | **Review ▸ Previous** / **Next** |
| Delete one comment, or all of them | **Review ▸ Delete ▾** |
| See every thread | **Review ▸ Show Comments** (or **Comments** at the top right) opens the Comments pane |

In **Simple Markup** and **All Markup**, comments are cards in the margin to the right of the page,
next to the line they belong to; a dashed line joins the active card to its text, which is shaded.
Cards that would overlap move down. On a phone, where there is no room beside the page, a small
marker with the number of messages takes the card's place; tap it to open the thread in the
Comments pane.

The **Comments pane** lists every thread in document order with the quoted text, the replies and
their times. **Show resolved** hides or shows threads that are done. Clicking a thread selects its
text in the document.

## Track Changes

Turn tracking on with **Review ▸ Track Changes** or Ctrl+Shift+E; the status bar shows **Track
Changes: On** when you add it to the status bar (right-click the status bar). While tracking is
on, everything you type is recorded as an insertion, everything you delete stays in the document
as a deletion, and formatting changes (bold, font, paragraph settings) are recorded with what
they were before. Deleting text you inserted yourself in the same session simply removes it, as in
Word. Moving or resizing a picture is not recorded.

### What you see: the markup modes

**Review ▸ Display for Review ▾** chooses how the changes are shown. The page is laid out again for
each mode, so the page count can change.

| Mode | Shows |
|---|---|
| **Simple Markup** | the document as it would be with every change accepted, with a red bar in the margin beside each changed line |
| **All Markup** | insertions underlined and deletions struck through in the reviewer's colour, formatting changes noted, a grey bar beside changed lines |
| **No Markup** | the document as it would be with every change accepted |
| **Original** | the document as it was before the changes |

**Show Markup ▾** filters what is shown: comments, insertions and deletions, and formatting.

### Accepting and rejecting

- **Accept ▾** / **Reject ▾**: this change (and move to the next), all changes shown, or all
  changes in the document. With text selected, Accept and Reject act on the changes in the
  selection.
- **Previous** / **Next** in the Changes group go from change to change.
- The **Reviewing pane** (**Review ▸ Reviewing Pane**) lists every change with its author, time
  and a summary ("Inserted: Bright.", "Formatted: Bold"), with Accept and Reject buttons on each.

Accepting or rejecting is one undo step, even for **Accept All Changes**.

## Spelling

Kiln checks spelling as you type with the en-US Hunspell dictionary from LibreOffice. Words it does
not know get a red wavy underline. Text in another language is checked only
when Kiln has a dictionary for that language, and is never flagged otherwise. Like Word, Kiln skips
words with digits, words in capitals, and web and e-mail addresses.

| To | Do |
|---|---|
| See suggestions | right-click the underlined word; the suggestions head the menu |
| Fix it | pick a suggestion |
| Leave this one | **Ignore** |
| Leave the word everywhere in this document | **Ignore All** |
| Teach Kiln the word | **Add to Dictionary** (your dictionary is kept on this device) |
| Check the whole document | **Review ▸ Spelling & Grammar** or F7 opens the Spelling pane, which steps through each word with the same choices |

The status bar shows **No issues** or the number of spelling errors; click it to open the Spelling
pane. Turn checking as you type off in **File ▸ Options ▸ Proofing**.

In the browser the dictionary (about 550 KB) is fetched the first time a document is opened and then
works offline; the desktop and Android apps have it built in.

## The Format Painter

**Home ▸ Format Painter** copies the formatting at the caret (font, size, colour, bold and the
like, and the paragraph's style when you paint a whole paragraph). Click it once, then select the
text to paint: the brush paints once and puts itself away. Double-click it to keep painting until
you press Esc or click it again. The keyboard way is Ctrl+Shift+C to copy formatting and
Ctrl+Shift+V to paste it.

## Markdown shortcuts and smart typography

Kiln understands Markdown as you type, and turns it into Word formatting at once:

| Type | You get |
|---|---|
| `# ` … `###### ` at the start of a paragraph | Heading 1 … Heading 6 |
| `- `, `* ` or `+ ` | a bulleted list |
| `1. ` or `1) ` | a numbered list |
| `> ` | a Quote paragraph |
| `---` then Enter | a horizontal rule (a bottom border) |
| ```` ``` ```` then Enter | a code block |
| `**bold**`, `*italic*` or `_italic_`, `` `code` ``, `~~struck~~` | the formatting, without the markers |

Ctrl+Z straight after a shortcut brings back what you typed. Kiln also replaces straight quotes with
curly ones, `--` with a dash, `...` with an ellipsis and `1/2`, `1/4`, `3/4` with fractions, but not
inside code. Each of these can be turned off in **File ▸ Options ▸ Proofing ▸ AutoFormat as you
type**.

**Paste as Markdown** on the right-click menu (and **Insert ▸ Table ▾ ▸ Paste Markdown Table**)
pastes Markdown text from the clipboard
as a styled document: headings, lists, emphasis, links, code blocks and pipe tables become Word
headings, lists, formatting, links, code and tables.

## Focus mode

**View ▸ Focus Mode**, **Focus** on the status bar or the Kiln group on the Home tab hides
everything but the page: the ribbon, the rulers, the panes and the status bar fade away (move the
pointer over them to bring them back), the lines above and below the one you are writing are
dimmed, and the page scrolls so that your line stays in the middle of the window. Press Esc or
click **Focus** again to leave.

## Word-count goals

**View ▸ Word Goal** (or a click on the goal in the status bar) sets how many words you are
aiming for. The status bar shows a progress bar with the percentage; the goal is saved in the
document as a custom property (`KilnWordCountGoal`), so it travels with the file.
