# Formula deviations

This page lists where Officina's formula engine (`@officina/formula`) knowingly behaves differently
from Microsoft Excel, and where the LibreOffice oracle used by the conformance suite differs from
Excel. It is kept current by the conformance suite in `packages/formula/test/conformance/`:

- `curated-*.csv` rows are Microsoft's documented examples (copied from the function documentation in
  `packages/formula/src/functions/docs/*.json`) and hand-written Excel behaviours and edge cases.
- `generated-*.csv` rows are produced by `packages/formula/tools/gen-conformance.mjs`, which writes the
  formulas into an `.xlsx` with exceljs, lets LibreOffice 24.2 recalculate it headless
  (`tools/oracle-soffice.sh … csv`), and records the values. Rows where LibreOffice is known to differ
  from Excel are excluded through the deny-list in `packages/formula/tools/conformance-deny.mjs`; every
  excluded row is listed with its reason in `test/conformance/oracle-exclusions.csv`.

The runner compares numbers and date serials to 1e-9 relative (rows marked `rounded` — Microsoft's
examples print rounded values — to their displayed precision), errors by name, and text and logicals
exactly. **Skipped rows:** none. A row typed `skip` must have its formula quoted in this file; the test
`documents every skipped row in docs/formula-deviations.md` enforces that.

## Implementation note

The plan proposed `@formulajs/formulajs` as the implementation base for scalar functions. Every function
is instead implemented natively in `packages/formula/src/functions/`: formulajs works with JavaScript
`Date` objects (time-zone dependent, no Lotus 1900-02-29), its own error objects and its own text and
logical coercion, so each function would have needed rewrapping or replacing anyway, and the conformance
suite checks the native implementations directly. This also removes formulajs and its `jstat` and
`bessel` dependencies from the shipped bundle.

## Deviations from Excel

These are behaviours where Officina is known, or not verified, to match Excel.

