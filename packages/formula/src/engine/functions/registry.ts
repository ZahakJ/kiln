/**
 * The function registry. WP-07 registers a native core set; WP-09 fills it with the full library.
 */
import type { Node } from '../../parser/ast.ts';
import type { EvalValue, Scope } from '../../values/types.ts';
import type { CallContext } from '../context.ts';
import { CORE_FUNCTIONS } from './core.ts';

export type FunctionCategory =
  | 'Math & Trig'
  | 'Statistical'
  | 'Logical'
  | 'Text'
  | 'Date & Time'
  | 'Lookup & Reference'
  | 'Information'
  | 'Financial'
  | 'Engineering'
  | 'Database'
  | 'Web'
  | 'Compatibility'
  | 'Dynamic Array';

export interface FunctionArgDoc {
  name: string;
  description: string;
  optional?: boolean;
  repeating?: boolean;
}

/** Documentation shown by autocomplete, the Insert Function dialog and the Formula Lab. */
export interface FunctionDocs {
  category: FunctionCategory;
  summary: string;
  syntax: string;
  args: FunctionArgDoc[];
  examples?: { formula: string; result: string }[];
  since?: string;
}

/** An argument passed unevaluated (see `FunctionDescriptor.lazy`). `force()` evaluates it once. */
export class LazyArg {
  private cached: EvalValue | undefined;
  constructor(
    readonly node: Node,
    private readonly ctx: CallContext,
    /** Lexical scope at the call site. */
    readonly scope: Scope | null,
  ) {}

  force(): EvalValue {
    if (this.cached === undefined) this.cached = this.ctx.evaluate(this.node, this.scope);
    return this.cached;
  }

  get isEmpty(): boolean {
    return this.node.type === 'empty';
  }
}

export type Arg = EvalValue | LazyArg;

export interface FunctionDescriptor {
  /** Upper-case name, e.g. `XLOOKUP` or `STDEV.S`. */
  name: string;
  minArgs: number;
  /** Use `Infinity` for variadic functions (Excel caps at 255). */
  maxArgs: number;
  /** Argument indices passed as {@link LazyArg}s (IF branches, LET, LAMBDA…). */
  lazy?: readonly number[];
  /**
   * Argument indices that take single values. When an array or multi-cell range is passed in one of
   * these positions the function is applied element-wise and returns an array (Excel's array
   * lifting). Lifted arguments always reach `impl` as scalars (or MISSING). `'all'` lifts every
   * argument.
   */
  lift?: readonly number[] | 'all';
  /** Recalculated on every recalc (NOW, RAND, OFFSET, INDIRECT…). */
  volatile?: boolean;
  impl: (args: Arg[], ctx: CallContext) => EvalValue;
  docs?: FunctionDocs;
}

const registry = new Map<string, FunctionDescriptor>();
let coreLoaded = false;

/** Registers the engine's native functions on first use (explicit registrations win). */
function ensureCore(): void {
  if (coreLoaded) return;
  coreLoaded = true;
  for (const d of CORE_FUNCTIONS) if (!registry.has(d.name)) registry.set(d.name, d);
}

const PREFIXES = ['_XLFN._XLWS.', '_XLFN.', '_XLWS.', '_XLETA.'];

/** Strips the file-format prefixes Excel writes for newer functions (`_xlfn.XLOOKUP`). */
export function normalizeFunctionName(name: string): string {
  let n = name.toUpperCase();
  for (const p of PREFIXES) {
    if (n.startsWith(p)) {
      n = n.slice(p.length);
      break;
    }
  }
  return n;
}

export function registerFunction(desc: FunctionDescriptor): void {
  ensureCore();
  registry.set(desc.name.toUpperCase(), desc);
}

export function registerFunctions(descs: readonly FunctionDescriptor[]): void {
  for (const d of descs) registerFunction(d);
}

export function unregisterFunction(name: string): void {
  ensureCore();
  registry.delete(name.toUpperCase());
}

export function getFunction(name: string): FunctionDescriptor | undefined {
  ensureCore();
  return registry.get(normalizeFunctionName(name));
}

/** Every registered function, sorted by name. */
export function listFunctions(): FunctionDescriptor[] {
  ensureCore();
  return [...registry.values()].sort((a, b) => a.name.localeCompare(b.name));
}
