/** `styles.xml` → `StyleSheet` (ECMA-376 Part 1 §17.7). */
import {
  type Style,
  StyleSheet,
  type StyleType,
  type TableConditionType,
  type TableProps,
  type TableStyleCondition,
} from '@kiln-ref/doc-model';
import { child, children, type XmlElement } from '@kiln-ref/ooxml';
import { parseOnOff } from '../common/units.ts';
import { attr } from '../common/xml.ts';
import {
  parseCellProps,
  parseParagraphProps,
  parseRowProps,
  parseRunProps,
  parseTableProps,
  type RawXml,
} from './props.ts';

const TYPES = new Set<string>(['paragraph', 'character', 'table', 'numbering']);
const CONDITIONS = new Set<string>([
  'wholeTable',
  'firstRow',
  'lastRow',
  'firstCol',
  'lastCol',
  'band1Vert',
  'band2Vert',
  'band1Horz',
  'band2Horz',
  'neCell',
  'nwCell',
  'seCell',
  'swCell',
]);

const val = (n: XmlElement | undefined) => attr(n, 'w:val');
const flag = (n: XmlElement | undefined) => (n ? parseOnOff(val(n)) : undefined);

function nonEmpty<T extends object>(o: T): T | undefined {
  return Object.keys(o).length ? o : undefined;
}

function parseStyle(node: XmlElement, raw: RawXml): Style | undefined {
  const id = attr(node, 'w:styleId');
  if (!id) return undefined;
  const typeAttr = attr(node, 'w:type') ?? 'paragraph';
  const type = (TYPES.has(typeAttr) ? typeAttr : 'paragraph') as StyleType;
  const style: Style = { id, type, name: val(child(node, 'w:name')) ?? id };
  const basedOn = val(child(node, 'w:basedOn'));
  if (basedOn) style.basedOn = basedOn;
  const next = val(child(node, 'w:next'));
  if (next) style.next = next;
  const link = val(child(node, 'w:link'));
  if (link) style.link = link;
  if (attr(node, 'w:default') !== undefined && parseOnOff(attr(node, 'w:default'))) style.isDefault = true;
  if (attr(node, 'w:customStyle') !== undefined && parseOnOff(attr(node, 'w:customStyle')))
    style.custom = true;
  if (flag(child(node, 'w:qFormat'))) style.quickStyle = true;
  const prio = val(child(node, 'w:uiPriority'));
  if (prio !== undefined && /^\d+$/.test(prio)) style.uiPriority = Number(prio);
  if (flag(child(node, 'w:semiHidden'))) style.semiHidden = true;
  if (flag(child(node, 'w:unhideWhenUsed'))) style.unhideWhenUsed = true;
  if (flag(child(node, 'w:hidden'))) style.hidden = true;
  if (flag(child(node, 'w:locked'))) style.locked = true;
  const aliases = val(child(node, 'w:aliases'));
  if (aliases)
    style.aliases = aliases
      .split(',')
      .map((a) => a.trim())
      .filter(Boolean);

  const pPr = child(node, 'w:pPr');
  if (pPr) style.pPr = parseParagraphProps(pPr, raw).props;
  const rPr = child(node, 'w:rPr');
  if (rPr) style.rPr = parseRunProps(rPr, raw).props;
  if (type === 'table') {
    const tblPr = child(node, 'w:tblPr');
    if (tblPr) {
      const t = parseTableProps(tblPr, raw);
      style.tblPr = t.xml.length ? ({ ...t.props, xml: t.xml } as TableProps) : t.props;
    }
    const trPr = child(node, 'w:trPr');
    if (trPr) style.trPr = parseRowProps(trPr, raw).props;
    const tcPr = child(node, 'w:tcPr');
    if (tcPr) style.tcPr = parseCellProps(tcPr, raw).props;
    const conditions: Partial<Record<TableConditionType, TableStyleCondition>> = {};
    for (const c of children(node, 'w:tblStylePr')) {
      const t = attr(c, 'w:type');
      if (!t || !CONDITIONS.has(t)) continue;
      const cond: TableStyleCondition = {};
      const cp = child(c, 'w:pPr');
      if (cp) cond.pPr = parseParagraphProps(cp, raw).props;
      const cr = child(c, 'w:rPr');
      if (cr) cond.rPr = parseRunProps(cr, raw).props;
      const ct = child(c, 'w:tblPr');
      if (ct) cond.tblPr = parseTableProps(ct, raw).props;
      const crow = child(c, 'w:trPr');
      if (crow) cond.trPr = parseRowProps(crow, raw).props;
      const cc = child(c, 'w:tcPr');
      if (cc) cond.tcPr = parseCellProps(cc, raw).props;
      conditions[t as TableConditionType] = cond;
    }
    const conds = nonEmpty(conditions);
    if (conds) style.conditions = conds;
  }
  return style;
}

export interface ParsedStyles {
  styles: StyleSheet;
  latentStyles?: string;
}

/** Parses the root of `styles.xml`. */
export function parseStyles(root: XmlElement, raw: RawXml): ParsedStyles {
  const defaults = child(root, 'w:docDefaults');
  const pPr = parseParagraphProps(child(child(defaults, 'w:pPrDefault'), 'w:pPr'), raw).props;
  const rPr = parseRunProps(child(child(defaults, 'w:rPrDefault'), 'w:rPr'), raw).props;
  const sheet = new StyleSheet({ pPr, rPr });
  for (const s of children(root, 'w:style')) {
    const style = parseStyle(s, raw);
    // Duplicate ids: Word keeps the first definition.
    if (style && !sheet.has(style.id)) sheet.add(style);
  }
  const latent = child(root, 'w:latentStyles');
  return latent ? { styles: sheet, latentStyles: raw(latent) } : { styles: sheet };
}
