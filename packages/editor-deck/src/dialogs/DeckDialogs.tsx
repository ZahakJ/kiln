/**
 * Deck's dialogs: Slide Size, Format Background, Insert Table, Header & Footer, Size and Position,
 * Alt Text, Link, Find, Replace, Replace Fonts, Rename Section, Crop, Paste Special, the Selection
 * Pane and the full Shapes gallery (Insert and Change Shape). Each ends in core commands; what
 * they show (the shapes' outlines, the selection's size) is the core's.
 */
import { msg, t, tx } from '@kiln/i18n';
import type { DeckShapeIcon } from '@kiln/protocol';
import {
  Button,
  type ColorChoice,
  ColorPicker,
  Dialog,
  DialogContent,
  Icon,
  NumberInput,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Select,
} from '@kiln/ui-kit';
import { ChevronDown, Eye, EyeOff } from 'lucide-react';
import { type ReactNode, useEffect, useId, useMemo, useState } from 'react';
import { ChartDataDialog, InsertChartDialog } from '../charts/DeckCharts.tsx';
import { EquationDialog } from '../features/EquationDialog.tsx';
import { SetUpShowDialog } from '../features/SetUpShowDialog.tsx';
import { pickFile } from '../files.ts';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import { IconsDialog } from './IconsDialog.tsx';
import { CellMarginsDialog, CodeBlockDialog, FormatShapeDialog, SplitCellsDialog } from './ObjectDialogs.tsx';
import { FontDialog, ParagraphDialog, SymbolDialog } from './TextDialogs.tsx';

/** Focuses a dialog's first field when it opens. */
const focusOnMount = (el: HTMLInputElement | null) => {
  if (el && !el.dataset.focused) {
    el.dataset.focused = '1';
    el.focus();
  }
};

/** A modal dialog of the Deck editor, closed through the model. */
export function DeckDialogFrame({
  model,
  title,
  onOk,
  okLabel = 'OK',
  okDisabled,
  size = 'sm',
  testId,
  extra,
  children,
}: {
  model: DeckEditorModel;
  title: string;
  onOk?: () => unknown;
  okLabel?: string;
  okDisabled?: boolean;
  size?: 'sm' | 'md' | 'lg';
  testId?: string;
  /** Buttons before OK (Apply to All, Replace All). */
  extra?: ReactNode;
  children: ReactNode;
}) {
  const close = () => model.closeDialog();
  const ok = async () => {
    if (okDisabled) return;
    await onOk?.();
    close();
  };
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : close())}>
      <DialogContent
        onCloseAutoFocus={(e) => {
          // Back to the slide (typing goes on there), not to the button that opened the dialog.
          e.preventDefault();
          model.stage?.focus();
        }}
        title={title}
        size={size}
        data-testid={testId}
        data-kiln-shortcuts="off"
        footer={
          <>
            {extra}
            {onOk ? (
              <>
                <Button onClick={close}>{t('Cancel')}</Button>
                <Button variant="primary" disabled={okDisabled} onClick={ok}>
                  {okLabel}
                </Button>
              </>
            ) : (
              <Button onClick={close}>{t('Close')}</Button>
            )}
          </>
        }
      >
        <form
          className="kiln-deckdlg"
          onSubmit={(e) => {
            e.preventDefault();
            void ok();
          }}
        >
          {children}
          <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function Field({ label, children }: { label: string; children: (id: string) => ReactNode }) {
  const id = useId();
  return (
    <div className="kiln-deckdlg__field">
      <label htmlFor={id}>{label}</label>
      {children(id)}
    </div>
  );
}

export function Check({
  label,
  checked,
  onChange,
  testId,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  testId?: string;
}) {
  return (
    <label className="kiln-deckdlg__check">
      <input
        type="checkbox"
        checked={checked}
        data-testid={testId}
        onChange={(e) => onChange(e.target.checked)}
      />
      {label}
    </label>
  );
}

