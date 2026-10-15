/**
 * List numbering (`numbering.xml`): abstract numbering definitions (`w:abstractNum`) with nine levels,
 * numbering instances (`w:num`) with level overrides, and the label computation Word performs when it
 * lays out a document.
 *
 * Counting model (matching Word, see MS-OI29500 §2.1.299 and §2.1.302):
 * - counters belong to the **abstract** definition, so two `w:num` instances of the same abstractNum
 *   continue one sequence ("Continue numbering");
 * - a `w:num` with `w:startOverride` restarts the levels it overrides the first time it is used
 *   ("Restart at 1" creates such an instance);
 * - incrementing level *n* resets deeper levels, unless a deeper level's `w:lvlRestart` says
 *   otherwise (`0` = never restart, *k* = restart only after level *k* (1-based) or a shallower one);
 * - `%k` placeholders of levels not yet used in the sequence render that level's start value;
 * - `w:isLgl` renders every referenced level in decimal (legal numbering, "1.1.1" under roman parts).
 */
import type { ParagraphProps, RunProps } from '../types.ts';
import { bulletDisplayText, formatNumber, type NumberFormat } from './formats.ts';

export interface NumberingLevel {
  /** Level index 0–8. */
  level: number;
  start: number;
  format: NumberFormat;
  /** Custom format string for `format="custom"` (`w:numFmt/@w:format`, e.g. `001, 002, …`). */
  customFormat?: string;
  /** Level text template, e.g. `%1.%2.` — or the literal bullet character. */
  text: string;
  /** `w:lvlRestart`: undefined = restart after any shallower level; 0 = never; k = after level k. */
  restartAfter?: number;
  /** Legal numbering (`w:isLgl`). */
  isLegal?: boolean;
  /** What follows the label: a tab (default), a space or nothing (`w:suff`). */
  suffix?: 'tab' | 'space' | 'nothing';
  /** Label justification within the indent (`w:lvlJc`). */
  alignment?: 'left' | 'center' | 'right';
  /** Paragraph style linked to this level (`w:pStyle`). */
  pStyle?: string;
  /** Picture bullet id (`w:lvlPicBulletId`), preserved. */
  picBulletId?: number;
  pPr?: ParagraphProps;
  /** Formatting of the label itself. */
  rPr?: RunProps;
}

export interface AbstractNumbering {
  id: number;
  /** `w:nsid` (hex), used by Word to recognise the same list when copying between documents. */
  nsid?: string;
  multiLevelType?: 'singleLevel' | 'multilevel' | 'hybridMultilevel';
  name?: string;
  /** This definition *is* the list style named here (`w:styleLink`). */
  styleLink?: string;
  /** This definition defers to the numbering of the list style named here (`w:numStyleLink`). */
  numStyleLink?: string;
  levels: NumberingLevel[];
}

export interface LevelOverride {
  level: number;
  startOverride?: number;
  /** A full replacement level definition (`w:lvlOverride/w:lvl`). */
  definition?: NumberingLevel;
}

export interface NumberingInstance {
  id: number;
  abstractId: number;
  overrides: LevelOverride[];
}

export interface NumberingJSON {
  abstracts: AbstractNumbering[];
  instances: NumberingInstance[];
}

/** Resolves a list style's numbering instance (`w:styleLink`/`w:numStyleLink`). */
export type ListStyleResolver = (styleId: string) => number | undefined;

export class NumberingError extends Error {
  override name = 'NumberingError';
}

export class Numbering {
  readonly abstracts = new Map<number, AbstractNumbering>();
  readonly instances = new Map<number, NumberingInstance>();
  /** Optional resolver for `numStyleLink` (list style → numId), supplied by the style sheet. */
  listStyleResolver: ListStyleResolver | undefined;

  constructor(json?: NumberingJSON) {
    for (const a of json?.abstracts ?? []) this.abstracts.set(a.id, a);
    for (const n of json?.instances ?? []) this.instances.set(n.id, n);
  }

