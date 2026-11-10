/**
 * Kiln Write's editing area: the document surface with Word's rulers, the
 * Navigation pane on the left, the Styles pane on the right, the mini toolbar over a mouse
 * selection, the context menu and the dialogs. Every edit is a core command; the clipboard, Tab,
 * Shift+Enter and Esc in a header are routed to the editor's commands before the surface's own
 * text input sees them (Esc is the window's `ui.edit.cancel`, `commands.ts`).
 */

import { msg, t } from '@kiln/i18n';
import type { SpellingHit } from '@kiln/protocol';
import { Surface, type SurfaceController } from '@kiln/surface';
import type { RibbonItem, RibbonLeafItem, RibbonSpec } from '@kiln/ui-kit';
import {
  type AnchorRect,
  ContextMenu,
  errorText,
  Icon,
  type MenuEntry,
  MiniToolbar,
  RibbonItemView,
  useFormFactor,
} from '@kiln/ui-kit';
import { X } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useStore } from 'zustand';
import { FeatureDialogs } from './features/dialogs.tsx';
import { WriteFeatureLayer } from './features/Layer.tsx';
import { PASTE_MARKDOWN_ENTRY, spellingEntries } from './features/menu.ts';
import { featuresOf } from './features/model.ts';
import './features/features.css';
import { useMediaQuery, useWrite } from './hooks.ts';
import type { WriteEditorModel, WriteUiState } from './model.ts';
import { ReadMode } from './office/ReadMode.tsx';
import { SplitPane } from './office/SplitPane.tsx';
import { FeaturePanes } from './panes/FeaturePanes.tsx';
import { NavigationPane } from './panes/NavigationPane.tsx';
import { StylesPane } from './panes/StylesPane.tsx';
import { HorizontalRuler, RULER_SIZE, VerticalRuler } from './Rulers.tsx';
import { WriteDialogs } from './WriteDialogs.tsx';
import './write.css';

export interface WriteEditorProps {
  model: WriteEditorModel;
  /** The document's title (the surface's accessible name). */
  title: string;
  /** The page colours follow the chrome's resolved theme. */
  theme: 'light' | 'dark';
  /** Write's ribbon, whose Home ▸ Font controls the mini toolbar reuses. */
  ribbon?: RibbonSpec;
}

/** Command ids of the mini toolbar (Word's: font, size, grow, shrink, B, I, U, highlight, colour, lists). */
const MINI_COMMANDS = [
  'suite.format.font_family',
  'suite.format.font_size',
  'suite.format.grow_font',
  'suite.format.shrink_font',
  'suite.format.bold',
  'suite.format.italic',
  'suite.format.underline',
  'suite.format.highlight',
  'suite.format.font_color',
  'suite.format.bullets',
  'suite.format.numbering',
] as const;

function flatten(items: readonly RibbonItem[]): RibbonLeafItem[] {
  return items.flatMap((i) => (i.kind === 'row' ? flatten(i.items) : [i]));
}

/** The mini toolbar's controls: the ribbon's own items for those commands, in Word's order. */
export function miniToolbarItems(ribbon: RibbonSpec | undefined): RibbonLeafItem[] {
  const home = ribbon?.tabs.find((t) => t.id === 'home');
  if (!home) return [];
  const all = home.groups.flatMap((g) => flatten(g.items));
  const out: RibbonLeafItem[] = [];
  for (const id of MINI_COMMANDS) {
    const item = all.find((i) => 'command' in i && i.command === id);
    if (item) out.push(item.kind === 'combobox' ? { ...item, width: id.endsWith('size') ? 52 : 116 } : item);
  }
  return out;
}

const PASTE_OPTIONS: MenuEntry[] = [
  { command: 'suite.edit.paste_keep_source', label: msg('Keep Source Formatting') },
  { command: 'suite.edit.paste_merge_formatting', label: msg('Merge Formatting') },
  { command: 'suite.edit.paste_text_only', label: msg('Keep Text Only') },
];

