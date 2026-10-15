/**
 * The function library (WP-09): ≥ 350 Excel functions on top of the engine's native core set, each
 * with documentation from `docs/<FAMILY>.json` (used by autocomplete, the Insert Function dialog and
 * the Formula Lab).
 */
import { CORE_FUNCTIONS } from '../engine/functions/core.ts';
import type { FunctionDescriptor, FunctionDocs } from '../engine/functions/registry.ts';
import { conditionalFunctions } from './conditional.ts';
import { databaseFunctions, webFunctions } from './database.ts';
import { dateTimeFunctions } from './datetime.ts';
import { distributionFunctions } from './distributions.ts';
import COMPATIBILITY from './docs/COMPATIBILITY.json' with { type: 'json' };
import DATABASE from './docs/DATABASE.json' with { type: 'json' };
import DATETIME from './docs/DATETIME.json' with { type: 'json' };
import ENGINEERING from './docs/ENGINEERING.json' with { type: 'json' };
import FINANCIAL from './docs/FINANCIAL.json' with { type: 'json' };
import INFORMATION from './docs/INFORMATION.json' with { type: 'json' };
import LOGICAL from './docs/LOGICAL.json' with { type: 'json' };
import LOOKUP from './docs/LOOKUP.json' with { type: 'json' };
import MATH from './docs/MATH.json' with { type: 'json' };
import STATISTICAL from './docs/STATISTICAL.json' with { type: 'json' };
import TEXT from './docs/TEXT.json' with { type: 'json' };
import WEB from './docs/WEB.json' with { type: 'json' };
import { engineeringFunctions } from './engineering.ts';
import { financialFunctions } from './financial.ts';
import { informationFunctions, logicalFunctions } from './logical.ts';
import { mathFunctions } from './math.ts';
import { referenceFunctions } from './reference.ts';
import { statisticalFunctions } from './stats.ts';
import { textFunctions } from './text.ts';

/** One entry of a `docs/<FAMILY>.json` file. */
export interface FunctionDocEntry extends FunctionDocs {
  name: string;
}

/** Documentation files by family, in the order the Insert Function dialog lists them. */
export const DOC_FAMILIES: Readonly<Record<string, readonly FunctionDocEntry[]>> = {
  FINANCIAL: FINANCIAL as FunctionDocEntry[],
  LOGICAL: LOGICAL as FunctionDocEntry[],
  TEXT: TEXT as FunctionDocEntry[],
  DATETIME: DATETIME as FunctionDocEntry[],
  LOOKUP: LOOKUP as FunctionDocEntry[],
  MATH: MATH as FunctionDocEntry[],
  STATISTICAL: STATISTICAL as FunctionDocEntry[],
  ENGINEERING: ENGINEERING as FunctionDocEntry[],
  INFORMATION: INFORMATION as FunctionDocEntry[],
  DATABASE: DATABASE as FunctionDocEntry[],
  WEB: WEB as FunctionDocEntry[],
  COMPATIBILITY: COMPATIBILITY as FunctionDocEntry[],
};

const DOCS = new Map<string, FunctionDocEntry>();
for (const entries of Object.values(DOC_FAMILIES)) for (const e of entries) DOCS.set(e.name, e);

/** Documentation for a function name, if any. */
export function functionDoc(name: string): FunctionDocEntry | undefined {
  return DOCS.get(name.toUpperCase());
}

/** Every documentation entry. */
export function listFunctionDocs(): FunctionDocEntry[] {
  return [...DOCS.values()];
}

function withDocs(d: FunctionDescriptor): FunctionDescriptor {
  const e = DOCS.get(d.name);
  if (!e) return d;
  const { name: _name, ...docs } = e;
  return { ...d, docs };
}

let cached: FunctionDescriptor[] | undefined;

/** The library's descriptors, plus documented copies of the engine's core functions. */
export function LIBRARY_FUNCTIONS(): FunctionDescriptor[] {
  if (cached) return cached;
  const library = [
    ...mathFunctions,
    ...conditionalFunctions,
    ...statisticalFunctions,
    ...distributionFunctions,
    ...logicalFunctions,
    ...informationFunctions,
    ...textFunctions,
    ...dateTimeFunctions,
    ...referenceFunctions,
    ...financialFunctions,
    ...engineeringFunctions,
    ...databaseFunctions,
    ...webFunctions,
  ];
  const names = new Set(library.map((d) => d.name));
  const core = CORE_FUNCTIONS.filter((d) => !names.has(d.name));
  cached = [...core, ...library].map(withDocs);
  return cached;
}

export { parseCriterion } from './criteria.ts';
export { formatComplex, parseComplex } from './engineering.ts';
export { formatNumber } from './text.ts';
