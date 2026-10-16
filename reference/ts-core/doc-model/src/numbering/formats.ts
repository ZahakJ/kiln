/**
 * Number formats (`ST_NumberFormat`, ECMA-376 §17.18.59) used by list labels, page numbers and field
 * `\*` format switches. Unknown or unsupported formats fall back to decimal, as Word does for formats
 * a locale cannot render.
 */

export type NumberFormat =
  | 'decimal'
  | 'decimalZero'
  | 'upperRoman'
  | 'lowerRoman'
  | 'upperLetter'
  | 'lowerLetter'
  | 'ordinal'
  | 'cardinalText'
  | 'ordinalText'
  | 'hex'
  | 'chicago'
  | 'bullet'
  | 'none'
  | 'decimalEnclosedCircle'
  | 'decimalEnclosedFullstop'
  | 'decimalEnclosedParen'
  | 'decimalFullWidth'
  | 'decimalHalfWidth'
  | 'numberInDash'
  | 'russianLower'
  | 'russianUpper'
  | 'hebrew1'
  | 'hebrew2'
  | 'arabicAlpha'
  | 'arabicAbjad'
  | 'hindiNumbers'
  | 'thaiNumbers'
  | 'ideographDigital'
  | 'japaneseCounting'
  | 'chineseCounting'
  | 'chineseCountingThousand'
  | 'aiueo'
  | 'iroha'
  | 'aiueoFullWidth'
  | 'irohaFullWidth'
  | (string & {});

const ROMAN: [number, string][] = [
  [1000, 'M'],
  [900, 'CM'],
  [500, 'D'],
  [400, 'CD'],
  [100, 'C'],
  [90, 'XC'],
  [50, 'L'],
  [40, 'XL'],
  [10, 'X'],
  [9, 'IX'],
  [5, 'V'],
  [4, 'IV'],
  [1, 'I'],
];

/** Upper-case Roman numerals. Word renders 0 and negatives as decimal; values ≥ 4000 repeat M. */
export function toRoman(n: number): string {
  if (n <= 0) return String(n);
  let rest = n;
  let out = '';
  for (const [v, s] of ROMAN) {
    while (rest >= v) {
      out += s;
      rest -= v;
    }
  }
  return out;
}

/**
 * Word's alphabetic list numbering: A…Z, then AA…ZZ, AAA… (the letter repeats; it is *not* the
 * spreadsheet-column sequence). 0 renders as empty in Word; we return ''.
 */
export function toLetters(n: number, alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'): string {
  if (n <= 0) return '';
  const len = alphabet.length;
  const ch = alphabet[(n - 1) % len] as string;
  return ch.repeat(Math.floor((n - 1) / len) + 1);
}

const ONES = [
  'zero',
  'one',
  'two',
  'three',
  'four',
  'five',
  'six',
  'seven',
  'eight',
  'nine',
  'ten',
  'eleven',
  'twelve',
  'thirteen',
  'fourteen',
  'fifteen',
  'sixteen',
  'seventeen',
  'eighteen',
  'nineteen',
];
const TENS = ['', '', 'twenty', 'thirty', 'forty', 'fifty', 'sixty', 'seventy', 'eighty', 'ninety'];
const SCALES: [number, string][] = [
  [1_000_000_000, 'billion'],
  [1_000_000, 'million'],
  [1000, 'thousand'],
  [100, 'hundred'],
];

/** English cardinal text as Word writes it (`121` → `one hundred twenty-one`). */
export function toCardinalText(n: number): string {
  if (!Number.isFinite(n)) return String(n);
  if (n < 0) return `minus ${toCardinalText(-n)}`;
  const v = Math.floor(n);
  if (v < 20) return ONES[v] as string;
  if (v < 100) {
    const t = TENS[Math.floor(v / 10)] as string;
    return v % 10 ? `${t}-${ONES[v % 10]}` : t;
  }
  for (const [scale, word] of SCALES) {
    if (v >= scale) {
      const head = `${toCardinalText(Math.floor(v / scale))} ${word}`;
      const rest = v % scale;
      return rest ? `${head} ${toCardinalText(rest)}` : head;
    }
  }
  return String(v);
}

