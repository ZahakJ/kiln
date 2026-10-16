#!/usr/bin/env node
/**
 * Generates the LibreOffice-oracle part of the formula conformance suite.
 *
 *   node tools/gen-conformance.mjs            # writes test/conformance/generated-*.csv
 *   node tools/gen-conformance.mjs --keep     # also keeps the work directory with the .xlsx/.csv
 *
 * 1. Builds formula cases per family (deterministic argument grids over the shared fixture).
 * 2. Writes an .xlsx with exceljs: sheet "Calc" (one formula per row, plus helper cells that record
 *    the result type and a 15-significant-digit rendering of numbers) and sheet "Data" (the fixture).
 * 3. Runs LibreOffice headless with recalculate-on-load (tools/oracle-soffice.sh … csv) and reads the
 *    recalculated values back.
 * 4. Drops rows LibreOffice cannot answer (functions it lacks, LibreOffice-only errors) and rows on the
 *    deny-list (cases where LibreOffice is known to differ from Excel), recording both in
 *    test/conformance/oracle-exclusions.csv, and writes the rest as conformance rows.
 *
 * Requires LibreOffice (default /usr/bin/soffice, override with SOFFICE).
 */
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import ExcelJS from 'exceljs';

const PKG = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = resolve(PKG, '../..');
const CONF = join(PKG, 'test/conformance');
const keep = process.argv.includes('--keep');

// ── functions that need the _xlfn. prefix in OOXML (introduced in Excel 2010 or later) ──────────

const docsDir = join(PKG, 'src/functions/docs');
const NEW_FUNCTIONS = [];
for (const f of readdirSync(docsDir)) {
  for (const e of JSON.parse(readFileSync(join(docsDir, f), 'utf8')))
    if (e.since !== '2007') NEW_FUNCTIONS.push(e.name);
}
NEW_FUNCTIONS.sort((a, b) => b.length - a.length);
const PREFIX_RE = new RegExp(
  `(^|[^A-Za-z0-9_.])(${NEW_FUNCTIONS.map((n) => n.replace(/\./g, '\\.')).join('|')})\\(`,
  'g',
);

/** Formula text for the file: `_xlfn.` prefixes outside string literals. */
function fileFormula(f) {
  return f
    .slice(1)
    .split('"')
    .map((part, i) => (i % 2 === 0 ? part.replace(PREFIX_RE, '$1_xlfn.$2(') : part))
    .join('"');
}

// ── cases ────────────────────────────────────────────────────────────────

const q = (s) => `"${s.replace(/"/g, '""')}"`;
const fam = {};
function add(family, ...formulas) {
  fam[family] ??= [];
  for (const f of formulas) fam[family].push(f);
}

const NUMS = [-2.5, -1, -0.5, 0, 0.3, 1, 2, 3.7, 10, 123.456];
const POS = [0.3, 1, 2, 3.7, 10, 123.456];
const UNIT = [-0.9, -0.5, 0, 0.25, 0.5, 0.99];

// Math & Trig
for (const f of [
  'ABS',
  'SIGN',
  'INT',
  'EVEN',
  'ODD',
  'EXP',
  'SIN',
  'COS',
  'TAN',
  'ATAN',
  'SINH',
  'COSH',
  'TANH',
  'ASINH',
  'DEGREES',
  'RADIANS',
])
  for (const x of NUMS) add('math', `=${f}(${x})`);
for (const f of [
  'LN',
  'LOG10',
  'SQRT',
  'SQRTPI',
  'FACT',
  'FACTDOUBLE',
  'GAMMALN',
  'ACOSH',
  'COT',
  'CSC',
  'SEC',
  'COTH',
  'CSCH',
  'SECH',
])
  for (const x of [...POS, -1, 0]) add('math', `=${f}(${x})`);
for (const f of ['ASIN', 'ACOS', 'ATANH', 'ACOT', 'FISHER']) for (const x of UNIT) add('math', `=${f}(${x})`);
for (const x of [1.5, 2, 5, 10, -2]) add('math', `=ACOTH(${x})`);
for (const f of ['ROUND', 'ROUNDUP', 'ROUNDDOWN', 'TRUNC'])
  for (const x of [2.15, -2.15, 1.005, 1234.5678, -0.5, 2.5, 21.5])
    for (const d of [-2, -1, 0, 1, 2]) add('math', `=${f}(${x},${d})`);
for (const [x, y] of [
  [10, 3],
  [-10, 3],
  [10, -3],
  [-10, -3],
  [5.5, 2],
  [0, 5],
  [7, 0],
  [1e10, 7],
  [2.5, 0.5],
])
  add('math', `=MOD(${x},${y})`, `=QUOTIENT(${x},${y})`, `=POWER(${x},${Math.abs(y) % 4})`);
for (const [x, y] of [
  [1, 1],
  [-1, 1],
  [1, -1],
  [-1, -1],
  [0, 2],
  [3, 0],
])
  add('math', `=ATAN2(${x},${y})`);
for (const [x, s] of [
  [2.5, 1],
  [-2.5, -2],
  [-2.5, 2],
  [1.5, 0.1],
  [0.234, 0.01],
  [4.42, 0.05],
  [2.5, -2],
  [10, 0],
])
  add('math', `=CEILING(${x},${s})`, `=FLOOR(${x},${s})`, `=MROUND(${x},${s})`);
for (const [x, s, m] of [
  [24.3, 5, 0],
  [6.7, 1, 0],
  [-8.1, 2, 0],
  [-5.5, 2, -1],
  [-5.5, 2, 0],
  [4.3, -2, 0],
])
  add(
    'math',
    `=CEILING.MATH(${x},${s},${m})`,
    `=FLOOR.MATH(${x},${s},${m})`,
    `=CEILING.PRECISE(${x},${s})`,
    `=FLOOR.PRECISE(${x},${s})`,
    `=ISO.CEILING(${x},${s})`,
  );
for (const [n, k] of [
  [8, 2],
  [10, 0],
  [10, 10],
  [5, 6],
  [30, 15],
  [4, 3],
])
  add('math', `=COMBIN(${n},${k})`, `=COMBINA(${n},${k})`, `=PERMUT(${n},${k})`, `=PERMUTATIONA(${n},${k})`);
