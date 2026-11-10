/** The open feature dialog of the Grid editor (one at a time, as Excel's modal dialogs). */
import { t } from '@kiln/i18n';
import type { GridFilterOp, GridImportFormat, GridSpecial } from '@kiln/protocol';
import { useEffect, useRef, useState } from 'react';
import { useStore } from 'zustand';
import { CircularWarning, IterativeDialog, ValidationAlert } from './dialogs/CalcDialogs.tsx';
import { type CfPreset, ManageRulesDialog, QuickRuleDialog, RuleDialog } from './dialogs/CfDialogs.tsx';
import {
  CreateTableDialog,
  ImportDialog,
  RemoveDuplicatesDialog,
  ResizeTableDialog,
  TextToColumnsDialog,
  ValidationDialog,
} from './dialogs/DataDialogs.tsx';
import { FindDialog, GoToSpecialDialog, LinkDialog, SymbolDialog } from './dialogs/FindDialogs.tsx';
import {
  CreateNamesDialog,
  DefineNameDialog,
  ErrorCheckingDialog,
  EvaluateDialog,
  NameManagerDialog,
  PasteNameDialog,
} from './dialogs/FormulaDialogs.tsx';
import { FunctionArgsDialog } from './dialogs/FunctionArgs.tsx';
import { AltTextDialog, CropDialog, FormatObjectDialog, ShapeTextDialog } from './dialogs/ObjectDialogs.tsx';
import { PageSetupDialog } from './dialogs/PageSetupDialog.tsx';
import { SortDialog } from './dialogs/SortDialog.tsx';
import { CustomFilterDialog, Top10Dialog } from './FilterMenu.tsx';
import { activeCell, authorName, type GridFeatures } from './model.ts';
import { colName } from './ui.tsx';

const QUICK = new Set([
  'greater',
  'less',
  'between',
  'equal',
  'text',
  'date',
  'duplicate',
  'top10',
  'top10pct',
  'bottom10',
  'bottom10pct',
  'above',
  'below',
]);

export function FeatureDialogs({ features }: { features: GridFeatures }) {
  const dialog = useStore(features.store, (s) => s.dialog);
  if (!dialog) return null;
  const arg = dialog.arg;
  switch (dialog.id) {
    case 'sort':
      return <SortDialog features={features} />;
    case 'cfRule':
      return typeof arg === 'string' && QUICK.has(arg) ? (
        <QuickRuleDialog features={features} preset={arg as CfPreset} />
      ) : (
        <RuleDialog features={features} rule={null} />
      );
    case 'cfManage':
      return <ManageRulesDialog features={features} />;
    case 'validation':
      return <ValidationDialog features={features} />;
    case 'createTable':
      return <CreateTableDialog features={features} style={typeof arg === 'string' ? arg : ''} />;
    case 'resizeTable':
      return <ResizeTableDialog features={features} />;
    case 'find':
      return <FindDialog features={features} tab={arg === 'replace' ? 'replace' : 'find'} />;
    case 'goToSpecial':
      return <GoToSpecialDialog features={features} preset={arg as GridSpecial | undefined} />;
    case 'nameManager':
      return <NameManagerDialog features={features} />;
    case 'defineName':
      return <DefineNameDialog features={features} />;
    case 'createNames':
      return <CreateNamesDialog features={features} />;
    case 'pasteName':
      return <PasteNameDialog features={features} />;
    case 'link':
      return <LinkDialog features={features} />;
    case 'pageSetup':
      return <PageSetupDialog features={features} tab={typeof arg === 'string' ? arg : 'page'} />;
    case 'textToColumns':
      return <TextToColumnsDialog features={features} />;
    case 'removeDuplicates':
      return <RemoveDuplicatesDialog features={features} />;
    case 'import':
      return (
        <ImportDialog
          features={features}
          file={arg as { name: string; bytes: number[]; format: GridImportFormat }}
        />
      );
    case 'evaluate':
      return <EvaluateDialog features={features} />;
    case 'errorChecking':
      return <ErrorCheckingDialog features={features} />;
    case 'symbol':
      return <SymbolDialog features={features} />;
    case 'note':
      return <NoteEditor features={features} />;
    case 'iterative':
      return <IterativeDialog features={features} />;
    case 'circular':
      return <CircularWarning features={features} />;
    case 'validationAlert':
      return <ValidationAlert features={features} />;
    case 'functionArgs':
      return <FunctionArgsDialog features={features} name={typeof arg === 'string' ? arg : 'SUM'} />;
    case 'altText':
      return <AltTextDialog features={features} />;
    case 'crop':
      return <CropDialog features={features} />;
    case 'formatObject':
      return <FormatObjectDialog features={features} />;
    case 'shapeText':
      return <ShapeTextDialog features={features} start={typeof arg === 'string' ? arg : undefined} />;
    case 'customFilter':
      return (
        <CustomFilterDialog
          features={features}
          at={arg as { row: number; col: number; op: GridFilterOp; kind: string }}
        />
      );
    case 'top10':
      return <Top10Dialog features={features} at={arg as { row: number; col: number }} />;
    default:
      return null;
  }
}

