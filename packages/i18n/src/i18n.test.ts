import { afterEach, describe, expect, it } from 'vitest';
import {
  direction,
  formatCount,
  formatNumber,
  language,
  localizeNumberText,
  resolveLanguage,
  setLanguage,
  t,
  tOr,
  tx,
} from './index.ts';

afterEach(async () => {
  await setLanguage('en', { applyToDocument: false });
});

describe('languages', () => {
  it('resolve a preference, System from the browser languages, English otherwise', () => {
    expect(resolveLanguage('de')).toBe('de');
    expect(resolveLanguage('system', ['ar-EG', 'en-US'])).toBe('ar');
    expect(resolveLanguage('system', ['fr-FR', 'de-AT'])).toBe('de');
    expect(resolveLanguage('system', ['fr-FR'])).toBe('en');
    expect(resolveLanguage('xx')).toBe('en');
  });

  it('switch the messages, and Arabic is right to left', async () => {
    expect(t('Close')).toBe('Close');
    await setLanguage('de', { applyToDocument: false });
    expect(language()).toBe('de');
    expect(t('Close')).not.toBe('Close');
    expect(direction()).toBe('ltr');
    await setLanguage('ar', { applyToDocument: false });
    expect(direction()).toBe('rtl');
    expect(t('Close')).toMatch(/\p{Script=Arabic}/u);
  });

  it('format ICU plurals in the language (Arabic has six forms)', async () => {
    expect(t('status.words', { count: 1 })).toBe('1 word');
    expect(t('status.words', { count: 1200 })).toBe('1,200 words');
    await setLanguage('de', { applyToDocument: false });
    expect(t('status.words', { count: 1200 })).toBe('1.200 Wörter');
    await setLanguage('ar', { applyToDocument: false });
    const forms = new Set([0, 1, 2, 3, 11, 100].map((count) => t('status.words', { count })));
    expect(forms.size).toBe(6);
  });

  it('keep unknown text as it is and use fallbacks for unknown keys', async () => {
    await setLanguage('de', { applyToDocument: false });
    expect(tx('Quarterly report {2026}')).toBe('Quarterly report {2026}');
    expect(tOr('cmd.no.such_command', 'Bold')).toBe(t('Bold'));
    expect(t('cmd.suite.format.bold')).toBe(t('Bold'));
  });
});

describe('numbers', () => {
  it('follow the interface language', async () => {
    expect(formatNumber(1234.5)).toBe('1,234.5');
    await setLanguage('de', { applyToDocument: false });
    expect(formatNumber(1234.5)).toBe('1.234,5');
    expect(formatCount(12000)).toBe('12.000');
    await setLanguage('ar', { applyToDocument: false });
    expect(formatNumber(1234.5)).toBe(new Intl.NumberFormat('ar').format(1234.5));
  });

  it("re-separate the core's plain numbers and leave other text alone", async () => {
    expect(localizeNumberText('1,234.50')).toBe('1,234.50');
    await setLanguage('de', { applyToDocument: false });
    expect(localizeNumberText('1,234.50')).toBe('1.234,50');
    expect(localizeNumberText('-12.5%')).toMatch(/^-12,5\s%$/);
    expect(localizeNumberText('7')).toBe('7');
    expect(localizeNumberText('$1,234.50')).toBe('$1,234.50');
    expect(localizeNumberText('1/2/2026')).toBe('1/2/2026');
    expect(localizeNumberText('#DIV/0!')).toBe('#DIV/0!');
  });
});