/** A colour field: a button with the colour that opens the palette. */
export function ColorField({
  label,
  value,
  onChange,
}: {
  label: string;
  /** `RRGGBB`, or a theme colour name. */
  value: string;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const hex = /^[0-9A-F]{6}$/i.test(value) ? `#${value}` : null;
  return (
    <div className="kiln-deckdlg__field">
      <span id={`${id}-label`}>{label}</span>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button type="button" className="kiln-deckcolor" aria-labelledby={`${id}-label ${id}`} id={id}>
            <span
              className="kiln-deckcolor__swatch"
              style={hex ? { background: hex } : undefined}
              aria-hidden="true"
            />
            <span>{hex ?? value}</span>
            <Icon icon={ChevronDown} size={14} />
          </button>
        </PopoverTrigger>
        <PopoverContent align="start">
          <ColorPicker
            onChange={(c: ColorChoice) => {
              if (c.type === 'theme') onChange(c.slot);
              else if (c.type === 'rgb') onChange(c.hex.replace('#', '').toUpperCase());
              setOpen(false);
            }}
          />
        </PopoverContent>
      </Popover>
    </div>
  );
}

// ── Slide Size ──────────────────────────────────────────────────────────────────────────────────

const SIZE_PRESETS: Record<string, [number, number]> = {
  widescreen: [13.333, 7.5],
  standard: [10, 7.5],
  a4: [10.833, 7.5],
  letter: [10, 7.5],
  banner: [8, 1],
};

function SlideSizeDialog({ model }: { model: DeckEditorModel }) {
  const deck = useDeck(model, (s) => s.deck);
  const [kind, setKind] = useState(
    deck?.size_kind === 'custom' ? 'custom' : (deck?.size_kind ?? 'widescreen'),
  );
  const [w, setW] = useState(Math.round(((deck?.slide_width ?? 1280) / 96) * 1000) / 1000);
  const [h, setH] = useState(Math.round(((deck?.slide_height ?? 720) / 96) * 1000) / 1000);
  const [mode, setMode] = useState<'EnsureFit' | 'Maximize'>('EnsureFit');
  return (
    <DeckDialogFrame
      model={model}
      title={t('Slide Size')}
      testId="deck-slide-size"
      onOk={() =>
        model.run(
          'deck.design.slide_size',
          kind === 'widescreen'
            ? { ratio: '16:9', mode }
            : kind === 'standard'
              ? { ratio: '4:3', mode }
              : { width_in: w, height_in: h, mode },
        )
      }
    >
      <Field label={t('Slides sized for')}>
        {(id) => (
          <Select
            id={id}
            value={kind}
            onValueChange={(v) => {
              setKind(v);
              const p = SIZE_PRESETS[v];
              if (p) {
                setW(p[0]);
                setH(p[1]);
              }
            }}
            options={[
              { value: 'widescreen', label: t('Widescreen (13.333 × 7.5 in)') },
              { value: 'standard', label: t('Standard 4:3 (10 × 7.5 in)') },
              { value: 'a4', label: t('A4 Paper (10.833 × 7.5 in)') },
              { value: 'letter', label: t('Letter Paper (10 × 7.5 in)') },
              { value: 'banner', label: t('Banner (8 × 1 in)') },
              { value: 'custom', label: t('Custom') },
            ]}
          />
        )}
      </Field>
      <div className="kiln-deckdlg__row">
        <Field label={t('Width')}>
          {(id) => (
            <NumberInput
              id={id}
              value={w}
              unit="in"
              min={1}
              max={56}
              step={0.1}
              onValueChange={(v) => {
                setW(v);
                setKind('custom');
              }}
            />
          )}
        </Field>
        <Field label={t('Height')}>
          {(id) => (
            <NumberInput
              id={id}
              value={h}
              unit="in"
              min={1}
              max={56}
              step={0.1}
              onValueChange={(v) => {
                setH(v);
                setKind('custom');
              }}
            />
          )}
        </Field>
      </div>
      <fieldset className="kiln-deckdlg__radios">
        <legend>{t('Content on the slides')}</legend>
        <label>
          <input type="radio" checked={mode === 'EnsureFit'} onChange={() => setMode('EnsureFit')} />
          {t('Ensure Fit (scale down to fit the new size)')}
        </label>
        <label>
          <input type="radio" checked={mode === 'Maximize'} onChange={() => setMode('Maximize')} />
          {t('Maximize (fill the new size, content may go off the slide)')}
        </label>
      </fieldset>
    </DeckDialogFrame>
  );
}

// ── Format Background ───────────────────────────────────────────────────────────────────────────

export const PATTERNS = [
  'pct10',
  'pct25',
  'ltUpDiag',
  'ltDnDiag',
  'smGrid',
  'dotGrid',
  'horz',
  'vert',
  'smCheck',
  'weave',
];

function FormatBackgroundDialog({ model }: { model: DeckEditorModel }) {
  const deck = useDeck(model, (s) => s.deck);
  const [kind, setKind] = useState<'solid' | 'gradient' | 'picture' | 'pattern'>('solid');
  const [color, setColor] = useState(deck?.background ?? 'FFFFFF');
  const [to, setTo] = useState('E05A27');
  const [angle, setAngle] = useState(90);
  const [transparency, setTransparency] = useState(0);
  const [pattern, setPattern] = useState('pct25');
  const [file, setFile] = useState<{ name: string; bytes: number[] } | null>(null);
  const [hide, setHide] = useState(deck?.hide_graphics ?? false);
  const args = () => {
    switch (kind) {
      case 'gradient':
        return { gradient: { from: color, to, angle } };
      case 'picture':
        return file ? { name: file.name, bytes: file.bytes } : null;
      case 'pattern':
        return { fill: { Pattern: { preset: pattern, fg: color, bg: to } } };
      default:
        return { color, transparency };
    }
  };
  const apply = async (all: boolean) => {
    const a = args();
    if (a) await model.run('deck.design.format_background', { ...a, all });
    if (hide !== (deck?.hide_graphics ?? false))
      await model.run('deck.design.hide_background_graphics', { hide });
  };
  return (
    <DeckDialogFrame
      model={model}
      title={t('Format Background')}
      testId="deck-format-background"
      okLabel={t('Apply')}
      okDisabled={kind === 'picture' && !file}
      onOk={() => apply(false)}
      extra={
        <>
          <Button
            onClick={async () => {
              await model.run('deck.design.format_background', { reset: true });
              model.closeDialog();
            }}
          >
            {t('Reset Background')}
          </Button>
          <Button
            disabled={kind === 'picture' && !file}
            onClick={async () => {
              await apply(true);
              model.closeDialog();
            }}
          >
            {t('Apply to All')}
          </Button>
        </>
      }
    >
      <fieldset className="kiln-deckdlg__radios">
        <legend>{t('Fill')}</legend>
        {(
          [
            ['solid', t('Solid fill')],
            ['gradient', t('Gradient fill')],
            ['picture', t('Picture fill')],
            ['pattern', t('Pattern fill')],
          ] as const
        ).map(([k, label]) => (
          <label key={k}>
            <input type="radio" name="fill" checked={kind === k} onChange={() => setKind(k)} />
            {label}
          </label>
        ))}
      </fieldset>
      {kind !== 'picture' ? (
        <ColorField
          label={kind === 'pattern' ? t('Foreground') : kind === 'gradient' ? t('From') : t('Colour')}
          value={color}
          onChange={setColor}
        />
      ) : null}
      {kind === 'gradient' || kind === 'pattern' ? (
        <ColorField label={kind === 'pattern' ? t('Background') : t('To')} value={to} onChange={setTo} />
      ) : null}
      {kind === 'gradient' ? (
        <Field label={t('Angle')}>
          {(id) => <NumberInput id={id} value={angle} unit="°" min={0} max={359} onValueChange={setAngle} />}
        </Field>
      ) : null}
      {kind === 'solid' ? (
        <Field label={t('Transparency')}>
          {(id) => (
            <NumberInput
              id={id}
              value={transparency}
              unit="%"
              min={0}
              max={100}
              onValueChange={setTransparency}
            />
          )}
        </Field>
      ) : null}
      {kind === 'pattern' ? (
        <Field label={t('Pattern')}>
          {(id) => (
            <Select
              id={id}
              value={pattern}
              onValueChange={setPattern}
              options={PATTERNS.map((p) => ({ value: p, label: p }))}
            />
          )}
        </Field>
      ) : null}
      {kind === 'picture' ? (
        <Field label={t('Picture')}>
          {(id) => (
            <input
              id={id}
              type="file"
              accept="image/png,image/jpeg,image/gif,image/bmp,image/webp"
              onChange={async (e) => {
                const f = e.target.files?.[0];
                if (!f) return;
                setFile({ name: f.name, bytes: Array.from(new Uint8Array(await f.arrayBuffer())) });
              }}
            />
          )}
        </Field>
      ) : null}
      <Check label={t('Hide background graphics')} checked={hide} onChange={setHide} />
    </DeckDialogFrame>
  );
}

// ── Insert Table, Header & Footer, Size and Position, Alt Text, Link ───────────────────────────

function InsertTableDialog({ model }: { model: DeckEditorModel }) {
  const [rows, setRows] = useState(3);
  const [cols, setCols] = useState(4);
  return (
    <DeckDialogFrame
      model={model}
      title={t('Insert Table')}
      testId="deck-insert-table"
      onOk={() => model.run('deck.insert.table', { rows, cols })}
    >
      <div className="kiln-deckdlg__row">
        <Field label={t('Number of columns')}>
          {(id) => (
            <NumberInput
              id={id}
              value={cols}
              unit=""
              min={1}
              max={75}
              precision={0}
              onValueChange={setCols}
            />
          )}
        </Field>
        <Field label={t('Number of rows')}>
          {(id) => (
            <NumberInput
              id={id}
              value={rows}
              unit=""
              min={1}
              max={75}
              precision={0}
              onValueChange={setRows}
            />
          )}
        </Field>
      </div>
    </DeckDialogFrame>
  );
}

function HeaderFooterDialog({ model }: { model: DeckEditorModel }) {
  const hf = useDeck(model, (s) => s.deck?.header_footer);
  const [date, setDate] = useState(hf?.date ?? false);
  const [fixed, setFixed] = useState('');
  const [fixedOn, setFixedOn] = useState(false);
  const [number, setNumber] = useState(hf?.slide_number ?? false);
  const [footerOn, setFooterOn] = useState(hf?.footer !== null && hf?.footer !== undefined);
  const [footer, setFooter] = useState(hf?.footer ?? '');
  const [notOnTitle, setNotOnTitle] = useState(false);
  const apply = (all: boolean) =>
    model.run('deck.insert.header_footer', {
      date,
      fixed_date: date && fixedOn ? fixed : null,
      slide_number: number,
      footer: footerOn ? footer : null,
      not_on_title: notOnTitle,
      all,
    });
  return (
    <DeckDialogFrame
      model={model}
      title={t('Header and Footer')}
      testId="deck-header-footer"
      size="md"
      okLabel={t('Apply')}
      onOk={() => apply(false)}
      extra={
        <Button
          onClick={async () => {
            await apply(true);
            model.closeDialog();
          }}
        >
          {t('Apply to All')}
        </Button>
      }
    >
      <Check label={t('Date and time')} checked={date} onChange={setDate} />
      {date ? (
        <div className="kiln-deckdlg__indent">
          <label className="kiln-deckdlg__check">
            <input type="radio" checked={!fixedOn} onChange={() => setFixedOn(false)} />
            {t('Update automatically')}
          </label>
          <label className="kiln-deckdlg__check">
            <input type="radio" checked={fixedOn} onChange={() => setFixedOn(true)} />
            {t('Fixed')}
          </label>
          {fixedOn ? (
            <input
              className="kiln-deckdlg__text"
              aria-label={t('Fixed date')}
              value={fixed}
              onChange={(e) => setFixed(e.target.value)}
            />
          ) : null}
        </div>
      ) : null}
      <Check label={t('Slide number')} checked={number} onChange={setNumber} testId="deck-hf-number" />
      <Check label={t('Footer')} checked={footerOn} onChange={setFooterOn} />
      {footerOn ? (
        <input
          className="kiln-deckdlg__text kiln-deckdlg__indent"
          aria-label={t('Footer text')}
          value={footer}
          onChange={(e) => setFooter(e.target.value)}
        />
      ) : null}
      <Check label={t("Don't show on title slide")} checked={notOnTitle} onChange={setNotOnTitle} />
    </DeckDialogFrame>
  );
}

function SizePositionDialog({ model }: { model: DeckEditorModel }) {
  const b = useDeck(model, (s) => s.deck?.selection_box);
  const [w, setW] = useState(b?.width ?? 0);
  const [h, setH] = useState(b?.height ?? 0);
  const [x, setX] = useState(b?.x ?? 0);
  const [y, setY] = useState(b?.y ?? 0);
  const [r, setR] = useState(b?.rotation ?? 0);
  const [lock, setLock] = useState(false);
  return (
    <DeckDialogFrame
      model={model}
      title={t('Size and Position')}
      testId="deck-size-position"
      okDisabled={!b}
      onOk={() =>
        model.run('deck.format.size_position', { x, y, width: w, height: h, rotation: r, lock_aspect: lock })
      }
    >
      <div className="kiln-deckdlg__row">
        <Field label={t('Height')}>
          {(id) => <NumberInput id={id} value={h} unit="pt" min={1} max={4000} onValueChange={setH} />}
        </Field>
        <Field label={t('Width')}>
          {(id) => <NumberInput id={id} value={w} unit="pt" min={1} max={4000} onValueChange={setW} />}
        </Field>
      </div>
      <Field label={t('Rotation')}>
        {(id) => <NumberInput id={id} value={r} unit="°" min={-360} max={360} onValueChange={setR} />}
      </Field>
      <Check label={t('Lock aspect ratio')} checked={lock} onChange={setLock} />
      <div className="kiln-deckdlg__row">
        <Field label={t('Horizontal position')}>
          {(id) => <NumberInput id={id} value={x} unit="pt" min={-4000} max={4000} onValueChange={setX} />}
        </Field>
        <Field label={t('Vertical position')}>
          {(id) => <NumberInput id={id} value={y} unit="pt" min={-4000} max={4000} onValueChange={setY} />}
        </Field>
      </div>
    </DeckDialogFrame>
  );
}

function AltTextDialog({ model }: { model: DeckEditorModel }) {
  const alt = useDeck(model, (s) => s.deck?.alt_text ?? '');
  const [text, setText] = useState(alt);
  return (
    <DeckDialogFrame
      model={model}
      title={t('Alt Text')}
      testId="deck-alt-text"
      onOk={() => model.run('deck.format.alt_text', { text })}
    >
      <p className="kiln-deckdlg__hint">
        {t('How would you describe this object and its context to someone who cannot see it?')}
      </p>
      <textarea
        className="kiln-deckdlg__area"
        aria-label={t('Description')}
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
    </DeckDialogFrame>
  );
}

function LinkDialog({ model }: { model: DeckEditorModel }) {
  const count = useDeck(model, (s) => s.deck?.slides.length ?? 0);
  const [kind, setKind] = useState<'web' | 'file' | 'slide' | 'mail'>('web');
  const [url, setUrl] = useState('https://');
  const [path, setPath] = useState('');
  const [slide, setSlide] = useState(1);
  const target =
    kind === 'slide'
      ? `#slide:${slide}`
      : kind === 'mail'
        ? `mailto:${url.replace(/^mailto:/, '')}`
        : kind === 'file'
          ? path.trim()
          : url;
  return (
    <DeckDialogFrame
      model={model}
      title={t('Insert Hyperlink')}
      testId="deck-link"
      okDisabled={kind === 'file' ? path.trim() === '' : kind !== 'slide' && url.trim() === ''}
      onOk={() => model.run('deck.insert.link', { url: target })}
      extra={
        <Button
          onClick={async () => {
            await model.run('deck.insert.link', { url: null });
            model.closeDialog();
          }}
        >
          {t('Remove Link')}
        </Button>
      }
    >
      <fieldset className="kiln-deckdlg__radios">
        <legend>{t('Link to')}</legend>
        <label>
          <input type="radio" checked={kind === 'web'} onChange={() => setKind('web')} />
          {t('Web page')}
        </label>
        <label>
          <input type="radio" checked={kind === 'file'} onChange={() => setKind('file')} />
          {t('Existing file')}
        </label>
        <label>
          <input type="radio" checked={kind === 'slide'} onChange={() => setKind('slide')} />
          {t('Place in this presentation')}
        </label>
        <label>
          <input type="radio" checked={kind === 'mail'} onChange={() => setKind('mail')} />
          {t('Email address')}
        </label>
      </fieldset>
      {kind === 'file' ? (
        <div className="kiln-deckdlg__row kiln-deckdlg__row--end">
          <Field label={t('File')}>
            {(id) => (
              <input
                id={id}
                className="kiln-deckdlg__text"
                placeholder={t('report.pdf or ../Budget.xlsx')}
                value={path}
                onChange={(e) => setPath(e.target.value)}
              />
            )}
          </Field>
          <Button
            onClick={async () => {
              // The browser gives the file's name only: the link is relative to the presentation.
              const f = await pickFile('*/*', 'deck-link-file');
              if (f) setPath(f.name);
            }}
          >
            {t('Browse…')}
          </Button>
        </div>
      ) : kind === 'slide' ? (
        <Field label={t('Slide')}>
          {(id) => (
            <NumberInput
              id={id}
              value={slide}
              unit=""
              min={1}
              max={Math.max(1, count)}
              precision={0}
              onValueChange={setSlide}
            />
          )}
        </Field>
      ) : (
        <Field label={kind === 'mail' ? t('Email address') : t('Address')}>
          {(id) => (
            <input
              id={id}
              className="kiln-deckdlg__text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
            />
          )}
        </Field>
      )}
    </DeckDialogFrame>
  );
}

// ── Find, Replace, Replace Fonts, Rename Section, Crop, Paste Special ─────────────────────────

function FindDialog({ model, replace }: { model: DeckEditorModel; replace: boolean }) {
  const find = useDeck(model, (s) => s.find);
  const done = useDeck(model, (s) => s.deck?.find_done ?? false);
  const set = (patch: Partial<typeof find>) => model.set({ find: { ...model.state.find, ...patch } });
  const opts = { match_case: find.matchCase, whole_word: find.wholeWord };
  const next = () => model.run('deck.edit.find', { text: find.text, ...opts, backward: false });
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : model.closeDialog())}>
      <DialogContent
        onCloseAutoFocus={(e) => {
          // Back to the slide (typing goes on there), not to the button that opened the dialog.
          e.preventDefault();
          model.stage?.focus();
        }}
        title={replace ? t('Replace') : t('Find')}
        size="sm"
        data-testid="deck-find"
        // Non-modal in spirit: the found text shows on the slide behind.
        footer={
          <>
            <Button variant="primary" disabled={!find.text} onClick={() => void next()}>
              {t('Find Next')}
            </Button>
            {replace ? (
              <>
                <Button
                  disabled={!find.text}
                  onClick={() =>
                    void model.run('deck.edit.replace', {
                      text: find.text,
                      with: find.with,
                      ...opts,
                      all: false,
                    })
                  }
                >
                  {t('Replace')}
                </Button>
                <Button
                  disabled={!find.text}
                  onClick={() =>
                    void model.run('deck.edit.replace', {
                      text: find.text,
                      with: find.with,
                      ...opts,
                      all: true,
                    })
                  }
                >
                  {t('Replace All')}
                </Button>
              </>
            ) : (
              <Button onClick={() => model.openDialog('replace')}>{t('Replace…')}</Button>
            )}
            <Button onClick={() => model.closeDialog()}>{t('Close')}</Button>
          </>
        }
      >
        <form
          className="kiln-deckdlg"
          onSubmit={(e) => {
            e.preventDefault();
            if (find.text) void next();
          }}
        >
          <Field label={t('Find what')}>
            {(id) => (
              <input
                id={id}
                className="kiln-deckdlg__text"
                value={find.text}
                ref={focusOnMount}
                onChange={(e) => set({ text: e.target.value })}
              />
            )}
          </Field>
          {replace ? (
            <Field label={t('Replace with')}>
              {(id) => (
                <input
                  id={id}
                  className="kiln-deckdlg__text"
                  value={find.with}
                  onChange={(e) => set({ with: e.target.value })}
                />
              )}
            </Field>
          ) : null}
          <Check label={t('Match case')} checked={find.matchCase} onChange={(v) => set({ matchCase: v })} />
          <Check
            label={t('Find whole words only')}
            checked={find.wholeWord}
            onChange={(v) => set({ wholeWord: v })}
          />
          {done ? (
            <p className="kiln-deckdlg__hint" role="status">
              {t('Kiln Deck has finished searching the presentation.')}
            </p>
          ) : null}
          <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
        </form>
      </DialogContent>
    </Dialog>
  );
}