/**
 * The context menu: spelling suggestions over a misspelled word, clipboard, paste options,
 * Font…, Paragraph…, Link…, and the table items in a table.
 */
export function contextMenuEntries(
  inTable: boolean,
  link: string | null,
  inList = false,
  spelling: SpellingHit | null = null,
): MenuEntry[] {
  const out: MenuEntry[] = [
    ...spellingEntries(spelling),
    { command: 'suite.edit.cut' },
    { command: 'suite.edit.copy' },
    { kind: 'heading', label: t('Paste Options') },
    ...PASTE_OPTIONS,
    PASTE_MARKDOWN_ENTRY,
    { kind: 'separator' },
    { command: 'suite.format.font_dialog', label: t('Font…') },
    { command: 'suite.format.paragraph_dialog', label: t('Paragraph…') },
  ];
  if (inList) {
    out.push(
      { kind: 'separator' },
      { command: 'write.list.restart', label: t('Restart at 1') },
      { command: 'write.list.continue', label: t('Continue Numbering') },
    );
  }
  if (inTable) {
    out.push(
      { kind: 'separator' },
      {
        kind: 'submenu',
        label: t('Insert'),
        icon: 'InsertTable',
        items: [
          { command: 'suite.table.insert_row_above', label: t('Insert Rows Above') },
          { command: 'suite.table.insert_row_below', label: t('Insert Rows Below') },
          { command: 'suite.table.insert_column_left', label: t('Insert Columns to the Left') },
          { command: 'suite.table.insert_column_right', label: t('Insert Columns to the Right') },
        ],
      },
      {
        kind: 'submenu',
        label: t('Delete'),
        icon: 'Trash2',
        items: [
          { command: 'suite.table.delete', label: t('Delete Rows'), args: { what: 'Rows' } },
          { command: 'suite.table.delete', label: t('Delete Columns'), args: { what: 'Columns' } },
          { command: 'suite.table.delete', label: t('Delete Table'), args: { what: 'Table' } },
        ],
      },
      { command: 'suite.table.merge_cells' },
      { command: 'suite.table.split_cells', label: t('Split Cells…') },
    );
  }
  out.push(
    { kind: 'separator' },
    // On a link: Word's Open Hyperlink (Ctrl+click from the pointer), then Remove.
    ...(link
      ? [
          { command: 'write.insert.follow_link', label: t('Open Link') },
          { command: 'write.insert.remove_link', label: t('Remove Link') },
        ]
      : [{ command: 'suite.insert.link', label: t('Link…') }]),
    { command: 'suite.review.new_comment', label: t('New Comment') },
  );
  return out;
}

/** Whether `selection` is no text selection or a collapsed one (no mini toolbar). */
function selectionEmpty(selection: WriteUiState['selection'] | undefined): boolean {
  const kind = selection?.kind;
  const text = typeof kind === 'object' && kind !== null && 'Text' in kind ? kind.Text : null;
  return !text || (text.anchor.block === text.head.block && text.anchor.offset === text.head.offset);
}

/** The union of the selection rectangles in viewport coordinates (mini toolbar anchor). */
function selectionAnchor(model: WriteEditorModel, controller: SurfaceController | null): AnchorRect | null {
  const sel = model.state.selection;
  if (!controller || !sel || sel.rects.length === 0) return null;
  let l = Number.POSITIVE_INFINITY;
  let t = Number.POSITIVE_INFINITY;
  let r = Number.NEGATIVE_INFINITY;
  let b = Number.NEGATIVE_INFINITY;
  // The first line of the selection: Word places the toolbar above where the selection starts.
  const first = sel.rects[0];
  for (const rect of sel.rects) {
    if (first && Math.abs(rect.y - first.y) > 1) continue;
    l = Math.min(l, rect.x);
    t = Math.min(t, rect.y);
    r = Math.max(r, rect.x + rect.width);
    b = Math.max(b, rect.y + rect.height);
  }
  if (!Number.isFinite(l)) return null;
  return controller.clientRectOf({ x: l, y: t, width: r - l, height: b - t });
}