add(
  'math',
  '=GCD(24,36)',
  '=GCD(5,0)',
  '=GCD(0,0)',
  '=GCD(12,18,27)',
  '=LCM(4,6)',
  '=LCM(5,0)',
  '=LCM(3,4,5)',
  '=MULTINOMIAL(2,3,4)',
  '=MULTINOMIAL(1,1)',
  '=PRODUCT(Data!A1:A5)',
  '=PRODUCT(Data!E1:E10)',
  '=SUMSQ(Data!E1:E10)',
  '=SUMSQ(3,4,"5")',
  '=SUMPRODUCT(Data!A1:A10,Data!E1:E10)',
  '=SUMPRODUCT(Data!A1:A5,Data!F1:F5,Data!E1:E5)',
  '=SUMPRODUCT((Data!A1:A10>50)*Data!E1:E10)',
  '=SUMX2MY2(Data!A1:A5,Data!E1:E5)',
  '=SUMX2PY2(Data!A1:A5,Data!E1:E5)',
  '=SUMXMY2(Data!A1:A5,Data!E1:E5)',
  '=SERIESSUM(0.5,1,2,Data!R1:R5)',
  '=MDETERM({1,3,8,5;1,3,6,1;1,1,1,0;7,3,10,2})',
  '=MDETERM({2,1;1,3})',
  '=INDEX(MINVERSE({4,-1;2,0}),2,1)',
  '=INDEX(MMULT({1,3;7,2},{2,0;0,2}),2,1)',
  '=SUM(MUNIT(4))',
  '=PI()',
  '=SUM(Data!A1:A10)',
  '=SUM(Data!D1:D10)',
  '=SUM(Data!A1:A10,Data!E1:E10,5)',
  '=SUM(Data!B1:B10)',
);
for (const n of [1, 4, 9, 14, 40, 49, 90, 99, 400, 499, 944, 1999, 3999]) {
  for (const form of [0, 1, 2, 3, 4]) add('math', `=ROMAN(${n},${form})`);
  add('math', `=ARABIC(ROMAN(${n}))`);
}
for (const [n, r] of [
  [7, 2],
  [100, 16],
  [255, 16],
  [35, 36],
  [1000, 8],
])
  add('math', `=BASE(${n},${r})`, `=BASE(${n},${r},10)`, `=DECIMAL(BASE(${n},${r}),${r})`);
for (const fn of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 101, 109])
  add('math', `=SUBTOTAL(${fn},Data!A1:A10)`, `=SUBTOTAL(${fn},Data!E1:E10)`);
for (const fn of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13])
  add('math', `=AGGREGATE(${fn},6,Data!G1:G4)`, `=AGGREGATE(${fn},4,Data!E1:E10)`);
for (const fn of [14, 15, 16, 17, 18, 19])
  add('math', `=AGGREGATE(${fn},6,Data!E1:E10,${fn >= 16 ? (fn === 17 || fn === 19 ? 1 : 0.4) : 2})`);

// Conditional aggregates
const CRIT = [
  '">50"',
  '"<=30"',
  '"<>40"',
  '50',
  '"apple"',
  '"a*"',
  '"*an*"',
  '"?????"',
  '"<>apple"',
  '""',
  '"<>"',
  '">b"',
  '"10"',
  '"=10"',
  '">=45413"',
  '"<2024-03-01"',
];
for (const c of CRIT)
  add(
    'conditional',
    `=COUNTIF(Data!A1:A10,${c})`,
    `=COUNTIF(Data!B1:B10,${c})`,
    `=COUNTIF(Data!C1:C10,${c})`,
    `=SUMIF(Data!B1:B10,${c},Data!A1:A10)`,
    `=SUMIF(Data!A1:A10,${c})`,
  );
for (const c of ['">0"', '"<0"', '">=3"', '"<>0"', '4', '"4"'])
  add(
    'conditional',
    `=AVERAGEIF(Data!E1:E10,${c})`,
    `=AVERAGEIF(Data!F1:F10,${c},Data!A1:A10)`,
    `=COUNTIF(Data!D1:D10,${c})`,
  );
add(
  'conditional',
  '=COUNTIFS(Data!A1:A10,">20",Data!E1:E10,">0")',
  '=COUNTIFS(Data!B1:B10,"a*",Data!A1:A10,"<60")',
  '=SUMIFS(Data!A1:A10,Data!B1:B10,"*a*",Data!E1:E10,"<5")',
  '=SUMIFS(Data!E1:E10,Data!A1:A10,">=30",Data!A1:A10,"<=80")',
  '=SUMIFS(Data!A1:A10,Data!C1:C10,">=2024-03-01",Data!C1:C10,"<2024-09-01")',
  '=AVERAGEIFS(Data!A1:A10,Data!E1:E10,">0",Data!F1:F10,"<10")',
  '=AVERAGEIFS(Data!A1:A10,Data!E1:E10,">100")',
  '=MAXIFS(Data!A1:A10,Data!E1:E10,"<0")',
  '=MINIFS(Data!A1:A10,Data!E1:E10,">0")',
  '=MAXIFS(Data!A1:A10,Data!B1:B10,"zzz")',
  '=COUNTBLANK(Data!D1:D10)',
  '=COUNTBLANK(Data!A1:A10)',
  '=COUNTIF(Data!G1:G4,"#N/A")',
  '=SUMIF(Data!B1:B10,"apple",Data!A1)',
  '=COUNTIF(Data!F1:F10,4)',
  '=SUMIF(Data!F1:F10,1.5,Data!E1:E10)',
);

// Statistical
const R1 = 'Data!A1:A10';
const R2 = 'Data!E1:E10';
const R3 = 'Data!F1:F10';
for (const f of [
  'AVERAGE',
  'MEDIAN',
  'MODE.SNGL',
  'MODE',
  'STDEV.S',
  'STDEV.P',
  'STDEV',
  'STDEVP',
  'VAR.S',
  'VAR.P',
  'VAR',
  'VARP',
  'AVEDEV',
  'DEVSQ',
  'KURT',
  'SKEW',
  'SKEW.P',
  'MAX',
  'MIN',
  'COUNT',
  'COUNTA',
  'GEOMEAN',
  'HARMEAN',
  'AVERAGEA',
  'MAXA',
  'MINA',
  'STDEVA',
  'STDEVPA',
  'VARA',
  'VARPA',
])
  for (const r of [R1, R2, R3, 'Data!D1:D10']) add('stats', `=${f}(${r})`);
for (const f of ['LARGE', 'SMALL'])
  for (const r of [R2, R3]) for (const k of [1, 2, 5, 10, 11]) add('stats', `=${f}(${r},${k})`);
for (const f of ['PERCENTILE.INC', 'PERCENTILE', 'PERCENTILE.EXC'])
  for (const r of [R2, R3]) for (const k of [0, 0.1, 0.25, 0.5, 0.9, 1]) add('stats', `=${f}(${r},${k})`);
for (const f of ['QUARTILE.INC', 'QUARTILE', 'QUARTILE.EXC'])
  for (const r of [R2, R3]) for (const k of [0, 1, 2, 3, 4]) add('stats', `=${f}(${r},${k})`);
for (const f of ['PERCENTRANK.INC', 'PERCENTRANK', 'PERCENTRANK.EXC'])
  for (const x of [-3.5, 0, 2.25, 3, 5, 8]) add('stats', `=${f}(${R2},${x})`, `=${f}(${R2},${x},2)`);
for (const f of ['RANK.EQ', 'RANK', 'RANK.AVG'])
  for (const x of [4, 7.25, 1.5, 20, 5]) add('stats', `=${f}(${x},${R3})`, `=${f}(${x},${R3},1)`);
