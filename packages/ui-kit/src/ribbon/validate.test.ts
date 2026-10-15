import { describe, expect, it } from 'vitest';
import { createHarness } from '../chrome/test-helpers.tsx';
import { button, dropdown, group, item, ribbon, split, submenu, tab } from './builders.ts';
import { assertValidRibbonSpec, validateRibbonSpec } from './validate.ts';

describe('validateRibbonSpec', () => {
  it('accepts a spec whose commands and icons all exist', () => {
    const { registry } = createHarness();
    const spec = ribbon('write', [
      tab(
        'home',
        'Home',
        [
          group(
            'clipboard',
            'Clipboard',
            [split('edit.paste', [item('edit.pasteTextOnly')]), button('edit.cut')],
            { launcher: { command: 'format.fontDialog' } },
          ),
        ],
        { keyTip: 'H' },
      ),
    ]);
    expect(validateRibbonSpec(spec, registry)).toEqual([]);
    expect(() => assertValidRibbonSpec(spec, registry)).not.toThrow();
  });

  it('reports unknown commands, unknown icons, duplicates and bad Key Tips', () => {
    const { registry } = createHarness();
    const spec = ribbon('write', [
      tab(
        'home',
        'Home',
        [
          group('g', 'G', [
            button('nope.command'),
            button('format.bold', { icon: 'NoSuchIcon' }),
            button('format.bold'),
            button('edit.copy', { keyTip: '!!' }),
          ]),
          group('g', 'Again', [
            dropdown(undefined, [submenu('More', [item('also.missing')])], { label: 'More' }),
          ]),
        ],
        { keyTip: 'H' },
      ),
      tab('home', 'Duplicate', [], { keyTip: 'H' }),
    ]);
    const messages = validateRibbonSpec(spec, registry).map((i) => `${i.path}: ${i.message}`);
    expect(messages).toEqual(
      expect.arrayContaining([
        expect.stringContaining('unknown command "nope.command"'),
        expect.stringContaining('unknown icon "NoSuchIcon"'),
        expect.stringContaining('duplicate item id "format.bold"'),
        expect.stringContaining('invalid Key Tip "!!"'),
        expect.stringContaining('duplicate group id "g"'),
        expect.stringContaining('unknown command "also.missing"'),
        expect.stringContaining('duplicate tab id "home"'),
        expect.stringContaining('Key Tip "H" also used by Home'),
      ]),
    );
    expect(() => assertValidRibbonSpec(spec, registry)).toThrow(/Invalid ribbon spec for write/);
  });

  it('validates the demo ribbon used for screenshots', async () => {
    const { demoRibbon } = await import('../../stories/Chrome.tsx');
    const { registry } = createHarness();
    expect(validateRibbonSpec(demoRibbon, registry)).toEqual([]);
  });
});
