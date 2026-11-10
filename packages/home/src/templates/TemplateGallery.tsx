/**
 * The template gallery: Kiln Home ▸ Templates and File ▸ New. Blank documents first, then
 * every template of the apps of the release, grouped by app, with search and per-app filters.
 * Thumbnails are the templates' first page, slide or sheet as `kiln render` drew it.
 *
 * Presentation only: templates come in through props and choosing one goes out through
 * `onOpenTemplate`; the host opens it as a new, untitled document (the template stays as it is).
 * Keyboard: Tab reaches the search, the filters and every card; the arrow keys move between cards
 * (left and right in reading order, up and down by row), Home and End to the first and last.
 */
import { msg, readingStep, t, tx } from '@kiln/i18n';
import { AppTile, Button, Flame, Icon } from '@kiln/ui-kit';
import { Search, X } from 'lucide-react';
import { type KeyboardEvent, type ReactNode, useId, useMemo, useRef, useState } from 'react';
import { HOME_APPS, type HomeApp } from '../model.ts';
import './gallery.css';

/** One template, as Home and the galleries show it. */
export interface HomeTemplate {
  id: string;
  app: HomeApp;
  /** English name (translated for display through the interface messages). */
  name: string;
  /** The bundle, relative to the templates folder (`write/letter.kwrite`). */
  file?: string;
  /** Thumbnail URL (rendered by `kiln render`). */
  thumbnail?: string;
  /** One English line about it. */
  description?: string;
  /** Shown in Home's featured strip. */
  featured?: boolean;
}

/** What each app's documents are called in the gallery's filters and headings. */
export const APP_KINDS: Readonly<Record<HomeApp, { plural: string; product: string; blank: string }>> = {
  write: { plural: msg('Documents'), product: 'Kiln Write', blank: msg('Blank document') },
  grid: { plural: msg('Workbooks'), product: 'Kiln Grid', blank: msg('Blank workbook') },
  deck: { plural: msg('Presentations'), product: 'Kiln Deck', blank: msg('Blank presentation') },
  // i18n-ignore: Kiln Notes is not part of this release (RELEASE_APPS).
  notes: { plural: 'Notebooks', product: 'Kiln Notes', blank: 'Blank notebook' },
};

/** Whether a template matches a search (its name and description, in English and as shown). */
export function templateMatches(template: HomeTemplate, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const fields = [
    template.name,
    tx(template.name),
    template.description ?? '',
    tx(template.description ?? ''),
    tx(APP_KINDS[template.app].plural),
    APP_KINDS[template.app].product,
  ];
  return q.split(/\s+/).every((word) => fields.some((f) => f.toLowerCase().includes(word)));
}

/** Moves focus between the cards of a gallery with the arrow keys, Home and End. */
export function onCardKeys(e: KeyboardEvent<HTMLElement>, root: HTMLElement | null): void {
  const target = e.target as HTMLElement;
  if (!root || !target.matches('.kiln-tcard')) return;
  const cards = [...root.querySelectorAll<HTMLElement>('.kiln-tcard')];
  const i = cards.indexOf(target);
  if (i < 0) return;
  let next = -1;
  const step = readingStep(e.key, target);
  if (step) next = i + step;
  else if (e.key === 'Home') next = 0;
  else if (e.key === 'End') next = cards.length - 1;
  else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    const here = target.getBoundingClientRect();
    const down = e.key === 'ArrowDown';
    const cx = here.left + here.width / 2;
    let best = -1;
    let bestScore = Number.POSITIVE_INFINITY;
    cards.forEach((c, j) => {
      const r = c.getBoundingClientRect();
      const dy = down ? r.top - here.bottom : here.top - r.bottom;
      if (dy < -2) return;
      const score = dy * 4 + Math.abs(r.left + r.width / 2 - cx);
      if (score < bestScore) {
        bestScore = score;
        best = j;
      }
    });
    next = best;
  } else return;
  e.preventDefault();
  cards[Math.max(0, Math.min(cards.length - 1, next))]?.focus();
}

/** A template's card: thumbnail, app and name, one line about it. */
export function TemplateCard({
  template,
  onOpen,
  compact = false,
}: {
  template: HomeTemplate;
  onOpen: (t: HomeTemplate) => void;
  compact?: boolean;
}) {
  return (
    <button
      type="button"
      className="kiln-tcard"
      data-app={template.app}
      data-template={template.id}
      onClick={() => onOpen(template)}
    >
      <span className="kiln-tcard__thumb" data-app={template.app}>
        {template.thumbnail ? (
          <img src={template.thumbnail} alt="" loading="lazy" decoding="async" draggable={false} />
        ) : (
          <AppTile app={template.app} size={40} />
        )}
      </span>
      <span className="kiln-tcard__name">
        <AppTile app={template.app} size={16} />
        <span>{tx(template.name)}</span>
      </span>
      {!compact && template.description ? (
        <span className="kiln-tcard__desc">{tx(template.description)}</span>
      ) : null}
    </button>
  );
}

function BlankCard({ app, enabled, onCreate }: { app: HomeApp; enabled: boolean; onCreate: () => void }) {
  return (
    <button
      type="button"
      className="kiln-tcard kiln-tcard--blank"
      data-app={app}
      data-blank={app}
      disabled={!enabled}
      onClick={onCreate}
    >
      <span className="kiln-tcard__thumb kiln-tcard__blank" data-app={app} aria-hidden="true">
        <span className="kiln-tcard__sheet" data-app={app}>
          <AppTile app={app} size={34} />
        </span>
      </span>
      <span className="kiln-tcard__name">
        <span>{tx(APP_KINDS[app].blank)}</span>
      </span>
      <span className="kiln-tcard__desc">{APP_KINDS[app].product}</span>
    </button>
  );
}

