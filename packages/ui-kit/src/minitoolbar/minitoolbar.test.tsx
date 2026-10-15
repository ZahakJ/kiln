import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '../chrome/test-matchers.ts';
import { MiniToolbar } from './MiniToolbar.tsx';

describe('MiniToolbar', () => {
  const anchor = { left: 200, top: 300, width: 100, height: 18 };

  it('appears above the selection as a toolbar and hides when the anchor is cleared', () => {
    const { rerender } = render(
      <MiniToolbar anchor={anchor} onDismiss={() => {}}>
        <button type="button">B</button>
      </MiniToolbar>,
    );
    const bar = screen.getByRole('toolbar', { name: 'Formatting' });
    expect(bar).toBeInTheDocument();
    const host = bar.parentElement as HTMLElement;
    expect(Number.parseFloat(host.style.top)).toBeLessThanOrEqual(anchor.top);
    rerender(
      <MiniToolbar anchor={null} onDismiss={() => {}}>
        <button type="button">B</button>
      </MiniToolbar>,
    );
    expect(screen.queryByRole('toolbar')).toBeNull();
  });

  it('flips below the selection near the top of the window', () => {
    render(
      <MiniToolbar anchor={{ left: 10, top: 2, width: 40, height: 18 }} onDismiss={() => {}}>
        <button type="button">B</button>
      </MiniToolbar>,
    );
    expect(document.querySelector('.of-minitoolbar--below')).toBeInTheDocument();
  });

  it('is dismissed by typing in the document and by Escape inside it', () => {
    const onDismiss = vi.fn();
    render(
      <MiniToolbar anchor={anchor} onDismiss={onDismiss}>
        <button type="button">B</button>
      </MiniToolbar>,
    );
    act(() => void window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift' })));
    expect(onDismiss).not.toHaveBeenCalled();
    act(() => void window.dispatchEvent(new KeyboardEvent('keydown', { key: 'a' })));
    expect(onDismiss).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(screen.getByRole('button', { name: 'B' }), { key: 'Escape' });
    expect(onDismiss).toHaveBeenCalledTimes(2);
  });
});
