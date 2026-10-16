/**
 * The Write ProseMirror schema (PLAN §3.3). Structure:
 *
 * ```
 * doc        > section+ footnote*
 * section    > (header | footer)* block+          attrs: props (SectionProps)
 * block      = paragraph | table | toc | opaqueBlock
 * paragraph  > inline*                            attrs: ParagraphProps + markProps
 * table      > tableRow+  > tableCell+ > block+    prosemirror-tables compatible (colspan/rowspan)
 * inline     = text | tab | softBreak | pageBreak | columnBreak | image | shape | field
 *            | footnoteRef | equation | bookmarkStart | bookmarkEnd | opaqueInline
 * ```
 *
 * Attribute values are plain JSON (see `types.ts`), so documents serialise losslessly with
 * `node.toJSON()`. Word's `w:gridSpan` is the cell's `colspan`; a `w:vMerge="restart"` cell followed by
 * `continue` cells is a single cell with `rowspan` > 1 (the DOCX reader/writer convert between the two),
 * which keeps the table rectangular for prosemirror-tables' `TableMap`.
 */
import {
  type DOMOutputSpec,
  type MarkSpec,
  type NodeSpec,
  type Node as PMNode,
  Schema,
} from '@tiptap/pm/model';
import type { Alignment, ParagraphProps, RunFonts, SectionProps } from './types.ts';

/** Paragraph attributes that mirror `ParagraphProps` one-to-one. */
export const PARAGRAPH_PROP_KEYS = [
  'styleId',
  'alignment',
  'indent',
  'spacing',
  'numbering',
  'keepNext',
  'keepLines',
  'pageBreakBefore',
  'widowControl',
  'contextualSpacing',
  'suppressAutoHyphens',
  'suppressLineNumbers',
  'outlineLevel',
  'tabs',
  'borders',
  'shading',
  'bidi',
  'textAlignment',
  'dropCap',
] as const satisfies readonly (keyof ParagraphProps)[];

export const TABLE_PROP_KEYS = [
  'styleId',
  'width',
  'alignment',
  'indent',
  'borders',
  'shading',
  'cellMargins',
  'cellSpacing',
  'layout',
  'look',
  'rowBandSize',
  'colBandSize',
  'bidiVisual',
  'caption',
  'description',
] as const;

export const ROW_PROP_KEYS = ['height', 'header', 'cantSplit', 'alignment'] as const;

export const CELL_PROP_KEYS = [
  'width',
  'borders',
  'shading',
  'margins',
  'vAlign',
  'noWrap',
  'textDirection',
  'fitText',
  'hideMark',
] as const;

/**
 * File-fidelity attributes of a paragraph (no editing semantics of their own; readers fill them and
 * writers re-emit them):
 * - `sdt`: enclosing block content controls (`w:sdt`), outermost first, as `SdtInfo[]`;
 * - `revision`: tracked changes of the paragraph itself — `pPrChange` (formatting before the change)
 *   and `mark` (inserted/deleted paragraph mark), see `ParagraphRevision`;
 * - `xml`: unmodelled `w:pPr` children as raw XML strings, re-emitted verbatim.
 */
export const PARAGRAPH_FILE_KEYS = ['sdt', 'revision', 'xml'] as const;

const nullAttrs = (keys: readonly string[]) =>
  Object.fromEntries(keys.map((k) => [k, { default: null }])) as Record<string, { default: null }>;

const ALIGN_CSS: Record<Alignment, string> = {
  left: 'left',
  center: 'center',
  right: 'right',
  both: 'justify',
  distribute: 'justify',
  mediumKashida: 'justify',
  highKashida: 'justify',
  lowKashida: 'justify',
  thaiDistribute: 'justify',
};

