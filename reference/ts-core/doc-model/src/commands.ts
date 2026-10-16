/**
 * `DocCommands`: pure ProseMirror commands (`(state, dispatch?) => boolean`) implementing Word's
 * formatting and structure operations. They need the document context (styles, numbering) to make
 * Word-correct decisions — e.g. Ctrl+B on text that is bold *because of its style* writes `bold=false`
 * direct formatting, exactly like Word's `<w:b w:val="0"/>`.
 *
 * Commands that create list definitions (toggleBulletList, toggleNumberedList, restartNumbering) add
 * them to `ctx.numbering` only when `dispatch` is given.
 */
import { Fragment, type Mark, type MarkType, type Node as PMNode, type Schema } from '@tiptap/pm/model';
import { type Command, type EditorState, TextSelection, type Transaction } from '@tiptap/pm/state';
import {
  CellSelection,
  addColumnAfter as pmAddColumnAfter,
  addColumnBefore as pmAddColumnBefore,
  addRowAfter as pmAddRowAfter,
  addRowBefore as pmAddRowBefore,
  deleteColumn as pmDeleteColumn,
  deleteRow as pmDeleteRow,
  deleteTable as pmDeleteTable,
  mergeCells as pmMergeCells,
  splitCell as pmSplitCell,
  TableMap,
} from '@tiptap/pm/tables';
import { clampLevel } from './numbering/numbering.ts';
import { PARAGRAPH_PROP_KEYS } from './schema.ts';
import { contentWidth, defaultSectionProps, withOrientation } from './sections.ts';
import {
  type DocContext,
  effectiveNumbering,
  paragraphPropsFromNode,
  resolveRunProps,
  type TableCellContext,
  tableCellContext,
} from './styles/resolve.ts';
import type { Alignment, ParagraphProps, SectionBreakType, SectionProps, ToggleRunProp } from './types.ts';
import { lineMultipleToUnits } from './units.ts';

/** Word's Grow/Shrink Font size ladder (points). */
export const FONT_SIZE_LADDER = [8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72] as const;

/** Marks that are content/annotation rather than formatting (kept by Clear Formatting). */
const NON_FORMATTING_MARKS = new Set([
  'link',
  'comment',
  'insertion',
  'deletion',
  'formatChange',
  'spellError',
]);

type ParagraphVisitor = (node: PMNode, pos: number) => void;

/** Calls `fn` for each paragraph touched by the selection (including inside tables). */
export function forEachSelectedParagraph(state: EditorState, fn: ParagraphVisitor): void {
  const { from, to } = state.selection;
  if (state.selection instanceof CellSelection) {
    state.selection.forEachCell((cell, cellPos) => {
      cell.descendants((n, p) => {
        if (n.type.name === 'paragraph') fn(n, cellPos + 1 + p);
        return n.type.name !== 'paragraph';
      });
    });
    return;
  }
  state.doc.nodesBetween(from, Math.max(to, from), (node, pos) => {
    if (node.type.name === 'paragraph') {
      fn(node, pos);
      return false;
    }
    return true;
  });
}

function tableContextAt(doc: PMNode, pos: number): TableCellContext | undefined {
  const $pos = doc.resolve(pos);
  for (let d = $pos.depth; d >= 2; d--) {
    if ($pos.node(d).type.name === 'tableCell') {
      const table = $pos.node(d - 2);
      const map = TableMap.get(table);
      const rect = map.findCell($pos.before(d) - $pos.start(d - 2));
      return tableCellContext(table, rect.top, rect.left);
    }
  }
  return undefined;
}

/** Text runs in the selection with their paragraph (for effective-formatting decisions). */
function selectedRuns(
  state: EditorState,
): { node: PMNode; pos: number; paragraph: PMNode; parentPos: number }[] {
  const out: { node: PMNode; pos: number; paragraph: PMNode; parentPos: number }[] = [];
  const { from, to } = state.selection;
  state.doc.nodesBetween(from, to, (node, pos, parent) => {
    if (node.isText && parent?.type.name === 'paragraph') {
      const parentPos = state.doc.resolve(pos).before();
      out.push({ node, pos, paragraph: parent, parentPos });
    }
    return true;
  });
  return out;
}

