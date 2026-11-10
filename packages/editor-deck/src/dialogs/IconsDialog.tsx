/**
 * Insert ▸ Icons: a searchable set of line icons by category. Each chosen icon goes on
 * the slide as a shape of its strokes (the core's `InsertIcon`), so Shape Outline recolours it
 * and it saves as an ordinary shape.
 */
import { msg, t, tx } from '@kiln/i18n';
import { Button, Select } from '@kiln/ui-kit';
import type { LucideIcon } from 'lucide-react';
import * as L from 'lucide-react';
import { useMemo, useRef, useState } from 'react';
import type { DeckEditorModel } from '../model.ts';
import { DeckDialogFrame } from './DeckDialogs.tsx';

const CATEGORIES: readonly [string, readonly [string, LucideIcon][]][] = [
  [
    msg('People'),
    [
      [msg('User'), L.User],
      [msg('Users'), L.Users],
      [msg('Person'), L.PersonStanding],
      [msg('Baby'), L.Baby],
      [msg('Handshake'), L.Handshake],
      [msg('Hand'), L.Hand],
      [msg('Thumbs Up'), L.ThumbsUp],
      [msg('Heart'), L.Heart],
    ],
  ],
  [
    msg('Technology'),
    [
      [msg('Laptop'), L.Laptop],
      [msg('Monitor'), L.Monitor],
      [msg('Smartphone'), L.Smartphone],
      [msg('Tablet'), L.Tablet],
      [msg('Chip'), L.Cpu],
      [msg('Server'), L.Server],
      [msg('Database'), L.Database],
      [msg('Cloud'), L.Cloud],
      [msg('Wi-Fi'), L.Wifi],
      [msg('Router'), L.Router],
      [msg('Printer'), L.Printer],
      [msg('Keyboard'), L.Keyboard],
      [msg('Mouse'), L.Mouse],
      [msg('Hard Drive'), L.HardDrive],
      [msg('Code'), L.Code],
    ],
  ],
  [
    msg('Business'),
    [
      [msg('Briefcase'), L.Briefcase],
      [msg('Building'), L.Building],
      [msg('Factory'), L.Factory],
      [msg('Store'), L.Store],
      [msg('Cart'), L.ShoppingCart],
      [msg('Card'), L.CreditCard],
      [msg('Wallet'), L.Wallet],
      [msg('Money'), L.Banknote],
      [msg('Savings'), L.PiggyBank],
      [msg('Column Chart'), L.ChartColumn],
      [msg('Line Chart'), L.ChartLine],
      [msg('Pie Chart'), L.ChartPie],
      [msg('Growth'), L.TrendingUp],
      [msg('Decline'), L.TrendingDown],
      [msg('Target'), L.Target],
      [msg('Trophy'), L.Trophy],
      [msg('Award'), L.Award],
      [msg('Medal'), L.Medal],
      [msg('Flag'), L.Flag],
      [msg('Rocket'), L.Rocket],
      [msg('Idea'), L.Lightbulb],
      [msg('Puzzle'), L.Puzzle],
    ],
  ],
  [
    msg('Time'),
    [
      [msg('Calendar'), L.Calendar],
      [msg('Clock'), L.Clock],
      [msg('Timer'), L.Timer],
      [msg('Alarm'), L.AlarmClock],
      [msg('Hourglass'), L.Hourglass],
    ],
  ],
  [
    msg('Communication'),
    [
      [msg('Mail'), L.Mail],
      [msg('Chat'), L.MessageCircle],
      [msg('Message'), L.MessageSquare],
      [msg('Phone'), L.Phone],
      [msg('Send'), L.Send],
      [msg('Bell'), L.Bell],
      [msg('Megaphone'), L.Megaphone],
      [msg('Share'), L.Share2],
    ],
  ],
  [
    msg('Travel'),
    [
      [msg('Globe'), L.Globe],
      [msg('Map'), L.Map],
      [msg('Pin'), L.MapPin],
      [msg('Compass'), L.Compass],
      [msg('Navigation'), L.Navigation],
      [msg('Plane'), L.Plane],
      [msg('Car'), L.Car],
      [msg('Bus'), L.Bus],
      [msg('Train'), L.TrainFront],
      [msg('Ship'), L.Ship],
      [msg('Bike'), L.Bike],
      [msg('Truck'), L.Truck],
    ],
  ],
  [
    msg('Nature'),
    [
      [msg('Sun'), L.Sun],
      [msg('Moon'), L.Moon],
      [msg('Rain'), L.CloudRain],
      [msg('Snowflake'), L.Snowflake],
      [msg('Leaf'), L.Leaf],
      [msg('Tree'), L.TreePine],
      [msg('Flower'), L.Flower],
      [msg('Mountain'), L.Mountain],
      [msg('Waves'), L.WavesHorizontal],
      [msg('Flame'), L.Flame],
      [msg('Drop'), L.Droplet],
    ],
  ],
  [
    msg('Arrows'),
    [
      [msg('Right'), L.ArrowRight],
      [msg('Left'), L.ArrowLeft],
      [msg('Up'), L.ArrowUp],
      [msg('Down'), L.ArrowDown],
      [msg('Up Right'), L.ArrowUpRight],
      [msg('Refresh'), L.RefreshCw],
      [msg('Rotate'), L.RotateCw],
      [msg('Repeat'), L.Repeat],
      [msg('Shuffle'), L.Shuffle],
      [msg('Move Right'), L.MoveRight],
      [msg('Forward'), L.ChevronsRight],
    ],
  ],
  [
    msg('Signs'),
    [
      [msg('Done'), L.CircleCheck],
      [msg('Error'), L.CircleX],
      [msg('Alert'), L.CircleAlert],
      [msg('Info'), L.Info],
      [msg('Warning'), L.TriangleAlert],
      [msg('Not Allowed'), L.Ban],
      [msg('Check'), L.Check],
      [msg('Cross'), L.X],
      [msg('Plus'), L.Plus],
      [msg('Minus'), L.Minus],
      [msg('Star'), L.Star],
      [msg('Shield'), L.Shield],
      [msg('Secure'), L.ShieldCheck],
      [msg('Lock'), L.Lock],
      [msg('Unlock'), L.LockOpen],
      [msg('Key'), L.Key],
      [msg('Eye'), L.Eye],
      [msg('Search'), L.Search],
      [msg('Settings'), L.Settings],
    ],
  ],
  [
    msg('Tools and Education'),
    [
      [msg('Wrench'), L.Wrench],
      [msg('Hammer'), L.Hammer],
      [msg('Book'), L.Book],
      [msg('Open Book'), L.BookOpen],
      [msg('Graduation'), L.GraduationCap],
      [msg('School'), L.School],
      [msg('Pencil'), L.Pencil],
      [msg('Pen'), L.PenTool],
      [msg('Palette'), L.Palette],
    ],
  ],
  [
    msg('Media and Home'),
    [
      [msg('Camera'), L.Camera],
      [msg('Picture'), L.Image],
      [msg('Video'), L.Video],
      [msg('Music'), L.Music],
      [msg('Headphones'), L.Headphones],
      [msg('Microphone'), L.Mic],
      [msg('Film'), L.Film],
      [msg('Gift'), L.Gift],
      [msg('House'), L.House],
      [msg('Coffee'), L.Coffee],
      [msg('Food'), L.Utensils],
      [msg('Health'), L.Stethoscope],
      [msg('Medicine'), L.Pill],
      [msg('Activity'), L.Activity],
      [msg('Fitness'), L.Dumbbell],
    ],
  ],
];

