/**
 * Key Tips (PLAN.md §1.3): pressing Alt (or F10) shows letter badges on the File button and the
 * ribbon tabs; typing a tab's letters opens it and shows badges on its commands; typing a command's
 * letters runs it (Alt, H, 1 = Bold). Letters match Office where Office has them.
 *
 * This module is UI-free: `assignKeyTips` picks unique letters and `KeyTipSession` is the state
 * machine the ribbon's overlay drives.
 */
import type { AppId } from './types.ts';

/** Office's tab letters. */
export const TAB_KEYTIPS: Readonly<Record<AppId, Readonly<Record<string, string>>>> = {
  write: {
    file: 'F',
    home: 'H',
    insert: 'N',
    layout: 'P',
    references: 'S',
    review: 'R',
    view: 'W',
    help: 'Y2',
  },
  grid: {
    file: 'F',
    home: 'H',
    insert: 'N',
    pageLayout: 'P',
    formulas: 'M',
    data: 'A',
    review: 'R',
    view: 'W',
    help: 'Y2',
  },
  deck: {
    file: 'F',
    home: 'H',
    insert: 'N',
    design: 'G',
    transitions: 'K',
    animations: 'A',
    slideShow: 'S',
    review: 'R',
    view: 'W',
    help: 'Y2',
  },
  notes: { file: 'F', home: 'H', insert: 'N', draw: 'D', view: 'W', help: 'Y2' },
  hub: { file: 'F' },
};

/** Office's letters for contextual tabs (Table Design = JT, Layout = JL, Picture Format = JP…). */
export const CONTEXTUAL_KEYTIPS: Readonly<Record<string, string>> = {
  tableDesign: 'JT',
  tableLayout: 'JL',
  pictureFormat: 'JP',
  shapeFormat: 'JD',
  chartDesign: 'JC',
  chartFormat: 'JO',
  headerFooter: 'JH',
  videoFormat: 'JV',
  pivotTableAnalyze: 'JT',
};

/** Top-level tips outside the ribbon tabs (title bar). */
export const TITLE_BAR_KEYTIPS: Readonly<Record<string, string>> = {
  search: 'Q',
  share: 'ZS',
  undo: '1',
  redo: '2',
  save: '3',
};

export interface KeyTipCandidate {
  id: string;
  /** Label to derive letters from ("Format Painter" → F, P, FP). */
  label: string;
  /** Preferred tip (Office's letters). Used when not already taken. */
  preferred?: string;
}

const VALID_TIP = /^[A-Z0-9]{1,3}$/;

/** Normalises a tip: upper-case, alphanumerics only. Returns `undefined` when unusable. */
export function normalizeKeyTip(tip: string | undefined): string | undefined {
  if (!tip) return undefined;
  const t = tip.toUpperCase().replace(/[^A-Z0-9]/g, '');
  return VALID_TIP.test(t) ? t : undefined;
}

/**
 * Assigns unique tips to candidates. Preferred tips are honoured first (in order); the rest get,
 * in turn: the first letter of the label, the initials of its words, two-letter combinations from the
 * label, then digits and letter pairs. The result is prefix-free (no tip is a prefix of another),
 * so typing never becomes ambiguous.
 */
export function assignKeyTips(
  candidates: readonly KeyTipCandidate[],
  reserved: Iterable<string> = [],
): Map<string, string> {
  const taken = new Set<string>();
  for (const r of reserved) {
    const n = normalizeKeyTip(r);
    if (n) taken.add(n);
  }
  const conflicts = (tip: string) => {
    for (const t of taken) if (t === tip || t.startsWith(tip) || tip.startsWith(t)) return true;
    return false;
  };
  const result = new Map<string, string>();
  const pending: KeyTipCandidate[] = [];
  for (const c of candidates) {
    const p = normalizeKeyTip(c.preferred);
    if (p && !conflicts(p)) {
      taken.add(p);
      result.set(c.id, p);
    } else pending.push(c);
  }
  // Single letters are scarce: give them only when nothing in the pending set needs the same prefix.
  for (const c of pending) {
    const tip = candidatesFor(c.label).find((t) => !conflicts(t)) ?? fallbackTip(conflicts);
    taken.add(tip);
    result.set(c.id, tip);
  }
  return result;
}