for (const f of [
  'CORREL',
  'PEARSON',
  'RSQ',
  'COVARIANCE.P',
  'COVARIANCE.S',
  'COVAR',
  'SLOPE',
  'INTERCEPT',
  'STEYX',
])
  add('stats', `=${f}(${R1},${R2})`, `=${f}(${R2},${R3})`, `=${f}(Data!E1:E10,Data!D1:D10)`);
for (const x of [0, 35, 110])
  add('stats', `=FORECAST(${x},${R2},${R1})`, `=FORECAST.LINEAR(${x},${R3},${R1})`);
add(
  'stats',
  `=TRIMMEAN(${R2},0.2)`,
  `=TRIMMEAN(${R3},0.5)`,
  `=PROB(Data!R1:R5,{0.1,0.2,0.3,0.2,0.2},2,4)`,
  `=PROB(Data!R1:R5,{0.1,0.2,0.3,0.2,0.2},3)`,
  `=INDEX(FREQUENCY(${R2},{0,3,6}),1)`,
  `=INDEX(FREQUENCY(${R2},{0,3,6}),4)`,
  `=INDEX(LINEST(${R2},${R1}),1)`,
  `=INDEX(LINEST(${R2},${R1}),2)`,
  `=INDEX(LINEST(${R3},${R1},TRUE,TRUE),3,1)`,
  `=INDEX(LINEST(${R3},${R1},TRUE,TRUE),4,2)`,
  `=INDEX(LINEST(${R3},${R1},FALSE),1)`,
  `=INDEX(LOGEST(${R3},${R1}),1)`,
  `=INDEX(TREND(${R3},${R1},{110,120}),2)`,
  `=INDEX(GROWTH(${R3},${R1},{110}),1)`,
  `=STANDARDIZE(42,40,1.5)`,
  `=FISHERINV(0.5)`,
  `=CONFIDENCE.NORM(0.05,2.5,50)`,
  `=CONFIDENCE(0.1,1,10)`,
  `=CONFIDENCE.T(0.05,1,50)`,
  `=CONFIDENCE.T(0.1,2,5)`,
  `=T.TEST(${R1},${R3},2,1)`,
  `=T.TEST(${R1},${R3},1,2)`,
  `=T.TEST(${R2},${R3},2,3)`,
  `=TTEST(${R2},${R3},1,3)`,
  `=F.TEST(${R2},${R3})`,
  `=FTEST(${R1},${R2})`,
  `=CHISQ.TEST({58,35;11,25;10,23},{45.35,47.65;17.56,18.44;16.09,16.91})`,
  `=CHITEST(Data!R1:R5,Data!T1:T5)`,
  `=Z.TEST(${R2},2)`,
  `=Z.TEST(${R2},2,3)`,
  `=ZTEST(${R3},6)`,
  '=GAMMA(2.5)',
  '=GAMMA(-3.75)',
  '=GAMMA(10)',
  '=GAMMALN.PRECISE(4.5)',
  '=GAUSS(1.5)',
  '=PHI(0.75)',
);
// Distributions
for (const x of [-2, -0.5, 0, 0.75, 1.96, 3])
  add(
    'dist',
    `=NORM.S.DIST(${x},TRUE)`,
    `=NORM.S.DIST(${x},FALSE)`,
    `=NORMSDIST(${x})`,
    `=NORM.DIST(${x},0.5,2,TRUE)`,
    `=NORM.DIST(${x},0.5,2,FALSE)`,
  );
for (const p of [0.001, 0.025, 0.3, 0.5, 0.8, 0.975, 0.999])
  add(
    'dist',
    `=NORM.S.INV(${p})`,
    `=NORMSINV(${p})`,
    `=NORM.INV(${p},10,3)`,
    `=LOGNORM.INV(${p},1,0.5)`,
    `=T.INV(${p},5)`,
    `=CHISQ.INV(${p},4)`,
    `=CHISQ.INV.RT(${p},4)`,
    `=F.INV(${p},3,7)`,
    `=F.INV.RT(${p},3,7)`,
    `=GAMMA.INV(${p},2.5,1.5)`,
    `=BETA.INV(${p},2,3)`,
    `=T.INV.2T(${p},12)`,
    `=BINOM.INV(20,0.3,${p})`,
  );
for (const x of [0.5, 1, 2.5, 6])
  add(
    'dist',
    `=LOGNORM.DIST(${x},0.5,0.8,TRUE)`,
    `=LOGNORM.DIST(${x},0.5,0.8,FALSE)`,
    `=LOGNORMDIST(${x},0.5,0.8)`,
    `=T.DIST(${x},4,TRUE)`,
    `=T.DIST(${x},4,FALSE)`,
    `=T.DIST.RT(${x},4)`,
    `=T.DIST.2T(${x},4)`,
    `=TDIST(${x},4,1)`,
    `=TDIST(${x},4,2)`,
    `=CHISQ.DIST(${x},3,TRUE)`,
    `=CHISQ.DIST(${x},3,FALSE)`,
    `=CHISQ.DIST.RT(${x},3)`,
    `=CHIDIST(${x},3)`,
    `=F.DIST(${x},4,9,TRUE)`,
    `=F.DIST(${x},4,9,FALSE)`,
    `=F.DIST.RT(${x},4,9)`,
    `=FDIST(${x},4,9)`,
    `=GAMMA.DIST(${x},2,1.5,TRUE)`,
    `=GAMMA.DIST(${x},2,1.5,FALSE)`,
    `=GAMMADIST(${x},0.7,2,TRUE)`,
    `=EXPON.DIST(${x},0.7,TRUE)`,
    `=EXPON.DIST(${x},0.7,FALSE)`,
    `=WEIBULL.DIST(${x},1.5,2,TRUE)`,
    `=WEIBULL.DIST(${x},1.5,2,FALSE)`,
    `=WEIBULL(${x},0.8,1,TRUE)`,
  );
for (const x of [0.1, 0.35, 0.6, 0.95])
  add(
    'dist',
    `=BETA.DIST(${x},2,3,TRUE)`,
    `=BETA.DIST(${x},2,3,FALSE)`,
    `=BETA.DIST(${x}*4+1,2,5,TRUE,1,5)`,
    `=BETADIST(${x},0.5,0.5)`,
  );
for (const k of [0, 3, 6, 10])
  add(
    'dist',
    `=BINOM.DIST(${k},10,0.35,TRUE)`,
    `=BINOM.DIST(${k},10,0.35,FALSE)`,
    `=BINOMDIST(${k},12,0.5,TRUE)`,
    `=POISSON.DIST(${k},4.2,TRUE)`,
    `=POISSON.DIST(${k},4.2,FALSE)`,
    `=POISSON(${k},1,TRUE)`,
    `=NEGBINOM.DIST(${k},3,0.4,TRUE)`,
    `=NEGBINOM.DIST(${k},3,0.4,FALSE)`,
    `=NEGBINOMDIST(${k},5,0.25)`,
  );
