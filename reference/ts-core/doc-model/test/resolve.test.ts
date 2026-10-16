import { describe, expect, it } from 'vitest';
import { Numbering } from '../src/numbering/numbering.ts';
import {
  type DocContext,
  type ResolvedParagraphProps,
  type ResolvedRunProps,
  resolveAt,
  resolveCellProps,
  resolveParagraphProps,
  resolveRunProps,
  resolveTableProps,
  type TableCellContext,
  tableCellContext,
  tableConditions,
} from '../src/styles/resolve.ts';
import { StyleSheet } from '../src/styles/stylesheet.ts';
import { createDefaultStyleSheet } from '../src/template.ts';
import type { ParagraphProps, RunProps } from '../src/types.ts';
import { cell, doc, p, posOf, row, table, text } from './helpers.ts';

/** A compact fixture exercising every inheritance level. */
function fixture(): DocContext {
  const styles = new StyleSheet(
    {
      pPr: { spacing: { after: 160, line: 259, lineRule: 'auto' } },
      rPr: { size: 22, fonts: { asciiTheme: 'minorHAnsi', hAnsiTheme: 'minorHAnsi' } },
    },
    [
      { id: 'Normal', type: 'paragraph', name: 'Normal', isDefault: true },
      { id: 'DefaultParagraphFont', type: 'character', name: 'Default Paragraph Font', isDefault: true },
      { id: 'TableNormal', type: 'table', name: 'Normal Table', isDefault: true },
      {
        id: 'Body',
        type: 'paragraph',
        name: 'Body',
        basedOn: 'Normal',
        pPr: {
          indent: { left: 360 },
          tabs: [
            { pos: 720, align: 'left' },
            { pos: 1440, align: 'center' },
          ],
        },
        rPr: { size: 24, color: { val: '111111' } },
      },
      {
        id: 'BodyBold',
        type: 'paragraph',
        name: 'Body Bold',
        basedOn: 'Body',
        pPr: { alignment: 'center', spacing: { before: 120 }, keepNext: true },
        rPr: { bold: true },
      },
      {
        id: 'BodyBoldOff',
        type: 'paragraph',
        name: 'Body Bold Off',
        basedOn: 'BodyBold',
        rPr: { bold: false },
      },
      {
        id: 'FirstLine',
        type: 'paragraph',
        name: 'First Line',
        basedOn: 'Normal',
        pPr: { indent: { firstLine: 300 } },
      },
      {
        id: 'Boxed',
        type: 'paragraph',
        name: 'Boxed',
        basedOn: 'Normal',
        pPr: { borders: { top: { style: 'single', size: 4 }, bottom: { style: 'single', size: 4 } } },
      },
      {
        id: 'ItalicPara',
        type: 'paragraph',
        name: 'Italic Para',
        basedOn: 'Normal',
        rPr: { italic: true, caps: true },
      },
      {
        id: 'ListNum',
        type: 'paragraph',
        name: 'List Num',
        basedOn: 'Normal',
        pPr: { numbering: { numId: 1, level: 0 }, indent: { left: 100 } },
      },
      {
        id: 'Emph',
        type: 'character',
        name: 'Emph',
        basedOn: 'DefaultParagraphFont',
        rPr: { italic: true, color: { val: '00FF00' } },
      },
      {
        id: 'StrongChar',
        type: 'character',
        name: 'Strong Char',
        basedOn: 'DefaultParagraphFont',
        rPr: { bold: true },
      },
      { id: 'ChildChar', type: 'character', name: 'Child Char', basedOn: 'StrongChar', rPr: { size: 30 } },
      { id: 'CycleA', type: 'paragraph', name: 'Cycle A', basedOn: 'CycleB', pPr: { alignment: 'right' } },
      { id: 'CycleB', type: 'paragraph', name: 'Cycle B', basedOn: 'CycleA', pPr: { indent: { left: 50 } } },
      {
        id: 'TS',
        type: 'table',
        name: 'Test Table',
        basedOn: 'TableNormal',
        pPr: { spacing: { after: 0 }, alignment: 'right' },
        rPr: { size: 18, color: { val: 'AAAAAA' }, italic: true },
        tcPr: { vAlign: 'center' },
        conditions: {
          firstRow: {
            rPr: { bold: true, color: { val: 'FFFFFF' } },
            tcPr: { shading: { val: 'clear', fill: { val: '4472C4' } } },
          },
          lastCol: { rPr: { size: 16 } },
          band1Horz: { tcPr: { shading: { val: 'clear', fill: { val: 'D9E2F3' } } } },
          nwCell: { rPr: { underline: { style: 'double' } } },
        },
      },
    ],
  );
  const numbering = new Numbering();
  numbering.addAbstract({
    id: 0,
    levels: [
      {
        level: 0,
        start: 1,
        format: 'decimal',
        text: '%1.',
        pPr: { indent: { left: 720, hanging: 360 } },
        rPr: { bold: true, color: { val: 'FF0000' }, size: 28 },
      },
      {
        level: 1,
        start: 1,
        format: 'lowerLetter',
        text: '%2)',
        pPr: { indent: { left: 1440, hanging: 360 } },
      },
    ],
  });
  numbering.addInstance(0, [], 1);
  return { styles, numbering };
}

