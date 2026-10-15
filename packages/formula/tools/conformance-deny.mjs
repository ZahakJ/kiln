/**
 * Deny-list for the LibreOffice-generated conformance rows: formulas where LibreOffice 24.2 is known
 * to return something different from Excel. Matching rows are excluded from generated-*.csv and listed
 * with their reason in test/conformance/oracle-exclusions.csv; every reason is explained in
 * docs/formula-deviations.md ("Where LibreOffice differs from Excel"). Excel's behaviour for these
 * cases is covered by the curated rows instead.
 */

/** Matches formulas exactly (escaped), so the list cannot hide unrelated rows. */
function exact(...formulas) {
  return formulas.map((f) => new RegExp(`^${f.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`));
}

const LOGICALS_AS_NUMBERS =
  'LibreOffice has no separate logical type: TRUE/FALSE in ranges count as 1/0 in numeric aggregates and numeric criteria, ISNUMBER(TRUE) is TRUE and TYPE(TRUE) is 1. Excel ignores logicals in ranges and keeps them logical.';
const LOGICALS_AS_TEXT = 'LibreOffice converts a logical used as text to "1"/"0"; Excel uses "TRUE"/"FALSE".';
const LO_ERROR = 'LibreOffice returns a different error value than Excel for this invalid input.';

