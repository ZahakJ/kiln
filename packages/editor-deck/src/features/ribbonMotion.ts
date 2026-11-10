/**
 * The Transitions and Animations tabs filled in from the core's state: Effect Options lists the
 * options of the current transition or the selected effect (the core's lists, grouped by heading),
 * and the galleries show a small picture of each transition and effect (green entrances, gold
 * emphasis, red exits, as PowerPoint colours them).
 */
import type {
  RibbonData,
  RibbonDataGroup,
  RibbonDataLeaf,
  RibbonDataRow,
  RibbonMenuEntry,
} from '@kiln/commands';
import type { DeckChoice, DeckMotionState } from '@kiln/protocol';

const ACCENT = '#E05A27';
const SLIDE = '#FFF7EE';
const EDGE = '#8A7769';
const NEW = '#E7B79C';

function svg(body: string): string {
  const xml = `<svg xmlns="http://www.w3.org/2000/svg" width="48" height="34" viewBox="0 0 48 34">${body}</svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(xml)}`;
}

const frame = `<rect x="4.5" y="3.5" width="39" height="27" rx="2.5" fill="${SLIDE}" stroke="${EDGE}"/>`;

/** A picture for each transition of the gallery. */
export const TRANSITION_ICONS: Readonly<Record<string, string>> = {
  none: svg(`${frame}<path d="M10 26 L38 8" stroke="${EDGE}" stroke-width="1.5"/>`),
  morph: svg(
    `${frame}<circle cx="18" cy="17" r="6" fill="${NEW}"/><rect x="25" y="11" width="11" height="12" rx="2" fill="${ACCENT}" opacity="0.85"/>`,
  ),
  fade: svg(
    `<defs><linearGradient id="g" x1="0" x2="1"><stop offset="0" stop-color="${SLIDE}"/><stop offset="1" stop-color="${ACCENT}"/></linearGradient></defs><rect x="4.5" y="3.5" width="39" height="27" rx="2.5" fill="url(#g)" stroke="${EDGE}"/>`,
  ),
  push: svg(
    `<rect x="4.5" y="3.5" width="39" height="13" fill="${SLIDE}" stroke="${EDGE}"/><rect x="4.5" y="16.5" width="39" height="14" fill="${NEW}" stroke="${EDGE}"/><path d="M24 27 V20 M20.5 23 L24 19.5 L27.5 23" stroke="${ACCENT}" stroke-width="1.6" fill="none"/>`,
  ),
  wipe: svg(
    `${frame}<rect x="26" y="4" width="17" height="26" fill="${NEW}"/><path d="M18 17 H30 M22 13 L18 17 L22 21" stroke="${ACCENT}" stroke-width="1.6" fill="none"/>`,
  ),
  split: svg(
    `${frame}<rect x="18" y="4" width="12" height="26" fill="${NEW}"/><path d="M14 17 H18 M34 17 H30" stroke="${ACCENT}" stroke-width="1.6"/>`,
  ),
  reveal: svg(
    `<rect x="4.5" y="3.5" width="39" height="27" rx="2.5" fill="${NEW}" stroke="${EDGE}"/><rect x="2.5" y="6" width="26" height="22" rx="2" fill="${SLIDE}" stroke="${EDGE}" opacity="0.8"/>`,
  ),
  cut: svg(`${frame}<path d="M24 4 V30" stroke="${ACCENT}" stroke-width="2"/>`),
  randomBars: svg(
    `${frame}<g fill="${NEW}"><rect x="5" y="6" width="38" height="2"/><rect x="5" y="11" width="38" height="3"/><rect x="5" y="18" width="38" height="2"/><rect x="5" y="23" width="38" height="4"/></g>`,
  ),
  shape: svg(`${frame}<circle cx="24" cy="17" r="9" fill="${NEW}" stroke="${ACCENT}" stroke-width="1.2"/>`),
  uncover: svg(
    `<rect x="4.5" y="3.5" width="39" height="27" rx="2.5" fill="${NEW}" stroke="${EDGE}"/><rect x="0.5" y="3.5" width="22" height="27" fill="${SLIDE}" stroke="${EDGE}"/><path d="M19 17 H9 M13 13 L9 17 L13 21" stroke="${ACCENT}" stroke-width="1.6" fill="none"/>`,
  ),
  cover: svg(
    `${frame}<rect x="21.5" y="3.5" width="26" height="27" fill="${NEW}" stroke="${EDGE}"/><path d="M38 17 H28 M32 13 L28 17 L32 21" stroke="${ACCENT}" stroke-width="1.6" fill="none"/>`,
  ),
  flash: svg(`${frame}<path d="M26 6 L18 19 H23 L21 28 L30 14 H25 Z" fill="${ACCENT}"/>`),
};

const CLASS_COLOURS = { Entrance: '#2E9D62', Emphasis: '#D69A12', Exit: '#C8463D' } as const;

