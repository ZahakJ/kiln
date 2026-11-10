/**
 * The status bar of each app: every figure is the `StatusBar` the core sends
 * with `Event::Changed`; the view buttons and the zoom run the core's `suite.view.mode` and
 * `suite.view.zoom`.
 */

import type { CoreDocState } from '@kiln/commands';
import { formatNumber, msg, t } from '@kiln/i18n';
import type { ViewMode } from '@kiln/protocol';
import { StatusBar, type StatusBarItem, type StatusBarView } from '@kiln/ui-kit';
import type { EditorApp } from './AppFrameRoute.tsx';

const VIEWS: Readonly<Record<EditorApp, readonly StatusBarView[]>> = {
  write: [
    { id: 'ReadMode', label: msg('Read Mode'), icon: 'ReadMode' },
    { id: 'PrintLayout', label: msg('Print Layout'), icon: 'PrintLayout' },
    { id: 'WebLayout', label: msg('Web Layout'), icon: 'WebLayout' },
  ],
  grid: [
    { id: 'Normal', label: msg('Normal'), icon: 'NormalView' },
    { id: 'PageLayout', label: msg('Page Layout'), icon: 'PageLayoutView' },
    { id: 'PageBreakPreview', label: msg('Page Break Preview'), icon: 'PageBreakPreview' },
  ],
  deck: [
    { id: 'SlideNormal', label: msg('Normal'), icon: 'NormalView' },
    { id: 'SlideSorter', label: msg('Slide Sorter'), icon: 'SlideSorter' },
    { id: 'ReadingView', label: msg('Reading View'), icon: 'ReadingView' },
  ],
  notes: [],
};

const fmt = (n: number) => formatNumber(n, { maximumFractionDigits: 2 });

function items(app: EditorApp, core: CoreDocState): StatusBarItem[] {
  const s = core.status;
  if (!core.summary) return [{ id: 'state', label: t('State'), content: t('No document') }];
  const out: StatusBarItem[] = [];
  if (app === 'grid') out.push({ id: 'mode', label: t('Cell mode'), content: t('Ready') });
  if (s?.page)
    out.push({
      id: 'page',
      label: t('Page number'),
      content: t('Page {current, number} of {total, number}', {
        current: s.page.current,
        total: s.page.total,
      }),
    });
  if (s?.slide)
    out.push({
      id: 'slide',
      label: t('Slide number'),
      content: t('Slide {current, number} of {total, number}', {
        current: s.slide.current,
        total: s.slide.total,
      }),
    });
  if (s?.words !== null && s?.words !== undefined)
    out.push({
      id: 'words',
      label: t('Word count'),
      content: t('status.words', { count: s.words }),
    });
  if (s?.language) out.push({ id: 'language', label: t('Language'), content: s.language });
  if (core.summary.read_only)
    out.push({ id: 'readonly', label: t('Read-only'), content: t('Read-only'), announce: true });
  return out;
}

function cellItems(core: CoreDocState): StatusBarItem[] {
  const c = core.status?.cells;
  if (!c || c.count < 2) return [];
  const out: StatusBarItem[] = [];
  if (c.average !== null)
    out.push({
      id: 'average',
      label: t('Average'),
      content: t('Average {value}', { value: fmt(c.average) }),
    });
  out.push({ id: 'count', label: t('Count'), content: t('Count {value}', { value: fmt(c.count) }) });
  if (c.numeric_count > 0)
    out.push({ id: 'sum', label: t('Sum'), content: t('Sum {value}', { value: fmt(c.sum) }) });
  if (c.min !== null)
    out.push({
      id: 'min',
      label: t('Minimum'),
      content: t('Min {value}', { value: fmt(c.min) }),
      defaultHidden: true,
    });
  if (c.max !== null)
    out.push({
      id: 'max',
      label: t('Maximum'),
      content: t('Max {value}', { value: fmt(c.max) }),
      defaultHidden: true,
    });
  return out;
}

export function FrameStatusBar({
  app,
  core,
  execute,
}: {
  app: EditorApp;
  core: CoreDocState;
  execute: (id: string, args?: unknown) => void;
}) {
  const views = VIEWS[app];
  const view = core.status?.view;
  const hasDoc = !!core.summary;
  return (
    <StatusBar
      left={items(app, core)}
      right={cellItems(core)}
      storageKey={`kiln.statusbar.${app}`}
      views={
        hasDoc && views.length > 0
          ? {
              value: views.some((v) => v.id === view) ? (view as string) : (views[0]?.id ?? ''),
              options: views,
              onChange: (mode) => execute('suite.view.mode', { mode: mode as ViewMode }),
            }
          : undefined
      }
      zoom={
        hasDoc
          ? {
              value: Math.round((core.status?.zoom ?? 1) * 100),
              onChange: (pct) => execute('suite.view.zoom', { zoom: pct / 100 }),
            }
          : undefined
      }
    />
  );
}
