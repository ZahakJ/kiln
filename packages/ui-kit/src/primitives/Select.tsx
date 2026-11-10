import * as RS from '@radix-ui/react-select';
import { Check, ChevronDown, ChevronUp } from 'lucide-react';
import type { ReactNode } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from './cx.ts';

export interface SelectOption {
  value: string;
  label: ReactNode;
  /** Plain-text label for typeahead when `label` is not a string. */
  textValue?: string;
  icon?: ReactNode;
  disabled?: boolean;
}

export interface SelectGroupDef {
  label: string;
  options: SelectOption[];
}

export interface SelectProps {
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  /** Flat options or labelled groups (separated by hairlines). */
  options: Array<SelectOption | SelectGroupDef>;
  /** Accessible name (visible labels should use `aria-labelledby` via `labelId`). */
  'aria-label'?: string;
  labelId?: string;
  placeholder?: string;
  disabled?: boolean;
  /** Trigger width in px or CSS length. */
  width?: number | string;
  size?: 'sm' | 'md';
  className?: string;
  id?: string;
  name?: string;
}

const isGroup = (o: SelectOption | SelectGroupDef): o is SelectGroupDef => 'options' in o;

function Item({ option }: { option: SelectOption }) {
  return (
    <RS.Item
      className="kiln-menu__item kiln-select__item"
      value={option.value}
      disabled={option.disabled}
      textValue={option.textValue ?? (typeof option.label === 'string' ? option.label : option.value)}
    >
      <span className="kiln-menu__indicator">
        <RS.ItemIndicator>
          <Icon icon={Check} size={14} />
        </RS.ItemIndicator>
      </span>
      {option.icon ? (
        <span className="kiln-menu__icon" aria-hidden="true">
          {option.icon}
        </span>
      ) : null}
      <RS.ItemText>{option.label}</RS.ItemText>
    </RS.Item>
  );
}

/** Non-editable drop-down list (`combobox` + `listbox` per WAI-ARIA, via Radix Select). */
export function Select({
  value,
  defaultValue,
  onValueChange,
  options,
  placeholder,
  disabled,
  width,
  size = 'md',
  className,
  labelId,
  id,
  name,
  ...aria
}: SelectProps) {
  return (
    <RS.Root
      value={value}
      defaultValue={defaultValue}
      onValueChange={onValueChange}
      disabled={disabled}
      name={name}
    >
      <RS.Trigger
        id={id}
        className={cx('kiln-field', 'kiln-select', `kiln-field--${size}`, className)}
        style={width !== undefined ? { width } : undefined}
        aria-label={aria['aria-label']}
        aria-labelledby={labelId}
      >
        <span className="kiln-select__value">
          <RS.Value placeholder={placeholder} />
        </span>
        <RS.Icon className="kiln-select__chevron">
          <Icon icon={ChevronDown} size={14} />
        </RS.Icon>
      </RS.Trigger>
      <RS.Portal>
        <RS.Content
          className="kiln-menu kiln-select__content"
          position="popper"
          sideOffset={4}
          collisionPadding={8}
        >
          <RS.ScrollUpButton className="kiln-select__scroll">
            <Icon icon={ChevronUp} size={14} />
          </RS.ScrollUpButton>
          <RS.Viewport className="kiln-select__viewport">
            {options.map((o, i) =>
              isGroup(o) ? (
                <RS.Group key={o.label}>
                  {i > 0 ? <RS.Separator className="kiln-menu__separator" /> : null}
                  <RS.Label className="kiln-menu__group-label">{o.label}</RS.Label>
                  {o.options.map((opt) => (
                    <Item key={opt.value} option={opt} />
                  ))}
                </RS.Group>
              ) : (
                <Item key={o.value} option={o} />
              ),
            )}
          </RS.Viewport>
          <RS.ScrollDownButton className="kiln-select__scroll">
            <Icon icon={ChevronDown} size={14} />
          </RS.ScrollDownButton>
        </RS.Content>
      </RS.Portal>
    </RS.Root>
  );
}
