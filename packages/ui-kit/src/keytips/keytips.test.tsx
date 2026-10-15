import { act, render, screen } from '@testing-library/react';
import { useRef, useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import '../chrome/test-matchers.ts';
import { KeyTipsLayer } from './KeyTipsLayer.tsx';

function Host({ onBold, onExit }: { onBold: () => void; onExit: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [tab, setTab] = useState<'home' | 'insert'>('home');
  const [active, setActive] = useState(true);
  return (
    <div ref={ref}>
      <button
        type="button"
        data-keytip="H"
        data-keytip-level="root"
        data-keytip-enter="tab:home"
        onClick={() => setTab('home')}
      >
        Home
      </button>
      <button
        type="button"
        data-keytip="N"
        data-keytip-level="root"
        data-keytip-enter="tab:insert"
        onClick={() => setTab('insert')}
      >
        Insert
      </button>
      {tab === 'home' ? (
        <div>
          <button type="button" data-keytip="1" data-keytip-level="tab:home" onClick={onBold}>
            Bold
          </button>
          <button type="button" data-keytip="FF" data-keytip-level="tab:home">
            Font
          </button>
          <button type="button" data-keytip="FS" data-keytip-level="tab:home">
            Size
          </button>
        </div>
      ) : (
        <button type="button" data-keytip="T" data-keytip-level="tab:insert">
          Table
        </button>
      )}
      <KeyTipsLayer
        active={active}
        rootRef={ref}
        onExit={() => {
          setActive(false);
          onExit();
        }}
      />
    </div>
  );
}

const badges = () =>
  [...document.querySelectorAll('[data-keytip-badge]')].map((b) => b.getAttribute('data-keytip-badge'));
const key = (k: string) =>
  act(() => void window.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true })));
const frame = () => act(() => new Promise((r) => requestAnimationFrame(() => r(undefined))));

describe('KeyTipsLayer', () => {
  it('shows tab tips, enters a tab and runs a command (Alt, H, 1)', async () => {
    const onBold = vi.fn();
    const onExit = vi.fn();
    render(<Host onBold={onBold} onExit={onExit} />);
    expect(badges()).toEqual(['H', 'N']);
    key('h');
    await frame();
    expect(badges()).toEqual(['1', 'FF', 'FS']);
    key('1');
    expect(onBold).toHaveBeenCalledOnce();
    expect(onExit).toHaveBeenCalledOnce();
    expect(badges()).toEqual([]);
  });

  it('narrows multi-letter tips and backs out with Escape', async () => {
    const onExit = vi.fn();
    render(<Host onBold={() => {}} onExit={onExit} />);
    key('H');
    await frame();
    key('F');
    expect(badges()).toEqual(['FF', 'FS']);
    expect(document.querySelector('.of-keytip__typed')).toHaveTextContent('F');
    key('Escape'); // clear typed letters
    expect(badges()).toEqual(['1', 'FF', 'FS']);
    key('Escape'); // back to tabs
    expect(badges()).toEqual(['H', 'N']);
    key('Escape'); // exit
    expect(onExit).toHaveBeenCalledOnce();
  });

  it('switches tabs: N shows Insert’s commands; other keys end Key Tips', async () => {
    const onExit = vi.fn();
    render(<Host onBold={() => {}} onExit={onExit} />);
    key('n');
    await frame();
    expect(screen.getByRole('button', { name: 'Table' })).toBeInTheDocument();
    expect(badges()).toEqual(['T']);
    key('ArrowDown');
    expect(onExit).toHaveBeenCalledOnce();
  });
});
