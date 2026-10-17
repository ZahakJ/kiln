import { ChevronDown } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import {
  useCommandMeta,
  useCommandShortcut,
  useCommandState,
  useCommands,
  useExecute,
} from '../chrome/CommandContext.tsx';
import { resolveCommandIcon } from '../chrome/commandIcons.tsx';
import { CommandMenuItems } from '../contextmenu/CommandMenuItems.tsx';
import { isGlyphName } from '../icons/glyphs/index.ts';
import { Icon } from '../icons/Icon.tsx';
import { ColorPicker } from '../primitives/ColorPicker.tsx';
import { Combobox } from '../primitives/Combobox.tsx';
import type { ColorChoice } from '../primitives/color.ts';
import { cx } from '../primitives/cx.ts';
import { Gallery } from '../primitives/Gallery.tsx';
import { Menu, MenuContent, MenuTrigger } from '../primitives/Menu.tsx';
import { NumberInput } from '../primitives/NumberInput.tsx';
import { SplitButton } from '../primitives/SplitButton.tsx';
import { ariaKeyShortcuts } from '../primitives/shortcut.ts';
import { Tooltip } from '../primitives/Tooltip.tsx';
import type { RibbonItemSize, RibbonLayout, RibbonLeafItem } from './types.ts';
import { isLeafVisible } from './visibility.ts';

/** Icon sized for the ribbon with a constant ~1.5 px stroke whatever the source grid. */
export function RibbonIcon({ name, size }: { name: string | undefined; size: number }) {
  const icon = resolveCommandIcon(name);
  if (!icon) return null;
  const px = size >= 28 ? 1.6 : 1.5;
  const grid = typeof icon === 'string' && isGlyphName(icon) ? 20 : 24;
  return <Icon icon={icon} size={size} strokeWidth={(px * grid) / size} />;
}

export interface RibbonItemViewProps {
  item: RibbonLeafItem;
  size: RibbonItemSize;
  layout: RibbonLayout;
  keyTip?: string;
  keyTipLevel?: string;
}

function TipBody({
  description,
  officeName,
  title,
  pending,
}: {
  description?: string;
  officeName?: string;
  title: string;
  pending: boolean;
}) {
  const office = officeName && officeName.toLowerCase() !== title.toLowerCase() ? officeName : undefined;
  if (!description && !office && !pending) return null;
  return (
    <>
      {description ? <span className="kiln-ribbon__tip-text">{description}</span> : null}
      {office ? <span className="kiln-ribbon__tip-office">In Microsoft Office: {office}</span> : null}
      {pending ? <span className="kiln-ribbon__tip-pending">Not available yet in this build.</span> : null}
    </>
  );
}

/** Wraps a control in the Office-style super tooltip: title (shortcut), description, Office name. */
function useTip(command: string | undefined, label: string | undefined) {
  const meta = useCommandMeta(command);
  const state = useCommandState(command);
  const shortcut = useCommandShortcut(command);
  const title = label ?? meta?.title ?? command ?? '';
  const description = (
    <TipBody
      description={meta?.description}
      officeName={meta?.officeName}
      title={title}
      pending={state.pending}
    />
  );
  const hasDescription = !!(meta?.description || meta?.officeName || state.pending);
  return { meta, state, shortcut, title, description: hasDescription ? description : undefined };
}

const ICON_SIZE: Record<RibbonItemSize, number> = { large: 24, medium: 16, small: 17 };