function star(colour: string, extra = ''): string {
  const pts = Array.from({ length: 10 }, (_, i) => {
    const r = i % 2 === 0 ? 11 : 4.6;
    const a = (Math.PI / 5) * i - Math.PI / 2;
    return `${(24 + r * Math.cos(a)).toFixed(1)},${(17 + r * Math.sin(a)).toFixed(1)}`;
  }).join(' ');
  return svg(`<polygon points="${pts}" fill="${colour}"/>${extra}`);
}

/** A picture for each effect of the gallery, by id. */
export function animationIcon(id: string, group: 'Entrance' | 'Emphasis' | 'Exit' | 'None'): string {
  if (group === 'None') return TRANSITION_ICONS.none ?? '';
  const c = CLASS_COLOURS[group];
  const arrow = (d: string) =>
    `<path d="${d}" stroke="#2B1C14" stroke-width="1.4" fill="none" opacity="0.7"/>`;
  switch (id) {
    case 'flyIn':
      return star(c, arrow('M24 33 V26 M21 29 L24 26 L27 29'));
    case 'flyOut':
      return star(c, arrow('M24 26 V33 M21 30 L24 33 L27 30'));
    case 'floatIn':
      return star(c, arrow('M37 26 V20 M35 22 L37 20 L39 22'));
    case 'fade':
    case 'fadeOut':
      return star(c, `<polygon points="35,5 44,5 44,14" fill="${c}" opacity="0.35"/>`);
    case 'spin':
    case 'teeter':
      return star(c, arrow('M36 9 A12 12 0 0 1 38 22'));
    case 'zoom':
    case 'zoomOut':
    case 'growShrink':
    case 'growTurn':
    case 'pulse':
      return star(
        c,
        `<polygon points="24,9 26,15 32,15 27,19 29,25 24,21 19,25 21,19 16,15 22,15" fill="#FFF" opacity="0.55"/>`,
      );
    case 'colorPulse':
      return star(c, `<circle cx="38" cy="8" r="4" fill="${ACCENT}"/>`);
    default:
      return star(c);
  }
}

/** The Effect Options menu of a list of choices (with their headings). */
export function effectMenu(
  command: string,
  choices: readonly DeckChoice[],
  chosen: string | null,
): RibbonMenuEntry[] {
  const out: RibbonMenuEntry[] = [];
  let group: string | null | undefined;
  for (const c of choices) {
    if (c.group !== group) {
      if (out.length > 0) out.push({ kind: 'separator' });
      if (c.group) out.push({ kind: 'heading', label: c.group });
      group = c.group;
    }
    out.push({
      command,
      label: c.id === chosen ? `${c.label} ✓` : c.label,
      args: { option: c.id },
    });
  }
  return out;
}

/** `spec` with Effect Options filled in and the motion galleries' pictures. */
export function deckRibbonWithMotion(spec: RibbonData, motion: DeckMotionState | null): RibbonData {
  const leaf = (item: RibbonDataLeaf): RibbonDataLeaf => {
    if (item.kind === 'gallery' && item.command === 'deck.transitions.gallery')
      return {
        ...item,
        items: item.items.map((e) => {
          const image = TRANSITION_ICONS[e.id];
          return image ? { ...e, look: { ...e.look, image } } : e;
        }),
      };
    if (item.kind === 'gallery' && item.command === 'deck.animations.gallery')
      return {
        ...item,
        items: item.items.map((e) => {
          const group =
            e.id === 'none'
              ? 'None'
              : e.label.endsWith('(Exit)')
                ? 'Exit'
                : EMPHASIS.has(e.id)
                  ? 'Emphasis'
                  : 'Entrance';
          return { ...e, look: { ...e.look, image: animationIcon(e.id, group) } };
        }),
      };
    if (item.kind === 'dropdown' && item.command === 'deck.transitions.effect_options') {
      const t = motion?.transition;
      return { ...item, menu: t ? effectMenu('deck.transitions.effect_options', t.options, t.option) : [] };
    }
    if (item.kind === 'dropdown' && item.command === 'deck.animations.effect_options')
      return {
        ...item,
        menu: motion ? effectMenu('deck.animations.effect_options', motion.options, motion.option) : [],
      };
    return item;
  };
  const group = (g: RibbonDataGroup): RibbonDataGroup => ({
    ...g,
    items: g.items.map((i) =>
      i.kind === 'row' ? ({ ...i, items: i.items.map(leaf) } as RibbonDataRow) : leaf(i as RibbonDataLeaf),
    ),
  });
  return { ...spec, tabs: spec.tabs.map((t) => ({ ...t, groups: t.groups.map(group) })) };
}

const EMPHASIS = new Set(['pulse', 'colorPulse', 'teeter', 'spin', 'growShrink']);
