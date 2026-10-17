import { type ReactNode, useId, useState } from 'react';
import { cx } from '../primitives/cx.ts';
import { Dialog, DialogContent } from '../primitives/Dialog.tsx';
import { NumberInput } from '../primitives/NumberInput.tsx';
import { useTheme } from '../theme/ThemeProvider.tsx';
import { useOptionalCommands } from './CommandContext.tsx';
import { KeyboardOptions } from './KeyboardOptions.tsx';
import { type OfficeOptions, type OptionsPatch, useOfficeOptions } from './options.ts';
import './options.css';

export type OptionsSectionId =
  | 'general'
  | 'display'
  | 'proofing'
  | 'save'
  | 'language'
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
  /** App-specific sections (Advanced, Formulas…), listed after the built-in ones. */
  extraSections?: readonly OptionsExtraSection[];
  /** Controlled options; default: the persisted `useOfficeOptions()` store. */
  value?: OfficeOptions;
  onChange?: (patch: OptionsPatch) => void;
  /** App name in the title ("Kiln Write Options"). */
  appName?: string;
}

const SECTIONS: { id: OptionsSectionId; label: string }[] = [
  { id: 'general', label: 'General' },
  { id: 'display', label: 'Display' },
  { id: 'proofing', label: 'Proofing' },
  { id: 'save', label: 'Save' },
  { id: 'language', label: 'Language' },
  { id: 'keyboard', label: 'Keyboard' },
  { id: 'privacy', label: 'Privacy' },
];

const EDITING_LANGUAGES = [
  ['en-US', 'English (United States)'],
  ['en-GB', 'English (United Kingdom)'],
  ['de-DE', 'German (Germany)'],
  ['fr-FR', 'French (France)'],
  ['es-ES', 'Spanish (Spain)'],
  ['ar-SA', 'Arabic (Saudi Arabia)'],
] as const;