/** One ribbon control, bound to its command's state. Returns null when the command is hidden. */
export function RibbonItemView({ item, size, layout, keyTip, keyTipLevel }: RibbonItemViewProps) {
  const { registry } = useCommands();
  const command = 'command' in item ? item.command : undefined;
  const tip = useTip(command, 'label' in item ? item.label : undefined);
  const execute = useExecute();
  if (item.kind === 'spacer') return <span className="kiln-ribbon__spacer" aria-hidden="true" />;
  if (!isLeafVisible(registry, item)) return null;

  const enabled = command ? tip.state.enabled : true;
  const iconName = ('icon' in item ? item.icon : undefined) ?? tip.meta?.icon;
  const showLabel = size !== 'small';
  const kt = { 'data-keytip': keyTip, 'data-keytip-level': keyTipLevel };
  const run = (args?: unknown) => {
    if (command && enabled) execute(command, args);
  };

  switch (item.kind) {
    case 'button':
    case 'toggle': {
      const pressed =
        item.kind !== 'toggle'
          ? undefined
          : item.checkedValue !== undefined
            ? tip.state.value === item.checkedValue
            : !!tip.state.checked;
      return (
        <Tooltip content={tip.title} shortcut={tip.shortcut} description={tip.description}>
          <button
            type="button"
            className={cx(
              'kiln-btn kiln-btn--toolbar kiln-ribbon__btn',
              `kiln-ribbon__btn--${size}`,
              size === 'large' && 'kiln-btn--stacked',
              !showLabel && 'kiln-btn--icon-only',
              item.kind === 'toggle' && 'kiln-toggle',
            )}
            aria-label={showLabel ? undefined : tip.title}
            aria-pressed={pressed}
            aria-keyshortcuts={tip.shortcut ? ariaKeyShortcuts(tip.shortcut) : undefined}
            data-state={pressed === undefined ? undefined : pressed ? 'on' : 'off'}
            aria-disabled={!enabled || undefined}
            data-command={command}
            {...kt}
            onClick={() => run(item.args)}
          >
            <span className="kiln-btn__icon">
              <RibbonIcon name={iconName} size={ICON_SIZE[size]} />
            </span>
            {showLabel ? <span className="kiln-btn__label">{tip.title}</span> : null}
          </button>
        </Tooltip>
      );
    }
    case 'split':
      return (
        <span
          className={cx('kiln-ribbon__split', `kiln-ribbon__split--${size}`)}
          {...kt}
          data-command={command}
        >
          <SplitButton
            label={tip.title}
            iconOnly={!showLabel}
            icon={<RibbonIcon name={iconName} size={ICON_SIZE[size]} />}
            layout={size === 'large' ? 'stacked' : 'inline'}
            shortcut={tip.shortcut}
            description={tip.description}
            disabled={!enabled}
            onClick={() => run(item.args)}
          >
            <CommandMenuItems entries={item.menu} />
          </SplitButton>
        </span>
      );
    case 'dropdown': {
      const label = item.label ?? tip.meta?.title ?? 'More';
      return (
        <Menu modal={false}>
          <Tooltip content={label} shortcut={tip.shortcut} description={tip.description}>
            <MenuTrigger asChild disabled={!!command && !tip.state.exists}>
              <button
                type="button"
                className={cx(
                  'kiln-btn kiln-btn--toolbar kiln-ribbon__btn kiln-ribbon__dropdown',
                  `kiln-ribbon__btn--${size}`,
                  size === 'large' && 'kiln-btn--stacked',
                  !showLabel && 'kiln-btn--icon-only',
                )}
                aria-label={showLabel ? undefined : label}
                data-command={command}
                {...kt}
              >
                <span className="kiln-btn__icon">
                  <RibbonIcon name={iconName} size={ICON_SIZE[size]} />
                </span>
                {showLabel ? <span className="kiln-btn__label">{label}</span> : null}
                <span className="kiln-btn__icon-after">
                  <Icon icon={ChevronDown} size={12} />
                </span>
              </button>
            </MenuTrigger>
          </Tooltip>
          <MenuContent>
            <CommandMenuItems entries={item.menu} />
          </MenuContent>
        </Menu>
      );
    }
    case 'gallery':
      return (
        <div className="kiln-ribbon__gallery" data-command={command} {...kt}>
          <Gallery
            label={tip.title}
            items={item.items}
            value={typeof tip.state.value === 'string' ? tip.state.value : undefined}
            onSelect={(id) => run({ value: id })}
            onPreview={
              item.previewCommand
                ? (id) => {
                    void registry.execute(item.previewCommand as string, { value: id });
                  }
                : undefined
            }
            columns={layout === 'simplified' ? Math.min(item.columns ?? 4, 4) : (item.columns ?? 5)}
            expandedColumns={item.columns ?? 5}
            itemWidth={item.itemWidth ?? (layout === 'simplified' ? 76 : 80)}
            itemHeight={layout === 'simplified' ? 30 : (item.itemHeight ?? 60)}
            footer={
              item.footer ? (
                <div className="kiln-ribbon__gallery-footer">
                  <GalleryFooter entries={item.footer} />
                </div>
              ) : undefined
            }
          />
        </div>
      );
    case 'combobox':
      return (
        <Tooltip content={tip.title} shortcut={tip.shortcut} description={tip.description}>
          <span className="kiln-ribbon__field" data-command={command} {...kt}>
            <Combobox
              aria-label={tip.title}
              value={tip.state.value == null ? '' : String(tip.state.value)}
              onValueChange={(value) => run({ value })}
              options={item.options}
              allowCustomValue={item.allowCustomValue ?? true}
              parse={item.parse}
              disabled={!enabled}
              width={item.width ?? 120}
              placeholder={item.placeholder}
              size="sm"
            />
          </span>
        </Tooltip>
      );
    case 'color':
      return (
        <ColorItem
          item={item}
          size={size}
          enabled={enabled}
          tip={tip}
          iconName={iconName}
          keyTip={keyTip}
          keyTipLevel={keyTipLevel}
        />
      );
    case 'numberinput':
      return (
        <span className="kiln-ribbon__number" data-command={command} {...kt}>
          {showLabel ? (
            <span className="kiln-ribbon__number-label" aria-hidden="true">
              <RibbonIcon name={iconName} size={16} />
              {tip.title}
            </span>
          ) : null}
          <NumberInput
            aria-label={tip.title}
            value={typeof tip.state.value === 'number' ? tip.state.value : 0}
            onValueChange={(value) => run({ value })}
            unit={item.unit}
            min={item.min}
            max={item.max}
            step={item.step}
            disabled={!enabled}
            width={item.width ?? 76}
            size="sm"
          />
        </span>
      );
    case 'custom':
      return (
        <span className="kiln-ribbon__custom" data-ribbon-custom={item.id} {...kt}>
          {item.render({ layout, context: registry.context })}
        </span>
      );
    default:
      return null;
  }
}

