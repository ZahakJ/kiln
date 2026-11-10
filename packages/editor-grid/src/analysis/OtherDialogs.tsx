/**
 * The other analysis dialogs: Create Sparklines, Edit Sparklines, Hidden and Empty Cell
 * Settings and the marker colours; Goal Seek and Goal Seek Status; Subtotal; Protect Sheet,
 * Unprotect Sheet, Protect Workbook and Unprotect Workbook with password confirmation; and the
 * message box of a refused change.
 */
import { msg, t, tx, txPairs } from '@kiln/i18n';
import type {
  GridPivotFunction,
  GridSheetPermissions,
  GridSparkEmpty,
  GridSparkKind,
  GridSubtotalSpec,
} from '@kiln/protocol';
import { Button, ColorPicker } from '@kiln/ui-kit';
import { useEffect, useState } from 'react';
import { activeCell } from '../features/model.ts';
import { a1, Check, Choice, columnLabel, FeatureFrame, Radios, TextField } from '../features/ui.tsx';
import { useGrid } from '../hooks.ts';
import { selectionText } from './commands.ts';
import type { GridAnalysis } from './model.ts';
import { FUNCTIONS } from './PivotDialogs.tsx';

const KIND_TITLE: Record<GridSparkKind, string> = {
  Line: msg('Line sparklines'),
  Column: msg('Column sparklines'),
  WinLoss: msg('Win/Loss sparklines'),
};

/** Insert ▸ Sparklines. */
export function SparklinesDialog({ a, kind }: { a: GridAnalysis; kind: GridSparkKind }) {
  // A selected block of data fills Data Range; an empty selection is where they go.
  const sel = selectionText(a.model);
  const g = a.model.state.grid;
  const looksLikeData = !!g?.stats && g.stats.numerical_count > 0;
  const [data, setData] = useState(looksLikeData ? sel : '');
  const [location, setLocation] = useState(looksLikeData ? '' : sel);
  // The field the keys start in, chosen once: typing in one must not move them to the other.
  const [startInData] = useState(!data);
  return (
    <FeatureFrame
      features={a}
      title={t('Create Sparklines')}
      size="sm"
      testId="grid-create-sparklines"
      okDisabled={!data.trim() || !location.trim()}
      onOk={() => a.tryExec({ InsertSparklines: { kind, data: data.trim(), location: location.trim() } })}
    >
      <p className="kiln-gfx__hint">{tx(KIND_TITLE[kind])}</p>
      <p className="kiln-gfx__title">{t('Choose the data that you want')}</p>
      <TextField
        label={t('Data Range')}
        value={data}
        onChange={setData}
        autoFocus={startInData}
        wide
        testId="grid-spark-data"
      />
      <p className="kiln-gfx__title">{t('Choose where you want the sparklines to be placed')}</p>
      <TextField
        label={t('Location Range')}
        value={location}
        onChange={setLocation}
        autoFocus={!startInData}
        wide
        testId="grid-spark-location"
      />
    </FeatureFrame>
  );
}

/** Sparkline ▸ Edit Data. */
export function SparkDataDialog({ a }: { a: GridAnalysis }) {
  const s = useGrid(a.model, (st) => st.grid?.analysis.sparkline ?? null);
  const [data, setData] = useState(s?.data ?? '');
  const [location, setLocation] = useState(s?.location ?? '');
  return (
    <FeatureFrame
      features={a}
      title={t('Edit Sparklines')}
      size="sm"
      testId="grid-edit-sparklines"
      onOk={() =>
        a.tryExec({ Sparklines: { edit: { Data: { data: data.trim(), location: location.trim() } } } })
      }
    >
      <TextField label={t('Data Range')} value={data} onChange={setData} autoFocus wide />
      <TextField label={t('Location Range')} value={location} onChange={setLocation} wide />
    </FeatureFrame>
  );
}

const POINT_TITLE: Record<string, string> = {
  negative: msg('Negative Points'),
  markers: msg('Markers'),
  high: msg('High Point'),
  low: msg('Low Point'),
  first: msg('First Point'),
  last: msg('Last Point'),
};
const SLOT: Record<string, number> = { negative: 1, markers: 2, high: 3, low: 4, first: 5, last: 6 };

