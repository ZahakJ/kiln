/**
 * Write's status bar (PLAN.md §1.3): Page x of y (Go To), word count (Word Count dialog),
 * language, the word-count goal, proofing and Track Changes (WP-66), Focus, the view buttons and
 * the zoom slider with Page Width / One Page / Multiple Pages. Every figure is the core's.
 */
import type { StatusBar as CoreStatus, ViewMode } from '@kiln/protocol';
import { StatusBar, type StatusBarItem, type StatusBarView } from '@kiln/ui-kit';
import { useMemo } from 'react';
import { featureStatusItems } from './features/status.tsx';
import { useWrite } from './hooks.ts';
import type { WriteEditorModel } from './model.ts';

const VIEWS: readonly StatusBarView[] = [
  { id: 'ReadMode', label: 'Read Mode', icon: 'ReadMode' },
  { id: 'PrintLayout', label: 'Print Layout', icon: 'PrintLayout' },
  { id: 'WebLayout', label: 'Web Layout', icon: 'WebLayout' },
];

const fmt = (n: number) => n.toLocaleString();

export function writeStatusItems(
  status: CoreStatus | null,
  readOnly: boolean,
  open: (dialog: 'find' | 'wordCount', tab?: string) => void,
): StatusBarItem[] {
  const out: StatusBarItem[] = [];
  if (status?.page)
    out.push({
      id: 'page',
      label: 'Page number',
      content: `Page ${fmt(status.page.current)} of ${fmt(status.page.total)}`,
      title: 'Go To (Ctrl+G)',
      onClick: () => open('find', 'goto'),
    });
  if (status?.words !== null && status?.words !== undefined)
    out.push({
      id: 'words',
      label: 'Word count',
      content: `${fmt(status.words)} ${status.words === 1 ? 'word' : 'words'}`,
      title: 'Word Count',
      onClick: () => open('wordCount'),
    });
  // Always listed (hidden by default, as in Word), so the status bar's customisation menu
  // offers it and its default applies from the first render.
  out.push({
    id: 'chars',
    label: 'Character count',
    content: status?.chars !== null && status?.chars !== undefined ? `${fmt(status.chars)} characters` : '',
    defaultHidden: true,
  });
  out.push({ id: 'language', label: 'Language', content: status?.language ?? 'English (United States)' });
  if (readOnly) out.push({ id: 'readonly', label: 'Read-only', content: 'Read-only' });
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
  // WP-66: goal, proofing, Track Changes and Focus follow the document's feature state.
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
          { id: 'page_width', label: 'Page width', onSelect: () => onFit('page_width') },
          { id: 'one_page', label: 'Whole page', onSelect: () => onFit('one_page') },
          { id: 'multiple_pages', label: 'Multiple pages', onSelect: () => onFit('multiple_pages') },
        ],
      }}
    />
  );
}
