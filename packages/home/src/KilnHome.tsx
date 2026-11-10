/**
 * Kiln Home (mockups docs/design/mockups/home-*.png and phone-*.png): the left rail
 * (New, Home, Recent, Shared with me, Pinned, Templates, Open file, the app filters, the local-first
 * card, Settings), the greeting, a door per app of the release and the templates door, the "Coming
 * from Microsoft Office?" banner, the Recent / Pinned / Shared lists and the featured templates,
 * with search over files and templates; and the Templates view (the full gallery).
 *
 * Presentation only: every file, template and figure comes in through props (from the core's
 * recents and the host), and actions go out through callbacks. The only state kept here is UI state
 * (the view, the selected list, the app filter, the search text).
 */
import { msg, readingStep, t, tx } from '@kiln/i18n';
import type { RecentFile } from '@kiln/protocol';
import {
  AppTile,
  Button,
  Flame,
  Icon,
  Kbd,
  limitRecents,
  Spark,
  Tooltip,
  TooltipProvider,
  useOfficeOptions,
} from '@kiln/ui-kit';
import {
  ArrowRight,
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
import { locationOf } from './recents/location.ts';
import {
  type HomeTemplate,
  onCardKeys,
  TemplateCard,
  TemplateGallery,
  templateMatches,
} from './templates/TemplateGallery.tsx';
import './home.css';
import './mobile/mobile.css';

export type { HomeTemplate } from './templates/TemplateGallery.tsx';

/** Home's two views: the start page and the template gallery. */
export type HomeView = 'home' | 'templates';

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
  /**
   * Documents others shared with the user through a collaboration server (the rooms joined from a
   * link); `null` when there are none to show.
   */
  shared?: readonly RecentFile[] | null;
  /** Every template (the featured ones fill the strip; search and the Templates view list all). */
  templates?: readonly HomeTemplate[];
  /** The view to start on (tests and the Templates route). */
  initialView?: HomeView;
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
  /** Recent files ▸ Remove from list (the file itself stays). */
  onRemoveRecent?: (file: RecentFile) => void;
  onOpenTemplate?: (template: HomeTemplate) => void;
  /** Opens a document shared with the user (a row of Shared with me). */
  onOpenShared?: (file: RecentFile) => void;
  onOpenSettings: () => void;
  /** The welcome tour; the button shows when given. */
  onTour?: () => void;
  /** Import Office settings; the button shows when given. */
  onImportSettings?: () => void;
  /** Suite version for the About line. */
  version?: string;
  /**
   * Whether this build opens and saves OpenDocument files (.odt, .ods, .odp): the native core
   * does, the web core does not.
   */
  openDocument?: boolean;
}

interface DoorSpec {
  app: HomeApp;
  product: string;
  noun: string;
  blurb: string;
  /** The New menu's entry ("Blank document"). */
  blank: string;
  /** The door's accessible name ("New document in Kiln Write"). */
  action: string;
  formats: readonly string[];
}

