/**
 * Content for the Backstage pages. Every figure comes from the core through the
 * props (`client.query(doc, Summary | Status | CompatReport)`, the host's recents); actions are
 * callbacks the frame turns into commands (`suite.file.new`, `suite.file.open`,
 * `suite.file.save_as`). Nothing here reads or computes document content.
 */

import { type AppId, RELEASE_APPS } from '@kiln/commands';
import { formatNumber, msg, t, tx } from '@kiln/i18n';
import type { DocPropertiesInfo, DocSummary, FileFormat, RecentFile, StatusBar } from '@kiln/protocol';
import { FolderOpen, History, PencilLine, ScanSearch, ShieldAlert, ShieldCheck } from 'lucide-react';
import type { ReactNode } from 'react';
import { AppMark } from '../chrome/AppMark.tsx';
import { limitRecents, useOfficeOptions } from '../chrome/options.ts';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { cx } from '../primitives/cx.ts';

type EditorApp = Exclude<AppId, 'hub'>;

const BLANK: Readonly<Record<EditorApp, { label: string; noun: string }>> = {
  write: { label: msg('Blank document'), noun: 'Kiln Write' },
  grid: { label: msg('Blank workbook'), noun: 'Kiln Grid' },
  deck: { label: msg('Blank presentation'), noun: 'Kiln Deck' },
  // i18n-ignore: Kiln Notes is not part of v0.1.
  notes: { label: 'New notebook', noun: 'Kiln Notes' },
};