function GalleryFooter({ entries }: { entries: RibbonGalleryFooterEntries }) {
  // Footer entries are rendered as plain buttons (the expanded gallery is a popover, not a menu).
  const execute = useExecute();
  const { registry } = useCommands();
  return (
    <>
      {entries.map((e, i) =>
        'command' in e ? (
          <button
            key={i}
            type="button"
            className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-ribbon__gallery-action"
            onClick={() => execute(e.command, e.args)}
          >
            <RibbonIcon name={e.icon ?? registry.get(e.command)?.icon} size={16} />
            <span className="kiln-btn__label">{e.label ?? registry.get(e.command)?.title ?? e.command}</span>
          </button>
        ) : null,
      )}
    </>
  );
}

type RibbonGalleryFooterEntries = NonNullable<Extract<RibbonLeafItem, { kind: 'gallery' }>['footer']>;

function colorHex(c: ColorChoice | undefined, fallback: string): string {
  if (!c) return fallback;
  if (c.type === 'rgb' || c.type === 'theme') return c.hex;
  if (c.type === 'auto') return 'var(--kiln-fg)';
  return 'transparent';
}

function ColorItem({
  item,
  size,
  enabled,
  tip,
  iconName,
  keyTip,
  keyTipLevel,
}: {
  item: Extract<RibbonLeafItem, { kind: 'color' }>;
  size: RibbonItemSize;
  enabled: boolean;
  tip: ReturnType<typeof useTip>;
  iconName: string | undefined;
  keyTip?: string;
  keyTipLevel?: string;
}) {
  const execute = useExecute();
  const [open, setOpen] = useState(false);
  const fallback = item.defaultColor ?? '#C00000';
  const [last, setLast] = useState<ColorChoice>({ type: 'rgb', hex: fallback });
  const current = (tip.state.value as ColorChoice | undefined) ?? undefined;
  const apply = (choice: ColorChoice) => {
    setLast(choice);
    setOpen(false);
    if (enabled) execute(item.command, { color: choice });
  };
  const icon: ReactNode = (
    <span className="kiln-ribbon__color-icon">
      <RibbonIcon name={iconName} size={size === 'large' ? 32 : 20} />
      <span className="kiln-ribbon__color-bar" style={{ background: colorHex(last, fallback) }} />
    </span>
  );
  return (
    <span
      className={cx('kiln-ribbon__split', `kiln-ribbon__split--${size}`)}
      data-keytip={keyTip}
      data-keytip-level={keyTipLevel}
      data-command={item.command}
    >
      <SplitButton
        label={tip.title}
        iconOnly={size !== 'large' && size !== 'medium'}
        icon={icon}
        panel
        open={open}
        onOpenChange={setOpen}
        shortcut={tip.shortcut}
        description={tip.description}
        disabled={!enabled}
        layout={size === 'large' ? 'stacked' : 'inline'}
        onClick={() => apply(last)}
      >
        <ColorPicker value={current} onChange={apply} allowAuto={item.allowAuto} allowNone={item.allowNone} />
      </SplitButton>
    </span>
  );
}