const ctx = fixture();
const whole: TableCellContext = { tableStyleId: 'TS', conditions: ['wholeTable'] };
const headerCell: TableCellContext = {
  tableStyleId: 'TS',
  conditions: ['wholeTable', 'firstCol', 'firstRow', 'nwCell'],
};
const lastColCell: TableCellContext = {
  tableStyleId: 'TS',
  conditions: ['wholeTable', 'band1Horz', 'lastCol'],
};

interface ParaCase {
  name: string;
  para: ParagraphProps;
  table?: TableCellContext;
  expect: (r: ResolvedParagraphProps) => void;
}

const paraCases: ParaCase[] = [
  {
    name: 'Normal takes docDefaults spacing',
    para: {},
    expect: (r) => expect(r.spacing).toMatchObject({ after: 160, line: 259, lineRule: 'auto', before: 0 }),
  },
  {
    name: 'Normal defaults alignment left, indent 0',
    para: {},
    expect: (r) => expect([r.alignment, r.indent.left, r.indent.firstLine]).toEqual(['left', 0, 0]),
  },
  { name: 'style indent applies', para: { styleId: 'Body' }, expect: (r) => expect(r.indent.left).toBe(360) },
  {
    name: 'basedOn chain inherits parent indent',
    para: { styleId: 'BodyBold' },
    expect: (r) => expect(r.indent.left).toBe(360),
  },
  {
    name: 'child style adds alignment',
    para: { styleId: 'BodyBold' },
    expect: (r) => expect(r.alignment).toBe('center'),
  },
  {
    name: 'spacing merges per attribute across levels',
    para: { styleId: 'BodyBold' },
    expect: (r) => expect([r.spacing.before, r.spacing.after]).toEqual([120, 160]),
  },
  {
    name: 'direct alignment overrides style',
    para: { styleId: 'BodyBold', alignment: 'right' },
    expect: (r) => expect(r.alignment).toBe('right'),
  },
  {
    name: 'direct firstLine keeps inherited left',
    para: { styleId: 'Body', indent: { firstLine: 200 } },
    expect: (r) => expect(r.indent).toMatchObject({ left: 360, firstLine: 200 }),
  },
  {
    name: 'direct hanging clears style firstLine',
    para: { styleId: 'FirstLine', indent: { hanging: 100 } },
    expect: (r) => expect([r.indent.firstLine, r.indent.hanging]).toEqual([0, 100]),
  },
  {
    name: 'direct spacing after 0 overrides docDefaults',
    para: { spacing: { after: 0 } },
    expect: (r) => expect([r.spacing.after, r.spacing.line]).toEqual([0, 259]),
  },
  {
    name: 'unknown style falls back to Normal',
    para: { styleId: 'Nope' },
    expect: (r) => expect(r.styleId).toBe('Normal'),
  },
  {
    name: 'numbering level indent overrides style indent',
    para: { styleId: 'Body', numbering: { numId: 1, level: 0 } },
    expect: (r) => expect(r.indent).toMatchObject({ left: 720, hanging: 360 }),
  },
  {
    name: 'direct indent overrides numbering indent',
    para: { styleId: 'Body', numbering: { numId: 1, level: 0 }, indent: { left: 1000 } },
    expect: (r) => expect(r.indent).toMatchObject({ left: 1000, hanging: 360 }),
  },
  {
    name: 'style-linked numbering is effective',
    para: { styleId: 'ListNum' },
    expect: (r) => expect(r.numbering).toEqual({ numId: 1, level: 0 }),
  },
  {
    name: 'style-linked numbering level indent beats style indent',
    para: { styleId: 'ListNum' },
    expect: (r) => expect(r.indent.left).toBe(720),
  },
  {
    name: 'numId 0 cancels style numbering',
    para: { styleId: 'ListNum', numbering: { numId: 0, level: 0 } },
    expect: (r) => expect([r.numbering, r.indent.left]).toEqual([undefined, 100]),
  },
  {
    name: 'ilvl-only override keeps the style list',
    para: { styleId: 'ListNum', numbering: { level: 1 } as never },
    expect: (r) => expect([r.numbering, r.indent.left]).toEqual([{ numId: 1, level: 1 }, 1440]),
  },
  {
    name: 'table style paragraph props apply in tables',
    para: {},
    table: whole,
    expect: (r) => expect([r.alignment, r.spacing.after]).toEqual(['right', 0]),
  },
  {
    name: 'paragraph style beats table style',
    para: { styleId: 'BodyBold' },
    table: whole,
    expect: (r) => expect(r.alignment).toBe('center'),
  },
  {
    name: 'docDefaults lose to table style',
    para: { styleId: 'Body' },
    table: whole,
    expect: (r) => expect(r.spacing.after).toBe(0),
  },
  {
    name: 'direct beats table style',
    para: { alignment: 'left' },
    table: whole,
    expect: (r) => expect(r.alignment).toBe('left'),
  },
  {
    name: 'tab stops accumulate and clear',
    para: {
      styleId: 'Body',
      tabs: [
        { pos: 1440, align: 'clear' },
        { pos: 2000, align: 'right' },
      ],
    },
    expect: (r) =>
      expect(r.tabs.map((t) => [t.pos, t.align])).toEqual([
        [720, 'left'],
        [2000, 'right'],
      ]),
  },
  {
    name: 'borders merge per edge',
    para: { styleId: 'Boxed', borders: { bottom: { style: 'double', size: 6 } } },
    expect: (r) => expect([r.borders?.top?.style, r.borders?.bottom?.style]).toEqual(['single', 'double']),
  },
  {
    name: 'keepNext inherited from style chain',
    para: { styleId: 'BodyBold' },
    expect: (r) => expect(r.keepNext).toBe(true),
  },
  {
    name: 'outline level defaults to body text (9)',
    para: {},
    expect: (r) => expect(r.outlineLevel).toBe(9),
  },
  {
    name: 'basedOn cycles terminate',
    para: { styleId: 'CycleA' },
    expect: (r) => expect(r.alignment).toBe('right'),
  },
];