/** Hidden and Empty Cell Settings, or the colour of one kind of point. */
export function SparkSettingsDialog({ a, color }: { a: GridAnalysis; color?: string }) {
  const s = useGrid(a.model, (st) => st.grid?.analysis.sparkline ?? null);
  const [empty, setEmpty] = useState<GridSparkEmpty>(s?.options.empty ?? 'Gap');
  const [hidden, setHidden] = useState(s?.options.show_hidden ?? false);
  const slot = color ? (SLOT[color] ?? 2) : -1;
  const [hex, setHex] = useState(s?.options.colors[slot] ?? 'D00000');
  if (!s) return null;
  if (color) {
    return (
      <FeatureFrame
        features={a}
        title={t('Marker Color: {point}', { point: tx(POINT_TITLE[color] ?? 'Markers') })}
        size="sm"
        testId="grid-spark-marker-color"
        onOk={() => {
          const colors = [...s.options.colors];
          colors[slot] = hex;
          const flag = color === 'markers' ? { markers: true } : { [color]: true };
          return a.tryExec({
            Sparklines: { edit: { Options: { options: { ...s.options, ...flag, colors } } } },
          });
        }}
      >
        <ColorPicker
          value={{ type: 'rgb', hex: `#${hex}` }}
          onChange={(c) => {
            if (c.type === 'rgb') setHex(c.hex.replace('#', '').toUpperCase());
          }}
        />
      </FeatureFrame>
    );
  }
  return (
    <FeatureFrame
      features={a}
      title={t('Hidden and Empty Cell Settings')}
      size="sm"
      testId="grid-spark-hidden"
      onOk={() =>
        a.tryExec({
          Sparklines: { edit: { Options: { options: { ...s.options, empty, show_hidden: hidden } } } },
        })
      }
    >
      <Radios
        label={t('Show empty cells as')}
        value={empty}
        onChange={setEmpty}
        options={[
          ['Gap', t('Gaps')],
          ['Zero', t('Zero')],
          ['Span', t('Connect data points with line')],
        ]}
      />
      <Check label={t('Show data in hidden rows and columns')} checked={hidden} onChange={setHidden} />
    </FeatureFrame>
  );
}

/** Data ▸ What-If Analysis ▸ Goal Seek. */
export function GoalSeekDialog({ a }: { a: GridAnalysis }) {
  const at = activeCell(a.model);
  const [set, setSet] = useState(at ? a1(at.row, at.col, true) : '');
  const [to, setTo] = useState('');
  const [by, setBy] = useState('');
  return (
    <FeatureFrame
      features={a}
      title={t('Goal Seek')}
      size="sm"
      testId="grid-goal-seek"
      okDisabled={!set.trim() || !to.trim() || !by.trim()}
      onOk={async () => {
        const e = await a.tryExec({
          GoalSeek: { set_cell: set.trim(), to_value: to.trim(), by_changing: by.trim() },
        });
        if (e) return e;
        // The status dialog takes over (this one closes first).
        setTimeout(() => a.openDialog('goalSeekStatus'), 0);
      }}
    >
      <TextField label={t('Set cell')} value={set} onChange={setSet} wide testId="grid-goal-set" />
      <TextField label={t('To value')} value={to} onChange={setTo} autoFocus wide testId="grid-goal-to" />
      <TextField label={t('By changing cell')} value={by} onChange={setBy} wide testId="grid-goal-by" />
    </FeatureFrame>
  );
}

/** Goal Seek Status: OK keeps the solution, Cancel undoes it. */
export function GoalSeekStatusDialog({ a }: { a: GridAnalysis }) {
  const s = useGrid(a.model, (st) => st.grid?.analysis.goal_seek ?? null);
  if (!s) return null;
  return (
    <FeatureFrame
      features={a}
      title={t('Goal Seek Status')}
      size="sm"
      testId="grid-goal-status"
      onOk={() => undefined}
      onEscape={async () => {
        await a.model.run('suite.edit.undo');
        a.closeDialog();
      }}
      actions={
        <>
          <Button
            onClick={async () => {
              await a.model.run('suite.edit.undo');
              a.closeDialog();
            }}
          >
            {t('Cancel')}
          </Button>
          <Button variant="primary" autoFocus onClick={() => a.closeDialog()}>
            {t('OK')}
          </Button>
        </>
      }
    >
      <p>
        {s.found
          ? t('Goal Seeking with Cell {cell} found a solution.', { cell: s.cell })
          : t('Goal Seeking with Cell {cell} may not have found a solution.', { cell: s.cell })}
      </p>
      <dl className="kiln-gan__status">
        <dt>{t('Target value:')}</dt>
        <dd>{s.target}</dd>
        <dt>{t('Current value:')}</dt>
        <dd data-testid="grid-goal-current">{s.current}</dd>
      </dl>
    </FeatureFrame>
  );
}

