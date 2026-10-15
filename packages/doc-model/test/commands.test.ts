import { EditorState, TextSelection } from '@tiptap/pm/state';
import { CellSelection, TableMap } from '@tiptap/pm/tables';
import { describe, expect, it } from 'vitest';
import { createDocCommands } from '../src/commands.ts';
import { computeListLabels } from '../src/numbering/labels.ts';
import { resolveAt, runPropsFromMarks } from '../src/styles/resolve.ts';
import { DefaultTemplate } from '../src/template.ts';
import type { SectionProps } from '../src/types.ts';
import { cell, doc, p, paragraphs, posOf, row, run, schema, stateWith, table, text } from './helpers.ts';

function setup() {
  const wd = DefaultTemplate.create();
  const ctx = { styles: wd.styles, numbering: wd.numbering, theme: wd.theme, settings: wd.settings };
  return { ctx, cmd: createDocCommands(ctx) };
}

const runAt = (state: EditorState, needle: string) => {
  const pos = posOf(state.doc, needle) + 1;
  return runPropsFromMarks(state.doc.resolve(pos).marks());
};

describe('DocCommands — character formatting', () => {
  it('toggleProperty bold sets and clears direct bold', () => {
    const { cmd } = setup();
    let s = stateWith(doc(p('Hello world')), 'world');
    s = run(s, cmd.toggleProperty('bold'));
    expect(runAt(s, 'world').bold).toBe(true);
    expect(runAt(s, 'Hello').bold).toBeUndefined();
    s = run(s, cmd.toggleProperty('bold'));
    expect(runAt(s, 'world').bold).toBeUndefined();
  });

  it('Ctrl+B on text bold from its style writes bold=false (like <w:b w:val="0"/>)', () => {
    const { ctx, cmd } = setup();
    let s = stateWith(doc(p('Plain'), p({ styleId: 'Title' }, 'Heading text')), 'Heading');
    s = run(s, cmd.applyStyle('Strong'));
    expect(resolveAt(ctx, s.doc, posOf(s.doc, 'Heading') + 1)?.run.bold).toBe(true);
    // Make the whole paragraph bold via a style-like run style, then toggle off.
    s = run(s, cmd.toggleProperty('bold'));
    expect(runAt(s, 'Heading').bold).toBe(false);
    expect(resolveAt(ctx, s.doc, posOf(s.doc, 'Heading') + 1)?.run.bold).toBe(false);
  });

  it('toggle on a mixed selection turns everything on first', () => {
    const { cmd } = setup();
    let s = stateWith(doc(p(text('bold', { bold: true }), ' plain')), 'bold', { to: 'plain' });
    s = run(s, cmd.toggleProperty('bold'));
    expect(runAt(s, 'plain').bold).toBe(true);
    expect(runAt(s, 'bold').bold).toBe(true);
  });

  it('toggle at a cursor sets stored marks', () => {
    const { cmd } = setup();
    let s = stateWith(doc(p('abc')), 'b', { cursor: true });
    s = run(s, cmd.toggleProperty('italic'));
    expect(s.storedMarks?.map((m) => m.type.name)).toEqual(['italic']);
    const typed = s.apply(s.tr.insertText('X'));
    expect(runAt(typed, 'X').italic).toBe(true);
  });

  it('toggleUnderline and toggleVertAlign', () => {
    const { cmd } = setup();
    let s = stateWith(doc(p('H2O x2')), '2', { to: '2' });
    s = run(s, cmd.toggleVertAlign('subscript'));
    expect(runAt(s, '2').vertAlign).toBe('subscript');
    s = run(s, cmd.toggleVertAlign('subscript'));
    expect(runAt(s, '2').vertAlign).toBeUndefined();
    s = run(stateWith(s.doc, 'x2'), cmd.toggleUnderline('double'));
    expect(runAt(s, 'x2').underline?.style).toBe('double');
    s = run(s, cmd.toggleUnderline());
    expect(runAt(s, 'x2').underline).toBeUndefined();
  });

  it('underline from the Hyperlink style is turned off with an explicit none', () => {
    const { cmd } = setup();
    let s = stateWith(doc(p(text('link', { styleId: 'Hyperlink' }))), 'link');
    s = run(s, cmd.toggleUnderline());
    expect(runAt(s, 'link').underline?.style).toBe('none');
  });

  it('font family, size, grow and shrink on Word’s ladder', () => {
    const { ctx, cmd } = setup();
    let s = stateWith(doc(p('Sized')), 'Sized');
    s = run(s, cmd.setFontFamily('Arial'));
    expect(runAt(s, 'Sized').fonts?.ascii).toBe('Arial');
    s = run(s, cmd.growFont());
    expect(runAt(s, 'Sized').size).toBe(24); // 11 → 12
    s = run(s, cmd.growFont());
    expect(runAt(s, 'Sized').size).toBe(28); // 12 → 14
    s = run(s, cmd.shrinkFont());
    expect(runAt(s, 'Sized').size).toBe(24);
    s = run(s, cmd.setFontSize(72));
    s = run(s, cmd.growFont());
    expect(runAt(s, 'Sized').size).toBe(160); // 72 → 80
    s = run(s, cmd.shrinkFont());
    expect(runAt(s, 'Sized').size).toBe(144);
    expect(resolveAt(ctx, s.doc, posOf(s.doc, 'Sized') + 1)?.run.resolvedFonts.ascii).toBe('Arial');
  });

  it('setMark / removeMark for colour and highlight', () => {
    const { cmd } = setup();
    let s = stateWith(doc(p('Color me')), 'Color');
    s = run(s, cmd.setMark('color', { color: { val: 'FF0000' } }));
    s = run(s, cmd.setMark('highlight', { color: 'yellow' }));
    expect(runAt(s, 'Color')).toMatchObject({ color: { val: 'FF0000' }, highlight: 'yellow' });
    s = run(s, cmd.removeMark('highlight'));
    expect(runAt(s, 'Color').highlight).toBeUndefined();
    expect(cmd.setMark('nonexistent')(s)).toBe(false);
  });

  it('changeCase modes', () => {
    const { cmd } = setup();
    const base = doc(p('hello WORLD. second sentence'));
    const all = (mode: Parameters<typeof cmd.changeCase>[0]) =>
      run(stateWith(base, 'hello', { to: 'sentence' }), cmd.changeCase(mode)).doc.textContent;
    expect(all('upper')).toBe('HELLO WORLD. SECOND SENTENCE');
    expect(all('lower')).toBe('hello world. second sentence');
    expect(all('sentence')).toBe('Hello world. Second sentence');
    expect(all('title')).toBe('Hello World. Second Sentence');
    expect(all('toggle')).toBe('HELLO world. SECOND SENTENCE');
  });

  it('clearFormatting keeps links and comments, resets paragraphs to Normal', () => {
    const { cmd } = setup();
    const linkMark = schema.mark('link', { href: 'https://x.org' });
    const d = doc(
      p({ styleId: 'Heading1', alignment: 'center' }, schema.text('styled', [schema.mark('bold'), linkMark])),
    );
    const s = run(stateWith(d, 'styled'), cmd.clearFormatting());
    const para = s.doc.firstChild?.firstChild;
    expect(para?.attrs.styleId).toBe('Normal');
    expect(para?.attrs.alignment).toBeNull();
    expect(s.doc.firstChild?.firstChild?.firstChild?.marks.map((m) => m.type.name)).toEqual(['link']);
  });
});

