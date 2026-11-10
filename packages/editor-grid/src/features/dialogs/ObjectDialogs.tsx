/**
 * Picture Format and Shape Format dialogs: Alt Text, Crop (percentages of each side), Size and
 * Properties (height, width, proportions, how the object follows the cells) and the text of a
 * shape. Each change is one `FormatObject` core command on the selected object.
 */
import { t } from '@kiln/i18n';
import type { GridObjectFormat, GridPlacement } from '@kiln/protocol';
import { useState } from 'react';
import type { GridFeatures } from '../model.ts';
import { Check, FeatureFrame, Field, Radios } from '../ui.tsx';

/** Points per sheet pixel at 100 % (96 px per inch, 72 pt per inch). */
export const PT_PER_PX = 0.75;

const drawing = (f: GridFeatures) => f.model.state.grid?.drawing ?? null;

const format = (f: GridFeatures, fmt: Partial<GridObjectFormat>) =>
  f.tryExec({
    FormatObject: {
      format: {
        name: null,
        alt_text: null,
        width: null,
        height: null,
        lock_aspect: null,
        crop: null,
        placement: null,
        preset: null,
        fill: null,
        line: null,
        line_width: null,
        text: null,
        text_color: null,
        text_size: null,
        ...fmt,
      },
    },
  });

function NumberField({
  label,
  value,
  onChange,
  unit,
  min = 0,
  max = 1584,
  testId,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
  unit: string;
  min?: number;
  max?: number;
  testId?: string;
}) {
  return (
    <Field label={label}>
      {(id) => (
        <span className="kiln-gfx__number">
          <input
            id={id}
            className="kiln-griddlg__input"
            type="number"
            min={min}
            max={max}
            step="any"
            value={Number.isFinite(value) ? Math.round(value * 100) / 100 : 0}
            data-testid={testId}
            onChange={(e) => onChange(Number(e.target.value))}
          />
          <span className="kiln-gfx__unit">{unit}</span>
        </span>
      )}
    </Field>
  );
}

export function AltTextDialog({ features }: { features: GridFeatures }) {
  const d = drawing(features);
  const [text, setText] = useState(d?.object?.alt_text ?? '');
  return (
    <FeatureFrame
      features={features}
      title={t('Alt Text')}
      testId="grid-alt-text"
      onOk={() => format(features, { alt_text: text })}
    >
      <p className="kiln-gfx__hint">
        {d?.kind === 'Picture'
          ? t('How would you describe this picture to someone who can’t see it?')
          : t('How would you describe this object to someone who can’t see it?')}
      </p>
      <Field label={t('Description')} wide>
        {(id) => (
          <textarea
            id={id}
            className="kiln-griddlg__input kiln-gfx__textarea"
            rows={4}
            value={text}
            // biome-ignore lint/a11y/noAutofocus: the dialog's only field takes the keys.
            autoFocus
            data-testid="grid-alt-text-input"
            onChange={(e) => setText(e.target.value)}
          />
        )}
      </Field>
    </FeatureFrame>
  );
}

export function CropDialog({ features }: { features: GridFeatures }) {
  const d = drawing(features);
  const [crop, setCrop] = useState<[number, number, number, number]>(
    (d?.object?.crop as [number, number, number, number] | undefined) ?? [0, 0, 0, 0],
  );
  const side = (i: number, label: string) => (
    <NumberField
      label={label}
      unit="%"
      max={99}
      value={crop[i] ?? 0}
      testId={`grid-crop-${label.toLowerCase()}`}
      onChange={(v) => {
        const next = [...crop] as [number, number, number, number];
        next[i] = Math.max(0, v);
        setCrop(next);
      }}
    />
  );
  return (
    <FeatureFrame
      features={features}
      title={t('Crop')}
      testId="grid-crop"
      onOk={() => format(features, { crop })}
    >
      <p className="kiln-gfx__hint">{t('How much of each side of the picture to hide.')}</p>
      <div className="kiln-gfx__row">
        {side(0, 'Left')}
        {side(2, 'Right')}
      </div>
      <div className="kiln-gfx__row">
        {side(1, 'Top')}
        {side(3, 'Bottom')}
      </div>
    </FeatureFrame>
  );
}