const DOORS: readonly DoorSpec[] = [
  {
    app: 'write',
    product: 'Kiln Write',
    noun: msg('Document'),
    blurb: msg('Letters, essays, reports'),
    blank: msg('Blank document'),
    action: msg('New document in Kiln Write'),
    formats: ['.docx', '.odt'],
  },
  {
    app: 'grid',
    product: 'Kiln Grid',
    noun: msg('Workbook'),
    blurb: msg('Budgets, lists, models'),
    blank: msg('Blank workbook'),
    action: msg('New workbook in Kiln Grid'),
    formats: ['.xlsx', '.csv'],
  },
  {
    app: 'deck',
    product: 'Kiln Deck',
    noun: msg('home.door.deck'),
    blurb: msg('Talks, pitches, lessons'),
    blank: msg('Blank deck'),
    action: msg('New deck in Kiln Deck'),
    formats: ['.pptx', '.odp'],
  },
  {
    app: 'notes',
    product: 'Kiln Notes',
    // i18n-ignore: Kiln Notes is not part of v0.1.
    noun: 'Notebook',
    blurb: 'Ideas, logs, clippings',
    blank: 'Blank notebook',
    action: 'New notebook in Kiln Notes',
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
  if (n < 1e9) return t('{n, number} MB', { n: Math.max(0, Math.round(n / 1e6)) });
  return t('{n, number} GB', { n: Number((n / 1e9).toFixed(n < 1e10 ? 1 : 0)) });
}

function splitName(name: string): { stem: string; ext: string } {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? { stem: name.slice(0, dot), ext: name.slice(dot) } : { stem: name, ext: '' };
}

/** Where a recent file is (browser places by name, not by their ids). */
const folderOf = locationOf;

/** Where a shared document lives: the collaboration server of its link. */
function sharedHost(link: string): string {
  const server = new URLSearchParams(link.slice(link.indexOf('?') + 1)).get('server');
  let host = '';
  try {
    host = server ? new URL(server).host : '';
  } catch {
    host = '';
  }
  return host ? t('Shared through {host}', { host }) : t('Shared with you');
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
  initialView = 'home',
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
  onRemoveRecent,
  onOpenTemplate,
  onOpenShared,
  onOpenSettings,
  onTour,
  onImportSettings,
  version,
  openDocument = false,
}: KilnHomeProps) {
  const [view, setView] = useState<HomeView>(initialView);
  const [list, setList] = useState<HomeList>('recent');
  const [filter, setFilter] = useState<HomeApp | null>(null);
  const [query, setQuery] = useState('');
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLElement>(null);
  const stripRef = useRef<HTMLUListElement>(null);
  const mainRef = useRef<HTMLElement>(null);
  const doors = DOORS.filter((d) => HOME_APPS.includes(d.app));
  const openTemplate = (tpl: HomeTemplate) => onOpenTemplate?.(tpl);
  const showView = (v: HomeView) => {
    setView(v);
    window.scrollTo?.({ top: 0 });
    // The new view's heading takes the focus, so a screen reader announces it.
    requestAnimationFrame(() =>
      mainRef.current?.querySelector<HTMLElement>('h1')?.focus({ preventScroll: true }),
    );
  };

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
  // Options ▸ Advanced ▸ Show this number of Recent Documents.
  const [options] = useOfficeOptions();
  const recentCount = options.advanced.recentCount;
  const recentShown = useMemo(
    () => (recent ? limitRecents(recent, recentCount) : recent),
    [recent, recentCount],
  );
  const source = list === 'recent' ? recentShown : list === 'pinned' ? pinned : shared;
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
  // The strip shows the featured templates; a search looks through every template.
  const shownTemplates = useMemo(
    () =>
      templates.filter(
        (tpl) =>
          HOME_APPS.includes(tpl.app) &&
          (!filter || tpl.app === filter) &&
          (q ? templateMatches(tpl, q) : tpl.featured),
      ),
    [templates, filter, q],
  );
  const releasedTemplates = useMemo(
    () => templates.filter((tpl) => HOME_APPS.includes(tpl.app)),
    [templates],
  );
  // The strip takes the featured templates of each app in turn (a letter, a budget, a pitch…).
  const strip = useMemo(() => {
    if (q) return shownTemplates;
    const byApp = HOME_APPS.map((a) => shownTemplates.filter((tpl) => tpl.app === a));
    const out: HomeTemplate[] = [];
    for (let i = 0; out.length < shownTemplates.length; i++)
      for (const list of byApp) if (list[i]) out.push(list[i] as HomeTemplate);
    return out;
  }, [shownTemplates, q]);

  const showList = (l: HomeList) => {
    if (view !== 'home') setView('home');
    setList(l);
    requestAnimationFrame(() => listRef.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }));
  };

  return (
    <div className="kiln-home" data-app="home">
      <nav className="kiln-home__rail" aria-label={t('Kiln Home')}>
        <div className="kiln-home__logo">
          <AppTile app="home" size={30} />
          {/* i18n-ignore: the wordmark */}
          <span className="kiln-home__wordmark">kiln</span>
        </div>
        <NewMenu creatable={creatable} onCreate={onCreate} />
        <ul className="kiln-home__nav">
          <NavItem
            icon={House}
            label={t('Home')}
            current={view === 'home'}
            onClick={() => showView('home')}
          />
          <NavItem icon={Clock} label={t('Recent')} onClick={() => showList('recent')} />
          <NavItem
            icon={Users}
            label={t('Shared with me')}
            count={shared?.length || undefined}
            onClick={() => showList('shared')}
          />
          <NavItem
            icon={Pin}
            label={t('Pinned')}
            count={pinned.length || undefined}
            onClick={() => showList('pinned')}
          />
          {releasedTemplates.length > 0 ? (
            <NavItem
              icon={LayoutTemplate}
              label={t('Templates')}
              count={releasedTemplates.length}
              current={view === 'templates'}
              onClick={() => showView('templates')}
            />
          ) : null}
          <NavItem icon={FolderOpen} label={t('Open file')} shortcut="Mod+O" onClick={onOpenFile} />
        </ul>
        <div className="kiln-home__cap" id="kiln-home-apps">
          {t('Apps')}
        </div>
        <ul className="kiln-home__nav" aria-labelledby="kiln-home-apps">
          {HOME_APPS.map((app) => (
            <li key={app}>
              <button
                type="button"
                className="kiln-home__nav-item"
                aria-pressed={filter === app}
                data-filter={app}
                onClick={() => {
                  if (view !== 'home') setView('home');
                  setFilter((f) => (f === app ? null : app));
                }}
              >
                <AppTile app={app} size={20} />
                <span>{APP_LABEL[app]}</span>
                {filter === app ? <span className="kiln-home__count">{t('Filter')}</span> : null}
              </button>
            </li>
          ))}
        </ul>
        <LocalFirstCard storage={storage} />
        <button type="button" className="kiln-home__nav-item kiln-home__settings" onClick={onOpenSettings}>
          <Icon icon={Settings} size={18} />
          <span>{t('Settings')}</span>
        </button>
      </nav>

      <main className="kiln-home__main" ref={mainRef}>
        <header className="kiln-home__bar">
          <div className="kiln-home__bar-logo" aria-hidden="true">
            <AppTile app="home" size={30} />
            {/* i18n-ignore: the wordmark */}
            <span className="kiln-home__wordmark">kiln</span>
          </div>
          <label className="kiln-home__search">
            <Icon icon={Search} size={15} />
            <span className="kiln-visually-hidden">{t('Search files, templates and help')}</span>
            <input
              ref={searchRef}
              type="search"
              placeholder={t('Search files, templates and help')}
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                if (view !== 'home') setView('home');
              }}
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
              aria-label={t('Search')}
              onClick={() => showView('templates')}
            >
              <Icon icon={Search} size={20} />
            </button>
            <button
              type="button"
              className="kiln-home__icon-btn kiln-home__phone-only"
              aria-label={t('Settings')}
              onClick={onOpenSettings}
            >
              <Icon icon={Settings} size={20} />
            </button>
            {initials ? (
              <span
                className="kiln-avatar"
                role="img"
                aria-label={userName ? t('Signed in as {userName}', { userName }) : t('Signed in')}
              >
                {initials}
              </span>
            ) : null}
          </div>
        </header>

        {view === 'templates' ? (
          <div className="kiln-home__content">
            <TemplateGallery
              templates={releasedTemplates}
              creatable={creatable}
              onCreate={onCreate}
              onOpenTemplate={openTemplate}
            >
              <section className="kiln-home__hello" aria-labelledby="kiln-home-title">
                <div>
                  <p className="kiln-home__eyebrow">
                    <Flame size={14} />
                    <span>{t('Each template opens as a new document. The template stays as it is.')}</span>
                  </p>
                  <h1 id="kiln-home-title" className="kiln-home__title" tabIndex={-1}>
                    <span className="kiln-visually-hidden">{t('Kiln Home.')} </span>
                    {t('Start from a template')}
                  </h1>
                </div>
                <div className="kiln-home__hello-actions">
                  <Button size="md" icon={<Icon icon={House} size={15} />} onClick={() => showView('home')}>
                    {t('Back to Home')}
                  </Button>
                </div>
              </section>
            </TemplateGallery>
          </div>
        ) : (
          <div className="kiln-home__content">
            <section className="kiln-home__hello" aria-labelledby="kiln-home-title">
              <div>
                <p className="kiln-home__eyebrow">
                  <Flame size={14} />
                  <span>{greeting(now, userName)}</span>
                </p>
                <h1 id="kiln-home-title" className="kiln-home__title" tabIndex={-1}>
                  <span className="kiln-visually-hidden">{t('Kiln Home.')} </span>
                  {t('What are we making today?')}
                </h1>
              </div>
              <div className="kiln-home__hello-actions">
                <Button size="md" icon={<Icon icon={FolderOpen} size={15} />} onClick={onOpenFile}>
                  {t('Open file')}
                </Button>
                {releasedTemplates.length > 0 ? (
                  <Button
                    size="md"
                    icon={<Icon icon={LayoutTemplate} size={15} />}
                    onClick={() => showView('templates')}
                  >
                    {t('All templates')}
                  </Button>
                ) : null}
              </div>
            </section>

            <section
              className="kiln-home__doors"
              aria-label={t('Create')}
              style={{ '--doors': doors.length + (releasedTemplates.length > 0 ? 1 : 0) } as CSSProperties}
            >
              {doors.map((d) => (
                <Door
                  key={d.app}
                  spec={openDocument ? d : { ...d, formats: d.formats.filter((f) => !f.startsWith('.od')) }}
                  available={creatable.includes(d.app)}
                  onCreate={() => onCreate(d.app)}
                />
              ))}
              {releasedTemplates.length > 0 ? (
                <TemplatesDoor count={releasedTemplates.length} onOpen={() => showView('templates')} />
              ) : null}
            </section>

            {switcherDismissed ? null : (
              <section className="kiln-home__switch" aria-label={t('Coming from Microsoft Office')}>
                <div className="kiln-home__formats" aria-hidden="true">
                  <span style={{ background: 'var(--kiln-write-solid)', '--r': '-6deg' } as CSSProperties}>
                    {/* i18n-ignore: a file extension */}
                    .docx
                  </span>
                  <span style={{ background: 'var(--kiln-grid-solid)', '--r': '2deg' } as CSSProperties}>
                    {/* i18n-ignore: a file extension */}
                    .xlsx
                  </span>
                  <span style={{ background: 'var(--kiln-deck-solid)', '--r': '8deg' } as CSSProperties}>
                    {/* i18n-ignore: a file extension */}
                    .pptx
                  </span>
                </div>
                <div className="kiln-home__switch-text">
                  <h2>{t('Coming from Microsoft Office?')}</h2>
                  <p>
                    {t(
                      'Your .docx, .xlsx and .pptx open as they are. The ribbon is where you left it, and your shortcuts still work.',
                    )}
                  </p>
                </div>
                <div className="kiln-home__switch-actions">
                  {onTour ? (
                    <Button variant="ember" icon={<Spark size={13} />} onClick={onTour}>
                      {t('Take the tour')}
                    </Button>
                  ) : null}
                  {onImportSettings ? (
                    <Button icon={<Icon icon={Upload} size={15} />} onClick={onImportSettings}>
                      {t('Import settings')}
                    </Button>
                  ) : null}
                  {onDismissSwitcher ? (
                    <button
                      type="button"
                      className="kiln-home__icon-btn"
                      aria-label={t('Dismiss')}
                      onClick={onDismissSwitcher}
                    >
                      <Icon icon={X} size={15} />
                    </button>
                  ) : null}
                </div>
              </section>
            )}

            <div className="kiln-home__lower" data-templates={shownTemplates.length > 0 || undefined}>
              <section ref={listRef} className="kiln-home__files" aria-label={t('Files')}>
                <div className="kiln-home__files-head">
                  <ListSwitch value={list} onChange={setList} />
                  {filter ? (
                    <button type="button" className="kiln-home__filter-chip" onClick={() => setFilter(null)}>
                      <AppTile app={filter} size={14} />
                      {t('{app} only', { app: APP_LABEL[filter] })}
                      <Icon icon={X} size={12} />
                    </button>
                  ) : null}
                </div>
                <FileTable
                  list={list}
                  rows={rows}
                  loading={source === null && list !== 'shared'}
                  sharedUnavailable={list === 'shared' && (shared === null || shared.length === 0)}
                  filtered={!!filter || !!q}
                  now={now}
                  onOpen={list === 'shared' && onOpenShared ? onOpenShared : onOpenRecent}
                  onTogglePin={list === 'shared' ? undefined : onTogglePin}
                  onRemove={onRemoveRecent}
                  onOpenFile={onOpenFile}
                  onShowTemplates={releasedTemplates.length > 0 ? () => showView('templates') : undefined}
                  onClearFilter={() => {
                    setFilter(null);
                    setQuery('');
                    searchRef.current?.focus();
                  }}
                />
              </section>
              {shownTemplates.length > 0 ? (
                <section className="kiln-home__templates" aria-labelledby="kiln-home-templates">
                  <div className="kiln-home__section-head">
                    <h2 id="kiln-home-templates">{q ? t('Templates') : t('Fresh from the kiln')}</h2>
                    <button type="button" className="kiln-home__link" onClick={() => showView('templates')}>
                      {q && shownTemplates.length > 4
                        ? t('home.templates.more', { count: shownTemplates.length - 4 })
                        : t('All templates ›')}
                    </button>
                  </div>
                  <ul
                    className="kiln-home__template-grid"
                    ref={stripRef}
                    onKeyDown={(e) => onCardKeys(e, stripRef.current)}
                  >
                    {strip.slice(0, 4).map((tpl) => (
                      <li key={tpl.id}>
                        <TemplateCard template={tpl} onOpen={openTemplate} compact />
                      </li>
                    ))}
                  </ul>
                </section>
              ) : null}
            </div>
            {version ? (
              <p className="kiln-home__version" data-testid="suite-version">
                {t('Version {version}', { version })}
              </p>
            ) : null}
          </div>
        )}
      </main>

      <nav className="kiln-home__tabbar" aria-label={t('Kiln Home sections')}>
        <button
          type="button"
          aria-current={view === 'home' ? 'page' : undefined}
          onClick={() => showView('home')}
        >
          <Icon icon={House} size={22} />
          <span>{t('Home')}</span>
        </button>
        <button type="button" onClick={onOpenFile}>
          <Icon icon={FolderOpen} size={22} />
          <span>{t('Files')}</span>
        </button>
        <button type="button" onClick={() => showList('shared')}>
          <Icon icon={Users} size={22} />
          <span>{t('Shared')}</span>
        </button>
        <button
          type="button"
          aria-current={view === 'templates' ? 'page' : undefined}
          onClick={() => showView('templates')}
        >
          <Icon icon={LayoutTemplate} size={22} />
          <span>{t('Templates')}</span>
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

