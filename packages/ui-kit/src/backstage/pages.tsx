/**
 * Content for the Backstage pages (PLAN.md §1.5). Every figure comes from the core through the
 * props (`client.query(doc, Summary | Status | CompatReport)`, the host's recents); actions are
 * callbacks the frame turns into commands (`suite.file.new`, `suite.file.open`,
 * `suite.file.save_as`). Nothing here reads or computes document content.
 */
import type { AppId } from '@kiln/commands';
import type { CompatIssue, DocSummary, FileFormat, RecentFile, StatusBar } from '@kiln/protocol';
import { FolderOpen, History, PencilLine, RefreshCw, ShieldAlert, ShieldCheck } from 'lucide-react';
import type { ReactNode } from 'react';
import { AppMark } from '../chrome/AppMark.tsx';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { cx } from '../primitives/cx.ts';

type EditorApp = Exclude<AppId, 'hub'>;

const BLANK: Readonly<Record<EditorApp, { label: string; noun: string }>> = {
  write: { label: 'Blank document', noun: 'Kiln Write' },
  grid: { label: 'Blank workbook', noun: 'Kiln Grid' },
  deck: { label: 'Blank presentation', noun: 'Kiln Deck' },
  notes: { label: 'New notebook', noun: 'Kiln Notes' },
};

