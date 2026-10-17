/**
 * Reads the shortcut tables of docs/SHORTCUTS.md, the human-readable specification of PLAN.md §1.7.
 * Used by the keymap conformance test, and available to tools that render the cheat sheet.
 */

export interface ShortcutDocRow {
  /** First column, verbatim (`Ctrl+N / O / S / P / W`). */
  shortcut: string;
  write: string;
  grid: string;
  deck: string;
}

export interface SlideShowDocRow {
  key: string;
  action: string;
}

export interface ShortcutDoc {
  rows: ShortcutDocRow[];
  slideShow: SlideShowDocRow[];
}

function cells(line: string): string[] {
  const inner = line.trim().replace(/^\|/, '').replace(/\|$/, '');
  return inner.split('|').map((c) => c.trim().replace(/^`(.*)`$/, '$1'));
}

/** Parses the main table (Shortcut | Kiln Write | Kiln Grid | Kiln Deck) and the "During a slide show" table. */
export function parseShortcutDoc(markdown: string): ShortcutDoc {
  const doc: ShortcutDoc = { rows: [], slideShow: [] };
  const lines = markdown.split(/\r?\n/);
  let mode: 'none' | 'main' | 'show' = 'none';
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? '';
    if (!line.trim().startsWith('|')) {
      mode = 'none';
      continue;
    }
    const cols = cells(line);
    if (mode === 'none') {
      const header = cols.map((c) => c.toLowerCase());
      if (header[0] === 'shortcut' && cols.length >= 4) mode = 'main';
      else if (header[0] === 'key' && cols.length >= 2) mode = 'show';
      else continue;
      i++; // skip the |---| separator
      continue;
    }
    if (mode === 'main') {
      doc.rows.push({
        shortcut: cols[0] ?? '',
        write: cols[1] ?? '',
        grid: cols[2] ?? '',
        deck: cols[3] ?? '',
      });
    } else {
      doc.slideShow.push({ key: cols[0] ?? '', action: cols[1] ?? '' });
    }
  }
  return doc;
}

/** `—` / `-` / empty: the shortcut does nothing in that app. */
export function isNoneCell(cell: string): boolean {
  return /^[—–-]?$/.test(cell.trim());
}

const MODIFIER_TOKENS = new Set(['ctrl', 'mod', 'alt', 'shift', 'cmd', 'meta']);
const ARROWS = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];

function splitPlus(s: string): string[] {
  const t = s.trim();
  if (t.endsWith('++')) return [...t.slice(0, -2).split('+'), '+'];
  return t.split('+');
}

function expandArrows(shortcut: string): string[] {
  return /(^|\+)Arrows$/i.test(shortcut) ? ARROWS.map((a) => shortcut.replace(/Arrows$/i, a)) : [shortcut];
}

/**
 * Expands a shortcut-column expression into individual shortcuts:
 * `Ctrl+N / O / S` → `Ctrl+N`, `Ctrl+O`, `Ctrl+S` (later parts inherit the modifiers),
 * `,` and ` or ` separate alternatives, `(also X)` adds X, `Arrows` expands to the four arrows,
 * `number, then Enter` → digits 0–9 and Enter. Other parentheticals and ★ are ignored.
 */
export function expandShortcutExpression(expr: string): string[] {
  let text = expr.replace(/★/g, ' ');
  const extra: string[] = [];
  text = text.replace(/\(([^)]*)\)/g, (_m, inner: string) => {
    const also = inner.match(/^\s*also\s+(.*)$/i);
    if (also?.[1]) extra.push(also[1]);
    return ' ';
  });
  const out: string[] = [];
  for (const group of [...text.split(/,\s*|\s+or\s+/i), ...extra]) {
    const g = group.trim().replace(/^then\s+/i, '');
    if (!g) continue;
    if (/^number$/i.test(g)) {
      for (let d = 0; d <= 9; d++) out.push(String(d));
      continue;
    }
    let mods: string[] = [];
    for (const part of g.split(/\s+\/\s+/)) {
      const tokens = splitPlus(part).map((t) => t.trim());
      const leadingMods = tokens.slice(0, -1);
      const key = tokens[tokens.length - 1] ?? '';
      if (leadingMods.length > 0 && leadingMods.every((m) => MODIFIER_TOKENS.has(m.toLowerCase()))) {
        mods = leadingMods;
        out.push(...expandArrows([...mods, key].join('+')));
      } else {
        out.push(...expandArrows([...mods, part.trim()].join('+')));
      }
    }
  }
  return [...new Set(out)];
}