/** Inline CSS for a paragraph's *direct* attributes (the editor layers resolved style CSS on top). */
export function paragraphDirectCss(attrs: Record<string, unknown>): string {
  const css: string[] = [];
  const align = attrs.alignment as Alignment | null;
  if (align) css.push(`text-align:${ALIGN_CSS[align] ?? 'left'}`);
  const ind = attrs.indent as ParagraphProps['indent'] | null;
  if (ind?.left !== undefined) css.push(`margin-left:${ind.left / 20}pt`);
  if (ind?.right !== undefined) css.push(`margin-right:${ind.right / 20}pt`);
  if (ind?.firstLine !== undefined) css.push(`text-indent:${ind.firstLine / 20}pt`);
  if (ind?.hanging !== undefined) css.push(`text-indent:-${ind.hanging / 20}pt`);
  const sp = attrs.spacing as ParagraphProps['spacing'] | null;
  if (sp?.before !== undefined) css.push(`margin-top:${sp.before / 20}pt`);
  if (sp?.after !== undefined) css.push(`margin-bottom:${sp.after / 20}pt`);
  if (attrs.bidi) css.push('direction:rtl');
  return css.join(';');
}

const STYLE_TO_TAG: Record<string, string> = {
  Heading1: 'h1',
  Heading2: 'h2',
  Heading3: 'h3',
  Heading4: 'h4',
  Heading5: 'h5',
  Heading6: 'h6',
};

const HEADING_TAGS = ['h1', 'h2', 'h3', 'h4', 'h5', 'h6'] as const;

function parseCssLength(v: string): number | undefined {
  const m = /^(-?[\d.]+)(pt|px|in|cm|mm)?$/.exec(v.trim());
  if (!m) return undefined;
  const n = Number(m[1]);
  const unit = m[2] ?? 'px';
  const pt =
    unit === 'pt'
      ? n
      : unit === 'px'
        ? n * 0.75
        : unit === 'in'
          ? n * 72
          : unit === 'cm'
            ? (n / 2.54) * 72
            : (n / 25.4) * 72;
  return pt;
}

function cssColorToHex(v: string): string | undefined {
  const s = v.trim();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(s);
  if (hex?.[1]) {
    const h = hex[1].length === 3 ? [...hex[1]].map((c) => c + c).join('') : hex[1];
    return h.toUpperCase();
  }
  const rgb = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(s);
  if (rgb) {
    return [rgb[1], rgb[2], rgb[3]]
      .map((c) => Number(c).toString(16).padStart(2, '0'))
      .join('')
      .toUpperCase();
  }
  return undefined;
}

function pmAlignFromCss(v: string | null | undefined): Alignment | null {
  switch (v) {
    case 'center':
      return 'center';
    case 'right':
    case 'end':
      return 'right';
    case 'justify':
      return 'both';
    case 'left':
    case 'start':
      return 'left';
    default:
      return null;
  }
}