/** File ▸ New without templates: a blank document of each app of the release. */
export function BackstageNewPage({
  app,
  creatable,
  onCreate,
}: {
  app: EditorApp;
  creatable: readonly EditorApp[];
  onCreate: (app: EditorApp) => void;
}) {
  const order: EditorApp[] = [
    ...RELEASE_APPS.filter((a) => a === app),
    ...RELEASE_APPS.filter((a) => a !== app),
  ];
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
              <span className="kiln-bs-tile__label">{tx(BLANK[a].label)}</span>
              <span className="kiln-bs-tile__sub">{ok ? BLANK[a].noun : t('Not in this build yet')}</span>
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
  // Options ▸ Advanced ▸ Show this number of Recent Documents.
  const [options] = useOfficeOptions();
  const shown = recent ? limitRecents(recent, options.advanced.recentCount) : null;
  return (
    <div className="kiln-bs-open">
      <Button variant="primary" icon={<Icon icon={FolderOpen} size={16} />} onClick={onBrowse}>
        {t('Browse this device…')}
      </Button>
      <h2 className="kiln-bs-h2">{t('Recent')}</h2>
      {recent === null ? (
        <p className="kiln-bs-muted" role="status">
          {t('Looking for your files…')}
        </p>
      ) : shown === null || shown.length === 0 ? (
        <p className="kiln-bs-muted">{t('Files you open or save appear here.')}</p>
      ) : (
        <ul className="kiln-bs-files">
          {shown.map((f) => (
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

/** One of File ▸ Info's actions: a large icon, its name and what it does (Office's Info rows). */
function InfoAction({
  icon,
  title,
  about,
  onClick,
  testId,
}: {
  icon: typeof History;
  title: string;
  about: string;
  onClick: () => void;
  testId: string;
}) {
  return (
    <button type="button" className="kiln-bs-action" onClick={onClick} data-testid={testId}>
      <span className="kiln-bs-action__icon" aria-hidden="true">
        <Icon icon={icon} size={22} />
      </span>
      <span className="kiln-bs-action__text">
        <b>{title}</b>
        <span>{about}</span>
      </span>
    </button>
  );
}

/**
 * File ▸ Info: Inspect Document, the Compatibility Checker and Version History on the left, the
 * document's properties (as the core reports them) on the right.
 */
export function BackstageInfoPage({
  summary,
  status,
  properties,
  onInspect,
  onCheckCompat,
  onVersionHistory,
  onEditProperties,
}: {
  summary: DocSummary | null;
  status: StatusBar | null;
  /** The document properties (`Query::Properties`); `null` while read or where there are none. */
  properties?: DocPropertiesInfo | null;
  /** Opens Inspect Document. */
  onInspect?: () => void;
  /** Opens the Compatibility Checker. */
  onCheckCompat?: () => void;
  onVersionHistory?: () => void;
  /** Opens the Properties dialog (title, subject, author…). */
  onEditProperties?: () => void;
}) {
  if (!summary) {
    return <p className="kiln-bs-muted">{t('No document is open in this window.')}</p>;
  }
  const n = (v: number | null | undefined) => (v === null || v === undefined ? '—' : formatNumber(v));
  const p = properties ?? null;
  const ext = summary.format ? `.${summary.format.toLowerCase()}` : null;
  return (
    <div className="kiln-bs-info">
      <section className="kiln-bs-card kiln-bs-card--actions" aria-labelledby="kiln-bs-check">
        <h2 id="kiln-bs-check" className="kiln-bs-h2">
          <Icon icon={ShieldCheck} size={18} />
          {t('Check before you share')}
        </h2>
        {onInspect ? (
          <InfoAction
            icon={ScanSearch}
            title={t('Inspect Document')}
            about={t(
              'Find comments, tracked changes, hidden sheets or slides, notes and personal information before you share.',
            )}
            onClick={onInspect}
            testId="info-inspect"
          />
        ) : null}
        {onCheckCompat ? (
          <InfoAction
            icon={ShieldAlert}
            title={t('Check compatibility')}
            about={t(
              'List anything that will not survive saving in an Office, OpenDocument or other format.',
            )}
            onClick={onCheckCompat}
            testId="info-compat"
          />
        ) : null}
        {onVersionHistory ? (
          <InfoAction
            icon={History}
            title={t('Version History')}
            about={t('See, compare and restore earlier versions of this document.')}
            onClick={onVersionHistory}
            testId="info-versions"
          />
        ) : null}
      </section>
      <section className="kiln-bs-card" aria-labelledby="kiln-bs-props">
        <h2 id="kiln-bs-props" className="kiln-bs-h2">
          {t('Properties')}
        </h2>
        <dl className="kiln-bs-facts">
          <Fact label={t('Title')}>
            {p?.title || <span className="kiln-bs-muted">{t('Add a title')}</span>}
          </Fact>
          {p?.keywords ? <Fact label={t('Tags')}>{p.keywords}</Fact> : null}
          {p?.author ? <Fact label={t('Author')}>{p.author}</Fact> : null}
          {p?.last_modified_by ? <Fact label={t('Last modified by')}>{p.last_modified_by}</Fact> : null}
          <Fact label={t('Location')}>{summary.path ?? t('Not saved yet')}</Fact>
          <Fact label={t('Format')}>{ext ?? '—'}</Fact>
          <Fact label={t('Changes')}>{summary.dirty ? t('Not saved') : t('All saved')}</Fact>
          {status?.page ? <Fact label={t('Pages')}>{n(status.page.total)}</Fact> : null}
          {status?.words !== null && status?.words !== undefined ? (
            <Fact label={t('Words')}>{n(status.words)}</Fact>
          ) : null}
          {p && p.sheets > 0 ? <Fact label={t('Worksheets')}>{n(p.sheets)}</Fact> : null}
          {status?.slide ? <Fact label={t('Slides')}>{n(status.slide.total)}</Fact> : null}
          <Fact label={t('Editing')}>{summary.read_only ? t('Read-only') : t('Editable')}</Fact>
        </dl>
        {onEditProperties ? (
          <Button icon={<Icon icon={PencilLine} size={15} />} onClick={onEditProperties}>
            {t('Edit Properties')}
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
            <button
              type="button"
              className="kiln-bs-format"
              data-format={f.format}
              onClick={() => onChoose(f.format)}
            >
              <span className="kiln-bs-format__ext">.{f.format.toLowerCase()}</span>
              <span>
                <b>{tx(f.label)}</b>
                {f.note ? <small>{tx(f.note)}</small> : null}
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
    ['system', t('Use system setting')],
    ['light', t('Light')],
    ['dark', t('Dark')],
    ['hc', t('High contrast')],
  ] as const;
  return (
    <div className="kiln-bs-info">
      <section className="kiln-bs-card" aria-labelledby="kiln-bs-theme">
        <h2 id="kiln-bs-theme" className="kiln-bs-h2">
          {t('Kiln theme')}
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
          {t('About Kiln')}
        </h2>
        <p>{t('Kiln {version}. Free software under the GNU AGPL 3.0 or later.', { version })}</p>
        {core ? <p className="kiln-bs-muted">{core}</p> : null}
      </section>
    </div>
  );
}