const ALL = msg('All');
const n = (el: Element, a: string) => Number(el.getAttribute(a) ?? 0);

/** The SVG path data of one element of an icon (paths, circles, rectangles, lines). */
export function pathOf(el: Element): string | null {
  switch (el.tagName.toLowerCase()) {
    case 'path':
      return el.getAttribute('d');
    case 'circle':
    case 'ellipse': {
      const [cx, cy] = [n(el, 'cx'), n(el, 'cy')];
      const rx = el.hasAttribute('r') ? n(el, 'r') : n(el, 'rx');
      const ry = el.hasAttribute('r') ? n(el, 'r') : n(el, 'ry');
      return `M${cx - rx} ${cy}a${rx} ${ry} 0 1 0 ${2 * rx} 0a${rx} ${ry} 0 1 0 ${-2 * rx} 0Z`;
    }
    case 'rect': {
      const [x, y, w, h] = [n(el, 'x'), n(el, 'y'), n(el, 'width'), n(el, 'height')];
      const r = Math.min(n(el, 'rx') || n(el, 'ry'), w / 2, h / 2);
      if (r <= 0) return `M${x} ${y}h${w}v${h}h${-w}Z`;
      return (
        `M${x + r} ${y}h${w - 2 * r}a${r} ${r} 0 0 1 ${r} ${r}v${h - 2 * r}a${r} ${r} 0 0 1 ${-r} ${r}` +
        `h${-(w - 2 * r)}a${r} ${r} 0 0 1 ${-r} ${-r}v${-(h - 2 * r)}a${r} ${r} 0 0 1 ${r} ${-r}Z`
      );
    }
    case 'line':
      return `M${n(el, 'x1')} ${n(el, 'y1')}L${n(el, 'x2')} ${n(el, 'y2')}`;
    case 'polyline':
    case 'polygon': {
      const pts = (el.getAttribute('points') ?? '').trim().split(/[\s,]+/);
      if (pts.length < 4) return null;
      let d = `M${pts[0]} ${pts[1]}`;
      for (let i = 2; i + 1 < pts.length; i += 2) d += `L${pts[i]} ${pts[i + 1]}`;
      return el.tagName.toLowerCase() === 'polygon' ? `${d}Z` : d;
    }
    default:
      return null;
  }
}