export function FormatObjectDialog({ features }: { features: GridFeatures }) {
  const d = drawing(features);
  const o = d?.object;
  const [h, setH] = useState((d?.height ?? 0) * PT_PER_PX);
  const [w, setW] = useState((d?.width ?? 0) * PT_PER_PX);
  const [lock, setLock] = useState(o?.lock_aspect ?? false);
  const [placement, setPlacement] = useState<GridPlacement>(o?.placement ?? 'MoveAndSize');
  const [name, setName] = useState(d?.name ?? '');
  const ratio = d && d.height > 0 ? d.width / d.height : 1;
  return (
    <FeatureFrame
      features={features}
      title={d?.kind === 'Picture' ? t('Format Picture') : t('Format Shape')}
      testId="grid-format-object"
      onOk={() =>
        format(features, {
          name,
          height: h / PT_PER_PX,
          width: w / PT_PER_PX,
          lock_aspect: lock,
          placement,
        })
      }
    >
      <Field label={t('Name')} wide>
        {(id) => (
          <input
            id={id}
            className="kiln-griddlg__input"
            value={name}
            data-testid="grid-object-name"
            onChange={(e) => setName(e.target.value)}
          />
        )}
      </Field>
      <div className="kiln-gfx__row">
        <NumberField
          label={t('Height')}
          unit="pt"
          value={h}
          testId="grid-object-height"
          onChange={(v) => {
            setH(v);
            if (lock) setW(v * ratio);
          }}
        />
        <NumberField
          label={t('Width')}
          unit="pt"
          value={w}
          testId="grid-object-width"
          onChange={(v) => {
            setW(v);
            if (lock) setH(v / ratio);
          }}
        />
      </div>
      <Check label={t('Lock aspect ratio')} checked={lock} onChange={setLock} testId="grid-object-lock" />
      {o && o.natural_width > 0 ? (
        <p className="kiln-gfx__hint">
          {t('Original size: {height, number} pt × {width, number} pt', {
            height: Math.round(o.natural_height * PT_PER_PX),
            width: Math.round(o.natural_width * PT_PER_PX),
          })}
        </p>
      ) : null}
      <Radios<GridPlacement>
        label={t('Properties')}
        value={placement}
        options={[
          ['MoveAndSize', t('Move and size with cells')],
          ['Move', t('Move but don’t size with cells')],
          ['Fixed', t('Don’t move or size with cells')],
        ]}
        onChange={setPlacement}
      />
    </FeatureFrame>
  );
}

export function ShapeTextDialog({ features, start }: { features: GridFeatures; start?: string }) {
  const d = drawing(features);
  const [text, setText] = useState(start ?? d?.object?.text ?? '');
  const [size, setSize] = useState(d?.object?.text_size ?? 11);
  return (
    <FeatureFrame
      features={features}
      title={t('Edit Text')}
      testId="grid-shape-text"
      onOk={() => format(features, { text, text_size: size })}
    >
      <Field label={t('Text')} wide>
        {(id) => (
          <textarea
            id={id}
            className="kiln-griddlg__input kiln-gfx__textarea"
            rows={4}
            value={text}
            // biome-ignore lint/a11y/noAutofocus: typing into a shape goes straight to its text.
            autoFocus
            data-testid="grid-shape-text-input"
            onFocus={(e) => {
              const n = e.currentTarget.value.length;
              e.currentTarget.setSelectionRange(n, n);
            }}
            onKeyDown={(e) => {
              // Enter starts a new paragraph, as in a shape; Ctrl+Enter is OK.
              if (e.key === 'Enter' && !e.ctrlKey) e.stopPropagation();
            }}
            onChange={(e) => setText(e.target.value)}
          />
        )}
      </Field>
      <NumberField label={t('Size')} unit="pt" min={1} max={409} value={size} onChange={setSize} />
    </FeatureFrame>
  );
}