const ORDINAL_WORDS: Record<string, string> = {
  one: 'first',
  two: 'second',
  three: 'third',
  five: 'fifth',
  eight: 'eighth',
  nine: 'ninth',
  twelve: 'twelfth',
};

/** English ordinal text (`21` → `twenty-first`). */
export function toOrdinalText(n: number): string {
  const card = toCardinalText(n);
  const m = /([a-z]+)$/.exec(card);
  if (!m?.[1]) return card;
  const last = m[1];
  let ord: string;
  if (ORDINAL_WORDS[last]) ord = ORDINAL_WORDS[last] as string;
  else if (last.endsWith('y')) ord = `${last.slice(0, -1)}ieth`;
  else ord = `${last}th`;
  return card.slice(0, card.length - last.length) + ord;
}

/** English ordinal with suffix (`1st`, `2nd`, `11th`, `23rd`). */
export function toOrdinal(n: number): string {
  const abs = Math.abs(n);
  const mod100 = abs % 100;
  const mod10 = abs % 10;
  const suffix =
    mod100 >= 11 && mod100 <= 13 ? 'th' : mod10 === 1 ? 'st' : mod10 === 2 ? 'nd' : mod10 === 3 ? 'rd' : 'th';
  return `${n}${suffix}`;
}

const CHICAGO = ['*', '†', '‡', '§'];
/** Chicago Manual of Style footnote symbols: *, †, ‡, §, then doubled, tripled… */
export function toChicago(n: number): string {
  if (n <= 0) return '';
  const sym = CHICAGO[(n - 1) % 4] as string;
  return sym.repeat(Math.floor((n - 1) / 4) + 1);
}

const RUSSIAN = 'абвгдежзиклмнопрстуфхцчшщэюя';
const ARABIC_ALPHA = 'أبتثجحخدذرزسشصضطظعغفقكلمنهوي';
const ARABIC_ABJAD = 'أبجدهوزحطيكلمنسعفصقرشتثخذضظغ';
const IROHA =
  'いろはにほへとちりぬるをわかよたれそつねならむうゐのおくやまけふこえてあさきゆめみしゑひもせす';
const AIUEO = 'アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲン';
const CJK_DIGITS = '〇一二三四五六七八九';

function mapDigits(n: number, digits: string): string {
  return String(n).replace(/\d/g, (d) => digits[Number(d)] as string);
}

/** Japanese/Chinese counting (`23` → `二十三`). */
function toCjkCounting(n: number): string {
  if (n <= 0) return mapDigits(n, CJK_DIGITS);
  if (n >= 10000) return mapDigits(n, CJK_DIGITS);
  const units: [number, string][] = [
    [1000, '千'],
    [100, '百'],
    [10, '十'],
  ];
  let rest = n;
  let out = '';
  for (const [v, s] of units) {
    const d = Math.floor(rest / v);
    if (d) out += (d === 1 ? '' : CJK_DIGITS[d]) + s;
    rest %= v;
  }
  if (rest) out += CJK_DIGITS[rest];
  return out;
}

/** Hebrew numerals (gematria) for `hebrew1`. */
function toHebrew(n: number): string {
  if (n <= 0 || n >= 1000) return String(n);
  const hundreds = ['', 'ק', 'ר', 'ש', 'ת', 'תק', 'תר', 'תש', 'תת', 'תתק'];
  const tens = ['', 'י', 'כ', 'ל', 'מ', 'נ', 'ס', 'ע', 'פ', 'צ'];
  const ones = ['', 'א', 'ב', 'ג', 'ד', 'ה', 'ו', 'ז', 'ח', 'ט'];
  const h = Math.floor(n / 100);
  const rem = n % 100;
  if (rem === 15) return `${hundreds[h]}טו`;
  if (rem === 16) return `${hundreds[h]}טז`;
  return `${hundreds[h]}${tens[Math.floor(rem / 10)]}${ones[rem % 10]}`;
}