function ReplaceFontsDialog({ model }: { model: DeckEditorModel }) {
  const family = useDeck(model, (s) => s.deck?.format?.font_family ?? '');
  const [from, setFrom] = useState(family);
  const [to, setTo] = useState('Figtree');
  return (
    <DeckDialogFrame
      model={model}
      title={t('Replace Font')}
      testId="deck-replace-fonts"
      okLabel={t('Replace')}
      okDisabled={!from || !to}
      onOk={() => model.run('deck.edit.replace_font', { from, to })}
    >
      <Field label={t('Replace')}>
        {(id) => (
          <input
            id={id}
            className="kiln-deckdlg__text"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
          />
        )}
      </Field>
      <Field label={t('With')}>
        {(id) => (
          <input id={id} className="kiln-deckdlg__text" value={to} onChange={(e) => setTo(e.target.value)} />
        )}
      </Field>
    </DeckDialogFrame>
  );
}

function RenameSectionDialog({ model, index }: { model: DeckEditorModel; index: number }) {
  const name = useDeck(model, (s) => s.deck?.sections[index]?.name ?? '');
  const [text, setText] = useState(name);
  return (
    <DeckDialogFrame
      model={model}
      title={t('Rename Section')}
      testId="deck-rename-section"
      okLabel={t('Rename')}
      okDisabled={!text.trim()}
      onOk={() => model.run('deck.slides.rename_section', { index, name: text.trim() })}
    >
      <Field label={t('Section name')}>
        {(id) => (
          <input
            id={id}
            className="kiln-deckdlg__text"
            ref={focusOnMount}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        )}
      </Field>
    </DeckDialogFrame>
  );
}

