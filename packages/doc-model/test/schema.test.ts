// @vitest-environment jsdom
import { DOMSerializer, DOMParser as PMDOMParser } from '@tiptap/pm/model';
import { TableMap, tableNodeTypes } from '@tiptap/pm/tables';
import { describe, expect, it } from 'vitest';
import { marksFromRunProps } from '../src/marks.ts';
import { createDocSchema, docSchema, paragraphDirectCss } from '../src/schema.ts';
import { runPropsFromMarks } from '../src/styles/resolve.ts';
import type { RunProps } from '../src/types.ts';
import { cell, doc, p, row, schema, section, table, text } from './helpers.ts';

const NODES = [
  'doc',
  'section',
  'header',
  'footer',
  'paragraph',
  'table',
  'tableRow',
  'tableCell',
  'toc',
  'footnote',
  'opaqueBlock',
  'text',
  'tab',
  'softBreak',
  'pageBreak',
  'columnBreak',
  'image',
  'shape',
  'field',
  'footnoteRef',
  'equation',
  'bookmarkStart',
  'bookmarkEnd',
  'opaqueInline',
];
const MARKS = [
  'runStyle',
  'link',
  'bold',
  'italic',
  'underline',
  'strike',
  'dstrike',
  'caps',
  'smallCaps',
  'hidden',
  'vertAlign',
  'color',
  'highlight',
  'runShading',
  'fontFamily',
  'fontSize',
  'charSpacing',
  'runProps',
  'comment',
  'insertion',
  'deletion',
  'formatChange',
  'spellError',
  'sdt',
];

describe('createDocSchema', () => {
  it('defines every node and mark of PLAN §3.3', () => {
    expect(Object.keys(docSchema.nodes).sort()).toEqual([...NODES].sort());
    expect(Object.keys(docSchema.marks).sort()).toEqual([...MARKS].sort());
  });

  it('paragraph attributes mirror WordprocessingML pPr', () => {
    const attrs = Object.keys(docSchema.nodes.paragraph?.spec.attrs ?? {});
    for (const k of [
      'styleId',
      'alignment',
      'indent',
      'spacing',
      'numbering',
      'pageBreakBefore',
      'keepNext',
      'tabs',
      'borders',
      'shading',
      'bidi',
    ]) {
      expect(attrs).toContain(k);
    }
  });

  it('table nodes have prosemirror-tables roles', () => {
    const types = tableNodeTypes(docSchema);
    expect(types.table?.name).toBe('table');
    expect(types.row?.name).toBe('tableRow');
    expect(types.cell?.name).toBe('tableCell');
  });

  it('builds a valid document with sections, headers, tables, notes and inline nodes', () => {
    const header = schema.node('header', { type: 'default' }, [p('Header')]);
    const s = schema.node('section', { props: null }, [
      header,
      p({ styleId: 'Heading1' }, 'Title'),
      p(
        'A',
        schema.node('tab'),
        text('bold', { bold: true }),
        schema.node('softBreak'),
        schema.node('field', { type: 'PAGE', instr: 'PAGE', result: '1' }),
        schema.node('footnoteRef', { id: '1' }),
        schema.node('image', { mediaId: 'word/media/image1.png', width: 914400, height: 457200 }),
        schema.node('equation', { omml: '<m:oMath/>' }),
        schema.node('bookmarkStart', { id: '0', name: '_Toc1' }),
        schema.node('bookmarkEnd', { id: '0' }),
      ),
      table({ styleId: 'TableGrid' }, row(cell(p('1')), cell(p('2')))),
      schema.node('toc', null, [p({ styleId: 'TOC1' }, 'Entry')]),
    ]);
    const d = schema.node('doc', null, [s, schema.node('footnote', { id: '1' }, [p('Note')])]);
    expect(() => d.check()).not.toThrow();
  });

  it('rejects invalid structure', () => {
    expect(() => schema.node('doc', null, [p('x')]).check()).toThrow();
    expect(() => schema.node('section', null, []).check()).toThrow();
    expect(() => schema.node('section', null, [schema.node('header', null, [p('h')])]).check()).toThrow();
    expect(() => schema.node('tableRow', null, [p('x')]).check()).toThrow();
  });

  it('round-trips through JSON', () => {
    const d = doc(
      p(
        { alignment: 'center', indent: { left: 720 } },
        text('Hi', { italic: true, color: { val: 'FF0000' } }),
      ),
    );
    const back = docSchema.nodeFromJSON(JSON.parse(JSON.stringify(d.toJSON())));
    expect(back.eq(d)).toBe(true);
  });

  it('allows overlapping comments but exclusive revision marks', () => {
    const c1 = docSchema.marks.comment?.create({ id: '1' });
    const c2 = docSchema.marks.comment?.create({ id: '2' });
    expect(c2?.addToSet(c1 ? [c1] : []).length).toBe(2);
    const ins = docSchema.marks.insertion?.create({ id: 'a', author: 'x' });
    const del = docSchema.marks.deletion?.create({ id: 'b', author: 'x' });
    expect(del?.addToSet(ins ? [ins] : []).map((m) => m.type.name)).toEqual(['deletion']);
  });

  it('leaf text for tabs, breaks and fields', () => {
    const para = p('a', schema.node('tab'), 'b', schema.node('field', { type: 'PAGE', result: '7' }));
    expect(para.textBetween(0, para.content.size, '\n', (n) => n.type.spec.leafText?.(n) ?? '')).toBe(
      'a\tb7',
    );
  });

  it('createDocSchema returns an independent instance', () => {
    expect(createDocSchema()).not.toBe(docSchema);
  });

  it('TableMap understands merged cells', () => {
    const t = table(
      {},
      row(schema.node('tableCell', { colspan: 2 }, [p('wide')])),
      row(cell(p('a')), cell(p('b'))),
    );
    const map = TableMap.get(t);
    expect([map.width, map.height]).toEqual([2, 2]);
  });
});

