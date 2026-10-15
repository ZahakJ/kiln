/**
 * The document style sheet (`styles.xml`): paragraph, character, table and numbering styles with
 * `basedOn` inheritance, `next` (style of the following paragraph) and `link` (paragraph ↔ character
 * linked styles), plus `w:docDefaults`. Latent style information (`w:latentStyles`) only drives Word's
 * UI gallery and is deliberately not modelled; the DOCX layer passes it through.
 */
import type {
  NumberingRef,
  ParagraphProps,
  RunProps,
  TableCellProps,
  TableConditionType,
  TableProps,
  TableRowProps,
} from '../types.ts';

export type StyleType = 'paragraph' | 'character' | 'table' | 'numbering';

/** Formatting of one conditional region of a table style (`w:tblStylePr`). */
export interface TableStyleCondition {
  pPr?: ParagraphProps;
  rPr?: RunProps;
  tblPr?: TableProps;
  trPr?: TableRowProps;
  tcPr?: TableCellProps;
}

export interface Style {
  /** `w:styleId` — the key used by `w:pStyle`/`w:rStyle`/`w:tblStyle`. */
  id: string;
  type: StyleType;
  /** `w:name` as stored (built-ins are lower-case in files: `heading 1`). */
  name: string;
  basedOn?: string;
  /** Style applied to the next paragraph when pressing Enter at the end of this one. */
  next?: string;
  /** Linked style of the other type (paragraph ↔ character). */
  link?: string;
  /** `w:default="1"`: the default style of its type. */
  isDefault?: boolean;
  /** `w:customStyle="1"`: user-defined (not a Word built-in). */
  custom?: boolean;
  /** Shown in the Styles gallery (`w:qFormat`). */
  quickStyle?: boolean;
  uiPriority?: number;
  semiHidden?: boolean;
  unhideWhenUsed?: boolean;
  hidden?: boolean;
  locked?: boolean;
  aliases?: string[];
  pPr?: ParagraphProps;
  rPr?: RunProps;
  /** Table styles only. */
  tblPr?: TableProps;
  trPr?: TableRowProps;
  tcPr?: TableCellProps;
  conditions?: Partial<Record<TableConditionType, TableStyleCondition>>;
}

export interface DocDefaults {
  pPr: ParagraphProps;
  rPr: RunProps;
}

export interface StyleSheetJSON {
  docDefaults: DocDefaults;
  styles: Style[];
}

const BUILTIN_DISPLAY: Record<string, string> = {
  'toc heading': 'TOC Heading',
  'table of figures': 'Table of Figures',
  'html preformatted': 'HTML Preformatted',
};

/** Word's UI name for a style (`heading 1` → `Heading 1`, `toc 2` → `TOC 2`). */
export function styleDisplayName(name: string): string {
  const special = BUILTIN_DISPLAY[name.toLowerCase()];
  if (special) return special;
  const toc = /^toc (\d)$/i.exec(name);
  if (toc) return `TOC ${toc[1]}`;
  if (name !== name.toLowerCase()) return name;
  return name.replace(/(^|\s)([a-z])/g, (_m, s: string, c: string) => s + c.toUpperCase());
}

/** Builds a style id from a display name the way Word does (`Heading 1` → `Heading1`). */
export function styleIdFromName(name: string): string {
  const cleaned = styleDisplayName(name).replace(/[^\p{L}\p{N}]+/gu, '');
  return cleaned || 'Style';
}

export class StyleSheetError extends Error {
  override name = 'StyleSheetError';
}

/** A mutable style registry. Use `clone()` before editing a sheet that is shared. */
export class StyleSheet {
  docDefaults: DocDefaults;
  readonly #styles = new Map<string, Style>();

  constructor(docDefaults: DocDefaults = { pPr: {}, rPr: {} }, styles: Iterable<Style> = []) {
    this.docDefaults = docDefaults;
    for (const s of styles) this.add(s);
  }

  get size(): number {
    return this.#styles.size;
  }

