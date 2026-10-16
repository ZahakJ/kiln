/**
 * Formula text as stored in a file ↔ formula text in the model.
 *
 * SpreadsheetML writes functions newer than Excel 2007 with a `_xlfn.` prefix (`_xlfn.XLOOKUP`), the
 * dynamic-array sort/filter functions with `_xlfn._xlws.` and LET/LAMBDA parameter names with
 * `_xlpm.`. Excel shows `#NAME?` when the prefixes are missing, so the reader strips them (the model
 * holds what the user types) and the writer puts them back. Text inside string literals, quoted sheet
 * names and brackets is never touched.
 */

/** Functions Excel stores with the `_xlfn.` prefix ("future functions", MS-XLSX §2.2.2). */
export const FUTURE_FUNCTIONS: ReadonlySet<string> = new Set([
  'ACOT', 'ACOTH', 'AGGREGATE', 'ANCHORARRAY', 'ARABIC', 'ARRAYTOTEXT', 'BASE', 'BETA.DIST', 'BETA.INV',
  'BINOM.DIST', 'BINOM.DIST.RANGE', 'BINOM.INV', 'BITAND', 'BITLSHIFT', 'BITOR', 'BITRSHIFT', 'BITXOR',
  'BYCOL', 'BYROW', 'CEILING.MATH', 'CEILING.PRECISE', 'CHISQ.DIST', 'CHISQ.DIST.RT', 'CHISQ.INV',
  'CHISQ.INV.RT', 'CHISQ.TEST', 'CHOOSECOLS', 'CHOOSEROWS', 'COMBINA', 'CONCAT', 'CONFIDENCE.NORM',
  'CONFIDENCE.T', 'COT', 'COTH', 'COVARIANCE.P', 'COVARIANCE.S', 'CSC', 'CSCH', 'DAYS', 'DECIMAL',
  'DROP', 'ECMA.CEILING', 'ENCODEURL', 'ERF.PRECISE', 'ERFC.PRECISE', 'EXPAND', 'EXPON.DIST', 'F.DIST',
  'F.DIST.RT', 'F.INV', 'F.INV.RT', 'F.TEST', 'FIELDVALUE', 'FILTERXML', 'FLOOR.MATH', 'FLOOR.PRECISE',
  'FORECAST.ETS', 'FORECAST.ETS.CONFINT', 'FORECAST.ETS.SEASONALITY', 'FORECAST.ETS.STAT',
  'FORECAST.LINEAR', 'FORMULATEXT', 'GAMMA', 'GAMMA.DIST', 'GAMMA.INV', 'GAMMALN.PRECISE', 'GAUSS',
  'GROUPBY', 'HSTACK', 'HYPGEOM.DIST', 'IFNA', 'IFS', 'IMAGE', 'IMCOSH', 'IMCOT', 'IMCSC', 'IMCSCH',
  'IMSEC', 'IMSECH', 'IMSINH', 'IMTAN', 'ISFORMULA', 'ISO.CEILING', 'ISOMITTED', 'ISOWEEKNUM', 'LAMBDA',
  'LET', 'LOGNORM.DIST', 'LOGNORM.INV', 'MAKEARRAY', 'MAP', 'MAXIFS', 'MINIFS', 'MODE.MULT', 'MODE.SNGL',
  'MUNIT', 'NEGBINOM.DIST', 'NETWORKDAYS.INTL', 'NORM.DIST', 'NORM.INV', 'NORM.S.DIST', 'NORM.S.INV',
  'NUMBERVALUE', 'PDURATION', 'PERCENTILE.EXC', 'PERCENTILE.INC', 'PERCENTOF', 'PERCENTRANK.EXC',
  'PERCENTRANK.INC', 'PERMUTATIONA', 'PHI', 'PIVOTBY', 'POISSON.DIST', 'QUARTILE.EXC', 'QUARTILE.INC',
  'QUERYSTRING', 'RANDARRAY', 'RANK.AVG', 'RANK.EQ', 'REDUCE', 'REGEXEXTRACT', 'REGEXREPLACE',
  'REGEXTEST', 'RRI', 'SCAN', 'SEC', 'SECH', 'SEQUENCE', 'SHEET', 'SHEETS', 'SINGLE', 'SKEW.P', 'SORTBY',
  'STDEV.P', 'STDEV.S', 'STOCKHISTORY', 'SWITCH', 'T.DIST', 'T.DIST.2T', 'T.DIST.RT', 'T.INV', 'T.INV.2T',
  'T.TEST', 'TAKE', 'TEXTAFTER', 'TEXTBEFORE', 'TEXTJOIN', 'TEXTSPLIT', 'TOCOL', 'TOROW', 'TRIMRANGE',
  'UNICHAR', 'UNICODE', 'UNIQUE', 'VALUETOTEXT', 'VAR.P', 'VAR.S', 'VSTACK', 'WEBSERVICE',
  'WEIBULL.DIST', 'WORKDAY.INTL', 'WRAPCOLS', 'WRAPROWS', 'XLOOKUP', 'XMATCH', 'XOR', 'Z.TEST',
]);

