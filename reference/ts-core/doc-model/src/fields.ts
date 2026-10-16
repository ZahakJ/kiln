/**
 * Field instructions (ECMA-376 §17.16): the text inside `w:fldSimple/@w:instr` or between
 * `w:fldChar begin` and `separate`. `parseFieldInstruction` tokenises Word's syntax (quoted arguments,
 * `\x` switches, the general `\*`, numeric `\#` and date `\@` format switches) and gives typed views of
 * PAGE, NUMPAGES, SECTIONPAGES, DATE, TIME, CREATEDATE, SAVEDATE, PRINTDATE, TOC, REF, PAGEREF,
 * HYPERLINK and SEQ. Any other field keeps its tokens and raw text so it round-trips unchanged.
 */
import { formatNumber, type NumberFormat } from './numbering/formats.ts';

export interface FieldSwitch {
  /** The switch name including backslash: `\o`, `\h`, `\*`, `\@`. */
  name: string;
  value?: string;
}

interface FieldBase {
  /** Upper-cased field keyword. */
  type: string;
  /** Positional (non-switch) arguments. */
  args: string[];
  switches: FieldSwitch[];
  /** General formatting switches (`\* MERGEFORMAT`, `\* roman`, …). */
  general: string[];
  /** Numeric picture (`\# "0.00"`). */
  numericFormat?: string;
  /** Date-time picture (`\@ "d MMMM yyyy"`). */
  dateFormat?: string;
  /** The original instruction text, preserved verbatim. */
  raw: string;
}

export interface PageField extends FieldBase {
  type: 'PAGE' | 'NUMPAGES' | 'SECTIONPAGES' | 'SECTION';
}

export interface DateField extends FieldBase {
  type: 'DATE' | 'TIME' | 'CREATEDATE' | 'SAVEDATE' | 'PRINTDATE';
  /** The effective picture, defaulting to Word's (`M/d/yyyy` for DATE, `h:mm am/pm` for TIME). */
  picture: string;
}

export interface TocField extends FieldBase {
  type: 'TOC';
  /** `\o "1-3"`: outline levels (headings) to include. */
  outlineLevels?: { from: number; to: number };
  /** `\t "Style,1,Other,2"`: custom styles and their TOC levels. */
  styles: { style: string; level: number }[];
  /** `\h`: entries are hyperlinks. */
  hyperlinks: boolean;
  /** `\z`: hide tab leaders and page numbers in Web Layout. */
  hideInWebLayout: boolean;
  /** `\u`: use paragraph outline levels. */
  useOutlineLevels: boolean;
  /** `\n "1-1"`: omit page numbers for these levels (all levels when no range is given). */
  omitPageNumbers?: { from: number; to: number } | 'all';
  /** `\c "Figure"`: table of figures for a caption label (SEQ identifier). */
  captionLabel?: string;
  /** `\b bookmark`: limit to a bookmarked range. */
  bookmark?: string;
  /** `\p "-"`: separator between entry and page number. */
  separator?: string;
}

export interface RefField extends FieldBase {
  type: 'REF' | 'PAGEREF' | 'NOTEREF';
  bookmark: string;
  hyperlink: boolean;
  /** `\p`: insert "above"/"below" relative position. */
  relativePosition: boolean;
  /** REF `\n`, `\r`, `\w`: paragraph number without/with relative/full context. */
  paragraphNumber?: 'noContext' | 'relative' | 'fullContext';
}

export interface HyperlinkField extends FieldBase {
  type: 'HYPERLINK';
  url?: string;
  /** `\l`: location (bookmark) in the target. */
  anchor?: string;
  /** `\o`: screen tip. */
  tooltip?: string;
  /** `\t`: target frame. */
  target?: string;
  /** `\n`: open in new window. */
  newWindow: boolean;
}

export interface SeqField extends FieldBase {
  type: 'SEQ';
  identifier: string;
  bookmark?: string;
  /** `\c`: repeat the closest preceding number. */
  repeat: boolean;
  /** `\h`: hide the result. */
  hidden: boolean;
  /** `\r n`: reset to n. */
  resetTo?: number;
  /** `\s n`: reset at heading level n. */
  resetAtHeading?: number;
}

