/**
 * Review ▸ Version History (Office's Version History pane): the shared document's versions,
 * newest first. Selecting one shows it read-only in place of the document; from there it can be
 * restored (as a new version: nothing is lost), named, and, in Kiln Write, compared word by word
 * with the document. Kiln Deck shows the version's slides with the ones that changed marked.
 */
import { formatDate, msg, t, tx } from '@kiln/i18n';
import { Message } from '@kiln/i18n/react';
import type {
  SceneDelta,
  VersionEntry,
  VersionParagraph,
  VersionSlide,
  VersionSlideMark,
} from '@kiln/protocol';
import { Painter, type PainterCanvas, paletteFor, SceneCache } from '@kiln/surface';
import { History, PencilLine, RotateCcw, Share2 } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import './collab.css';

export interface VersionHistoryPaneProps {
  /** Whether the document is shared (everyone who has it sees the same versions). */
  shared: boolean;
  /** the document keeps versions on this device (a file that is not shared). */
  local?: boolean;
  /** The versions, oldest first (as the core lists them). */
  versions: readonly VersionEntry[];
  /** The version shown in place of the document (`null`: the document). */
  selected: string | null;
  onSelect: (id: string | null) => void;
  onRestore: (id: string) => void;
  onRename: (id: string, name: string) => void;
  /** Opens the Share dialog (an unshared document has no versions). */
  onShare: () => void;
  /** Kiln Write: the comparison of the selected version with the document, once asked. */
  comparison?: readonly VersionParagraph[] | null;
  /** Kiln Write: asks for the comparison (`null` hides it). */
  onCompare?: (id: string | null) => void;
  /** Kiln Deck: the selected version's slides and their thumbnails. */
  slides?: { slides: readonly VersionSlide[]; scene: SceneDelta } | null;
  theme?: 'light' | 'dark';
  /** The clock, for "Today" and "Yesterday". */
  now?: number;
}

const ORIGIN = (v: VersionEntry, all: readonly VersionEntry[], now: number): string => {
  const o = v.origin;
  if (o === 'Shared') return t('Shared');
  if (o === 'Save') return t('Saved');
  if (o === 'Auto') return t('While editing');
  const from = all.find((x) => x.id === o.Restore);
  return from ? t('Restored from {when}', { when: when(from.time_ms, now) }) : t('Restored');
};

/** "Today, 14:05", "Yesterday, 09:12" or "21 Oct 2026, 16:40". */
export function when(ms: number, now: number = Date.now()): string {
  const d = new Date(ms);
  const time = formatDate(d, { hour: '2-digit', minute: '2-digit' });
  const day = (at: number) => Math.floor((at - new Date(at).getTimezoneOffset() * 60_000) / 86_400_000);
  const ago = day(now) - day(ms);
  if (ago === 0) return t('Today, {time}', { time });
  if (ago === 1) return t('Yesterday, {time}', { time });
  const date = formatDate(d, { day: 'numeric', month: 'short', year: 'numeric' });
  return t('{date}, {time}', { date, time });
}

const MARKS: Record<VersionSlideMark, string | null> = {
  Same: null,
  Added: msg('New'),
  Changed: msg('Changed'),
};

/** One slide thumbnail of a self-contained scene. */
function Thumb({ canvas, label }: { canvas: PainterCanvas | undefined; label: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || !canvas) return;
    const src = canvas as unknown as { width: number; height: number };
    el.width = src.width;
    el.height = src.height;
    el.getContext('2d')?.drawImage(canvas as unknown as CanvasImageSource, 0, 0);
  }, [canvas]);
  return <canvas ref={ref} className="kiln-versions__thumb" role="img" aria-label={label} />;
}

function paint(scene: SceneDelta, theme: 'light' | 'dark'): Map<number, PainterCanvas> {
  const cache = new SceneCache<PainterCanvas>();
  const out = new Map<number, PainterCanvas>();
  try {
    cache.apply(scene);
  } catch {
    return out;
  }
  const painter = new Painter(paletteFor(theme, 'deck'));
  for (const t of cache.tiles.values()) {
    const c = painter.ensureTile(cache, t);
    if (c) out.set(t.tile.id.index, c);
  }
  return out;
}