describe('DocCommands — paragraphs and styles', () => {
  it('applyStyle sets the paragraph style of all selected paragraphs', () => {
    const { cmd } = setup();
    const s = run(
      stateWith(doc(p('one'), p('two'), p('three')), 'one', { to: 'two' }),
      cmd.applyStyle('Heading2'),
    );
    expect(paragraphs(s.doc).map((x) => x.attrs.styleId)).toEqual(['Heading2', 'Heading2', null]);
  });

  it('linked paragraph style on a partial selection applies the character style', () => {
    const { cmd } = setup();
    const s = run(stateWith(doc(p('one two three')), 'two'), cmd.applyStyle('Heading1'));
    expect(paragraphs(s.doc)[0]?.attrs.styleId).toBeNull();
    expect(runAt(s, 'two').styleId).toBe('Heading1Char');
  });

  it('applyStyle rejects unknown styles and applies character styles as marks', () => {
    const { cmd } = setup();
    const s0 = stateWith(doc(p('word')), 'word');
    expect(cmd.applyStyle('Missing')(s0)).toBe(false);
    expect(runAt(run(s0, cmd.applyStyle('Emphasis')), 'word').styleId).toBe('Emphasis');
  });

  it('alignment, line spacing and paragraph props', () => {
    const { ctx, cmd } = setup();
    let s = stateWith(doc(p('text')), 'text');
    s = run(s, cmd.setAlignment('both'));
    s = run(s, cmd.setLineSpacing(1.5));
    s = run(s, cmd.setParagraphProps({ spacing: { before: 120 }, keepNext: true }));
    const para = paragraphs(s.doc)[0];
    expect(para?.attrs).toMatchObject({
      alignment: 'both',
      keepNext: true,
      spacing: { line: 360, lineRule: 'auto', before: 120 },
    });
    const r = resolveAt(ctx, s.doc, posOf(s.doc, 'text'));
    expect(r?.paragraph.spacing).toMatchObject({ before: 120, after: 160, line: 360 });
  });

  it('indent/outdent move non-list paragraphs between 0.5" stops', () => {
    const { cmd } = setup();
    let s = stateWith(doc(p({ indent: { left: 300 } }, 'text')), 'text');
    s = run(s, cmd.indent());
    expect(paragraphs(s.doc)[0]?.attrs.indent.left).toBe(720);
    s = run(s, cmd.indent());
    expect(paragraphs(s.doc)[0]?.attrs.indent.left).toBe(1440);
    s = run(s, cmd.outdent());
    s = run(s, cmd.outdent());
    expect(paragraphs(s.doc)[0]?.attrs.indent.left).toBe(0);
    expect(cmd.outdent()(s)).toBe(false);
  });
});