describe('marks ↔ run properties', () => {
  const samples: RunProps[] = [
    { bold: true, italic: false },
    { underline: { style: 'double', color: { val: 'FF0000' } }, vertAlign: 'superscript' },
    { color: { val: '2F5496', themeColor: 'accent1', themeShade: 191 }, highlight: 'yellow' },
    { fonts: { ascii: 'Arial', hAnsi: 'Arial' }, size: 28, sizeCs: 28, spacing: -10 },
    { styleId: 'Strong', caps: true, smallCaps: false, strike: true, dstrike: false, hidden: true },
    { kern: 28, scale: 90, position: 6, emboss: true, lang: { val: 'de-DE' } },
    { shading: { val: 'clear', fill: { val: 'FFFF00' } } },
  ];
  it.each(samples)('round-trips %o', (props) => {
    expect(runPropsFromMarks(marksFromRunProps(props))).toEqual(props);
  });
});

describe('DOM serialisation (clipboard)', () => {
  it('renders headings, alignment and marks to HTML', () => {
    const d = doc(
      p({ styleId: 'Heading1' }, 'Title'),
      p(
        { alignment: 'both', indent: { left: 720 } },
        text('Bold', { bold: true }),
        ' ',
        text('sup', { vertAlign: 'superscript' }),
      ),
    );
    const frag = DOMSerializer.fromSchema(docSchema).serializeFragment(d.content);
    const div = document.createElement('div');
    div.appendChild(frag);
    expect(div.querySelector('h1')?.textContent).toBe('Title');
    const para = div.querySelector('p');
    expect(para?.style.textAlign).toBe('justify');
    expect(para?.style.marginLeft).toBe('36pt');
    expect(div.querySelector('strong')?.textContent).toBe('Bold');
    expect(div.querySelector('sup')?.textContent).toBe('sup');
  });

  it('parses pasted HTML into Word-style paragraphs and marks', () => {
    const div = document.createElement('div');
    div.innerHTML =
      '<section data-of-section><h2>Sub</h2><p style="text-align:center">Hi <b>there</b> <a href="https://x.org">link</a> <span style="color:#ff0000;font-size:14pt">red</span></p></section>';
    const d = PMDOMParser.fromSchema(docSchema).parse(div);
    const [h, para] = [d.firstChild?.child(0), d.firstChild?.child(1)];
    expect(h?.attrs.styleId).toBe('Heading2');
    expect(para?.attrs.alignment).toBe('center');
    const names = new Set<string>();
    para?.descendants((n) => {
      for (const m of n.marks) names.add(m.type.name);
    });
    expect([...names].sort()).toEqual(['bold', 'color', 'fontSize', 'link']);
    let red: RunProps | undefined;
    para?.descendants((n) => {
      if (n.text === 'red') red = runPropsFromMarks(n.marks);
    });
    expect(red).toMatchObject({ color: { val: 'FF0000' }, size: 28 });
  });

  it('paragraphDirectCss maps hanging indents to negative text-indent', () => {
    expect(paragraphDirectCss({ indent: { left: 720, hanging: 360 } })).toBe(
      'margin-left:36pt;text-indent:-18pt',
    );
  });

  it('section helper creates default props', () => {
    expect(section(p('x')).attrs.props.pageSize.width).toBe(12240);
  });
});
