/**
 * Write's status bar: Page x of y (Go To), word count (Word Count dialog),
 * language, the word-count goal, proofing and Track Changes, Focus, the view buttons and
 * the zoom slider with Page Width / One Page / Multiple Pages. Every figure is the core's.
 */
import { languageName, msg, t } from '@kiln/i18n';
import type { StatusBar as CoreStatus, ViewMode } from '@kiln/protocol';
import { StatusBar, type StatusBarItem, type StatusBarView } from '@kiln/ui-kit';
import { useMemo } from 'react';
import { featureStatusItems } from './features/status.tsx';
import { useWrite } from './hooks.ts';
import type { WriteEditorModel } from './model.ts';

const VIEWS: readonly StatusBarView[] = [
  { id: 'ReadMode', label: msg('Read Mode'), icon: 'ReadMode' },
  { id: 'PrintLayout', label: msg('Print Layout'), icon: 'PrintLayout' },
  { id: 'WebLayout', label: msg('Web Layout'), icon: 'WebLayout' },
];

export function writeStatusItems(
  status: CoreStatus | null,
  readOnly: boolean,
  open: (dialog: 'find' | 'wordCount', tab?: string) => void,
): StatusBarItem[] {
  const out: StatusBarItem[] = [];
  if (status?.page)
    out.push({
      id: 'page',
      label: t('Page number'),
      content: t('Page {current, number} of {total, number}', {
        current: status.page.current,
        total: status.page.total,
      }),
      title: t('Go To (Ctrl+G)'),
      onClick: () => open('find', 'goto'),
    });
  if (status?.words !== null && status?.words !== undefined)
    out.push({
      id: 'words',
      label: t('Word count'),
      content: t('status.words', { count: status.words }),
      title: t('Word Count'),
      onClick: () => open('wordCount'),
    });
  // Always listed (hidden by default, as in Word), so the status bar's customisation menu
  // offers it and its default applies from the first render.
  out.push({
    id: 'chars',
    label: t('Character count'),
    content:
      status?.chars !== null && status?.chars !== undefined
        ? t('status.characters', { count: status.chars })
        : '',
    defaultHidden: true,
  });
  out.push({ id: 'language', label: t('Language'), content: languageName(status?.language ?? 'en-US') });
  if (readOnly) out.push({ id: 'readonly', label: t('Read-only'), content: t('Read-only') });
  return out;
}

export function WriteStatusBar({
  model,
  readOnly,
  onFit,
}: {
  model: WriteEditorModel;
  readOnly: boolean;
  onFit: (kind: 'page_width' | 'one_page' | 'multiple_pages') => void;
}) {
  // Re-render only for what the bar shows by default (the character count changes with every
  // key and is hidden unless the user turns it on; it follows with the next shown change).
  const key = useWrite(model, (s) => {
    const st = s.status;
    return st ? `${st.page?.current}/${st.page?.total}|${st.words}|${st.language}|${st.view}|${st.zoom}` : '';
  });
  const status = useMemo(() => model.state.status, [model, key]);
  // goal, proofing, Track Changes and Focus follow the document's feature state.
  const features = useWrite(model, (s) => s.format?.features ?? null);
  const open = (d: 'find' | 'wordCount', tab?: string) => model.openDialog(d, tab);
  const view = status?.view ?? 'PrintLayout';
  const extra = featureStatusItems(model, features, status?.words ?? null);
  return (
    <StatusBar
      left={[...writeStatusItems(status, readOnly, open), ...extra.left]}
      right={extra.right}
      storageKey="kiln.statusbar.write"
      views={{
        value: VIEWS.some((v) => v.id === view) ? view : 'PrintLayout',
        options: VIEWS,
        onChange: (mode) => void model.run('suite.view.mode', { mode: mode as ViewMode }),
      }}
      zoom={{
        value: Math.round((status?.zoom ?? 1) * 100),
        onChange: (pct) => void model.run('suite.view.zoom', { zoom: pct / 100 }),
        fits: [
          { id: 'page_width', label: t('Page width'), onSelect: () => onFit('page_width') },
          { id: 'one_page', label: t('Whole page'), onSelect: () => onFit('one_page') },
          { id: 'multiple_pages', label: t('Multiple pages'), onSelect: () => onFit('multiple_pages') },
        ],
      }}
    />
  );
}
