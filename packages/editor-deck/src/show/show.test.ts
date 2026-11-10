import { describe, expect, it } from 'vitest';
import { clickSelection, rowName } from '../features/AnimationPane.tsx';
import {
  animationIcon,
  deckRibbonWithMotion,
  effectMenu,
  TRANSITION_ICONS,
} from '../features/ribbonMotion.ts';
import { formatElapsed } from '../presenter/PresenterView.tsx';
import { relays } from '../presenter/relay.ts';
import { ManualClock } from './clock.ts';
import { fitSlide, multiply, normalRamp, toSlidePoint } from './compose.ts';

describe('the show compositor', () => {
  it('fits the slide into the screen and centres it on whole pixels', () => {
    expect(fitSlide(1280, 720, 1920, 1080)).toEqual({ k: 1.5, ox: 0, oy: 0 });
    const tall = fitSlide(1280, 720, 1000, 1000);
    expect(tall.k).toBeCloseTo(1000 / 1280);
    expect(tall.ox).toBe(0);
    expect(tall.oy).toBe(Math.round((1000 - 720 * tall.k) / 2));
    const wide = fitSlide(960, 720, 2000, 720);
    expect(wide.ox).toBe(520);
  });

  it('maps client points back to the slide', () => {
    const fit = fitSlide(1280, 720, 2560, 1600);
    // A dpr of 2: the canvas is 2560 × 1600 device px for 1280 × 800 CSS px.
    const p = toSlidePoint(fit, 2, { left: 0, top: 0 }, { x: 640, y: 400 });
    expect(p.x).toBeCloseTo(640);
    expect(p.y).toBeCloseTo(360);
  });

  it('folds ramp stops beyond the ends into the gradient line', () => {
    const r = normalRamp({
      from: [0, 0],
      to: [100, 0],
      stops: [
        [-0.5, 0],
        [0.25, 1],
        [1.5, 0],
      ],
    });
    expect(r.from).toEqual([-50, 0]);
    expect(r.to).toEqual([150, 0]);
    expect(r.stops.map(([o]) => o)).toEqual([0, 0.375, 1]);
    const plain = {
      from: [0, 0] as [number, number],
      to: [10, 0] as [number, number],
      stops: [
        [0, 1],
        [1, 0],
      ] as [number, number][],
    };
    expect(normalRamp(plain)).toBe(plain);
  });

  it('multiplies matrices in canvas order', () => {
    const t = [1, 0, 0, 1, 10, 20];
    const s = [2, 0, 0, 2, 0, 0];
    expect(multiply(t, s)).toEqual([2, 0, 0, 2, 10, 20]);
    expect(multiply(s, t)).toEqual([2, 0, 0, 2, 20, 40]);
  });
});

describe('the manual clock', () => {
  it('moves only when set, never back, and tells its listeners', () => {
    const c = new ManualClock(5);
    let told = 0;
    const off = c.subscribe(() => {
      told += 1;
    });
    c.set(100);
    c.set(50);
    c.advance(10);
    expect(c.now()).toBe(110);
    expect(told).toBe(3);
    off();
    c.set(200);
    expect(told).toBe(3);
  });
});

describe('the Animation Pane', () => {
  const rows = [1, 2, 3, 4].map((id) => ({
    id,
    shape: 9,
    shape_name: 'Title 1',
    paragraphs: id === 2 ? ([1, 1] as [number, number]) : null,
    effect: 'fade',
    name: 'Fade',
    class: 'entrance',
    option: null,
    start: 'OnClick' as const,
    duration_ms: 500,
    delay_ms: 0,
    repeat: 1,
    click: id,
    begin_ms: 0,
    end_ms: 500,
    playable: true,
  }));

  it('selects with a click, adds with Ctrl, extends with Shift', () => {
    expect(clickSelection(rows, [], 1, { ctrl: false, shift: false }, 0)).toEqual([2]);
    expect(clickSelection(rows, [2], 3, { ctrl: true, shift: false }, 1)).toEqual([2, 4]);
    expect(clickSelection(rows, [2, 4], 3, { ctrl: true, shift: false }, 1)).toEqual([2]);
    expect(clickSelection(rows, [2], 3, { ctrl: false, shift: true }, 1)).toEqual([2, 3, 4]);
  });

  it('names paragraph builds after their paragraph', () => {
    expect(rowName(rows[0] as (typeof rows)[number])).toBe('Title 1');
    expect(rowName(rows[1] as (typeof rows)[number])).toBe('Title 1: paragraph 2');
  });
});