export interface UnknownField extends FieldBase {
  kind: 'unknown';
}

export type FieldInstruction =
  | PageField
  | DateField
  | TocField
  | RefField
  | HyperlinkField
  | SeqField
  | UnknownField;

/** Switches that never take an argument, per field type (otherwise a following token is the value). */
const FLAG_SWITCHES: Record<string, string[]> = {
  TOC: ['\\h', '\\z', '\\u', '\\w', '\\x'],
  REF: ['\\h', '\\n', '\\r', '\\w', '\\p', '\\f', '\\t'],
  NOTEREF: ['\\h', '\\p', '\\f'],
  PAGEREF: ['\\h', '\\p'],
  HYPERLINK: ['\\m', '\\n'],
  SEQ: ['\\c', '\\h', '\\n'],
  DATE: ['\\h', '\\l', '\\s'],
  TIME: [],
};
const GLOBAL_FLAGS = ['\\!'];

interface Token {
  text: string;
  quoted: boolean;
}

/** Splits an instruction into tokens; quotes group words and `\"` escapes a quote inside them. */
export function tokenizeField(instr: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  const s = instr;
  while (i < s.length) {
    const ch = s[i] as string;
    if (/\s/.test(ch)) {
      i++;
      continue;
    }
    if (ch === '"' || ch === '\u201C' || ch === '\u201D') {
      let text = '';
      i++;
      while (i < s.length && s[i] !== '"' && s[i] !== '\u201D' && s[i] !== '\u201C') {
        if (s[i] === '\\' && (s[i + 1] === '"' || s[i + 1] === '\\')) {
          text += s[i + 1];
          i += 2;
        } else {
          text += s[i];
          i++;
        }
      }
      i++; // closing quote (tolerate a missing one)
      tokens.push({ text, quoted: true });
      continue;
    }
    if (ch === '\\' && i + 1 < s.length && /[*#@!A-Za-z]/.test(s[i + 1] as string)) {
      const name = s[i + 1] as string;
      tokens.push({ text: `\\${name}`, quoted: false });
      i += 2;
      continue;
    }
    let text = '';
    while (i < s.length && !/\s/.test(s[i] as string) && s[i] !== '"') {
      if (s[i] === '\\' && s[i + 1] === '\\') {
        text += '\\';
        i += 2;
        continue;
      }
      text += s[i];
      i++;
    }
    tokens.push({ text, quoted: false });
  }
  return tokens;
}

const isSwitch = (t: Token) => !t.quoted && /^\\[*#@!A-Za-z]$/.test(t.text);

function parseRange(v: string | undefined): { from: number; to: number } | undefined {
  if (!v) return undefined;
  const m = /^\s*(\d+)\s*-\s*(\d+)\s*$/.exec(v);
  if (m) return { from: Number(m[1]), to: Number(m[2]) };
  const single = /^\s*(\d+)\s*$/.exec(v);
  return single ? { from: Number(single[1]), to: Number(single[1]) } : undefined;
}

const DEFAULT_PICTURE: Record<string, string> = {
  DATE: 'M/d/yyyy',
  TIME: 'h:mm am/pm',
  CREATEDATE: 'M/d/yyyy h:mm:ss am/pm',
  SAVEDATE: 'M/d/yyyy h:mm:ss am/pm',
  PRINTDATE: 'M/d/yyyy h:mm:ss am/pm',
};

/** Parses a field instruction. Never throws; malformed input yields an `unknown` field. */
export function parseFieldInstruction(instr: string): FieldInstruction {
  const tokens = tokenizeField(instr);
  const first = tokens[0];
  const type = first && !first.quoted ? first.text.toUpperCase() : '';
  const flags = new Set([...(FLAG_SWITCHES[type] ?? []), ...GLOBAL_FLAGS]);
  const args: string[] = [];
  const switches: FieldSwitch[] = [];
  const general: string[] = [];
  let numericFormat: string | undefined;
  let dateFormat: string | undefined;
  for (let i = 1; i < tokens.length; i++) {
    const t = tokens[i] as Token;
    if (isSwitch(t)) {
      const name =
        t.text.length === 2 &&
        /[A-Z]/.test(t.text[1] as string) &&
        !['\\*', '\\#', '\\@', '\\!'].includes(t.text)
          ? t.text.toLowerCase()
          : t.text;
      const next = tokens[i + 1];
      const takesValue = !flags.has(name) && next !== undefined && !isSwitch(next);
      const sw: FieldSwitch = takesValue ? { name, value: (next as Token).text } : { name };
      if (takesValue) i++;
      switches.push(sw);
      if (name === '\\*' && sw.value) general.push(sw.value);
      if (name === '\\#' && sw.value !== undefined) numericFormat = sw.value;
      if (name === '\\@' && sw.value !== undefined) dateFormat = sw.value;
    } else {
      args.push(t.text);
    }
  }
  const base: FieldBase = { type, args, switches, general, raw: instr };
  if (numericFormat !== undefined) base.numericFormat = numericFormat;
  if (dateFormat !== undefined) base.dateFormat = dateFormat;
  const sw = (n: string) => switches.find((s) => s.name === n);
  const has = (n: string) => switches.some((s) => s.name === n);

  switch (type) {
    case 'PAGE':
    case 'NUMPAGES':
    case 'SECTIONPAGES':
    case 'SECTION':
      return { ...base, type };
    case 'DATE':
    case 'TIME':
    case 'CREATEDATE':
    case 'SAVEDATE':
    case 'PRINTDATE':
      return { ...base, type, picture: dateFormat ?? (DEFAULT_PICTURE[type] as string) };
    case 'TOC': {
      const toc: TocField = {
        ...base,
        type,
        styles: [],
        hyperlinks: has('\\h'),
        hideInWebLayout: has('\\z'),
        useOutlineLevels: has('\\u'),
      };
      const o = has('\\o') ? (parseRange(sw('\\o')?.value) ?? { from: 1, to: 9 }) : undefined;
      if (o) toc.outlineLevels = o;
      const t = sw('\\t')?.value;
      if (t) {
        const sep = t.includes(';') ? ';' : ',';
        const parts = t.split(sep).map((x) => x.trim());
        for (let k = 0; k < parts.length; k += 2) {
          const style = parts[k];
          if (style) toc.styles.push({ style, level: Number(parts[k + 1] ?? 1) || 1 });
        }
      }
      if (has('\\n')) toc.omitPageNumbers = parseRange(sw('\\n')?.value) ?? 'all';
      const c = sw('\\c')?.value;
      if (c !== undefined) toc.captionLabel = c;
      const b = sw('\\b')?.value;
      if (b) toc.bookmark = b;
      const p = sw('\\p')?.value;
      if (p !== undefined) toc.separator = p;
      return toc;
    }
    case 'REF':
    case 'PAGEREF':
    case 'NOTEREF': {
      const ref: RefField = {
        ...base,
        type,
        bookmark: args[0] ?? '',
        hyperlink: has('\\h'),
        relativePosition: has('\\p'),
      };
      if (type === 'REF') {
        if (has('\\w')) ref.paragraphNumber = 'fullContext';
        else if (has('\\r')) ref.paragraphNumber = 'relative';
        else if (has('\\n')) ref.paragraphNumber = 'noContext';
      }
      return ref;
    }
    case 'HYPERLINK': {
      const h: HyperlinkField = { ...base, type, newWindow: has('\\n') };
      if (args[0] !== undefined) h.url = args[0];
      const l = sw('\\l')?.value;
      if (l !== undefined) h.anchor = l;
      const o = sw('\\o')?.value;
      if (o !== undefined) h.tooltip = o;
      const t = sw('\\t')?.value;
      if (t !== undefined) h.target = t;
      return h;
    }
    case 'SEQ': {
      const seq: SeqField = {
        ...base,
        type,
        identifier: args[0] ?? '',
        repeat: has('\\c'),
        hidden: has('\\h'),
      };
      if (args[1] !== undefined) seq.bookmark = args[1];
      const r = sw('\\r')?.value;
      if (r !== undefined && Number.isFinite(Number(r))) seq.resetTo = Number(r);
      const s = sw('\\s')?.value;
      if (s !== undefined && Number.isFinite(Number(s))) seq.resetAtHeading = Number(s);
      return seq;
    }
    default:
      return { ...base, kind: 'unknown' };
  }
}

/** True when the parsed instruction is not one of the modelled field types. */
export const isUnknownField = (f: FieldInstruction): f is UnknownField => 'kind' in f && f.kind === 'unknown';

const needsQuotes = (v: string) => v === '' || /[\s"\\]/.test(v);
const quoteArg = (v: string) => (needsQuotes(v) ? `"${v.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"` : v);

/**
 * Serialises a field back to instruction text. Unchanged parses return `raw` exactly; this canonical
 * form is used for fields built in code (`buildField`).
 */
export function serializeFieldInstruction(field: Pick<FieldBase, 'type' | 'args' | 'switches'>): string {
  const parts = [field.type, ...field.args.map(quoteArg)];
  for (const s of field.switches) {
    parts.push(s.name);
    if (s.value !== undefined) parts.push(quoteArg(s.value));
  }
  return parts.join(' ');
}

/** Builds a field instruction from parts, e.g. `buildField('TOC', [], [['\\o','1-3'],['\\h']])`. */
export function buildField(
  type: string,
  args: string[] = [],
  switches: ([string] | [string, string])[] = [],
): string {
  return serializeFieldInstruction({
    type: type.toUpperCase(),
    args,
    switches: switches.map(([name, value]) => (value === undefined ? { name } : { name, value })),
  });
}

/** Word's default TOC instruction (References ▸ Table of Contents ▸ Automatic Table 1). */
export const DEFAULT_TOC_INSTRUCTION = 'TOC \\o "1-3" \\h \\z \\u';

// ------------------------------------------------------------------ result formatting

const MONTHS = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];
const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];

/**
 * Formats a date with a Word date-time picture (`\@`): d dd ddd dddd, M MM MMM MMMM, yy yyyy,
 * h hh H HH, m mm, s ss, am/pm AM/PM, and 'quoted literals'. English names.
 */
export function formatFieldDate(picture: string, date: Date): string {
  const hasAmPm = /am\/pm/i.test(picture);
  let out = '';
  let i = 0;
  const p = picture;
  const run = (ch: string) => {
    let n = 0;
    while (p[i + n] === ch) n++;
    return n;
  };
  while (i < p.length) {
    const ch = p[i] as string;
    if (ch === "'") {
      const end = p.indexOf("'", i + 1);
      out += end < 0 ? p.slice(i + 1) : p.slice(i + 1, end);
      i = end < 0 ? p.length : end + 1;
      continue;
    }
    if (/^am\/pm/i.test(p.slice(i))) {
      const pm = date.getHours() >= 12;
      const token = p.slice(i, i + 5);
      out +=
        token === 'AM/PM' ? (pm ? 'PM' : 'AM') : token === 'am/pm' ? (pm ? 'pm' : 'am') : pm ? 'PM' : 'AM';
      i += 5;
      continue;
    }
    const n = run(ch);
    switch (ch) {
      case 'd':
        out +=
          n === 1
            ? date.getDate()
            : n === 2
              ? String(date.getDate()).padStart(2, '0')
              : n === 3
                ? DAYS[date.getDay()]?.slice(0, 3)
                : DAYS[date.getDay()];
        break;
      case 'M':
        out +=
          n === 1
            ? date.getMonth() + 1
            : n === 2
              ? String(date.getMonth() + 1).padStart(2, '0')
              : n === 3
                ? MONTHS[date.getMonth()]?.slice(0, 3)
                : MONTHS[date.getMonth()];
        break;
      case 'y':
        out += n <= 2 ? String(date.getFullYear() % 100).padStart(2, '0') : String(date.getFullYear());
        break;
      case 'h': {
        const h = hasAmPm ? date.getHours() % 12 || 12 : date.getHours();
        out += n === 1 ? h : String(h).padStart(2, '0');
        break;
      }
      case 'H':
        out += n === 1 ? date.getHours() : String(date.getHours()).padStart(2, '0');
        break;
      case 'm':
        out += n === 1 ? date.getMinutes() : String(date.getMinutes()).padStart(2, '0');
        break;
      case 's':
        out += n === 1 ? date.getSeconds() : String(date.getSeconds()).padStart(2, '0');
        break;
      default:
        out += ch.repeat(n);
    }
    i += n;
  }
  return out;
}

const GENERAL_NUMBER_FORMATS: Record<string, NumberFormat> = {
  arabic: 'decimal',
  roman: 'lowerRoman',
  alphabetic: 'lowerLetter',
  ordinal: 'ordinal',
  cardtext: 'cardinalText',
  ordtext: 'ordinalText',
  hex: 'hex',
  arabicdash: 'numberInDash',
};

/**
 * Applies `\*` general formats to a numeric field result (`\* ROMAN` → `XIV`, `\* alphabetic` → `n`).
 * Case of the switch argument selects upper/lower case for roman and alphabetic as in Word.
 */
export function applyNumberSwitches(value: number, general: readonly string[]): string {
  let text = String(value);
  for (const g of general) {
    const key = g.toLowerCase();
    const fmt = GENERAL_NUMBER_FORMATS[key];
    if (fmt) {
      text = formatNumber(value, fmt);
      if ((key === 'roman' || key === 'alphabetic') && g === g.toUpperCase()) text = text.toUpperCase();
      if ((key === 'cardtext' || key === 'ordtext') && g !== g.toUpperCase()) text = text.toLowerCase();
    }
  }
  return applyTextSwitches(text, general);
}

/** Applies case `\*` switches (Upper, Lower, FirstCap, Caps); MERGEFORMAT/CHARFORMAT do not change text. */
export function applyTextSwitches(text: string, general: readonly string[]): string {
  let out = text;
  for (const g of general) {
    switch (g.toLowerCase()) {
      case 'upper':
        out = out.toUpperCase();
        break;
      case 'lower':
        out = out.toLowerCase();
        break;
      case 'firstcap':
        out = out.charAt(0).toUpperCase() + out.slice(1);
        break;
      case 'caps':
        out = out.replace(/(^|\s)(\S)/g, (_m, s: string, c: string) => s + c.toUpperCase());
        break;
      default:
        break;
    }
  }
  return out;
}

/** Context for computing simple field results. */
export interface FieldEvalContext {
  page?: number;
  numPages?: number;
  sectionPages?: number;
  section?: number;
  now?: Date;
  /** Bookmark text for REF fields. */
  bookmarkText?: (name: string) => string | undefined;
  /** Page of a bookmark for PAGEREF. */
  bookmarkPage?: (name: string) => number | undefined;
}

/**
 * Computes the display result of fields that can be evaluated locally (PAGE, NUMPAGES, SECTIONPAGES,
 * SECTION, DATE/TIME, REF, PAGEREF). Returns undefined for fields the layout/editor must compute
 * (TOC, SEQ, unknown) so their cached result stays.
 */
export function evaluateField(field: FieldInstruction, ctx: FieldEvalContext): string | undefined {
  switch (field.type) {
    case 'PAGE':
      return ctx.page === undefined ? undefined : applyNumberSwitches(ctx.page, field.general);
    case 'NUMPAGES':
      return ctx.numPages === undefined ? undefined : applyNumberSwitches(ctx.numPages, field.general);
    case 'SECTIONPAGES':
      return ctx.sectionPages === undefined
        ? undefined
        : applyNumberSwitches(ctx.sectionPages, field.general);
    case 'SECTION':
      return ctx.section === undefined ? undefined : applyNumberSwitches(ctx.section, field.general);
    case 'DATE':
    case 'TIME':
      return applyTextSwitches(
        formatFieldDate((field as DateField).picture, ctx.now ?? new Date()),
        field.general,
      );
    case 'REF': {
      const t = ctx.bookmarkText?.((field as RefField).bookmark);
      return t === undefined ? undefined : applyTextSwitches(t, field.general);
    }
    case 'PAGEREF': {
      const p = ctx.bookmarkPage?.((field as RefField).bookmark);
      return p === undefined ? undefined : applyNumberSwitches(p, field.general);
    }
    default:
      return undefined;
  }
}
