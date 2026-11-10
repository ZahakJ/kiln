import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AppFrame } from '../chrome/AppFrame.tsx';
import { resetOptionsCache } from '../chrome/options.ts';
import { createHarness } from '../chrome/test-helpers.tsx';
import { button, color, group, ribbon, split, tab, toggle } from '../ribbon/builders.ts';
import { formFactorOf } from './formFactor.ts';
import { keyboardInset } from './keyboard.ts';
import { quickContextOf, quickItems } from './quick.ts';

vi.setConfig({ testTimeout: 20_000 });

const spec = ribbon('write', [
  tab('home', 'Home', [
    group('font', 'Font', [
      toggle('suite.format.bold'),
      toggle('suite.format.italic'),
      color('suite.format.font_color'),
    ]),
    group('paragraph', 'Paragraph', [split('suite.format.bullets', [{ command: 'suite.format.numbering' }])]),
  ]),
  tab('insert', 'Insert', [group('tables', 'Tables', [button('suite.insert.table')])]),
]);

/** A window `width` CSS px wide, for `matchMedia`. */
function windowWidth(width: number) {
  vi.stubGlobal('matchMedia', (query: string) => {
    const max = Number(/max-width:\s*(\d+)px/.exec(query)?.[1] ?? Number.POSITIVE_INFINITY);
    return {
      matches: width <= max,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    } as unknown as MediaQueryList;
  });
}

