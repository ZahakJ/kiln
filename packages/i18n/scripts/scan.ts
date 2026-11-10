/**
 * Source scanning for the interface strings: the hard-coded string guard and the message
 * extractor share it. Files are parsed with oxc (through rolldown, the parser Vite already uses).
 *
 * The guard reports user-facing English in TSX that does not go through `t`: JSX text, string
 * values of user-facing attributes (`aria-label`, `title`, `placeholder`, `label`…), strings shown
 * as JSX children and user-facing properties of object literals (`label: 'Bold'`). Mark a string
 * that is not interface text (a sample, a font name, a format code) with an `i18n-ignore` comment on
 * its line or the line before; `i18n-ignore-file` at the top of a file skips it (developer pages).
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { parseSync } from 'rolldown/utils';

/** JSX attributes whose string value is read or seen by people. */
export const UI_ATTRS: ReadonlySet<string> = new Set([
  'aria-label',
  'aria-description',
  'aria-roledescription',
  'aria-valuetext',
  'aria-placeholder',
  'title',
  'placeholder',
  'alt',
  'label',
  'description',
  'hint',
  'heading',
  'tooltip',
  'content',
  'caption',
  'message',
  'emptyText',
  'emptyLabel',
  'subtitle',
  'legend',
  'confirmLabel',
  'cancelLabel',
  'okLabel',
  'actionLabel',
  'groupLabel',
  'ariaLabel',
  'announce',
  'prefix',
  'suffix',
  'status',
  'detail',
  'note',
  'intro',
]);

/** Object-literal properties whose string value is shown to people. */
export const UI_PROPS: ReadonlySet<string> = new Set([
  'label',
  'title',
  'description',
  'tooltip',
  'placeholder',
  'hint',
  'heading',
  'message',
  'ariaLabel',
  'caption',
  'emptyText',
  'announce',
  'subtitle',
  'detail',
  'confirmLabel',
  'cancelLabel',
  'actionLabel',
  'textValue',
  'content',
  'note',
  'notice',
  'intro',
]);

/** Names that hold interface text by their suffix (`noneLabel`, `emptyTitle`, `searchPlaceholder`). */
const UI_SUFFIX =
  /^[a-z][A-Za-z]*(Label|Title|Hint|Tooltip|Placeholder|Description|Message|Caption|Heading)$/;

/** Whether a JSX attribute's string value is interface text. */
export function isUiAttr(name: string): boolean {
  return UI_ATTRS.has(name) || UI_SUFFIX.test(name);
}

/** Whether an object property's string value is interface text. */
export function isUiProp(name: string): boolean {
  return UI_PROPS.has(name) || UI_SUFFIX.test(name);
}

/** Calls whose string arguments are shown or read to people (status lines, announcements). */
export const MESSAGE_SINKS =
  /^(announce|setMessage|setError|setProblem|setStatus|setDone|setNotice|setHint|setWarning|setNote|setInfo|setFeedback|alert|confirm|prompt|notify|fail)$/;

/** Calls whose first string argument is a message key. */
export const MESSAGE_CALLS: ReadonlySet<string> = new Set(['t', 'msg', 'tOr']);

export type FindingKind = 'jsx-text' | 'attr' | 'child' | 'prop' | 'call';

export interface Finding {
  file: string;
  line: number;
  column: number;
  kind: FindingKind;
  /** The English text as shown (JSX whitespace collapsed). */
  text: string;
  /** Offsets of the literal (or JSX text) in the source, UTF-16. */
  start: number;
  end: number;
  /** For `attr`/`prop`: the attribute or property name. */
  name?: string;
  /** Whether the literal is at module level (outside every function). */
  moduleLevel: boolean;
}

export interface MessageUse {
  file: string;
  line: number;
  key: string;
  /** `tOr(key, fallback)`: the English fallback. */
  fallback?: string;
}

// biome-ignore lint/suspicious/noExplicitAny: ESTree nodes are walked generically.
type Node = any;

const LETTER = /\p{L}/u;

