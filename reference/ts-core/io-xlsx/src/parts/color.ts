/** SpreadsheetML `CT_Color` ↔ the model's {@link Color}. */
import { attrsFrom, el, type XmlElement } from '@kiln-ref/ooxml';
import type { Color } from '@kiln-ref/sheet-model';
import { formatNumber, parseNum } from '../util/text.ts';

export function readColor(e: XmlElement | undefined): Color | undefined {
  if (!e) return undefined;
  const a = e.attrs;
  const tint = parseNum(a.tint);
  let c: Color | undefined;
  if (a.auto === '1' || a.auto === 'true') return { auto: true };
  if (a.rgb !== undefined) {
    const h = a.rgb.toUpperCase();
    c = { rgb: h.length === 6 ? `FF${h}` : h };
  } else if (a.theme !== undefined) c = { theme: Number(a.theme) };
  else if (a.indexed !== undefined) c = { indexed: Number(a.indexed) };
  else return undefined;
  if (tint !== undefined && tint !== 0) c.tint = tint;
  return c;
}

export function colorAttrs(c: Color): Record<string, string> {
  if ('auto' in c) return attrsFrom({ auto: '1' });
  const out: Record<string, string | undefined> = {};
  if ('indexed' in c) out.indexed = String(c.indexed);
  else if ('rgb' in c) out.rgb = c.rgb;
  else if ('theme' in c) out.theme = String(c.theme);
  if (c.tint !== undefined && c.tint !== 0) out.tint = formatNumber(c.tint);
  return attrsFrom(out);
}

export function writeColor(name: string, c: Color | undefined): XmlElement | null {
  if (!c) return null;
  return el(name, colorAttrs(c));
}