/** The size of an element, kept current (the rulers' lengths). */
function useSize(ref: React.RefObject<HTMLElement | null>): { width: number; height: number } {
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => setSize({ width: el.clientWidth, height: el.clientHeight });
    update();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return size;
}

function Pane({
  side,
  title,
  onClose,
  children,
  testId,
}: {
  side: 'left' | 'right';
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  testId: string;
}) {
  return (
    <aside className={`kiln-write__pane kiln-write__pane--${side}`} aria-label={title} data-testid={testId}>
      <header className="kiln-write__pane-head">
        <h2>{title}</h2>
        <button
          type="button"
          className="kiln-write__pane-close"
          aria-label={t('Close {title}', { title })}
          onClick={onClose}
        >
          <Icon icon={X} size={14} />
        </button>
      </header>
      <div className="kiln-write__pane-body">{children}</div>
    </aside>
  );
}

export function WriteEditor({ model, title, theme, ribbon }: WriteEditorProps) {
  const showRuler = useWrite(model, (s) => s.showRuler);
  const navOpen = useWrite(model, (s) => s.navOpen);
  const stylesOpen = useWrite(model, (s) => s.stylesOpen);
  const mini = useWrite(model, (s) => s.mini);
  const notice = useWrite(model, (s) => s.notice);
  const viewMode = useWrite(model, (s) => s.status?.view ?? 'PrintLayout');
  const phone = useFormFactor() === 'phone';
  const inTable = useWrite(model, (s) => !!s.format?.table);
  const inList = useWrite(model, (s) => !!s.format?.list);
  const link = useWrite(model, (s) => s.format?.link ?? null);
  // the misspelled word under the pointer heads the context menu.
  const spellMenu = useStore(featuresOf(model).store, (s) => s.spellMenu);
  const [controller, setController] = useState<SurfaceController | null>(null);
  const area = useRef<HTMLDivElement>(null);
  const size = useSize(area);
  const wantMini = useRef(false);
  /** The selection when the mouse went down: a drag whose last selection is in already shows at once. */
  const downSelection = useRef('null');
  const miniItems = useMemo(() => miniToolbarItems(ribbon), [ribbon]);
  // The open task panes, which the mini toolbar keeps clear of.
  const paneRects = useCallback(
    () =>
      typeof document === 'undefined'
        ? []
        : Array.from(document.querySelectorAll('.kiln-write__pane')).map((el) => el.getBoundingClientRect()),
    [],
  );
  // Phones (Word mobile has no rulers either): the page needs the width more than the rulers.
  const compact = useMediaQuery('(max-width: 599px)');
  const rulers = showRuler && viewMode === 'PrintLayout' && !compact;

  const onController = useCallback(
    (c: SurfaceController | null) => {
      model.controller = c;
      setController(c);
    },
    [model],
  );

  // After a mouse selection, show the mini toolbar once the core reports a non-empty selection.
  useEffect(
    () =>
      model.store.subscribe((s, prev) => {
        if (!wantMini.current || s.selection === prev.selection) return;
        wantMini.current = false;
        model.set({ mini: selectionEmpty(s.selection) ? null : selectionAnchor(model, model.controller) });
      }),
    [model],
  );

  // Clipboard, Tab, Shift+Enter and Esc go to the editor's commands, ahead of the surface.
  useEffect(() => {
    const el = area.current;
    if (!el) return;
    const inSurface = (t: EventTarget | null) =>
      t instanceof Node && !!el.querySelector('[data-testid="kiln-input-proxy"]')?.contains(t);
    const onPaste = (e: ClipboardEvent) => {
      if (!inSurface(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      const text = e.clipboardData?.getData('text/plain') ?? '';
      const html = e.clipboardData?.getData('text/html') || null;
      if (!text && !html) {
        // A picture alone on the clipboard (a screenshot, an image copied from a browser) is
        // pasted as an inline picture, as in Word.
        const file = Array.from(e.clipboardData?.files ?? []).find((f) => f.type.startsWith('image/'));
        if (file) {
          void file.arrayBuffer().then((b) =>
            model.runCore('write.insert.picture', {
              name: file.name || `Pasted picture.${file.type.split('/')[1] ?? 'png'}`,
              bytes: Array.from(new Uint8Array(b)),
              floating: false,
            }),
          );
        }
        return;
      }
      void model.runCore('suite.edit.paste', { data: { text, html, kiln: null } });
    };
    const onCopy = (e: ClipboardEvent, cut: boolean) => {
      if (!inSurface(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      void model.runCore(cut ? 'suite.edit.cut' : 'suite.edit.copy');
    };
    const copy = (e: ClipboardEvent) => onCopy(e, false);
    const cut = (e: ClipboardEvent) => onCopy(e, true);
    const onKey = (e: KeyboardEvent) => {
      if (!inSurface(e.target) || e.isComposing || e.defaultPrevented) return;
      const plain = !e.ctrlKey && !e.metaKey && !e.altKey;
      const primary = (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey;
      if (primary && e.isTrusted && ['c', 'x', 'v'].includes(e.key.toLowerCase())) {
        // Cut, Copy and Paste in the document go through the browser's clipboard events (above):
        // in order with the keys before them, whatever the selection the window last heard of,
        // and Paste reads the clipboard the event carries, with no permission prompt. (A
        // synthetic key event has no clipboard action, so the keymap takes it.)
        e.stopPropagation();
        return;
      }
      let id: string | null = null;
      let args: unknown;
      if (e.key === 'Tab' && plain) {
        id = 'write.edit.tab';
        args = { back: e.shiftKey };
      } else if (e.key === 'Enter' && plain && e.shiftKey) id = 'write.edit.line_break';
      // Alt+X: the hexadecimal code before the caret becomes its character, and back (Word).
      else if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && e.code === 'KeyX')
        id = 'write.edit.toggle_unicode';
      if (!id) return;
      e.preventDefault();
      e.stopPropagation();
      void model.run(id, args);
    };
    el.addEventListener('paste', onPaste, true);
    el.addEventListener('copy', copy, true);
    el.addEventListener('cut', cut, true);
    el.addEventListener('keydown', onKey, true);
    return () => {
      el.removeEventListener('paste', onPaste, true);
      el.removeEventListener('copy', copy, true);
      el.removeEventListener('cut', cut, true);
      el.removeEventListener('keydown', onKey, true);
    };
  }, [model]);

  const dismissMini = useCallback(() => model.set({ mini: null }), [model]);
  // After a ribbon, menu or status bar command the next keystroke types in the document (Office).
  useEffect(() => {
    if (!controller) return;
    const back = () => controller.focus();
    window.addEventListener('kiln:return-to-document', back);
    return () => window.removeEventListener('kiln:return-to-document', back);
  }, [controller]);
  const split = useWrite(model, (s) => s.split);

  return (
    <div className="kiln-write" data-view={viewMode} data-testid="write-editor">
      {navOpen ? (
        <Pane
          side="left"
          title={t('Navigation')}
          onClose={() => model.set({ navOpen: false })}
          testId="write-nav-pane"
        >
          <NavigationPane model={model} />
        </Pane>
      ) : null}
      <div className={`kiln-write__doc${rulers ? ' kiln-write__doc--rulers' : ''}`}>
        {rulers ? (
          <>
            <div className="kiln-write__corner" aria-hidden="true" />
            <HorizontalRuler model={model} width={Math.max(0, size.width)} />
            <VerticalRuler model={model} height={Math.max(0, size.height)} />
          </>
        ) : null}
        <ContextMenu
          entries={contextMenuEntries(inTable, link, inList, spellMenu)}
          aria-label={t('Document')}
          miniToolbar={
            miniItems.length > 0
              ? miniItems.map((item) => (
                  <RibbonItemView
                    key={'command' in item ? item.command : item.kind}
                    item={item}
                    size="small"
                    layout="simplified"
                  />
                ))
              : undefined
          }
          onOpenChange={(open) => {
            if (open) dismissMini();
            else featuresOf(model).set({ spellMenu: null });
          }}
        >
          <div
            ref={area}
            className="kiln-write__surface"
            style={rulers ? { gridColumn: 2, gridRow: 2 } : undefined}
            onPointerDownCapture={(e) => {
              // In the capture phase: a press the surface keeps to itself (dragging selected text)
              // still closes the toolbar and is recorded.
              if (e.button === 0) dismissMini();
              downSelection.current = JSON.stringify(model.store.getState().selection ?? null);
            }}
            onPointerUp={(e) => {
              // Not after a click on a comment card or marker, the object frame or a drawing:
              // those select for their own purpose (a marker opens the Comments pane).
              const own = (e.target as HTMLElement).closest?.(
                '.kiln-comment, .kiln-comment-marker, .kiln-object-frame, .kiln-draw-overlay',
              );
              if (e.pointerType !== 'mouse' || e.button !== 0 || own) return;
              // The drag's last selection may have come in before the button went up: no later
              // change would show the toolbar then.
              const now = model.store.getState().selection;
              if (JSON.stringify(now ?? null) !== downSelection.current && !selectionEmpty(now)) {
                model.set({ mini: selectionAnchor(model, model.controller) });
              } else {
                wantMini.current = true;
              }
            }}
          >
            <div className="kiln-write__pane-main" data-split={split ? '' : undefined}>
              <Surface
                client={model.client}
                doc={model.doc}
                label={title}
                theme={theme}
                // on a phone Web Layout's one long page fills the screen.
                canvasFill={phone && viewMode === 'WebLayout' ? 'page' : 'canvas'}
                zoomKeys={false}
                onController={onController}
                onViewChange={(v) => model.set({ view: v })}
                onError={(err) => model.set({ notice: errorText(err) })}
              />
              {/* comment balloons, the object frame, focus mode, Format Painter. */}
              <WriteFeatureLayer model={model} controller={controller} />
              {/* Read Mode's page turning and zoomed pictures. */}
              {controller ? <ReadMode model={model} controller={controller} /> : null}
            </div>
            {/* View ▸ Split. */}
            {split && controller ? <SplitPane model={model} controller={controller} /> : null}
          </div>
        </ContextMenu>
        {notice ? (
          <div className="kiln-write__notice" role="status">
            <span>{notice}</span>
            <button type="button" aria-label={t('Dismiss')} onClick={() => model.set({ notice: null })}>
              <Icon icon={X} size={12} />
            </button>
          </div>
        ) : null}
      </div>
      {stylesOpen ? (
        <Pane
          side="right"
          title={t('Styles')}
          onClose={() => model.set({ stylesOpen: false })}
          testId="write-styles"
        >
          <StylesPane model={model} />
        </Pane>
      ) : null}
      {/* the Comments, Reviewing, Selection and Spelling panes. */}
      <FeaturePanes model={model} />
      <MiniToolbar
        anchor={controller ? mini : null}
        onDismiss={dismissMini}
        avoid={paneRects}
        aria-label={t('Formatting')}
      >
        {miniItems.map((item) => (
          <RibbonItemView
            key={'command' in item ? item.command : item.kind}
            item={item}
            size="small"
            layout="simplified"
          />
        ))}
      </MiniToolbar>
      <WriteDialogs model={model} />
      <FeatureDialogs model={model} />
    </div>
  );
}

export { RULER_SIZE };