/** JSX's whitespace rule: lines trimmed, lines joined with one space, blank lines dropped. */
export function jsxText(raw: string): string {
  const lines = raw.split(/\r\n|\n|\r/);
  if (lines.length === 1) return raw;
  return lines
    .map((l, i) => {
      let s = l;
      if (i > 0) s = s.replace(/^[ \t]+/, '');
      if (i < lines.length - 1) s = s.replace(/[ \t]+$/, '');
      return s;
    })
    .filter((s) => s.length > 0)
    .join(' ');
}

const ENTITIES: Readonly<Record<string, string>> = {
  nbsp: '\u00a0',
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
};

/** JSX text with its HTML entities decoded (`&nbsp;`, `&amp;`, `&#8211;`). */
export function decodeEntities(s: string): string {
  return s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e: string) => {
    if (e[0] === '#') {
      const code = e[1] === 'x' || e[1] === 'X' ? Number.parseInt(e.slice(2), 16) : Number(e.slice(1));
      return Number.isFinite(code) ? String.fromCodePoint(code) : m;
    }
    return ENTITIES[e.toLowerCase()] ?? m;
  });
}

function isTextual(s: string): boolean {
  const v = s.trim();
  return v.length > 1 && LETTER.test(v);
}

function lineStarts(src: string): number[] {
  const out = [0];
  for (let i = 0; i < src.length; i++) if (src.charCodeAt(i) === 10) out.push(i + 1);
  return out;
}