export function IconsDialog({ model }: { model: DeckEditorModel }) {
  const [category, setCategory] = useState<string>(ALL);
  const [query, setQuery] = useState('');
  const [chosen, setChosen] = useState<string[]>([]);
  const grid = useRef<HTMLFieldSetElement>(null);
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return CATEGORIES.filter(([c]) => category === ALL || c === category)
      .flatMap(([, icons]) => icons)
      .filter(([name]) => q === '' || name.toLowerCase().includes(q) || tx(name).toLowerCase().includes(q));
  }, [category, query]);
  const insert = async () => {
    for (const name of chosen) {
      const svg = grid.current?.querySelector(`[data-icon="${CSS.escape(name)}"] svg`);
      if (!svg) continue;
      const paths = [...svg.children].map(pathOf).filter((d): d is string => !!d);
      await model.core('deck.insert.icon', { name, paths });
    }
  };
  return (
    <DeckDialogFrame
      model={model}
      title={t('Insert Icons')}
      testId="deck-icons"
      size="lg"
      okLabel={chosen.length > 1 ? t('Insert ({count, number})', { count: chosen.length }) : t('Insert')}
      okDisabled={chosen.length === 0}
      onOk={insert}
    >
      <div className="kiln-deckdlg__row">
        <input
          className="kiln-deckdlg__text"
          type="search"
          aria-label={t('Search icons')}
          placeholder={t('Search icons')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Select
          aria-label={t('Category')}
          value={category}
          onValueChange={setCategory}
          options={[ALL, ...CATEGORIES.map(([c]) => c)].map((c) => ({ value: c, label: tx(c) }))}
        />
      </div>
      <fieldset ref={grid} className="kiln-deckicons" aria-label={t('Icons')}>
        {shown.map(([name, Glyph]) => {
          const on = chosen.includes(name);
          return (
            <Button
              key={name}
              className="kiln-deckicons__item"
              data-icon={name}
              aria-label={tx(name)}
              aria-pressed={on}
              title={tx(name)}
              onClick={() => setChosen((c) => (on ? c.filter((x) => x !== name) : [...c, name]))}
            >
              <Glyph size={32} strokeWidth={2} aria-hidden="true" />
            </Button>
          );
        })}
        {shown.length === 0 ? <p className="kiln-deckdlg__hint">{t('No icons match.')}</p> : null}
      </fieldset>
    </DeckDialogFrame>
  );
}
