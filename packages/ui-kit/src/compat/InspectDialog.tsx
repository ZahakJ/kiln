/**
 * File ▸ Info ▸ Check for Issues ▸ Inspect Document (Office's Document Inspector): the hidden
 * data and personal information a document carries (`Query::Inspect`), each kind with Remove All
 * (`suite.file.inspect_remove`). Kiln Write: comments, tracked changes, properties; Kiln Grid:
 * comments and notes, hidden worksheets, properties; Kiln Deck: comments, speaker notes, hidden
 * slides, properties.
 */
import { t } from '@kiln/i18n';
import type { InspectItem, InspectKind } from '@kiln/protocol';
import {
  CircleCheck,
  EyeOff,
  GitPullRequestArrow,
  type LucideIcon,
  MessageSquare,
  NotebookPen,
  RefreshCw,
  TriangleAlert,
  UserRound,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { Dialog, DialogContent } from '../primitives/Dialog.tsx';
import type { CompatApp } from './types.ts';
import './compat.css';

interface KindInfo {
  icon: LucideIcon;
  title: string;
  about: string;
  found: (count: number) => string;
}

function kindInfo(kind: InspectKind, app: CompatApp): KindInfo {
  switch (kind) {
    case 'Comments':
      return {
        icon: MessageSquare,
        title: app === 'grid' ? t('Comments and notes') : t('Comments'),
        about: t('What reviewers wrote, with their names.'),
        found: (count) => t('{count, plural, one {# comment found} other {# comments found}}', { count }),
      };
    case 'Revisions':
      return {
        icon: GitPullRequestArrow,
        title: t('Tracked changes'),
        about: t('Insertions, deletions and formatting changes not yet accepted. Remove All accepts them.'),
        found: (count) =>
          t('{count, plural, one {# tracked change found} other {# tracked changes found}}', { count }),
      };
    case 'Properties':
      return {
        icon: UserRound,
        title: t('Document properties and personal information'),
        about: t('Author, last modified by, company and manager. The title and tags stay.'),
        found: (count) =>
          t('{count, plural, one {# personal property found} other {# personal properties found}}', {
            count,
          }),
      };
    case 'HiddenSheets':
      return {
        icon: EyeOff,
        title: t('Hidden worksheets'),
        about: t('Sheets hidden from view but kept in the file. Remove All deletes them.'),
        found: (count) =>
          t('{count, plural, one {# hidden worksheet found} other {# hidden worksheets found}}', { count }),
      };
    case 'HiddenSlides':
      return {
        icon: EyeOff,
        title: t('Hidden slides'),
        about: t('Slides the show skips but the file keeps. Remove All deletes them.'),
        found: (count) =>
          t('{count, plural, one {# hidden slide found} other {# hidden slides found}}', { count }),
      };
    case 'SpeakerNotes':
      return {
        icon: NotebookPen,
        title: t('Presentation notes'),
        about: t('The speaker notes under the slides.'),
        found: (count) =>
          t('{count, plural, one {# slide with notes found} other {# slides with notes found}}', { count }),
      };
  }
}

export interface InspectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  app: CompatApp;
  inspect: () => Promise<InspectItem[]>;
  remove: (kind: InspectKind) => Promise<void>;
  readOnly?: boolean;
}

export function InspectDialog({ open, onOpenChange, app, inspect, remove, readOnly }: InspectDialogProps) {
  const [items, setItems] = useState<InspectItem[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [removed, setRemoved] = useState<InspectKind[]>([]);
  const [busy, setBusy] = useState<InspectKind | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!open) return;
    let live = true;
    setFailed(false);
    inspect().then(
      (r) => live && setItems(r),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, [open, attempt]);
  useEffect(() => {
    if (!open) {
      setItems(null);
      setRemoved([]);
    }
  }, [open]);

  const removeAll = async (kind: InspectKind) => {
    setBusy(kind);
    try {
      await remove(kind);
      setRemoved((r) => [...r, kind]);
      setItems(await inspect());
    } catch {
      setFailed(true);
    } finally {
      setBusy(null);
    }
  };

  const found = items?.filter((i) => i.count > 0).length ?? 0;
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('Inspect Document')}
        size="lg"
        className="kiln-compat"
        data-testid="inspect-dialog"
        footer={
          <>
            <Button icon={<Icon icon={RefreshCw} size={15} />} onClick={() => setAttempt((a) => a + 1)}>
              {t('Inspect again')}
            </Button>
            <Button variant="primary" onClick={() => onOpenChange(false)}>
              {t('Done')}
            </Button>
          </>
        }
      >
        <p className="kiln-compat__intro">
          {items === null
            ? t('Looking for data you may not want to share…')
            : found === 0
              ? t('Nothing hidden travels with this document.')
              : t(
                  '{count, plural, one {# kind of hidden data found.} other {# kinds of hidden data found.}} Remove what you do not want to share; Undo brings comments, changes, sheets and slides back.',
                  {
                    count: found,
                  },
                )}
        </p>
        {failed ? (
          <div className="kiln-compat__state" role="alert">
            <strong>{t('The inspection did not finish.')}</strong>
            <span>{t('Nothing in the document changed. Inspect again to retry.')}</span>
          </div>
        ) : null}
        <ul className="kiln-inspect" aria-busy={items === null}>
          {(items ?? []).map((item) => {
            const k = kindInfo(item.kind, app);
            const has = item.count > 0;
            return (
              <li
                key={item.kind}
                className="kiln-inspect__row"
                data-kind={item.kind}
                data-found={has || undefined}
              >
                <span className="kiln-inspect__icon" aria-hidden="true">
                  <Icon icon={k.icon} size={18} />
                </span>
                <div className="kiln-inspect__text">
                  <strong>{k.title}</strong>
                  <span className="kiln-inspect__about">{k.about}</span>
                  <span className="kiln-inspect__status">
                    <Icon icon={has ? TriangleAlert : CircleCheck} size={14} />
                    {has ? k.found(item.count) : removed.includes(item.kind) ? t('Removed') : t('None found')}
                  </span>
                </div>
                {has ? (
                  <Button
                    size="sm"
                    busy={busy === item.kind}
                    disabled={readOnly || (busy !== null && busy !== item.kind)}
                    onClick={() => void removeAll(item.kind)}
                    aria-label={t('Remove All: {what}', { what: k.title })}
                  >
                    {t('Remove All')}
                  </Button>
                ) : null}
              </li>
            );
          })}
        </ul>
      </DialogContent>
    </Dialog>
  );
}
