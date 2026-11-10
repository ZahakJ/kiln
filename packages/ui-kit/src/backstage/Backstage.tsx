import { msg, t } from '@kiln/i18n';
import * as RD from '@radix-ui/react-dialog';
import {
  ArrowLeft,
  CircleUserRound,
  FileOutput,
  FilePlus,
  FolderOpen,
  House,
  Info,
  type LucideIcon,
  Printer,
  Save,
  SaveAll,
  Settings,
  Share2,
  X,
} from 'lucide-react';
import { type ReactNode, useEffect, useRef, useState } from 'react';
import { useOptionalCommands } from '../chrome/CommandContext.tsx';
import { Icon } from '../icons/Icon.tsx';
import { cx } from '../primitives/cx.ts';
import './backstage.css';

/** Backstage pages in Office order. `save`, `close` and `options` are actions. */
export type BackstagePageId =
  | 'home'
  | 'new'
  | 'open'
  | 'info'
  | 'saveAs'
  | 'saveCopy'
  | 'print'
  | 'share'
  | 'export'
  | 'account';

export type BackstageActionId = 'save' | 'close' | 'options';

type NavEntry =
  | { kind: 'page'; id: BackstagePageId; label: string; icon: LucideIcon }
  | { kind: 'action'; id: BackstageActionId; label: string; icon: LucideIcon; command: string }
  | { kind: 'divider' };

const TOP: NavEntry[] = [
  { kind: 'page', id: 'home', label: msg('Home'), icon: House },
  { kind: 'page', id: 'new', label: msg('New'), icon: FilePlus },
  { kind: 'page', id: 'open', label: msg('Open'), icon: FolderOpen },
  { kind: 'divider' },
  { kind: 'page', id: 'info', label: msg('Info'), icon: Info },
  { kind: 'action', id: 'save', label: msg('Save'), icon: Save, command: 'suite.file.save' },
  { kind: 'page', id: 'saveAs', label: msg('Save As'), icon: SaveAll },
  { kind: 'page', id: 'saveCopy', label: msg('Save a Copy'), icon: SaveAll },
  { kind: 'page', id: 'print', label: msg('Print'), icon: Printer },
  { kind: 'page', id: 'share', label: msg('Share'), icon: Share2 },
  { kind: 'page', id: 'export', label: msg('Export'), icon: FileOutput },
  { kind: 'action', id: 'close', label: msg('Close'), icon: X, command: 'suite.file.close' },
];
const BOTTOM: NavEntry[] = [
  { kind: 'page', id: 'account', label: msg('Account'), icon: CircleUserRound },
  { kind: 'action', id: 'options', label: msg('Options'), icon: Settings, command: 'ui.options.open' },
];

export const BACKSTAGE_PAGE_TITLES: Readonly<Record<BackstagePageId, string>> = {
  home: msg('Home'),
  new: msg('New'),
  open: msg('Open'),
  info: msg('Info'),
  saveAs: msg('Save As'),
  saveCopy: msg('Save a Copy'),
  print: msg('Print'),
  share: msg('Share'),
  export: msg('Export'),
  account: msg('Account'),
};

export interface BackstageProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Content of each page. Pages without content are not listed (no empty placeholders). */
  pages: Partial<Record<BackstagePageId, ReactNode>>;
  /** Controlled page. */
  page?: BackstagePageId;
  defaultPage?: BackstagePageId;
  onPageChange?: (page: BackstagePageId) => void;
  /**
   * Save / Close / Options. Default: run `suite.file.save` / `suite.file.close` / `ui.options.open` through the
   * command registry (when inside a `CommandsProvider`), after closing the Backstage.
   */
  onAction?: (action: BackstageActionId) => void;
  /** Which actions to list. Default: all three. */
  actions?: readonly BackstageActionId[];
  /** Office shows "Save a Copy" instead of "Save As" while AutoSave is on. */
  autoSave?: boolean;
  /** App name for the accessible title ("Kiln Write"). */
  appName?: string;
  /** Document title shown at the top of the rail. */
  documentTitle?: string;
  className?: string;
}

/**
 * Office Backstage (File): a full-screen overlay with the familiar left navigation. Apps plug the
 * content in per page. Esc or the back arrow closes it; focus is trapped while open and returns to
 * the File tab afterwards.
 */