function lineOf(starts: number[], offset: number): number {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if ((starts[mid] ?? 0) <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo + 1;
}

function calleeName(call: Node): string | undefined {
  const c = call.callee;
  if (c?.type === 'Identifier') return c.name;
  if (c?.type === 'MemberExpression' && c.property?.type === 'Identifier') return c.property.name;
  return undefined;
}

function attrName(attr: Node): string {
  const n = attr.name;
  return n.type === 'JSXNamespacedName' ? `${n.namespace.name}:${n.name.name}` : n.name;
}

function propName(prop: Node): string | undefined {
  if (prop.computed) return undefined;
  const k = prop.key;
  if (k?.type === 'Identifier') return k.name;
  if (k?.type === 'Literal' && typeof k.value === 'string') return k.value;
  return undefined;
}

/** String-valued leaves of an expression that is shown as it is (through ?:, ||, ??, &&). */
function shownStrings(e: Node, out: Node[]): void {
  if (!e) return;
  switch (e.type) {
    case 'Literal':
      if (typeof e.value === 'string') out.push(e);
      return;
    case 'TemplateLiteral':
      out.push(e);
      return;
    case 'ConditionalExpression':
      shownStrings(e.consequent, out);
      shownStrings(e.alternate, out);
      return;
    case 'LogicalExpression':
      shownStrings(e.right, out);
      if (e.operator !== '&&') shownStrings(e.left, out);
      return;
    case 'ParenthesizedExpression':
    case 'TSAsExpression':
    case 'TSSatisfiesExpression':
    case 'TSNonNullExpression':
      shownStrings(e.expression, out);
      return;
    case 'BinaryExpression':
      if (e.operator === '+') {
        shownStrings(e.left, out);
        shownStrings(e.right, out);
      }
      return;
    default:
      return;
  }
}

function literalText(n: Node): string {
  if (n.type === 'Literal') return String(n.value);
  // Template literal: the static parts, expressions as {…}.
  return n.quasis.map((q: Node) => q.value.cooked ?? q.value.raw).join('{…}');
}

export interface ParsedFile {
  program: Node;
  comments: { value: string; start: number; end: number }[];
  starts: number[];
}

export function parse(file: string, src: string): ParsedFile {
  const r = parseSync(file, src);
  if (r.errors?.length) {
    const e = r.errors[0];
    throw new Error(`${file}: ${e?.message ?? 'parse error'}`);
  }
  return { program: r.program, comments: r.comments as ParsedFile['comments'], starts: lineStarts(src) };
}

function ignoredLines(p: ParsedFile): { lines: Set<number>; file: boolean } {
  const lines = new Set<number>();
  let file = false;
  for (const c of p.comments) {
    if (c.value.includes('i18n-ignore-file')) file = true;
    else if (c.value.includes('i18n-ignore')) {
      const l = lineOf(p.starts, c.start);
      const end = lineOf(p.starts, c.end);
      lines.add(l);
      lines.add(end);
      lines.add(end + 1);
    }
  }
  return { lines, file };
}

/** TypeScript nodes that hold runtime expressions (the rest are types). */
const TS_EXPRESSIONS = new Set([
  'TSAsExpression',
  'TSSatisfiesExpression',
  'TSNonNullExpression',
  'TSInstantiationExpression',
  'TSTypeAssertion',
  'TSExportAssignment',
]);

const FUNCTION_TYPES = new Set([
  'FunctionDeclaration',
  'FunctionExpression',
  'ArrowFunctionExpression',
  'MethodDefinition',
  'ClassBody',
]);

/**
 * Hard-coded interface strings of a source file. `jsxOnly` limits the object-property rule to
 * objects inside JSX-bearing files (TSX), the guard's scope.
 */
export function scanSource(file: string, src: string, options: { props?: boolean } = {}): Finding[] {
  const p = parse(file, src);
  const ignore = ignoredLines(p);
  if (ignore.file) return [];
  const findings: Finding[] = [];
  const add = (kind: FindingKind, node: Node, text: string, depth: number, name?: string) => {
    if (!isTextual(text)) return;
    const line = lineOf(p.starts, node.start);
    if (ignore.lines.has(line)) return;
    findings.push({
      file,
      line,
      column: node.start - (p.starts[line - 1] ?? 0) + 1,
      kind,
      text: text.trim(),
      start: node.start,
      end: node.end,
      moduleLevel: depth === 0,
      ...(name ? { name } : {}),
    });
  };
  const props = options.props ?? true;

  const walk = (n: Node, depth: number, parent: Node | undefined): void => {
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) {
      for (const c of n) walk(c, depth, parent);
      return;
    }
    if (typeof n.type !== 'string') return;
    // Arguments of t()/msg()/tOr() are messages already; type annotations are never shown.
    if (n.type === 'CallExpression' && MESSAGE_CALLS.has(calleeName(n) ?? '')) {
      walk(n.callee, depth, n);
      walk(n.arguments.slice(calleeName(n) === 'tOr' ? 2 : 1), depth, n);
      return;
    }
    if (n.type === 'CallExpression' && MESSAGE_SINKS.test(calleeName(n) ?? '')) {
      const leaves: Node[] = [];
      for (const a of n.arguments) shownStrings(a, leaves);
      for (const leaf of leaves) add('call', leaf, literalText(leaf), depth, calleeName(n));
    }
    if (n.type.startsWith('TS') && !TS_EXPRESSIONS.has(n.type)) return;
    if (n.type === 'ImportDeclaration' || n.type === 'ExportAllDeclaration') return;
    const d = FUNCTION_TYPES.has(n.type) ? depth + 1 : depth;
    if (n.type === 'JSXText') {
      add('jsx-text', n, decodeEntities(jsxText(n.value)), d);
      return;
    }
    if (n.type === 'JSXAttribute') {
      const name = attrName(n);
      if (isUiAttr(name) && n.value) {
        const leaves: Node[] = [];
        if (n.value.type === 'Literal') leaves.push(n.value);
        else if (n.value.type === 'JSXExpressionContainer') shownStrings(n.value.expression, leaves);
        for (const leaf of leaves) add('attr', leaf, literalText(leaf), d, name);
      }
      walk(n.value, d, n);
      return;
    }
    if (
      n.type === 'JSXExpressionContainer' &&
      (parent?.type === 'JSXElement' || parent?.type === 'JSXFragment')
    ) {
      const leaves: Node[] = [];
      shownStrings(n.expression, leaves);
      for (const leaf of leaves) add('child', leaf, literalText(leaf), d);
    }
    if (props && n.type === 'Property' && !n.method) {
      const name = propName(n);
      if (name && isUiProp(name)) {
        const leaves: Node[] = [];
        shownStrings(n.value, leaves);
        for (const leaf of leaves) add('prop', leaf, literalText(leaf), d, name);
      }
    }
    for (const k of Object.keys(n)) {
      if (k === 'type' || k === 'start' || k === 'end' || k === 'range' || k === 'loc') continue;
      const v = n[k];
      if (v && typeof v === 'object') walk(v, d, n);
    }
  };
  walk(p.program, 0, undefined);
  return findings;
}