const groups = [
  {
    reason: LOGICALS_AS_NUMBERS,
    patterns: [
      /^=(AVERAGE|MEDIAN|MODE|MODE\.SNGL|STDEV|STDEV\.S|STDEV\.P|STDEVP|VAR|VAR\.S|VAR\.P|VARP|AVEDEV|DEVSQ|KURT|SKEW|SKEW\.P|COUNT|GEOMEAN|HARMEAN|AVERAGEA|STDEVA|STDEVPA|VARA|VARPA|SUM)\(Data!D1:D10\)$/,
      /^=(CORREL|PEARSON|RSQ|COVARIANCE\.P|COVARIANCE\.S|COVAR|SLOPE|INTERCEPT|STEYX)\(Data!E1:E10,Data!D1:D10\)$/,
      ...exact('=COUNTIF(Data!D1:D10,">0")', '=COUNTIF(Data!D1:D10,"<>0")'),
      ...exact('=ISNUMBER(TRUE)', '=ISNUMBER(FALSE)', '=TYPE(TRUE)', '=TYPE(FALSE)'),
    ],
  },
  {
    reason: LOGICALS_AS_TEXT,
    patterns: [
      /^=(LEN|UPPER|LOWER|PROPER|TRIM|CLEAN|CODE)\(TRUE\)$/,
      /^=(LEFT|RIGHT|MID|REPT|SUBSTITUTE)\(TRUE,/,
      ...exact('=CONCATENATE("a",1,TRUE,2.5)', '="a"&1.5&FALSE', '=TEXT(TRUE,"0")'),
    ],
  },
  {
    reason: 'LibreOffice applies wildcards to numbers; Excel wildcard criteria only match text.',
    patterns: exact('=COUNTIF(Data!C1:C10,"?????")'),
  },
  {
    reason:
      'Excel returns #DIV/0! when a sample statistic (DSTDEV, DVAR) has a single matching record; LibreOffice returns #NUM!.',
    patterns: exact('=DSTDEV(Data!H1:L7,"Yield",Data!N1:P2)', '=DVAR(Data!H1:L7,"Yield",Data!N1:P2)'),
  },
  {
    reason:
      'Excel rejects complex numbers that mix the "i" and "j" suffixes (#VALUE!); LibreOffice accepts them.',
    patterns: exact('=IMSUM("1+i","1+j")'),
  },
  {
    reason:
      'LibreOffice uses legacy conversion factors for these units; Officina uses the exact definitions (international pound and ounce, standard atmosphere, mechanical horsepower, IT BTU, short ton).',
    patterns: exact(
      '=CONVERT(1,"lbm","kg")',
      '=CONVERT(1,"atm","Pa")',
      '=CONVERT(1,"HP","W")',
      '=CONVERT(1,"BTU","J")',
      '=CONVERT(1,"ozm","g")',
      '=CONVERT(1,"ton","kg")',
    ),
  },
  {
    reason:
      'Excel and LibreOffice evaluate BESSELK with polynomial approximations (about 1e-8 relative); Officina computes it to about 1e-14, so results differ beyond the 1e-9 tolerance.',
    patterns: [/^=BESSELK\(/],
  },
  {
    reason:
      'Excel converts text that reads as a logical ("TRUE") when typed directly into AND/OR/XOR; LibreOffice returns #VALUE!.',
    patterns: exact('=AND(TRUE,1,"TRUE")'),
  },
  {
    reason:
      'Excel returns #N/A when no value repeats (MODE, MODE.SNGL, AGGREGATE 13); LibreOffice returns #VALUE!.',
    patterns: [
      ...exact(
        '=MODE.SNGL(Data!A1:A10)',
        '=MODE.SNGL(Data!E1:E10)',
        '=MODE(Data!A1:A10)',
        '=MODE(Data!E1:E10)',
      ),
      ...exact('=AGGREGATE(13,6,Data!G1:G4)', '=AGGREGATE(13,4,Data!E1:E10)'),
    ],
  },
  {
    reason:
      'Excel returns #NUM! when k is larger than the number of values (LARGE, SMALL); LibreOffice returns #VALUE!.',
    patterns: [/^=(LARGE|SMALL)\(Data![EF]1:[EF]10,11\)$/],
  },
  {
    reason:
      "Excel truncates PERCENTRANK results to the requested number of decimal places (0.5555… → 0.555, as in Microsoft's examples); LibreOffice rounds and counts significant digits.",
    patterns: [/^=PERCENTRANK(\.INC|\.EXC)?\(/],
  },
  {
    reason:
      'LibreOffice requires x_range and prob_range of PROB to have the same shape; Excel only requires the same number of values.',
    patterns: [/^=PROB\(Data!R1:R5,\{/],
  },
  {
    reason:
      'Excel: CODE("") is #VALUE!, CODE("€") is 128 (Windows-1252), FIND/SEARCH with empty find_text return start_num, NUMBERVALUE accepts a trailing %. LibreOffice differs.',
    patterns: [...exact('=CODE("")', '=CODE("€")', '=NUMBERVALUE("3.5%")'), /^=(FIND|SEARCH)\("","abc"/],
  },
  {
    reason:
      "Excel (en-US) formats negative DOLLAR amounts in parentheses, as in Microsoft's examples; LibreOffice uses a minus sign.",
    patterns: [/^=DOLLAR\(-/],
  },
  {
    reason:
      'Excel number formats: "0.###" keeps the decimal point ("0."), serial 0 is 1900-01-00, negative values cannot be formatted as dates or times (#VALUE!), and hh:mm:ss rounds to the nearest second. LibreOffice differs.',
    patterns: [
      ...exact('=TEXT(0,"0.###")', '=TEXT(1234.5678,"hh:mm:ss")'),
      /^=TEXT\(0,"(yyyy-mm-dd|dd\/mm\/yyyy|mmm d, yyyy|mmmm)"\)$/,
      /^=TEXT\(-42\.5,"(yyyy-mm-dd|dd\/mm\/yyyy|mmm d, yyyy|dddd|mmmm|h:mm AM\/PM|hh:mm:ss|\[h\]:mm)"\)$/,
    ],
  },
  {
    reason:
      'Excel\'s ENCODEURL leaves the unreserved characters - _ . ~ unencoded (Microsoft\'s example); LibreOffice encodes "." and "~".',
    patterns: [/^=ENCODEURL\(/],
  },
  {
    reason:
      'Excel WEEKNUM (system 1) numbers 31 December 2023 as week 53; LibreOffice returns 1 for these return types.',
    patterns: [/^=WEEKNUM\(DATE\(2023,12,31\),(1|1[2-7])\)$/],
  },
  {
    reason:
      'Dates before 1 March 1900: Excel keeps the Lotus 1-2-3 calendar (1900-02-29 exists, 1900-01-01 is a Sunday, EDATE before 1900 is #NUM!, DATE(108,1,2) is 2008-01-02); LibreOffice uses the proleptic Gregorian calendar.',
    patterns: [
      ...exact('=EDATE(DATE(1900,3,1),-13)', '=DAY(60)', '=DATE(108,1,2)'),
      /^=WEEKNUM\(DATE\(1900,3,1\),(2|11)\)$/,
    ],
  },
  {
    reason:
      "Times within half a second of midnight: Officina rounds to the nearest second without wrapping into the next day (23:59:59); LibreOffice returns 23, 59 and 0. Excel's result for this edge case is not verified.",
    patterns: [/^=(HOUR|MINUTE|SECOND)\(0\.999999\)$/],
  },
  {
    reason: LO_ERROR,
    patterns: [...exact('=ERROR.TYPE(SQRT(-1))', '=COT(0)', '=CSC(0)', '=COTH(0)', '=CSCH(0)')],
  },
  {
    reason:
      'Excel MROUND returns #NUM! when number and multiple have different signs (documented); LibreOffice rounds anyway.',
    patterns: exact('=MROUND(-2.5,2)', '=MROUND(2.5,-2)'),
  },
  {
    reason:
      'Excel counts numbers typed as text directly in the argument list of SUMSQ (documented); LibreOffice returns #VALUE!.',
    patterns: exact('=SUMSQ(3,4,"5")'),
  },
  {
    reason:
      "LibreOffice computes DURATION/MDURATION from YEARFRAC for bases 1–3; Excel times cash flows by coupon periods (DSC/E), which reproduces Microsoft's documented DURATION example (10.9191453).",
    patterns: [/^=M?DURATION\(.*,[123]\)$/],
  },
  {
    reason:
      'LibreOffice counts Treasury-bill days with a 30/360 calendar (and the simple formula beyond 182 days); Excel uses actual days.',
    patterns: exact(
      '=TBILLEQ(DATE(2008,3,31),DATE(2008,12,1),0.0914)',
      '=TBILLPRICE(DATE(2008,3,31),DATE(2009,3,1),0.05)',
    ),
  },
  {
    reason:
      "LibreOffice's TYPE() returns 8 for a cell holding a formula; Excel returns the type of the result.",
    patterns: exact('=TYPE(Data!B6)'),
  },
];

export const DENY = groups.flatMap((g) => g.patterns.map((re) => ({ re, reason: g.reason })));
