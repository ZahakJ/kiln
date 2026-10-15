/**
 * AI-ready hooks (PLAN.md §2.1, ★ P1). Apps call an `AssistantProvider` for four tasks: summarise,
 * rewrite, formula from text and slides from an outline. Officina ships no network provider and makes
 * no network calls by default: the default is `NullProvider` ("no assistant configured"), and
 * `EchoProvider` is a deterministic local sample that shows the plumbing end to end.
 */

export type AssistantCapability = 'summarise' | 'rewrite' | 'formulaFromText' | 'slideFromOutline';

export const ASSISTANT_CAPABILITIES: readonly AssistantCapability[] = [
  'summarise',
  'rewrite',
  'formulaFromText',
  'slideFromOutline',
];

export interface SummariseRequest {
  text: string;
  /** Upper bound on sentences in the summary. Default 3. */
  maxSentences?: number;
}

export type RewriteTone = 'formal' | 'casual' | 'concise' | 'friendly';

export interface RewriteRequest {
  text: string;
  tone?: RewriteTone;
}

export interface FormulaRequest {
  /** "sum of B2:B20", "average sales" */
  prompt: string;
  /** Column headers by letter, so prompts can refer to names: `{ B: 'Sales' }`. */
  headers?: Readonly<Record<string, string>>;
  /** Rows of data under the headers (for name references). Default 2..100. */
  dataRows?: { first: number; last: number };
}

export interface FormulaResult {
  /** Starts with `=`. */
  formula: string;
  explanation: string;
}

export interface OutlineSlide {
  title: string;
  bullets: string[];
  /** Nested bullets keep their level (0 = top). */
  levels: number[];
}

export interface SlidesResult {
  slides: OutlineSlide[];
}

export interface TextResult {
  text: string;
}

export interface AssistantRequestOptions {
  signal?: AbortSignal;
}

export interface AssistantProvider {
  readonly id: string;
  /** Shown in Options and in the palette ("Assistant: Local echo (sample)"). */
  readonly name: string;
  /** False when nothing is configured: the UI shows how to set one up instead of the commands. */
  readonly available: boolean;
  /** True when nothing leaves the device. */
  readonly local: boolean;
  readonly capabilities: readonly AssistantCapability[];
  summarise(req: SummariseRequest, options?: AssistantRequestOptions): Promise<TextResult>;
  rewrite(req: RewriteRequest, options?: AssistantRequestOptions): Promise<TextResult>;
  formulaFromText(req: FormulaRequest, options?: AssistantRequestOptions): Promise<FormulaResult>;
  slideFromOutline(req: { outline: string }, options?: AssistantRequestOptions): Promise<SlidesResult>;
}

export class AssistantUnavailableError extends Error {
  override name = 'AssistantUnavailableError';
  constructor(message = 'No assistant is configured. Choose one in File ▸ Options ▸ General.') {
    super(message);
  }
}

export class AssistantRequestError extends Error {
  override name = 'AssistantRequestError';
}

/** The default: no provider. Every request rejects with `AssistantUnavailableError`. */
export class NullProvider implements AssistantProvider {
  readonly id = 'none';
  readonly name = 'No assistant';
  readonly available = false;
  readonly local = true;
  readonly capabilities: readonly AssistantCapability[] = [];
  summarise(): Promise<TextResult> {
    return Promise.reject(new AssistantUnavailableError());
  }
  rewrite(): Promise<TextResult> {
    return Promise.reject(new AssistantUnavailableError());
  }
  formulaFromText(): Promise<FormulaResult> {
    return Promise.reject(new AssistantUnavailableError());
  }
  slideFromOutline(): Promise<SlidesResult> {
    return Promise.reject(new AssistantUnavailableError());
  }
}

function checkAbort(signal?: AbortSignal): void {
  if (signal?.aborted) throw signal.reason ?? new DOMException('Aborted', 'AbortError');
}