  addAbstract(def: Omit<AbstractNumbering, 'id'> & { id?: number }): AbstractNumbering {
    const id = def.id ?? this.#nextId(this.abstracts);
    const a: AbstractNumbering = { ...def, id, levels: normaliseLevels(def.levels) };
    this.abstracts.set(id, a);
    return a;
  }

  addInstance(abstractId: number, overrides: LevelOverride[] = [], id?: number): NumberingInstance {
    if (!this.abstracts.has(abstractId)) throw new NumberingError(`unknown abstractNum ${abstractId}`);
    const n: NumberingInstance = { id: id ?? this.#nextId(this.instances, 1), abstractId, overrides };
    this.instances.set(n.id, n);
    return n;
  }

  /**
   * The abstract definition actually used by `numId`, following `numStyleLink` indirection through
   * list styles (at most a few hops; cycles stop).
   */
  abstractFor(numId: number): AbstractNumbering | undefined {
    const inst = this.instances.get(numId);
    let abs = inst ? this.abstracts.get(inst.abstractId) : undefined;
    const seen = new Set<number>();
    while (abs?.numStyleLink && this.listStyleResolver && !seen.has(abs.id)) {
      seen.add(abs.id);
      const linkedNum = this.listStyleResolver(abs.numStyleLink);
      const linked = linkedNum !== undefined ? this.instances.get(linkedNum) : undefined;
      const next = linked ? this.abstracts.get(linked.abstractId) : undefined;
      if (!next) break;
      abs = next;
    }
    return abs;
  }

  /** Effective level definition for `numId` at `level`, after `w:lvlOverride/w:lvl`. */
  level(numId: number, level: number): NumberingLevel | undefined {
    const abs = this.abstractFor(numId);
    if (!abs) return undefined;
    const lvl = clampLevel(level);
    const override = this.instances.get(numId)?.overrides.find((o) => o.level === lvl);
    const base = override?.definition ?? abs.levels[lvl];
    return base;
  }

  /** All nine effective levels for `numId`. */
  levels(numId: number): NumberingLevel[] {
    const out: NumberingLevel[] = [];
    for (let i = 0; i < 9; i++) {
      const l = this.level(numId, i);
      if (l) out.push(l);
    }
    return out;
  }

  /** The level whose `w:pStyle` is `styleId` (Word's style-linked outline numbering). */
  levelForStyle(numId: number, styleId: string): number | undefined {
    return this.levels(numId).find((l) => l.pStyle === styleId)?.level;
  }

  /** Creates a new instance that restarts `numId`'s list at its level starts ("Restart at 1"). */
  restart(numId: number, level = 0, startAt?: number): NumberingInstance {
    const inst = this.instances.get(numId);
    if (!inst) throw new NumberingError(`unknown numId ${numId}`);
    const lvl = this.level(numId, level);
    return this.addInstance(inst.abstractId, [{ level, startOverride: startAt ?? lvl?.start ?? 1 }]);
  }

  /** Creates a new single-sequence bulleted list (Word's default bullet library: •, o, ▪). */
  createBulletList(): NumberingInstance {
    const glyphs: [string, string][] = [
      ['', 'Symbol'],
      ['o', 'Courier New'],
      ['', 'Wingdings'],
    ];
    const levels: NumberingLevel[] = Array.from({ length: 9 }, (_, i) => {
      const [text, font] = glyphs[i % 3] as [string, string];
      return {
        level: i,
        start: 1,
        format: 'bullet',
        text,
        alignment: 'left',
        pPr: { indent: { left: 720 * (i + 1), hanging: 360 } },
        rPr: { fonts: { ascii: font, hAnsi: font, ...(font === 'Courier New' ? { cs: font } : {}) } },
      };
    });
    const abs = this.addAbstract({ multiLevelType: 'hybridMultilevel', levels, nsid: this.#nsid() });
    return this.addInstance(abs.id);
  }

  /** Creates a new numbered list (1. a. i. 1. a. i. … like Word's default numbering library). */
  createNumberedList(): NumberingInstance {
    const fmts: [NumberFormat, string, 'left' | 'right'][] = [
      ['decimal', '.', 'left'],
      ['lowerLetter', '.', 'left'],
      ['lowerRoman', '.', 'right'],
    ];
    const levels: NumberingLevel[] = Array.from({ length: 9 }, (_, i) => {
      const [format, sep, alignment] = fmts[i % 3] as [NumberFormat, string, 'left' | 'right'];
      return {
        level: i,
        start: 1,
        format,
        text: `%${i + 1}${sep}`,
        alignment,
        pPr: { indent: { left: 720 * (i + 1), hanging: alignment === 'right' ? 180 : 360 } },
      };
    });
    const abs = this.addAbstract({ multiLevelType: 'hybridMultilevel', levels, nsid: this.#nsid() });
    return this.addInstance(abs.id);
  }

  /** Creates Word's "1. / 1.1. / 1.1.1." multilevel list. */
  createMultilevelList(options: { legal?: boolean } = {}): NumberingInstance {
    const levels: NumberingLevel[] = Array.from({ length: 9 }, (_, i) => ({
      level: i,
      start: 1,
      format: 'decimal',
      text: `${Array.from({ length: i + 1 }, (_x, k) => `%${k + 1}`).join('.')}.`,
      alignment: 'left',
      ...(options.legal ? { isLegal: true } : {}),
      pPr: { indent: { left: 360 + i * 432, hanging: 360 + i * 72 } },
    }));
    const abs = this.addAbstract({ multiLevelType: 'multilevel', levels, nsid: this.#nsid() });
    return this.addInstance(abs.id);
  }

  clone(): Numbering {
    const n = new Numbering(structuredClone(this.toJSON()));
    n.listStyleResolver = this.listStyleResolver;
    return n;
  }

  toJSON(): NumberingJSON {
    return {
      abstracts: [...this.abstracts.values()].sort((a, b) => a.id - b.id),
      instances: [...this.instances.values()].sort((a, b) => a.id - b.id),
    };
  }

  static fromJSON(json: NumberingJSON): Numbering {
    return new Numbering(json);
  }

  #nextId(map: Map<number, unknown>, min = 0): number {
    let max = min - 1;
    for (const k of map.keys()) if (k > max) max = k;
    return max + 1;
  }

  #nsid(): string {
    // Deterministic per definition count so tests and documents diff stably.
    return (0x1a2b3c4d + this.abstracts.size * 0x9e3779b1)
      .toString(16)
      .slice(-8)
      .toUpperCase()
      .padStart(8, '0');
  }
}

export const clampLevel = (level: number): number => Math.max(0, Math.min(8, Math.trunc(level) || 0));

function normaliseLevels(levels: NumberingLevel[]): NumberingLevel[] {
  return [...levels].sort((a, b) => a.level - b.level);
}

/**
 * Renders a level's label text for the given counters (index = level, value = current count). Levels
 * without a counter use their start value.
 */
export function formatLevelText(
  levels: readonly (NumberingLevel | undefined)[],
  level: number,
  counters: readonly (number | undefined)[],
): string {
  const lvl = levels[level];
  if (!lvl) return '';
  if (lvl.format === 'none' && !/%[1-9]/.test(lvl.text)) return lvl.text;
  if (lvl.format === 'bullet') return lvl.text;
  return lvl.text.replace(/%([1-9])/g, (_m, d: string) => {
    const k = Number(d) - 1;
    const ref = levels[k];
    if (!ref) return '';
    const value = counters[k] ?? ref.start;
    if (ref.format === 'none') return '';
    let fmt: NumberFormat = ref.format;
    // Legal numbering: every level shown in Arabic numerals (the bullet/none special cases aside).
    if (lvl.isLegal && fmt !== 'decimalZero') fmt = 'decimal';
    if (fmt === 'bullet') fmt = 'decimal';
    if (fmt === 'custom' && ref.customFormat) return formatCustom(value, ref.customFormat);
    return formatNumber(value, fmt);
  });
}

/** `w:numFmt w:val="custom" w:format="001, 002, 003, …"`: zero-padded decimal by the first token. */
function formatCustom(value: number, pattern: string): string {
  const first = pattern.split(',')[0]?.trim() ?? '';
  const digits = /^\d+$/.test(first) ? first.length : 1;
  return String(value).padStart(digits, '0');
}

/** A computed list label for one paragraph. */
export interface ListLabel {
  /** Text of the label as displayed (bullets mapped to Unicode). */
  text: string;
  /** Raw label text before bullet glyph mapping. */
  rawText: string;
  numId: number;
  abstractId: number;
  level: number;
  /** Counter values of levels 0..level at this paragraph. */
  counters: number[];
  isBullet: boolean;
  suffix: 'tab' | 'space' | 'nothing';
  alignment: 'left' | 'center' | 'right';
  /** The label's own run properties from the level definition. */
  rPr?: RunProps;
}

/**
 * Stateful list counter used while walking paragraphs in document order. `next(numId, level)`
 * returns the label of the next paragraph in that list.
 */
export class ListCounter {
  readonly #numbering: Numbering;
  readonly #counters = new Map<number, (number | undefined)[]>();
  readonly #seenInstances = new Set<number>();

  constructor(numbering: Numbering) {
    this.#numbering = numbering;
  }

  next(numId: number, level: number): ListLabel | undefined {
    if (!numId) return undefined;
    const abs = this.#numbering.abstractFor(numId);
    const inst = this.#numbering.instances.get(numId);
    if (!abs || !inst) return undefined;
    const lvl = clampLevel(level);
    const levels = Array.from({ length: 9 }, (_, i) => this.#numbering.level(numId, i));
    const def = levels[lvl];
    if (!def) return undefined;

    let counters = this.#counters.get(abs.id);
    if (!counters) {
      counters = new Array(9).fill(undefined);
      this.#counters.set(abs.id, counters);
    }

    // First use of an instance with start overrides restarts those levels.
    if (!this.#seenInstances.has(numId)) {
      this.#seenInstances.add(numId);
      for (const o of inst.overrides) {
        if (o.startOverride !== undefined || o.definition) counters[clampLevel(o.level)] = undefined;
      }
    }

    const startOf = (i: number) => {
      const o = inst.overrides.find((x) => x.level === i);
      return o?.startOverride ?? levels[i]?.start ?? 1;
    };

    const current = counters[lvl];
    counters[lvl] = current === undefined ? startOf(lvl) : current + 1;

    // Reset deeper levels according to their restart rule.
    for (let d = lvl + 1; d < 9; d++) {
      const rule = levels[d]?.restartAfter;
      const restart = rule === undefined ? true : rule === 0 ? false : lvl < rule;
      if (restart) counters[d] = undefined;
    }

    const effective = levels.map((l, i) => (counters[i] === undefined && l ? startOf(i) : counters[i]));
    const rawText = formatLevelText(levels, lvl, effective);
    const font = def.rPr?.fonts?.ascii ?? def.rPr?.fonts?.hAnsi;
    const isBullet = def.format === 'bullet';
    const label: ListLabel = {
      text: isBullet ? bulletDisplayText(rawText, font) : rawText,
      rawText,
      numId,
      abstractId: abs.id,
      level: lvl,
      counters: effective.slice(0, lvl + 1).map((c) => c ?? 0),
      isBullet,
      suffix: def.suffix ?? 'tab',
      alignment: def.alignment ?? 'left',
    };
    if (def.rPr) label.rPr = def.rPr;
    return label;
  }
}
