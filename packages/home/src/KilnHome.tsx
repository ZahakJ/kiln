/**
 * Kiln Home (PLAN.md §1.6; mockups docs/design/mockups/home-*.png and phone-*.png): the left rail
 * (New, Home, Recent, Shared with me, Pinned, Templates, Open file, the app filters, the local-first
 * card, Settings), the greeting, the four app doors, the "Coming from Microsoft Office?" banner, the
 * Recent / Pinned / Shared lists and the templates strip, with search over files and templates.
 *
 * Presentation only: every file, template and figure comes in through props (from the core's
 * recents and the host), and actions go out through callbacks. The only state kept here is UI state
 * (the selected list, the app filter, the search text).
 */
import type { RecentFile } from '@kiln/protocol';
import { AppTile, Button, Flame, Icon, Kbd, Spark, Tooltip, TooltipProvider } from '@kiln/ui-kit';
import {
  Clock,
  FolderOpen,
  House,
  LayoutTemplate,
  Pin,
  PinOff,
  Plus,
  Search,
  Settings,
  Upload,
  Users,
  X,
} from 'lucide-react';
import {
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from 'react';
import { DOOR_MASK } from './door-path.ts';
import { greeting } from './greeting.ts';
import { HOME_APPS, type HomeApp, kindApp, relativeTime } from './model.ts';
import './home.css';

export interface HomeTemplate {
  id: string;
  app: HomeApp;
  name: string;
  /** Thumbnail URL (rendered by the core's thumbnailer). */
  thumbnail?: string;
}

export interface HomeStorage {
  /** Bytes used by Kiln's local storage. */
  used: number;
  /** Bytes available to it. */
  quota: number;
}

export type HomeList = 'recent' | 'pinned' | 'shared';

export interface KilnHomeProps {
  /** Recent files (the pinned ones are flagged); `null` while loading. */
  recent: readonly RecentFile[] | null;
  /** Files shared with the user through a collaboration server; `null` when none is connected. */
  shared?: readonly RecentFile[] | null;
  /** Featured templates (the strip shows when there are any). */
  templates?: readonly HomeTemplate[];
  /** Local storage use for the "Local first" card. */
  storage?: HomeStorage | null;
  /** First name for the greeting. */
  userName?: string | null;
  /** Initials for the avatar (signed in). */
  initials?: string | null;
  /** Clock for the greeting and relative dates (tests pin it). */
  now?: Date;
  /** Which apps the core can create documents for; others show "Soon" instead of the + button. */
  creatable?: readonly HomeApp[];
  /** Hides the "Coming from Microsoft Office?" banner (a UI preference the host remembers). */
  switcherDismissed?: boolean;
  onDismissSwitcher?: () => void;
  onCreate: (app: HomeApp) => void;
  onOpenFile: () => void;
  onOpenRecent: (file: RecentFile) => void;
  onTogglePin?: (file: RecentFile) => void;
  onOpenTemplate?: (template: HomeTemplate) => void;
  onShowTemplates?: () => void;
  onOpenSettings: () => void;
  /** The 2-minute tour (WP-85); the button shows when given. */
  onTour?: () => void;
  /** Import Office settings (WP-85); the button shows when given. */
  onImportSettings?: () => void;
  /** Suite version for the About line. */
  version?: string;
}

interface DoorSpec {
  app: HomeApp;
  product: string;
  noun: string;
  blurb: string;
  formats: readonly string[];
}

const DOORS: readonly DoorSpec[] = [
  {
    app: 'write',
    product: 'Kiln Write',
    noun: 'Document',
    blurb: 'Letters, essays, reports',
    formats: ['.docx', '.odt'],
  },
  {
    app: 'grid',
    product: 'Kiln Grid',
    noun: 'Workbook',
    blurb: 'Budgets, lists, models',
    formats: ['.xlsx', '.csv'],
  },
  {
    app: 'deck',
    product: 'Kiln Deck',
    noun: 'Deck',
    blurb: 'Talks, pitches, lessons',
    formats: ['.pptx', '.odp'],
  },
  {
    app: 'notes',
    product: 'Kiln Notes',
    noun: 'Notebook',
    blurb: 'Ideas, logs, clippings',
    formats: ['.md'],
  },
];

const APP_LABEL: Readonly<Record<HomeApp, string>> = {
  write: 'Write',
  grid: 'Grid',
  deck: 'Deck',
  notes: 'Notes',
};

function formatBytes(n: number): string {
  if (n < 1e9) return `${Math.max(0, Math.round(n / 1e6))} MB`;
  return `${(n / 1e9).toFixed(n < 1e10 ? 1 : 0)} GB`;
}

function splitName(name: string): { stem: string; ext: string } {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? { stem: name.slice(0, dot), ext: name.slice(dot) } : { stem: name, ext: '' };
}

function folderOf(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.slice(-3, -1).join(' › ');
}

/** Kiln Home. */
export function KilnHome(props: KilnHomeProps) {
  return (
    <TooltipProvider>
      <HomeInner {...props} />
    </TooltipProvider>
  );
}

function HomeInner({
  recent,
  shared = null,
  templates = [],
  storage,
  userName,
  initials,
  now = new Date(),
  creatable = HOME_APPS,
  switcherDismissed = false,
  onDismissSwitcher,
  onCreate,
  onOpenFile,
  onOpenRecent,
  onTogglePin,
  onOpenTemplate,
  onShowTemplates,
  onOpenSettings,
  onTour,
  onImportSettings,
  version,
}: KilnHomeProps) {
  const [list, setList] = useState<HomeList>('recent');
  const [filter, setFilter] = useState<HomeApp | null>(null);
  const [query, setQuery] = useState('');
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLElement>(null);

  // Ctrl+K focuses the search (mockup); Ctrl+O opens a file; Ctrl+N creates the default document.
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
      const k = e.key.toLowerCase();
      if (k === 'k') {
        e.preventDefault();
        searchRef.current?.focus();
      } else if (k === 'o') {
        e.preventDefault();
        onOpenFile();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onOpenFile]);

  const pinned = useMemo(() => (recent ?? []).filter((f) => f.pinned), [recent]);
  const source = list === 'recent' ? recent : list === 'pinned' ? pinned : shared;
  const q = query.trim().toLowerCase();
  const rows = useMemo(
    () =>
      (source ?? []).filter(
        (f) =>
          (!filter || kindApp(f.kind) === filter) &&
          (!q || f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q)),
      ),
    [source, filter, q],
  );
  const shownTemplates = useMemo(
    () => templates.filter((t) => (!filter || t.app === filter) && (!q || t.name.toLowerCase().includes(q))),
    [templates, filter, q],
  );

  const showList = (l: HomeList) => {
    setList(l);
    listRef.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  };

  return (
    <div className="kiln-home" data-app="home">
      <nav className="kiln-home__rail" aria-label="Kiln Home">
        <div className="kiln-home__logo">
          <AppTile app="home" size={30} />
          <span className="kiln-home__wordmark">kiln</span>
        </div>
        <NewMenu creatable={creatable} onCreate={onCreate} />
        <ul className="kiln-home__nav">
          <NavItem icon={House} label="Home" current onClick={() => window.scrollTo({ top: 0 })} />
          <NavItem icon={Clock} label="Recent" onClick={() => showList('recent')} />
          <NavItem
            icon={Users}
            label="Shared with me"
            count={shared?.length || undefined}
            onClick={() => showList('shared')}
          />
          <NavItem
            icon={Pin}
            label="Pinned"
            count={pinned.length || undefined}
            onClick={() => showList('pinned')}
          />
          {onShowTemplates ? (
            <NavItem icon={LayoutTemplate} label="Templates" onClick={onShowTemplates} />
          ) : null}
          <NavItem icon={FolderOpen} label="Open file" shortcut="Mod+O" onClick={onOpenFile} />
        </ul>
        <div className="kiln-home__cap" id="kiln-home-apps">
          Apps
        </div>
        <ul className="kiln-home__nav" aria-labelledby="kiln-home-apps">
          {HOME_APPS.map((app) => (
            <li key={app}>
              <button
                type="button"
                className="kiln-home__nav-item"
                aria-pressed={filter === app}
                data-filter={app}
                onClick={() => setFilter((f) => (f === app ? null : app))}
              >
                <AppTile app={app} size={20} />
                <span>{APP_LABEL[app]}</span>
                {filter === app ? <span className="kiln-home__count">Filter</span> : null}
              </button>
            </li>
          ))}
        </ul>
        <LocalFirstCard storage={storage} />
        <button type="button" className="kiln-home__nav-item kiln-home__settings" onClick={onOpenSettings}>
          <Icon icon={Settings} size={18} />
          <span>Settings</span>
        </button>
      </nav>

      <main className="kiln-home__main">
        <header className="kiln-home__bar">
          <div className="kiln-home__bar-logo" aria-hidden="true">
            <AppTile app="home" size={30} />
            <span className="kiln-home__wordmark">kiln</span>
          </div>
          <label className="kiln-home__search">
            <Icon icon={Search} size={15} />
            <span className="kiln-visually-hidden">Search files, templates and help</span>
            <input
              ref={searchRef}
              type="search"
              placeholder="Search files, templates and help"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e: KeyboardEvent<HTMLInputElement>) => {
                if (e.key === 'Escape') setQuery('');
              }}
            />
            <Kbd shortcut="Mod+K" variant="inline" className="kiln-home__kbd" />
          </label>
          <div className="kiln-home__bar-end">
            <button
              type="button"
              className="kiln-home__icon-btn kiln-home__phone-search"
              aria-label="Search"
              onClick={() => searchRef.current?.focus()}
            >
              <Icon icon={Search} size={20} />
            </button>
            {initials ? (
              <span
                className="kiln-avatar"
                role="img"
                aria-label={userName ? `Signed in as ${userName}` : 'Signed in'}
              >
                {initials}
              </span>
            ) : null}
          </div>
        </header>

        <div className="kiln-home__content">
          <section className="kiln-home__hello" aria-labelledby="kiln-home-title">
            <div>
              <p className="kiln-home__eyebrow">
                <Flame size={14} />
                <span>{greeting(now, userName)}</span>
              </p>
              <h1 id="kiln-home-title" className="kiln-home__title">
                <span className="kiln-visually-hidden">Kiln Home. </span>What are we making today?
              </h1>
            </div>
            <div className="kiln-home__hello-actions">
              <Button size="md" icon={<Icon icon={FolderOpen} size={15} />} onClick={onOpenFile}>
                Open file
              </Button>
              {onShowTemplates ? (
                <Button size="md" icon={<Icon icon={LayoutTemplate} size={15} />} onClick={onShowTemplates}>
                  All templates
                </Button>
              ) : null}
            </div>
          </section>

          <section className="kiln-home__doors" aria-label="Create">
            {DOORS.map((d) => (
              <Door
                key={d.app}
                spec={d}
                available={creatable.includes(d.app)}
                onCreate={() => onCreate(d.app)}
              />
            ))}
          </section>

          {switcherDismissed ? null : (
            <section className="kiln-home__switch" aria-label="Coming from Microsoft Office">
              <div className="kiln-home__formats" aria-hidden="true">
                <span style={{ background: 'var(--kiln-write-solid)', '--r': '-6deg' } as CSSProperties}>
                  .docx
                </span>
                <span style={{ background: 'var(--kiln-grid-solid)', '--r': '2deg' } as CSSProperties}>
                  .xlsx
                </span>
                <span style={{ background: 'var(--kiln-deck-solid)', '--r': '8deg' } as CSSProperties}>
                  .pptx
                </span>
              </div>
              <div className="kiln-home__switch-text">
                <h2>Coming from Microsoft Office?</h2>
                <p>
                  Your .docx, .xlsx and .pptx open as they are. The ribbon is where you left it, and your
                  shortcuts still work.
                </p>
              </div>
              <div className="kiln-home__switch-actions">
                {onTour ? (
                  <Button variant="ember" icon={<Spark size={13} />} onClick={onTour}>
                    Take the 2-minute tour
                  </Button>
                ) : null}
                {onImportSettings ? (
                  <Button icon={<Icon icon={Upload} size={15} />} onClick={onImportSettings}>
                    Import settings
                  </Button>
                ) : null}
                {onDismissSwitcher ? (
                  <button
                    type="button"
                    className="kiln-home__icon-btn"
                    aria-label="Dismiss"
                    onClick={onDismissSwitcher}
                  >
                    <Icon icon={X} size={15} />
                  </button>
                ) : null}
              </div>
            </section>
          )}

          <div className="kiln-home__lower" data-templates={shownTemplates.length > 0 || undefined}>
            <section ref={listRef} className="kiln-home__files" aria-label="Files">
              <div className="kiln-home__files-head">
                <ListSwitch value={list} onChange={setList} />
                {filter ? (
                  <button type="button" className="kiln-home__filter-chip" onClick={() => setFilter(null)}>
                    <AppTile app={filter} size={14} />
                    {APP_LABEL[filter]} only
                    <Icon icon={X} size={12} />
                  </button>
                ) : null}
              </div>
              <FileTable
                list={list}
                rows={rows}
                loading={source === null && list !== 'shared'}
                sharedUnavailable={list === 'shared' && shared === null}
                filtered={!!filter || !!q}
                now={now}
                onOpen={onOpenRecent}
                onTogglePin={onTogglePin}
              />
            </section>
            {shownTemplates.length > 0 ? (
              <section className="kiln-home__templates" aria-labelledby="kiln-home-templates">
                <div className="kiln-home__section-head">
                  <h2 id="kiln-home-templates">Fresh from the kiln</h2>
                  {onShowTemplates ? (
                    <button type="button" className="kiln-home__link" onClick={onShowTemplates}>
                      All templates ›
                    </button>
                  ) : null}
                </div>
                <ul className="kiln-home__template-grid">
                  {shownTemplates.slice(0, 8).map((t) => (
                    <li key={t.id}>
                      <button
                        type="button"
                        className="kiln-home__template"
                        onClick={() => onOpenTemplate?.(t)}
                      >
                        <span className="kiln-home__template-img">
                          {t.thumbnail ? <img src={t.thumbnail} alt="" /> : <AppTile app={t.app} size={40} />}
                        </span>
                        <span className="kiln-home__template-name">
                          <AppTile app={t.app} size={16} />
                          {t.name}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            ) : null}
          </div>
          {version ? (
            <p className="kiln-home__version" data-testid="suite-version">
              Version {version}
            </p>
          ) : null}
        </div>
      </main>

      <nav className="kiln-home__tabbar" aria-label="Kiln Home sections">
        <button type="button" aria-current="page" onClick={() => window.scrollTo({ top: 0 })}>
          <Icon icon={House} size={22} />
          <span>Home</span>
        </button>
        <button type="button" onClick={onOpenFile}>
          <Icon icon={FolderOpen} size={22} />
          <span>Files</span>
        </button>
        <button type="button" onClick={() => showList('shared')}>
          <Icon icon={Users} size={22} />
          <span>Shared</span>
        </button>
        <button type="button" onClick={onOpenSettings}>
          <Icon icon={Settings} size={22} />
          <span>Settings</span>
        </button>
      </nav>
    </div>
  );
}

function NavItem({
  icon,
  label,
  count,
  shortcut,
  current,
  onClick,
}: {
  icon: typeof House;
  label: string;
  count?: number;
  shortcut?: string;
  current?: boolean;
  onClick: () => void;
}) {
  return (
    <li>
      <button
        type="button"
        className="kiln-home__nav-item"
        aria-current={current ? 'page' : undefined}
        onClick={onClick}
      >
        <Icon icon={icon} size={18} />
        <span>{label}</span>
        {count !== undefined ? <span className="kiln-home__count">{count}</span> : null}
        {shortcut ? <Kbd shortcut={shortcut} variant="inline" className="kiln-home__count" /> : null}
      </button>
    </li>
  );
}

/** The ember New button: a menu of the four document types (Ctrl+N creates the first). */
function NewMenu({
  creatable,
  onCreate,
}: {
  creatable: readonly HomeApp[];
  onCreate: (app: HomeApp) => void;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', close);
    return () => document.removeEventListener('pointerdown', close);
  }, [open]);
  return (
    <div className="kiln-home__new" ref={ref}>
      <button
        type="button"
        className="kiln-home__new-btn"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
      >
        <Icon icon={Plus} size={18} />
        <span>New</span>
        <Kbd shortcut="Mod+N" variant="inline" className="kiln-home__new-kbd" />
      </button>
      {open ? (
        <div className="kiln-home__new-menu" role="menu" id={id}>
          {DOORS.map((d) => (
            <button
              key={d.app}
              type="button"
              role="menuitem"
              disabled={!creatable.includes(d.app)}
              onClick={() => {
                setOpen(false);
                onCreate(d.app);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Escape') setOpen(false);
              }}
            >
              <AppTile app={d.app} size={20} />
              <span>
                Blank {d.noun.toLowerCase()}
                <small>{d.product}</small>
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function Door({ spec, available, onCreate }: { spec: DoorSpec; available: boolean; onCreate: () => void }) {
  const label = `New ${spec.noun.toLowerCase()} in ${spec.product}`;
  const body = (
    <>
      <span className="kiln-visually-hidden">
        {available ? `${label}: ` : `${label} (not in this build yet): `}
      </span>
      <span className="kiln-door__rim" aria-hidden="true" />
      <AppTile app={spec.app} size={92} className="kiln-door__tile" />
      <span className="kiln-door__product">{spec.product}</span>
      <span className="kiln-door__noun">{spec.noun}</span>
      <span className="kiln-door__blurb">{spec.blurb}</span>
      <span className="kiln-door__meta">
        {spec.formats.map((f) => (
          <span key={f} className="kiln-door__chip">
            {f}
          </span>
        ))}
        <span className="kiln-door__plus" aria-hidden="true">
          {available ? <Icon icon={Plus} size={18} /> : <span className="kiln-door__soon">Soon</span>}
        </span>
      </span>
    </>
  );
  return (
    <button
      type="button"
      className="kiln-door"
      data-app={spec.app}
      data-door={spec.app}
      style={{ maskImage: DOOR_MASK, WebkitMaskImage: DOOR_MASK } as CSSProperties}
      aria-disabled={!available || undefined}
      onClick={() => available && onCreate()}
    >
      {body}
    </button>
  );
}

function ListSwitch({ value, onChange }: { value: HomeList; onChange: (l: HomeList) => void }) {
  const items: [HomeList, string][] = [
    ['recent', 'Recent'],
    ['pinned', 'Pinned'],
    ['shared', 'Shared with me'],
  ];
  const refs = useRef(new Map<HomeList, HTMLButtonElement>());
  const onKey = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const next =
      e.key === 'ArrowRight'
        ? (i + 1) % items.length
        : e.key === 'ArrowLeft'
          ? (i + items.length - 1) % items.length
          : -1;
    if (next < 0) return;
    e.preventDefault();
    const id = items[next]?.[0];
    if (!id) return;
    onChange(id);
    refs.current.get(id)?.focus();
  };
  return (
    <div className="kiln-home__seg" role="tablist" aria-label="File lists">
      {items.map(([id, label], i) => (
        <button
          key={id}
          ref={(el) => {
            if (el) refs.current.set(id, el);
          }}
          type="button"
          role="tab"
          aria-selected={value === id}
          tabIndex={value === id ? 0 : -1}
          onClick={() => onChange(id)}
          onKeyDown={(e) => onKey(e, i)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function FileTable({
  list,
  rows,
  loading,
  sharedUnavailable,
  filtered,
  now,
  onOpen,
  onTogglePin,
}: {
  list: HomeList;
  rows: readonly RecentFile[];
  loading: boolean;
  sharedUnavailable: boolean;
  filtered: boolean;
  now: Date;
  onOpen: (f: RecentFile) => void;
  onTogglePin?: (f: RecentFile) => void;
}) {
  if (loading) {
    return (
      <div className="kiln-home__empty" role="status">
        Looking for your files…
      </div>
    );
  }
  if (rows.length === 0) {
    const [title, text]: [string, ReactNode] = sharedUnavailable
      ? ['Nothing shared yet', 'Files people share with you through a Kiln collaboration server appear here.']
      : filtered
        ? ['No matches', 'Try another word, or clear the app filter.']
        : list === 'pinned'
          ? ['Nothing pinned yet', 'Pin a file you keep coming back to and it stays at the top.']
          : [
              'Your recent files will gather here',
              'Create something with one of the doors above, or open a file from this device.',
            ];
    return (
      <div className="kiln-home__empty" role="status">
        <Flame size={22} />
        <strong>{title}</strong>
        <span>{text}</span>
      </div>
    );
  }
  return (
    <table className="kiln-home__table">
      <thead>
        <tr>
          <th scope="col">Name</th>
          <th scope="col">Opened</th>
          <th scope="col" className="kiln-home__col-where">
            Location
          </th>
          <th scope="col">
            <span className="kiln-visually-hidden">Pin</span>
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((f) => {
          const { stem, ext } = splitName(f.name);
          const app = kindApp(f.kind);
          return (
            <tr key={f.path}>
              <td>
                <button type="button" className="kiln-home__file" onClick={() => onOpen(f)}>
                  <AppTile app={app} size={24} />
                  <span>
                    <b>{stem}</b>
                    <span className="kiln-home__ext">{ext}</span>
                    <small>{folderOf(f.path) || 'This device'}</small>
                  </span>
                </button>
              </td>
              <td className="kiln-home__muted">{relativeTime(f.opened_at_ms, now)}</td>
              <td className="kiln-home__muted kiln-home__col-where">{folderOf(f.path) || 'This device'}</td>
              <td className="kiln-home__pin-cell">
                {onTogglePin ? (
                  <Tooltip content={f.pinned ? 'Unpin' : 'Pin to the top'}>
                    <button
                      type="button"
                      className="kiln-home__icon-btn"
                      aria-label={f.pinned ? `Unpin ${f.name}` : `Pin ${f.name}`}
                      aria-pressed={f.pinned}
                      onClick={() => onTogglePin(f)}
                    >
                      <Icon icon={f.pinned ? PinOff : Pin} size={15} />
                    </button>
                  </Tooltip>
                ) : null}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function LocalFirstCard({ storage }: { storage?: HomeStorage | null }) {
  const pct =
    storage && storage.quota > 0 ? Math.min(100, Math.max(1, (storage.used / storage.quota) * 100)) : null;
  return (
    <div className="kiln-home__local">
      <strong>
        <Flame size={12} />
        Local first
      </strong>
      <span>Your files live on this device. Sync is optional.</span>
      {pct !== null && storage ? (
        <>
          {/* biome-ignore lint/a11y/useSemanticElements: a styled bar; <meter> cannot take the kit's gradient across browsers */}
          <span
            className="kiln-home__meter"
            role="meter"
            aria-label="Storage used"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(pct)}
          >
            <i style={{ width: `${pct}%` }} />
          </span>
          <span className="kiln-home__tnum">
            {formatBytes(storage.used)} of {formatBytes(storage.quota)}
          </span>
        </>
      ) : null}
    </div>
  );
}
