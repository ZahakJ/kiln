# Formula deviations

This page lists where Kiln's formula engine knowingly behaves differently from Microsoft Excel, and
where the LibreOffice oracle used by the conformance suite differs from Excel. The engine and its
484-function library are Rust (`crates/kiln-formula`: the core from WP-56, the library in
`src/functions/` from WP-57), ported from the v1.0 TypeScript engine kept read-only in
`reference/ts-core/formula/`. The port keeps every name, arity, volatility and lazy or lifted
argument of v1.0 (`tests/conformance_registry.rs` checks them against a snapshot of the v1.0
registry) and reproduces the entries below; where the port changed a behaviour, the entry says so.
The conformance suite lives in `conformance/` and runs with
`cargo nextest run -p kiln-formula --test conformance` (`tests/conformance.rs`, 4,879 rows, 0
failures):

- `curated-*.csv` rows are Microsoft's documented examples (copied from the function documentation in
  `crates/kiln-formula/docs/*.json`) and hand-written Excel behaviours and edge cases.
  `curated-dynamic-arrays.csv` (190 rows) and `curated-lambda.csv` (89 rows) were added with the Rust
  port: `SEQUENCE`, `FILTER`, `SORT`/`SORTBY`, `UNIQUE`, `TAKE`/`DROP`, `HSTACK`/`VSTACK`,
  `TOCOL`/`TOROW`, `WRAPROWS`/`WRAPCOLS`, `EXPAND`, `CHOOSEROWS`/`CHOOSECOLS`, `XLOOKUP`/`XMATCH`
  (including references returned by `XLOOKUP`), `TEXTSPLIT`/`TEXTBEFORE`/`TEXTAFTER`, array
  broadcasting, and `LET`, `LAMBDA`, `MAP`, `REDUCE`, `SCAN`, `BYROW`, `BYCOL`, `MAKEARRAY`, closures,
  currying and recursion by passing a lambda to itself.
- `generated-*.csv` rows are produced by `reference/ts-core/formula/tools/gen-conformance.mjs`, which writes the
  formulas into an `.xlsx` with exceljs, lets LibreOffice 24.2 recalculate it headless
  (`tools/oracle-soffice.sh … csv`), and records the values. Rows where LibreOffice is known to differ
  from Excel are excluded through the deny-list in `reference/ts-core/formula/tools/conformance-deny.mjs`; every
  excluded row is listed with its reason in `conformance/oracle-exclusions.csv`.

The runner compares numbers and date serials to 1e-9 relative (rows marked `rounded` — Microsoft's
examples print rounded values — to their displayed precision), errors by name, and text and logicals
exactly. **Skipped rows:** none. A row typed `skip` must have its formula quoted in this file;
`tests/conformance.rs` fails otherwise. Every documented example in `crates/kiln-formula/docs/*.json`
(866 of them) is evaluated by `tests/docs.rs` as well.

## Implementation note

The plan proposed `@formulajs/formulajs` as the implementation base for scalar functions. Every function
was instead implemented natively in the v1.0 engine (now `reference/ts-core/formula/src/functions/`): formulajs works with JavaScript
`Date` objects (time-zone dependent, no Lotus 1900-02-29), its own error objects and its own text and
logical coercion, so each function would have needed rewrapping or replacing anyway, and the conformance
suite checks the native implementations directly. This also removes formulajs and its `jstat` and
`bessel` dependencies from the shipped bundle.

The Rust port keeps the native implementations. The gamma function, its logarithm and the error
function come from `statrs`; the regularised incomplete beta and gamma functions are Kiln's own
(continued fractions without `statrs`' 140-iteration cap, which loses digits for
`BINOM.DIST(4900,10000,0.5,TRUE)`), and the distribution rows of `generated-dist.csv` check them to
1e-9 relative. `DOLLARDE` and `DOLLARFR` use `rust_decimal` where v1.0 used decimal.js.

## Deviations from Excel

These are behaviours where Kiln is known, or not verified, to match Excel.