| Area | Officina | Excel | Why |
|---|---|---|---|
| `HYPERLINK` | The cell value is the friendly name (or the link); the target is recorded on the formula cell and exposed by `Engine.getHyperlink(sheet, row, col)`. | A cell showing a clickable link. | A separate "link" scalar type would ripple through every function's type checks; the grid reads the target from the engine instead. Nested uses (`=IF(x, HYPERLINK(…), "…")`) record the target only when the HYPERLINK branch is evaluated. |
| `CELL` | Supports `address`, `col`, `row`, `contents`, `type` and `filename` (always `""`). Other info types return `#VALUE!`. Without a reference it describes the formula cell. Addresses on other sheets read `Sheet!$A$1` without a `[Book]` prefix. | Many more info types; without a reference it uses the last changed cell. | Officina documents have no file path exposed to formulas, and formatting information lives outside the engine. |
| `INFO` | `numfile` (sheets in the workbook), `recalc` ("Automatic"), `release` ("16.0"), `system` ("pcdos"), `osversion` ("Officina"), `origin` ("$A:$A$1"); `directory` is `#N/A`. | Values of the running Excel. | Formulas must not leak information about the user's machine. |
| `REGEXTEST`, `REGEXEXTRACT`, `REGEXREPLACE` | ECMAScript regular expressions (`u` flag). `REGEXEXTRACT` mode 1 returns a column. | PCRE2. | The JavaScript engine's regular expressions; common patterns behave the same, PCRE-only syntax (possessive quantifiers, `\K` …) gives `#VALUE!`. |
| `CHAR`, `CODE` | Windows-1252 on every platform. | Windows-1252 on Windows, Mac Roman on macOS. | One behaviour for every platform. |
| `LENB`, `LEFTB`, `RIGHTB`, `MIDB`, `FINDB`, `SEARCHB`, `REPLACEB` | Count characters. | Count bytes when a double-byte language is the default editing language. | Matches Excel in single-byte locales. |
| `TEXT`, `DOLLAR`, `FIXED`, `VALUE`, `DATEVALUE` | Number formats and text parsing use en-US conventions (`.` decimals, `,` thousands, `$`, month-first dates). | Follow the system locale. | Locale-aware formatting arrives with the i18n work; `NUMBERVALUE` already takes explicit separators. |
| `DATEDIF(…, "MD")` | Days left after moving the start date forward by the whole months between the dates, clamped to the month end: `DATEDIF(DATE(2020,1,31),DATE(2020,3,1),"md")` is 1. | Microsoft warns that "MD" "may result in a negative number, a zero, or an inaccurate result". | A defined, non-negative answer. |
| `BESSELK` (and to a lesser degree the other Bessel functions) | Integral representations evaluated with Gauss–Legendre quadrature, accurate to about 1e-14. | Polynomial approximations (about 1e-8 relative; Microsoft's `BESSELK(1.5,1)` example shows 0.277387804 where the exact value is 0.2773878004…). | Accuracy. Results agree with Excel to about 8 significant digits. |
| `CONVERT` | Exact unit definitions (international pound and ounce, standard atmosphere, mechanical horsepower, IT BTU, short ton …). | Not verified for every unit. | LibreOffice uses legacy factors for six units (see below); Officina uses the defining values. |
| `ACCRINT` | Interest accrued from `issue` to `settlement` with `YEARFRAC(issue, settlement, basis)`; `first_interest` and `calc_method` are validated but do not change the result. | Sums coupon-period fractions, which differs slightly for odd first periods. | Excel's algorithm for irregular first periods is not documented. |
| `INTRATE`, `RECEIVED`, `PRICEDISC`, `ACCRINTM` with basis 1 | Day count divided by the length of the settlement (or issue) year; `DISC` and `YIELDDISC` use `YEARFRAC`. | "B = number of days in a year, depending on the year basis". | Follows LibreOffice's Excel-compatible implementation; not verified against Excel for multi-year spans. |
| `TBILLEQ` over 182 days | The bond-equivalent yield from the quadratic for a full coupon period. | Formula not documented for this range. | Standard money-market formula. |
| `HOUR`, `MINUTE`, `SECOND` within half a second of midnight | Rounded to the nearest second without wrapping into the next day: `0.999999` is 23:59:59. | Not verified. | — |
| `SUBTOTAL`, `AGGREGATE` (functions 1–13) | Also accept arrays (no hidden-row or nested-subtotal checks apply to them). | Require references. | Lenient; never changes a result Excel would compute. |
| `LINEST`, `LOGEST`, `TREND`, `GROWTH` | Collinear predictors give `#NUM!`. | Removes redundant columns and reports a zero coefficient. | Not implemented yet. |
| `INDIRECT`, references | References to other workbooks give `#REF!`. | Resolve open workbooks. | Officina has no cross-workbook references. |
| Not implemented | `GROUPBY`, `PIVOTBY`, `PERCENTOF`, `TRIMRANGE`, `IMAGE`, `WEBSERVICE`, `FILTERXML`, `CUBE*`, `RTD`, `CALL`, `REGISTER.ID`, `EUROCONVERT`, `AMORDEGRC`, `AMORLINC`, `ODDFPRICE`, `ODDFYIELD`, `ODDLPRICE`, `ODDLYIELD`, `STOCKHISTORY`, `GETPIVOTDATA`, `ASC`, `JIS`, `DBCS`, `BAHTTEXT`, `PHONETIC`, `TRANSLATE`, `DETECTLANGUAGE`, `PY`. | Available. | They return `#NAME?`. Pivot tables, external data and Python are outside the v0.1 scope; the odd-period bond and French depreciation functions are planned. |

The WP-09 acceptance text asks for `=IRR({-100,30,40,50})` "within 1e-7 of 0.0894". The internal rate
of return of those cash flows is 0.0889633947 (the net present value at 8.94% is −0.084, not 0); that
is what Officina, Excel and LibreOffice return, and what the spot-check test asserts.

## Where LibreOffice differs from Excel

These groups of LibreOffice 24.2 results are excluded from the generated rows. Officina follows Excel,
and the curated rows (`Excel behaviour` notes) check Excel's answer for each group. Entries marked
"(not verified)" state Excel's expected behaviour from its documentation and conventions but have not
been re-checked in a copy of Excel for this suite.

| Group | Rows | LibreOffice | Excel (and Officina) |
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
