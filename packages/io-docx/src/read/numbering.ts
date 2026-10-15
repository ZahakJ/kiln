/** `numbering.xml` → `Numbering` (ECMA-376 Part 1 §17.9). */
import {
  type AbstractNumbering,
  type LevelOverride,
  Numbering,
  type NumberingInstance,
  type NumberingLevel,
} from '@officina/doc-model';
import { child, children, type XmlElement } from '@officina/ooxml';
import { parseDecimal, parseOnOff } from '../common/units.ts';
import { attr } from '../common/xml.ts';
import { parseParagraphProps, parseRunProps, type RawXml } from './props.ts';

const val = (n: XmlElement | undefined) => attr(n, 'w:val');

function parseLevel(node: XmlElement, raw: RawXml): NumberingLevel | undefined {
  const level = parseDecimal(attr(node, 'w:ilvl'));
  if (level === undefined || level < 0 || level > 8) return undefined;
  const numFmt = child(node, 'w:numFmt');
  const lvlText = child(node, 'w:lvlText');
  const lvl: NumberingLevel = {
    level,
    // ECMA-376 §17.9.26: an omitted w:start means the level starts at zero.
    start: parseDecimal(val(child(node, 'w:start'))) ?? 0,
    format: val(numFmt) ?? 'decimal',
    // w:lvlText absent = no label text; w:null="1" = explicitly empty.
    text: lvlText && !parseOnOff(attr(lvlText, 'w:null'), false) ? (val(lvlText) ?? '') : '',
  };
  const custom = attr(numFmt, 'w:format');
  if (custom) lvl.customFormat = custom;
  const restart = child(node, 'w:lvlRestart');
  if (restart) {
    const n = parseDecimal(val(restart));
    if (n !== undefined) lvl.restartAfter = n;
  }
  const isLgl = child(node, 'w:isLgl');
  if (isLgl && parseOnOff(val(isLgl))) lvl.isLegal = true;
  const suff = val(child(node, 'w:suff'));
  if (suff === 'tab' || suff === 'space' || suff === 'nothing') lvl.suffix = suff;
  const jc = val(child(node, 'w:lvlJc'));
  if (jc === 'center') lvl.alignment = 'center';
  else if (jc === 'right' || jc === 'end') lvl.alignment = 'right';
  else if (jc === 'left' || jc === 'start') lvl.alignment = 'left';
  const pStyle = val(child(node, 'w:pStyle'));
  if (pStyle) lvl.pStyle = pStyle;
  const pic = parseDecimal(val(child(node, 'w:lvlPicBulletId')));
  if (pic !== undefined) lvl.picBulletId = pic;
  const pPr = child(node, 'w:pPr');
  if (pPr) lvl.pPr = parseParagraphProps(pPr, raw).props;
  const rPr = child(node, 'w:rPr');
  if (rPr) lvl.rPr = parseRunProps(rPr, raw).props;
  return lvl;
}

function parseAbstract(node: XmlElement, raw: RawXml): AbstractNumbering | undefined {
  const id = parseDecimal(attr(node, 'w:abstractNumId'));
  if (id === undefined) return undefined;
  const abs: AbstractNumbering = { id, levels: [] };
  const nsid = val(child(node, 'w:nsid'));
  if (nsid) abs.nsid = nsid;
  const mlt = val(child(node, 'w:multiLevelType'));
  if (mlt === 'singleLevel' || mlt === 'multilevel' || mlt === 'hybridMultilevel') abs.multiLevelType = mlt;
  const name = val(child(node, 'w:name'));
  if (name) abs.name = name;
  const styleLink = val(child(node, 'w:styleLink'));
  if (styleLink) abs.styleLink = styleLink;
  const numStyleLink = val(child(node, 'w:numStyleLink'));
  if (numStyleLink) abs.numStyleLink = numStyleLink;
  const seen = new Set<number>();
  for (const l of children(node, 'w:lvl')) {
    const lvl = parseLevel(l, raw);
    if (lvl && !seen.has(lvl.level)) {
      seen.add(lvl.level);
      abs.levels.push(lvl);
    }
  }
  abs.levels.sort((a, b) => a.level - b.level);
  return abs;
}

function parseInstance(node: XmlElement, raw: RawXml): NumberingInstance | undefined {
  const id = parseDecimal(attr(node, 'w:numId'));
  const abstractId = parseDecimal(val(child(node, 'w:abstractNumId')));
  if (id === undefined || abstractId === undefined) return undefined;
  const overrides: LevelOverride[] = [];
  for (const o of children(node, 'w:lvlOverride')) {
    const level = parseDecimal(attr(o, 'w:ilvl'));
    if (level === undefined || level < 0 || level > 8) continue;
    const ov: LevelOverride = { level };
    const start = parseDecimal(val(child(o, 'w:startOverride')));
    if (start !== undefined) ov.startOverride = start;
    const def = child(o, 'w:lvl');
    if (def) {
      const lvl = parseLevel({ ...def, attrs: { ...def.attrs, 'w:ilvl': String(level) } }, raw);
      if (lvl) ov.definition = lvl;
    }
    overrides.push(ov);
  }
  return { id, abstractId, overrides };
}

export interface ParsedNumbering {
  numbering: Numbering;
  /** `w:numPicBullet` definitions as raw XML (picture bullets). */
  picBullets: string[];
}

/** Parses the root of `numbering.xml`. Instances whose abstract definition is missing are dropped. */
export function parseNumbering(root: XmlElement, raw: RawXml): ParsedNumbering {
  const abstracts: AbstractNumbering[] = [];
  const ids = new Set<number>();
  for (const a of children(root, 'w:abstractNum')) {
    const abs = parseAbstract(a, raw);
    if (abs && !ids.has(abs.id)) {
      ids.add(abs.id);
      abstracts.push(abs);
    }
  }
  const instances: NumberingInstance[] = [];
  const numIds = new Set<number>();
  for (const n of children(root, 'w:num')) {
    const inst = parseInstance(n, raw);
    if (inst && ids.has(inst.abstractId) && !numIds.has(inst.id)) {
      numIds.add(inst.id);
      instances.push(inst);
    }
  }
  return {
    numbering: new Numbering({ abstracts, instances }),
    picBullets: children(root, 'w:numPicBullet').map(raw),
  };
}
