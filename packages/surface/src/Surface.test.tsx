// @vitest-environment jsdom
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { A11yTree, Command, KilnClient, Event as KilnEvent, SceneDelta, Viewport } from '@kiln/protocol';
import { decodeAs } from '@kiln/protocol';
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import type { SurfaceController } from './controller';
import { Surface } from './Surface';
import { installPath2D, RecordingContext } from './test-support';

// jsdom gives modules http: URLs; vitest runs from the package directory.
const FRAME = resolve(process.cwd(), '../../fixtures/golden/scene/lorem-frame.bin');

const TREE: A11yTree = {
  nodes: [
    { id: 0, role: 'Document', name: 'lorem.txt', text: null, bounds: null, level: null, children: [1, 2] },
    {
      id: 1,
      role: 'Paragraph',
      name: null,
      text: 'Lorem ipsum dolor sit amet.',
      bounds: null,
      level: null,
      children: [],
    },
    {
      id: 2,
      role: 'Paragraph',
      name: null,
      text: 'Second paragraph.',
      bounds: null,
      level: null,
      children: [],
    },
  ],
  active: 2,
  announce: 'Ready',
  caret: { block: 2, offset: 3 },
};

class FakeClient implements KilnClient {
  readonly bridge = 'wasm' as const;
  commands: Command[] = [];
  viewports: Viewport[] = [];
  frames = 0;
  listener: ((e: KilnEvent) => void) | null = null;
  private readonly delta = decodeAs<SceneDelta>('SceneDelta', new Uint8Array(readFileSync(FRAME)));
  ping = async () => ({ version: '0.1.0', protocol: 1, target: 'test' });
  execute = async (_doc: number | null, command: Command) => {
    this.commands.push(command);
    return { outcome: 'Applied' as const, summary: null, value: 'None' as const };
  };
  query = async (_doc: number | null, q: unknown) => {
    if (q === 'SelectedText') return { Text: 'Lorem' };
    return {
      Selection: {
        kind: { Text: { anchor: { block: 1, offset: 0 }, head: { block: 1, offset: 5 } } },
        rects: [{ x: 160, y: 128, width: 40, height: 19 }],
        caret: { rect: { x: 200, y: 128, width: 2, height: 19 }, rtl: false },
        handles: [],
      },
    } as never;
  };
  open = async () => {
    throw new Error('unused');
  };
  new = async () => {
    throw new Error('unused');
  };
  setViewport = async (_doc: number, vp: Viewport) => {
    this.viewports.push(vp);
  };
  frame = async () => {
    this.frames += 1;
    return this.frames === 1
      ? this.delta
      : { ...this.delta, reset: false, tiles: [], atlas: [], overlay: null };
  };
  a11y = async () => TREE;
  subscribe = (l: (e: KilnEvent) => void) => {
    this.listener = l;
    return () => {
      this.listener = null;
    };
  };
  hostReply = async () => {};
  dispose = () => {};
}

const settle = async () => {
  for (let i = 0; i < 5; i++) {
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
  }
};

beforeAll(() => {
  installPath2D();
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, get: () => 1000 });
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 });
  HTMLCanvasElement.prototype.getContext = function getContext(this: HTMLCanvasElement) {
    const self = this as unknown as { __ctx?: RecordingContext };
    self.__ctx ??= new RecordingContext(this);
    return self.__ctx as never;
  } as never;
});

afterEach(cleanup);

async function mount() {
  const client = new FakeClient();
  let controller: SurfaceController | null = null;
  render(
    <Surface
      client={client}
      doc={4}
      label="lorem.txt"
      blink={false}
      theme="light"
      onController={(c) => (controller = c)}
    />,
  );
  await settle();
  await act(async () => {
    await new Promise((r) => setTimeout(r, 150)); // the a11y refresh is debounced
  });
  return { client, controller: controller as unknown as SurfaceController };
}

