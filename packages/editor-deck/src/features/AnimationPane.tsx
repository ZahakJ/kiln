/**
 * The Animation Pane (Animations ▸ Animation Pane): the current slide's effects in order, numbered
 * by the click they run on, with their class colour, start, the shape they animate and a timeline
 * bar; select (Ctrl and Shift add), drag or Move Earlier / Later to reorder, Start, Duration and
 * Delay, Remove, and Play. Everything shown is the core's `DeckMotionState`; every change is a
 * core command.
 */
import { t } from '@kiln/i18n';
import type { DeckAnimationInfo, DeckEffectStart } from '@kiln/protocol';
import { Button, Icon, NumberInput, Select } from '@kiln/ui-kit';
import { ChevronDown, ChevronUp, Clock, MousePointerClick, Play, Trash2, X } from 'lucide-react';
import { useMemo, useRef, useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';

const START_LABEL: Record<DeckEffectStart, string> = {
  OnClick: 'On Click',
  WithPrevious: 'With Previous',
  AfterPrevious: 'After Previous',
};

/** The name a row shows: the shape, and the paragraph when the effect builds one. */
export function rowName(a: DeckAnimationInfo): string {
  const base = a.shape_name || 'Shape';
  if (!a.paragraphs) return base;
  const [first, last] = a.paragraphs;
  return first === last ? `${base}: paragraph ${first + 1}` : `${base}: paragraphs ${first + 1}–${last + 1}`;
}

/** The ids selected by a click on row `i` with Ctrl or Shift. */
export function clickSelection(
  rows: readonly DeckAnimationInfo[],
  selected: readonly number[],
  i: number,
  mods: { ctrl: boolean; shift: boolean },
  anchor: number,
): number[] {
  const id = rows[i]?.id;
  if (id === undefined) return [...selected];
  if (mods.shift) {
    const [a, b] = anchor <= i ? [anchor, i] : [i, anchor];
    return rows.slice(Math.max(0, a), b + 1).map((r) => r.id);
  }
  if (mods.ctrl) return selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
  return [id];
}

export function AnimationPane({ model }: { model: DeckEditorModel }) {
  const motion = useDeck(model, (s) => s.deck?.motion ?? null);
  const rows = motion?.animations ?? [];
  const selected = motion?.selected ?? [];
  const anchor = useRef(0);
  const [drag, setDrag] = useState<{ from: number; over: number } | null>(null);
  const total = useMemo(() => Math.max(1000, ...rows.map((r) => r.end_ms)), [rows]);
  const first = rows.find((r) => r.id === selected[0]) ?? null;
  const select = (ids: number[]) => void model.core('deck.animations.select', { ids });
  const close = () => {
    model.set({ animPane: false });
    void model.core('deck.animations.badges', { on: model.state.tab === 'animations' });
    model.stage?.focus();
  };
  const move = (id: number, to: number) => void model.core('deck.animations.move', { id, to });
  const onKey = (e: React.KeyboardEvent) => {
    const i = rows.findIndex((r) => r.id === selected[0]);
    if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      void model.core('deck.animations.remove');
    } else if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && e.altKey) {
      e.preventDefault();
      void model.core('deck.animations.reorder', { earlier: e.key === 'ArrowUp' });
    } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault();
      const next = Math.max(0, Math.min(rows.length - 1, i + (e.key === 'ArrowUp' ? -1 : 1)));
      const r = rows[next];
      if (r) {
        anchor.current = next;
        select([r.id]);
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      model.stage?.focus();
    }
  };
  let lastClick = -1;
  return (
    <aside
      className="kiln-animpane"
      aria-label={t('Animation Pane')}
      data-testid="deck-animation-pane"
      data-kiln-shortcuts="off"
    >
      <div className="kiln-animpane__head">
        <h2>{t('Animation Pane')}</h2>
        <button
          type="button"
          className="kiln-animpane__close"
          aria-label={t('Close the Animation Pane')}
          onClick={close}
        >
          <Icon icon={X} size={14} />
        </button>
      </div>
      <div className="kiln-animpane__play">
        <Button
          size="sm"
          onClick={() => model.set({ preview: 'animations' })}
          disabled={rows.length === 0}
          data-testid="deck-animation-play"
        >
          <Icon icon={Play} size={14} /> {first ? t('Play From') : t('Play All')}
        </Button>
      </div>
      {rows.length === 0 ? (
        <p className="kiln-animpane__empty">
          {t('Select a shape, then choose an effect in Animations ▸ Add Animation.')}
        </p>
      ) : (
        <div
          className="kiln-animpane__list"
          role="listbox"
          aria-label={t('Effects')}
          aria-multiselectable="true"
          tabIndex={0}
          onKeyDown={onKey}
        >
          {rows.map((r, i) => {
            const showNumber = r.click !== lastClick;
            lastClick = r.click;
            const left = (r.begin_ms / total) * 100;
            const width = Math.max(2, ((r.end_ms - r.begin_ms) / total) * 100);
            return (
              <div
                key={r.id}
                role="option"
                tabIndex={-1}
                aria-selected={selected.includes(r.id)}
                className="kiln-animpane__row"
                data-class={r.class}
                data-drop={
                  drag && drag.over === i && drag.from !== i
                    ? drag.from < i
                      ? 'after'
                      : 'before'
                    : undefined
                }
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.effectAllowed = 'move';
                  e.dataTransfer.setData('text/plain', String(r.id));
                  setDrag({ from: i, over: i });
                }}
                onDragOver={(e) => {
                  if (!drag) return;
                  e.preventDefault();
                  if (drag.over !== i) setDrag({ ...drag, over: i });
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  if (drag && drag.from !== i) {
                    const moving = rows[drag.from];
                    if (moving) move(moving.id, i);
                  }
                  setDrag(null);
                }}
                onDragEnd={() => setDrag(null)}
                onKeyDown={(e) => {
                  if (e.key !== 'Enter' && e.key !== ' ') return;
                  e.preventDefault();
                  anchor.current = i;
                  select([r.id]);
                }}
                onClick={(e) => {
                  const ids = clickSelection(
                    rows,
                    selected,
                    i,
                    { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey },
                    anchor.current,
                  );
                  if (!e.shiftKey) anchor.current = i;
                  select(ids);
                }}
              >
                <span className="kiln-animpane__num">{showNumber ? r.click : ''}</span>
                <span className="kiln-animpane__start" aria-hidden>
                  {r.start === 'OnClick' ? (
                    <Icon icon={MousePointerClick} size={12} />
                  ) : r.start === 'AfterPrevious' ? (
                    <Icon icon={Clock} size={12} />
                  ) : null}
                </span>
                <span className="kiln-animpane__star" aria-hidden />
                <span className="kiln-animpane__name">
                  {rowName(r)}
                  <span className="kiln-animpane__effect">
                    {r.playable ? r.name : t('{name} (kept)', { name: r.name })}
                  </span>
                </span>
                <span className="kiln-animpane__bar" aria-hidden>
                  <span style={{ left: `${left}%`, width: `${width}%` }} />
                </span>
              </div>
            );
          })}
        </div>
      )}
      {first ? (
        <div className="kiln-animpane__timing">
          <div className="kiln-animpane__field">
            <span>{t('Start')}</span>
            <Select
              aria-label={t('Start')}
              value={first.start}
              onValueChange={(v) => void model.core('deck.animations.start', { start: v })}
              options={(['OnClick', 'WithPrevious', 'AfterPrevious'] as const).map((v) => ({
                value: v,
                label: START_LABEL[v],
              }))}
            />
          </div>
          <div className="kiln-animpane__field">
            <span>{t('Duration')}</span>
            <NumberInput
              aria-label={t('Duration in seconds')}
              value={first.duration_ms / 1000}
              min={0.01}
              max={59}
              step={0.25}
              onValueChange={(v) => {
                if (Number.isFinite(v))
                  void model.core('deck.animations.duration', { ms: Math.round(v * 1000) });
              }}
            />
          </div>
          <div className="kiln-animpane__field">
            <span>{t('Delay')}</span>
            <NumberInput
              aria-label={t('Delay in seconds')}
              value={first.delay_ms / 1000}
              min={0}
              max={59}
              step={0.25}
              onValueChange={(v) => {
                if (Number.isFinite(v))
                  void model.core('deck.animations.delay', { ms: Math.round(v * 1000) });
              }}
            />
          </div>
          <div className="kiln-animpane__field">
            <span>{t('Repeat')}</span>
            {/* PowerPoint's Timing ▸ Repeat: how many times the effect plays. */}
            <NumberInput
              aria-label={t('Repeat (times)')}
              value={first.repeat || 1}
              min={1}
              max={99}
              step={1}
              precision={0}
              onValueChange={(v) => {
                if (Number.isFinite(v) && v >= 1)
                  void model.core('deck.animations.repeat', { count: Math.round(v) });
              }}
            />
          </div>
          <div className="kiln-animpane__actions">
            <button
              type="button"
              className="kiln-animpane__icon"
              aria-label={t('Move Earlier')}
              title={t('Move Earlier')}
              onClick={() => void model.core('deck.animations.reorder', { earlier: true })}
            >
              <Icon icon={ChevronUp} size={14} />
            </button>
            <button
              type="button"
              className="kiln-animpane__icon"
              aria-label={t('Move Later')}
              title={t('Move Later')}
              onClick={() => void model.core('deck.animations.reorder', { earlier: false })}
            >
              <Icon icon={ChevronDown} size={14} />
            </button>
            <button
              type="button"
              className="kiln-animpane__icon"
              aria-label={t('Remove')}
              title={t('Remove')}
              onClick={() => void model.core('deck.animations.remove')}
            >
              <Icon icon={Trash2} size={14} />
            </button>
          </div>
        </div>
      ) : null}
    </aside>
  );
}