/** Slide Master ▸ Rename: the name of the master or layout on the stage. */
function RenameMasterDialog({ model }: { model: DeckEditorModel }) {
  const name = useDeck(model, (s) => {
    const v = s.deck?.master_view;
    return v?.items.find((i) => i.id === v.current)?.name ?? '';
  });
  const master = useDeck(model, (s) => {
    const v = s.deck?.master_view;
    return v?.items.find((i) => i.id === v.current)?.master ?? false;
  });
  const [text, setText] = useState(name);
  return (
    <DeckDialogFrame
      model={model}
      title={master ? t('Rename Master') : t('Rename Layout')}
      testId="deck-rename-master"
      okLabel={t('Rename')}
      okDisabled={!text.trim()}
      onOk={() => model.run('deck.master.rename', { name: text.trim() })}
    >
      <Field label={master ? t('Master name') : t('Layout name')}>
        {(id) => (
          <input
            id={id}
            className="kiln-deckdlg__text"
            ref={focusOnMount}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        )}
      </Field>
    </DeckDialogFrame>
  );
}

function CropDialog({ model }: { model: DeckEditorModel }) {
  const [v, setV] = useState({ left: 0, top: 0, right: 0, bottom: 0 });
  return (
    <DeckDialogFrame
      model={model}
      title={t('Crop Picture')}
      testId="deck-crop"
      okLabel={t('Crop')}
      onOk={() =>
        model.run('deck.picture.crop', {
          left: v.left / 100,
          top: v.top / 100,
          right: v.right / 100,
          bottom: v.bottom / 100,
        })
      }
    >
      <p className="kiln-deckdlg__hint">{t('How much to trim from each side, as a share of the picture.')}</p>
      <div className="kiln-deckdlg__row">
        {(['left', 'right', 'top', 'bottom'] as const).map((k) => (
          <Field key={k} label={k[0]?.toUpperCase() + k.slice(1)}>
            {(id) => (
              <NumberInput
                id={id}
                value={v[k]}
                unit="%"
                min={0}
                max={90}
                onValueChange={(n) => setV((o) => ({ ...o, [k]: n }))}
              />
            )}
          </Field>
        ))}
      </div>
    </DeckDialogFrame>
  );
}