/** Functions stored as `_xlfn._xlws.NAME`. */
export const XLWS_FUNCTIONS: ReadonlySet<string> = new Set(['FILTER', 'SORT']);

interface Tok {
  start: number;
  end: number;
  /** Identifier text (undefined for everything else). */
  ident?: string;
  call?: boolean;
  /** `(` / `)` / `,` / `;` (argument separator inside an array constant is ignored). */
  punct?: string;
}

function isIdentStart(c: number): boolean {
  return (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c === 95 || c === 92 || c > 127;
}

function isIdentChar(c: number): boolean {
  return isIdentStart(c) || (c >= 48 && c <= 57) || c === 46 || c === 63;
}

/** Splits formula text into identifiers and structural punctuation; literals are skipped over. */
function scan(text: string): Tok[] {
  const toks: Tok[] = [];
  const n = text.length;
  let i = 0;
  let braces = 0;
  while (i < n) {
    const c = text.charCodeAt(i);
    if (c === 34) {
      // "string" with "" escapes
      i++;
      while (i < n) {
        if (text.charCodeAt(i) === 34) {
          if (text.charCodeAt(i + 1) === 34) i += 2;
          else break;
        } else i++;
      }
      i++;
      continue;
    }
    if (c === 39) {
      // 'quoted sheet' with '' escapes
      i++;
      while (i < n) {
        if (text.charCodeAt(i) === 39) {
          if (text.charCodeAt(i + 1) === 39) i += 2;
          else break;
        } else i++;
      }
      i++;
      continue;
    }
    if (c === 91) {
      // [bracketed] structured reference or external book index, possibly nested
      let depth = 0;
      while (i < n) {
        const d = text.charCodeAt(i);
        if (d === 39 && text.charCodeAt(i + 1) !== 0) {
          // an escape character inside structured references
          i += 2;
          continue;
        }
        if (d === 91) depth++;
        else if (d === 93) {
          depth--;
          if (depth === 0) {
            i++;
            break;
          }
        }
        i++;
      }
      continue;
    }
    if (c === 123) braces++;
    else if (c === 125) braces--;
    if (c === 40 || c === 41 || ((c === 44 || c === 59) && braces === 0)) {
      toks.push({ start: i, end: i + 1, punct: text[i] as string });
      i++;
      continue;
    }
    if (isIdentStart(c)) {
      const start = i;
      while (i < n && isIdentChar(text.charCodeAt(i))) i++;
      // An identifier directly followed by `!` is a sheet name; by `:`/digit forms are refs — both are
      // harmless here because only calls and LET/LAMBDA names are rewritten.
      let j = i;
      while (j < n && (text.charCodeAt(j) === 32 || text.charCodeAt(j) === 10 || text.charCodeAt(j) === 13)) j++;
      const call = j < n && text.charCodeAt(j) === 40;
      const prev = start > 0 ? text.charCodeAt(start - 1) : 0;
      // Skip identifiers that are part of a number (1E+5) or follow `!`/`#` in a reference.
      if (prev >= 48 && prev <= 57) continue;
      toks.push({ start, end: i, ident: text.slice(start, i), call });
      continue;
    }
    i++;
  }
  return toks;
}

const PREFIX_RE = /^(?:_xlfn\._xlws\.|_xlfn\.|_xlws\.|_xlpm\.)/i;

/**
 * File formula → model formula: strips `_xlfn.`, `_xlfn._xlws.` and `_xlpm.` prefixes. Names of
 * prefixed functions are added to `seen` (so the writer can restore prefixes it does not know).
 */
export function fromFileFormula(text: string, seen?: Set<string>): string {
  if (text.indexOf('_x') < 0 && text.indexOf('_X') < 0) return text;
  const toks = scan(text);
  let out = '';
  let last = 0;
  for (const t of toks) {
    if (!t.ident) continue;
    const m = PREFIX_RE.exec(t.ident);
    if (!m) continue;
    const bare = t.ident.slice(m[0].length);
    if (t.call && seen && !/^_xlpm/i.test(m[0])) seen.add(bare.toUpperCase());
    out += text.slice(last, t.start) + bare;
    last = t.end;
  }
  return last === 0 ? text : out + text.slice(last);
}

/**
 * Model formula → file formula: adds `_xlfn.` / `_xlfn._xlws.` to future functions and `_xlpm.` to
 * LET/LAMBDA parameter names. `extra` lists more function names to prefix (seen when reading).
 */
export function toFileFormula(text: string, extra?: ReadonlySet<string>): string {
  if (text.indexOf('(') < 0) return text;
  const toks = scan(text);
  // Frames of open calls: which LET/LAMBDA parameter names they declare.
  interface Frame {
    name: string;
    arg: number;
    /** Identifier tokens that form a whole argument (candidates for parameter declarations). */
    argIdents: (Tok | undefined)[];
    argTokCount: number[];
    params: Set<string>;
    open: number;
  }
  const stack: Frame[] = [];
  const enclosing: Frame[][] = new Array(toks.length);
  const frames: Frame[] = [];
  for (let k = 0; k < toks.length; k++) {
    const t = toks[k] as Tok;
    enclosing[k] = stack.slice();
    if (t.punct === '(') {
      const prev = toks[k - 1];
      const name = prev?.ident && prev.call && prev.end <= t.start ? prev.ident.toUpperCase() : '';
      const f: Frame = { name, arg: 0, argIdents: [], argTokCount: [0], params: new Set(), open: k };
      stack.push(f);
      frames.push(f);
    } else if (t.punct === ')') {
      const f = stack.pop();
      if (f) finishFrame(f);
    } else if (t.punct === ',' || t.punct === ';') {
      const f = stack[stack.length - 1];
      if (f && t.punct === ',') {
        f.arg++;
        f.argTokCount[f.arg] = 0;
      }
    } else if (t.ident) {
      const f = stack[stack.length - 1];
      if (f) {
        f.argTokCount[f.arg] = (f.argTokCount[f.arg] ?? 0) + 1;
        f.argIdents[f.arg] = (f.argTokCount[f.arg] as number) === 1 && !t.call ? t : undefined;
      }
    }
  }
  for (const f of stack) finishFrame(f);

  function finishFrame(f: Frame): void {
    const bare = f.name.replace(PREFIX_RE, '');
    const lastArg = f.arg;
    if (bare === 'LET') {
      for (let a = 0; a < lastArg; a += 2) {
        const id = f.argIdents[a];
        if (id?.ident) f.params.add(id.ident.replace(PREFIX_RE, '').toUpperCase());
      }
    } else if (bare === 'LAMBDA') {
      for (let a = 0; a < lastArg; a++) {
        const id = f.argIdents[a];
        if (id?.ident) f.params.add(id.ident.replace(PREFIX_RE, '').toUpperCase());
      }
    }
  }

  let out = '';
  let last = 0;
  for (let k = 0; k < toks.length; k++) {
    const t = toks[k] as Tok;
    if (!t.ident || PREFIX_RE.test(t.ident)) continue;
    const upper = t.ident.toUpperCase();
    let replacement: string | undefined;
    if (t.call) {
      if (XLWS_FUNCTIONS.has(upper)) replacement = `_xlfn._xlws.${t.ident}`;
      else if (FUTURE_FUNCTIONS.has(upper) || extra?.has(upper)) replacement = `_xlfn.${t.ident}`;
    } else {
      const scopes = enclosing[k] as Frame[];
      if (scopes.some((f) => f.params.has(upper))) replacement = `_xlpm.${t.ident}`;
    }
    if (replacement === undefined) continue;
    out += text.slice(last, t.start) + replacement;
    last = t.end;
  }
  // Parameters are also referenced inside nested LAMBDA calls (`MAP(A1:A3,LAMBDA(x,x*2))`): the scope
  // check above covers them because `enclosing` holds every open frame.
  return last === 0 ? text : out + text.slice(last);
}

/** Normalises formula text for comparisons (prefixes removed, upper-case outside literals, no spaces). */
export function normalizeFormula(text: string): string {
  const bare = fromFileFormula(text.replace(/^\s*=/, ''));
  let out = '';
  let inStr = false;
  for (let i = 0; i < bare.length; i++) {
    const ch = bare[i] as string;
    if (ch === '"') inStr = !inStr;
    if (!inStr && (ch === ' ' || ch === '\n' || ch === '\r')) continue;
    out += inStr ? ch : ch.toUpperCase();
  }
  return out;
}