function candidatesFor(label: string): string[] {
  const words = label
    .toUpperCase()
    .replace(/&/g, ' ')
    .split(/[^A-Z0-9]+/)
    .filter(Boolean);
  const letters = words.join('');
  const out: string[] = [];
  const push = (t: string | undefined) => {
    if (t && VALID_TIP.test(t) && !out.includes(t)) out.push(t);
  };
  if (words.length > 1)
    push(
      words
        .map((w) => w[0])
        .join('')
        .slice(0, 2),
    );
  push(letters[0]);
  for (const w of words) push(w[0]);
  if (letters[0]) for (const ch of letters.slice(1)) push(letters[0] + ch);
  for (const ch of letters) push(ch);
  return out;
}

function fallbackTip(conflicts: (t: string) => boolean): string {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
  for (const a of alphabet) for (const b of alphabet) if (!conflicts(a + b)) return a + b;
  for (const a of alphabet)
    for (const b of alphabet) for (const c of alphabet) if (!conflicts(a + b + c)) return a + b + c;
  throw new Error('Key Tips exhausted');
}

/** A node of the Key Tips tree: tabs at the top level, their commands below. */
export interface KeyTipNode {
  id: string;
  tip: string;
  label: string;
  /** `container` opens a level (a tab, a gallery, a menu); `action` runs and ends the session. */
  kind: 'container' | 'action';
  children?: readonly KeyTipNode[];
}

export type KeyTipResult =
  /** More letters needed: tips starting with `prefix` stay visible. */
  | { type: 'partial'; prefix: string }
  /** A container was chosen: its children are now the visible tips. */
  | { type: 'enter'; node: KeyTipNode }
  /** An action was chosen: the session is over. */
  | { type: 'invoke'; node: KeyTipNode }
  /** No tip starts with the typed letters: input ignored (Office beeps). */
  | { type: 'none' }
  /** Esc at the top level, or `cancel()`. */
  | { type: 'exit' };

/**
 * Key Tips state machine. `press` feeds typed characters; `back` handles Esc (up one level, or exit
 * at the top); Backspace removes the last typed letter.
 */
export class KeyTipSession {
  readonly #stack: KeyTipNode[][];
  #prefix = '';
  #active = true;

  constructor(root: readonly KeyTipNode[]) {
    this.#stack = [[...root]];
  }

  get active(): boolean {
    return this.#active;
  }

  /** Tips at the current level, filtered by what has been typed so far. */
  get visible(): readonly KeyTipNode[] {
    const level = this.#stack[this.#stack.length - 1] ?? [];
    return level.filter((n) => n.tip.startsWith(this.#prefix));
  }

  get prefix(): string {
    return this.#prefix;
  }

  get depth(): number {
    return this.#stack.length - 1;
  }

  /** Programmatic entry into a level, e.g. when the UI activated a tab by click. */
  enter(children: readonly KeyTipNode[]): void {
    this.#stack.push([...children]);
    this.#prefix = '';
  }

  press(char: string): KeyTipResult {
    if (!this.#active) return { type: 'exit' };
    const ch = char.toUpperCase();
    if (!/^[A-Z0-9]$/.test(ch)) return { type: 'none' };
    const next = this.#prefix + ch;
    const level = this.#stack[this.#stack.length - 1] ?? [];
    const matches = level.filter((n) => n.tip.startsWith(next));
    if (matches.length === 0) return { type: 'none' };
    const exact = matches.find((n) => n.tip === next);
    if (exact && matches.length === 1) {
      this.#prefix = '';
      if (exact.kind === 'container') {
        this.#stack.push([...(exact.children ?? [])]);
        return { type: 'enter', node: exact };
      }
      this.#active = false;
      return { type: 'invoke', node: exact };
    }
    this.#prefix = next;
    return { type: 'partial', prefix: next };
  }

  /** Backspace: drop the last typed letter. */
  erase(): void {
    this.#prefix = this.#prefix.slice(0, -1);
  }

  /** Esc: clear typed letters, else go up a level, else end the session. */
  back(): KeyTipResult {
    if (this.#prefix) {
      this.#prefix = '';
      return { type: 'partial', prefix: '' };
    }
    if (this.#stack.length > 1) {
      this.#stack.pop();
      return { type: 'partial', prefix: '' };
    }
    this.#active = false;
    return { type: 'exit' };
  }

  cancel(): void {
    this.#active = false;
  }
}
