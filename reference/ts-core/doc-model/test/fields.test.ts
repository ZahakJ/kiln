import { describe, expect, it } from 'vitest';
import {
  applyNumberSwitches,
  buildField,
  type DateField,
  evaluateField,
  formatFieldDate,
  type HyperlinkField,
  isUnknownField,
  parseFieldInstruction,
  type RefField,
  type SeqField,
  serializeFieldInstruction,
  type TocField,
  tokenizeField,
} from '../src/fields.ts';

describe('parseFieldInstruction', () => {
  it('PAGE with MERGEFORMAT', () => {
    const f = parseFieldInstruction(' PAGE   \\* MERGEFORMAT ');
    expect(f.type).toBe('PAGE');
    expect(f.general).toEqual(['MERGEFORMAT']);
    expect(isUnknownField(f)).toBe(false);
  });

  it('NUMPAGES and lower-case keywords', () => {
    expect(parseFieldInstruction('numpages').type).toBe('NUMPAGES');
    expect(parseFieldInstruction('SECTIONPAGES').type).toBe('SECTIONPAGES');
  });

  it('DATE with a picture switch', () => {
    const f = parseFieldInstruction('DATE \\@ "dddd, MMMM d, yyyy" \\* MERGEFORMAT') as DateField;
    expect(f.type).toBe('DATE');
    expect(f.picture).toBe('dddd, MMMM d, yyyy');
    expect(f.dateFormat).toBe('dddd, MMMM d, yyyy');
  });

  it('DATE and TIME default pictures', () => {
    expect((parseFieldInstruction('DATE') as DateField).picture).toBe('M/d/yyyy');
    expect((parseFieldInstruction('TIME') as DateField).picture).toBe('h:mm am/pm');
  });

  it('TOC with Word default switches', () => {
    const f = parseFieldInstruction('TOC \\o "1-3" \\h \\z \\u') as TocField;
    expect(f.outlineLevels).toEqual({ from: 1, to: 3 });
    expect([f.hyperlinks, f.hideInWebLayout, f.useOutlineLevels]).toEqual([true, true, true]);
    expect(f.styles).toEqual([]);
  });

  it('TOC with custom styles, no page numbers and a separator', () => {
    const f = parseFieldInstruction('TOC \\t "Title,1,Appendix Heading,2" \\n "2-3" \\p "-"') as TocField;
    expect(f.styles).toEqual([
      { style: 'Title', level: 1 },
      { style: 'Appendix Heading', level: 2 },
    ]);
    expect(f.omitPageNumbers).toEqual({ from: 2, to: 3 });
    expect(f.separator).toBe('-');
    expect(f.outlineLevels).toBeUndefined();
  });

  it('TOC table of figures (\\c) and \\n without range', () => {
    const f = parseFieldInstruction('TOC \\h \\z \\c "Figure" \\n') as TocField;
    expect(f.captionLabel).toBe('Figure');
    expect(f.omitPageNumbers).toBe('all');
  });

  it('TOC semicolon-separated style list', () => {
    const f = parseFieldInstruction('TOC \\t "Heading 1;1;Heading 2;2"') as TocField;
    expect(f.styles.map((s) => s.level)).toEqual([1, 2]);
  });

  it('REF with hyperlink and paragraph number switches', () => {
    const f = parseFieldInstruction('REF _Ref123456 \\w \\h') as RefField;
    expect(f.type).toBe('REF');
    expect(f.bookmark).toBe('_Ref123456');
    expect(f.hyperlink).toBe(true);
    expect(f.paragraphNumber).toBe('fullContext');
  });

  it('PAGEREF with \\h and \\p', () => {
    const f = parseFieldInstruction('PAGEREF _Toc1 \\h \\p') as RefField;
    expect([f.type, f.bookmark, f.hyperlink, f.relativePosition]).toEqual(['PAGEREF', '_Toc1', true, true]);
  });

  it('HYPERLINK external with tooltip', () => {
    const f = parseFieldInstruction(
      'HYPERLINK "https://example.com/a b" \\o "Open site" \\t "_blank"',
    ) as HyperlinkField;
    expect(f.url).toBe('https://example.com/a b');
    expect(f.tooltip).toBe('Open site');
    expect(f.target).toBe('_blank');
    expect(f.newWindow).toBe(false);
  });

  it('HYPERLINK internal location', () => {
    const f = parseFieldInstruction('HYPERLINK \\l "_Toc12345" \\n') as HyperlinkField;
    expect(f.url).toBeUndefined();
    expect(f.anchor).toBe('_Toc12345');
    expect(f.newWindow).toBe(true);
  });

  it('SEQ with format and reset', () => {
    const f = parseFieldInstruction('SEQ Figure \\* ARABIC \\r 3 \\s 1') as SeqField;
    expect(f.identifier).toBe('Figure');
    expect(f.general).toEqual(['ARABIC']);
    expect(f.resetTo).toBe(3);
    expect(f.resetAtHeading).toBe(1);
  });

  it('SEQ flags', () => {
    const f = parseFieldInstruction('SEQ Table \\c \\h') as SeqField;
    expect([f.repeat, f.hidden]).toEqual([true, true]);
  });

  it('unknown fields are preserved verbatim', () => {
    const raw = ' MERGEFIELD  FirstName \\* Upper ';
    const f = parseFieldInstruction(raw);
    expect(isUnknownField(f)).toBe(true);
    expect(f.raw).toBe(raw);
    expect(f.type).toBe('MERGEFIELD');
    expect(f.args).toEqual(['FirstName']);
    expect(f.general).toEqual(['Upper']);
  });

  it('numeric picture switch', () => {
    const f = parseFieldInstruction('= SUM(ABOVE) \\# "#,##0.00"');
    expect(f.numericFormat).toBe('#,##0.00');
    expect(isUnknownField(f)).toBe(true);
  });

  it('empty and garbage input never throws', () => {
    expect(parseFieldInstruction('').type).toBe('');
    expect(parseFieldInstruction('"unterminated').type).toBe('');
    expect(() => parseFieldInstruction('\\\\ \\ "')).not.toThrow();
  });
});

