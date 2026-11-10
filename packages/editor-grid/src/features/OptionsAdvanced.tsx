/**
 * Kiln Grid's page of File ▸ Options: Advanced ▸ When calculating this workbook (Excel's place for
 * the workbook's date system). The core keeps the option with the workbook and saves it in the
 * file (`workbookPr/@date1904`).
 */
import { t } from '@kiln/i18n';
import type { OptionsExtraSection } from '@kiln/ui-kit';
import { useEffect, useId, useState } from 'react';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';

export function GridAdvancedOptions({ model }: { model: GridEditorModel }) {
  const on = useGrid(model, (s) => s.grid?.features.date1904 ?? false);
  // The box follows a click at once; the core's state confirms it (or puts it back).
  const [shown, setShown] = useState(on);
  useEffect(() => setShown(on), [on]);
  const id = useId();
  return (
    <fieldset className="kiln-options__section">
      <legend>{t('When calculating this workbook')}</legend>
      <div className="kiln-options__check">
        <input
          id={id}
          type="checkbox"
          data-testid="grid-date-1904"
          checked={shown}
          onChange={(e) => {
            const want = e.target.checked;
            setShown(want);
            void model.client
              .execute(model.doc, { Grid: { DateSystem: { date1904: want } } })
              .catch(() => undefined)
              .then(async () => {
                const s = await model.fetch();
                setShown(s?.features.date1904 ?? want);
              });
          }}
        />
        <label htmlFor={id}>
          {t('Use 1904 date system')}
          <span className="kiln-options__hint">
            {t(
              'Dates count from 1 January 1904. Cells keep their numbers, so dates already in the workbook move by four years and a day.',
            )}
          </span>
        </label>
      </div>
    </fieldset>
  );
}

/** The Options sections Kiln Grid adds to the app frame's dialog. */
export function gridOptionsSections(model: GridEditorModel): OptionsExtraSection[] {
  return [{ id: 'advanced', label: t('Advanced'), content: <GridAdvancedOptions model={model} /> }];
}
