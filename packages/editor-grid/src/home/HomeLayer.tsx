/**
 * The Auto Fill Options button Excel shows at the corner of a fill-handle fill, with its menu
 * (Copy Cells, Fill Series, Fill Formatting Only, Fill Without Formatting, the date units and
 * Flash Fill). A choice redoes the fill in the core (`grid.editing.auto_fill_option`); the button
 * goes once anything else changes the workbook. The border pen's mode shows as a badge.
 */

import { msg, t, tx } from '@kiln/i18n';
import type { GridAutoFillChoice } from '@kiln/protocol';
import { Icon, Menu, MenuContent, MenuRadioGroup, MenuRadioItem, MenuTrigger } from '@kiln/ui-kit';
import { ChevronDown, ClipboardList } from 'lucide-react';
import './home.css';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';

const LABELS: Readonly<Record<string, string>> = {
  Copy: 'Copy Cells',
  Series: 'Fill Series',
  FormatsOnly: 'Fill Formatting Only',
  WithoutFormatting: 'Fill Without Formatting',
  Days: 'Fill Days',
  Weekdays: 'Fill Weekdays',
  Months: 'Fill Months',
  Years: 'Fill Years',
  FlashFill: 'Flash Fill',
};

/** The id and label of a choice. */
export function choiceKey(c: GridAutoFillChoice): string {
  return c === 'FlashFill' ? 'FlashFill' : c.Fill;
}

export function choiceLabel(c: GridAutoFillChoice): string {
  const k = choiceKey(c);
  return LABELS[k] ?? k;
}

const PEN_LABELS = {
  Draw: msg('Draw Border'),
  Grid: msg('Draw Border Grid'),
  Erase: msg('Erase Border'),
} as const;

export function GridHomeLayer({ model }: { model: GridEditorModel }) {
  const fill = useGrid(model, (s) => s.grid?.home?.auto_fill ?? null);
  const pen = useGrid(model, (s) => s.grid?.home?.pen ?? null);
  if (!fill && !pen) return null;
  return (
    <div className="kiln-ghome__layer">
      {fill ? (
        <div className="kiln-ghome__autofill" style={{ left: fill.x + 2, top: fill.y + 2 }}>
          <Menu
            onOpenChange={(open) => {
              // The choices of the latest fill (the chrome's copy can lag behind the core).
              if (open) void model.fetch();
            }}
          >
            <MenuTrigger asChild>
              <button
                type="button"
                className="kiln-ghome__autofill-button"
                aria-label={t('Auto Fill Options')}
                title={t('Auto Fill Options')}
                data-testid="grid-auto-fill-options"
              >
                <Icon icon={ClipboardList} size={14} />
                <Icon icon={ChevronDown} size={10} />
              </button>
            </MenuTrigger>
            <MenuContent
              align="start"
              data-kiln-shortcuts="off"
              onCloseAutoFocus={(e: Event) => {
                e.preventDefault();
                model.surface?.focus();
              }}
            >
              <MenuRadioGroup value={choiceKey(fill.current)}>
                {fill.choices.map((c) => (
                  <MenuRadioItem
                    key={choiceKey(c)}
                    value={choiceKey(c)}
                    onSelect={async () => {
                      // Only a choice the core's last fill still offers.
                      const now = (await model.fetch())?.home?.auto_fill;
                      if (now?.choices.some((k) => choiceKey(k) === choiceKey(c)))
                        await model.run('grid.editing.auto_fill_option', { choice: choiceKey(c) });
                      model.surface?.focus();
                    }}
                  >
                    {choiceLabel(c)}
                  </MenuRadioItem>
                ))}
              </MenuRadioGroup>
            </MenuContent>
          </Menu>
        </div>
      ) : null}
      {pen ? (
        <output className="kiln-ghome__pen" data-testid="grid-border-pen" aria-live="polite">
          {t('{pen} — Esc to stop', { pen: tx(PEN_LABELS[pen.mode]) })}
        </output>
      ) : null}
    </div>
  );
}
