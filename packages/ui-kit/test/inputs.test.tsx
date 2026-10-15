import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import {
  Combobox,
  type ComboboxOption,
  convertLength,
  NumberInput,
  parseLength,
  Select,
  Slider,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  ZoomSlider,
  zoomToPosition,
} from '../src/index.ts';

const FONTS: ComboboxOption[] = ['Arial', 'Caladea', 'Calibri', 'Cambria', 'Carlito', 'Courier New'].map(
  (value) => ({
    value,
  }),
);

function FontBox(props: {
  onChange?: (v: string) => void;
  autocomplete?: boolean;
  parse?: (t: string) => string | null;
}) {
  const [v, setV] = useState('Calibri');
  return (
    <Combobox
      aria-label="Font"
      value={v}
      options={FONTS}
      autocomplete={props.autocomplete}
      parse={props.parse}
      onValueChange={(x) => {
        setV(x);
        props.onChange?.(x);
      }}
    />
  );
}

describe('Combobox', () => {
  it('filters while typing, completes inline and commits with Enter', async () => {
    const onChange = vi.fn();
    render(<FontBox onChange={onChange} />);
    const input = screen.getByRole('combobox', { name: 'Font' }) as HTMLInputElement;
    await userEvent.click(input);
    await userEvent.keyboard('{Control>}a{/Control}Cam');
    expect(input.value).toBe('Cambria');
    expect(input.getAttribute('aria-expanded')).toBe('true');
    const options = screen.getAllByRole('option');
    expect(options.map((o) => o.textContent)).toEqual(['Cambria']);
    await userEvent.keyboard('{Enter}');
    expect(onChange).toHaveBeenLastCalledWith('Cambria');
    expect(input.getAttribute('aria-expanded')).toBe('false');
  });

  it('moves the active option with arrows (aria-activedescendant) and Escape reverts', async () => {
    const onChange = vi.fn();
    render(<FontBox onChange={onChange} />);
    const input = screen.getByRole('combobox') as HTMLInputElement;
    input.focus();
    await userEvent.keyboard('{Alt>}{ArrowDown}{/Alt}');
    const active = () => document.getElementById(input.getAttribute('aria-activedescendant') ?? '');
    expect(active()?.textContent).toBe('Calibri');
    await userEvent.keyboard('{ArrowDown}');
    expect(active()?.textContent).toBe('Cambria');
    expect(input.value).toBe('Cambria');
    await userEvent.keyboard('{Escape}');
    expect(input.getAttribute('aria-expanded')).toBe('false');
    await userEvent.keyboard('{Escape}');
    expect(input.value).toBe('Calibri');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('accepts custom values through parse and rejects invalid ones', async () => {
    const onChange = vi.fn();
    const parse = (t: string) => (/^\d+(\.5)?$/.test(t) ? t : null);
    render(<FontBox onChange={onChange} autocomplete={false} parse={parse} />);
    const input = screen.getByRole('combobox') as HTMLInputElement;
    await userEvent.click(input);
    await userEvent.keyboard('{Control>}a{/Control}10.5{Enter}');
    expect(onChange).toHaveBeenLastCalledWith('10.5');
    await userEvent.keyboard('{Control>}a{/Control}abc{Enter}');
    expect(input.value).toBe('10.5');
  });

  it('commits a clicked option', async () => {
    const onChange = vi.fn();
    render(<FontBox onChange={onChange} />);
    await userEvent.click(screen.getByRole('button', { name: 'Show Font list' }));
    await userEvent.click(screen.getByRole('option', { name: 'Arial' }));
    expect(onChange).toHaveBeenCalledWith('Arial');
  });
});

describe('NumberInput', () => {
  it('steps with arrows and PageUp, clamps, and exposes spinbutton values', async () => {
    const onChange = vi.fn();
    function Box() {
      const [v, setV] = useState(1);
      return (
        <NumberInput
          aria-label="Indent"
          value={v}
          unit="cm"
          min={0}
          max={2}
          onValueChange={(x) => {
            setV(x);
            onChange(x);
          }}
        />
      );
    }
    render(<Box />);
    const input = screen.getByRole('spinbutton', { name: 'Indent' }) as HTMLInputElement;
    expect(input.value).toBe('1 cm');
    input.focus();
    await userEvent.keyboard('{ArrowUp}');
    expect(onChange).toHaveBeenLastCalledWith(1.1);
    await userEvent.keyboard('{PageUp}');
    expect(onChange).toHaveBeenLastCalledWith(2);
    expect(input.getAttribute('aria-valuenow')).toBe('2');
    await userEvent.keyboard('{Home}');
    expect(onChange).toHaveBeenLastCalledWith(0);
  });

  it('converts typed units and reverts invalid input with Escape', async () => {
    const onChange = vi.fn();
    render(<NumberInput aria-label="Width" value={1} unit="in" onValueChange={onChange} />);
    const input = screen.getByRole('spinbutton') as HTMLInputElement;
    await userEvent.click(input);
    await userEvent.keyboard('{Control>}a{/Control}5.08 cm{Enter}');
    expect(onChange).toHaveBeenLastCalledWith(2);
    expect(input.value).toBe('2 in');
    await userEvent.keyboard('{Control>}a{/Control}72pt{Enter}');
    expect(input.value).toBe('1 in');
    await userEvent.keyboard('{Control>}a{/Control}5 furlongs{Enter}');
    expect(input.getAttribute('aria-invalid')).toBe('true');
    await userEvent.keyboard('{Escape}');
    expect(input.value).toBe('1 in');
  });

  it('parses and converts length units', () => {
    expect(parseLength('2,5 cm', 'cm')).toBe(2.5);
    expect(parseLength('1"', 'pt')).toBe(72);
    expect(parseLength('96px', 'in')).toBeCloseTo(1);
    expect(parseLength('abc', 'pt')).toBeNull();
    expect(convertLength(1, 'in', 'cm')).toBeCloseTo(2.54);
  });
});

describe('Select', () => {
  it('opens a listbox and selects with the keyboard', async () => {
    const onChange = vi.fn();
    render(
      <Select
        aria-label="Line spacing"
        defaultValue="1.0"
        onValueChange={onChange}
        options={[
          { value: '1.0', label: '1.0' },
          { value: '1.15', label: '1.15' },
          { value: '1.5', label: '1.5' },
        ]}
      />,
    );
    const trigger = screen.getByRole('combobox', { name: 'Line spacing' });
    expect(trigger.textContent).toContain('1.0');
    trigger.focus();
    await userEvent.keyboard('{Enter}');
    expect(await screen.findByRole('listbox')).toBeTruthy();
    await userEvent.keyboard('{ArrowDown}{Enter}');
    expect(onChange).toHaveBeenCalledWith('1.15');
  });
});

describe('Slider and ZoomSlider', () => {
  it('slider responds to arrows, PageUp and Home/End', () => {
    const onChange = vi.fn();
    render(
      <Slider aria-label="Transparency" value={50} onValueChange={onChange} formatValue={(v) => `${v}%`} />,
    );
    const s = screen.getByRole('slider', { name: 'Transparency' });
    expect(s.getAttribute('aria-valuetext')).toBe('50%');
    fireEvent.keyDown(s, { key: 'ArrowRight' });
    expect(onChange).toHaveBeenLastCalledWith(51);
    fireEvent.keyDown(s, { key: 'PageDown' });
    expect(onChange).toHaveBeenLastCalledWith(40);
    fireEvent.keyDown(s, { key: 'End' });
    expect(onChange).toHaveBeenLastCalledWith(100);
  });

  it('zoom maps 100 % to the middle and steps through Office zoom levels', async () => {
    expect(zoomToPosition(100)).toBe(0.5);
    expect(zoomToPosition(10)).toBe(0);
    expect(zoomToPosition(500)).toBe(1);
    const onChange = vi.fn();
    render(<ZoomSlider value={100} onValueChange={onChange} />);
    await userEvent.click(screen.getByRole('button', { name: 'Zoom in' }));
    expect(onChange).toHaveBeenLastCalledWith(110);
    await userEvent.click(screen.getByRole('button', { name: 'Zoom out' }));
    expect(onChange).toHaveBeenLastCalledWith(90);
  });
});

describe('Tabs', () => {
  it('moves between tabs with arrow keys and shows the matching panel', async () => {
    render(
      <Tabs defaultValue="home">
        <TabsList aria-label="Ribbon">
          <TabsTrigger value="home">Home</TabsTrigger>
          <TabsTrigger value="insert">Insert</TabsTrigger>
        </TabsList>
        <TabsContent value="home">Home panel</TabsContent>
        <TabsContent value="insert">Insert panel</TabsContent>
      </Tabs>,
    );
    const home = screen.getByRole('tab', { name: 'Home' });
    expect(home.getAttribute('aria-selected')).toBe('true');
    act(() => home.focus());
    await userEvent.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Insert' }).getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('tabpanel').textContent).toBe('Insert panel');
  });
});