describe('the motion ribbon', () => {
  it('lists effect options under their headings with the chosen one marked', () => {
    const m = effectMenu(
      'deck.animations.effect_options',
      [
        { id: 'clockwise', label: 'Clockwise', group: 'Direction' },
        { id: 'counterclockwise', label: 'Counterclockwise', group: 'Direction' },
        { id: 'half', label: 'Half Spin', group: 'Amount' },
      ],
      'half',
    );
    expect(m.map((e) => ('label' in e ? e.label : e.kind))).toEqual([
      'Direction',
      'Clockwise',
      'Counterclockwise',
      'separator',
      'Amount',
      'Half Spin ✓',
    ]);
  });

  it('gives every transition and effect a picture', () => {
    for (const id of [
      'none',
      'fade',
      'push',
      'wipe',
      'split',
      'reveal',
      'cut',
      'randomBars',
      'shape',
      'uncover',
      'cover',
      'flash',
      'morph',
    ])
      expect(TRANSITION_ICONS[id], id).toMatch(/^data:image\/svg\+xml/);
    expect(animationIcon('flyIn', 'Entrance')).not.toBe(animationIcon('flyIn', 'Exit'));
    const spec = deckRibbonWithMotion(
      {
        app: 'deck',
        tabs: [
          {
            id: 'transitions',
            label: 'Transitions',
            groups: [
              {
                id: 'g',
                label: 'G',
                items: [
                  { kind: 'dropdown', command: 'deck.transitions.effect_options', menu: [] },
                  {
                    kind: 'gallery',
                    command: 'deck.transitions.gallery',
                    items: [{ id: 'fade', label: 'Fade' }],
                  },
                ],
              },
            ],
          },
        ],
      },
      {
        transition: {
          effect: 'fade',
          option: 'smoothly',
          options: [
            { id: 'smoothly', label: 'Smoothly', group: null },
            { id: 'throughBlack', label: 'Through Black', group: null },
          ],
          duration_ms: 700,
          on_click: true,
          after_ms: null,
          kept: false,
        },
        animations: [],
        selected: [],
        options: [],
        option: null,
        painter: false,
        badges: false,
        settings: {
          kind: 'Speaker',
          loop_show: false,
          no_narration: false,
          no_animation: false,
          from: null,
          to: null,
          manual: false,
          presenter_view: false,
        },
        show: {
          running: false,
          mode: 'Show',
          slide: 0,
          position: 0,
          count: 0,
          click: 0,
          clicks: 0,
          screen: 'Slide',
          tool: 'Arrow',
          typed: '',
          next: null,
          on_click: true,
          kind: 'Speaker',
          version: 0,
        },
      },
    );
    const items = spec.tabs[0]?.groups[0]?.items ?? [];
    const dd = items[0];
    expect(dd && dd.kind === 'dropdown' ? dd.menu.length : 0).toBe(2);
    const g = items[1];
    expect(g && g.kind === 'gallery' ? g.items[0]?.look?.image : undefined).toMatch(/^data:image/);
  });
});

describe('Presenter View', () => {
  it('formats the timer', () => {
    expect(formatElapsed(0)).toBe('00:00');
    expect(formatElapsed(61_500)).toBe('01:01');
    expect(formatElapsed(3_723_000)).toBe('1:02:03');
  });

  it('relays the show commands only', () => {
    expect(relays('deck.show.next')).toBe(true);
    expect(relays('deck.show.pointer')).toBe(true);
    expect(relays('deck.show.settings')).toBe(false);
    expect(relays('deck.animations.add_animation')).toBe(false);
  });
});
