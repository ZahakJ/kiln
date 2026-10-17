/**
 * The status bar of each app (PLAN.md §1.3, §1.4): every figure is the `StatusBar` the core sends
 * with `Event::Changed`; the view buttons and the zoom run the core's `suite.view.mode` and
 * `suite.view.zoom`.
 */
import type { CoreDocState } from '@kiln/commands';
import type { ViewMode } from '@kiln/protocol';
import { StatusBar, type StatusBarItem, type StatusBarView } from '@kiln/ui-kit';
import type { EditorApp } from './AppFrameRoute.tsx';

const VIEWS: Readonly<Record<EditorApp, readonly StatusBarView[]>> = {
  write: [
    { id: 'ReadMode', label: 'Read Mode', icon: 'ReadMode' },
    { id: 'PrintLayout', label: 'Print Layout', icon: 'PrintLayout' },
    { id: 'WebLayout', label: 'Web Layout', icon: 'WebLayout' },
  ],
  grid: [
    { id: 'Normal', label: 'Normal', icon: 'NormalView' },
    { id: 'PageLayout', label: 'Page Layout', icon: 'PageLayoutView' },
    { id: 'PageBreakPreview', label: 'Page Break Preview', icon: 'PageBreakPreview' },
  ],
  deck: [
    { id: 'SlideNormal', label: 'Normal', icon: 'NormalView' },
    { id: 'SlideSorter', label: 'Slide Sorter', icon: 'SlideSorter' },
    { id: 'ReadingView', label: 'Reading View', icon: 'ReadingView' },
  ],
  notes: [],
};

const fmt = (n: number) => n.toLocaleString(undefined, { maximumFractionDigits: 2 });

function items(app: EditorApp, core: CoreDocState): StatusBarItem[] {
  const s = core.status;
  if (!core.summary) return [{ id: 'state', label: 'State', content: 'No document' }];
  const out: StatusBarItem[] = [];
  if (app === 'grid') out.push({ id: 'mode', label: 'Cell mode', content: 'Ready' });
  if (s?.page)
    out.push({
      id: 'page',
      label: 'Page number',
      content: `Page ${fmt(s.page.current)} of ${fmt(s.page.total)}`,
    });
  if (s?.slide)
    out.push({
      id: 'slide',
      label: 'Slide number',
      content: `Slide ${fmt(s.slide.current)} of ${fmt(s.slide.total)}`,
    });
  if (s?.words !== null && s?.words !== undefined)
    out.push({
      id: 'words',
      label: 'Word count',
      content: `${fmt(s.words)} ${s.words === 1 ? 'word' : 'words'}`,
    });
  if (s?.language) out.push({ id: 'language', label: 'Language', content: s.language });
  if (core.summary.read_only) out.push({ id: 'readonly', label: 'Read-only', content: 'Read-only' });
  return out;
}

function cellItems(core: CoreDocState): StatusBarItem[] {
  const c = core.status?.cells;
  if (!c || c.count < 2) return [];
  const out: StatusBarItem[] = [];
  if (c.average !== null) out.push({ id: 'average', label: 'Average', content: `Average ${fmt(c.average)}` });
  out.push({ id: 'count', label: 'Count', content: `Count ${fmt(c.count)}` });
  if (c.numeric_count > 0) out.push({ id: 'sum', label: 'Sum', content: `Sum ${fmt(c.sum)}` });
  if (c.min !== null)
    out.push({ id: 'min', label: 'Minimum', content: `Min ${fmt(c.min)}`, defaultHidden: true });
  if (c.max !== null)
    out.push({ id: 'max', label: 'Maximum', content: `Max ${fmt(c.max)}`, defaultHidden: true });
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