export function Backstage({
  open,
  onOpenChange,
  pages,
  page: pageProp,
  defaultPage,
  onPageChange,
  onAction,
  actions = ['save', 'close', 'options'],
  autoSave = false,
  appName = 'Kiln',
  documentTitle,
  className,
}: BackstageProps) {
  const commands = useOptionalCommands();
  const available = (id: BackstagePageId) => pages[id] !== undefined && pages[id] !== null;
  const hidden = (e: NavEntry) => {
    if (e.kind === 'divider') return false;
    if (e.kind === 'action') return !actions.includes(e.id);
    if (e.id === 'saveAs' && autoSave && available('saveCopy')) return true;
    if (e.id === 'saveCopy' && !autoSave && available('saveAs')) return true;
    return !available(e.id);
  };
  const firstPage = (TOP.find((e) => e.kind === 'page' && !hidden(e)) as { id: BackstagePageId } | undefined)
    ?.id;
  const [pageState, setPageState] = useState<BackstagePageId | undefined>(defaultPage);
  const requested = pageProp ?? pageState;
  const page =
    requested && available(requested)
      ? requested
      : defaultPage && available(defaultPage)
        ? defaultPage
        : firstPage;
  const contentRef = useRef<HTMLDivElement>(null);
  // The File button is not a dialog trigger (Key Tips and shortcuts open it too): remember focus.
  const returnFocus = useRef<HTMLElement | null>(null);
  if (open && !returnFocus.current && typeof document !== 'undefined') {
    returnFocus.current = document.activeElement as HTMLElement | null;
  }

  const go = (id: BackstagePageId) => {
    if (pageProp === undefined) setPageState(id);
    onPageChange?.(id);
  };

  // Each page change moves focus to the page heading, so screen readers announce it.
  useEffect(() => {
    if (open && page) contentRef.current?.querySelector<HTMLElement>('h1')?.focus({ preventScroll: true });
  }, [open, page]);

  const runAction = (e: Extract<NavEntry, { kind: 'action' }>) => {
    onOpenChange(false);
    if (onAction) onAction(e.id);
    else if (commands) void commands.registry.execute(e.command);
  };

  const renderEntry = (e: NavEntry, i: number) => {
    if (e.kind === 'divider') {
      return <li key={`d${i}`} className="kiln-backstage__divider" aria-hidden="true" />;
    }
    if (hidden(e)) return null;
    const current = e.kind === 'page' && e.id === page;
    return (
      <li key={e.id}>
        <button
          type="button"
          className={cx('kiln-backstage__nav-item', current && 'is-current')}
          aria-current={current ? 'page' : undefined}
          data-backstage={e.id}
          data-command={e.kind === 'action' ? e.command : undefined}
          onClick={() => (e.kind === 'page' ? go(e.id) : runAction(e))}
        >
          <Icon icon={e.icon} size={18} />
          <span>{t(e.label)}</span>
        </button>
      </li>
    );
  };

  return (
    <RD.Root open={open} onOpenChange={onOpenChange}>
      <RD.Portal>
        <RD.Content
          className={cx('kiln-backstage', className)}
          aria-describedby={undefined}
          onCloseAutoFocus={(e) => {
            e.preventDefault();
            const el = returnFocus.current;
            returnFocus.current = null;
            if (el?.isConnected) el.focus();
          }}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            (e.currentTarget as HTMLElement | null)
              ?.querySelector<HTMLElement>('.kiln-backstage__back')
              ?.focus();
          }}
        >
          <RD.Title className="kiln-visually-hidden">
            {t('{appName} File menu', { appName })}
            {documentTitle ? `: ${documentTitle}` : ''}
          </RD.Title>
          <nav className="kiln-backstage__rail" aria-label={t('File menu')}>
            <RD.Close className="kiln-backstage__back" aria-label={t('Back to the document')}>
              <Icon icon={ArrowLeft} size={20} />
            </RD.Close>
            {documentTitle ? (
              <div className="kiln-backstage__doc" title={documentTitle}>
                {documentTitle}
              </div>
            ) : null}
            <ul className="kiln-backstage__nav">{TOP.map(renderEntry)}</ul>
            <ul className="kiln-backstage__nav kiln-backstage__nav--bottom">{BOTTOM.map(renderEntry)}</ul>
          </nav>
          <main ref={contentRef} className="kiln-backstage__content" data-page={page}>
            {page ? (
              <>
                <h1 className="kiln-backstage__title" tabIndex={-1}>
                  {t(BACKSTAGE_PAGE_TITLES[page])}
                </h1>
                <div className="kiln-backstage__page">{pages[page]}</div>
              </>
            ) : null}
          </main>
        </RD.Content>
      </RD.Portal>
    </RD.Root>
  );
}
