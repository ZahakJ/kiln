import { direction, isLanguagePref, LANGUAGE_NAMES, LOCALES, languageName, msg, t, tx } from '@kiln/i18n';
import { isTauri } from '@kiln/protocol';
import {
  Keyboard,
  Languages,
  type LucideIcon,
  Monitor,
  Save,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  SpellCheck,
} from 'lucide-react';
import { type ReactNode, useId, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from '../primitives/cx.ts';
import { Dialog, DialogContent } from '../primitives/Dialog.tsx';
import { NumberInput } from '../primitives/NumberInput.tsx';
import { useTheme } from '../theme/ThemeProvider.tsx';
import { AUTOCORRECT_BUILTIN } from './autocorrect.ts';
import { useOptionalCommands } from './CommandContext.tsx';
import { KeyboardOptions } from './KeyboardOptions.tsx';
import {
  EDITING_LANGUAGES,
  editingLanguage,
  type OfficeOptions,
  type OptionsPatch,
  useOfficeOptions,
} from './options.ts';
import './options.css';

export type OptionsSectionId =
  | 'general'
  | 'display'
  | 'proofing'
  | 'save'
  | 'language'
  | 'advanced'
  | 'keyboard'
  | 'privacy';

export interface OptionsExtraSection {
  id: string;
  label: string;
  content: ReactNode;
}

export interface OptionsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialSection?: OptionsSectionId | string;
  /**
   * App-specific sections. One with the id `advanced` is shown at the end of the Advanced page
   * (Grid's workbook options); the others are listed after the built-in sections.
   */
  extraSections?: readonly OptionsExtraSection[];
  /** Controlled options; default: the persisted `useOfficeOptions()` store. */
  value?: OfficeOptions;
  onChange?: (patch: OptionsPatch) => void;
  /** App name in the title ("Kiln Write Options"). */
  appName?: string;
}

interface SectionInfo {
  id: OptionsSectionId;
  label: string;
  icon: LucideIcon;
  /** One line under the page title. */
  about: string;
}

const SECTIONS: readonly SectionInfo[] = [
  {
    id: 'general',
    label: msg('General'),
    icon: Settings2,
    about: msg('How Kiln looks and who you are in comments and tracked changes.'),
  },
  {
    id: 'display',
    label: msg('Display'),
    icon: Monitor,
    about: msg('The ribbon and the ScreenTips that explain each command.'),
  },
  {
    id: 'proofing',
    label: msg('Proofing'),
    icon: SpellCheck,
    about: msg('Spelling, AutoCorrect and what changes as you type.'),
  },
  {
    id: 'save',
    label: msg('Save'),
    icon: Save,
    about: msg('AutoSave, recovery after a crash and the format of new files.'),
  },
  {
    id: 'language',
    label: msg('Language'),
    icon: Languages,
    about: msg('The language of the interface and of new documents.'),
  },
  {
    id: 'advanced',
    label: msg('Advanced'),
    icon: SlidersHorizontal,
    about: msg('Editing, measurements and lists, with the options of each app.'),
  },
  {
    id: 'keyboard',
    label: msg('Keyboard'),
    icon: Keyboard,
    about: msg('Every shortcut, searchable. Press new keys to remap a command.'),
  },
  {
    id: 'privacy',
    label: msg('Privacy'),
    icon: ShieldCheck,
    about: msg('What Kiln keeps, and where it goes.'),
  },
];

function Check({
  label,
  checked,
  onChange,
  hint,
  disabled,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  hint?: string;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className={cx('kiln-options__check', disabled && 'is-disabled')}>
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <label htmlFor={id}>
        {label}
        {hint ? <span className="kiln-options__hint">{hint}</span> : null}
      </label>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    // biome-ignore lint/a11y/noLabelWithoutControl: the label wraps its control, passed as children
    <label className="kiln-options__field">
      <span>{label}</span>
      {children}
    </label>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <fieldset className="kiln-options__section">
      <legend>{title}</legend>
      {children}
    </fieldset>
  );
}

