/**
 * Fuzzy command search for the palette (PLAN.md §1.9): matches titles, Office names and synonyms,
 * tolerates missing letters ("frz pn" → Freeze Panes), ranks word-start and prefix matches first.
 */
import type { CommandMeta } from './types.ts';

const normalize = (s: string) =>
  s
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/&/g, ' and ')
    .replace(/[^\p{L}\p{N}$%Σσ¶ ]+/gu, ' ')
    .replace(/\s+/g, ' ')
    .trim();

/**
 * Scores how well `query` matches `text`, 0..1 (1 = equal), or 0 for no match. Every query
 * character must appear in order (a subsequence); contiguous runs and word starts score higher.
 */
export function fuzzyScore(query: string, text: string): number {
  const q = normalize(query);
  const t = normalize(text);
  if (!q) return 1;
  if (!t) return 0;
  if (t === q) return 1;
  if (t.startsWith(q)) return 0.95 - Math.min(0.1, (t.length - q.length) / 200);
  // Every query word is a prefix of some text word, in any order: "panes freeze".
  const qWords = q.split(' ');
  const tWords = t.split(' ');
  if (qWords.every((w) => tWords.some((tw) => tw.startsWith(w)))) {
    return 0.85 - Math.min(0.1, (t.length - q.length) / 300);
  }
  const idx = t.indexOf(q);
  if (idx >= 0) return 0.75 - Math.min(0.15, idx / 100);

  // Subsequence match with bonuses.
  const qs = q.replace(/ /g, '');
  let ti = 0;
  let score = 0;
  let run = 0;
  let firstMatch = -1;
  for (const ch of qs) {
    let found = -1;
    for (let j = ti; j < t.length; j++) {
      if (t[j] === ch) {
        found = j;
        break;
      }
    }
    if (found < 0) return 0;
    if (firstMatch < 0) firstMatch = found;
    const wordStart = found === 0 || t[found - 1] === ' ';
    run = found === ti && ti > 0 ? run + 1 : 0;
    score += 1 + (wordStart ? 2 : 0) + run * 0.5;
    ti = found + 1;
  }
  const max = qs.length * 3.5;
  const density = qs.length / Math.max(qs.length, ti - firstMatch);
  // Subsequence matches stay below substring matches.
  return Math.min(0.6, 0.6 * (0.7 * (score / max) + 0.3 * density));
}

export interface SearchableCommand
  extends Pick<CommandMeta, 'id' | 'title' | 'officeName' | 'synonyms' | 'category'> {}

export interface CommandMatch<T extends SearchableCommand = SearchableCommand> {
  command: T;
  score: number;
  /** What matched best: the title, the Office name or a synonym (shown as a hint). */
  matchedOn: 'title' | 'officeName' | 'synonym';
  matchedText: string;
}

/** Ranks commands for a query. Empty query returns every command (score 1) in input order. */
export function searchCommands<T extends SearchableCommand>(
  commands: readonly T[],
  query: string,
  options: {
    limit?: number;
    minScore?: number;
    /** Multiplier per command (e.g. rank not-yet-available commands lower). Default 1. */
    weight?: (command: T) => number;
  } = {},
): CommandMatch<T>[] {
  const minScore = options.minScore ?? 0.28;
  const out: CommandMatch<T>[] = [];
  for (const command of commands) {
    if (!query.trim()) {
      out.push({ command, score: 1, matchedOn: 'title', matchedText: command.title });
      continue;
    }
    let best: CommandMatch<T> = {
      command,
      score: fuzzyScore(query, command.title),
      matchedOn: 'title',
      matchedText: command.title,
    };
    if (command.officeName) {
      const s = fuzzyScore(query, command.officeName) * 0.98;
      if (s > best.score)
        best = { command, score: s, matchedOn: 'officeName', matchedText: command.officeName };
    }
    for (const syn of command.synonyms ?? []) {
      const s = fuzzyScore(query, syn) * 0.9;
      if (s > best.score) best = { command, score: s, matchedOn: 'synonym', matchedText: syn };
    }
    if (best.score >= minScore) {
      if (options.weight) best.score *= options.weight(command);
      out.push(best);
    }
  }
  if (query.trim()) {
    out.sort(
      (a, b) =>
        b.score - a.score ||
        a.command.title.length - b.command.title.length ||
        a.command.title.localeCompare(b.command.title),
    );
  }
  return options.limit ? out.slice(0, options.limit) : out;
}