/** Shortcuts mentioned in parentheses inside an app cell: `Go To (also F5)`, `(Shift+F2: note)`. */
export function cellShortcuts(cell: string): string[] {
  const out: string[] = [];
  for (const m of cell.matchAll(/\(([^)]*)\)/g)) {
    const inner =
      (m[1] ?? '')
        .replace(/^\s*also\s+/i, '')
        .split(':')[0]
        ?.trim() ?? '';
    if (/^(ctrl|alt|shift|f\d+)/i.test(inner)) out.push(...expandShortcutExpression(inner));
  }
  return out;
}

// ── Generating docs/SHORTCUTS.md ────────────────────────────────────────────────────────────────

/** What the generator needs from the keymaps (kept structural so the module stays dependency-free). */
export interface DocBinding {
  key: string;
  command: string;
  args?: unknown;
  scope?: string;
  row?: string;
}

export interface RenderShortcutsDocInput {
  rows: readonly ShortcutDocRow[];
  slideShow: readonly SlideShowDocRow[];
  /** Bindings per layer: `common` plus one list per app. */
  layers: Readonly<Record<'common' | 'write' | 'grid' | 'deck', readonly DocBinding[]>>;
  /** Display label for a binding's key (`Ctrl+Shift+L`). */
  label: (key: string) => string;
}

const APP_TITLES = [
  ['write', 'Kiln Write'],
  ['grid', 'Kiln Grid'],
  ['deck', 'Kiln Deck'],
] as const;

function argsLabel(args: unknown): string {
  if (args === undefined || args === null) return '';
  if (typeof args !== 'object') return ` ${JSON.stringify(args)}`;
  const parts = Object.entries(args as Record<string, unknown>).map(([k, v]) => `${k}: ${JSON.stringify(v)}`);
  return parts.length ? ` { ${parts.join(', ')} }` : '';
}

function escapeCell(text: string): string {
  return text.replace(/\|/g, '\\|');
}

/**
 * The text of docs/SHORTCUTS.md: the §1.7 table, the slide-show table, and, per app, every binding
 * with the catalog id it resolves to. Deterministic, so a test can compare it with the committed file.
 */
export function renderShortcutsDoc(input: RenderShortcutsDocInput): string {
  const lines: string[] = [
    '# Keyboard shortcuts',
    '',
    '<!-- Generated by `pnpm --filter @kiln/commands shortcuts:doc` from packages/commands/src/keymaps/. Do not edit by hand. -->',
    '',
    "Kiln keeps Microsoft Office's default shortcuts. On macOS, use ⌘ where the",
    'table says Ctrl. Shortcuts marked ★ are Kiln additions that Office does not',
    'have. A dash means the shortcut does nothing in that application.',
    '',
    'The table below is the specification (PLAN.md §1.7). The keymaps in',
    '`packages/commands/src/keymaps/` implement it, the same keymaps drive the',
    "ribbon's Key Tips and the desktop menus, and `e2e/specs/shortcuts.spec.ts` checks",
    'every row. In the application, **Ctrl+/** ★ shows the shortcuts for the current',
    'app, and **File ▸ Options ▸ Keyboard** lists and remaps them.',
    '',
    '| Shortcut | Kiln Write (documents) | Kiln Grid (spreadsheets) | Kiln Deck (presentations) |',
    '|---|---|---|---|',
    ...input.rows.map(
      (r) =>
        `| ${escapeCell(r.shortcut)} | ${escapeCell(r.write)} | ${escapeCell(r.grid)} | ${escapeCell(r.deck)} |`,
    ),
    '',
    '## During a slide show',
    '',
    '| Key | Action |',
    '|---|---|',
    ...input.slideShow.map((r) => `| ${escapeCell(r.key)} | ${escapeCell(r.action)} |`),
    '',
    '## Commands behind each shortcut',
    '',
    'Every binding names a command id. Ids in `packages/protocol/src/generated/catalog.json` run in the',
    'Rust core; `ui.*` ids are chrome actions (palette, ribbon, Key Tips); the others are reserved for',
    'commands the core does not implement yet and are hidden in release builds until it does. A scope',
    'limits a binding to a context (`cell-edit`: editing a cell; `object`: a shape is selected; `rail`:',
    'the slide thumbnails have focus; `slideshow`: a show is running).',
  ];
  for (const [app, title] of APP_TITLES) {
    lines.push('', `### ${title}`, '', '| Keys | Command | Scope |', '|---|---|---|');
    const bindings = [...input.layers.common, ...input.layers[app]];
    for (const b of bindings) {
      lines.push(
        `| ${escapeCell(input.label(b.key))} | \`${b.command}\`${escapeCell(argsLabel(b.args))} | ${b.scope ?? ''} |`,
      );
    }
  }
  lines.push('');
  return lines.join('\n');
}