/** Data ▸ Subtotal. */
export function SubtotalDialog({ a }: { a: GridAnalysis }) {
  const [headers, setHeaders] = useState<string[]>([]);
  const [c0, setC0] = useState(0);
  const [at, setAt] = useState('0');
  const [fn, setFn] = useState<GridPivotFunction>('Sum');
  const [cols, setCols] = useState<number[]>([]);
  const [replace, setReplace] = useState(true);
  const [breaks, setBreaks] = useState(false);
  const [below, setBelow] = useState(true);
  useEffect(() => {
    let live = true;
    void a.features.ask('DataRange').then((r) => {
      if (!live || !r || !('DataRange' in r)) return;
      const d = r.DataRange;
      setC0(d.range.c0);
      setHeaders(d.headers);
      // Excel's defaults: the first column, and the last numeric-looking column.
      setAt(String(d.range.c0));
      setCols([d.range.c0 + d.headers.length - 1]);
    });
    return () => {
      live = false;
    };
  }, [a]);
  const spec = (): GridSubtotalSpec => ({
    at_change_in: Number(at),
    function: fn,
    columns: cols,
    replace,
    page_breaks: breaks,
    below,
  });
  return (
    <FeatureFrame
      features={a}
      title={t('Subtotal')}
      size="sm"
      testId="grid-subtotal"
      okDisabled={cols.length === 0 || headers.length === 0}
      footerStart={
        <Button
          onClick={async () => {
            const e = await a.tryExec({ Subtotal: { spec: null } });
            if (!e) a.closeDialog();
            else a.model.set({ notice: e });
          }}
        >
          {t('Remove All')}
        </Button>
      }
      onOk={() => a.tryExec({ Subtotal: { spec: spec() } })}
    >
      <Choice
        label={t('At each change in')}
        value={at}
        options={headers.map((h, i) => [String(c0 + i), columnLabel(h, c0 + i)] as const)}
        onChange={setAt}
        width={240}
      />
      <Choice
        label={t('Use function')}
        value={fn}
        options={txPairs(FUNCTIONS)}
        onChange={setFn}
        width={240}
      />
      <p className="kiln-gfx__title">{t('Add subtotal to')}</p>
      <div className="kiln-gan__items" data-testid="grid-subtotal-columns">
        {headers.map((h, i) => (
          <Check
            key={i}
            label={columnLabel(h, c0 + i)}
            checked={cols.includes(c0 + i)}
            onChange={(on) => setCols(on ? [...cols, c0 + i] : cols.filter((c) => c !== c0 + i))}
          />
        ))}
      </div>
      <Check label={t('Replace current subtotals')} checked={replace} onChange={setReplace} />
      <Check label={t('Page break between groups')} checked={breaks} onChange={setBreaks} />
      <Check label={t('Summary below data')} checked={below} onChange={setBelow} />
    </FeatureFrame>
  );
}

const PERMISSIONS: readonly (readonly [keyof GridSheetPermissions, string])[] = [
  ['select_locked', msg('Select locked cells')],
  ['select_unlocked', msg('Select unlocked cells')],
  ['format_cells', msg('Format cells')],
  ['format_columns', msg('Format columns')],
  ['format_rows', msg('Format rows')],
  ['insert_columns', msg('Insert columns')],
  ['insert_rows', msg('Insert rows')],
  ['insert_hyperlinks', msg('Insert hyperlinks')],
  ['delete_columns', msg('Delete columns')],
  ['delete_rows', msg('Delete rows')],
  ['sort', msg('Sort')],
  ['auto_filter', msg('Use AutoFilter')],
  ['pivot_tables', msg('Use PivotTable & PivotChart')],
  ['edit_objects', msg('Edit objects')],
  ['edit_scenarios', msg('Edit scenarios')],
];

const MISMATCH = msg('Confirmation password is not identical.');

/** Review ▸ Protect Sheet. */
export function ProtectSheetDialog({ a }: { a: GridAnalysis }) {
  const current = useGrid(a.model, (s) => s.grid?.analysis.permissions);
  const [allow, setAllow] = useState<GridSheetPermissions>(
    current ?? {
      select_locked: true,
      select_unlocked: true,
      format_cells: false,
      format_columns: false,
      format_rows: false,
      insert_columns: false,
      insert_rows: false,
      insert_hyperlinks: false,
      delete_columns: false,
      delete_rows: false,
      sort: false,
      auto_filter: false,
      pivot_tables: false,
      edit_objects: false,
      edit_scenarios: false,
    },
  );
  const [password, setPassword] = useState('');
  return (
    <FeatureFrame
      features={a}
      title={t('Protect Sheet')}
      size="sm"
      testId="grid-protect-sheet"
      onOk={() => {
        if (password) {
          // Excel asks for the password again first (this dialog closes, that one opens).
          setTimeout(() => a.openDialog('confirmPassword', { target: 'sheet', password, allow }), 0);
          return;
        }
        return a.tryExec({ ProtectSheet: { protect: true, password: '', allow } });
      }}
    >
      <PasswordField
        label={t('Password to unprotect sheet')}
        value={password}
        onChange={setPassword}
        autoFocus
      />
      <p className="kiln-gfx__title">{t('Allow all users of this worksheet to')}</p>
      <div className="kiln-gan__items" data-testid="grid-protect-permissions">
        {PERMISSIONS.map(([key, label]) => (
          <Check
            key={key}
            label={tx(label)}
            checked={allow[key]}
            onChange={(on) => setAllow({ ...allow, [key]: on })}
          />
        ))}
      </div>
    </FeatureFrame>
  );
}