/** Builds a fresh instance of the Write schema. Use the shared `docSchema` unless you need isolation. */
export function createDocSchema(): Schema {
  const nodes: Record<string, NodeSpec> = {
    doc: { content: 'section+ footnote*' },

    section: {
      content: 'headerFooter* block+',
      attrs: { props: { default: null } },
      isolating: true,
      parseDOM: [
        {
          tag: 'section[data-kiln-section]',
          getAttrs: (dom) => {
            const raw = (dom as HTMLElement).getAttribute('data-props');
            return { props: raw ? (JSON.parse(raw) as SectionProps) : null };
          },
        },
      ],
      toDOM: (node) => [
        'section',
        { 'data-kiln-section': '', 'data-props': node.attrs.props ? JSON.stringify(node.attrs.props) : null },
        0,
      ],
    },

    header: {
      group: 'headerFooter',
      content: 'block+',
      attrs: { type: { default: 'default' }, partId: { default: null } },
      isolating: true,
      parseDOM: [
        {
          tag: 'header[data-kiln-hf]',
          getAttrs: (dom) => ({ type: (dom as HTMLElement).dataset.type ?? 'default' }),
        },
      ],
      toDOM: (node) => ['header', { 'data-kiln-hf': '', 'data-type': node.attrs.type }, 0],
    },

    footer: {
      group: 'headerFooter',
      content: 'block+',
      attrs: { type: { default: 'default' }, partId: { default: null } },
      isolating: true,
      parseDOM: [
        {
          tag: 'footer[data-kiln-hf]',
          getAttrs: (dom) => ({ type: (dom as HTMLElement).dataset.type ?? 'default' }),
        },
      ],
      toDOM: (node) => ['footer', { 'data-kiln-hf': '', 'data-type': node.attrs.type }, 0],
    },

    paragraph: {
      group: 'block',
      content: 'inline*',
      attrs: {
        ...nullAttrs(PARAGRAPH_PROP_KEYS),
        markProps: { default: null },
        paraId: { default: null },
        ...nullAttrs(PARAGRAPH_FILE_KEYS),
      },
      parseDOM: [
        {
          tag: 'p',
          getAttrs: (dom) => {
            const el = dom as HTMLElement;
            return {
              styleId: el.getAttribute('data-style') || null,
              alignment: pmAlignFromCss(el.style?.textAlign),
            };
          },
        },
        ...HEADING_TAGS.map((tag, i) => ({
          tag,
          getAttrs: (dom: HTMLElement | string) => ({
            styleId: `Heading${i + 1}`,
            alignment: typeof dom === 'string' ? null : pmAlignFromCss(dom.style?.textAlign),
          }),
        })),
        { tag: 'li', getAttrs: () => ({ styleId: 'ListParagraph' }) },
        { tag: 'blockquote', getAttrs: () => ({ styleId: 'Quote' }) },
      ],
      toDOM: (node) => {
        const styleId = node.attrs.styleId as string | null;
        const tag = (styleId && STYLE_TO_TAG[styleId]) || 'p';
        const css = paragraphDirectCss(node.attrs);
        return [tag, { 'data-style': styleId, style: css || null }, 0];
      },
    },

    table: {
      group: 'block',
      content: 'tableRow+',
      tableRole: 'table',
      isolating: true,
      attrs: {
        ...nullAttrs(TABLE_PROP_KEYS),
        grid: { default: null },
        sdt: { default: null },
        xml: { default: null },
      },
      parseDOM: [{ tag: 'table' }],
      toDOM: (node) => ['table', { 'data-style': node.attrs.styleId }, ['tbody', 0]],
    },

    tableRow: {
      content: 'tableCell+',
      tableRole: 'row',
      attrs: { ...nullAttrs(ROW_PROP_KEYS), revision: { default: null }, xml: { default: null } },
      parseDOM: [{ tag: 'tr' }],
      toDOM: () => ['tr', 0],
    },

    tableCell: {
      content: 'block+',
      tableRole: 'cell',
      isolating: true,
      attrs: {
        colspan: { default: 1 },
        rowspan: { default: 1 },
        /** Column widths in px for prosemirror-tables' column resizing (derived from the grid). */
        colwidth: { default: null },
        ...nullAttrs(CELL_PROP_KEYS),
        xml: { default: null },
      },
      parseDOM: [
        {
          tag: 'td',
          getAttrs: (dom) => ({
            colspan: Number((dom as HTMLElement).getAttribute('colspan') ?? 1) || 1,
            rowspan: Number((dom as HTMLElement).getAttribute('rowspan') ?? 1) || 1,
          }),
        },
        {
          tag: 'th',
          getAttrs: (dom) => ({
            colspan: Number((dom as HTMLElement).getAttribute('colspan') ?? 1) || 1,
            rowspan: Number((dom as HTMLElement).getAttribute('rowspan') ?? 1) || 1,
          }),
        },
      ],
      toDOM: (node) => {
        const attrs: Record<string, string> = {};
        if (node.attrs.colspan !== 1) attrs.colspan = String(node.attrs.colspan);
        if (node.attrs.rowspan !== 1) attrs.rowspan = String(node.attrs.rowspan);
        return ['td', attrs, 0];
      },
    },

    toc: {
      group: 'block',
      content: 'paragraph+',
      attrs: { instr: { default: 'TOC \\o "1-3" \\h \\z \\u' }, dirty: { default: false } },
      defining: true,
      parseDOM: [
        { tag: 'nav[data-kiln-toc]', getAttrs: (dom) => ({ instr: (dom as HTMLElement).dataset.instr }) },
      ],
      toDOM: (node) => ['nav', { 'data-kiln-toc': '', 'data-instr': node.attrs.instr }, 0],
    },

    footnote: {
      content: 'block+',
      attrs: { id: { default: '' }, kind: { default: 'footnote' } },
      isolating: true,
      parseDOM: [
        {
          tag: 'aside[data-kiln-note]',
          getAttrs: (dom) => ({
            id: (dom as HTMLElement).dataset.id ?? '',
            kind: (dom as HTMLElement).dataset.kind ?? 'footnote',
          }),
        },
      ],
      toDOM: (node) => [
        'aside',
        { 'data-kiln-note': '', 'data-id': node.attrs.id, 'data-kind': node.attrs.kind },
        0,
      ],
    },

    opaqueBlock: {
      group: 'block',
      atom: true,
      attrs: { xml: { default: '' }, label: { default: '' } },
      parseDOM: [
        {
          tag: 'div[data-kiln-opaque]',
          getAttrs: (dom) => ({
            xml: (dom as HTMLElement).dataset.xml ?? '',
            label: (dom as HTMLElement).textContent ?? '',
          }),
        },
      ],
      toDOM: (node) => [
        'div',
        { 'data-kiln-opaque': '', 'data-xml': node.attrs.xml, contenteditable: 'false' },
        node.attrs.label || ' ',
      ],
    },

    text: { group: 'inline' },

    tab: {
      group: 'inline',
      inline: true,
      atom: true,
      selectable: false,
      parseDOM: [{ tag: 'span[data-kiln-tab]' }],
      toDOM: (): DOMOutputSpec => ['span', { 'data-kiln-tab': '', class: 'of-tab' }, '\t'],
      leafText: () => '\t',
    },

    softBreak: {
      group: 'inline',
      inline: true,
      atom: true,
      selectable: false,
      attrs: { clear: { default: null } },
      parseDOM: [{ tag: 'br' }],
      toDOM: () => ['br'],
      leafText: () => '\n',
    },

    pageBreak: {
      group: 'inline',
      inline: true,
      atom: true,
      parseDOM: [{ tag: 'span[data-kiln-page-break]' }],
      toDOM: () => ['span', { 'data-kiln-page-break': '', class: 'of-page-break', contenteditable: 'false' }],
      leafText: () => '\f',
    },

    columnBreak: {
      group: 'inline',
      inline: true,
      atom: true,
      parseDOM: [{ tag: 'span[data-kiln-column-break]' }],
      toDOM: () => [
        'span',
        { 'data-kiln-column-break': '', class: 'of-column-break', contenteditable: 'false' },
      ],
      leafText: () => '\v',
    },

    image: {
      group: 'inline',
      inline: true,
      atom: true,
      draggable: true,
      attrs: {
        /** Displayable URL (blob:, data:, https:) once loaded. */
        src: { default: null },
        /** Key into the document's media store (the DOCX part name). */
        mediaId: { default: null },
        alt: { default: '' },
        title: { default: '' },
        decorative: { default: false },
        /** Extent in EMU. */
        width: { default: 0 },
        height: { default: 0 },
        placement: { default: 'inline' },
        wrap: { default: 'inline' },
        position: { default: null },
        /** Crop fractions `{l,t,r,b}` (0–1). */
        crop: { default: null },
        /** Rotation in degrees. */
        rotation: { default: 0 },
        flipH: { default: false },
        flipV: { default: false },
      },
      parseDOM: [
        {
          tag: 'img[src]',
          getAttrs: (dom) => {
            const el = dom as HTMLImageElement;
            const px = (v: string | null) => (v ? Math.round(Number(v) * 9525) : 0);
            return {
              src: el.getAttribute('src'),
              alt: el.getAttribute('alt') ?? '',
              title: el.getAttribute('title') ?? '',
              width: px(el.getAttribute('width')),
              height: px(el.getAttribute('height')),
            };
          },
        },
      ],
      toDOM: (node) => [
        'img',
        {
          src: node.attrs.src,
          alt: node.attrs.alt,
          title: node.attrs.title || null,
          width: node.attrs.width ? String(Math.round(node.attrs.width / 9525)) : null,
          height: node.attrs.height ? String(Math.round(node.attrs.height / 9525)) : null,
          'data-placement': node.attrs.placement,
        },
      ],
    },

    shape: {
      group: 'inline',
      inline: true,
      atom: true,
      draggable: true,
      attrs: {
        /** DrawingML preset geometry name (`rect`, `ellipse`, …). */
        preset: { default: 'rect' },
        width: { default: 0 },
        height: { default: 0 },
        placement: { default: 'anchor' },
        wrap: { default: 'square' },
        position: { default: null },
        /** Fill / line as `@kiln-ref/ooxml` Fill/Line JSON. */
        fill: { default: null },
        line: { default: null },
        rotation: { default: 0 },
        /** Original `wps:wsp` XML for passthrough of unmodelled parts (text boxes, effects). */
        xml: { default: null },
      },
      parseDOM: [
        {
          tag: 'span[data-kiln-shape]',
          getAttrs: (dom) => ({ preset: (dom as HTMLElement).dataset.preset ?? 'rect' }),
        },
      ],
      toDOM: (node) => [
        'span',
        { 'data-kiln-shape': '', 'data-preset': node.attrs.preset, contenteditable: 'false' },
      ],
    },

    field: {
      group: 'inline',
      inline: true,
      atom: true,
      attrs: {
        /** Field type keyword (`PAGE`, `TOC`, …) — derived from `instr`, kept for fast lookup. */
        type: { default: '' },
        /** Full instruction text, e.g. `PAGE \* MERGEFORMAT`. */
        instr: { default: '' },
        /** Cached result text as last computed (what Word shows until fields update). */
        result: { default: '' },
        dirty: { default: false },
        locked: { default: false },
      },
      parseDOM: [
        {
          tag: 'span[data-kiln-field]',
          getAttrs: (dom) => {
            const el = dom as HTMLElement;
            return {
              type: el.dataset.type ?? '',
              instr: el.dataset.instr ?? '',
              result: el.textContent ?? '',
            };
          },
        },
      ],
      toDOM: (node) => [
        'span',
        {
          'data-kiln-field': '',
          'data-type': node.attrs.type,
          'data-instr': node.attrs.instr,
          class: 'of-field',
        },
        node.attrs.result,
      ],
      leafText: (node: PMNode) => node.attrs.result as string,
    },

    footnoteRef: {
      group: 'inline',
      inline: true,
      atom: true,
      attrs: { id: { default: '' }, kind: { default: 'footnote' }, customMark: { default: null } },
      parseDOM: [
        {
          tag: 'sup[data-kiln-noteref]',
          getAttrs: (dom) => ({
            id: (dom as HTMLElement).dataset.id ?? '',
            kind: (dom as HTMLElement).dataset.kind ?? 'footnote',
          }),
        },
      ],
      toDOM: (node) => [
        'sup',
        { 'data-kiln-noteref': '', 'data-id': node.attrs.id, 'data-kind': node.attrs.kind },
      ],
    },

    equation: {
      group: 'inline',
      inline: true,
      atom: true,
      attrs: {
        /** Office Math Markup (`m:oMath`) XML. */
        omml: { default: '' },
        /** Display (own paragraph, `m:oMathPara`) vs inline. */
        display: { default: false },
        /** Linear-format/LaTeX source when authored in Kiln (editor-side cache). */
        source: { default: null },
      },
      parseDOM: [
        { tag: 'span[data-kiln-eq]', getAttrs: (dom) => ({ omml: (dom as HTMLElement).dataset.omml ?? '' }) },
      ],
      toDOM: (node) => ['span', { 'data-kiln-eq': '', 'data-omml': node.attrs.omml, contenteditable: 'false' }],
    },

    bookmarkStart: {
      group: 'inline',
      inline: true,
      atom: true,
      selectable: false,
      attrs: { id: { default: '' }, name: { default: '' } },
      parseDOM: [
        {
          tag: 'a[data-kiln-bm-start]',
          getAttrs: (dom) => ({
            id: (dom as HTMLElement).dataset.id ?? '',
            name: (dom as HTMLElement).getAttribute('name') ?? '',
          }),
        },
      ],
      toDOM: (node) => ['a', { 'data-kiln-bm-start': '', 'data-id': node.attrs.id, name: node.attrs.name }],
      leafText: () => '',
    },

    bookmarkEnd: {
      group: 'inline',
      inline: true,
      atom: true,
      selectable: false,
      attrs: { id: { default: '' } },
      parseDOM: [
        { tag: 'span[data-kiln-bm-end]', getAttrs: (dom) => ({ id: (dom as HTMLElement).dataset.id ?? '' }) },
      ],
      toDOM: (node) => ['span', { 'data-kiln-bm-end': '', 'data-id': node.attrs.id }],
      leafText: () => '',
    },

    opaqueInline: {
      group: 'inline',
      inline: true,
      atom: true,
      attrs: { xml: { default: '' }, label: { default: '' } },
      parseDOM: [
        { tag: 'span[data-kiln-opaque]', getAttrs: (dom) => ({ xml: (dom as HTMLElement).dataset.xml ?? '' }) },
      ],
      toDOM: (node) => [
        'span',
        { 'data-kiln-opaque': '', 'data-xml': node.attrs.xml, contenteditable: 'false' },
        node.attrs.label || '',
      ],
      leafText: () => '',
    },
  };

  const toggle = (tags: { tag?: string; style?: string; off?: boolean }[], dom: DOMOutputSpec): MarkSpec => ({
    attrs: { value: { default: true } },
    parseDOM: tags.map((t) =>
      t.style
        ? { style: t.style, getAttrs: () => (t.off ? { value: false } : null) }
        : { tag: t.tag as string },
    ),
    toDOM: (mark) => (mark.attrs.value === false ? ['span', { 'data-off': '' }, 0] : dom),
  });

  const marks: Record<string, MarkSpec> = {
    /** Character style (`w:rStyle`). Listed first so it wraps the other formatting marks. */
    runStyle: {
      attrs: { styleId: { default: '' } },
      parseDOM: [
        {
          tag: 'span[data-cstyle]',
          getAttrs: (dom) => ({ styleId: (dom as HTMLElement).dataset.cstyle ?? '' }),
        },
      ],
      toDOM: (mark) => ['span', { 'data-cstyle': mark.attrs.styleId }, 0],
    },
    link: {
      attrs: {
        href: { default: null },
        anchor: { default: null },
        tooltip: { default: null },
        target: { default: null },
      },
      inclusive: false,
      parseDOM: [
        {
          tag: 'a[href]',
          getAttrs: (dom) => {
            const href = (dom as HTMLElement).getAttribute('href') ?? '';
            return href.startsWith('#')
              ? { anchor: href.slice(1), tooltip: (dom as HTMLElement).getAttribute('title') }
              : { href, tooltip: (dom as HTMLElement).getAttribute('title') };
          },
        },
      ],
      toDOM: (mark) => [
        'a',
        {
          href: mark.attrs.href ?? (mark.attrs.anchor ? `#${mark.attrs.anchor}` : null),
          title: mark.attrs.tooltip,
          target: mark.attrs.target,
          rel: mark.attrs.href ? 'noopener noreferrer' : null,
        },
        0,
      ],
    },
    bold: toggle(
      [
        { tag: 'strong' },
        { tag: 'b' },
        { style: 'font-weight=bold' },
        { style: 'font-weight=700' },
        { style: 'font-weight=normal', off: true },
      ],
      ['strong', 0],
    ),
    italic: toggle(
      [
        { tag: 'em' },
        { tag: 'i' },
        { style: 'font-style=italic' },
        { style: 'font-style=normal', off: true },
      ],
      ['em', 0],
    ),
    underline: {
      attrs: { style: { default: 'single' }, color: { default: null } },
      parseDOM: [
        { tag: 'u' },
        { style: 'text-decoration-line=underline' },
        { style: 'text-decoration=underline' },
      ],
      toDOM: (mark) =>
        mark.attrs.style === 'none'
          ? ['span', { 'data-u': 'none' }, 0]
          : [
              'u',
              { 'data-u': mark.attrs.style, style: underlineCss(mark.attrs.style, mark.attrs.color?.val) },
              0,
            ],
    },
    strike: toggle(
      [{ tag: 's' }, { tag: 'strike' }, { style: 'text-decoration-line=line-through' }],
      ['s', 0],
    ),
    dstrike: toggle(
      [{ tag: 's[data-double]' }],
      ['s', { 'data-double': '', style: 'text-decoration-style:double' }, 0],
    ),
    caps: toggle([{ style: 'text-transform=uppercase' }], ['span', { style: 'text-transform:uppercase' }, 0]),
    smallCaps: toggle(
      [{ style: 'font-variant=small-caps' }, { style: 'font-variant-caps=small-caps' }],
      ['span', { style: 'font-variant-caps:small-caps' }, 0],
    ),
    hidden: toggle([{ tag: 'span[data-hidden]' }], ['span', { 'data-hidden': '', class: 'of-hidden' }, 0]),
    vertAlign: {
      attrs: { value: { default: 'superscript' } },
      excludes: 'vertAlign',
      parseDOM: [
        { tag: 'sup', getAttrs: () => ({ value: 'superscript' }) },
        { tag: 'sub', getAttrs: () => ({ value: 'subscript' }) },
      ],
      toDOM: (mark) => [
        mark.attrs.value === 'subscript' ? 'sub' : mark.attrs.value === 'superscript' ? 'sup' : 'span',
        0,
      ],
    },
    color: {
      attrs: { color: { default: { val: '000000' } } },
      parseDOM: [
        {
          style: 'color',
          getAttrs: (v) => {
            const hex = cssColorToHex(v as string);
            return hex ? { color: { val: hex } } : false;
          },
        },
      ],
      toDOM: (mark) => [
        'span',
        {
          style:
            mark.attrs.color?.val && mark.attrs.color.val !== 'auto'
              ? `color:#${mark.attrs.color.val}`
              : null,
        },
        0,
      ],
    },
    highlight: {
      attrs: { color: { default: 'yellow' } },
      parseDOM: [
        { tag: 'mark', getAttrs: (dom) => ({ color: (dom as HTMLElement).dataset.color ?? 'yellow' }) },
      ],
      toDOM: (mark) => ['mark', { 'data-color': mark.attrs.color }, 0],
    },
    runShading: {
      attrs: { shading: { default: null } },
      parseDOM: [
        {
          style: 'background-color',
          getAttrs: (v) => {
            const hex = cssColorToHex(v as string);
            return hex ? { shading: { val: 'clear', fill: { val: hex } } } : false;
          },
        },
      ],
      toDOM: (mark) => [
        'span',
        { style: mark.attrs.shading?.fill?.val ? `background-color:#${mark.attrs.shading.fill.val}` : null },
        0,
      ],
    },
    fontFamily: {
      attrs: { fonts: { default: {} } },
      parseDOM: [
        {
          style: 'font-family',
          getAttrs: (v) => {
            const first = (v as string)
              .split(',')[0]
              ?.trim()
              .replace(/^['"]|['"]$/g, '');
            return first
              ? { fonts: { ascii: first, hAnsi: first, cs: first, eastAsia: first } satisfies RunFonts }
              : false;
          },
        },
      ],
      toDOM: (mark) => {
        const f = mark.attrs.fonts as RunFonts;
        const name = f.ascii ?? f.hAnsi;
        return ['span', { style: name ? `font-family:"${name}"` : null }, 0];
      },
    },
    fontSize: {
      /** `size`/`sizeCs` in half-points (`w:sz`, `w:szCs`). */
      attrs: { size: { default: 22 }, sizeCs: { default: null } },
      parseDOM: [
        {
          style: 'font-size',
          getAttrs: (v) => {
            const pt = parseCssLength(v as string);
            return pt ? { size: Math.round(pt * 2) } : false;
          },
        },
      ],
      toDOM: (mark) => ['span', { style: `font-size:${mark.attrs.size / 2}pt` }, 0],
    },
    charSpacing: {
      /** `w:spacing` (twips). */
      attrs: { value: { default: 0 } },
      parseDOM: [],
      toDOM: (mark) => ['span', { style: `letter-spacing:${mark.attrs.value / 20}pt` }, 0],
    },
    /** Less common run properties (emboss, kern, scale, position, lang, rtl, …) as a RunProps subset. */
    runProps: {
      attrs: { props: { default: {} } },
      parseDOM: [],
      toDOM: () => ['span', { 'data-rprops': '' }, 0],
    },
    /** Run-level content control (`w:sdt` inside a paragraph); `info` is an `SdtInfo`. */
    sdt: {
      attrs: { info: { default: null } },
      excludes: '',
      parseDOM: [],
      toDOM: () => ['span', { 'data-kiln-sdt': '', class: 'of-sdt' }, 0],
    },
    comment: {
      attrs: { id: { default: '' } },
      excludes: '',
      inclusive: false,
      parseDOM: [
        { tag: 'span[data-kiln-comment]', getAttrs: (dom) => ({ id: (dom as HTMLElement).dataset.id ?? '' }) },
      ],
      toDOM: (mark) => ['span', { 'data-kiln-comment': '', 'data-id': mark.attrs.id, class: 'of-comment' }, 0],
    },
    insertion: {
      attrs: { id: { default: '' }, author: { default: '' }, date: { default: '' } },
      inclusive: false,
      excludes: 'insertion deletion',
      parseDOM: [
        {
          tag: 'ins',
          getAttrs: (dom) => ({
            id: (dom as HTMLElement).dataset.id ?? '',
            author: (dom as HTMLElement).dataset.author ?? '',
            date: (dom as HTMLElement).getAttribute('datetime') ?? '',
          }),
        },
      ],
      toDOM: (mark) => [
        'ins',
        { 'data-id': mark.attrs.id, 'data-author': mark.attrs.author, datetime: mark.attrs.date || null },
        0,
      ],
    },
    deletion: {
      attrs: { id: { default: '' }, author: { default: '' }, date: { default: '' } },
      inclusive: false,
      excludes: 'insertion deletion',
      parseDOM: [
        {
          tag: 'del',
          getAttrs: (dom) => ({
            id: (dom as HTMLElement).dataset.id ?? '',
            author: (dom as HTMLElement).dataset.author ?? '',
            date: (dom as HTMLElement).getAttribute('datetime') ?? '',
          }),
        },
      ],
      toDOM: (mark) => [
        'del',
        { 'data-id': mark.attrs.id, 'data-author': mark.attrs.author, datetime: mark.attrs.date || null },
        0,
      ],
    },
    formatChange: {
      /** `w:rPrChange`: `before` holds the run properties prior to the tracked change. */
      attrs: { id: { default: '' }, author: { default: '' }, date: { default: '' }, before: { default: {} } },
      inclusive: false,
      parseDOM: [],
      toDOM: (mark) => ['span', { 'data-kiln-fmtchange': '', 'data-author': mark.attrs.author }, 0],
    },
    spellError: {
      attrs: { kind: { default: 'spelling' } },
      inclusive: false,
      parseDOM: [],
      toDOM: (mark) => [
        'span',
        { class: mark.attrs.kind === 'grammar' ? 'of-grammar-error' : 'of-spell-error' },
        0,
      ],
    },
  };

  return new Schema({ nodes, marks, topNode: 'doc' });
}

function underlineCss(style: string, color: string | undefined): string {
  const map: Record<string, string> = {
    double: 'double',
    dotted: 'dotted',
    dottedHeavy: 'dotted',
    wave: 'wavy',
    wavyHeavy: 'wavy',
    wavyDouble: 'wavy',
    dash: 'dashed',
    dashLong: 'dashed',
    dashedHeavy: 'dashed',
  };
  const css = [`text-decoration-style:${map[style] ?? 'solid'}`];
  if (style.includes('Heavy') || style === 'thick') css.push('text-decoration-thickness:2px');
  if (color && color !== 'auto') css.push(`text-decoration-color:#${color}`);
  return css.join(';');
}

/** The shared schema instance. All doc-model helpers use it unless given another schema. */
export const docSchema: Schema = createDocSchema();