function setup(width: number, extra: Partial<Parameters<typeof AppFrame>[0]> = {}) {
  windowWidth(width);
  resetOptionsCache();
  const h = createHarness();
  let bold = false;
  const boldRun = vi.fn(() => {
    bold = !bold;
  });
  h.registry.register({ id: 'suite.format.bold', title: 'Bold', run: boldRun, isChecked: () => bold });
  h.registry.register({
    id: 'suite.format.italic',
    title: 'Italic',
    run: () => undefined,
    isChecked: () => false,
  });
  const onBack = vi.fn();
  render(
    <AppFrame
      app="write"
      registry={h.registry}
      keymap={h.keymap}
      title="Report"
      extension=".docx"
      onRename={() => undefined}
      ribbon={spec}
      statusBar={<div className="kiln-statusbar">status</div>}
      onBack={onBack}
      {...extra}
    >
      <textarea aria-label="Document text" defaultValue="Document" />
    </AppFrame>,
  );
  return { ...h, boldRun, onBack };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('form factors', () => {
  it('splits phone, tablet and desktop at 640 and 1024 px', () => {
    expect(formFactorOf(390)).toBe('phone');
    expect(formFactorOf(639)).toBe('phone');
    expect(formFactorOf(640)).toBe('tablet');
    expect(formFactorOf(1023)).toBe('tablet');
    expect(formFactorOf(1024)).toBe('desktop');
  });
});

describe('keyboard docking', () => {
  it('measures what the keyboard covers, and ignores zoom and small moves', () => {
    expect(keyboardInset({ height: 500, offsetTop: 0 }, 839)).toBe(339);
    expect(keyboardInset({ height: 500, offsetTop: 40 }, 839)).toBe(299);
    // The address bar or a rounding: no keyboard.
    expect(keyboardInset({ height: 800, offsetTop: 0 }, 839)).toBe(0);
    // Pinch zoom shrinks the visual viewport too: nothing to dock to.
    expect(keyboardInset({ height: 400, offsetTop: 0, scale: 2 }, 839)).toBe(0);
  });
});

describe('quick commands', () => {
  it('follow what is selected', () => {
    expect(quickContextOf('write', {})).toBe('text');
    expect(quickContextOf('write', { contexts: ['table'] })).toBe('table');
    expect(quickContextOf('grid', {})).toBe('cell');
    expect(quickContextOf('grid', { scopes: ['cell-edit'] })).toBe('text');
    expect(quickContextOf('deck', {})).toBe('slide');
    expect(quickContextOf('deck', { scopes: ['object'] })).toBe('object');
    expect(quickContextOf('deck', { scopes: ['text-edit'] })).toBe('text');
  });

  it("are the ribbon's own controls, split buttons as their main command", () => {
    const items = quickItems(spec.tabs, [
      'suite.format.bold',
      'suite.format.bullets',
      'suite.format.font_color',
      'x.y',
    ]);
    expect(items.map((i) => [i.kind, 'command' in i ? i.command : ''])).toEqual([
      ['toggle', 'suite.format.bold'],
      ['button', 'suite.format.bullets'],
      ['color', 'suite.format.font_color'],
    ]);
  });
});

describe('AppFrame on a phone', () => {
  it('has the top app bar and the bottom toolbar instead of the title bar and ribbon', () => {
    const { onBack } = setup(390);
    const bar = screen.getByTestId('app-bar');
    expect(screen.queryByRole('tablist', { name: 'Ribbon tabs' })).not.toBeInTheDocument();
    expect(within(bar).getByRole('button', { name: 'Undo' })).toBeInTheDocument();
    expect(within(bar).getByRole('button', { name: 'Redo' })).toBeInTheDocument();
    expect(within(bar).getByRole('button', { name: 'Search commands' })).toBeInTheDocument();
    expect(within(bar).getByRole('button', { name: 'More options' })).toBeInTheDocument();
    // Name and extension as one file name.
    expect(within(bar).getByTestId('document-title')).toHaveTextContent('Report.docx');
    within(bar).getByRole('button', { name: 'Back' }).click();
    expect(onBack).toHaveBeenCalledOnce();
    const toolbar = screen.getByTestId('bottom-toolbar');
    expect(within(toolbar).getByRole('toolbar', { name: 'Quick commands' })).toBeInTheDocument();
    expect(within(toolbar).getByTestId('ribbon-sheet-button')).toHaveTextContent('Home');
  });

  it('opens the ribbon sheet: tabs as chips, groups as tiles, the same commands', async () => {
    const user = userEvent.setup();
    const { boldRun } = setup(390);
    await user.click(screen.getByTestId('ribbon-sheet-button'));
    const sheet = await screen.findByTestId('ribbon-sheet');
    const chips = within(sheet).getAllByRole('tab');
    expect(chips.map((c) => c.textContent)).toEqual(['Home', 'Insert']);
    expect(within(sheet).getByRole('region', { name: 'Font' })).toBeInTheDocument();
    expect(within(sheet).getByRole('region', { name: 'Paragraph' })).toBeInTheDocument();
    await user.click(within(sheet).getByRole('button', { name: 'Bold' }));
    expect(boldRun).toHaveBeenCalledOnce();
    await user.click(within(sheet).getByRole('tab', { name: 'Insert' }));
    expect(within(sheet).getByRole('region', { name: 'Tables' })).toBeInTheDocument();
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByTestId('ribbon-sheet')).not.toBeInTheDocument());
    expect(screen.getByTestId('ribbon-sheet-button')).toHaveTextContent('Insert');
  });

  it('lists Share, Save As, Find and Replace, File and Settings in the overflow menu', async () => {
    const user = userEvent.setup();
    const send = vi.fn(() => Promise.reject(new Error('Save the document first, then share it.')));
    setup(390, {
      backstage: { saveAs: <p>Save As page</p>, info: <p>Info</p> },
      shareCopy: { mode: 'download', fileName: 'Report.docx', send },
    });
    await user.click(screen.getByRole('button', { name: 'More options' }));
    const menu = await screen.findByRole('menu');
    for (const name of ['Share', 'Save As', 'Find and Replace', 'File', 'Settings'])
      expect(within(menu).getByRole('menuitem', { name })).toBeInTheDocument();
    await user.click(within(menu).getByRole('menuitem', { name: 'Share' }));
    const sheet = await screen.findByTestId('share-sheet');
    expect(within(sheet).getByText('Report.docx, as last saved')).toBeInTheDocument();
    await user.click(within(sheet).getByTestId('share-file'));
    expect(send).toHaveBeenCalledWith('file');
    // The reason is shown as a toast, in words.
    expect(await screen.findByText('Save the document first, then share it.')).toBeInTheDocument();
  });

  it('keeps the desktop chrome on a tablet and a desktop', () => {
    setup(800);
    expect(screen.getByRole('tablist', { name: 'Ribbon tabs' })).toBeInTheDocument();
    expect(screen.queryByTestId('bottom-toolbar')).not.toBeInTheDocument();
    expect(screen.queryByTestId('app-bar')).not.toBeInTheDocument();
  });

  it('docks on the keyboard: the frame is padded by what the keyboard covers', async () => {
    const listeners = new Set<() => void>();
    const vv = {
      height: 839,
      offsetTop: 0,
      scale: 1,
      addEventListener: (_: string, fn: () => void) => listeners.add(fn),
      removeEventListener: (_: string, fn: () => void) => listeners.delete(fn),
    };
    vi.stubGlobal('visualViewport', vv);
    vi.stubGlobal('innerHeight', 839);
    setup(390);
    const frame = document.querySelector<HTMLElement>('.kiln-appframe');
    expect(frame?.style.getPropertyValue('--kiln-keyboard-inset')).toBe('0px');
    vv.height = 539;
    await act(async () => {
      for (const l of listeners) l();
      await new Promise((r) => requestAnimationFrame(() => r(null)));
    });
    expect(frame?.style.getPropertyValue('--kiln-keyboard-inset')).toBe('300px');
    expect(frame?.dataset.keyboard).toBe('open');
  });
});

describe('the floating context toolbar', () => {
  const size = { width: 300, height: 60 };
  const view = { width: 412, height: 800 };
  it('stands above the selection, centred on it and inside the window', async () => {
    const { placeContextToolbar } = await import('./ContextToolbar.tsx');
    expect(placeContextToolbar({ left: 100, top: 300, right: 200, bottom: 320 }, size, view, 'text')).toEqual(
      {
        left: 8,
        top: 230,
      },
    );
    expect(
      placeContextToolbar({ left: 300, top: 300, right: 400, bottom: 320 }, size, view, 'cells'),
    ).toEqual({
      left: 104,
      top: 230,
    });
  });
  it('goes below a selection at the top, clear of the text handles, and away with one off screen', async () => {
    const { placeContextToolbar } = await import('./ContextToolbar.tsx');
    expect(placeContextToolbar({ left: 100, top: 20, right: 200, bottom: 40 }, size, view, 'text')).toEqual({
      left: 8,
      top: 76,
    });
    expect(
      placeContextToolbar({ left: 100, top: 900, right: 200, bottom: 920 }, size, view, 'text'),
    ).toBeNull();
  });
});