describe('DocCommands — lists', () => {
  it('toggleBulletList creates a list, applies List Paragraph, toggles off', () => {
    const { ctx, cmd } = setup();
    let s = stateWith(doc(p('a'), p('b')), 'a', { to: 'b' });
    s = run(s, cmd.toggleBulletList());
    const paras = paragraphs(s.doc);
    expect(paras.map((x) => x.attrs.styleId)).toEqual(['ListParagraph', 'ListParagraph']);
    expect(new Set(paras.map((x) => x.attrs.numbering.numId)).size).toBe(1);
    expect([...computeListLabels(s.doc, ctx).values()].map((l) => l.text)).toEqual(['•', '•']);
    s = run(s, cmd.toggleBulletList());
    expect(paragraphs(s.doc).map((x) => [x.attrs.styleId, x.attrs.numbering])).toEqual([
      [null, null],
      [null, null],
    ]);
  });

  it('numbered list continues an adjacent list and outdents/indents levels', () => {
    const { ctx, cmd } = setup();
    let s = stateWith(doc(p('one'), p('two'), p('three')), 'one');
    s = run(s, cmd.toggleNumberedList());
    s = run(stateWith(s.doc, 'two'), cmd.toggleNumberedList());
    s = run(stateWith(s.doc, 'three'), cmd.toggleNumberedList());
    expect([...computeListLabels(s.doc, ctx).values()].map((l) => l.text)).toEqual(['1.', '2.', '3.']);
    s = run(stateWith(s.doc, 'two'), cmd.indent());
    expect([...computeListLabels(s.doc, ctx).values()].map((l) => l.text)).toEqual(['1.', 'a.', '2.']);
    s = run(stateWith(s.doc, 'two'), cmd.outdent());
    expect([...computeListLabels(s.doc, ctx).values()].map((l) => l.text)).toEqual(['1.', '2.', '3.']);
  });

  it('restartNumbering restarts at the paragraph and following items', () => {
    const { ctx, cmd } = setup();
    let s = stateWith(doc(p('a'), p('b'), p('c'), p('d')), 'a', { to: 'd' });
    s = run(s, cmd.toggleNumberedList());
    s = run(stateWith(s.doc, 'c', { cursor: true }), cmd.restartNumbering());
    expect([...computeListLabels(s.doc, ctx).values()].map((l) => l.text)).toEqual(['1.', '2.', '1.', '2.']);
  });

  it('setNumbering / removeNumbering', () => {
    const { ctx, cmd } = setup();
    const list = ctx.numbering.createMultilevelList();
    let s = run(stateWith(doc(p('x')), 'x'), cmd.setNumbering(list.id, 1));
    expect(paragraphs(s.doc)[0]?.attrs.numbering).toEqual({ numId: list.id, level: 1 });
    expect([...computeListLabels(s.doc, ctx).values()][0]?.text).toBe('1.1.');
    s = run(s, cmd.removeNumbering());
    expect(paragraphs(s.doc)[0]?.attrs.numbering).toBeNull();
  });
});