for (const k of [0, 1, 2, 4])
  add(
    'dist',
    `=HYPGEOM.DIST(${k},4,8,20,TRUE)`,
    `=HYPGEOM.DIST(${k},4,8,20,FALSE)`,
    `=HYPGEOMDIST(${k},6,9,30)`,
  );
add(
  'dist',
  '=BINOM.DIST.RANGE(60,0.75,48)',
  '=BINOM.DIST.RANGE(60,0.75,45,50)',
  '=CRITBINOM(6,0.5,0.75)',
  '=NORMINV(0.908789,40,1.5)',
  '=LOGINV(0.039084,3.5,1.2)',
  '=TINV(0.05,10)',
  '=CHIINV(0.05,10)',
  '=FINV(0.01,6,4)',
  '=GAMMAINV(0.068094,9,2)',
  '=BETAINV(0.685470581,8,10,1,3)',
  '=EXPONDIST(0.2,10,FALSE)',
  '=NORMDIST(42,40,1.5,FALSE)',
);

// Logical & information
const VALS = [
  '1',
  '0',
  '-2.5',
  '"abc"',
  '""',
  'TRUE',
  'FALSE',
  '1/0',
  'NA()',
  'Data!A1',
  'Data!D4',
  'Data!B6',
  'Data!G1',
  '"5"',
  'SQRT(-1)',
];
for (const v of VALS)
  add(
    'info',
    `=ISBLANK(${v})`,
    `=ISERR(${v})`,
    `=ISERROR(${v})`,
    `=ISNA(${v})`,
    `=ISLOGICAL(${v})`,
    `=ISNUMBER(${v})`,
    `=ISTEXT(${v})`,
    `=ISNONTEXT(${v})`,
    `=N(${v})`,
    `=TYPE(${v})`,
    `=IFERROR(${v},"err")`,
    `=IFNA(${v},"na")`,
    `=ERROR.TYPE(${v})`,
  );
for (const v of ['1', '2', '-3', '2.9', '0', 'Data!D4']) add('info', `=ISEVEN(${v})`, `=ISODD(${v})`);
add('info', '=ISREF(Data!A1)', '=ISREF("A1")', '=ISREF(Data!A1:B2)', '=T("abc")', '=T(12)');
add(
  'logical',
  '=AND(TRUE,1,"TRUE")',
  '=AND(Data!D1:D10)',
  '=OR(Data!D1:D10)',
  '=XOR(TRUE,TRUE,FALSE)',
  '=XOR(Data!D1:D10)',
  '=AND(1>0,2>1,3>4)',
  '=OR(0,0,0)',
  '=NOT(0)',
  '=NOT("a")',
  '=IF(Data!E1>0,"pos","neg")',
  '=IF(0,1)',
  '=IF("x",1,2)',
  '=IFS(Data!A1>50,"big",Data!A1>5,"mid",TRUE,"small")',
  '=IFS(FALSE,1)',
  '=SWITCH(Data!B2,"apple",1,"banana",2,3)',
  '=SWITCH(3,1,"a",2,"b")',
  '=SWITCH(Data!A3,30,"thirty","other")',
  '=IFERROR(SQRT(-1),0)',
  '=AND()',
);

// Text
const STRS = [
  '"Office Suite"',
  '"  hello   world  "',
  '"ABC def"',
  '"x"',
  '""',
  '12345.678',
  'TRUE',
  'Data!B5',
];
for (const s of STRS)
  add(
    'text',
    `=LEN(${s})`,
    `=UPPER(${s})`,
    `=LOWER(${s})`,
    `=PROPER(${s})`,
    `=TRIM(${s})`,
    `=LEFT(${s},3)`,
    `=RIGHT(${s},2)`,
    `=MID(${s},2,4)`,
    `=REPT(${s},2)`,
    `=CODE(${s})`,
    `=EXACT(${s},"x")`,
    `=CLEAN(${s})`,
    `=T(${s})`,
    `=SUBSTITUTE(${s},"l","L")`,
  );
for (const [n, s] of [
  ['"o"', '"Hello world"'],
  ['"O"', '"Hello world"'],
  ['"wor"', '"Hello world"'],
  ['""', '"abc"'],
  ['"?l"', '"Hello world"'],
  ['"*d"', '"Hello world"'],
  ['"z"', '"abc"'],
  ['"~?"', '"what?"'],
])
  add('text', `=FIND(${n},${s})`, `=SEARCH(${n},${s})`, `=FIND(${n},${s},3)`, `=SEARCH(${n},${s},3)`);
add(
  'text',
  '=REPLACE("abcdefghijk",6,5,"*")',
  '=REPLACE("2009",3,2,"10")',
  '=REPLACE("abc",10,1,"z")',
  '=SUBSTITUTE("Quarter 1, 2011","1","2",3)',
  '=SUBSTITUTE("aaa","a","b",2)',
  '=SUBSTITUTE("abc","","x")',
  '=CONCATENATE("a",1,TRUE,2.5)',
  '=CONCATENATE(Data!B1," & ",Data!B2)',
  '="a"&1.5&FALSE',
  '=CHAR(65)',
  '=CHAR(97)',
  '=CHAR(32)',
  '=CODE("€")',
  '=UNICODE("€")',
  '=UNICHAR(8364)',
  '=UNICHAR(97)',
  '=UNICODE("a")',
  '=VALUE("1,234.5")',
  '=VALUE("$1,000")',
  '=VALUE("12%")',
  '=VALUE("(25)")',
  '=VALUE("1e3")',
  '=VALUE("2024-01-15")',
  '=VALUE("6:00 PM")',
  '=VALUE("abc")',
  '=NUMBERVALUE("2.500,27",",",".")',
  '=NUMBERVALUE("3.5%")',
  '=NUMBERVALUE(" 1 234 ")',
  '=REPT("ab",0)',
  '=LEFT("abc",0)',
  '=LEFT("abc",10)',
  '=MID("abc",5,2)',
  '=RIGHT("abc",-1)',
);
for (const x of [1234.567, -1234.567, 0.5, 0, 1e6, -0.004])
  for (const d of [-2, 0, 1, 2, 4])
    add('text', `=FIXED(${x},${d})`, `=FIXED(${x},${d},TRUE)`, `=DOLLAR(${x},${d})`);
const FORMATS = [
  '"#,##0.00"',
  '"0%"',
  '"0.0%"',
  '"0.00E+00"',
  '"$#,##0.00"',
  '"000000"',
  '"0.###"',
  '"# ?/?"',
  '"yyyy-mm-dd"',
  '"dd/mm/yyyy"',
  '"mmm d, yyyy"',
  '"dddd"',
  '"mmmm"',
  '"h:mm AM/PM"',
  '"hh:mm:ss"',
  '"[h]:mm"',
  '"@"',
  '"0;(0)"',
  '"#,##0,"',
  '"0.00;-0.00;""zero"""',
];
for (const fmt of FORMATS) for (const v of [1234.5678, -42.5, 0, 45306.75]) add('text', `=TEXT(${v},${fmt})`);
add(
  'text',
  '=TEXT("1234","0.0")',
  '=TEXT("abc","0.0")',
  '=TEXT(TRUE,"0")',
  '=TEXT(0.285,"0.0%")',
  '=TEXT(1/3,"0.000000")',
);