function PasteSpecialDialog({ model }: { model: DeckEditorModel }) {
  const [how, setHow] = useState('suite.edit.paste_keep_source');
  return (
    <DeckDialogFrame
      model={model}
      title={t('Paste Special')}
      testId="deck-paste-special"
      onOk={() => model.run(how)}
    >
      <fieldset className="kiln-deckdlg__radios">
        <legend>{t('As')}</legend>
        {(
          [
            ['suite.edit.paste_keep_source', t('Keep Source Formatting')],
            ['suite.edit.paste_merge_formatting', t('Use Destination Theme')],
            ['suite.edit.paste_text_only', t('Unformatted Text')],
            ['deck.clipboard.paste_picture', t('Picture (PNG)')],
          ] as const
        ).map(([id, label]) => (
          <label key={id}>
            <input type="radio" checked={how === id} onChange={() => setHow(id)} />
            {label}
          </label>
        ))}
      </fieldset>
    </DeckDialogFrame>
  );
}

// ── Selection Pane ──────────────────────────────────────────────────────────────────────────────

function SelectionPaneDialog({ model }: { model: DeckEditorModel }) {
  const shapes = useDeck(model, (s) => s.deck?.shapes ?? []);
  const selection = useDeck(model, (s) => s.deck?.selection ?? []);
  return (
    <DeckDialogFrame model={model} title={t('Selection')} testId="deck-selection-pane" size="sm">
      <ul className="kiln-deckselection" aria-label={t('Shapes on this slide')}>
        {shapes.map((sh) => {
          const on = selection.includes(sh.id);
          return (
            <li key={sh.id} data-selected={on || undefined}>
              <button
                type="button"
                className="kiln-deckselection__name"
                aria-pressed={on}
                onClick={(e) =>
                  void model.core('deck.select.shapes', {
                    ids:
                      e.ctrlKey || e.metaKey
                        ? on
                          ? selection.filter((i) => i !== sh.id)
                          : [...selection, sh.id]
                        : [sh.id],
                  })
                }
              >
                {sh.name || sh.kind}
              </button>
              <span className="kiln-deckselection__kind">{sh.kind}</span>
              <Icon
                icon={sh.hidden ? EyeOff : Eye}
                size={14}
                aria-label={sh.hidden ? t('Hidden') : t('Shown')}
              />
            </li>
          );
        })}
      </ul>
      <div className="kiln-deckdlg__row">
        <Button onClick={() => void model.run('deck.arrange.order', { order: 'Forward' })}>
          {t('Bring Forward')}
        </Button>
        <Button onClick={() => void model.run('deck.arrange.order', { order: 'Backward' })}>
          {t('Send Backward')}
        </Button>
      </div>
    </DeckDialogFrame>
  );
}