describe('DocCommands — tables', () => {
  it('insertTable inserts a Table Grid table sized to the text column and puts the cursor inside', () => {
    const { cmd } = setup();
    const s = run(stateWith(doc(p('before')), 'before', { cursor: true }), cmd.insertTable(2, 3));
    let tbl: import('@tiptap/pm/model').Node | undefined;
    s.doc.descendants((n) => {
      if (n.type.name === 'table') tbl = n;
      return !tbl;
    });
    expect(tbl?.attrs.styleId).toBe('TableGrid');
    expect(tbl?.attrs.grid).toEqual([3120, 3120, 3120]);
    const map = tbl ? TableMap.get(tbl) : undefined;
    expect([map?.width, map?.height]).toEqual([3, 2]);
    expect(s.selection.$from.node(-1).type.name).toBe('tableCell');
    expect(() => s.doc.check()).not.toThrow();
    expect(cmd.insertTable(0, 2)(s)).toBe(false);
  });

  it('add/delete rows and columns keep the grid in sync', () => {
    const { cmd } = setup();
    const d = doc(
      table(
        { styleId: 'TableGrid', grid: [4680, 4680] },
        row(cell(p('a1')), cell(p('b1'))),
        row(cell(p('a2')), cell(p('b2'))),
      ),
    );
    let s = stateWith(d, 'a1', { cursor: true });
    s = run(s, cmd.addRowAfter);
    s = run(s, cmd.addColumnAfter);
    let t = s.doc.firstChild?.firstChild;
    expect(t?.childCount).toBe(3);
    expect(t?.attrs.grid).toHaveLength(3);
    s = run(stateWith(s.doc, 'a1', { cursor: true }), cmd.deleteColumn);
    s = run(stateWith(s.doc, 'b2', { cursor: true }), cmd.deleteRow);
    t = s.doc.firstChild?.firstChild;
    expect(t?.attrs.grid).toHaveLength(2);
    expect(t?.childCount).toBe(2);
    expect(t?.textContent).toBe('b1');
  });

  it('mergeCellRange merges a rectangle; splitCell restores single cells', () => {
    const { cmd } = setup();
    const d = doc(table({}, row(cell(p('a')), cell(p('b'))), row(cell(p('c')), cell(p('d')))));
    let s = EditorState.create({ doc: d, schema });
    s = run(s, cmd.mergeCellRange(1, { top: 0, left: 0, bottom: 1, right: 2 }));
    const merged = s.doc.firstChild?.firstChild?.firstChild?.firstChild;
    expect(merged?.attrs.colspan).toBe(2);
    expect(merged?.textContent).toBe('ab');
    expect(cmd.mergeCellRange(1, { top: 0, left: 0, bottom: 5, right: 2 })(s)).toBe(false);
    const cellPos = posOf(s.doc, 'a');
    s = s.apply(s.tr.setSelection(TextSelection.create(s.doc, cellPos)));
    s = run(s, cmd.splitCell);
    expect(TableMap.get(s.doc.firstChild?.firstChild as import('@tiptap/pm/model').Node).map).toHaveLength(4);
  });

  it('mergeCells works on a CellSelection', () => {
    const { cmd } = setup();
    const d = doc(table({}, row(cell(p('a'))), row(cell(p('c')))));
    let s = EditorState.create({ doc: d, schema });
    const a = posOf(d, 'a') - 2;
    const c = posOf(d, 'c') - 2;
    s = s.apply(s.tr.setSelection(CellSelection.create(s.doc, a, c)));
    s = run(s, cmd.mergeCells);
    expect(s.doc.firstChild?.firstChild?.firstChild?.firstChild?.attrs.rowspan).toBe(2);
  });
});