/** Splits text into sentences (keeps terminal punctuation). */
export function splitSentences(text: string): string[] {
  const clean = text.replace(/\s+/g, ' ').trim();
  if (!clean) return [];
  return (clean.match(/[^.!?]+(?:[.!?]+["')\]]*|$)/g) ?? [clean]).map((s) => s.trim()).filter(Boolean);
}

const FILLER = /\b(really|very|just|actually|basically|quite|simply|literally|totally|in order)\b\s*/gi;
const CONTRACTIONS: ReadonlyArray<[RegExp, string]> = [
  [/\bdo not\b/gi, "don't"],
  [/\bcannot\b/gi, "can't"],
  [/\bit is\b/gi, "it's"],
  [/\bwe are\b/gi, "we're"],
  [/\bwill not\b/gi, "won't"],
  [/\bI am\b/g, "I'm"],
];
const EXPANSIONS: ReadonlyArray<[RegExp, string]> = [
  [/\bdon't\b/gi, 'do not'],
  [/\bcan't\b/gi, 'cannot'],
  [/\bwon't\b/gi, 'will not'],
  [/\bit's\b/gi, 'it is'],
  [/\bwe're\b/gi, 'we are'],
  [/\bI'm\b/g, 'I am'],
  [/\bisn't\b/gi, 'is not'],
  [/\baren't\b/gi, 'are not'],
];

const FUNCTION_WORDS: ReadonlyArray<[RegExp, string, string]> = [
  [/\b(sum|total|add up)\b/i, 'SUM', 'Adds the numbers in'],
  [/\b(average|mean)\b/i, 'AVERAGE', 'Averages the numbers in'],
  [/\b(count)\b/i, 'COUNT', 'Counts the numbers in'],
  [/\b(max|maximum|largest|highest|biggest)\b/i, 'MAX', 'Finds the largest number in'],
  [/\b(min|minimum|smallest|lowest)\b/i, 'MIN', 'Finds the smallest number in'],
  [/\b(median)\b/i, 'MEDIAN', 'Finds the median of'],
];

/**
 * A local, deterministic sample provider. It does real (if simple) work so the UI can be exercised
 * without any service: extractive summaries, rule-based rewrites, pattern-based formulas and
 * outline-to-slides parsing.
 */
export class EchoProvider implements AssistantProvider {
  readonly id = 'echo';
  readonly name = 'Local echo (sample)';
  readonly available = true;
  readonly local = true;
  readonly capabilities = ASSISTANT_CAPABILITIES;

  async summarise(req: SummariseRequest, options: AssistantRequestOptions = {}): Promise<TextResult> {
    checkAbort(options.signal);
    const sentences = splitSentences(req.text);
    const max = Math.max(1, req.maxSentences ?? 3);
    return { text: sentences.slice(0, max).join(' ') };
  }

  async rewrite(req: RewriteRequest, options: AssistantRequestOptions = {}): Promise<TextResult> {
    checkAbort(options.signal);
    let text = req.text.replace(/[ \t]+/g, ' ').trim();
    switch (req.tone ?? 'concise') {
      case 'concise':
        text = text.replace(FILLER, '').replace(/\s{2,}/g, ' ');
        break;
      case 'formal':
        for (const [re, rep] of EXPANSIONS) text = text.replace(re, rep);
        break;
      case 'casual':
      case 'friendly':
        for (const [re, rep] of CONTRACTIONS) text = text.replace(re, rep);
        break;
    }
    // Re-capitalise sentence starts that filler removal may have exposed.
    text = text.replace(/(^|[.!?]\s+)([a-z])/g, (_m, p: string, ch: string) => p + ch.toUpperCase());
    return { text };
  }

  async formulaFromText(req: FormulaRequest, options: AssistantRequestOptions = {}): Promise<FormulaResult> {
    checkAbort(options.signal);
    const fn = FUNCTION_WORDS.find(([re]) => re.test(req.prompt));
    if (!fn) {
      throw new AssistantRequestError(
        'The sample assistant understands sum, average, count, max, min and median of a range or column.',
      );
    }
    const range = findRange(req);
    if (!range) {
      throw new AssistantRequestError('Name a range (B2:B20), a column (column B) or a header (Sales).');
    }
    return { formula: `=${fn[1]}(${range.ref})`, explanation: `${fn[2]} ${range.description}.` };
  }

  async slideFromOutline(
    req: { outline: string },
    options: AssistantRequestOptions = {},
  ): Promise<SlidesResult> {
    checkAbort(options.signal);
    return { slides: parseOutline(req.outline) };
  }
}

function findRange(req: FormulaRequest): { ref: string; description: string } | null {
  const explicit = req.prompt.match(/\b(\$?[A-Z]{1,3}\$?\d+)\s*(?::|to|-)\s*(\$?[A-Z]{1,3}\$?\d+)\b/i);
  if (explicit) {
    const ref = `${explicit[1]}:${explicit[2]}`.toUpperCase();
    return { ref, description: ref };
  }
  const rows = req.dataRows ?? { first: 2, last: 100 };
  const col = req.prompt.match(/\bcolumn\s+([A-Z]{1,3})\b/i);
  if (col?.[1]) {
    const c = col[1].toUpperCase();
    return { ref: `${c}${rows.first}:${c}${rows.last}`, description: `column ${c}` };
  }
  for (const [letter, header] of Object.entries(req.headers ?? {})) {
    if (header && new RegExp(`\\b${escapeRe(header)}\\b`, 'i').test(req.prompt)) {
      const c = letter.toUpperCase();
      return { ref: `${c}${rows.first}:${c}${rows.last}`, description: `the ${header} column (${c})` };
    }
  }
  return null;
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Parses a Markdown-ish outline into slides: `#`/`##` headings or unindented lines start a slide;
 * indented lines or `-`/`*`/`1.` items become bullets (indent depth = level).
 */
export function parseOutline(outline: string): OutlineSlide[] {
  const slides: OutlineSlide[] = [];
  let current: OutlineSlide | null = null;
  for (const raw of outline.replace(/\t/g, '  ').split(/\r?\n/)) {
    if (!raw.trim()) continue;
    const indent = raw.match(/^ */)?.[0].length ?? 0;
    const line = raw.trim();
    const heading = line.match(/^#{1,6}\s+(.*)$/);
    const bullet = line.match(/^(?:[-*•]|\d+[.)])\s+(.*)$/);
    if (heading || (!bullet && indent === 0)) {
      current = { title: (heading?.[1] ?? line).trim(), bullets: [], levels: [] };
      slides.push(current);
      continue;
    }
    if (!current) {
      current = { title: 'Untitled', bullets: [], levels: [] };
      slides.push(current);
    }
    current.bullets.push((bullet?.[1] ?? line).trim());
    current.levels.push(Math.max(0, Math.floor(indent / 2) - (bullet ? 0 : 1)));
  }
  return slides;
}

let activeProvider: AssistantProvider = new NullProvider();
const providerListeners = new Set<(p: AssistantProvider) => void>();

/** The process-wide provider (NullProvider until the user configures one). */
export function getAssistantProvider(): AssistantProvider {
  return activeProvider;
}

export function setAssistantProvider(provider: AssistantProvider): void {
  activeProvider = provider;
  for (const l of providerListeners) l(provider);
}

export function onAssistantProviderChange(listener: (p: AssistantProvider) => void): () => void {
  providerListeners.add(listener);
  return () => providerListeners.delete(listener);
}