interface RunCase {
  name: string;
  para: ParagraphProps;
  run?: RunProps;
  table?: TableCellContext;
  target?: 'text' | 'listLabel';
  expect: (r: ResolvedRunProps) => void;
}

const runCases: RunCase[] = [
  { name: 'docDefaults size', para: {}, expect: (r) => expect(r.size).toBe(22) },
  {
    name: 'toggles default false',
    para: {},
    expect: (r) => expect([r.bold, r.italic, r.caps]).toEqual([false, false, false]),
  },
  {
    name: 'paragraph style size and colour',
    para: { styleId: 'Body' },
    expect: (r) => expect([r.size, r.colorHex]).toEqual([24, '111111']),
  },
  { name: 'paragraph style bold', para: { styleId: 'BodyBold' }, expect: (r) => expect(r.bold).toBe(true) },
  {
    name: 'derived style turns bold off',
    para: { styleId: 'BodyBoldOff' },
    expect: (r) => expect(r.bold).toBe(false),
  },
  {
    name: 'character style bold over bold paragraph toggles off',
    para: { styleId: 'BodyBold' },
    run: { styleId: 'StrongChar' },
    expect: (r) => expect(r.bold).toBe(false),
  },
  {
    name: 'character style bold over plain paragraph',
    para: { styleId: 'Body' },
    run: { styleId: 'StrongChar' },
    expect: (r) => expect(r.bold).toBe(true),
  },
  {
    name: 'emphasis inside italic paragraph is upright',
    para: { styleId: 'ItalicPara' },
    run: { styleId: 'Emph' },
    expect: (r) => expect(r.italic).toBe(false),
  },
  {
    name: 'direct bold=false beats style bold',
    para: { styleId: 'BodyBold' },
    run: { bold: false },
    expect: (r) => expect(r.bold).toBe(false),
  },
  {
    name: 'direct bold=true is absolute after a toggle',
    para: { styleId: 'BodyBold' },
    run: { styleId: 'StrongChar', bold: true },
    expect: (r) => expect(r.bold).toBe(true),
  },
  {
    name: 'character style colour beats paragraph style',
    para: { styleId: 'Body' },
    run: { styleId: 'Emph' },
    expect: (r) => expect([r.italic, r.colorHex]).toEqual([true, '00FF00']),
  },
  {
    name: 'direct colour beats character style',
    para: { styleId: 'Body' },
    run: { styleId: 'Emph', color: { val: '123456' } },
    expect: (r) => expect(r.colorHex).toBe('123456'),
  },
  {
    name: 'character style chain',
    para: {},
    run: { styleId: 'ChildChar' },
    expect: (r) => expect([r.bold, r.size]).toEqual([true, 30]),
  },
  {
    name: 'paragraph style used as rStyle is ignored',
    para: {},
    run: { styleId: 'Body' },
    expect: (r) => expect(r.size).toBe(22),
  },
  {
    name: 'direct size beats everything',
    para: { styleId: 'Body' },
    run: { styleId: 'ChildChar', size: 40 },
    table: whole,
    expect: (r) => expect(r.size).toBe(40),
  },
  {
    name: 'table style run props',
    para: {},
    table: whole,
    expect: (r) => expect([r.size, r.colorHex, r.italic]).toEqual([18, 'AAAAAA', true]),
  },
  {
    name: 'paragraph style beats table style',
    para: { styleId: 'Body' },
    table: whole,
    expect: (r) => expect([r.size, r.colorHex, r.italic]).toEqual([24, '111111', true]),
  },
  {
    name: 'header row condition',
    para: {},
    table: headerCell,
    expect: (r) => expect([r.bold, r.colorHex]).toEqual([true, 'FFFFFF']),
  },
  {
    name: 'corner cell condition applies last',
    para: {},
    table: headerCell,
    expect: (r) => expect(r.underline?.style).toBe('double'),
  },
  {
    name: 'last column condition beats whole table',
    para: {},
    table: lastColCell,
    expect: (r) => expect(r.size).toBe(16),
  },
  {
    name: 'header row bold with bold paragraph style stays bold',
    para: { styleId: 'BodyBold' },
    table: headerCell,
    expect: (r) => expect(r.bold).toBe(true),
  },
  {
    name: 'list label takes numbering rPr',
    para: { styleId: 'Body', numbering: { numId: 1, level: 0 } },
    target: 'listLabel',
    expect: (r) => expect([r.bold, r.colorHex, r.size]).toEqual([true, 'FF0000', 28]),
  },
  {
    name: 'text runs ignore numbering rPr',
    para: { styleId: 'Body', numbering: { numId: 1, level: 0 } },
    expect: (r) => expect([r.bold, r.size]).toEqual([false, 24]),
  },
  {
    name: 'level without rPr leaves label like the paragraph',
    para: { styleId: 'Body', numbering: { numId: 1, level: 1 } },
    target: 'listLabel',
    expect: (r) => expect([r.bold, r.size]).toEqual([false, 24]),
  },
  {
    name: 'theme minor font resolves to Calibri',
    para: {},
    expect: (r) => expect(r.resolvedFonts.ascii).toBe('Calibri'),
  },
  {
    name: 'explicit font replaces inherited theme font',
    para: {},
    run: { fonts: { ascii: 'Arial', hAnsi: 'Arial' } },
    expect: (r) => expect([r.resolvedFonts.ascii, r.resolvedFonts.hAnsi]).toEqual(['Arial', 'Arial']),
  },
  {
    name: 'caps from paragraph, direct caps=false',
    para: { styleId: 'ItalicPara' },
    run: { caps: false },
    expect: (r) => expect(r.caps).toBe(false),
  },
  { name: 'auto colour without settings', para: {}, expect: (r) => expect(r.colorHex).toBe('auto') },
];