/** File ▸ New: blank documents of every app (templates join with WP-85). */
export function BackstageNewPage({
  app,
  creatable,
  onCreate,
}: {
  app: EditorApp;
  creatable: readonly EditorApp[];
  onCreate: (app: EditorApp) => void;
}) {
  const order: EditorApp[] = [app, ...(['write', 'grid', 'deck', 'notes'] as const).filter((a) => a !== app)];
  return (
    <ul className="kiln-bs-tiles">
      {order.map((a) => {
        const ok = creatable.includes(a);
        return (
          <li key={a}>
            <button
              type="button"
              className="kiln-bs-tile"
              data-app={a}
              disabled={!ok}
              onClick={() => onCreate(a)}
            >
              <span className="kiln-bs-tile__page">
                <AppMark app={a} size={48} />
              </span>
              <span className="kiln-bs-tile__label">{BLANK[a].label}</span>
              <span className="kiln-bs-tile__sub">{ok ? BLANK[a].noun : 'Not in this build yet'}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

const KIND_APP: Readonly<Record<string, EditorApp>> = { Grid: 'grid', Deck: 'deck', Notes: 'notes' };

/** File ▸ Open (and Home): recent files from the core's recents, and Browse. */
export function BackstageOpenPage({
  recent,
  onOpenRecent,
  onBrowse,
}: {
  recent: readonly RecentFile[] | null;
  onOpenRecent: (file: RecentFile) => void;
  onBrowse: () => void;
}) {
  return (
    <div className="kiln-bs-open">
      <Button variant="primary" icon={<Icon icon={FolderOpen} size={16} />} onClick={onBrowse}>
        Browse this device…
      </Button>
      <h2 className="kiln-bs-h2">Recent</h2>
      {recent === null ? (
        <p className="kiln-bs-muted" role="status">
          Looking for your files…
        </p>
      ) : recent.length === 0 ? (
        <p className="kiln-bs-muted">Files you open or save appear here.</p>
      ) : (
        <ul className="kiln-bs-files">
          {recent.map((f) => (
            <li key={f.path}>
              <button type="button" className="kiln-bs-file" onClick={() => onOpenRecent(f)}>
                <AppMark app={KIND_APP[f.kind] ?? 'write'} size={24} />
                <span>
                  <b>{f.name}</b>
                  <small>{f.path}</small>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="kiln-bs-fact">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/** File ▸ Info: properties and the compatibility check, as the core reports them. */
export function BackstageInfoPage({
  summary,
  status,
  compat,
  compatError,
  onCheckCompat,
  onVersionHistory,
  onEditProperties,
}: {
  summary: DocSummary | null;
  status: StatusBar | null;
  /** `null` until checked. */
  compat: readonly CompatIssue[] | null;
  /** Why the last check failed. */
  compatError?: string | null;
  onCheckCompat?: () => void;
  onVersionHistory?: () => void;
  /** Opens the app's Properties dialog (title, subject, author…), when it has one. */
  onEditProperties?: () => void;
}) {
  if (!summary) {
    return <p className="kiln-bs-muted">No document is open in this window.</p>;
  }
  const n = (v: number | null | undefined) => (v === null || v === undefined ? '—' : v.toLocaleString());
  return (
    <div className="kiln-bs-info">
      <section className="kiln-bs-card" aria-labelledby="kiln-bs-compat">
        <h2 id="kiln-bs-compat" className="kiln-bs-h2">
          <Icon icon={compat && compat.length > 0 ? ShieldAlert : ShieldCheck} size={18} />
          Check before you share
        </h2>
        <p className="kiln-bs-muted">
          Lists anything in this document that will not survive saving in another format.
        </p>
        {compatError ? (
          <p className="kiln-bs-muted" role="alert">
            {compatError}
          </p>
        ) : null}
        {compat === null ? null : compat.length === 0 ? (
          <p className="kiln-bs-ok">No issues found.</p>
        ) : (
          <ul className="kiln-bs-issues">
            {compat.map((c, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: the core's issue list has no ids and is replaced as a whole
              <li key={i} data-severity={c.severity}>
                {c.message}
              </li>
            ))}
          </ul>
        )}
        {onCheckCompat ? (
          <Button icon={<Icon icon={RefreshCw} size={15} />} onClick={onCheckCompat}>
            Check compatibility
          </Button>
        ) : null}
      </section>
      <section className="kiln-bs-card" aria-labelledby="kiln-bs-props">
        <h2 id="kiln-bs-props" className="kiln-bs-h2">
          Properties
        </h2>
        <dl className="kiln-bs-facts">
          <Fact label="Title">{summary.title}</Fact>
          <Fact label="Location">{summary.path ?? 'Not saved yet'}</Fact>
          <Fact label="Format">{summary.format ?? '—'}</Fact>
          <Fact label="Changes">{summary.dirty ? 'Not saved' : 'All saved'}</Fact>
          {status?.page ? <Fact label="Pages">{n(status.page.total)}</Fact> : null}
          {status?.words !== null && status?.words !== undefined ? (
            <Fact label="Words">{n(status.words)}</Fact>
          ) : null}
          {status?.slide ? <Fact label="Slides">{n(status.slide.total)}</Fact> : null}
          <Fact label="Editing">{summary.read_only ? 'Read-only' : 'Editable'}</Fact>
        </dl>
        {onEditProperties ? (
          <Button icon={<Icon icon={PencilLine} size={15} />} onClick={onEditProperties}>
            Edit Properties
          </Button>
        ) : null}
        {onVersionHistory ? (
          <Button icon={<Icon icon={History} size={15} />} onClick={onVersionHistory}>
            Version History
          </Button>
        ) : null}
      </section>
    </div>
  );
}

export interface FormatChoice {
  format: FileFormat;
  label: string;
  /** "Kiln and Office", "Open standard", "Read-only copy". */
  note?: string;
}

/** File ▸ Save As / Save a Copy / Export: one button per format the app saves. */
export function BackstageFormatsPage({
  formats,
  intro,
  onChoose,
}: {
  formats: readonly FormatChoice[];
  intro?: string;
  onChoose: (format: FileFormat) => void;
}) {
  return (
    <div>
      {intro ? <p className="kiln-bs-muted">{intro}</p> : null}
      <ul className="kiln-bs-formats">
        {formats.map((f) => (
          <li key={f.format}>
            <button type="button" className="kiln-bs-format" onClick={() => onChoose(f.format)}>
              <span className="kiln-bs-format__ext">.{f.format.toLowerCase()}</span>
              <span>
                <b>{f.label}</b>
                {f.note ? <small>{f.note}</small> : null}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** File ▸ Account: theme and the build. */
export function BackstageAccountPage({
  theme,
  onTheme,
  version,
  core,
}: {
  theme: 'system' | 'light' | 'dark' | 'hc';
  onTheme: (theme: 'system' | 'light' | 'dark' | 'hc') => void;
  version: string;
  /** The core's own description (`suite.dev.ping`): version, protocol, target. */
  core?: string | null;
}) {
  const choices = [
    ['system', 'Use system setting'],
    ['light', 'Light'],
    ['dark', 'Dark'],
    ['hc', 'High contrast'],
  ] as const;
  return (
    <div className="kiln-bs-info">
      <section className="kiln-bs-card" aria-labelledby="kiln-bs-theme">
        <h2 id="kiln-bs-theme" className="kiln-bs-h2">
          Kiln theme
        </h2>
        <div className="kiln-bs-themes" role="radiogroup" aria-labelledby="kiln-bs-theme">
          {choices.map(([id, label]) => (
            // biome-ignore lint/a11y/useSemanticElements: styled radio cards, keyboard handled by the buttons
            <button
              key={id}
              type="button"
              role="radio"
              aria-checked={theme === id}
              className={cx('kiln-bs-theme', theme === id && 'is-on')}
              data-theme-choice={id}
              onClick={() => onTheme(id)}
            >
              <span className="kiln-bs-theme__swatch" data-swatch={id} aria-hidden="true" />
              {label}
            </button>
          ))}
        </div>
      </section>
      <section className="kiln-bs-card" aria-labelledby="kiln-bs-about">
        <h2 id="kiln-bs-about" className="kiln-bs-h2">
          About Kiln
        </h2>
        <p>Kiln {version}. Free software under the GNU AGPL 3.0 or later.</p>
        {core ? <p className="kiln-bs-muted">{core}</p> : null}
      </section>
    </div>
  );
}