describe('DocCommands — breaks and sections', () => {
  const props = (s: EditorState, i: number) => s.doc.child(i).attrs.props as SectionProps;

  it('insertSectionBreak splits the paragraph and the section', () => {
    const { cmd } = setup();
    let s = stateWith(doc(p('first part'), p('end')), 'part', { cursor: true });
    s = run(s, cmd.insertSectionBreak('continuous'));
    expect(s.doc.childCount).toBe(2);
    expect(s.doc.child(0).textContent).toBe('first ');
    expect(s.doc.child(1).textContent).toBe('partend');
    expect(props(s, 0).type).toBe('nextPage');
    expect(props(s, 1).type).toBe('continuous');
    expect(() => s.doc.check()).not.toThrow();
  });

  it('section break inside a table goes after the table', () => {
    const { cmd } = setup();
    let s = stateWith(doc(table({}, row(cell(p('in cell')))), p('after')), 'cell', { cursor: true });
    s = run(s, cmd.insertSectionBreak('oddPage'));
    expect(s.doc.childCount).toBe(2);
    expect(s.doc.child(0).firstChild?.type.name).toBe('table');
    expect(s.doc.child(1).textContent).toBe('after');
    expect(props(s, 1).type).toBe('oddPage');
  });

  it('section break in a table at the end of the section adds a new section', () => {
    const { cmd } = setup();
    const s = run(
      stateWith(doc(table({}, row(cell(p('only'))))), 'only', { cursor: true }),
      cmd.insertSectionBreak('nextPage'),
    );
    expect(s.doc.childCount).toBe(2);
    expect(s.doc.child(1).firstChild?.type.name).toBe('paragraph');
  });

  it('page setup commands update the current section only', () => {
    const { cmd } = setup();
    let s = stateWith(doc(p('first part'), p('end')), 'part', { cursor: true });
    s = run(s, cmd.insertSectionBreak('nextPage'));
    s = run(stateWith(s.doc, 'end', { cursor: true }), cmd.setOrientation('landscape'));
    expect(props(s, 0).pageSize.orientation).toBe('portrait');
    expect(props(s, 1).pageSize).toMatchObject({ orientation: 'landscape', width: 15840, height: 12240 });
    s = run(s, cmd.setSectionProps({ titlePage: true }));
    expect(props(s, 1).titlePage).toBe(true);
  });

  it('insertPageBreak inserts a break and moves following text to a new paragraph', () => {
    const { cmd } = setup();
    const s = run(stateWith(doc(p('beforeafter')), 'after', { cursor: true }), cmd.insertPageBreak);
    const paras = paragraphs(s.doc);
    expect(paras).toHaveLength(2);
    expect(paras[0]?.lastChild?.type.name).toBe('pageBreak');
    expect(paras[1]?.textContent).toBe('after');
  });
});
