import { COMMAND_CATALOG } from '@officina/commands';
import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CommandIcon, hasCommandIcon, registerCommandIcons, resolveCommandIcon } from './commandIcons.tsx';

describe('command icons', () => {
  it('resolves every icon named by the command catalog', () => {
    const missing = COMMAND_CATALOG.filter((c) => c.icon && !hasCommandIcon(c.icon)).map(
      (c) => `${c.id}: ${c.icon}`,
    );
    expect(missing).toEqual([]);
  });

  it('resolves glyphs by name, Lucide icons by component, and registered extras', () => {
    expect(resolveCommandIcon('FormatPainter')).toBe('FormatPainter');
    expect(typeof resolveCommandIcon('Bold')).toBe('object');
    expect(resolveCommandIcon('NoSuchIcon')).toBeUndefined();
    const Custom = () => <svg data-testid="custom" />;
    registerCommandIcons({ CustomThing: Custom });
    expect(hasCommandIcon('CustomThing')).toBe(true);
    const { container } = render(<CommandIcon name="Bold" size={20} />);
    expect(container.querySelector('svg')?.getAttribute('width')).toBe('20');
  });
});
