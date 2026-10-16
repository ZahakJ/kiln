import {
  type AppId,
  chordId,
  chordToShortcut,
  eventToChord,
  formatChord,
  isBareModifier,
  isBrowserReservedChord,
  type KeyChord,
  parseShortcut,
} from '@kiln/commands';
import { RotateCcw, Search, X } from 'lucide-react';
import { type KeyboardEvent, useId, useMemo, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from '../primitives/cx.ts';
import { useCommands, useKeymapRevision } from './CommandContext.tsx';
import { CommandIcon } from './commandIcons.tsx';

const APP_NAMES: Partial<Record<AppId, string>> = {
  write: 'Kiln Write',
  grid: 'Kiln Grid',
  deck: 'Kiln Deck',
  notes: 'Kiln Notes',
};

/**
 * Options ▸ Keyboard ★: every command with its shortcuts, searchable; select one, press a new
 * shortcut and Assign (Office's Customize Keyboard, live). Remapping persists through the keymap.
 */
export function KeyboardOptions() {
  const { registry, keymap, app: currentApp, platform } = useCommands();
  useKeymapRevision();
  const [app, setApp] = useState<AppId>(currentApp === 'hub' ? 'write' : currentApp);
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [captured, setCaptured] = useState<KeyChord | null>(null);
  const [message, setMessage] = useState('');
  const captureId = useId();

  const commands = useMemo(() => {
    const ctx = { ...registry.context, app };
    const q = query.trim().toLowerCase();
    return registry
      .list({ visibleOnly: true, context: ctx })
      .filter((c) => c.palette !== false || (keymap?.shortcutsFor(c.id, app).length ?? 0) > 0)
      .filter(
        (c) =>
          !q ||
          c.title.toLowerCase().includes(q) ||
          (c.officeName ?? '').toLowerCase().includes(q) ||
          c.id.toLowerCase().includes(q) ||
          (keymap?.shortcutsFor(c.id, app) ?? []).some((k) =>
            formatChord(parseShortcut(k, platform), platform).toLowerCase().includes(q),
          ),
      )
      .sort((a, b) => (a.category ?? '').localeCompare(b.category ?? '') || a.title.localeCompare(b.title));
  }, [registry, keymap, app, query, platform]);

  if (!keymap) return <p>Keyboard shortcuts are not available here.</p>;

  const label = (k: string) => formatChord(parseShortcut(k, platform), platform);
  const selectedMeta = selected ? registry.get(selected) : undefined;
  const conflicts = captured ? keymap.commandsFor(captured, app).filter((c) => c !== selected) : [];

  const onCapture = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Tab' && !e.ctrlKey && !e.altKey && !e.metaKey) return; // keep Tab for focus movement
    e.preventDefault();
    e.stopPropagation();
    const chord = eventToChord(e);
    if (isBareModifier(chord)) return;
    if (chord.key === 'escape' && !chord.ctrl && !chord.alt && !chord.meta && !chord.shift) {
      setCaptured(null);
      return;
    }
    if (chord.key === 'backspace' && !chord.ctrl && !chord.alt && !chord.meta) {
      setCaptured(null);
      return;
    }
    setCaptured(chord);
    setMessage('');
  };

  const assign = () => {
    if (!selected || !captured) return;
    const key = chordToShortcut(captured, platform);
    const losers = keymap.assign(selected, key, app);
    const name = registry.get(selected)?.title ?? selected;
    setMessage(
      losers.length
        ? `${label(key)} now runs ${name}; removed from ${losers.map((l) => registry.get(l)?.title ?? l).join(', ')}.`
        : `${label(key)} now runs ${name}.`,
    );
    setCaptured(null);
  };

  return (
    <div className="kiln-options-keyboard">
      <div className="kiln-options-keyboard__bar">
        <label className="kiln-options__field">
          <span>Shortcuts for</span>
          <select
            className="kiln-options__select"
            value={app}
            onChange={(e) => setApp(e.target.value as AppId)}
          >
            {(Object.keys(APP_NAMES) as AppId[]).map((a) => (
              <option key={a} value={a}>
                {APP_NAMES[a]}
              </option>
            ))}
          </select>
        </label>
        <div className="kiln-options-keyboard__search">
          <Icon icon={Search} size={14} />
          <input
            type="search"
            aria-label="Search commands or shortcuts"
            placeholder="Search commands or shortcuts"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <button
          type="button"
          className="kiln-btn kiln-btn--sm"
          onClick={() => {
            keymap.resetAll();
            setMessage('All shortcuts restored to the Office defaults.');
          }}
        >
          <span className="kiln-btn__icon">
            <Icon icon={RotateCcw} size={14} />
          </span>
          <span className="kiln-btn__label">Reset All</span>
        </button>
      </div>
      <div className="kiln-options-keyboard__main">
        <div className="kiln-options-keyboard__list" role="listbox" aria-label="Commands" tabIndex={-1}>
          {commands.map((c) => {
            const keys = keymap.shortcutsFor(c.id, app);
            return (
              <div
                key={c.id}
                role="option"
                aria-selected={selected === c.id}
                tabIndex={selected === c.id || (!selected && c === commands[0]) ? 0 : -1}
                className={cx('kiln-options-keyboard__row', selected === c.id && 'is-selected')}
                onClick={() => {
                  setSelected(c.id);
                  setCaptured(null);
                }}
                onKeyDown={(e) => {
                  const i = commands.indexOf(c);
                  const next =
                    e.key === 'ArrowDown'
                      ? commands[i + 1]
                      : e.key === 'ArrowUp'
                        ? commands[i - 1]
                        : undefined;
                  if (next) {
                    e.preventDefault();
                    setSelected(next.id);
                    (
                      e.currentTarget.parentElement?.querySelector(
                        `[data-command-row="${CSS.escape(next.id)}"]`,
                      ) as HTMLElement | null
                    )?.focus();
                  } else if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    setSelected(c.id);
                    requestAnimationFrame(() => document.getElementById(captureId)?.focus());
                  }
                }}
                data-command-row={c.id}
              >
                <CommandIcon name={c.icon} size={16} />
                <span className="kiln-options-keyboard__title">
                  {c.title}
                  {c.pending ? <span className="kiln-options-keyboard__pending"> (coming soon)</span> : null}
                </span>
                <span className="kiln-options-keyboard__keys">
                  {keys.map((k) => (
                    <kbd key={k} className="kiln-options-keyboard__key">
                      {label(k)}
                    </kbd>
                  ))}
                  {keymap.isCustomized(c.id, app) ? (
                    <span className="kiln-options-keyboard__custom">Custom</span>
                  ) : null}
                </span>
              </div>
            );
          })}
          {commands.length === 0 ? <p className="kiln-options__hint">No commands match “{query}”.</p> : null}
        </div>
        <div className="kiln-options-keyboard__editor" aria-live="polite">
          {selectedMeta ? (
            <>
              <h3>{selectedMeta.title}</h3>
              {selectedMeta.officeName && selectedMeta.officeName !== selectedMeta.title ? (
                <p className="kiln-options__hint">In Microsoft Office: {selectedMeta.officeName}</p>
              ) : null}
              {selectedMeta.description ? (
                <p className="kiln-options__hint">{selectedMeta.description}</p>
              ) : null}
              <div className="kiln-options__label">Current shortcuts</div>
              <div className="kiln-options-keyboard__current">
                {keymap.shortcutsFor(selectedMeta.id, app).length === 0 ? (
                  <span className="kiln-options__hint">None</span>
                ) : null}
                {keymap.shortcutsFor(selectedMeta.id, app).map((k) => (
                  <span key={k} className="kiln-options-keyboard__chip">
                    {label(k)}
                    <button
                      type="button"
                      aria-label={`Remove ${label(k)}`}
                      onClick={() => {
                        keymap.unassign(selectedMeta.id, k, app);
                        setMessage(`${label(k)} removed.`);
                      }}
                    >
                      <Icon icon={X} size={12} />
                    </button>
                  </span>
                ))}
              </div>
              <label className="kiln-options__label" htmlFor={captureId}>
                Press new shortcut
              </label>
              <input
                id={captureId}
                className="kiln-options-keyboard__capture"
                readOnly
                value={captured ? formatChord(captured, platform) : ''}
                placeholder="Click here, then press keys"
                onKeyDown={onCapture}
              />
              {captured ? (
                <p className={cx('kiln-options__hint', conflicts.length > 0 && 'kiln-options__warn')}>
                  {conflicts.length
                    ? `Currently assigned to: ${conflicts.map((c) => registry.get(c)?.title ?? c).join(', ')}`
                    : 'Currently unassigned'}
                  {isBrowserReservedChord(captured, platform)
                    ? ' · In a browser tab this shortcut may be taken by the browser; it works in the desktop app.'
                    : ''}
                </p>
              ) : null}
              <div className="kiln-options-keyboard__actions">
                <button
                  type="button"
                  className="kiln-btn kiln-btn--primary kiln-btn--sm"
                  disabled={!captured || chordId(captured) === ''}
                  onClick={assign}
                >
                  Assign
                </button>
                <button
                  type="button"
                  className="kiln-btn kiln-btn--sm"
                  disabled={!keymap.isCustomized(selectedMeta.id, app)}
                  onClick={() => {
                    keymap.resetCommand(selectedMeta.id, app);
                    keymap.resetCommand(selectedMeta.id, 'common');
                    setMessage(`${selectedMeta.title}: default shortcuts restored.`);
                  }}
                >
                  Reset
                </button>
              </div>
            </>
          ) : (
            <p className="kiln-options__hint">Select a command to see or change its shortcuts.</p>
          )}
          {message ? <p className="kiln-options-keyboard__message">{message}</p> : null}
        </div>
      </div>
    </div>
  );
}