function Check({
  label,
  checked,
  onChange,
  hint,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  hint?: string;
}) {
  const id = useId();
  return (
    <div className="kiln-options__check">
      <input id={id} type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
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

/**
 * File ▸ Options: the Office-style dialog with a section list on the left (General, Display,
 * Proofing, Save, Language, Keyboard ★, Privacy). Changes apply immediately and persist.
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
  const all = [...SECTIONS.filter((s) => s.id !== 'keyboard' || commands?.keymap), ...extraSections];
  const current = all.find((s) => s.id === section) ?? all[0];

  const body = (() => {
    switch (current?.id) {
      case 'general':
        return (
          <>
            <Section title="User interface options">
              <Check
                label="Show Mini Toolbar on selection"
                checked={value.general.showMiniToolbar}
                onChange={(v) => patch({ general: { showMiniToolbar: v } })}
              />
              <Check
                label="Enable Live Preview"
                checked={value.general.livePreview}
                onChange={(v) => patch({ general: { livePreview: v } })}
              />
              <Check
                label="Classic Office look"
                hint="Colourful ribbon icons, like Office 2019."
                checked={value.general.classicLook}
                onChange={(v) => patch({ general: { classicLook: v } })}
              />
              <Field label="Theme">
                <select
                  className="kiln-options__select"
                  value={theme.preference}
                  onChange={(e) => theme.setPreference(e.target.value as typeof theme.preference)}
                >
                  <option value="system">Use system setting</option>
                  <option value="light">Light</option>
                  <option value="dark">Dark</option>
                  <option value="hc">High contrast</option>
                </select>
              </Field>
            </Section>
            <Section title="Personalize your copy of Kiln">
              <Field label="User name">
                <input
                  className="kiln-options__input"
                  value={value.general.userName}
                  onChange={(e) => patch({ general: { userName: e.target.value } })}
                />
              </Field>
              <Field label="Initials">
                <input
                  className="kiln-options__input kiln-options__input--short"
                  maxLength={4}
                  value={value.general.initials}
                  onChange={(e) => patch({ general: { initials: e.target.value } })}
                />
              </Field>
              <p className="kiln-options__hint">
                Used for comments, tracked changes and document properties. Stored only on this device.
              </p>
            </Section>
          </>
        );
      case 'display':
        return (
          <Section title="Ribbon and ScreenTips">
            <Field label="Ribbon layout">
              <select
                className="kiln-options__select"
                value={value.display.ribbonLayout}
                onChange={(e) =>
                  patch({
                    display: { ribbonLayout: e.target.value as OfficeOptions['display']['ribbonLayout'] },
                  })
                }
              >
                <option value="auto">Automatic (Classic on wide windows)</option>
                <option value="classic">Classic</option>
                <option value="simplified">Simplified</option>
              </select>
            </Field>
            <Field label="ScreenTip style">
              <select
                className="kiln-options__select"
                value={value.display.screenTips}
                onChange={(e) =>
                  patch({ display: { screenTips: e.target.value as OfficeOptions['display']['screenTips'] } })
                }
              >
                <option value="full">Show feature descriptions in ScreenTips</option>
                <option value="compact">Don’t show feature descriptions</option>
                <option value="off">Don’t show ScreenTips</option>
              </select>
            </Field>
            <Check
              label="Show shortcut keys in ScreenTips"
              checked={value.display.showShortcutsInTips}
              onChange={(v) => patch({ display: { showShortcutsInTips: v } })}
            />
          </Section>
        );
      case 'proofing':
        return (
          <>
            <Section title="When correcting spelling">
              <Check
                label="Ignore words in UPPERCASE"
                checked={value.proofing.ignoreUppercase}
                onChange={(v) => patch({ proofing: { ignoreUppercase: v } })}
              />
              <Check
                label="Ignore words that contain numbers"
                checked={value.proofing.ignoreWordsWithNumbers}
                onChange={(v) => patch({ proofing: { ignoreWordsWithNumbers: v } })}
              />
              <Check
                label="Ignore Internet and file addresses"
                checked={value.proofing.ignoreInternetAddresses}
                onChange={(v) => patch({ proofing: { ignoreInternetAddresses: v } })}
              />
            </Section>
            <Section title="While you type">
              <Check
                label="Check spelling as you type"
                checked={value.proofing.checkSpellingAsYouType}
                onChange={(v) => patch({ proofing: { checkSpellingAsYouType: v } })}
              />
              <Check
                label="Correct common typing mistakes (AutoCorrect)"
                checked={value.proofing.autoCorrect}
                onChange={(v) => patch({ proofing: { autoCorrect: v } })}
              />
            </Section>
            <Section title="AutoFormat as you type">
              <Check
                label="Markdown shortcuts (# headings, - lists, **bold**, `code`, --- rules, > quotes)"
                checked={value.proofing.markdownShortcuts}
                onChange={(v) => patch({ proofing: { markdownShortcuts: v } })}
              />
              <Check
                label="“Straight quotes” with “smart quotes”"
                checked={value.proofing.smartQuotes}
                onChange={(v) => patch({ proofing: { smartQuotes: v } })}
              />
              <Check
                label="Hyphens (--) with dash (—)"
                checked={value.proofing.smartDashes}
                onChange={(v) => patch({ proofing: { smartDashes: v } })}
              />
              <Check
                label="Three dots (...) with an ellipsis (…)"
                checked={value.proofing.smartEllipsis}
                onChange={(v) => patch({ proofing: { smartEllipsis: v } })}
              />
              <Check
                label="Fractions (1/2) with fraction characters (½)"
                checked={value.proofing.smartFractions}
                onChange={(v) => patch({ proofing: { smartFractions: v } })}
              />
            </Section>
          </>
        );
      case 'save':
        return (
          <Section title="Save documents">
            <Check
              label="AutoSave files by default"
              checked={value.save.autoSave}
              onChange={(v) => patch({ save: { autoSave: v } })}
            />
            <Field label="AutoSave every (seconds)">
              <NumberInput
                aria-label="AutoSave every (seconds)"
                value={value.save.autoSaveInterval}
                onValueChange={(v) => patch({ save: { autoSaveInterval: Math.round(v) } })}
                min={1}
                max={600}
                precision={0}
                width={96}
              />
            </Field>
            <Check
              label="Keep AutoRecover information"
              checked={value.save.keepRecoveryInfo}
              onChange={(v) => patch({ save: { keepRecoveryInfo: v } })}
            />
            <Field label="Save AutoRecover information every (minutes)">
              <NumberInput
                aria-label="Save AutoRecover information every (minutes)"
                value={value.save.recoveryInterval}
                onValueChange={(v) => patch({ save: { recoveryInterval: Math.round(v) } })}
                min={1}
                max={120}
                precision={0}
                disabled={!value.save.keepRecoveryInfo}
                width={96}
              />
            </Field>
            <Field label="Save new files in">
              <select
                className="kiln-options__select"
                value={value.save.defaultFormat}
                onChange={(e) =>
                  patch({ save: { defaultFormat: e.target.value as OfficeOptions['save']['defaultFormat'] } })
                }
              >
                <option value="office">Office formats (.docx, .xlsx, .pptx)</option>
                <option value="odf">OpenDocument formats (.odt, .ods, .odp)</option>
              </select>
            </Field>
          </Section>
        );
      case 'language':
        return (
          <>
            <Section title="Display language">
              <Field label="Interface language">
                <select
                  className="kiln-options__select"
                  value={value.language.ui}
                  onChange={(e) => patch({ language: { ui: e.target.value } })}
                >
                  <option value="en">English</option>
                </select>
              </Field>
            </Section>
            <Section title="Editing language">
              <Field label="Default language for new documents">
                <select
                  className="kiln-options__select"
                  value={value.language.editing}
                  onChange={(e) => patch({ language: { editing: e.target.value } })}
                >
                  {EDITING_LANGUAGES.map(([code, name]) => (
                    <option key={code} value={code}>
                      {name}
                    </option>
                  ))}
                </select>
              </Field>
              <p className="kiln-options__hint">
                Sets the language of new text, which hyphenation and right-to-left layout follow.
              </p>
            </Section>
          </>
        );
      case 'keyboard':
        return <KeyboardOptions />;
      case 'privacy':
        return (
          <Section title="Privacy">
            <p className="kiln-options__text">
              Kiln has no telemetry. Recent commands, recent files and your preferences stay on this device.
            </p>
            <Field label="Writing assistant">
              <select
                className="kiln-options__select"
                value={value.privacy.assistant}
                onChange={(e) =>
                  patch({ privacy: { assistant: e.target.value as OfficeOptions['privacy']['assistant'] } })
                }
              >
                <option value="none">None (no assistant)</option>
                <option value="echo">Local sample (works offline, nothing leaves this device)</option>
              </select>
            </Field>
          </Section>
        );
      default:
        return current && 'content' in current ? (current as OptionsExtraSection).content : null;
    }
  })();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={`${appName} Options`}
        size="xl"
        className="kiln-options"
        footer={
          <button type="button" className="kiln-btn kiln-btn--primary" onClick={() => onOpenChange(false)}>
            Close
          </button>
        }
      >
        <div className="kiln-options__layout">
          <nav className="kiln-options__nav" aria-label="Options sections">
            {all.map((s) => (
              <button
                key={s.id}
                type="button"
                className={cx('kiln-options__nav-item', s.id === current?.id && 'is-current')}
                aria-current={s.id === current?.id ? 'page' : undefined}
                onClick={() => setSection(s.id)}
              >
                {s.label}
              </button>
            ))}
          </nav>
          <div className="kiln-options__body" data-section={current?.id}>
            {body}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