function PageHeader({ section }: { section: SectionInfo }) {
  return (
    <header className="kiln-options__page-head">
      <span className="kiln-options__page-icon" aria-hidden="true">
        <Icon icon={section.icon} size={20} />
      </span>
      <div>
        <h2 className="kiln-options__page-title">{tx(section.label)}</h2>
        <p className="kiln-options__page-about">{tx(section.about)}</p>
      </div>
    </header>
  );
}

/**
 * AutoCorrect's replace list: the built-in entries (deletable) and the user's own, with a box to
 * add one and a filter, as Word's AutoCorrect dialog.
 */
function AutoCorrectList({
  added,
  removed,
  onChange,
}: {
  added: readonly (readonly [string, string])[];
  removed: readonly string[];
  onChange: (added: [string, string][], removed: string[]) => void;
}) {
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const fromId = useId();
  const toId = useId();
  const rows = [
    ...added.map(([f, t]) => ({ from: f, to: t, own: true })),
    ...AUTOCORRECT_BUILTIN.filter(([f]) => !removed.includes(f) && !added.some(([a]) => a === f)).map(
      ([f, t]) => ({ from: f, to: t, own: false }),
    ),
  ];
  const q = from.trim().toLowerCase();
  const shown = q ? rows.filter((r) => r.from.toLowerCase().startsWith(q)) : rows;
  const add = () => {
    const f = from.trim();
    if (!f || !to) return;
    onChange(
      [[f, to], ...added.filter(([a]) => a !== f)].map(([a, b]) => [a, b] as [string, string]),
      removed.filter((r) => r !== f),
    );
    setFrom('');
    setTo('');
  };
  const remove = (r: { from: string; own: boolean }) => {
    onChange(
      added.filter(([a]) => a !== r.from).map(([a, b]) => [a, b] as [string, string]),
      r.own || removed.includes(r.from) ? [...removed] : [...removed, r.from],
    );
  };
  return (
    <div className="kiln-options__autocorrect" data-testid="options-autocorrect">
      <div className="kiln-options__autocorrect-add">
        <label htmlFor={fromId}>{t('Replace')}</label>
        <input
          id={fromId}
          className="kiln-options__input"
          value={from}
          onChange={(e) => setFrom(e.target.value)}
        />
        <label htmlFor={toId}>{t('With')}</label>
        <input
          id={toId}
          className="kiln-options__input"
          value={to}
          onChange={(e) => setTo(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') add();
          }}
        />
        <button type="button" className="kiln-options__button" disabled={!from.trim() || !to} onClick={add}>
          {t('Add')}
        </button>
      </div>
      <ul className="kiln-options__autocorrect-list" aria-label={t('AutoCorrect entries')}>
        {shown.slice(0, 200).map((r) => (
          <li key={r.from}>
            <span className="kiln-options__autocorrect-from">{r.from}</span>
            <span className="kiln-options__autocorrect-to">{r.to}</span>
            <button type="button" aria-label={t('Delete {from}', { from: r.from })} onClick={() => remove(r)}>
              {t('Delete')}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * File ▸ Options: the Office-style dialog with a section list on the left (General, Display,
 * Proofing, Save, Language, Advanced, Keyboard ★, Privacy). Changes apply at once and persist;
 * the core hears of the ones it applies (AutoSave's wait, recovery, spelling) from the app frame.
 */
export function OptionsDialog({
  open,
  onOpenChange,
  initialSection = 'general',
  extraSections = [],
  value: valueProp,
  onChange,
  appName = 'Kiln',
}: OptionsDialogProps) {
  const [stored, update] = useOfficeOptions();
  const value = valueProp ?? stored;
  const patch = onChange ?? update;
  const [section, setSection] = useState<string>(initialSection);
  const theme = useTheme();
  const commands = useOptionalCommands();
  const app = commands?.app;
  const appAdvanced = extraSections.filter((s) => s.id === 'advanced');
  const sections: (SectionInfo | OptionsExtraSection)[] = [
    ...SECTIONS.filter((s) => s.id !== 'keyboard' || commands?.keymap),
    ...extraSections.filter((s) => s.id !== 'advanced'),
  ];
  const current = sections.find((s) => s.id === section) ?? sections[0];
  const builtIn = current && 'icon' in current ? current : undefined;
  // App-specific options show in that app's window, and all of them on Home.
  const forApp = (a: string) => !app || app === 'hub' || app === a;

  const body = (() => {
    switch (current?.id) {
      case 'general':
        return (
          <>
            <Section title={t('User interface options')}>
              <Check
                label={t('Show Mini Toolbar on selection')}
                hint={t('Formatting buttons appear beside the text you select.')}
                checked={value.general.showMiniToolbar}
                onChange={(v) => patch({ general: { showMiniToolbar: v } })}
              />
              <Check
                label={t('Enable Live Preview')}
                hint={t(
                  'Pointing at a style, colour or effect shows it on the document before you choose it.',
                )}
                checked={value.general.livePreview}
                onChange={(v) => patch({ general: { livePreview: v } })}
              />
              <Check
                label={t('Classic Office look')}
                hint={t('Colourful ribbon icons and a title bar in the colour of the app, like Office 2019.')}
                checked={value.general.classicLook}
                onChange={(v) => patch({ general: { classicLook: v } })}
              />
              <Field label={t('Theme')}>
                <select
                  className="kiln-options__select"
                  value={theme.preference}
                  onChange={(e) => theme.setPreference(e.target.value as typeof theme.preference)}
                >
                  <option value="system">{t('Use system setting')}</option>
                  <option value="light">{t('Light')}</option>
                  <option value="dark">{t('Dark')}</option>
                  <option value="hc">{t('High contrast')}</option>
                </select>
              </Field>
            </Section>
            <Section title={t('Personalize your copy of Kiln')}>
              <Field label={t('User name')}>
                <input
                  className="kiln-options__input"
                  value={value.general.userName}
                  onChange={(e) => patch({ general: { userName: e.target.value } })}
                />
              </Field>
              <Field label={t('Initials')}>
                <input
                  className="kiln-options__input kiln-options__input--short"
                  maxLength={4}
                  value={value.general.initials}
                  onChange={(e) => patch({ general: { initials: e.target.value } })}
                />
              </Field>
              <p className="kiln-options__hint">
                {t('Used for comments, tracked changes and document properties. Stored only on this device.')}
              </p>
            </Section>
          </>
        );
      case 'display':
        return (
          <Section title={t('Ribbon and ScreenTips')}>
            <Field label={t('Ribbon layout')}>
              <select
                className="kiln-options__select"
                value={value.display.ribbonLayout}
                onChange={(e) =>
                  patch({
                    display: { ribbonLayout: e.target.value as OfficeOptions['display']['ribbonLayout'] },
                  })
                }
              >
                <option value="auto">{t('Automatic (Classic on wide windows)')}</option>
                <option value="classic">{t('Classic')}</option>
                <option value="simplified">{t('Simplified')}</option>
              </select>
            </Field>
            <Field label={t('ScreenTip style')}>
              <select
                className="kiln-options__select"
                value={value.display.screenTips}
                onChange={(e) =>
                  patch({
                    display: { screenTips: e.target.value as OfficeOptions['display']['screenTips'] },
                  })
                }
              >
                <option value="full">{t('Show feature descriptions in ScreenTips')}</option>
                <option value="compact">{t('Don’t show feature descriptions')}</option>
                <option value="off">{t('Don’t show ScreenTips')}</option>
              </select>
            </Field>
            <Check
              label={t('Show shortcut keys in ScreenTips')}
              checked={value.display.showShortcutsInTips}
              disabled={value.display.screenTips === 'off'}
              onChange={(v) => patch({ display: { showShortcutsInTips: v } })}
            />
          </Section>
        );
      case 'proofing':
        return (
          <>
            <Section title={t('When correcting spelling')}>
              <Check
                label={t('Ignore words in UPPERCASE')}
                checked={value.proofing.ignoreUppercase}
                onChange={(v) => patch({ proofing: { ignoreUppercase: v } })}
              />
              <Check
                label={t('Ignore words that contain numbers')}
                checked={value.proofing.ignoreWordsWithNumbers}
                onChange={(v) => patch({ proofing: { ignoreWordsWithNumbers: v } })}
              />
              <Check
                label={t('Ignore Internet and file addresses')}
                checked={value.proofing.ignoreInternetAddresses}
                onChange={(v) => patch({ proofing: { ignoreInternetAddresses: v } })}
              />
            </Section>
            <Section title={t('While you type')}>
              <Check
                label={t('Check spelling as you type')}
                checked={value.proofing.checkSpellingAsYouType}
                onChange={(v) => patch({ proofing: { checkSpellingAsYouType: v } })}
              />
            </Section>
            <Section title={t('AutoCorrect')}>
              <Check
                label={t('Replace text as you type')}
                checked={value.proofing.autoCorrect}
                onChange={(v) => patch({ proofing: { autoCorrect: v } })}
              />
              <Check
                label={t('Capitalise first letter of sentences')}
                checked={value.proofing.capitalizeSentences}
                onChange={(v) => patch({ proofing: { capitalizeSentences: v } })}
              />
              <Check
                label={t('Capitalise names of days')}
                checked={value.proofing.capitalizeDays}
                onChange={(v) => patch({ proofing: { capitalizeDays: v } })}
              />
              <Check
                label={t('Correct TWo INitial CApitals')}
                checked={value.proofing.twoInitialCaps}
                onChange={(v) => patch({ proofing: { twoInitialCaps: v } })}
              />
              <AutoCorrectList
                added={value.proofing.autoCorrectAdded}
                removed={value.proofing.autoCorrectRemoved}
                onChange={(autoCorrectAdded, autoCorrectRemoved) =>
                  patch({ proofing: { autoCorrectAdded, autoCorrectRemoved } })
                }
              />
            </Section>
            <Section title={t('AutoFormat as you type')}>
              <Check
                label={t('Markdown shortcuts (# headings, - lists, **bold**, `code`, --- rules, > quotes)')}
                checked={value.proofing.markdownShortcuts}
                onChange={(v) => patch({ proofing: { markdownShortcuts: v } })}
              />
              <Check
                label={t('“Straight quotes” with “smart quotes”')}
                checked={value.proofing.smartQuotes}
                onChange={(v) => patch({ proofing: { smartQuotes: v } })}
              />
              <Check
                label={t('Hyphens (--) with dash (—)')}
                checked={value.proofing.smartDashes}
                onChange={(v) => patch({ proofing: { smartDashes: v } })}
              />
              <Check
                label={t('Three dots (...) with an ellipsis (…)')}
                checked={value.proofing.smartEllipsis}
                onChange={(v) => patch({ proofing: { smartEllipsis: v } })}
              />
              <Check
                label={t('Fractions (1/2) with fraction characters (½)')}
                checked={value.proofing.smartFractions}
                onChange={(v) => patch({ proofing: { smartFractions: v } })}
              />
            </Section>
          </>
        );
      case 'save':
        return (
          <>
            <Section title={t('AutoSave')}>
              <Check
                label={t('Turn on AutoSave when a file opens')}
                hint={t(
                  'Files that can be saved where they are get the title bar’s AutoSave switch turned on.',
                )}
                checked={value.save.autoSave}
                onChange={(v) => patch({ save: { autoSave: v } })}
              />
              <Field label={t('Save after a pause of (seconds)')}>
                <NumberInput
                  aria-label={t('Save after a pause of (seconds)')}
                  value={value.save.autoSaveInterval}
                  onValueChange={(v) => patch({ save: { autoSaveInterval: Math.round(v) } })}
                  min={1}
                  max={600}
                  precision={0}
                  width={96}
                />
              </Field>
            </Section>
            <Section title={t('Recovery')}>
              <Check
                label={t('Keep AutoRecover information')}
                hint={t(
                  'Kiln records every change as you make it, so a crash loses nothing. Applies to documents opened from now on.',
                )}
                checked={value.save.keepRecoveryInfo}
                onChange={(v) => patch({ save: { keepRecoveryInfo: v } })}
              />
            </Section>
            <Section title={t('New files')}>
              <Field label={t('Save new files in')}>
                <select
                  className="kiln-options__select"
                  value={value.save.defaultFormat}
                  onChange={(e) =>
                    patch({
                      save: { defaultFormat: e.target.value as OfficeOptions['save']['defaultFormat'] },
                    })
                  }
                >
                  <option value="office">{t('Office formats (.docx, .xlsx, .pptx)')}</option>
                  {/* OpenDocument is saved by the native core only (the web core has no writer). */}
                  {isTauri() || value.save.defaultFormat === 'odf' ? (
                    <option value="odf">{t('OpenDocument formats (.odt, .ods, .odp)')}</option>
                  ) : null}
                </select>
              </Field>
            </Section>
          </>
        );
      case 'language':
        return (
          <>
            <Section title={t('Display language')}>
              <Field label={t('Interface language')}>
                <select
                  className="kiln-options__select"
                  data-testid="options-ui-language"
                  value={isLanguagePref(value.language.ui) ? value.language.ui : 'en'}
                  onChange={(e) => patch({ language: { ui: e.target.value } })}
                >
                  <option value="system">{t('System')}</option>
                  {LOCALES.map((l) => (
                    // Each language in its own name and script, as Office lists them.
                    <option key={l} value={l} lang={l} dir={direction(l)}>
                      {LANGUAGE_NAMES[l]}
                    </option>
                  ))}
                </select>
              </Field>
              <p className="kiln-options__hint">
                {t(
                  'Menus, buttons, dialogs and messages are shown in this language. Documents keep their own.',
                )}
              </p>
            </Section>
            <Section title={t('Editing language')}>
              <Field label={t('Default language for new documents')}>
                <select
                  className="kiln-options__select"
                  data-testid="options-editing-language"
                  value={
                    value.language.editing && value.language.editing !== 'auto'
                      ? value.language.editing
                      : 'auto'
                  }
                  onChange={(e) => patch({ language: { editing: e.target.value } })}
                >
                  {/* Office's default: new documents follow the display language until one is picked. */}
                  <option value="auto">
                    {t('Match display language — {language}', {
                      language: languageName(
                        editingLanguage({ ...value, language: { ...value.language, editing: 'auto' } }),
                      ),
                    })}
                  </option>
                  {EDITING_LANGUAGES.map((code) => (
                    <option key={code} value={code}>
                      {languageName(code)}
                    </option>
                  ))}
                </select>
              </Field>
              <p className="kiln-options__hint">
                {t(
                  'New documents, workbooks and presentations are in this language: their names, spelling and hyphenation follow it, and in a right-to-left language paragraphs and sheets start at the right.',
                )}
              </p>
            </Section>
          </>
        );
      case 'advanced':
        return (
          <>
            {forApp('write') ? (
              <Section title={t('Editing options')}>
                <Check
                  label={t('Allow text to be dragged and dropped')}
                  hint={t('Kiln Write: drag selected text to move it, or hold Ctrl to copy it.')}
                  checked={value.advanced.dragAndDrop}
                  onChange={(v) => patch({ advanced: { dragAndDrop: v } })}
                />
              </Section>
            ) : null}
            {forApp('grid') ? (
              <Section title={t('Kiln Grid editing')}>
                <Check
                  label={t('After pressing Enter, move selection')}
                  checked={value.advanced.enterMoves}
                  onChange={(v) => patch({ advanced: { enterMoves: v } })}
                />
                <Field label={t('Direction')}>
                  <select
                    className="kiln-options__select"
                    data-testid="options-enter-direction"
                    value={value.advanced.enterDirection}
                    disabled={!value.advanced.enterMoves}
                    onChange={(e) =>
                      patch({
                        advanced: {
                          enterDirection: e.target.value as OfficeOptions['advanced']['enterDirection'],
                        },
                      })
                    }
                  >
                    <option value="down">{t('Down')}</option>
                    <option value="right">{t('Right')}</option>
                    <option value="up">{t('Up')}</option>
                    <option value="left">{t('Left')}</option>
                  </select>
                </Field>
              </Section>
            ) : null}
            <Section title={t('Display')}>
              <Field label={t('Show measurements in units of')}>
                <select
                  className="kiln-options__select"
                  value={value.display.measurementUnit ?? 'auto'}
                  onChange={(e) =>
                    patch({
                      display: {
                        measurementUnit: e.target.value as OfficeOptions['display']['measurementUnit'],
                      },
                    })
                  }
                >
                  <option value="auto">{t('As in my region')}</option>
                  <option value="in">{t('Inches')}</option>
                  <option value="cm">{t('Centimetres')}</option>
                  <option value="mm">{t('Millimetres')}</option>
                  <option value="pt">{t('Points')}</option>
                </select>
              </Field>
              <Field label={t('Show this number of Recent Documents')}>
                <NumberInput
                  aria-label={t('Show this number of Recent Documents')}
                  value={value.advanced.recentCount}
                  onValueChange={(v) => patch({ advanced: { recentCount: Math.round(v) } })}
                  min={0}
                  max={50}
                  precision={0}
                  width={96}
                />
              </Field>
            </Section>
            {/* The app's own (Grid: When calculating this workbook), each a section of its own. */}
            {appAdvanced.map((s) => (
              <div key={s.label}>{s.content}</div>
            ))}
          </>
        );
      case 'keyboard':
        return <KeyboardOptions />;
      case 'privacy':
        return (
          <>
            <Section title={t('Your data')}>
              <p className="kiln-options__text">
                {t('Kiln has no telemetry. Nothing about how you use it is sent anywhere.')}
              </p>
              <ul className="kiln-options__facts">
                <li>{t('Recent files, pinned files and your preferences stay on this device.')}</li>
                <li>
                  {t('Recovery journals and version history are kept beside your documents, on this device.')}
                </li>
                <li>{t('A shared document goes only to the collaboration server named in its link.')}</li>
              </ul>
            </Section>
            <Section title={t('Writing assistant')}>
              <Field label={t('Assistant')}>
                <select
                  className="kiln-options__select"
                  value={value.privacy.assistant}
                  onChange={(e) =>
                    patch({ privacy: { assistant: e.target.value as OfficeOptions['privacy']['assistant'] } })
                  }
                >
                  <option value="none">{t('None (no assistant)')}</option>
                  <option value="echo">
                    {t('Local sample (works offline, nothing leaves this device)')}
                  </option>
                </select>
              </Field>
              <p className="kiln-options__hint">
                {t('Off unless you choose one. No assistant reaches the network without your say.')}
              </p>
            </Section>
          </>
        );
      default:
        return current && 'content' in current ? current.content : null;
    }
  })();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('{appName} Options', { appName })}
        size="xl"
        className="kiln-options"
        footer={
          <button type="button" className="kiln-btn kiln-btn--primary" onClick={() => onOpenChange(false)}>
            {t('Close')}
          </button>
        }
      >
        <div className="kiln-options__layout">
          <nav className="kiln-options__nav" aria-label={t('Options sections')}>
            {sections.map((s) => (
              <button
                key={s.id}
                type="button"
                className={cx('kiln-options__nav-item', s.id === current?.id && 'is-current')}
                aria-current={s.id === current?.id ? 'page' : undefined}
                onClick={() => setSection(s.id)}
              >
                {'icon' in s ? (
                  <span className="kiln-options__nav-icon" aria-hidden="true">
                    <Icon icon={s.icon} size={16} />
                  </span>
                ) : null}
                <span className="kiln-options__nav-label">{tx(s.label)}</span>
              </button>
            ))}
          </nav>
          <div className="kiln-options__body" data-section={current?.id}>
            {builtIn ? <PageHeader section={builtIn} /> : null}
            {body}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
