/**
 * The hard-coded string guard: no user-facing English in the interface's TSX that does not
 * go through `t`. A string that is not interface text (a font name, a sample, a format code) is
 * marked `i18n-ignore` on its line or the line before.
 */
import { describe, expect, it } from 'vitest';
import { read, scanSource, sourceFiles } from '../scripts/scan.ts';

describe('the interface sources', () => {
  it('have no hard-coded user-facing strings in TSX', () => {
    const findings = sourceFiles(['.tsx']).flatMap((f) => scanSource(f, read(f)));
    const report = findings.map((f) => `${f.file}:${f.line}:${f.column} ${f.kind} ${JSON.stringify(f.text)}`);
    expect(report).toEqual([]);
  });

  it('catch every kind of hard-coded string the guard knows', () => {
    const src = `
      export function X({ n }: { n: number }) {
        const items = [{ label: 'Bold' }];
        announce('Saved');
        return (
          <div title="Close" aria-label={n > 1 ? 'Many' : t('One')}>
            Hello {n} world
            {'Text child'}
            <i18n-free data-x="not text" />
          </div>
        );
      }`;
    const kinds = scanSource('x.tsx', src).map((f) => `${f.kind}:${f.text}`);
    expect(kinds).toEqual([
      'prop:Bold',
      'call:Saved',
      'attr:Close',
      'attr:Many',
      'jsx-text:Hello',
      'jsx-text:world',
      'child:Text child',
    ]);
  });

  it('accepts messages, ignore markers and text without letters', () => {
    const src = `
      export const Y = () => (
        <p title={t('Close')}>
          {t('Hello {name}', { name: 'x' })} — 42 %
          {/* i18n-ignore: a sample */}
          <span>AaBbCc</span>
        </p>
      );`;
    expect(scanSource('y.tsx', src)).toEqual([]);
  });
});
