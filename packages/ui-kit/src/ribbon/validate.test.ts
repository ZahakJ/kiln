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
            [split('suite.edit.paste', [item('suite.edit.paste_text_only')]), button('suite.edit.cut')],
            { launcher: { command: 'suite.format.font_dialog' } },
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
            button('suite.format.bold', { icon: 'NoSuchIcon' }),
            button('suite.format.bold'),
            button('suite.edit.copy', { keyTip: '!!' }),
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
        expect.stringContaining('duplicate item id "suite.format.bold"'),
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