/** Marks that would apply to typed text at the cursor. */
function cursorMarks(state: EditorState): readonly Mark[] {
  const sel = state.selection;
  return (
    state.storedMarks ??
    (sel instanceof TextSelection && sel.$cursor ? sel.$cursor.marks() : sel.$from.marks())
  );
}

export interface DocCommands {
  /** Applies a paragraph style to selected paragraphs, or a character style to the selection. */
  applyStyle(styleId: string): Command;
  /** Toggles a Word toggle property (bold, italic, strike, dstrike, caps, smallCaps, hidden). */
  toggleProperty(
    prop: Extract<ToggleRunProp, 'bold' | 'italic' | 'strike' | 'dstrike' | 'caps' | 'smallCaps' | 'hidden'>,
  ): Command;
  /** Toggles underline (`style` defaults to single). */
  toggleUnderline(style?: string): Command;
  /** Toggles superscript/subscript. */
  toggleVertAlign(value: 'superscript' | 'subscript'): Command;
  /** Sets a formatting mark on the selection (or the stored marks at a cursor). */
  setMark(markName: string, attrs?: Record<string, unknown>): Command;
  /** Removes a mark type from the selection. */
  removeMark(markName: string): Command;
  setFontFamily(name: string): Command;
  /** Sets the font size in points. */
  setFontSize(pt: number): Command;
  growFont(): Command;
  shrinkFont(): Command;
  setAlignment(alignment: Alignment): Command;
  /** Merges direct paragraph properties into selected paragraphs. */
  setParagraphProps(props: ParagraphProps): Command;
  /** Line spacing as a multiple (1, 1.08, 1.15, 1.5, 2, 2.5, 3). */
  setLineSpacing(multiple: number): Command;
  /** Increase Indent: list paragraphs go one level deeper; others indent to the next 0.5" stop. */
  indent(): Command;
  /** Decrease Indent / Shift+Tab. */
  outdent(): Command;
  setNumbering(numId: number, level?: number): Command;
  removeNumbering(): Command;
  toggleBulletList(): Command;
  toggleNumberedList(): Command;
  restartNumbering(): Command;
  clearFormatting(): Command;
  changeCase(mode: 'upper' | 'lower' | 'sentence' | 'title' | 'toggle'): Command;
  insertTable(rows: number, cols: number, options?: { styleId?: string }): Command;
  addRowBefore: Command;
  addRowAfter: Command;
  addColumnBefore: Command;
  addColumnAfter: Command;
  deleteRow: Command;
  deleteColumn: Command;
  deleteTable: Command;
  /** Merges the selected cells (needs a cell selection). */
  mergeCells: Command;
  /** Merges a rectangle of cells of the table at `tablePos` (grid coordinates). */
  mergeCellRange(
    tablePos: number,
    rect: { top: number; left: number; bottom: number; right: number },
  ): Command;
  /** Splits a merged cell back into single cells. */
  splitCell: Command;
  insertPageBreak: Command;
  insertSectionBreak(type: SectionBreakType): Command;
  /** Updates the section containing the selection (Layout ▸ Page Setup). */
  setSectionProps(patch: Partial<SectionProps>): Command;
  setOrientation(orientation: 'portrait' | 'landscape'): Command;
}