describe('resolveParagraphProps inheritance matrix', () => {
  it.each(paraCases)('$name', (c) => {
    c.expect(resolveParagraphProps(ctx, { ...c.para }, c.table));
  });
});

describe('resolveRunProps inheritance matrix', () => {
  it.each(runCases)('$name', (c) => {
    c.expect(
      resolveRunProps(ctx, {
        paragraph: { ...c.para },
        ...(c.run ? { props: c.run } : {}),
        ...(c.table ? { table: c.table } : {}),
        ...(c.target ? { target: c.target } : {}),
      }),
    );
  });
});

describe('Word precedence: table style < paragraph style < numbering < direct', () => {
  /**
   * A list paragraph in a styled paragraph inside a styled table. Each layer sets a property that a
   * later layer may override; removing the later layer must reveal the earlier one.
   */
  function case(opts: { numberingRPr: RunProps | undefined; paraMark: RunProps | undefined }) {
    const styles = new StyleSheet({ pPr: {}, rPr: { size: 20 } }, [
      { id: 'Normal', type: 'paragraph', name: 'Normal', isDefault: true },
      { id: 'TableNormal', type: 'table', name: 'Normal Table', isDefault: true },
      {
        id: 'Grid',
        type: 'table',
        name: 'Grid',
        basedOn: 'TableNormal',
        rPr: { size: 18, color: { val: 'AAAAAA' }, italic: true, caps: true },
      },
      {
        id: 'Para',
        type: 'paragraph',
        name: 'Para',
        basedOn: 'Normal',
        rPr: { size: 24, color: { val: '111111' } },
      },
    ]);
    const numbering = new Numbering();
    numbering.addAbstract({
      id: 0,
      levels: [
        {
          level: 0,
          start: 1,
          format: 'decimal',
          text: '%1.',
          ...(opts.numberingRPr ? { rPr: opts.numberingRPr } : {}),
        },
      ],
    });
    numbering.addInstance(0, [], 1);
    const paragraph = p({ styleId: 'Para', numbering: { numId: 1, level: 0 } }, 'Item');
    const para = opts.paraMark
      ? paragraph.type.create({ ...paragraph.attrs, markProps: opts.paraMark }, paragraph.content)
      : paragraph;
    const tbl = table({ styleId: 'Grid' }, row(cell(para)));
    const d = doc(tbl);
    const c: DocContext = { styles, numbering };
    let tableCtx: TableCellContext | undefined;
    d.descendants((n) => {
      if (n.type.name === 'table') tableCtx = tableCellContext(n, 0, 0);
      return true;
    });
    return resolveRunProps(c, { paragraph: para, table: tableCtx as TableCellContext, target: 'listLabel' });
  }

  it('resolves each property from the highest layer that sets it', () => {
    const r = case({ numberingRPr: { size: 28, bold: true }, paraMark: { color: { val: '0000FF' } } });
    expect(r.caps).toBe(true); // table style only
    expect(r.italic).toBe(true); // table style only
    expect(r.size).toBe(28); // numbering beats paragraph style (24) and table style (18)
    expect(r.bold).toBe(true); // numbering
    expect(r.colorHex).toBe('0000FF'); // direct beats paragraph style (111111) and table style (AAAAAA)
  });

  it('without direct formatting the paragraph style beats the table style', () => {
    const r = case({ numberingRPr: { size: 28 }, paraMark: undefined });
    expect(r.colorHex).toBe('111111');
  });

  it('without numbering rPr the paragraph style beats the table style', () => {
    const r = case({ numberingRPr: undefined, paraMark: undefined });
    expect(r.size).toBe(24);
  });

  it('direct formatting beats numbering for the same property', () => {
    const r = case({ numberingRPr: { size: 28 }, paraMark: { size: 32 } });
    expect(r.size).toBe(32);
  });
});

