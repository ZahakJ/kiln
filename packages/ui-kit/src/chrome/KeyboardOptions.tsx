import {
  type AppId,
  chordId,
  chordToShortcut,
  eventToChord,
  formatChord,
  isBareModifier,
  isBrowserReservedChord,
  isReleasedApp,
  type KeyChord,
  parseShortcut,
} from '@kiln/commands';
import { compareText, formatList, t } from '@kiln/i18n';
import { useLanguage } from '@kiln/i18n/react';
import { Download, RotateCcw, Search, TriangleAlert, Upload, X } from 'lucide-react';
import { type KeyboardEvent, useId, useMemo, useRef, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import {
  changedCommands,
  KEYBOARD_FILE_NAME,
  keyboardFile,
  parseKeyboardFile,
} from '../options/keyboardFile.ts';
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
  const [problem, setProblem] = useState('');
  const captureId = useId();
  const fileRef = useRef<HTMLInputElement>(null);

  const language = useLanguage();
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
      .sort((a, b) => compareText(a.category ?? '', b.category ?? '') || compareText(a.title, b.title));
  }, [registry, keymap, app, query, platform, language]);

  if (!keymap) return <p>{t('Keyboard shortcuts are not available here.')}</p>;

  // Two commands on one key, where the user's remapping put them (Office's "Currently assigned
  // to" after the fact): listed above the commands with a way to each.
  const clashes = (() => {
    const groups = new Map<string, { label: string; commands: string[]; user: boolean }>();
    for (const b of keymap.bindings(app)) {
      const k = `${b.scope ?? ''}|${chordId(b.chord)}`;
      const g = groups.get(k) ?? { label: formatChord(b.chord, platform), commands: [], user: false };
      if (!g.commands.includes(b.command)) g.commands.push(b.command);
      g.user ||= b.source === 'user';
      groups.set(k, g);
    }
    return [...groups.values()].filter((g) => g.user && g.commands.length > 1);
  })();

  const exportFile = () => {
    const blob = new Blob([keyboardFile(keymap.overrides)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = KEYBOARD_FILE_NAME;
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1_000);
    setProblem('');
    setMessage(t('Your shortcuts were saved as {name}.', { name: KEYBOARD_FILE_NAME }));
  };

  const importFile = async (file: File | undefined) => {
    if (!file) return;
    const overrides = parseKeyboardFile(await file.text());
    if (!overrides) {
      setMessage('');
      setProblem(
        t('{name} is not a Kiln keyboard file. Export one from Options ▸ Keyboard, then import that file.', {
          name: file.name,
        }),
      );
      return;
    }
    keymap.importOverrides(overrides);
    setProblem('');
    setMessage(
      t('Shortcuts imported: {count, plural, one {# command} other {# commands}} changed.', {
        count: changedCommands(keymap.overrides),
      }),
    );
  };

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
        ? t('{key} now runs {name}; removed from {others}.', {
            key: label(key),
            name,
            others: formatList(losers.map((l) => registry.get(l)?.title ?? l)),
          })
        : t('{key} now runs {name}.', { key: label(key), name }),
    );
    setCaptured(null);
  };

  return (
    <div className="kiln-options-keyboard">
      <div className="kiln-options-keyboard__bar">
        <label className="kiln-options__field">
          <span>{t('Shortcuts for')}</span>
          <select
            className="kiln-options__select"
            value={app}
            onChange={(e) => setApp(e.target.value as AppId)}
          >
            {(Object.keys(APP_NAMES) as AppId[]).filter(isReleasedApp).map((a) => (
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
            aria-label={t('Search commands or shortcuts')}
            placeholder={t('Search commands or shortcuts')}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <input
          ref={fileRef}
          type="file"
          accept="application/json,.json"
          hidden
          data-testid="keyboard-import-file"
          onChange={(e) => {
            void importFile(e.target.files?.[0]);
            e.target.value = '';
          }}
        />
        <button type="button" className="kiln-btn kiln-btn--sm" onClick={() => fileRef.current?.click()}>
          <span className="kiln-btn__icon">
            <Icon icon={Upload} size={14} />
          </span>
          <span className="kiln-btn__label">{t('Import…')}</span>
        </button>
        <button type="button" className="kiln-btn kiln-btn--sm" onClick={exportFile}>
          <span className="kiln-btn__icon">
            <Icon icon={Download} size={14} />
          </span>
          <span className="kiln-btn__label">{t('Export')}</span>
        </button>
        <button
          type="button"
          className="kiln-btn kiln-btn--sm"
          onClick={() => {
            keymap.resetAll();
            setProblem('');
            setMessage(t('All shortcuts restored to the Office defaults.'));
          }}
        >
          <span className="kiln-btn__icon">
            <Icon icon={RotateCcw} size={14} />
          </span>
          <span className="kiln-btn__label">{t('Reset All')}</span>
        </button>
      </div>
      {problem ? (
        <p className="kiln-options-keyboard__problem" role="alert">
          <Icon icon={TriangleAlert} size={16} />
          <span>{problem}</span>
        </p>
      ) : null}
      {clashes.length > 0 ? (
        <div className="kiln-options-keyboard__clashes" role="status" data-testid="keyboard-conflicts">
          <Icon icon={TriangleAlert} size={16} />
          <div>
            <strong>
              {t('{count, plural, one {# shortcut runs} other {# shortcuts run}} more than one command', {
                count: clashes.length,
              })}
            </strong>
            <ul>
              {clashes.map((c) => (
                <li key={c.label}>
                  <kbd className="kiln-options-keyboard__key">{c.label}</kbd>{' '}
                  {c.commands.map((id, i) => (
                    <span key={id}>
                      {i > 0 ? ', ' : ''}
                      <button
                        type="button"
                        className="kiln-options-keyboard__link"
                        onClick={() => {
                          setQuery('');
                          setSelected(id);
                        }}
                      >
                        {registry.get(id)?.title ?? id}
                      </button>
                    </span>
                  ))}
                </li>
              ))}
            </ul>
          </div>
        </div>
      ) : null}
      <div className="kiln-options-keyboard__main">
        <div className="kiln-options-keyboard__list" role="listbox" aria-label={t('Commands')} tabIndex={-1}>
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
                  {c.pending ? (
                    <span className="kiln-options-keyboard__pending"> {t('(coming soon)')}</span>
                  ) : null}
                </span>
                <span className="kiln-options-keyboard__keys">
                  {keys.map((k) => (
                    <kbd key={k} className="kiln-options-keyboard__key">
                      {label(k)}
                    </kbd>
                  ))}
                  {keymap.isCustomized(c.id, app) ? (
                    <span className="kiln-options-keyboard__custom">{t('Custom')}</span>
                  ) : null}
                </span>
              </div>
            );
          })}
          {commands.length === 0 ? (
            <p className="kiln-options__hint">{t('No commands match “{query}”.', { query })}</p>
          ) : null}
        </div>
        <div className="kiln-options-keyboard__editor" aria-live="polite">
          {selectedMeta ? (
            <>
              <h3>{selectedMeta.title}</h3>
              {selectedMeta.officeName && selectedMeta.officeName !== selectedMeta.title ? (
                <p className="kiln-options__hint">
                  {t('In Microsoft Office: {officeName}', { officeName: selectedMeta.officeName })}
                </p>
              ) : null}
              {selectedMeta.description ? (
                <p className="kiln-options__hint">{selectedMeta.description}</p>
              ) : null}
              <div className="kiln-options__label">{t('Current shortcuts')}</div>
              <div className="kiln-options-keyboard__current">
                {keymap.shortcutsFor(selectedMeta.id, app).length === 0 ? (
                  <span className="kiln-options__hint">{t('None')}</span>
                ) : null}
                {keymap.shortcutsFor(selectedMeta.id, app).map((k) => (
                  <span key={k} className="kiln-options-keyboard__chip">
                    {label(k)}
                    <button
                      type="button"
                      aria-label={t('Remove {label}', { label: label(k) })}
                      onClick={() => {
                        keymap.unassign(selectedMeta.id, k, app);
                        setMessage(t('{key} removed.', { key: label(k) }));
                      }}
                    >
                      <Icon icon={X} size={12} />
                    </button>
                  </span>
                ))}
              </div>
              <label className="kiln-options__label" htmlFor={captureId}>
                {t('Press new shortcut')}
              </label>
              <input
                id={captureId}
                className="kiln-options-keyboard__capture"
                readOnly
                value={captured ? formatChord(captured, platform) : ''}
                placeholder={t('Click here, then press keys')}
                onKeyDown={onCapture}
              />
              {captured ? (
                <p className={cx('kiln-options__hint', conflicts.length > 0 && 'kiln-options__warn')}>
                  {conflicts.length
                    ? t('Currently assigned to: {commands}', {
                        commands: formatList(conflicts.map((c) => registry.get(c)?.title ?? c)),
                      })
                    : t('Currently unassigned')}
                  {isBrowserReservedChord(captured, platform)
                    ? t(
                        ' · In a browser tab this shortcut may be taken by the browser; it works in the desktop app.',
                      )
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
                  {t('Assign')}
                </button>
                <button
                  type="button"
                  className="kiln-btn kiln-btn--sm"
                  disabled={!keymap.isCustomized(selectedMeta.id, app)}
                  onClick={() => {
                    keymap.resetCommand(selectedMeta.id, app);
                    keymap.resetCommand(selectedMeta.id, 'common');
                    setMessage(t('{title}: default shortcuts restored.', { title: selectedMeta.title }));
                  }}
                >
                  {t('Reset')}
                </button>
              </div>
            </>
          ) : (
            <p className="kiln-options__hint">{t('Select a command to see or change its shortcuts.')}</p>
          )}
          {message ? <p className="kiln-options-keyboard__message">{message}</p> : null}
        </div>
      </div>
    </div>
  );
}