describe('<Surface>', () => {
  it('asks the core to repaint, sends the measured viewport and paints the frame', async () => {
    const { client, controller } = await mount();
    expect(client.commands[0]).toEqual({ Suite: 'Repaint' });
    expect(client.viewports[0]).toMatchObject({ width_px: 1000, height_px: 900, zoom: 1, dpr: 1 });
    expect(client.frames).toBeGreaterThan(0);
    expect(controller.getStats().tiles).toBeGreaterThan(0);
    // The page is centred: the extent (944 px) is narrower than the surface.
    expect(client.viewports.at(-1)?.scroll.x).toBeCloseTo(-28);
  });

  it('renders the input proxy for IME and the accessibility mirror', async () => {
    await mount();
    const proxy = screen.getByTestId('kiln-input-proxy');
    expect(proxy.getAttribute('contenteditable')).toBe('true');
    expect(proxy.getAttribute('role')).toBe('textbox');
    expect(proxy.getAttribute('spellcheck')).toBe('false');
    expect(proxy.getAttribute('autocapitalize')).toBe('off');
    expect(proxy.getAttribute('aria-label')).toBe('lorem.txt');
    const mirror = screen.getByTestId('kiln-a11y-mirror');
    expect(mirror.getAttribute('role')).toBe('document');
    expect([...mirror.querySelectorAll('p')].map((p) => p.textContent)).toEqual([
      'Lorem ipsum dolor sit amet.',
      'Second paragraph.',
    ]);
    const active = proxy.getAttribute('aria-activedescendant');
    expect(active && document.getElementById(active)?.textContent).toBe('Second paragraph.');
    expect(getComputedStyle(mirror).display).not.toBe('none');
    expect(screen.getByTestId('kiln-live').textContent).toBe('Ready');
    // The proxy sits at the painted caret (the frame's caret at the start of the first line,
    // document x 159, moved by the 28 px of centring), not at the stale selection geometry.
    expect(proxy.style.transform).toBe('translate(187px, 129px)');
  });

  it('turns keys, beforeinput and IME composition into commands', async () => {
    const { client } = await mount();
    const proxy = screen.getByTestId('kiln-input-proxy');
    client.commands = [];
    await act(async () => {
      proxy.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'ArrowRight', ctrlKey: true, bubbles: true, cancelable: true }),
      );
      const typed = new InputEvent('beforeinput', {
        inputType: 'insertText',
        data: 'k',
        bubbles: true,
        cancelable: true,
      });
      proxy.dispatchEvent(typed);
      expect(typed.defaultPrevented).toBe(true);
      proxy.dispatchEvent(new CompositionEvent('compositionstart', { data: '' }));
      proxy.dispatchEvent(new CompositionEvent('compositionupdate', { data: 'にほ' }));
      // Composition text is not intercepted as beforeinput.
      proxy.dispatchEvent(
        new InputEvent('beforeinput', {
          inputType: 'insertCompositionText',
          data: 'にほ',
          isComposing: true,
          cancelable: true,
        }),
      );
      proxy.dispatchEvent(new CompositionEvent('compositionend', { data: '日本' }));
      proxy.dispatchEvent(
        new InputEvent('beforeinput', { inputType: 'deleteContentBackward', cancelable: true }),
      );
    });
    await settle();
    expect(client.commands).toEqual([
      { Write: { MoveCaret: { motion: 'WordForward', extend: false } } },
      { Write: { InsertText: { text: 'k' } } },
      { Write: { SetComposition: { text: 'にほ', caret: 2 } } },
      { Write: { CommitComposition: { text: '日本' } } },
      { Write: 'DeleteBackward' },
    ]);
  });

  it('copies the selected text and pastes plain text', async () => {
    const { client } = await mount();
    const proxy = screen.getByTestId('kiln-input-proxy');
    client.commands = [];
    const data = new Map<string, string>();
    const clip = (type: string, text?: string) => {
      const e = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(e, 'clipboardData', {
        value: { getData: () => text ?? '', setData: (k: string, v: string) => data.set(k, v) },
      });
      return e;
    };
    await act(async () => {
      proxy.dispatchEvent(clip('copy'));
      proxy.dispatchEvent(clip('paste', 'pasted text'));
      proxy.dispatchEvent(clip('cut'));
    });
    await settle();
    expect(data.get('text/plain')).toBe('Lorem');
    expect(client.commands).toEqual([
      { Write: { InsertText: { text: 'pasted text' } } },
      { Write: 'DeleteBackward' },
    ]);
  });

  it('selects by pointer: click, double click for words, Shift+click and drag extend', async () => {
    const { client } = await mount();
    const scroller = screen.getByTestId('kiln-surface-scroller');
    client.commands = [];
    const pointer = (type: string, x: number, y: number, extra: Record<string, unknown> = {}) => {
      const e = new MouseEvent(type, {
        clientX: x,
        clientY: y,
        bubbles: true,
        cancelable: true,
        button: 0,
        ...extra,
      });
      Object.defineProperty(e, 'pointerId', { value: 1 });
      Object.defineProperty(e, 'pointerType', { value: 'mouse' });
      return e;
    };
    await act(async () => {
      scroller.dispatchEvent(pointer('pointerdown', 300, 200, { detail: 2 }));
    });
    await settle();
    await act(async () => {
      scroller.dispatchEvent(pointer('pointermove', 320, 260));
      scroller.dispatchEvent(pointer('pointerup', 320, 260));
      scroller.dispatchEvent(pointer('pointerdown', 100, 100, { detail: 1, shiftKey: true }));
      scroller.dispatchEvent(pointer('pointerup', 100, 100));
    });
    await settle();
    expect(client.commands).toEqual([
      { Write: { SelectAt: { point: { x: 272, y: 200 }, extend: false, unit: 'Word' } } },
      { Write: { SelectAt: { point: { x: 292, y: 260 }, extend: true, unit: 'Word' } } },
      { Write: { SelectAt: { point: { x: 72, y: 100 }, extend: true, unit: 'Char' } } },
    ]);
  });

  it('follows Changed events with frames and zooms with Ctrl+wheel', async () => {
    const { client, controller } = await mount();
    const frames = client.frames;
    await act(async () => {
      client.listener?.({
        Changed: {
          summary: { doc: 4, revision: 99 } as never,
          selection: { kind: 'None', rects: [], caret: null, handles: [] },
          status: {} as never,
        },
      });
    });
    await settle();
    expect(client.frames).toBeGreaterThan(frames);
    const scroller = screen.getByTestId('kiln-surface-scroller');
    const wheel = new WheelEvent('wheel', {
      deltaY: -100,
      ctrlKey: true,
      clientX: 500,
      clientY: 450,
      cancelable: true,
    });
    await act(async () => {
      scroller.dispatchEvent(wheel);
    });
    expect(wheel.defaultPrevented).toBe(true);
    expect(controller.getZoom()).toBeGreaterThan(1.2);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 200));
    });
    await settle();
    expect(client.viewports.at(-1)?.zoom).toBeCloseTo(controller.getZoom(), 5);
  });
});