/** Every `t('…')`, `tOr('…', '…')` and `msg('…')` with a literal key. */
export function collectMessages(file: string, src: string): MessageUse[] {
  const p = parse(file, src);
  const out: MessageUse[] = [];
  if (ignoredLines(p).file) return out;
  const str = (a: Node): string | undefined => {
    if (a?.type === 'Literal' && typeof a.value === 'string') return a.value;
    if (a?.type === 'TemplateLiteral' && a.expressions.length === 0) return a.quasis[0].value.cooked;
    return undefined;
  };
  const walk = (n: Node): void => {
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) {
      for (const c of n) walk(c);
      return;
    }
    // <Message id="…" values={…} />: a message with markup in its placeholders.
    if (n.type === 'JSXOpeningElement' && n.name?.type === 'JSXIdentifier' && n.name.name === 'Message') {
      const id = n.attributes.find((a: Node) => a.type === 'JSXAttribute' && a.name?.name === 'id');
      const key = id?.value?.type === 'Literal' ? id.value.value : undefined;
      if (typeof key === 'string') out.push({ file, line: lineOf(p.starts, n.start), key });
    }
    if (n.type === 'CallExpression') {
      const name = calleeName(n);
      if (name && MESSAGE_CALLS.has(name)) {
        const key = str(n.arguments[0]);
        if (key !== undefined) {
          const fallback = name === 'tOr' ? str(n.arguments[1]) : undefined;
          out.push({
            file,
            line: lineOf(p.starts, n.start),
            key,
            ...(fallback !== undefined ? { fallback } : {}),
          });
        }
      }
    }
    for (const k of Object.keys(n)) {
      if (k === 'type' || k === 'start' || k === 'end') continue;
      const v = n[k];
      if (v && typeof v === 'object') walk(v);
    }
  };
  walk(p.program);
  return out;
}

/** The workspace root (this file is packages/i18n/scripts/scan.ts). */
export const ROOT = join(import.meta.dirname, '..', '..', '..');

/** Source trees whose interface strings go through @kiln/i18n. */
export const UI_SOURCES: readonly string[] = [
  'apps/web/src',
  'packages/ui-kit/src',
  'packages/home/src',
  'packages/editor-write/src',
  'packages/editor-grid/src',
  'packages/editor-deck/src',
  'packages/commands/src',
  'packages/surface/src',
];

/** Files outside the interface: tests, stories, generated code, the core's wasm glue. */
function excluded(rel: string): boolean {
  return (
    /\.(test|spec|stories)\.tsx?$/.test(rel) ||
    rel.includes('/generated/') ||
    rel.includes('/wasm/') ||
    rel.includes('/__tests__/') ||
    rel.endsWith('test-setup.ts') ||
    rel.endsWith('test-matchers.ts') ||
    rel.endsWith('.d.ts')
  );
}

/** Source files of the interface (`.ts` and `.tsx`, or only `.tsx`). */
export function sourceFiles(ext: readonly string[] = ['.ts', '.tsx']): string[] {
  const out: string[] = [];
  const visit = (dir: string) => {
    for (const name of readdirSync(dir)) {
      if (name === 'node_modules' || name === 'dist') continue;
      const path = join(dir, name);
      if (statSync(path).isDirectory()) visit(path);
      else if (ext.some((e) => name.endsWith(e))) {
        const rel = relative(ROOT, path);
        if (!excluded(rel)) out.push(rel);
      }
    }
  };
  for (const s of UI_SOURCES) visit(join(ROOT, s));
  return out.sort();
}

export function read(rel: string): string {
  return readFileSync(join(ROOT, rel), 'utf8');
}
