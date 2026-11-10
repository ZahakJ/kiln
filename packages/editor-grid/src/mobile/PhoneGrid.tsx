/**
 * Kiln Grid's phone parts:
 *
 * - the quick keys under the formula bar while the on-screen keyboard is up (the bar is pinned
 *   on it then, with ✕ and ✓): 123 brings the number keys, A→Z the letters, fx Insert Function,
 *   and the characters formulas need most (= ( ) : , $ %), typed where the caret is;
 * - the selection's statistics as a chip beside the sheet chips (the desktop's status bar), with
 *   every figure in a bottom sheet;
 * - a cell's pick list (data validation, Alt+Down) as a bottom sheet of finger-sized rows.
 */
import { formatCount, localizeNumberText, t } from '@kiln/i18n';
import { BottomSheet, Icon } from '@kiln/ui-kit';
import { Sigma } from 'lucide-react';
import { type PointerEvent, useEffect, useRef, useState } from 'react';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';
import './mobile.css';

/** The characters formulas need most, typed where the caret is. */
const KEYS = ['=', '(', ')', ':', ',', '$', '%'] as const;

/** The text field that has the keys in the grid editor (the formula box, or the cell's proxy). */
function editingField(from: HTMLElement | null): HTMLTextAreaElement | null {
  const active = document.activeElement;
  if (active instanceof HTMLTextAreaElement && from?.closest('.kiln-grid')?.contains(active)) return active;
  return from?.closest('.kiln-grid')?.querySelector<HTMLTextAreaElement>('.kiln-formulabox__input') ?? null;
}

const keep = (e: PointerEvent) => e.preventDefault();

export function GridQuickKeys({ model }: { model: GridEditorModel }) {
  const row = useRef<HTMLDivElement>(null);
  const [keys, setKeys] = useState<'decimal' | 'text'>('text');
  const switchTo = (mode: 'decimal' | 'text') => {
    setKeys(mode);
    const field = editingField(row.current);
    if (!field) return;
    field.inputMode = mode;
    // The keyboard changes its keys when the field is focused again.
    field.blur();
    field.focus({ preventScroll: true });
  };
  const type = (text: string) => {
    const field = editingField(row.current);
    if (!field) return;
    if (document.activeElement !== field) field.focus({ preventScroll: true });
    // As if typed: the field's input event sends it to the core.
    document.execCommand('insertText', false, text);
  };
  return (
    <div
      ref={row}
      className="kiln-grid-quick"
      role="toolbar"
      aria-label={t('Quick keys')}
      data-testid="grid-quick-keys"
    >
      <div className="kiln-grid-quick__modes">
        <button
          type="button"
          className="kiln-grid-quick__mode"
          aria-pressed={keys === 'decimal'}
          aria-label={t('{keys} number keys', { keys: '123' })}
          data-testid="grid-quick-123"
          onPointerDown={keep}
          onClick={() => switchTo('decimal')}
        >
          {/* i18n-ignore: the number keys' symbol */}
          123
        </button>
        <button
          type="button"
          className="kiln-grid-quick__mode"
          aria-label={t('fx Insert Function')}
          data-testid="grid-quick-fx"
          onPointerDown={keep}
          onClick={() => model.openDialog('insertFunction')}
        >
          {/* i18n-ignore: Excel's fx symbol */}
          fx
        </button>
        <button
          type="button"
          className="kiln-grid-quick__mode"
          aria-pressed={keys === 'text'}
          aria-label={t('{keys} letter keys', { keys: 'A→Z' })}
          data-testid="grid-quick-abc"
          onPointerDown={keep}
          onClick={() => switchTo('text')}
        >
          {/* i18n-ignore: the letter keys' symbol */}
          A→Z
        </button>
      </div>
      <div className="kiln-grid-quick__keys">
        {KEYS.map((k) => (
          <button
            type="button"
            key={k}
            className="kiln-grid-quick__key"
            aria-label={t('Type {key}', { key: k })}
            onPointerDown={keep}
            onClick={() => type(k)}
          >
            {k}
          </button>
        ))}
      </div>
    </div>
  );
}

/** The selection's figures, as the desktop's status bar shows them. */
function figures(stats: NonNullable<ReturnType<typeof statsOf>>) {
  const num = (v: string | null) => (v === null ? null : localizeNumberText(v));
  return [
    { id: 'sum', label: t('Sum'), value: num(stats.sum) },
    { id: 'average', label: t('Average'), value: num(stats.average) },
    { id: 'count', label: t('Count'), value: formatCount(stats.count) },
    { id: 'numerical', label: t('Numerical Count'), value: formatCount(stats.numerical_count) },
    { id: 'min', label: t('Minimum'), value: num(stats.min) },
    { id: 'max', label: t('Maximum'), value: num(stats.max) },
  ].filter((f): f is { id: string; label: string; value: string } => f.value !== null);
}

function statsOf(model: GridEditorModel) {
  return model.state.grid?.stats ?? null;
}

export function PhoneStats({ model }: { model: GridEditorModel }) {
  const stats = useGrid(model, (s) => s.grid?.stats ?? null);
  const [open, setOpen] = useState(false);
  if (!stats) return null;
  const all = figures(stats);
  const first = all[0];
  if (!first) return null;
  return (
    <>
      <button
        type="button"
        className="kiln-grid-stats"
        aria-haspopup="dialog"
        data-testid="grid-phone-stats"
        onClick={() => setOpen(true)}
      >
        <Icon icon={Sigma} size={15} />
        <span className="kiln-grid-stats__label">{first.label}</span>
        <b>{first.value}</b>
      </button>
      <BottomSheet
        open={open}
        onOpenChange={setOpen}
        title={t('Selection')}
        testId="grid-stats-sheet"
        onCloseAutoFocus={(e) => {
          e.preventDefault();
          model.surface?.focus();
        }}
      >
        <dl className="kiln-grid-stats__list">
          {all.map((f) => (
            <div key={f.id} className="kiln-grid-stats__row">
              <dt>{f.label}</dt>
              <dd>{f.value}</dd>
            </div>
          ))}
        </dl>
      </BottomSheet>
    </>
  );
}

export function PickListSheet({ model }: { model: GridEditorModel }) {
  const list = useGrid(model, (s) => s.grid?.editor?.pick_list ?? null);
  const index = useGrid(model, (s) => s.grid?.editor?.completion_index ?? -1);
  // A pick commits the cell; until then closing the sheet cancels. Each list opened starts unpicked.
  const picked = useRef(false);
  const shown = list ? list.join('\n') : null;
  useEffect(() => {
    if (shown !== null) picked.current = false;
  }, [shown]);
  if (!list) return null;
  const pick = async (i: number) => {
    picked.current = true;
    await model.core('grid.editing.accept_completion', { index: i });
    await model.core('grid.editing.commit', {});
    model.surface?.focus();
  };
  return (
    <BottomSheet
      open
      onOpenChange={(open) => {
        if (open || picked.current) return;
        void model.core('grid.editing.cancel_edit');
        model.surface?.focus();
      }}
      title={t('Pick from list')}
      testId="grid-pick-sheet"
    >
      <div className="kiln-grid-pick">
        {list.map((v, i) => (
          <button
            type="button"
            key={`${i}:${v}`}
            aria-current={i === index || undefined}
            className="kiln-grid-pick__item"
            onClick={() => void pick(i)}
          >
            {v || t('(Blanks)')}
          </button>
        ))}
      </div>
    </BottomSheet>
  );
}