export interface TemplateGalleryProps {
  templates: readonly HomeTemplate[];
  /** Apps the core creates documents for (blank cards of the others are disabled). */
  creatable?: readonly HomeApp[];
  /** The app listed first (File ▸ New in an app's window). */
  app?: HomeApp | null;
  onCreate: (app: HomeApp) => void;
  onOpenTemplate: (template: HomeTemplate) => void;
  /** Focus the search when the gallery opens. */
  autoFocus?: boolean;
  /** A heading above everything (Home's Templates view has its own). */
  children?: ReactNode;
}

/** The gallery of blank documents and templates, with search and per-app filters. */
export function TemplateGallery({
  templates,
  creatable = HOME_APPS,
  app = null,
  onCreate,
  onOpenTemplate,
  autoFocus = false,
  children,
}: TemplateGalleryProps) {
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<HomeApp | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const id = useId();
  const apps = useMemo(
    () => (app && HOME_APPS.includes(app) ? [app, ...HOME_APPS.filter((a) => a !== app)] : [...HOME_APPS]),
    [app],
  );
  const shown = useMemo(
    () =>
      templates.filter(
        (tpl) =>
          HOME_APPS.includes(tpl.app) && (!filter || tpl.app === filter) && templateMatches(tpl, query),
      ),
    [templates, filter, query],
  );
  const searching = query.trim() !== '';
  const blanks = apps.filter(
    (a) =>
      (!filter || a === filter) &&
      (!searching ||
        templateMatches(
          { id: a, app: a, name: APP_KINDS[a].blank, description: APP_KINDS[a].product },
          query,
        )),
  );
  const groups = apps
    .filter((a) => !filter || a === filter)
    .map((a) => ({ app: a, items: shown.filter((tpl) => tpl.app === a) }))
    .filter((g) => g.items.length > 0);
  const nothing = shown.length === 0 && blanks.length === 0;

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: arrow-key navigation between the cards inside
    <div className="kiln-tgallery" ref={root} onKeyDown={(e) => onCardKeys(e, root.current)}>
      {children}
      <div className="kiln-tgallery__bar">
        <label className="kiln-tgallery__search">
          <Icon icon={Search} size={15} />
          <span className="kiln-visually-hidden">{t('Search templates')}</span>
          <input
            ref={searchRef}
            type="search"
            // biome-ignore lint/a11y/noAutofocus: the gallery is opened to search it (File ▸ New, Templates)
            autoFocus={autoFocus}
            placeholder={t('Search templates')}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape' && query) {
                e.stopPropagation();
                setQuery('');
              }
            }}
          />
          {query ? (
            <button
              type="button"
              className="kiln-tgallery__clear"
              aria-label={t('Clear search')}
              onClick={() => {
                setQuery('');
                searchRef.current?.focus();
              }}
            >
              <Icon icon={X} size={14} />
            </button>
          ) : null}
        </label>
        {/* biome-ignore lint/a11y/useSemanticElements: a row of toggle buttons, as Office's gallery filters */}
        <div className="kiln-tgallery__filters" role="group" aria-label={t('Show templates for')}>
          <button
            type="button"
            className="kiln-tgallery__filter"
            aria-pressed={filter === null}
            onClick={() => setFilter(null)}
          >
            {t('All')}
          </button>
          {apps.map((a) => (
            <button
              key={a}
              type="button"
              className="kiln-tgallery__filter"
              data-app={a}
              aria-pressed={filter === a}
              onClick={() => setFilter((f) => (f === a ? null : a))}
            >
              <AppTile app={a} size={16} />
              {tx(APP_KINDS[a].plural)}
            </button>
          ))}
        </div>
        <p className="kiln-tgallery__count" aria-live="polite">
          {t('gallery.count', { count: shown.length })}
        </p>
      </div>

      {nothing ? (
        <div className="kiln-tgallery__empty" role="status">
          <Flame size={22} />
          <strong>{t('No templates match “{query}”', { query: query.trim() })}</strong>
          <span>{t('Try another word, or show every app.')}</span>
          <Button
            size="sm"
            onClick={() => {
              setQuery('');
              setFilter(null);
              searchRef.current?.focus();
            }}
          >
            {t('Show every template')}
          </Button>
        </div>
      ) : null}

      {blanks.length > 0 ? (
        <section className="kiln-tgallery__section" aria-labelledby={`${id}-blank`}>
          <h2 id={`${id}-blank`} className="kiln-tgallery__h2">
            {t('Start blank')}
          </h2>
          <ul className="kiln-tgallery__grid">
            {blanks.map((a) => (
              <li key={a}>
                <BlankCard app={a} enabled={creatable.includes(a)} onCreate={() => onCreate(a)} />
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {groups.map((g) => (
        <section key={g.app} className="kiln-tgallery__section" aria-labelledby={`${id}-${g.app}`}>
          <h2 id={`${id}-${g.app}`} className="kiln-tgallery__h2">
            {tx(APP_KINDS[g.app].plural)}
            <small>{APP_KINDS[g.app].product}</small>
          </h2>
          <ul className="kiln-tgallery__grid">
            {g.items.map((tpl) => (
              <li key={tpl.id}>
                <TemplateCard template={tpl} onOpen={onOpenTemplate} />
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