/** The note of the active cell, edited in a card beside it (New Note, Edit Note, Shift+F2). */
function NoteEditor({ features }: { features: GridFeatures }) {
  // The cell, place and sheet the editor was opened on (from the core's current state): the
  // note goes there even if the selection moves before it saves.
  type Target = { cell: string; sheet: number | undefined; rect: readonly number[]; existing: boolean };
  const [target, setTarget] = useState<Target | null>(null);
  // The target as it loads: a save typed faster than that waits for it.
  const ready = useRef<Promise<Target | null>>(Promise.resolve(null));
  const [text, setText] = useState('');
  const original = useRef('');
  const saved = useRef(false);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    let live = true;
    let found: (t: Target | null) => void = () => undefined;
    ready.current = new Promise((resolve) => {
      found = resolve;
    });
    void (async () => {
      const g = await features.model.fetch();
      const a = activeCell(features.model);
      if (!g || !a) {
        found(null);
        return;
      }
      const t: Target = {
        cell: `${colName(a.col)}${a.row + 1}`,
        sheet: g.active_sheet,
        rect: g.features.cell_rect,
        existing: g.features.note,
      };
      found(t);
      if (!live) return;
      const r = await features.ask('Comments');
      const list = r && 'Comments' in r ? r.Comments : [];
      const n = list.find((c) => c.note && c.sheet === t.sheet && c.cell === t.cell);
      if (!live) return;
      if (n) {
        setText(n.text);
        original.current = n.text;
      }
      setTarget(t);
      requestAnimationFrame(() => ref.current?.focus());
    })();
    return () => {
      live = false;
    };
  }, [features]);
  const [x = 0, y = 0, w = 0] = target?.rect ?? [];
  const save = async () => {
    // Esc saves and closes; the blur that follows must not save again.
    if (saved.current) return;
    saved.current = true;
    const t = target ?? (await ready.current);
    if (!t) {
      features.closeDialog();
      return;
    }
    const changed = text !== original.current && (text.trim() !== '' || t.existing);
    if (changed && features.model.state.grid?.active_sheet === t.sheet) {
      await features.exec({
        SetNote: { text: text.trim() ? text : null, author: authorName(), cell: t.cell },
      });
    }
    features.closeDialog();
  };
  return (
    <div
      className="kiln-gfx__note-editor"
      role="dialog"
      aria-label={t('Note for {cell}', { cell: target?.cell ?? '' })}
      data-testid="grid-note-editor"
      data-kiln-shortcuts="off"
      style={{ left: x + w + 12, top: y }}
    >
      <div className="kiln-gfx__note-author">{authorName()}:</div>
      <textarea
        ref={ref}
        value={text}
        aria-label={t('Note')}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            void save();
          }
        }}
        onBlur={() => void save()}
      />
      <div className="kiln-gfx__note-cell">
        {target?.existing ? t('Edit note') : t('New note')} · {target?.cell ?? ''}
      </div>
    </div>
  );
}