/** Formats a single counter value in `format`. `bullet` and `none` return ''. */
export function formatNumber(value: number, format: NumberFormat): string {
  switch (format) {
    case 'decimal':
      return String(value);
    case 'decimalZero':
      return value >= 0 && value < 10 ? `0${value}` : String(value);
    case 'upperRoman':
      return toRoman(value);
    case 'lowerRoman':
      return toRoman(value).toLowerCase();
    case 'upperLetter':
      return toLetters(value);
    case 'lowerLetter':
      return toLetters(value).toLowerCase();
    case 'ordinal':
      return toOrdinal(value);
    case 'cardinalText': {
      const t = toCardinalText(value);
      return t.charAt(0).toUpperCase() + t.slice(1);
    }
    case 'ordinalText': {
      const t = toOrdinalText(value);
      return t.charAt(0).toUpperCase() + t.slice(1);
    }
    case 'hex':
      return value.toString(16).toUpperCase();
    case 'chicago':
      return toChicago(value);
    case 'bullet':
    case 'none':
      return '';
    case 'decimalEnclosedCircle':
      return value >= 1 && value <= 20 ? String.fromCodePoint(0x2460 + value - 1) : String(value);
    case 'decimalEnclosedParen':
      return value >= 1 && value <= 20 ? String.fromCodePoint(0x2474 + value - 1) : `(${value})`;
    case 'decimalEnclosedFullstop':
      return value >= 1 && value <= 20 ? String.fromCodePoint(0x2488 + value - 1) : `${value}.`;
    case 'decimalFullWidth':
      return String(value).replace(/\d/g, (d) => String.fromCodePoint(0xff10 + Number(d)));
    case 'decimalHalfWidth':
      return String(value);
    case 'numberInDash':
      return `- ${value} -`;
    case 'russianLower':
      return toLetters(value, RUSSIAN);
    case 'russianUpper':
      return toLetters(value, RUSSIAN).toUpperCase();
    case 'hebrew1':
      return toHebrew(value);
    case 'hebrew2':
      return toLetters(value, 'אבגדהוזחטיכלמנסעפצקרשת');
    case 'arabicAlpha':
      return toLetters(value, ARABIC_ALPHA);
    case 'arabicAbjad':
      return toLetters(value, ARABIC_ABJAD);
    case 'hindiNumbers':
      return mapDigits(value, '०१२३४५६७८९');
    case 'thaiNumbers':
      return mapDigits(value, '๐๑๒๓๔๕๖๗๘๙');
    case 'ideographDigital':
      return mapDigits(value, CJK_DIGITS);
    case 'japaneseCounting':
    case 'chineseCounting':
    case 'chineseCountingThousand':
      return toCjkCounting(value);
    case 'aiueo':
    case 'aiueoFullWidth':
      return toLetters(value, AIUEO);
    case 'iroha':
    case 'irohaFullWidth':
      return toLetters(value, IROHA);
    default:
      return String(value);
  }
}

/**
 * Symbol-font bullets: Word stores bullets as Private Use Area code points of the Symbol/Wingdings
 * fonts (`U+F0B7` in Symbol is •). Maps them to Unicode so bullets render without those fonts.
 */
const SYMBOL_PUA: Record<string, Record<number, string>> = {
  symbol: { 183: '•', 168: '♦', 216: '⇒', 174: '→', 45: '−', 111: 'ο', 167: '♣', 169: '♥', 170: '♠' },
  wingdings: {
    167: '▪',
    168: '◻',
    110: '■',
    113: '❑',
    117: '◆',
    118: '❖',
    216: '➢',
    252: '✓',
    251: '✗',
    161: '○',
    159: '•',
    119: '⬥',
    224: '➔',
    232: '➔',
    108: '●',
    175: '✪',
    70: '☞',
  },
};

/** Converts a bullet's text (as stored in `w:lvlText`) to a renderable Unicode string. */
export function bulletDisplayText(text: string, fontName: string | undefined): string {
  const font = fontName?.toLowerCase().replace(/\s+\d+$/, '') ?? '';
  const table =
    font === 'symbol' ? SYMBOL_PUA.symbol : font.startsWith('wingdings') ? SYMBOL_PUA.wingdings : undefined;
  return [...text]
    .map((ch) => {
      const cp = ch.codePointAt(0) ?? 0;
      const low = cp >= 0xf000 && cp <= 0xf0ff ? cp - 0xf000 : cp;
      if (table) return table[low] ?? (cp >= 0xf000 && cp <= 0xf0ff ? '•' : ch);
      return cp >= 0xf000 && cp <= 0xf0ff ? '•' : ch;
    })
    .join('');
}