/** Confirm Password: the password typed again before a sheet or the workbook is protected. */
export function ConfirmPasswordDialog({
  a,
  target,
  password,
  allow,
}: {
  a: GridAnalysis;
  target: 'sheet' | 'workbook';
  password: string;
  allow?: GridSheetPermissions;
}) {
  const [again, setAgain] = useState('');
  return (
    <FeatureFrame
      features={a}
      title={t('Confirm Password')}
      size="sm"
      testId="grid-confirm-password"
      onOk={() => {
        if (again !== password) return t(MISMATCH);
        return target === 'sheet'
          ? a.tryExec({ ProtectSheet: { protect: true, password, allow: allow ?? null } })
          : a.tryExec({ ProtectWorkbook: { protect: true, password } });
      }}
    >
      <PasswordField label={t('Reenter password to proceed')} value={again} onChange={setAgain} autoFocus />
      <p className="kiln-gfx__hint">
        {t('Keep the password in a safe place: a lost password cannot be recovered.')}
      </p>
    </FeatureFrame>
  );
}

/** Review ▸ Unprotect Sheet. */
export function UnprotectSheetDialog({ a }: { a: GridAnalysis }) {
  const [password, setPassword] = useState('');
  return (
    <FeatureFrame
      features={a}
      title={t('Unprotect Sheet')}
      size="sm"
      testId="grid-unprotect-sheet"
      onOk={() => a.tryExec({ ProtectSheet: { protect: false, password, allow: null } })}
    >
      <PasswordField label={t('Password')} value={password} onChange={setPassword} autoFocus />
    </FeatureFrame>
  );
}

/** Review ▸ Protect Workbook (structure). */
export function ProtectWorkbookDialog({ a }: { a: GridAnalysis }) {
  const [password, setPassword] = useState('');
  const [structure, setStructure] = useState(true);
  return (
    <FeatureFrame
      features={a}
      title={t('Protect Structure and Windows')}
      size="sm"
      testId="grid-protect-workbook"
      okDisabled={!structure}
      onOk={() => {
        if (password) {
          setTimeout(() => a.openDialog('confirmPassword', { target: 'workbook', password }), 0);
          return;
        }
        return a.tryExec({ ProtectWorkbook: { protect: true, password: '' } });
      }}
    >
      <p className="kiln-gfx__title">{t('Protect workbook for')}</p>
      <Check label={t('Structure')} checked={structure} onChange={setStructure} />
      <PasswordField label={t('Password (optional)')} value={password} onChange={setPassword} autoFocus />
    </FeatureFrame>
  );
}

/** Review ▸ Unprotect Workbook. */
export function UnprotectWorkbookDialog({ a }: { a: GridAnalysis }) {
  const [password, setPassword] = useState('');
  return (
    <FeatureFrame
      features={a}
      title={t('Unprotect Workbook')}
      size="sm"
      testId="grid-unprotect-workbook"
      onOk={() => a.tryExec({ ProtectWorkbook: { protect: false, password } })}
    >
      <PasswordField label={t('Password')} value={password} onChange={setPassword} autoFocus />
    </FeatureFrame>
  );
}

/** The message box of a refused change (a protected sheet, a PivotTable report). */
export function AlertDialog({ a, message }: { a: GridAnalysis; message: string }) {
  return (
    <FeatureFrame
      features={a}
      title={t('Kiln Grid')}
      size="sm"
      testId="grid-alert"
      actions={
        <Button variant="primary" autoFocus onClick={() => a.closeDialog()}>
          {t('OK')}
        </Button>
      }
    >
      <p role="alert">{message}</p>
    </FeatureFrame>
  );
}

function PasswordField({
  label,
  value,
  onChange,
  autoFocus,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  autoFocus?: boolean;
}) {
  const id = `kiln-gan-pw-${label.replace(/\W+/g, '-').toLowerCase()}`;
  return (
    <div className="kiln-griddlg__field kiln-gfx__wide">
      <label className="kiln-griddlg__label" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        type="password"
        className="kiln-griddlg__input"
        value={value}
        // biome-ignore lint/a11y/noAutofocus: the dialog's first field takes the keys, as in Excel.
        autoFocus={autoFocus}
        autoComplete="new-password"
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}