describe('tokenizer and serialisation', () => {
  it('handles escaped quotes and doubled backslashes', () => {
    const t = tokenizeField('INCLUDETEXT "C:\\\\Docs\\\\a \\"b\\".docx"');
    expect(t.map((x) => x.text)).toEqual(['INCLUDETEXT', 'C:\\Docs\\a "b".docx']);
  });

  it('accepts smart quotes as Word does', () => {
    const f = parseFieldInstruction('TOC \\o \u201C1-2\u201D') as TocField;
    expect(f.outlineLevels).toEqual({ from: 1, to: 2 });
  });

  it('serialises a canonical form that re-parses identically', () => {
    const instr = buildField('HYPERLINK', ['https://x.org/a b'], [['\\o', 'Tip "q"'], ['\\n']]);
    expect(instr).toBe('HYPERLINK "https://x.org/a b" \\o "Tip \\"q\\"" \\n');
    const f = parseFieldInstruction(instr) as HyperlinkField;
    expect([f.url, f.tooltip, f.newWindow]).toEqual(['https://x.org/a b', 'Tip "q"', true]);
    expect(serializeFieldInstruction(f)).toBe(instr);
  });
});

describe('field results', () => {
  const date = new Date(2026, 9, 5, 14, 7, 9); // Mon 5 Oct 2026 14:07:09

  it.each([
    ['M/d/yyyy', '10/5/2026'],
    ['dddd, MMMM d, yyyy', 'Monday, October 5, 2026'],
    ['dd.MM.yy', '05.10.26'],
    ['ddd d MMM', 'Mon 5 Oct'],
    ['h:mm am/pm', '2:07 pm'],
    ['HH:mm:ss', '14:07:09'],
    ["'Week of' MMMM", 'Week of October'],
    ['hh:mm AM/PM', '02:07 PM'],
  ])('date picture %s', (pic, out) => expect(formatFieldDate(pic, date)).toBe(out));

  it('number switches', () => {
    expect(applyNumberSwitches(14, ['ROMAN'])).toBe('XIV');
    expect(applyNumberSwitches(14, ['roman'])).toBe('xiv');
    expect(applyNumberSwitches(3, ['ALPHABETIC'])).toBe('C');
    expect(applyNumberSwitches(3, ['Ordinal'])).toBe('3rd');
    expect(applyNumberSwitches(21, ['CardText'])).toBe('twenty-one');
    expect(applyNumberSwitches(21, ['CardText', 'Caps'])).toBe('Twenty-one');
    expect(applyNumberSwitches(7, ['MERGEFORMAT'])).toBe('7');
  });

  it('evaluates page fields from layout context', () => {
    expect(evaluateField(parseFieldInstruction('PAGE \\* roman'), { page: 4 })).toBe('iv');
    expect(evaluateField(parseFieldInstruction('NUMPAGES'), { numPages: 12 })).toBe('12');
    expect(evaluateField(parseFieldInstruction('PAGE'), {})).toBeUndefined();
    expect(evaluateField(parseFieldInstruction('DATE \\@ "yyyy" \\* MERGEFORMAT'), { now: date })).toBe(
      '2026',
    );
  });

  it('evaluates REF/PAGEREF through bookmark callbacks and leaves TOC/SEQ to layout', () => {
    const ctx = {
      bookmarkText: (n: string) => (n === 'bm' ? 'Introduction' : undefined),
      bookmarkPage: () => 3,
    };
    expect(evaluateField(parseFieldInstruction('REF bm \\h \\* Upper'), ctx)).toBe('INTRODUCTION');
    expect(evaluateField(parseFieldInstruction('REF missing'), ctx)).toBeUndefined();
    expect(evaluateField(parseFieldInstruction('PAGEREF bm \\h'), ctx)).toBe('3');
    expect(evaluateField(parseFieldInstruction('TOC \\o "1-3"'), ctx)).toBeUndefined();
    expect(evaluateField(parseFieldInstruction('SEQ Figure'), ctx)).toBeUndefined();
  });
});
