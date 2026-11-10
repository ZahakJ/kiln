/**
 * Function Arguments (after Insert Function, a Function Library menu or Recently Used): a box for
 * each argument with its value as the core computes it, the description of the argument being
 * typed, the formula's result, and OK to put the function in the cell (or into the formula being
 * edited, at the caret).
 */
import { t } from '@kiln/i18n';
import type { GridFunctionDoc } from '@kiln/protocol';
import { errorText } from '@kiln/ui-kit';
import { useEffect, useMemo, useRef, useState } from 'react';
import type { GridFeatures } from '../model.ts';
import { noteRecentFunction } from '../model.ts';
import { FeatureFrame } from '../ui.tsx';

/** Asks the core for a formula's value at the active cell. */
async function evaluate(features: GridFeatures, formula: string): Promise<string> {
  const r = await features.ask({ Evaluate: { formula } });
  return r && 'Evaluated' in r ? r.Evaluated : '';
}

/** The value of each box, refreshed as the user types (a short pause between keys). */
function useValues(features: GridFeatures, texts: readonly string[]): string[] {
  const [values, setValues] = useState<string[]>([]);
  const key = texts.join('\u0001');
  useEffect(() => {
    let live = true;
    const t = setTimeout(async () => {
      const out = await Promise.all(texts.map((x) => (x.trim() ? evaluate(features, `=${x}`) : '')));
      if (live) setValues(out);
    }, 120);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [features, key]);
  return values;
}

export function FunctionArgsDialog({ features, name }: { features: GridFeatures; name: string }) {
  const [doc, setDoc] = useState<GridFunctionDoc | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [args, setArgs] = useState<string[]>([]);
  const [focus, setFocus] = useState(0);
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => {
    void features.ask({ FunctionDoc: { name } }).then(
      (r) => {
        if (r && 'FunctionDoc' in r) setDoc(r.FunctionDoc);
      },
      (e: unknown) => setError(errorText(e)),
    );
  }, [features, name]);
  useEffect(() => {
    if (doc) first.current?.focus();
  }, [doc]);
  // The boxes: every argument, and one more of a repeating argument after the last filled.
  const boxes = useMemo(() => {
    if (!doc) return [];
    const out: { label: string; description: string; optional: boolean }[] = [];
    for (const a of doc.args) {
      if (!a.repeat) {
        out.push({ label: a.name, description: a.description, optional: a.optional });
        continue;
      }
      const stem = a.name.replace(/\d+$/, '');
      const n0 = Number(a.name.match(/\d+$/)?.[0] ?? 1);
      const start = out.length;
      const filled = args.slice(start).reduce((n, t, i) => (t.trim() ? i + 1 : n), 0);
      const count = Math.min(255, Math.max(2, filled + 1));
      for (let i = 0; i < count; i++)
        out.push({ label: `${stem}${n0 + i}`, description: a.description, optional: i > 0 || a.optional });
      break;
    }
    return out;
  }, [doc, args]);
  const used = (() => {
    let last = 0;
    args.forEach((t, i) => {
      if (t.trim() && i < boxes.length) last = i + 1;
    });
    return args.slice(0, last);
  })();
  const formula = `=${name}(${used.join(',')})`;
  const values = useValues(
    features,
    boxes.map((_, i) => args[i] ?? ''),
  );
  const [result] = useValues(features, [formula.slice(1)]);
  const ok = async () => {
    noteRecentFunction(name);
    const m = features.model;
    const ed = m.state.grid?.editor;
    if (ed?.text.startsWith('=')) {
      const call = formula.slice(1);
      const text = `${ed.text.slice(0, ed.start)}${call}${ed.text.slice(ed.end)}`;
      const at = ed.start + call.length;
      await m.edit('grid.editing.set_text', { text, start: at, end: at, formula_bar: ed.formula_bar });
      return;
    }
    await m.core('grid.editing.begin', { text: formula });
    await m.core('grid.editing.commit', { advance: null });
  };
  const current = boxes[focus];
  return (
    <FeatureFrame
      features={features}
      title={t('Function Arguments')}
      size="md"
      testId="grid-function-args"
      okDisabled={!doc}
      onOk={ok}
    >
      {error ? (
        <p className="kiln-gfx__error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="kiln-gfx__fn-name">{name}</div>
      <div className="kiln-gfx__fn-args">
        {boxes.map((b, i) => (
          <label key={b.label} className="kiln-gfx__fn-arg">
            <span
              className={b.optional ? 'kiln-gfx__fn-label' : 'kiln-gfx__fn-label kiln-gfx__fn-label--needed'}
            >
              {b.label}
            </span>
            <input
              ref={i === 0 ? first : undefined}
              className="kiln-griddlg__input"
              value={args[i] ?? ''}
              data-testid={`grid-fn-arg-${i}`}
              onFocus={() => setFocus(i)}
              onChange={(e) => {
                const next = [...args];
                while (next.length <= i) next.push('');
                next[i] = e.target.value;
                setArgs(next);
              }}
            />
            <span className="kiln-gfx__fn-value" data-testid={`grid-fn-value-${i}`}>
              {values[i] ? `= ${values[i]}` : ''}
            </span>
          </label>
        ))}
      </div>
      {doc ? <p className="kiln-gfx__hint">{doc.summary}</p> : null}
      {current ? (
        <p className="kiln-gfx__hint">
          <strong>{current.label}</strong> {current.description}
        </p>
      ) : null}
      <p className="kiln-gfx__fn-result" data-testid="grid-fn-result">
        {t('Formula result =')} {result && !result.startsWith('Invalid') ? result : ''}
      </p>
    </FeatureFrame>
  );
}