// Date & time
const DATES = [
  'DATE(2024,1,15)',
  'DATE(2023,12,31)',
  'DATE(2024,2,29)',
  'DATE(2021,1,1)',
  'DATE(2008,9,30)',
  'DATE(1900,3,1)',
  'DATE(2027,6,6)',
];
for (const d of DATES) {
  add(
    'date',
    `=${d}`,
    `=YEAR(${d})`,
    `=MONTH(${d})`,
    `=DAY(${d})`,
    `=ISOWEEKNUM(${d})`,
    `=EDATE(${d},1)`,
    `=EDATE(${d},-13)`,
    `=EOMONTH(${d},0)`,
    `=EOMONTH(${d},2)`,
    `=EOMONTH(${d},-1)`,
  );
  for (const t of [1, 2, 3, 11, 12, 13, 14, 15, 16, 17]) add('date', `=WEEKDAY(${d},${t})`);
  for (const t of [1, 2, 11, 12, 13, 14, 15, 16, 17, 21]) add('date', `=WEEKNUM(${d},${t})`);
}
const PAIRS = [
  ['DATE(2020,1,31)', 'DATE(2020,3,1)'],
  ['DATE(2001,6,1)', 'DATE(2002,8,15)'],
  ['DATE(2012,1,1)', 'DATE(2012,7,30)'],
  ['DATE(2011,1,30)', 'DATE(2011,2,28)'],
  ['DATE(2010,2,28)', 'DATE(2012,2,29)'],
  ['DATE(2023,5,15)', 'DATE(2027,11,3)'],
  ['DATE(2024,3,31)', 'DATE(2024,4,30)'],
];
for (const [a, b] of PAIRS) {
  for (const u of ['"Y"', '"M"', '"D"', '"YM"']) add('date', `=DATEDIF(${a},${b},${u})`);
  for (const basis of [0, 1, 2, 3, 4]) add('date', `=YEARFRAC(${a},${b},${basis})`);
  add(
    'date',
    `=DAYS360(${a},${b})`,
    `=DAYS360(${a},${b},TRUE)`,
    `=DAYS(${b},${a})`,
    `=NETWORKDAYS(${a},${b})`,
    `=NETWORKDAYS(${a},${b},Data!C1:C10)`,
    `=NETWORKDAYS.INTL(${a},${b},7)`,
    `=NETWORKDAYS.INTL(${a},${b},"0010001")`,
    `=NETWORKDAYS.INTL(${a},${b},11)`,
  );
}
for (const d of [5, 10, 30, 151, -5, -20])
  add(
    'date',
    `=WORKDAY(DATE(2024,1,10),${d})`,
    `=WORKDAY(DATE(2024,1,10),${d},Data!C1:C10)`,
    `=WORKDAY.INTL(DATE(2024,1,10),${d},7)`,
    `=WORKDAY.INTL(DATE(2024,1,10),${d},"1000001")`,
  );
for (const [h, m, s] of [
  [12, 0, 0],
  [16, 48, 10],
  [25, 0, 0],
  [0, 750, 0],
  [0, 0, 2000],
])
  add('date', `=TIME(${h},${m},${s})`);
for (const t of ['0.75', '0.999999', '"3:30:30 PM"', '45306.25', '"12:45:59"'])
  add('date', `=HOUR(${t})`, `=MINUTE(${t})`, `=SECOND(${t})`);
add(
  'date',
  '=DATEVALUE("8/22/2011")',
  '=DATEVALUE("22-MAY-2011")',
  '=DATEVALUE("2011/02/23")',
  '=DATEVALUE("5-JUL")',
  '=TIMEVALUE("2:24 AM")',
  '=TIMEVALUE("22-Aug-2011 6:35 AM")',
  '=DATE(2008,14,2)',
  '=DATE(2008,1,-15)',
  '=DATE(108,1,2)',
  '=DATE(2024,0,0)',
  '=DAY(60)',
  '=MONTH(60)',
  '=WEEKDAY(1)',
);

// Lookup & reference
const KEYS = ['30', '35', '5', '100', '"cherry"', '"Banana"', '"k*"', '"zzz"'];
for (const k of KEYS) {
  add(
    'lookup',
    `=VLOOKUP(${k},Data!A1:C10,3,FALSE)`,
    `=VLOOKUP(${k},Data!A1:C10,2)`,
    `=MATCH(${k},Data!A1:A10,0)`,
    `=MATCH(${k},Data!A1:A10)`,
    `=MATCH(${k},Data!B1:B10,0)`,
    `=LOOKUP(${k},Data!A1:A10,Data!B1:B10)`,
    `=HLOOKUP(${k},Data!R1:T5,2,FALSE)`,
  );
}
add(
  'lookup',
  '=MATCH(3,Data!T1:T5,-1)',
  '=MATCH(2.5,Data!T1:T5,-1)',
  '=HLOOKUP(2,{1,2,3;"a","b","c"},2)',
  '=HLOOKUP(2.5,{1,2,3;"a","b","c"},2,TRUE)',
  '=INDEX(Data!A1:C10,4,2)',
  '=INDEX(Data!A1:C10,10,3)',
  '=INDEX(Data!A1:A10,11)',
  '=INDEX({1,2;3,4},2,2)',
  '=SUM(INDEX(Data!A1:C10,0,1))',
  '=SUM(INDEX(Data!R1:T5,2,0))',
  '=CHOOSE(2,"a","b","c")',
  '=CHOOSE(4,"a","b","c")',
  '=SUM(CHOOSE(2,Data!A1:A3,Data!E1:E3))',
  '=ADDRESS(2,3)',
  '=ADDRESS(2,3,2)',
  '=ADDRESS(2,3,3)',
  '=ADDRESS(2,3,4)',
  '=ADDRESS(2,3,1,FALSE)',
  '=ADDRESS(2,3,4,FALSE)',
  '=ADDRESS(2,30,1,TRUE,"My Sheet")',
  '=ADDRESS(1,16384)',
  '=ROWS(Data!A1:C10)',
  '=COLUMNS(Data!A1:C10)',
  '=ROWS({1,2;3,4;5,6})',
  '=AREAS(Data!A1:B2)',
  '=ROW(Data!C7)',
  '=COLUMN(Data!C7)',
  '=SUM(OFFSET(Data!A1,2,0,3,1))',
  '=OFFSET(Data!A1,9,1)',
  '=SUM(INDIRECT("Data!A1:A3"))',
  '=INDIRECT("Data!B"&3)',
  '=LOOKUP(25,Data!A1:A10)',
  '=LOOKUP("c",{"a","b","c","d";1,2,3,4})',
  '=VLOOKUP("apple",Data!B1:C10,2,FALSE)',
  '=VLOOKUP(10,Data!A1:A10,2,FALSE)',
  '=TRANSPOSE(Data!A1:A3)',
);