// ── Shapes ──────────────────────────────────────────────────────────────────────────────────────

const CATEGORIES: readonly [string, string][] = [
  ['lines', msg('Lines')],
  ['basic', msg('Basic Shapes')],
  ['arrows', msg('Block Arrows')],
  ['math', msg('Equation Shapes')],
  ['flowchart', msg('Flowchart')],
  ['stars', msg('Stars and Banners')],
  ['callouts', msg('Callouts')],
  ['actions', msg('Action Buttons')],
];

/** A friendly name of a preset (`rightArrow` → "Right Arrow"). */
export function presetLabel(preset: string): string {
  return preset
    .replace(/([a-z])([A-Z0-9])/g, '$1 $2')
    .replace(/^./, (c) => c.toUpperCase())
    .replace(/\bRect\b/, 'Rectangle');
}

export function ShapesGallery({
  icons,
  onPick,
}: {
  icons: readonly DeckShapeIcon[];
  onPick: (preset: string) => void;
}) {
  return (
    <div className="kiln-deckshapes" data-testid="deck-shapes-gallery">
      {CATEGORIES.map(([cat, label]) => {
        const list = icons.filter((i) => i.category === cat);
        if (list.length === 0) return null;
        return (
          <section key={cat} className="kiln-deckshapes__group" aria-label={tx(label)}>
            <h3>{tx(label)}</h3>
            <div className="kiln-deckshapes__grid">
              {list.map((i) => (
                <button
                  key={i.preset}
                  type="button"
                  className="kiln-deckshapes__item"
                  title={i.label || presetLabel(i.preset)}
                  aria-label={i.label || presetLabel(i.preset)}
                  data-preset={i.preset}
                  onClick={() => onPick(i.preset)}
                >
                  <svg viewBox="-1 -1 22 22" width="22" height="22" aria-hidden="true">
                    <path d={i.path} />
                  </svg>
                </button>
              ))}
            </div>
          </section>
        );
      })}
    </div>
  );
}

