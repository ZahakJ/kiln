/** `settings.xml` → `DocxSettings` (ECMA-376 Part 1 §17.15). */
import { DEFAULT_SETTINGS } from '@officina/doc-model';
import { child, children, type XmlElement } from '@officina/ooxml';
import type { DocxSettings, NoteProps } from '../common/types.ts';
import { parseDecimal, parseOnOff, parseTwips } from '../common/units.ts';
import { attr } from '../common/xml.ts';

const val = (n: XmlElement | undefined) => attr(n, 'w:val');
const on = (n: XmlElement | undefined) => (n ? parseOnOff(val(n)) : false);

/** `w:footnotePr` / `w:endnotePr` (shared by settings and section properties). */
export function parseNoteProps(node: XmlElement | undefined): NoteProps | undefined {
  if (!node) return undefined;
  const p: NoteProps = {};
  const pos = val(child(node, 'w:pos'));
  if (pos) p.position = pos;
  const fmt = val(child(node, 'w:numFmt'));
  if (fmt) p.numFmt = fmt;
  const start = parseDecimal(val(child(node, 'w:numStart')));
  if (start !== undefined) p.numStart = start;
  const restart = val(child(node, 'w:numRestart'));
  if (restart === 'continuous' || restart === 'eachSect' || restart === 'eachPage') p.numRestart = restart;
  return Object.keys(p).length ? p : undefined;
}

/** Parses the root of `settings.xml`. Missing values take Word's defaults. */
export function parseSettings(root: XmlElement | undefined): DocxSettings {
  const s: DocxSettings = { ...DEFAULT_SETTINGS };
  if (!root) return s;
  const tab = parseTwips(val(child(root, 'w:defaultTabStop')));
  if (tab !== undefined && tab > 0) s.defaultTabStop = tab;
  s.evenAndOddHeaders = on(child(root, 'w:evenAndOddHeaders'));
  s.mirrorMargins = on(child(root, 'w:mirrorMargins'));
  s.trackRevisions = on(child(root, 'w:trackRevisions'));
  if (child(root, 'w:updateFields')) s.updateFields = on(child(root, 'w:updateFields'));
  if (child(root, 'w:doNotTrackMoves')) s.doNotTrackMoves = on(child(root, 'w:doNotTrackMoves'));
  if (child(root, 'w:doNotTrackFormatting'))
    s.doNotTrackFormatting = on(child(root, 'w:doNotTrackFormatting'));
  if (child(root, 'w:autoHyphenation')) s.autoHyphenation = on(child(root, 'w:autoHyphenation'));

  const decimal = val(child(root, 'w:decimalSymbol'));
  if (decimal !== undefined) s.decimalSymbol = decimal;
  const list = val(child(root, 'w:listSeparator'));
  if (list !== undefined) s.listSeparator = list;

  const mapping = child(root, 'w:clrSchemeMapping');
  if (mapping) {
    const m: Record<string, string> = {};
    for (const [k, v] of Object.entries(mapping.attrs)) {
      if (k.startsWith('xmlns')) continue;
      m[k.slice(k.indexOf(':') + 1)] = v;
    }
    if (Object.keys(m).length) s.colorSchemeMapping = m;
  }

  let mode: number | undefined;
  for (const cs of children(child(root, 'w:compat'), 'w:compatSetting')) {
    if (attr(cs, 'w:name') === 'compatibilityMode') mode = parseDecimal(attr(cs, 'w:val'));
  }
  // No compatSetting means a Word 2007 (mode 12) or older document.
  s.compatibilityMode = mode ?? (child(root, 'w:compat') ? 12 : DEFAULT_SETTINGS.compatibilityMode);

  const fn = parseNoteProps(child(root, 'w:footnotePr'));
  if (fn) s.footnotePr = fn;
  const en = parseNoteProps(child(root, 'w:endnotePr'));
  if (en) s.endnotePr = en;

  const lang = child(root, 'w:themeFontLang');
  if (lang) {
    const l: NonNullable<DocxSettings['themeFontLang']> = {};
    const v = val(lang);
    const ea = attr(lang, 'w:eastAsia');
    const bidi = attr(lang, 'w:bidi');
    if (v) l.val = v;
    if (ea) l.eastAsia = ea;
    if (bidi) l.bidi = bidi;
    s.themeFontLang = l;
  }

  const prot = child(root, 'w:documentProtection');
  if (prot) {
    const edit = attr(prot, 'w:edit');
    s.protection = { enforced: parseOnOff(attr(prot, 'w:enforcement'), false), ...(edit ? { edit } : {}) };
  }
  return s;
}
