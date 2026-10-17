/**
 * A colour field of the Grid dialogs: a button showing the chosen colour that opens the palette
 * (Excel's colour drop-downs in Format Cells), so the dialog stays compact.
 */
import { type ColorChoice, ColorPicker, Icon, Popover, PopoverContent, PopoverTrigger } from '@kiln/ui-kit';
import { ChevronDown } from 'lucide-react';
import { useId, useState } from 'react';

export interface ColorButtonProps {
  label: string;
  /** `RRGGBB`, or null for Automatic / No Color. */
  value: string | null;
  allowAuto?: boolean;
  allowNone?: boolean;
  noneLabel?: string;
  onChange: (choice: ColorChoice) => void;
}

/** The `RRGGBB` of a choice (null for Automatic and No Color). */
export const hexOfChoice = (c: ColorChoice): string | null =>
  c.type === 'rgb' || c.type === 'theme' ? c.hex.replace('#', '').toUpperCase() : null;

export function ColorButton({
  label,
  value,
  allowAuto,
  allowNone,
  noneLabel = 'No Color',
  onChange,
}: ColorButtonProps) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const empty = allowAuto ? 'Automatic' : noneLabel;
  return (
    <span className="kiln-fc__field">
      <span className="kiln-fc__label" id={`${id}-label`}>
        {label}
      </span>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button type="button" className="kiln-colorbtn" id={id} aria-labelledby={`${id}-label ${id}`}>
            <span
              className={`kiln-colorbtn__swatch${value ? '' : ' kiln-colorbtn__swatch--empty'}`}
              style={value ? { background: `#${value}` } : undefined}
              aria-hidden="true"
            />
            <span className="kiln-colorbtn__text">{value ? `#${value}` : empty}</span>
            <Icon icon={ChevronDown} size={14} />
          </button>
        </PopoverTrigger>
        <PopoverContent align="start">
          <ColorPicker
            allowAuto={allowAuto}
            allowNone={allowNone}
            noneLabel={noneLabel}
            onChange={(c) => {
              onChange(c);
              setOpen(false);
            }}
          />
        </PopoverContent>
      </Popover>
    </span>
  );
}