export function VersionHistoryPane({
  shared,
  local = false,
  versions,
  selected,
  onSelect,
  onRestore,
  onRename,
  onShare,
  comparison,
  onCompare,
  slides,
  theme = 'light',
  now = Date.now(),
}: VersionHistoryPaneProps) {
  const newest = useMemo(() => [...versions].reverse(), [versions]);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const thumbs = useMemo(() => (slides ? paint(slides.scene, theme) : null), [slides, theme]);
  useEffect(() => setRenaming(null), [selected]);

  if (!shared && local && versions.length === 0) {
    return (
      <div className="kiln-versions kiln-versions--empty" data-testid="version-history">
        <Icon icon={History} size={28} className="kiln-versions__empty-icon" />
        <p>
          {t(
            'Kiln keeps a version of this document each time you save it, and one for every ten minutes of editing. Save it to keep the first one.',
          )}
        </p>
        <Button variant="ghost" icon={<Icon icon={Share2} size={14} />} onClick={onShare}>
          {t('Share to edit together…')}
        </Button>
      </div>
    );
  }

  if (!shared && !local) {
    return (
      <div className="kiln-versions kiln-versions--empty" data-testid="version-history">
        <Icon icon={History} size={28} className="kiln-versions__empty-icon" />
        <p>
          {t(
            'Kiln keeps the versions of a shared document: one each time someone saves it, and one for every ten minutes of editing. Everyone who has the document sees the same list.',
          )}
        </p>
        <Button variant="primary" icon={<Icon icon={Share2} size={14} />} onClick={onShare}>
          {t('Share…')}
        </Button>
      </div>
    );
  }

  const current = selected === null;
  return (
    <div className="kiln-versions" data-testid="version-history">
      <ul className="kiln-versions__list" aria-label={t('Versions')}>
        <li>
          <button
            type="button"
            className={`kiln-versions__item${current ? ' is-selected' : ''}`}
            aria-current={current ? 'true' : undefined}
            onClick={() => onSelect(null)}
            data-testid="version-current"
          >
            <span className="kiln-versions__time">{t('Current version')}</span>
            <span className="kiln-versions__meta">{t('The document as it is now')}</span>
          </button>
        </li>
        {newest.map((v) => {
          const on = v.id === selected;
          return (
            <li key={v.id}>
              <button
                type="button"
                className={`kiln-versions__item${on ? ' is-selected' : ''}`}
                aria-current={on ? 'true' : undefined}
                onClick={() => onSelect(on ? null : v.id)}
                data-version={v.id}
              >
                <span className="kiln-versions__time">{when(v.time_ms, now)}</span>
                <span className="kiln-versions__meta">
                  {v.name ? <strong>{v.name}</strong> : ORIGIN(v, versions, now)}
                  <span aria-hidden="true"> · </span>
                  {v.author}
                </span>
              </button>
              {on ? (
                <div className="kiln-versions__detail">
                  <p className="kiln-versions__hint">
                    {t('You are looking at this version; the document stays as it is until you restore it.')}
                  </p>
                  <div className="kiln-versions__actions">
                    <Button
                      variant="primary"
                      size="sm"
                      icon={<Icon icon={RotateCcw} size={14} />}
                      onClick={() => onRestore(v.id)}
                      data-testid="version-restore"
                    >
                      {t('Restore')}
                    </Button>
                    {onCompare ? (
                      <Button
                        size="sm"
                        aria-pressed={!!comparison}
                        onClick={() => onCompare(comparison ? null : v.id)}
                        data-testid="version-compare"
                      >
                        {comparison ? t('Hide changes') : t('Compare')}
                      </Button>
                    ) : null}
                    <Button
                      size="sm"
                      variant="ghost"
                      icon={<Icon icon={PencilLine} size={14} />}
                      onClick={() => {
                        setDraft(v.name ?? '');
                        setRenaming(v.id);
                      }}
                    >
                      {t('Name')}
                    </Button>
                  </div>
                  {renaming === v.id ? (
                    <form
                      className="kiln-versions__rename"
                      onSubmit={(e) => {
                        e.preventDefault();
                        onRename(v.id, draft.trim());
                        setRenaming(null);
                      }}
                    >
                      <input
                        className="kiln-field kiln-field--sm"
                        aria-label={t('Version name')}
                        value={draft}
                        maxLength={120}
                        // biome-ignore lint/a11y/noAutofocus: the field the Name button opened.
                        autoFocus
                        onChange={(e) => setDraft(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Escape') {
                            e.stopPropagation();
                            setRenaming(null);
                          }
                        }}
                      />
                      <Button size="sm" type="submit">
                        {t('Save')}
                      </Button>
                    </form>
                  ) : null}
                  {comparison ? <Comparison paragraphs={comparison} /> : null}
                  {slides && thumbs ? (
                    <ol className="kiln-versions__slides" aria-label={t('Slides of this version')}>
                      {slides.slides.map((s, i) => {
                        const mark = MARKS[s.mark];
                        return (
                          <li
                            key={s.slide}
                            className={mark ? 'is-marked' : undefined}
                            data-mark={s.mark}
                            data-testid="version-slide"
                          >
                            <Thumb
                              canvas={thumbs.get(i)}
                              label={
                                s.mark === 'Added'
                                  ? t('Slide {n, number}, new', { n: i + 1 })
                                  : s.mark === 'Changed'
                                    ? t('Slide {n, number}, changed', { n: i + 1 })
                                    : t('Slide {n, number}', { n: i + 1 })
                              }
                            />
                            <span className="kiln-versions__slide-no">{i + 1}</span>
                            {mark ? <span className="kiln-versions__mark">{tx(mark)}</span> : null}
                          </li>
                        );
                      })}
                    </ol>
                  ) : null}
                </div>
              ) : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** Kiln Write's word-level comparison: the paragraphs that differ, with what was added and removed. */
function Comparison({ paragraphs }: { paragraphs: readonly VersionParagraph[] }) {
  const changed = paragraphs.filter((p) => p.change !== 'Same' || p.spans.some((s) => s.change !== 'Same'));
  if (changed.length === 0)
    return <p className="kiln-versions__hint">{t('No differences from the current version.')}</p>;
  return (
    <div className="kiln-versions__diff" data-testid="version-diff">
      <p className="kiln-versions__hint">
        <Message
          id="Changes since this version: {added} and {removed}."
          values={{ added: <ins>{t('added')}</ins>, removed: <del>{t('removed')}</del> }}
        />
      </p>
      {changed.map((p, i) => (
        // The paragraphs of a comparison have no stable key of their own; their order is the
        // document's.
        <p key={i} className={`kiln-versions__para kiln-versions__para--${p.change.toLowerCase()}`}>
          {p.spans.map((s, j) =>
            s.change === 'Added' ? (
              <ins key={j}>{s.text}</ins>
            ) : s.change === 'Removed' ? (
              <del key={j}>{s.text}</del>
            ) : (
              <span key={j}>{s.text}</span>
            ),
          )}
        </p>
      ))}
    </div>
  );
}

export interface VersionBarProps {
  version: VersionEntry;
  onRestore: () => void;
  onClose: () => void;
  now?: number;
}

/** Over the document while a version is shown in its place (Office's "previous version" bar). */
export function VersionBar({ version, onRestore, onClose, now = Date.now() }: VersionBarProps) {
  return (
    <div className="kiln-version-bar" role="status" data-testid="version-bar">
      <Icon icon={History} size={15} />
      <span>
        <strong>{version.name ?? when(version.time_ms, now)}</strong>
        {version.name ? ` (${when(version.time_ms, now)})` : ''}
        {t(', by {author}. Read-only.', { author: version.author })}
      </span>
      <Button size="sm" variant="primary" onClick={onRestore}>
        {t('Restore')}
      </Button>
      <Button size="sm" onClick={onClose} data-testid="version-bar-close">
        {t('Back to the document')}
      </Button>
    </div>
  );
}