// Financial
for (const [r, n, pv, fv, t] of [
  [0.08 / 12, 10, 10000, 0, 0],
  [0.08 / 12, 10, 10000, 0, 1],
  [0.06 / 12, 216, 0, 50000, 0],
  [0, 12, 1200, 0, 0],
  [0.05, 30, -250000, 1000, 1],
  [0.1, 3, 8000, 0, 0],
])
  add(
    'financial',
    `=PMT(${r},${n},${pv},${fv},${t})`,
    `=FV(${r},${n},-100,${pv},${t})`,
    `=PV(${r},${n},-100,${fv},${t})`,
    `=IPMT(${r},1,${n},${pv},${fv},${t})`,
    `=IPMT(${r},${Math.max(1, Math.floor(n / 2))},${n},${pv},${fv},${t})`,
    `=PPMT(${r},${n},${n},${pv},${fv},${t})`,
    `=NPER(${r},-1000,${pv},${fv},${t})`,
    `=ISPMT(${r},1,${n},${pv})`,
  );
add(
  'financial',
  '=RATE(48,-200,8000)',
  '=RATE(10,-1000,7000,0,1)',
  '=RATE(360,-1000,150000)',
  '=RATE(5,0,-1000,1500)',
  '=NPV(0.1,-10000,3000,4200,6800)',
  '=NPV(0.08,Data!W1:W5)',
  '=NPV(0.05,Data!U1:U5,100)',
  '=IRR(Data!U1:U4)',
  '=IRR(Data!U1:U5)',
  '=IRR(Data!W1:W5)',
  '=IRR({-70000,12000,15000,18000,21000})',
  '=IRR({-70000,12000,15000,18000,21000,26000})',
  '=IRR({-70000,12000,15000},-0.4)',
  '=MIRR({-120000,39000,30000,21000,37000,46000},0.1,0.12)',
  '=MIRR(Data!U1:U5,0.05,0.07)',
  '=XNPV(0.09,Data!W1:W5,Data!V1:V5)',
  '=XIRR(Data!W1:W5,Data!V1:V5)',
  '=XIRR(Data!W1:W5,Data!V1:V5,0.5)',
  '=EFFECT(0.0525,4)',
  '=EFFECT(0.1,12)',
  '=NOMINAL(0.053543,4)',
  '=NOMINAL(0.12,12)',
  '=FVSCHEDULE(1,{0.09,0.11,0.1})',
  '=FVSCHEDULE(1000,Data!E2:E4)',
  '=PDURATION(0.025,2000,2200)',
  '=RRI(96,10000,11000)',
  '=SLN(30000,7500,10)',
  '=DOLLARDE(1.02,16)',
  '=DOLLARDE(1.1,32)',
  '=DOLLARDE(-3.25,8)',
  '=DOLLARFR(1.125,16)',
  '=DOLLARFR(1.125,32)',
  '=DOLLARFR(2.5,3)',
);
for (const per of [1, 2, 5, 10])
  add('financial', `=SYD(30000,7500,10,${per})`, `=DDB(2400,300,10,${per})`, `=DDB(2400,300,10,${per},1.5)`);
for (const per of [1, 2, 4, 6, 7])
  add('financial', `=DB(1000000,100000,6,${per},7)`, `=DB(1000000,100000,6,${Math.min(per, 6)})`);
for (const [s, e] of [
  [0, 1],
  [0, 0.875],
  [6, 18],
  [3, 5.5],
  [0, 10],
])
  add(
    'financial',
    `=VDB(2400,300,10,${s},${Math.min(e, 10)})`,
    `=VDB(2400,300,10,${s},${Math.min(e, 10)},1.5)`,
    `=VDB(2400,300,10,${s},${Math.min(e, 10)},2,TRUE)`,
  );
add('financial', '=VDB(2400,300,120,6,18)', '=VDB(2400,300,3650,0,1)');
for (const [r, n, pv, s, e, t] of [
  [0.0075, 360, 125000, 13, 24, 0],
  [0.0075, 360, 125000, 1, 1, 0],
  [0.01, 24, 5000, 1, 24, 1],
])
  add('financial', `=CUMIPMT(${r},${n},${pv},${s},${e},${t})`, `=CUMPRINC(${r},${n},${pv},${s},${e},${t})`);
const BONDS = [
  ['DATE(2008,2,15)', 'DATE(2017,11,15)'],
  ['DATE(2011,1,25)', 'DATE(2011,11,15)'],
  ['DATE(2018,7,1)', 'DATE(2048,1,1)'],
  ['DATE(2024,3,10)', 'DATE(2029,9,30)'],
];
for (const [s, m] of BONDS) {
  for (const freq of [1, 2, 4])
    for (const basis of [0, 1, 2, 3, 4])
      add(
        'financial',
        `=COUPNUM(${s},${m},${freq},${basis})`,
        `=COUPPCD(${s},${m},${freq},${basis})`,
        `=COUPNCD(${s},${m},${freq},${basis})`,
        `=COUPDAYS(${s},${m},${freq},${basis})`,
        `=COUPDAYBS(${s},${m},${freq},${basis})`,
        `=COUPDAYSNC(${s},${m},${freq},${basis})`,
      );
  for (const basis of [0, 1, 2, 3, 4])
    add(
      'financial',
      `=PRICE(${s},${m},0.0575,0.065,100,2,${basis})`,
      `=YIELD(${s},${m},0.0575,95.04287,100,2,${basis})`,
      `=DURATION(${s},${m},0.08,0.09,2,${basis})`,
      `=MDURATION(${s},${m},0.08,0.09,4,${basis})`,
      `=DISC(${s},${m},97.975,100,${basis})`,
      `=INTRATE(${s},${m},1000000,1014420,${basis})`,
      `=RECEIVED(${s},${m},1000000,0.0575,${basis})`,
      `=PRICEDISC(${s},${m},0.0525,100,${basis})`,
      `=YIELDDISC(${s},${m},99.795,100,${basis})`,
      `=ACCRINTM(DATE(2007,4,1),${s},0.1,1000,${basis})`,
      `=PRICEMAT(${s},${m},DATE(2007,11,11),0.061,0.061,${basis})`,
      `=YIELDMAT(${s},${m},DATE(2007,11,8),0.0625,100.0123,${basis})`,
    );
}
add(
  'financial',
  '=ACCRINT(DATE(2008,3,1),DATE(2008,8,31),DATE(2008,5,1),0.1,1000,2,0)',
  '=TBILLPRICE(DATE(2008,3,31),DATE(2008,6,1),0.09)',
  '=TBILLYIELD(DATE(2008,3,31),DATE(2008,6,1),98.45)',
  '=TBILLEQ(DATE(2008,3,31),DATE(2008,6,1),0.0914)',
  '=TBILLEQ(DATE(2008,3,31),DATE(2008,12,1),0.0914)',
  '=TBILLPRICE(DATE(2008,3,31),DATE(2009,3,1),0.05)',
);