describe('table conditions', () => {
  it('orders regions like Word and bands rows after the header', () => {
    expect(tableConditions({ row: 0, col: 0, rowCount: 4, colCount: 3 })).toEqual([
      'wholeTable',
      'firstCol',
      'firstRow',
      'nwCell',
    ]);
    expect(tableConditions({ row: 1, col: 1, rowCount: 4, colCount: 3 })).toEqual([
      'wholeTable',
      'band1Horz',
    ]);
    expect(tableConditions({ row: 2, col: 1, rowCount: 4, colCount: 3 })).toEqual([
      'wholeTable',
      'band2Horz',
    ]);
  });

  it('respects tblLook flags and band sizes', () => {
    const look = {
      firstRow: false,
      firstColumn: false,
      lastRow: true,
      lastColumn: true,
      noHBand: false,
      noVBand: false,
    };
    expect(tableConditions({ row: 3, col: 2, rowCount: 4, colCount: 3, look })).toEqual([
      'wholeTable',
      'lastCol',
      'lastRow',
      'seCell',
    ]);
    expect(
      tableConditions({ row: 2, col: 0, rowCount: 6, colCount: 3, look, rowBandSize: 2, colBandSize: 1 }),
    ).toEqual(['wholeTable', 'band1Vert', 'band2Horz']);
  });

  it('computes cell properties from the table style and the cell', () => {
    const tbl = table({ styleId: 'TS' }, row(cell(p('a')), cell(p('b'))), row(cell(p('c')), cell(p('d'))));
    const header = resolveCellProps(ctx, tbl, 0, 1);
    expect(header.shading?.fill?.val).toBe('4472C4');
    expect(header.vAlign).toBe('center');
    const body = resolveCellProps(ctx, tbl, 1, 1);
    expect(body.shading?.fill?.val).toBe('D9E2F3');
  });

  it('resolves table props through the style chain', () => {
    const styles = createDefaultStyleSheet();
    const tbl = table({ styleId: 'TableGrid', alignment: 'center' }, row(cell(p('x'))));
    const tp = resolveTableProps({ styles }, tbl);
    expect(tp.borders?.insideH?.style).toBe('single');
    expect(tp.cellMargins).toEqual({ top: 0, left: 108, bottom: 0, right: 108 });
    expect(tp.alignment).toBe('center');
  });
});

describe('resolveAt', () => {
  it('reports the effective formatting at a position inside a table', () => {
    const styles = createDefaultStyleSheet();
    const d = doc(
      p({ styleId: 'Heading1' }, 'Title'),
      table({ styleId: 'TableGrid' }, row(cell(p('Cell text')))),
    );
    const atHeading = resolveAt({ styles }, d, posOf(d, 'Title') + 1);
    expect(atHeading?.run.size).toBe(32);
    expect(atHeading?.run.colorHex).toBe('2F5496');
    expect(atHeading?.run.resolvedFonts.ascii).toBe('Calibri Light');
    const inCell = resolveAt({ styles }, d, posOf(d, 'Cell') + 1);
    expect(inCell?.paragraph.spacing.after).toBe(0); // Table Grid pPr beats docDefaults' 8 pt
    expect(inCell?.paragraph.spacing.line).toBe(240);
  });

  it('includes direct run formatting from marks', () => {
    const styles = createDefaultStyleSheet();
    const d = doc(p(text('Big', { size: 48, bold: true })));
    const r = resolveAt({ styles }, d, posOf(d, 'Big') + 1);
    expect([r?.run.size, r?.run.bold]).toEqual([48, true]);
  });
});