| Area | Kiln | Excel | Why |
|---|---|---|---|
| `HYPERLINK` | The cell value is the friendly name (or the link); the target is recorded on the formula cell and exposed by `Engine::hyperlink(sheet, row, col)`. | A cell showing a clickable link. | A separate "link" scalar type would ripple through every function's type checks; the grid reads the target from the engine instead. Nested uses (`=IF(x, HYPERLINK(…), "…")`) record the target only when the HYPERLINK branch is evaluated. |
| `CELL` | Supports `address`, `col`, `row`, `contents`, `type` and `filename` (always `""`). Other info types return `#VALUE!`. Without a reference it describes the formula cell. Addresses on other sheets read `Sheet!$A$1` without a `[Book]` prefix. | Many more info types; without a reference it uses the last changed cell. | Kiln documents have no file path exposed to formulas, and formatting information lives outside the engine. |
| `INFO` | `numfile` (sheets in the workbook), `recalc` ("Automatic"), `release` ("16.0"), `system` ("pcdos"), `osversion` ("Kiln"), `origin` ("$A:$A$1"); `directory` is `#N/A`. | Values of the running Excel. | Formulas must not leak information about the user's machine. |
| `REGEXTEST`, `REGEXEXTRACT`, `REGEXREPLACE` | Perl/PCRE-style regular expressions (`fancy-regex`): classes, Unicode, named groups, look-ahead and look-behind, back-references, atomic groups and possessive quantifiers. A search that needs more than 1,000,000 backtracking steps is `#VALUE!`. Replacements take `$1`, `${1}`, `${name}` and `$$` (PCRE2) and also `$&`, `$<name>`, `` $` `` and `$'` (as v1.0 did). `REGEXEXTRACT` mode 1 returns a column. | PCRE2 with no step limit. | Common patterns behave the same; a few PCRE2-only constructs (`\K`, recursion `(?R)`, callouts, verbs such as `(*SKIP)`) are `#VALUE!`. The step limit keeps a catastrophic pattern from freezing recalculation. v1.0 used ECMAScript expressions, which lacked atomic groups and possessive quantifiers. |
| `CHAR`, `CODE` | Windows-1252 on every platform. | Windows-1252 on Windows, Mac Roman on macOS. | One behaviour for every platform. |
| `LENB`, `LEFTB`, `RIGHTB`, `MIDB`, `FINDB`, `SEARCHB`, `REPLACEB` | Count characters. | Count bytes when a double-byte language is the default editing language. | Matches Excel in single-byte locales. |
| `TEXT`, `DOLLAR`, `FIXED`, `VALUE`, `DATEVALUE` | Number formats and text parsing use en-US conventions (`.` decimals, `,` thousands, `$`, month-first dates). | Follow the system locale. | Locale-aware formatting arrives with the i18n work; `NUMBERVALUE` already takes explicit separators. |
| `DATEDIF(…, "MD")` | Days left after moving the start date forward by the whole months between the dates, clamped to the month end: `DATEDIF(DATE(2020,1,31),DATE(2020,3,1),"md")` is 1. | Microsoft warns that "MD" "may result in a negative number, a zero, or an inaccurate result". | A defined, non-negative answer. |
| `BESSELK` (and to a lesser degree the other Bessel functions) | Integral representations evaluated with Gauss–Legendre quadrature, accurate to about 1e-14. | Polynomial approximations (about 1e-8 relative; Microsoft's `BESSELK(1.5,1)` example shows 0.277387804 where the exact value is 0.2773878004…). | Accuracy. Results agree with Excel to about 8 significant digits. |
| `CONVERT` | Exact unit definitions (international pound and ounce, standard atmosphere, mechanical horsepower, IT BTU, short ton …). | Not verified for every unit. | LibreOffice uses legacy factors for six units (see below); Kiln uses the defining values. |
| `ACCRINT` | Interest accrued from `issue` to `settlement` with `YEARFRAC(issue, settlement, basis)`; `first_interest` and `calc_method` are validated but do not change the result. | Sums coupon-period fractions, which differs slightly for odd first periods. | Excel's algorithm for irregular first periods is not documented. |
| `INTRATE`, `RECEIVED`, `PRICEDISC`, `ACCRINTM` with basis 1 | Day count divided by the length of the settlement (or issue) year; `DISC` and `YIELDDISC` use `YEARFRAC`. | "B = number of days in a year, depending on the year basis". | Follows LibreOffice's Excel-compatible implementation; not verified against Excel for multi-year spans. |
| `TBILLEQ` over 182 days | The bond-equivalent yield from the quadratic for a full coupon period. | Formula not documented for this range. | Standard money-market formula. |
| `HOUR`, `MINUTE`, `SECOND` within half a second of midnight | Rounded to the nearest second without wrapping into the next day: `0.999999` is 23:59:59. | Not verified. | — |
| `SUBTOTAL`, `AGGREGATE` (functions 1–13) | Also accept arrays (no hidden-row or nested-subtotal checks apply to them). | Require references. | Lenient; never changes a result Excel would compute. |
| `LINEST`, `LOGEST`, `TREND`, `GROWTH` | Collinear predictors give `#NUM!`. | Removes redundant columns and reports a zero coefficient. | Not implemented yet. |
| `INDIRECT`, references | References to other workbooks give `#REF!`. | Resolve open workbooks. | Kiln has no cross-workbook references. |
| Circular references | Cells in a cycle show 0 and are flagged (`Engine::is_circular`, `circular_cells`), unless iterative calculation is on. A cycle between defined names (`X=Y+1`, `Y=X+1`) evaluates to `#NAME?` (the name-nesting limit) instead of being reported as circular. | Shows 0 and warns; reports name cycles as circular references. | The plan text mentioned a `#CIRCULAR` error; Kiln follows Excel instead. Name cycles are rare and still never hang. |
| Number formats (`TEXT`, cell display) | Format `0` shows `0` for -0.4; `General` shows -0.0000001 and 0.000012345 in decimal. | May show `-0`, and `-1E-07` / `1.2345E-05` (not verified). | Follows the numfmt oracle of v1.0; LibreOffice 24.2 agrees. |
| `LET` names that are function names | `=LET(fact,LAMBDA(…),fact(fact,5))` calls the built-in `FACT`, so it is `#VALUE!` here. | Not verified. | A name followed by `(` resolves to a worksheet function first; name `LET` variables that are not function names (`fac`). The conformance rows do. |
| Huge arguments | `MULTINOMIAL`, the Bessel functions and other functions with loops over an argument are computed in bounded time: they overflow to `#NUM!` (or underflow to 0) instead of iterating up to 10^308 times; dates beyond 9999-12-31 in the date, `YEARFRAC` and security functions are `#NUM!`. | Same results where Excel gives one. | `tests/conformance_fuzz.rs` calls every function with every arity and hostile arguments and requires an answer without a panic or a stall. v1.0 could hang on `MULTINOMIAL(1E+308)` and `BESSELJ(1,1E+15)`. |
| Not implemented | `GROUPBY`, `PIVOTBY`, `PERCENTOF`, `TRIMRANGE`, `IMAGE`, `WEBSERVICE`, `FILTERXML`, `CUBE*`, `RTD`, `CALL`, `REGISTER.ID`, `EUROCONVERT`, `AMORDEGRC`, `AMORLINC`, `ODDFPRICE`, `ODDFYIELD`, `ODDLPRICE`, `ODDLYIELD`, `STOCKHISTORY`, `GETPIVOTDATA`, `ASC`, `JIS`, `DBCS`, `BAHTTEXT`, `PHONETIC`, `TRANSLATE`, `DETECTLANGUAGE`, `PY`. | Available. | They return `#NAME?`. Pivot tables, external data and Python are outside the v0.1 scope; the odd-period bond and French depreciation functions are planned. |

The WP-09 acceptance text asks for `=IRR({-100,30,40,50})` "within 1e-7 of 0.0894". The internal rate
of return of those cash flows is 0.0889633947 (the net present value at 8.94% is −0.084, not 0); that
is what Kiln, Excel and LibreOffice return, and what the spot-check test asserts.

## Where LibreOffice differs from Excel

These groups of LibreOffice 24.2 results are excluded from the generated rows. Kiln follows Excel,
and the curated rows (`Excel behaviour` notes) check Excel's answer for each group. Entries marked
"(not verified)" state Excel's expected behaviour from its documentation and conventions but have not
been re-checked in a copy of Excel for this suite.

| Group | Rows | LibreOffice | Excel (and Kiln) |
|---|---|---|---|
| Logical values in ranges | 41 | Counts TRUE/FALSE as 1/0 in `SUM`, `AVERAGE`, `STDEV`, `CORREL` … and numeric criteria; `ISNUMBER(TRUE)` is TRUE, `TYPE(TRUE)` is 1. | Ignores logicals in ranges; `ISNUMBER(TRUE)` is FALSE, `TYPE(TRUE)` is 4. |
| Logical values as text | 15 | `LEN(TRUE)` is 1, `TRUE&""` is "1". | "TRUE"/"FALSE". |
| `PERCENTRANK`, `PERCENTRANK.INC`, `PERCENTRANK.EXC` | 36 | Rounds, and counts significant digits. | Truncates to decimal places (Microsoft's example: 0.555 for 5/9). |
| `DURATION`, `MDURATION`, bases 1–3 | 24 | Times cash flows with `YEARFRAC`. | Times them by coupon periods; reproduces Microsoft's example 10.9191453. |
| `BESSELK` | 16 | A&S polynomial approximation. | See the deviation above. |
| `TEXT` formats | 14 | `TEXT(0,"0.###")` is "0", serial 0 is 1899-12-30, negative dates format, `hh:mm:ss` truncates. | "0.", 1900-01-00, `#VALUE!` (not verified), rounds to the second. |
| `DOLLAR` negative amounts | 10 | "-$1,234.57". | "($1,234.57)" (Microsoft's example). |
| Text functions | 7 | `CODE("")` is 0, `CODE("€")` is 226, `FIND("","abc")` is `#VALUE!`, `NUMBERVALUE("3.5%")` is `#VALUE!`. | `#VALUE!`, 128, 1 (start_num), 0.035. |
| `WEEKNUM` of 31 December 2023, return types 1 and 12–17 | 7 | 1. | 53. |
| `MODE`, `MODE.SNGL`, `AGGREGATE(13,…)` with no repeated value | 6 | `#VALUE!`. | `#N/A`. |
| `CONVERT` units lbm, ozm, ton, atm, HP, BTU | 6 | Legacy factors (e.g. 0.453592309748811 kg per lbm). | Exact (0.45359237). |
| Error values for invalid input (`COT(0)`, `CSC(0)`, `COTH(0)`, `CSCH(0)`, `ERROR.TYPE(SQRT(-1))`) | 5 | `#NUM!`, or `#N/A` for its internal error 502. | `#DIV/0!`, 6. |
| Dates before 1 March 1900 | 5 | Proleptic Gregorian calendar; `DATE(108,1,2)` is `#VALUE!`. | Lotus 1-2-3 calendar with 1900-02-29; `DATE(108,1,2)` is 2008-01-02. |
| `LARGE`, `SMALL` with k beyond the data | 4 | `#VALUE!`. | `#NUM!`. |
| `HOUR`/`MINUTE`/`SECOND(0.999999)` | 3 | 23, 59, 0. | See the deviation above. |
| `ENCODEURL` | 3 | Encodes "." and "~". | Leaves the unreserved characters `- _ . ~` (Microsoft's example). |
| `MROUND` with mixed signs | 2 | Rounds. | `#NUM!` (documented). |
| `PROB` with a column of values and a row of probabilities | 2 | `#N/A`. | Only the counts must match (not verified). |
| T-bills (`TBILLPRICE`, `TBILLEQ` over 182 days) | 2 | 30/360 day count. | Actual days (not verified beyond Microsoft's examples). |
| `DSTDEV`, `DVAR` with one matching record | 2 | `#NUM!`. | `#DIV/0!` (not verified). |
| `SUMSQ(3,4,"5")` | 1 | `#VALUE!`. | 50 (numbers typed as text count, documented). |
| `COUNTIF` wildcard on numbers | 1 | Matches numbers. | Wildcards only match text. |
| `TYPE` of a formula cell | 1 | 8. | The type of the result. |
| `AND(TRUE,1,"TRUE")` | 1 | `#VALUE!`. | TRUE (text logicals typed directly are converted; not verified). |
| `IMSUM("1+i","1+j")` | 1 | "2+2i". | `#VALUE!` (mixed suffixes). |
| `IFERROR`/`IFNA` of an empty cell | 2 | An empty result. | 0. |

Rows LibreOffice cannot answer at all are excluded automatically: functions it does not read from
`.xlsx` files (93 rows, e.g. `XLOOKUP`, `LET`, `TEXTSPLIT`, `NETWORKDAYS.INTL`, `IMCOSH` and other
`_xlfn.` functions in LibreOffice 24.2) and LibreOffice-only error codes such as `Err:502` (63 rows),
which map to different Excel errors depending on the function. The curated rows cover those functions.