// Engineering
for (const n of [0, 1, 9, 100, 511, -1, -100, -512])
  add('engineering', `=DEC2BIN(${n})`, `=DEC2OCT(${n})`, `=DEC2HEX(${n})`, `=DEC2BIN(${n},10)`);
for (const n of [0, 255, 4096, 123456789, -54, -549755813888])
  add('engineering', `=DEC2HEX(${n})`, `=DEC2OCT(${n % 536870912})`);
for (const b of ['"0"', '"1100100"', '"1111111111"', '"1000000000"', '"101"', '"12"'])
  add('engineering', `=BIN2DEC(${b})`, `=BIN2HEX(${b})`, `=BIN2OCT(${b})`, `=BIN2HEX(${b},8)`);
for (const h of ['"A5"', '"FFFFFFFF5B"', '"3DA408B9"', '"F"', '"1FF"', '"FFFFFFFE00"', '"G1"'])
  add('engineering', `=HEX2DEC(${h})`, `=HEX2BIN(${h})`, `=HEX2OCT(${h})`, `=HEX2BIN(${h},10)`);
for (const o of ['"54"', '"7777777533"', '"3"', '"777"', '"7777777000"', '"8"'])
  add('engineering', `=OCT2DEC(${o})`, `=OCT2BIN(${o})`, `=OCT2HEX(${o})`, `=OCT2HEX(${o},4)`);
for (const [a, b] of [
  [13, 25],
  [1, 5],
  [23, 10],
  [0, 7],
  [281474976710655, 1],
])
  add(
    'engineering',
    `=BITAND(${a},${b})`,
    `=BITOR(${a},${b})`,
    `=BITXOR(${a},${b})`,
    `=BITLSHIFT(${a % 1000},${b % 8})`,
    `=BITRSHIFT(${a},${b % 8})`,
  );
for (const [a, b] of [
  [5, 4],
  [5, 5],
  [0.5, 0],
  [-3, -3],
])
  add('engineering', `=DELTA(${a},${b})`, `=GESTEP(${a},${b})`, `=DELTA(${a})`, `=GESTEP(${a})`);
for (const x of [-1.5, -0.3, 0, 0.745, 1, 2.5, 4])
  add(
    'engineering',
    `=ERF(${x})`,
    `=ERFC(${x})`,
    `=ERF.PRECISE(${x})`,
    `=ERFC.PRECISE(${x})`,
    `=ERF(${x},${x + 1})`,
  );
for (const x of [0.5, 1.5, 2.5, 7.2])
  for (const n of [0, 1, 2, 3])
    add(
      'engineering',
      `=BESSELJ(${x},${n})`,
      `=BESSELI(${x},${n})`,
      `=BESSELK(${x},${n})`,
      `=BESSELY(${x},${n})`,
    );
const CPLX = ['"3+4i"', '"1+i"', '"-2-3j"', '"5"', '"-i"', '"2.5-0.5i"'];
for (const z of CPLX)
  add(
    'engineering',
    `=IMABS(${z})`,
    `=IMREAL(${z})`,
    `=IMAGINARY(${z})`,
    `=IMARGUMENT(${z})`,
    `=IMCONJUGATE(${z})`,
    `=IMREAL(IMEXP(${z}))`,
    `=IMAGINARY(IMEXP(${z}))`,
    `=IMREAL(IMLN(${z}))`,
    `=IMAGINARY(IMLN(${z}))`,
    `=IMREAL(IMSQRT(${z}))`,
    `=IMAGINARY(IMSQRT(${z}))`,
    `=IMREAL(IMSIN(${z}))`,
    `=IMAGINARY(IMCOS(${z}))`,
    `=IMREAL(IMTAN(${z}))`,
    `=IMREAL(IMLOG10(${z}))`,
    `=IMAGINARY(IMLOG2(${z}))`,
    `=IMREAL(IMPOWER(${z},3))`,
    `=IMAGINARY(IMPOWER(${z},0.5))`,
    `=IMREAL(IMSINH(${z}))`,
    `=IMAGINARY(IMCOSH(${z}))`,
    `=IMREAL(IMSEC(${z}))`,
    `=IMAGINARY(IMCSC(${z}))`,
    `=IMREAL(IMCOT(${z}))`,
    `=IMAGINARY(IMSECH(${z}))`,
    `=IMREAL(IMCSCH(${z}))`,
  );
add(
  'engineering',
  '=COMPLEX(3,4)',
  '=COMPLEX(3,4,"j")',
  '=COMPLEX(0,1)',
  '=COMPLEX(0,-1)',
  '=COMPLEX(1.5,0)',
  '=COMPLEX(0,0)',
  '=COMPLEX(-2,2.5)',
  '=IMSUM("3+4i","5-3i")',
  '=IMSUB("13+4j","5+3j")',
  '=IMPRODUCT("3+4i","5-3i")',
  '=IMPRODUCT("1+2i",30)',
  '=IMDIV("-238+240i","10+24i")',
  '=IMABS(IMSUM("1+i","1-i","2"))',
  '=IMSUM("1+i","1+j")',
);
for (const [v, a, b] of [
  [1, 'lbm', 'kg'],
  [68, 'F', 'C'],
  [100, 'C', 'F'],
  [0, 'K', 'C'],
  [2.5, 'ft', 'sec'],
  [1, 'mi', 'km'],
  [1, 'km', 'mi'],
  [6, 'in', 'cm'],
  [1, 'gal', 'l'],
  [1, 'hr', 'mn'],
  [1, 'yr', 'day'],
  [1, 'atm', 'Pa'],
  [1, 'HP', 'W'],
  [1, 'BTU', 'J'],
  [1, 'kWh', 'J'],
  [100, 'ft2', 'm2'],
  [1, 'ha', 'm2'],
  [1, 'm3', 'l'],
  [1, 'kbyte', 'bit'],
  [1, 'Gibyte', 'Mibyte'],
  [1, 'mph', 'm/s'],
  [1, 'kn', 'km/hr'],
  [1, 'T', 'ga'],
  [1, 'N', 'dyn'],
  [1, 'ozm', 'g'],
  [1, 'ton', 'kg'],
  [10, 'Rank', 'K'],
  [1, 'xyz', 'm'],
  [1, 'cup', 'tsp'],
  [1, 'nmi', 'm'],
])
  add('engineering', `=CONVERT(${v},${q(a)},${q(b)})`);