/** Creates the command set bound to a document context. `ctx` may be a getter for late binding. */
export function createDocCommands(context: DocContext | (() => DocContext)): DocCommands {
  const ctx = typeof context === 'function' ? context : () => context;

  const markType = (state: EditorState, name: string): MarkType => {
    const t = state.schema.marks[name];
    if (!t) throw new Error(`schema has no mark ${name}`);
    return t;
  };

  /** Effective value of a run property for each selected run (or at the cursor). */
  const effective = <T>(state: EditorState, pick: (r: ReturnType<typeof resolveRunProps>) => T): T[] => {
    const c = ctx();
    if (state.selection.empty) {
      const $c = state.selection.$from;
      const para = $c.parent.type.name === 'paragraph' ? $c.parent : undefined;
      if (!para) return [];
      const table = tableContextAt(state.doc, $c.pos);
      return [
        pick(resolveRunProps(c, { paragraph: para, marks: cursorMarks(state), ...(table ? { table } : {}) })),
      ];
    }
    return selectedRuns(state).map((r) => {
      const table = tableContextAt(state.doc, r.pos);
      return pick(
        resolveRunProps(c, { paragraph: r.paragraph, marks: r.node.marks, ...(table ? { table } : {}) }),
      );
    });
  };

  /**
   * Sets a mark so the *effective* value becomes `on`: removes direct marks of that type, then adds an
   * explicit mark where the inherited value still differs.
   */
  const setToggle = (
    state: EditorState,
    dispatch: ((tr: Transaction) => void) | undefined,
    name: string,
    prop: ToggleRunProp,
    on: boolean,
  ): boolean => {
    const type = markType(state, name);
    if (!dispatch) return true;
    const c = ctx();
    const tr = state.tr;
    if (state.selection.empty) {
      const base = cursorMarks(state).filter((m) => m.type !== type);
      const $c = state.selection.$from;
      const table = tableContextAt(state.doc, $c.pos);
      const inherited = resolveRunProps(c, {
        paragraph: $c.parent,
        marks: base,
        ...(table ? { table } : {}),
      })[prop];
      tr.setStoredMarks(inherited === on ? base : type.create({ value: on }).addToSet(base));
      dispatch(tr);
      return true;
    }
    for (const r of selectedRuns(state)) {
      const from = Math.max(r.pos, state.selection.from);
      const to = Math.min(r.pos + r.node.nodeSize, state.selection.to);
      if (from >= to) continue;
      tr.removeMark(from, to, type);
      const marks = r.node.marks.filter((m) => m.type !== type);
      const table = tableContextAt(state.doc, r.pos);
      const inherited = resolveRunProps(c, { paragraph: r.paragraph, marks, ...(table ? { table } : {}) })[
        prop
      ];
      if (inherited !== on) tr.addMark(from, to, type.create({ value: on }));
    }
    dispatch(tr.scrollIntoView());
    return true;
  };

  const setMarkCmd =
    (name: string, attrs?: Record<string, unknown>): Command =>
    (state, dispatch) => {
      const type = state.schema.marks[name];
      if (!type) return false;
      if (dispatch) {
        const mark = type.create(attrs);
        const tr = state.tr;
        if (state.selection.empty) tr.setStoredMarks(mark.addToSet(cursorMarks(state)));
        else for (const range of state.selection.ranges) tr.addMark(range.$from.pos, range.$to.pos, mark);
        dispatch(tr.scrollIntoView());
      }
      return true;
    };

  const removeMarkCmd =
    (name: string): Command =>
    (state, dispatch) => {
      const type = state.schema.marks[name];
      if (!type) return false;
      if (dispatch) {
        const tr = state.tr;
        if (state.selection.empty) tr.setStoredMarks(type.removeFromSet(cursorMarks(state)));
        else for (const range of state.selection.ranges) tr.removeMark(range.$from.pos, range.$to.pos, type);
        dispatch(tr.scrollIntoView());
      }
      return true;
    };

  const updateParagraphs =
    (fn: (node: PMNode, pos: number) => Record<string, unknown> | null | undefined): Command =>
    (state, dispatch) => {
      const changes: { pos: number; attrs: Record<string, unknown> }[] = [];
      forEachSelectedParagraph(state, (node, pos) => {
        const attrs = fn(node, pos);
        if (attrs) changes.push({ pos, attrs: { ...node.attrs, ...attrs } });
      });
      if (changes.length === 0) return false;
      if (dispatch) {
        const tr = state.tr;
        for (const c of changes) tr.setNodeMarkup(c.pos, undefined, c.attrs);
        dispatch(tr.scrollIntoView());
      }
      return true;
    };

  const listKindOf = (node: PMNode): 'bullet' | 'number' | undefined => {
    const c = ctx();
    const num = effectiveNumbering(c, paragraphPropsFromNode(node));
    if (!num || !c.numbering) return undefined;
    const lvl = c.numbering.level(num.numId, num.level);
    if (!lvl) return undefined;
    return lvl.format === 'bullet' ? 'bullet' : 'number';
  };

  const toggleList =
    (kind: 'bullet' | 'number'): Command =>
    (state, dispatch) => {
      const c = ctx();
      if (!c.numbering) return false;
      const paragraphs: { node: PMNode; pos: number }[] = [];
      forEachSelectedParagraph(state, (node, pos) => paragraphs.push({ node, pos }));
      if (paragraphs.length === 0) return false;
      if (!dispatch) return true;
      const tr = state.tr;
      const allInKind = paragraphs.every((p) => listKindOf(p.node) === kind);
      if (allInKind) {
        for (const p of paragraphs) {
          const fromStyle = c.styles.styleNumbering(p.node.attrs.styleId ?? undefined);
          const attrs: Record<string, unknown> = {
            ...p.node.attrs,
            numbering: fromStyle ? { numId: 0, level: 0 } : null,
          };
          if (attrs.styleId === 'ListParagraph') attrs.styleId = null;
          tr.setNodeMarkup(p.pos, undefined, attrs);
        }
        dispatch(tr);
        return true;
      }
      // Continue an adjacent list of the same kind (Word joins a list directly above), else create one.
      const first = paragraphs[0] as { node: PMNode; pos: number };
      const $first = state.doc.resolve(first.pos);
      const prev = $first.index() > 0 ? $first.parent.child($first.index() - 1) : undefined;
      let numId: number | undefined;
      if (prev?.type.name === 'paragraph' && listKindOf(prev) === kind) {
        numId = effectiveNumbering(c, paragraphPropsFromNode(prev))?.numId;
      }
      numId ??= (kind === 'bullet' ? c.numbering.createBulletList() : c.numbering.createNumberedList()).id;
      for (const p of paragraphs) {
        const current = effectiveNumbering(c, paragraphPropsFromNode(p.node));
        const styleId = p.node.attrs.styleId as string | null;
        const isNormal = !styleId || styleId === c.styles.defaultStyle('paragraph')?.id;
        tr.setNodeMarkup(p.pos, undefined, {
          ...p.node.attrs,
          numbering: { numId, level: current?.level ?? 0 },
          styleId: isNormal && c.styles.has('ListParagraph') ? 'ListParagraph' : styleId,
        });
      }
      dispatch(tr.scrollIntoView());
      return true;
    };

  const shiftIndent =
    (dir: 1 | -1): Command =>
    (state, dispatch) => {
      const c = ctx();
      const step = c.settings?.defaultTabStop ?? 720;
      return updateParagraphs((node) => {
        const num = effectiveNumbering(c, paragraphPropsFromNode(node));
        if (num) {
          const level = clampLevel(num.level + dir);
          if (level === num.level) return null;
          return { numbering: { numId: num.numId, level } };
        }
        const ind = (node.attrs.indent as ParagraphProps['indent'] | null) ?? {};
        const left = ind.left ?? 0;
        const next =
          dir > 0 ? (Math.floor(left / step) + 1) * step : Math.max(0, (Math.ceil(left / step) - 1) * step);
        if (next === left) return null;
        return { indent: { ...ind, left: next } };
      })(state, dispatch);
    };

  const fixGrids = (tr: Transaction): Transaction => {
    tr.doc.descendants((node, pos) => {
      if (node.type.name !== 'table') return true;
      const map = TableMap.get(node);
      const grid = node.attrs.grid as number[] | null;
      if (grid && grid.length !== map.width) {
        const total = grid.reduce((a, b) => a + b, 0);
        const w = Math.floor(total / map.width);
        tr.setNodeMarkup(pos, undefined, { ...node.attrs, grid: Array.from({ length: map.width }, () => w) });
      }
      return false;
    });
    return tr;
  };

  const tableCmd =
    (cmd: Command): Command =>
    (state, dispatch, view) =>
      cmd(state, dispatch ? (tr) => dispatch(fixGrids(tr)) : undefined, view);

  const sectionAt = (state: EditorState): { node: PMNode; pos: number } | undefined => {
    const $from = state.selection.$from;
    for (let d = $from.depth; d >= 1; d--) {
      if ($from.node(d).type.name === 'section') return { node: $from.node(d), pos: $from.before(d) };
    }
    return undefined;
  };

  const sizeStep =
    (dir: 1 | -1): Command =>
    (state, dispatch) => {
      const sizes = effective(state, (r) => r.size / 2);
      if (sizes.length === 0) return false;
      const current = dir > 0 ? Math.min(...sizes) : Math.max(...sizes);
      let next: number;
      if (dir > 0)
        next =
          FONT_SIZE_LADDER.find((s) => s > current) ?? Math.min(1638, Math.floor(current / 10) * 10 + 10);
      else next = [...FONT_SIZE_LADDER].reverse().find((s) => s < current) ?? Math.max(1, current - 1);
      if (current > 72)
        next =
          dir > 0
            ? Math.floor(current / 10) * 10 + 10
            : current - 10 < 72
              ? 72
              : Math.ceil(current / 10) * 10 - 10;
      return setMarkCmd('fontSize', { size: Math.round(next * 2) })(state, dispatch);
    };

  return {
    applyStyle: (styleId) => (state, dispatch) => {
      const c = ctx();
      const style = c.styles.get(styleId);
      if (!style) return false;
      const sel = state.selection;
      if (style.type === 'character') return setMarkCmd('runStyle', { styleId })(state, dispatch);
      if (style.type !== 'paragraph') return false;
      // Linked style on a partial selection inside one paragraph applies the linked character style.
      if (
        !sel.empty &&
        style.link &&
        sel.$from.sameParent(sel.$to) &&
        sel.$from.parent.type.name === 'paragraph'
      ) {
        const whole = sel.$from.parentOffset === 0 && sel.$to.parentOffset === sel.$to.parent.content.size;
        const linked = c.styles.get(style.link);
        if (!whole && linked?.type === 'character')
          return setMarkCmd('runStyle', { styleId: linked.id })(state, dispatch);
      }
      return updateParagraphs(() => ({ styleId }))(state, dispatch);
    },

    toggleProperty: (prop) => (state, dispatch) => {
      const values = effective(state, (r) => r[prop] === true);
      if (values.length === 0) return false;
      const allOn = values.every(Boolean);
      return setToggle(state, dispatch, prop, prop, !allOn);
    },

    toggleUnderline:
      (style = 'single') =>
      (state, dispatch) => {
        const values = effective(state, (r) => !!r.underline && r.underline.style !== 'none');
        if (values.length === 0) return false;
        if (values.every(Boolean)) {
          // Remove direct underline; if the style still underlines, write an explicit "none".
          const inherited = (() => {
            const type = markType(state, 'underline');
            const c = ctx();
            const $c = state.selection.$from;
            if ($c.parent.type.name !== 'paragraph') return false;
            const marks = cursorMarks(state).filter((m) => m.type !== type);
            const u = resolveRunProps(c, { paragraph: $c.parent, marks }).underline;
            return !!u && u.style !== 'none';
          })();
          if (!inherited) return removeMarkCmd('underline')(state, dispatch);
          return setMarkCmd('underline', { style: 'none' })(state, dispatch);
        }
        return setMarkCmd('underline', { style })(state, dispatch);
      },

    toggleVertAlign: (value) => (state, dispatch) => {
      const values = effective(state, (r) => r.vertAlign === value);
      if (values.length === 0) return false;
      if (values.every(Boolean)) return removeMarkCmd('vertAlign')(state, dispatch);
      return setMarkCmd('vertAlign', { value })(state, dispatch);
    },

    setMark: (name, attrs) => setMarkCmd(name, attrs),
    removeMark: (name) => removeMarkCmd(name),
    setFontFamily: (name) =>
      setMarkCmd('fontFamily', { fonts: { ascii: name, hAnsi: name, eastAsia: name, cs: name } }),
    setFontSize: (pt) => setMarkCmd('fontSize', { size: Math.max(2, Math.round(pt * 2)) }),
    growFont: () => sizeStep(1),
    shrinkFont: () => sizeStep(-1),

    setAlignment: (alignment) => updateParagraphs(() => ({ alignment })),

    setParagraphProps: (props) =>
      updateParagraphs((node) => {
        const out: Record<string, unknown> = {};
        for (const key of PARAGRAPH_PROP_KEYS) {
          const v = props[key];
          if (v === undefined) continue;
          const cur = node.attrs[key];
          out[key] =
            v !== null && typeof v === 'object' && !Array.isArray(v) && cur && typeof cur === 'object'
              ? { ...cur, ...v }
              : v;
        }
        return out;
      }),

    setLineSpacing: (multiple) =>
      updateParagraphs((node) => ({
        spacing: { ...(node.attrs.spacing ?? {}), line: lineMultipleToUnits(multiple), lineRule: 'auto' },
      })),

    indent: () => shiftIndent(1),
    outdent: () => shiftIndent(-1),

    setNumbering: (numId, level = 0) =>
      updateParagraphs(() => ({ numbering: { numId, level: clampLevel(level) } })),
    removeNumbering: () =>
      updateParagraphs((node) => {
        const c = ctx();
        const fromStyle = c.styles.styleNumbering(node.attrs.styleId ?? undefined);
        return { numbering: fromStyle ? { numId: 0, level: 0 } : null };
      }),
    toggleBulletList: () => toggleList('bullet'),
    toggleNumberedList: () => toggleList('number'),

    restartNumbering: () => (state, dispatch) => {
      const c = ctx();
      const $from = state.selection.$from;
      const para = $from.parent.type.name === 'paragraph' ? $from.parent : undefined;
      if (!para || !c.numbering) return false;
      const num = effectiveNumbering(c, paragraphPropsFromNode(para));
      if (!num) return false;
      if (!dispatch) return true;
      const fresh = c.numbering.restart(num.numId, num.level);
      const start = $from.before();
      const tr = state.tr;
      state.doc.nodesBetween(start, state.doc.content.size, (node, pos) => {
        if (node.type.name !== 'paragraph') return true;
        if (pos < start) return false;
        const n = effectiveNumbering(c, paragraphPropsFromNode(node));
        if (n && n.numId === num.numId)
          tr.setNodeMarkup(pos, undefined, { ...node.attrs, numbering: { numId: fresh.id, level: n.level } });
        return false;
      });
      dispatch(tr);
      return true;
    },

    clearFormatting: () => (state, dispatch) => {
      if (!dispatch) return true;
      const tr = state.tr;
      const { from, to } = state.selection;
      if (from !== to) {
        for (const [name, type] of Object.entries(state.schema.marks)) {
          if (!NON_FORMATTING_MARKS.has(name)) tr.removeMark(from, to, type);
        }
      } else {
        tr.setStoredMarks(cursorMarks(state).filter((m) => NON_FORMATTING_MARKS.has(m.type.name)));
      }
      const normal = ctx().styles.defaultStyle('paragraph')?.id ?? null;
      forEachSelectedParagraph(state, (node, pos) => {
        const attrs: Record<string, unknown> = { ...node.attrs };
        for (const k of PARAGRAPH_PROP_KEYS) attrs[k] = null;
        attrs.styleId = normal;
        attrs.markProps = null;
        tr.setNodeMarkup(pos, undefined, attrs);
      });
      dispatch(tr);
      return true;
    },

    changeCase: (mode) => (state, dispatch) => {
      const { from, to } = state.selection;
      if (from === to) return false;
      if (dispatch) {
        const tr = state.tr;
        let sentenceStart = true;
        const textNodes: { pos: number; node: PMNode }[] = [];
        state.doc.nodesBetween(from, to, (node, pos) => {
          if (node.isText) textNodes.push({ pos, node });
          return true;
        });
        for (const { pos, node } of textNodes) {
          const start = Math.max(from, pos);
          const end = Math.min(to, pos + node.nodeSize);
          const text = (node.text ?? '').slice(start - pos, end - pos);
          let out = text;
          if (mode === 'upper') out = text.toUpperCase();
          else if (mode === 'lower') out = text.toLowerCase();
          else if (mode === 'toggle')
            out = [...text]
              .map((ch) => (ch === ch.toUpperCase() ? ch.toLowerCase() : ch.toUpperCase()))
              .join('');
          else if (mode === 'title')
            out = text
              .toLowerCase()
              .replace(/(^|[\s\-–—(["'])(\p{L})/gu, (_m, s: string, ch: string) => s + ch.toUpperCase());
          else {
            out = [...text.toLowerCase()]
              .map((ch) => {
                if (sentenceStart && /\p{L}/u.test(ch)) {
                  sentenceStart = false;
                  return ch.toUpperCase();
                }
                if (/[.!?]/.test(ch)) sentenceStart = true;
                return ch;
              })
              .join('');
          }
          if (out !== text)
            tr.replaceWith(tr.mapping.map(start), tr.mapping.map(end), state.schema.text(out, node.marks));
        }
        dispatch(tr);
      }
      return true;
    },

    insertTable:
      (rows, cols, options = {}) =>
      (state, dispatch) => {
        if (rows < 1 || cols < 1 || rows > 32767 || cols > 63) return false;
        const schema = state.schema;
        const section = sectionAt(state);
        const props = (section?.node.attrs.props as SectionProps | null) ?? defaultSectionProps();
        const c = ctx();
        const styleId = options.styleId ?? (c.styles.has('TableGrid') ? 'TableGrid' : null);
        const cellMargins = 216; // 108 left + 108 right (Normal Table)
        const width = Math.floor(contentWidth(props) / cols);
        const grid = Array.from({ length: cols }, () => width);
        const colPx = Math.round(((width - cellMargins) / 1440) * 96);
        const table = buildTable(schema, rows, cols, {
          styleId,
          grid,
          width: { type: 'dxa', value: width },
          colwidth: colPx,
        });
        if (dispatch) {
          const tr = state.tr.replaceSelectionWith(table);
          // Place the cursor in the first cell (table → row → cell → paragraph → text). The inserted
          // node keeps its identity in the new document.
          let found = -1;
          tr.doc.descendants((n, pos) => {
            if (found >= 0) return false;
            if (n === table) found = pos;
            return (
              n.type.name === 'doc' ||
              n.type.name === 'section' ||
              n.type.name === 'tableCell' ||
              n.type.name === 'tableRow' ||
              (n.type.name === 'table' && n !== table)
            );
          });
          if (found >= 0) tr.setSelection(TextSelection.near(tr.doc.resolve(found + 4)));
          dispatch(tr.scrollIntoView());
        }
        return true;
      },

    addRowBefore: tableCmd(pmAddRowBefore),
    addRowAfter: tableCmd(pmAddRowAfter),
    addColumnBefore: tableCmd(pmAddColumnBefore),
    addColumnAfter: tableCmd(pmAddColumnAfter),
    deleteRow: tableCmd(pmDeleteRow),
    deleteColumn: tableCmd(pmDeleteColumn),
    deleteTable: pmDeleteTable,
    mergeCells: tableCmd(pmMergeCells),
    mergeCellRange: (tablePos, rect) => (state, dispatch, view) => {
      const table = state.doc.nodeAt(tablePos);
      if (table?.type.name !== 'table') return false;
      const map = TableMap.get(table);
      if (
        rect.left < 0 ||
        rect.top < 0 ||
        rect.right > map.width ||
        rect.bottom > map.height ||
        rect.right <= rect.left ||
        rect.bottom <= rect.top
      )
        return false;
      const start = tablePos + 1;
      const anchor = start + (map.map[rect.top * map.width + rect.left] as number);
      const head = start + (map.map[(rect.bottom - 1) * map.width + rect.right - 1] as number);
      const selState = state.apply(state.tr.setSelection(CellSelection.create(state.doc, anchor, head)));
      return tableCmd(pmMergeCells)(
        selState,
        dispatch
          ? (tr) => {
              // Re-base the merge steps onto the original state.
              const out = state.tr;
              for (const step of tr.steps) out.step(step);
              dispatch(out);
            }
          : undefined,
        view,
      );
    },
    splitCell: tableCmd(pmSplitCell),

    insertPageBreak: (state, dispatch) => {
      const type = state.schema.nodes.pageBreak;
      if (!type) return false;
      const $from = state.selection.$from;
      if ($from.parent.type.name !== 'paragraph') return false;
      if (dispatch) {
        const tr = state.tr.replaceSelectionWith(type.create(), false);
        const after = tr.selection.from;
        tr.split(after);
        dispatch(tr.scrollIntoView());
      }
      return true;
    },

    insertSectionBreak: (type) => (state, dispatch) => {
      if (!sectionAt(state)) return false;
      if (!dispatch) return true;
      const tr = state.tr;
      if (!state.selection.empty) tr.deleteSelection();
      const $pos = tr.doc.resolve(tr.selection.from);
      if ($pos.depth < 1) return false;
      const section = $pos.node(1);
      const sectionPos = $pos.before(1);
      const props = (section.attrs.props as SectionProps | null) ?? defaultSectionProps();
      const sectionType = state.schema.nodes.section as PMNode['type'];
      // The section before the break keeps how it started; the new one starts with `type`.
      const secondProps: SectionProps = { ...props, type };
      tr.setNodeMarkup(sectionPos, undefined, { ...section.attrs, props: { ...props } });
      if ($pos.depth === 2 && $pos.parent.type.name === 'paragraph') {
        // Text-level break inside a top-level paragraph: split the paragraph and the section.
        tr.split($pos.pos, 2, [
          { type: sectionType, attrs: { props: secondProps } },
          { type: $pos.parent.type, attrs: { ...$pos.parent.attrs, paraId: null } },
        ]);
      } else {
        // Inside a table or another nested block: break after the containing top-level block.
        const after = $pos.after(2);
        const sectionEnd = sectionPos + section.nodeSize;
        if (after >= sectionEnd - 1) {
          const paragraph = state.schema.nodes.paragraph?.create() as PMNode;
          tr.insert(sectionEnd, sectionType.create({ props: secondProps }, [paragraph]));
          tr.setSelection(TextSelection.near(tr.doc.resolve(sectionEnd + 2)));
        } else {
          tr.split(after, 1, [{ type: sectionType, attrs: { props: secondProps } }]);
        }
      }
      dispatch(tr.scrollIntoView());
      return true;
    },

    setSectionProps: (patch) => (state, dispatch) => {
      const section = sectionAt(state);
      if (!section) return false;
      if (dispatch) {
        const props = (section.node.attrs.props as SectionProps | null) ?? defaultSectionProps();
        dispatch(
          state.tr.setNodeMarkup(section.pos, undefined, {
            ...section.node.attrs,
            props: { ...props, ...patch },
          }),
        );
      }
      return true;
    },

    setOrientation: (orientation) => (state, dispatch) => {
      const section = sectionAt(state);
      if (!section) return false;
      if (dispatch) {
        const props = (section.node.attrs.props as SectionProps | null) ?? defaultSectionProps();
        dispatch(
          state.tr.setNodeMarkup(section.pos, undefined, {
            ...section.node.attrs,
            props: withOrientation(props, orientation),
          }),
        );
      }
      return true;
    },
  };
}

/** Builds a table node with empty paragraphs. */
export function buildTable(
  schema: Schema,
  rows: number,
  cols: number,
  opts: {
    styleId?: string | null;
    grid?: number[];
    width?: { type: 'dxa'; value: number };
    colwidth?: number;
  } = {},
): PMNode {
  const cell = () =>
    schema.node(
      'tableCell',
      { colwidth: opts.colwidth ? [opts.colwidth] : null, width: opts.width ?? null },
      [schema.node('paragraph')],
    );
  const row = () => schema.node('tableRow', null, Fragment.from(Array.from({ length: cols }, cell)));
  return schema.node(
    'table',
    {
      styleId: opts.styleId ?? null,
      grid: opts.grid ?? null,
      width: { type: 'auto', value: 0 },
      look: {
        firstRow: true,
        lastRow: false,
        firstColumn: true,
        lastColumn: false,
        noHBand: false,
        noVBand: true,
      },
    },
    Array.from({ length: rows }, row),
  );
}