function ShapesDialog({ model, change }: { model: DeckEditorModel; change: boolean }) {
  const [icons, setIcons] = useState<DeckShapeIcon[]>([]);
  useEffect(() => {
    let live = true;
    void model.shapeIcons().then((i) => {
      if (live) setIcons(i);
    });
    return () => {
      live = false;
    };
  }, [model]);
  const shown = useMemo(
    () => (change ? icons.filter((i) => i.category !== 'lines') : icons),
    [icons, change],
  );
  return (
    <DeckDialogFrame
      model={model}
      title={change ? t('Change Shape') : t('Shapes')}
      size="lg"
      testId="deck-shapes"
    >
      <ShapesGallery
        icons={shown}
        onPick={async (preset) => {
          model.closeDialog();
          if (change) await model.run('deck.drawing.change_shape', { preset });
          // Insert ▸ Shapes: the next drag on the slide draws it (a click places the default size).
          else await model.run('deck.stage.arm_insert', { preset });
        }}
      />
    </DeckDialogFrame>
  );
}

export function DeckDialogs({ model }: { model: DeckEditorModel }) {
  const dialog = useDeck(model, (s) => s.dialog);
  if (!dialog) return null;
  switch (dialog.id) {
    case 'slideSize':
      return <SlideSizeDialog model={model} />;
    case 'formatBackground':
      return <FormatBackgroundDialog model={model} />;
    case 'insertTable':
      return <InsertTableDialog model={model} />;
    case 'headerFooter':
      return <HeaderFooterDialog model={model} />;
    case 'sizePosition':
      return <SizePositionDialog model={model} />;
    case 'altText':
      return <AltTextDialog model={model} />;
    case 'link':
      return <LinkDialog model={model} />;
    case 'find':
      return <FindDialog model={model} replace={false} />;
    case 'replace':
      return <FindDialog model={model} replace />;
    case 'replaceFonts':
      return <ReplaceFontsDialog model={model} />;
    case 'renameSection':
      return <RenameSectionDialog model={model} index={dialog.arg ?? 0} />;
    case 'crop':
      return <CropDialog model={model} />;
    case 'pasteSpecial':
      return <PasteSpecialDialog model={model} />;
    case 'selectionPane':
      return <SelectionPaneDialog model={model} />;
    case 'shapes':
      return <ShapesDialog model={model} change={false} />;
    case 'changeShape':
      return <ShapesDialog model={model} change />;
    case 'font':
      return <FontDialog model={model} />;
    case 'paragraph':
      return <ParagraphDialog model={model} />;
    case 'symbol':
      return <SymbolDialog model={model} />;
    case 'setUpShow':
      return <SetUpShowDialog model={model} />;
    case 'equation':
      return <EquationDialog model={model} />;
    case 'insertChart':
      return <InsertChartDialog model={model} />;
    case 'chartData':
      return <ChartDataDialog model={model} />;
    case 'renameMaster':
      return <RenameMasterDialog model={model} />;
    case 'icons':
      return <IconsDialog model={model} />;
    case 'codeBlock':
      return <CodeBlockDialog model={model} />;
    case 'formatShape':
      return <FormatShapeDialog model={model} />;
    case 'splitCells':
      return <SplitCellsDialog model={model} />;
    case 'cellMargins':
      return <CellMarginsDialog model={model} />;
    default:
      return null;
  }
}