  /** All styles in insertion order. */
  all(): Style[] {
    return [...this.#styles.values()];
  }

  byType(type: StyleType): Style[] {
    return this.all().filter((s) => s.type === type);
  }

  has(id: string): boolean {
    return this.#styles.has(id);
  }

  get(id: string | undefined | null): Style | undefined {
    return id ? this.#styles.get(id) : undefined;
  }

  /** Finds a style by its stored name, display name or alias (case-insensitive). */
  getByName(name: string): Style | undefined {
    const key = name.trim().toLowerCase();
    for (const s of this.#styles.values()) {
      if (s.name.toLowerCase() === key || styleDisplayName(s.name).toLowerCase() === key) return s;
      if (s.aliases?.some((a) => a.toLowerCase() === key)) return s;
    }
    return undefined;
  }

  /** Adds or replaces a style. A new default style clears the previous default of its type. */
  add(style: Style): this {
    if (!style.id) throw new StyleSheetError('style id must not be empty');
    if (style.isDefault) {
      for (const s of this.#styles.values())
        if (s.type === style.type && s.id !== style.id && s.isDefault) s.isDefault = false;
    }
    this.#styles.set(style.id, style);
    return this;
  }

  /** Merges `patch` into an existing style (shallow per property group). */
  update(id: string, patch: Partial<Omit<Style, 'id'>>): Style {
    const s = this.#styles.get(id);
    if (!s) throw new StyleSheetError(`unknown style ${id}`);
    const next: Style = { ...s, ...patch, id };
    if (patch.basedOn !== undefined && patch.basedOn !== '' && this.#wouldCycle(id, patch.basedOn)) {
      throw new StyleSheetError(`basedOn cycle: ${id} → ${patch.basedOn}`);
    }
    this.add(next);
    return next;
  }

  /**
   * Removes a style. Styles based on it are re-parented to its own `basedOn` (as Word does), and
   * `next`/`link` references to it are dropped.
   */
  remove(id: string): boolean {
    const s = this.#styles.get(id);
    if (!s) return false;
    this.#styles.delete(id);
    for (const other of this.#styles.values()) {
      if (other.basedOn === id) {
        if (s.basedOn) other.basedOn = s.basedOn;
        else delete other.basedOn;
      }
      if (other.next === id) delete other.next;
      if (other.link === id) delete other.link;
    }
    return true;
  }

  /** The default style of a type (`Normal`, `Default Paragraph Font`, `Normal Table`, `No List`). */
  defaultStyle(type: StyleType): Style | undefined {
    for (const s of this.#styles.values()) if (s.type === type && s.isDefault) return s;
    return undefined;
  }

  /**
   * The `basedOn` chain of a style ordered from root ancestor to the style itself. Cycles, missing
   * parents and parents of a different type end the chain (Word ignores such links).
   */
  chain(id: string | undefined | null): Style[] {
    const out: Style[] = [];
    const seen = new Set<string>();
    let cur = this.get(id);
    const type = cur?.type;
    while (cur && !seen.has(cur.id) && cur.type === type) {
      seen.add(cur.id);
      out.push(cur);
      cur = this.get(cur.basedOn);
    }
    return out.reverse();
  }

  /** The style of the paragraph created after one in `id` (Word's "Style for following paragraph"). */
  nextStyleId(id: string): string {
    const s = this.get(id);
    if (s?.next && this.has(s.next)) return s.next;
    return s?.id ?? this.defaultStyle('paragraph')?.id ?? 'Normal';
  }

  /** The character style linked to a paragraph style (or vice versa), if any. */
  linked(id: string): Style | undefined {
    const linked = this.get(this.get(id)?.link);
    return linked && linked.type !== this.get(id)?.type ? linked : undefined;
  }

  /** Quick styles for the Home ▸ Styles gallery, ordered like Word (uiPriority, then name). */
  quickStyles(): Style[] {
    return this.all()
      .filter((s) => s.quickStyle && !s.hidden && (s.type === 'paragraph' || s.type === 'character'))
      .sort(
        (a, b) =>
          (a.uiPriority ?? 99) - (b.uiPriority ?? 99) ||
          styleDisplayName(a.name).localeCompare(styleDisplayName(b.name)),
      );
  }

  /** Numbering reference contributed by a paragraph style chain (nearest wins; `numId` 0 cancels). */
  styleNumbering(id: string | undefined | null): NumberingRef | undefined {
    let found: Partial<NumberingRef> | undefined;
    for (const s of this.chain(id)) {
      const n = s.pPr?.numbering;
      if (n) found = { ...found, ...n };
    }
    if (!found || found.numId === undefined || found.numId === 0) return undefined;
    return { numId: found.numId, level: found.level ?? 0 };
  }

  /** True if `ancestorId` is (transitively) a `basedOn` ancestor of `id` or `id` itself. */
  inheritsFrom(id: string, ancestorId: string): boolean {
    return this.chain(id).some((s) => s.id === ancestorId);
  }

  /** Generates an unused style id for a new style named `name`. */
  uniqueId(name: string): string {
    const base = styleIdFromName(name);
    if (!this.has(base)) return base;
    for (let i = 1; ; i++) if (!this.has(`${base}${i}`)) return `${base}${i}`;
  }

  #wouldCycle(id: string, basedOn: string): boolean {
    let cur: string | undefined = basedOn;
    const seen = new Set<string>();
    while (cur && !seen.has(cur)) {
      if (cur === id) return true;
      seen.add(cur);
      cur = this.get(cur)?.basedOn;
    }
    return false;
  }

  clone(): StyleSheet {
    return StyleSheet.fromJSON(structuredClone(this.toJSON()));
  }

  toJSON(): StyleSheetJSON {
    return { docDefaults: this.docDefaults, styles: this.all() };
  }

  static fromJSON(json: StyleSheetJSON): StyleSheet {
    return new StyleSheet(json.docDefaults, json.styles);
  }
}