/** The ember New button: a menu of the document types of the release. */
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
        <Icon icon={Plus} size={18} className="kiln-home__new-plus" />
        {/* The phone's floating New button carries the spark (phone mockup). */}
        <Spark size={16} className="kiln-home__new-spark" />
        <span>{t('New')}</span>
        <Kbd shortcut="Mod+N" variant="inline" className="kiln-home__new-kbd" />
      </button>
      {open ? (
        <div className="kiln-home__new-menu" role="menu" id={id}>
          {DOORS.filter((d) => HOME_APPS.includes(d.app)).map((d) => (
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
                {tx(d.blank)}
                <small>{d.product}</small>
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** The fourth door: the kiln's own, opening the template gallery. */
function TemplatesDoor({ count, onOpen }: { count: number; onOpen: () => void }) {
  return (
    <button
      type="button"
      className="kiln-door kiln-door--templates"
      data-app="templates"
      data-door="templates"
      style={{ maskImage: DOOR_MASK, WebkitMaskImage: DOOR_MASK } as CSSProperties}
      onClick={onOpen}
    >
      <span className="kiln-visually-hidden">{t('Browse templates: ')}</span>
      <span className="kiln-door__rim" aria-hidden="true" />
      <AppTile app="home" size={92} className="kiln-door__tile" />
      <span className="kiln-door__product">{t('Start from')}</span>
      <span className="kiln-door__noun">{t('Templates')}</span>
      <span className="kiln-door__blurb">{t('home.templates.ready', { count })}</span>
      <span className="kiln-door__meta" aria-hidden="true">
        {HOME_APPS.map((a) => (
          <span key={a} className="kiln-door__chip kiln-door__chip--tile">
            <AppTile app={a} size={16} />
          </span>
        ))}
        <span className="kiln-door__plus">
          <Icon icon={ArrowRight} size={18} />
        </span>
      </span>
    </button>
  );
}

function Door({ spec, available, onCreate }: { spec: DoorSpec; available: boolean; onCreate: () => void }) {
  const label = tx(spec.action);
  const body = (
    <>
      <span className="kiln-visually-hidden">
        {available ? t('{label}: ', { label }) : t('{label} (not in this build yet): ', { label })}
      </span>
      <span className="kiln-door__rim" aria-hidden="true" />
      <AppTile app={spec.app} size={92} className="kiln-door__tile" />
      <span className="kiln-door__product">{spec.product}</span>
      <span className="kiln-door__noun">{tx(spec.noun)}</span>
      <span className="kiln-door__blurb">{tx(spec.blurb)}</span>
      <span className="kiln-door__meta">
        {spec.formats.map((f) => (
          <span key={f} className="kiln-door__chip">
            {f}
          </span>
        ))}
        <span className="kiln-door__plus" aria-hidden="true">
          {available ? <Icon icon={Plus} size={18} /> : <span className="kiln-door__soon">{t('Soon')}</span>}
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
    ['recent', t('Recent')],
    ['pinned', t('Pinned')],
    ['shared', t('Shared with me')],
  ];
  const refs = useRef(new Map<HomeList, HTMLButtonElement>());
  const onKey = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const step = readingStep(e.key, e.currentTarget);
    const next = step ? (i + step + items.length) % items.length : -1;
    if (next < 0) return;
    e.preventDefault();
    const id = items[next]?.[0];
    if (!id) return;
    onChange(id);
    refs.current.get(id)?.focus();
  };
  return (
    <div className="kiln-home__seg" role="tablist" aria-label={t('File lists')}>
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
  onRemove,
  onOpenFile,
  onShowTemplates,
  onClearFilter,
}: {
  list: HomeList;
  rows: readonly RecentFile[];
  loading: boolean;
  sharedUnavailable: boolean;
  filtered: boolean;
  now: Date;
  onOpen: (f: RecentFile) => void;
  onTogglePin?: (f: RecentFile) => void;
  onRemove?: (f: RecentFile) => void;
  onOpenFile: () => void;
  onShowTemplates?: () => void;
  onClearFilter: () => void;
}) {
  const body = useRef<HTMLTableSectionElement>(null);
  if (loading) {
    return (
      <div className="kiln-home__empty" role="status">
        {t('Looking for your files…')}
      </div>
    );
  }
  if (rows.length === 0) {
    const [icon, title, text, actions]: [typeof Pin, string, ReactNode, ReactNode] = filtered
      ? [
          Search,
          t('No matches'),
          t('Try another word, or clear the app filter.'),
          <Button key="clear" size="sm" icon={<Icon icon={X} size={14} />} onClick={onClearFilter}>
            {t('Clear the search and filter')}
          </Button>,
        ]
      : list === 'shared' && sharedUnavailable
        ? [
            Users,
            t('Nothing shared yet'),
            t('When someone sends you a link to a shared document, open it once and it appears here.'),
            null,
          ]
        : list === 'pinned'
          ? [
              Pin,
              t('Nothing pinned yet'),
              t('Pin a file you keep coming back to and it stays at the top.'),
              null,
            ]
          : [
              Clock,
              t('Your recent files will gather here'),
              t('Create something with one of the doors above, or open a file from this device.'),
              <>
                <Button size="sm" icon={<Icon icon={FolderOpen} size={14} />} onClick={onOpenFile}>
                  {t('Open file')}
                </Button>
                {onShowTemplates ? (
                  <Button size="sm" icon={<Icon icon={LayoutTemplate} size={14} />} onClick={onShowTemplates}>
                    {t('Browse templates')}
                  </Button>
                ) : null}
              </>,
            ];
    return (
      <div className="kiln-home__empty" role="status" data-empty={filtered ? 'search' : list}>
        <span className="kiln-home__empty-icon" aria-hidden="true">
          <Icon icon={icon} size={20} />
        </span>
        <strong>{title}</strong>
        <span>{text}</span>
        {actions ? <span className="kiln-home__empty-actions">{actions}</span> : null}
      </div>
    );
  }
  // Up and Down move between the files, Home and End to the first and last.
  const onRowKeys = (e: KeyboardEvent<HTMLTableSectionElement>) => {
    const target = e.target as HTMLElement;
    if (!target.matches('.kiln-home__file')) return;
    const files = [...(body.current?.querySelectorAll<HTMLElement>('.kiln-home__file') ?? [])];
    const i = files.indexOf(target);
    const next =
      e.key === 'ArrowDown'
        ? i + 1
        : e.key === 'ArrowUp'
          ? i - 1
          : e.key === 'Home'
            ? 0
            : e.key === 'End'
              ? files.length - 1
              : -2;
    if (next === -2) return;
    e.preventDefault();
    files[Math.max(0, Math.min(files.length - 1, next))]?.focus();
  };
  return (
    <table className="kiln-home__table">
      <thead>
        <tr>
          <th scope="col">{t('Name')}</th>
          <th scope="col">{t('Opened')}</th>
          <th scope="col" className="kiln-home__col-where">
            {t('Location')}
          </th>
          <th scope="col">
            <span className="kiln-visually-hidden">{t('Pin')}</span>
          </th>
        </tr>
      </thead>
      <tbody ref={body} onKeyDown={onRowKeys}>
        {rows.map((f) => {
          const { stem, ext } = splitName(f.name);
          const app = kindApp(f.kind);
          const where = list === 'shared' ? sharedHost(f.path) : folderOf(f.path) || t('This device');
          return (
            <tr key={f.path}>
              <td>
                <button type="button" className="kiln-home__file" onClick={() => onOpen(f)}>
                  <AppTile app={app} size={24} />
                  <span>
                    <b>{stem}</b>
                    <span className="kiln-home__ext">{ext}</span>
                    <small>{where}</small>
                  </span>
                </button>
              </td>
              <td className="kiln-home__muted">{relativeTime(f.opened_at_ms, now)}</td>
              <td className="kiln-home__muted kiln-home__col-where">{where}</td>
              <td className="kiln-home__pin-cell">
                {onTogglePin ? (
                  <Tooltip content={f.pinned ? t('Unpin') : t('Pin to the top')}>
                    <button
                      type="button"
                      className="kiln-home__icon-btn"
                      aria-label={
                        f.pinned ? t('Unpin {name}', { name: f.name }) : t('Pin {name}', { name: f.name })
                      }
                      aria-pressed={f.pinned}
                      data-command="suite.home.recent_pin"
                      onClick={() => onTogglePin(f)}
                    >
                      <Icon icon={f.pinned ? PinOff : Pin} size={15} />
                    </button>
                  </Tooltip>
                ) : null}
                {onRemove && list !== 'shared' ? (
                  <Tooltip content={t('Remove from list')}>
                    <button
                      type="button"
                      className="kiln-home__icon-btn"
                      aria-label={t('Remove {name} from the list', { name: f.name })}
                      data-command="suite.home.recent_remove"
                      onClick={() => onRemove(f)}
                    >
                      <Icon icon={X} size={15} />
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
        {t('Local first')}
      </strong>
      <span>{t('Your files live on this device. Sync is optional.')}</span>
      {pct !== null && storage ? (
        <>
          {/* biome-ignore lint/a11y/useSemanticElements: a styled bar; <meter> cannot take the kit's gradient across browsers */}
          <span
            className="kiln-home__meter"
            role="meter"
            aria-label={t('Storage used')}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(pct)}
          >
            <i style={{ width: `${pct}%` }} />
          </span>
          <span className="kiln-home__tnum">
            {t('{used} of {quota}', { used: formatBytes(storage.used), quota: formatBytes(storage.quota) })}
          </span>
        </>
      ) : null}
    </div>
  );
}