// Database
for (const f of [
  'DSUM',
  'DAVERAGE',
  'DCOUNT',
  'DCOUNTA',
  'DMAX',
  'DMIN',
  'DPRODUCT',
  'DSTDEV',
  'DSTDEVP',
  'DVAR',
  'DVARP',
  'DGET',
])
  for (const [field, crit] of [
    ['"Yield"', 'Data!N1:P2'],
    ['"Profit"', 'Data!N1:O3'],
    ['"Height"', 'Data!N5:O6'],
    ['3', 'Data!N8:N9'],
    ['"Age"', 'Data!N1:N3'],
  ])
    add('database', `=${f}(Data!H1:L7,${field},${crit})`);

// Web
add(
  'web',
  '=ENCODEURL("http://contoso.sharepoint.com/Finance/Profit and Loss Statement.xlsx")',
  '=ENCODEURL("a b&c=d")',
  '=ENCODEURL("ü~-_.!*\'()")',
);

// ── deny-list: cases where LibreOffice 24.2 is known to differ from Excel ─────────────────────────

const DENY = [];
function deny(re, reason) {
  DENY.push({ re, reason });
}
// (populated below after review of differences; every entry is also listed in docs/formula-deviations.md)
import('./conformance-deny.mjs')
  .then((m) => {
    for (const d of m.DENY) deny(d.re, d.reason);
  })
  .then(main)
  .catch((e) => {
    console.error(e);
    process.exit(1);
  });

// ── workbook, oracle, parsing ────────────────────────────────────────────

function a1(addr) {
  const m = /^([A-Z]+)(\d+)$/.exec(addr);
  let col = 0;
  for (const ch of m[1]) col = col * 26 + ch.charCodeAt(0) - 64;
  return { row: Number(m[2]), col };
}

function parseCsv(text) {
  const rows = [];
  let row = [];
  let cur = '';
  let inQ = false;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (inQ) {
      if (ch === '"') {
        if (text[i + 1] === '"') {
          cur += '"';
          i++;
        } else inQ = false;
      } else cur += ch;
    } else if (ch === '"') inQ = true;
    else if (ch === ',') {
      row.push(cur);
      cur = '';
    } else if (ch === '\n') {
      row.push(cur);
      rows.push(row);
      row = [];
      cur = '';
    } else if (ch !== '\r') cur += ch;
  }
  if (cur !== '' || row.length) {
    row.push(cur);
    rows.push(row);
  }
  return rows;
}

function csvField(s) {
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

async function main() {
  const fixture = JSON.parse(readFileSync(join(CONF, 'fixture.json'), 'utf8'));
  const cases = [];
  for (const [family, list] of Object.entries(fam)) for (const f of list) cases.push({ family, formula: f });
  const seen = new Set();
  const unique = cases.filter((c) => {
    if (seen.has(c.formula)) return false;
    seen.add(c.formula);
    return true;
  });

  const wb = new ExcelJS.Workbook();
  const calc = wb.addWorksheet('Calc');
  const data = wb.addWorksheet('Data');
  for (const [addr, v] of Object.entries(fixture.cells)) {
    const { row, col } = a1(addr);
    const cell = data.getCell(row, col);
    if (typeof v === 'string' && v.startsWith('=')) cell.value = { formula: fileFormula(v) };
    else if (typeof v === 'string' && v.startsWith("'")) cell.value = v.slice(1);
    else cell.value = v;
  }
  unique.forEach((c, i) => {
    const r = i + 1;
    calc.getCell(r, 1).value = { formula: fileFormula(c.formula) };
    calc.getCell(r, 2).value = {
      formula: `IF(ISERROR(A${r}),"e",IF(ISLOGICAL(A${r}),"b",IF(ISTEXT(A${r}),"s",IF(ISNUMBER(A${r}),"n","x"))))`,
    };
    calc.getCell(r, 3).value = { formula: `IF(ISNUMBER(A${r}),TEXT(A${r},"0.00000000000000E+000"),"")` };
  });

  const work = mkdtempSync(join(tmpdir(), 'kiln-conformance-'));
  const xlsx = join(work, 'conformance.xlsx');
  await wb.xlsx.writeFile(xlsx);
  const filter = 'csv:Text - txt - csv (StarCalc):44,34,76,1,,1033,false,true,false,false,false';
  const res = spawnSync('bash', [join(REPO, 'tools/oracle-soffice.sh'), xlsx, filter, work], {
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    console.error(res.stdout, res.stderr);
    throw new Error('LibreOffice conversion failed');
  }
  const rows = parseCsv(readFileSync(join(work, 'conformance.csv'), 'utf8'));

  const out = {};
  const excluded = [];
  unique.forEach((c, i) => {
    const [value = '', tag = '', sci = ''] = rows[i] ?? [];
    const denied = DENY.find((d) => d.re.test(c.formula));
    if (denied) {
      excluded.push({ formula: c.formula, lo: value, reason: denied.reason });
      return;
    }
    let expected;
    let type;
    if (tag === 'n') {
      expected = String(Number(sci));
      type = 'number';
    } else if (tag === 'b') {
      expected = value.toUpperCase();
      type = 'boolean';
    } else if (tag === 's') {
      expected = value;
      type = 'text';
    } else if (tag === 'x') {
      excluded.push({
        formula: c.formula,
        lo: value,
        reason: 'LibreOffice returns an empty result (Excel shows 0)',
      });
      return;
    } else {
      // LibreOffice-only codes (Err:502 …) map to different Excel errors per function: excluded.
      const code = value;
      if (!code.startsWith('#')) {
        excluded.push({ formula: c.formula, lo: value, reason: 'LibreOffice-specific error code' });
        return;
      }
      if (code === '#NAME?') {
        excluded.push({
          formula: c.formula,
          lo: value,
          reason: 'function not available in LibreOffice 24.2',
        });
        return;
      }
      expected = code;
      type = 'error';
    }
    out[c.family] ??= [];
    out[c.family].push([c.formula, expected, type, 'LibreOffice 24.2 oracle']);
  });

  for (const f of readdirSync(CONF)) if (/^generated-.*\.csv$/.test(f)) rmSync(join(CONF, f));
  let total = 0;
  for (const [family, list] of Object.entries(out)) {
    const lines = ['formula,expected,type,notes', ...list.map((r) => r.map(csvField).join(','))];
    writeFileSync(join(CONF, `generated-${family}.csv`), `${lines.join('\n')}\n`);
    total += list.length;
    console.log(`generated-${family}.csv: ${list.length} rows`);
  }
  const ex = [
    'formula,libreoffice,reason',
    ...excluded.map((e) => [e.formula, e.lo, e.reason].map(csvField).join(',')),
  ];
  writeFileSync(join(CONF, 'oracle-exclusions.csv'), `${ex.join('\n')}\n`);
  console.log(`${total} generated rows, ${excluded.length} excluded (see oracle-exclusions.csv)`);
  if (keep) console.log(`work directory: ${work}`);
  else rmSync(work, { recursive: true, force: true });
}
