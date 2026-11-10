/**
 * The Selection pane (Layout ▸ Arrange ▸ Selection Pane): the document's pictures, shapes and
 * text boxes, front first. Click one to select it (Shift or Ctrl+click adds it to the selection);
 * double-click its name to rename it; the eye hides or shows it; the arrows bring it forward or
 * send it backward. Show All and Hide All at the top, as in Word.
 */
import { t } from '@kiln/i18n';
import type { ObjectInfo } from '@kiln/protocol';
import { Button, Icon } from '@kiln/ui-kit';
import { ChevronDown, ChevronUp, Eye, EyeOff } from 'lucide-react';
import { type KeyboardEvent, useState } from 'react';
import { useStore } from 'zustand';
import { featuresOf } from '../features/model.ts';
import type { WriteEditorModel } from '../model.ts';

const KIND: Record<string, string> = {
  Picture: 'Picture',
  Shape: 'Shape',
  TextBox: 'Text box',
  Chart: 'Chart',
  Other: 'Object',
};

function Row({ model, o }: { model: WriteEditorModel; o: ObjectInfo }) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(o.name);
  const commit = async () => {
    setEditing(false);
    const n = name.trim();
    if (n && n !== o.name) await model.run('write.arrange.layout', { id: o.id, spec: { name: n } });
    else setName(o.name);
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation();
    if (e.key === 'Enter') void commit();
    if (e.key === 'Escape') {
      setName(o.name);
      setEditing(false);
    }
  };
  const order = async (forward: boolean) => {
    await model.run('write.arrange.select', { id: o.id });
    await model.run('write.arrange.order', { order: forward ? 'Forward' : 'Backward' });
  };
  return (
    <li
      className={`kiln-selection-pane__item${o.selected || o.in_selection ? ' is-selected' : ''}${o.hidden ? ' is-hidden' : ''}`}
      data-testid="write-selection-item"
    >
      {editing ? (
        <input
          className="kiln-write-input kiln-selection-pane__rename"
          aria-label={t('Object name')}
          value={name}
          // biome-ignore lint/a11y/noAutofocus: renaming starts in the box, as in Office
          autoFocus
          onChange={(e) => setName(e.target.value)}
          onBlur={() => void commit()}
          onKeyDown={onKey}
        />
      ) : (
        <button
          type="button"
          className="kiln-selection-pane__name"
          aria-pressed={o.selected || o.in_selection}
          onClick={(e) =>
            void model.run('write.arrange.select', {
              id: o.id,
              ...(e.shiftKey || e.ctrlKey || e.metaKey ? { add: true } : {}),
            })
          }
          onDoubleClick={() => setEditing(true)}
          title={t('Click to select, double-click to rename')}
        >
          <span>{o.name}</span>
          <span className="kiln-selection-pane__kind">
            {KIND[o.kind] ?? t('Object')}
            {o.wrap === 'InLine' ? t(' · in line') : ''}
          </span>
        </button>
      )}
      <button
        type="button"
        className="kiln-selection-pane__eye"
        aria-label={o.hidden ? t('Show {name}', { name: o.name }) : t('Hide {name}', { name: o.name })}
        aria-pressed={o.hidden}
        title={o.hidden ? t('Show') : t('Hide')}
        onClick={() => void model.run('write.arrange.visible', { id: o.id, visible: o.hidden })}
      >
        <Icon icon={o.hidden ? EyeOff : Eye} size={15} />
      </button>
      <span className="kiln-selection-pane__order">
        <button
          type="button"
          aria-label={t('Bring {name} forward', { name: o.name })}
          disabled={o.wrap === 'InLine'}
          onClick={() => void order(true)}
        >
          <Icon icon={ChevronUp} size={14} />
        </button>
        <button
          type="button"
          aria-label={t('Send {name} backward', { name: o.name })}
          disabled={o.wrap === 'InLine'}
          onClick={() => void order(false)}
        >
          <Icon icon={ChevronDown} size={14} />
        </button>
      </span>
    </li>
  );
}

export function SelectionPane({ model }: { model: WriteEditorModel }) {
  const features = featuresOf(model);
  const objects = useStore(features.store, (s) => s.objects);
  if (!objects) return <p className="kiln-comments-pane__empty">{t('Reading the objects…')}</p>;
  if (objects.length === 0)
    return (
      <p className="kiln-comments-pane__empty" data-testid="write-selection-pane">
        {t('This document has no pictures, shapes or text boxes.')}
      </p>
    );
  return (
    <div data-testid="write-selection-pane">
      <div className="kiln-selection-pane__all">
        <Button
          size="sm"
          onClick={() => void model.run('write.arrange.visible', { id: null, visible: true })}
        >
          {t('Show All')}
        </Button>
        <Button
          size="sm"
          onClick={() => void model.run('write.arrange.visible', { id: null, visible: false })}
        >
          {t('Hide All')}
        </Button>
      </div>
      <ul className="kiln-selection-pane" aria-label={t('Objects, front first')}>
        {objects.map((o) => (
          <Row key={o.id} model={model} o={o} />
        ))}
      </ul>
    </div>
  );
}
