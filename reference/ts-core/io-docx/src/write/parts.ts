/**
 * Package-level parts generated from the model: `styles.xml`, `numbering.xml`, `settings.xml`,
 * `people.xml` and `commentsExtended.xml`. Story parts (body, headers, footers, notes, comments) are
 * written with `StoryWriter`.
 */
import type {
  AbstractNumbering,
  LevelOverride,
  Numbering,
  NumberingInstance,
  NumberingLevel,
  Style,
  StyleSheet,
  TableProps,
} from '@kiln-ref/doc-model';
import { buildXml, el, type XmlElement } from '@kiln-ref/ooxml';
import type {
  DocxParagraphProps,
  DocxPerson,
  DocxRunProps,
  DocxSettings,
  NoteProps,
} from '../common/types.ts';
import { parseFragment } from '../common/xml.ts';
import {
  cellPropsInner,
  notePropsXml,
  paragraphPropsInner,
  rowPropsInner,
  runPropsInner,
  tablePropsInner,
} from './props.ts';
import { attrs, part, tag, toggle, val } from './xml.ts';

// ------------------------------------------------------------------ styles

const CONDITION_ORDER = [
  'wholeTable',
  'band1Horz',
  'band2Horz',
  'band1Vert',
  'band2Vert',
  'firstCol',
  'firstRow',
  'lastCol',
  'lastRow',
  'neCell',
  'nwCell',
  'seCell',
  'swCell',
] as const;

/** An element that is present in the model: written even when empty (the reader keeps `{}`). */
function wrapIf(name: string, inner: string): string {
  return inner ? `<${name}>${inner}</${name}>` : `<${name}/>`;
}

function tableStyleProps(p: TableProps | undefined): string {
  if (!p) return '';
  const { xml, ...rest } = p as TableProps & { xml?: string[] };
  return wrapIf('w:tblPr', tablePropsInner(rest, xml));
}

function styleXml(s: Style): string {
  let inner = val('w:name', s.name);
  if (s.aliases?.length) inner += val('w:aliases', s.aliases.join(','));
  inner += val('w:basedOn', s.basedOn) + val('w:next', s.next) + val('w:link', s.link);
  if (s.hidden) inner += '<w:hidden/>';
  inner += val('w:uiPriority', s.uiPriority);
  if (s.semiHidden) inner += '<w:semiHidden/>';
  if (s.unhideWhenUsed) inner += '<w:unhideWhenUsed/>';
  if (s.quickStyle) inner += '<w:qFormat/>';
  if (s.locked) inner += '<w:locked/>';
  if (s.pPr) inner += wrapIf('w:pPr', paragraphPropsInner(s.pPr as DocxParagraphProps));
  if (s.rPr) inner += wrapIf('w:rPr', runPropsInner(s.rPr as DocxRunProps));
  if (s.type === 'table') {
    inner += tableStyleProps(s.tblPr);
    if (s.trPr) inner += wrapIf('w:trPr', rowPropsInner(s.trPr));
    if (s.tcPr) inner += wrapIf('w:tcPr', cellPropsInner(s.tcPr));
    for (const type of CONDITION_ORDER) {
      const c = s.conditions?.[type];
      if (!c) continue;
      let ci = '';
      if (c.pPr) ci += wrapIf('w:pPr', paragraphPropsInner(c.pPr as DocxParagraphProps));
      if (c.rPr) ci += wrapIf('w:rPr', runPropsInner(c.rPr as DocxRunProps));
      ci += tableStyleProps(c.tblPr);
      if (c.trPr) ci += wrapIf('w:trPr', rowPropsInner(c.trPr));
      if (c.tcPr) ci += wrapIf('w:tcPr', cellPropsInner(c.tcPr));
      inner += tag('w:tblStylePr', [['w:type', type]], ci);
    }
  }
  return tag(
    'w:style',
    [
      ['w:type', s.type],
      ['w:default', s.isDefault ? '1' : undefined],
      ['w:customStyle', s.custom ? '1' : undefined],
      ['w:styleId', s.id],
    ],
    inner,
  );
}

/** `styles.xml`. `latentStyles` is the preserved `w:latentStyles` element, if any. */
export function stylesPart(styles: StyleSheet, latentStyles?: string): string {
  const rPr = runPropsInner(styles.docDefaults.rPr as DocxRunProps);
  const pPr = paragraphPropsInner(styles.docDefaults.pPr as DocxParagraphProps);
  const defaults =
    '<w:docDefaults>' +
    (rPr ? `<w:rPrDefault><w:rPr>${rPr}</w:rPr></w:rPrDefault>` : '<w:rPrDefault/>') +
    (pPr ? `<w:pPrDefault><w:pPr>${pPr}</w:pPr></w:pPrDefault>` : '<w:pPrDefault/>') +
    '</w:docDefaults>';
  const body = defaults + (latentStyles ?? '') + styles.all().map(styleXml).join('');
  return part('w:styles', body, '', [
    'w',
    'r',
    'w14',
    'w15',
    'w16se',
    'w16cid',
    'w16',
    'w16cex',
    'w16sdtdh',
    'mc',
  ]);
}

// ------------------------------------------------------------------ numbering

function levelXml(l: NumberingLevel, name = 'w:lvl'): string {
  let inner = val('w:start', l.start);
  inner += tag('w:numFmt', [
    ['w:val', l.format],
    ['w:format', l.customFormat],
  ]);
  if (l.restartAfter !== undefined) inner += val('w:lvlRestart', l.restartAfter);
  inner += val('w:pStyle', l.pStyle);
  if (l.isLegal) inner += '<w:isLgl/>';
  if (l.suffix) inner += val('w:suff', l.suffix);
  inner += val('w:lvlText', l.text ?? '');
  if (l.picBulletId !== undefined) inner += val('w:lvlPicBulletId', l.picBulletId);
  if (l.alignment) inner += val('w:lvlJc', l.alignment);
  if (l.pPr) inner += wrapIf('w:pPr', paragraphPropsInner(l.pPr as DocxParagraphProps));
  if (l.rPr) inner += wrapIf('w:rPr', runPropsInner(l.rPr as DocxRunProps));
  return tag(name, [['w:ilvl', l.level]], inner);
}

function abstractXml(a: AbstractNumbering): string {
  let inner = val('w:nsid', a.nsid) + val('w:multiLevelType', a.multiLevelType) + val('w:name', a.name);
  inner += val('w:styleLink', a.styleLink) + val('w:numStyleLink', a.numStyleLink);
  inner += [...a.levels]
    .sort((x, y) => x.level - y.level)
    .map((l) => levelXml(l))
    .join('');
  return tag('w:abstractNum', [['w:abstractNumId', a.id]], inner);
}

function overrideXml(o: LevelOverride): string {
  return tag(
    'w:lvlOverride',
    [['w:ilvl', o.level]],
    val('w:startOverride', o.startOverride) +
      (o.definition ? levelXml({ ...o.definition, level: o.level }) : ''),
  );
}

function instanceXml(n: NumberingInstance): string {
  return tag(
    'w:num',
    [['w:numId', n.id]],
    val('w:abstractNumId', n.abstractId) + n.overrides.map(overrideXml).join(''),
  );
}

/** `numbering.xml` (`w:numPicBullet`s first, then abstract definitions, then instances). */
export function numberingPart(numbering: Numbering, picBullets: readonly string[] = []): string {
  const json = numbering.toJSON();
  const body =
    picBullets.join('') +
    [...json.abstracts]
      .sort((a, b) => a.id - b.id)
      .map(abstractXml)
      .join('') +
    [...json.instances]
      .sort((a, b) => a.id - b.id)
      .map(instanceXml)
      .join('');
  return part('w:numbering', body, '', ['w', 'r', 'w14', 'w15', 'w16se', 'mc']);
}

// ------------------------------------------------------------------ settings

/** CT_Settings (ECMA-376 Part 1 §17.15.1.78). */
const SETTINGS_SEQUENCE = [
  'w:writeProtection',
  'w:view',
  'w:zoom',
  'w:removePersonalInformation',
  'w:removeDateAndTime',
  'w:doNotDisplayPageBoundaries',
  'w:displayBackgroundShape',
  'w:printPostScriptOverText',
  'w:printFractionalCharacterWidth',
  'w:printFormsData',
  'w:embedTrueTypeFonts',
  'w:embedSystemFonts',
  'w:saveSubsetFonts',
  'w:saveFormsData',
  'w:mirrorMargins',
  'w:alignBordersAndEdges',
  'w:bordersDoNotSurroundHeader',
  'w:bordersDoNotSurroundFooter',
  'w:gutterAtTop',
  'w:hideSpellingErrors',
  'w:hideGrammaticalErrors',
  'w:activeWritingStyle',
  'w:proofState',
  'w:formsDesign',
  'w:attachedTemplate',
  'w:linkStyles',
  'w:stylePaneFormatFilter',
  'w:stylePaneSortMethod',
  'w:documentType',
  'w:mailMerge',
  'w:revisionView',
  'w:trackRevisions',
  'w:doNotTrackMoves',
  'w:doNotTrackFormatting',
  'w:documentProtection',
  'w:autoFormatOverride',
  'w:styleLockTheme',
  'w:styleLockQFSet',
  'w:defaultTabStop',
  'w:autoHyphenation',
  'w:consecutiveHyphenLimit',
  'w:hyphenationZone',
  'w:doNotHyphenateCaps',
  'w:showEnvelope',
  'w:summaryLength',
  'w:clickAndTypeStyle',
  'w:defaultTableStyle',
  'w:evenAndOddHeaders',
  'w:bookFoldRevPrinting',
  'w:bookFoldPrinting',
  'w:bookFoldPrintingSheets',
  'w:drawingGridHorizontalSpacing',
  'w:drawingGridVerticalSpacing',
  'w:displayHorizontalDrawingGridEvery',
  'w:displayVerticalDrawingGridEvery',
  'w:doNotUseMarginsForDrawingGridOrigin',
  'w:drawingGridHorizontalOrigin',
  'w:drawingGridVerticalOrigin',
  'w:doNotShadeFormData',
  'w:noPunctuationKerning',
  'w:characterSpacingControl',
  'w:printTwoOnOne',
  'w:strictFirstAndLastChars',
  'w:noLineBreaksAfter',
  'w:noLineBreaksBefore',
  'w:savePreviewPicture',
  'w:doNotValidateAgainstSchema',
  'w:saveInvalidXml',
  'w:ignoreMixedContent',
  'w:alwaysShowPlaceholderText',
  'w:doNotDemarcateInvalidXml',
  'w:saveXmlDataOnly',
  'w:useXSLTWhenSaving',
  'w:saveThroughXslt',
  'w:showXMLTags',
  'w:alwaysMergeEmptyNamespace',
  'w:updateFields',
  'w:hdrShapeDefaults',
  'w:footnotePr',
  'w:endnotePr',
  'w:compat',
  'w:docVars',
  'w:rsids',
  'm:mathPr',
  'w:attachedSchema',
  'w:themeFontLang',
  'w:clrSchemeMapping',
  'w:doNotIncludeSubdocsInStats',
  'w:doNotAutoCompressPictures',
  'w:forceUpgrade',
  'w:captions',
  'w:readModeInkLockDown',
  'w:smartTagType',
  'sl:schemaLibrary',
  'w:shapeDefaults',
  'w:doNotEmbedSmartTags',
  'w:decimalSymbol',
  'w:listSeparator',
];

const RANK = new Map(SETTINGS_SEQUENCE.map((n, i) => [n, i]));

/** Replaces (or removes, with `undefined`) the `name` children of `root`, keeping schema order. */
function upsert(root: XmlElement, name: string, replacement: XmlElement | undefined): void {
  const first = root.children.findIndex((c) => c.type === 'element' && c.name === name);
  root.children = root.children.filter((c) => !(c.type === 'element' && c.name === name));
  if (!replacement) return;
  if (first >= 0) {
    root.children.splice(first, 0, replacement);
    return;
  }
  const rank = RANK.get(name) ?? SETTINGS_SEQUENCE.length;
  const at = root.children.findIndex(
    (c) => c.type === 'element' && (RANK.get(c.name) ?? SETTINGS_SEQUENCE.length) > rank,
  );
  if (at >= 0) root.children.splice(at, 0, replacement);
  else root.children.push(replacement);
}

const frag = (xml: string) => parseFragment(xml);
const onOffEl = (name: string, on: boolean | undefined): XmlElement | undefined =>
  on ? el(name) : undefined;

export interface SettingsInput {
  settings: DocxSettings;
  /** The settings part as read (`extras.settingsXml`), patched rather than regenerated. */
  original?: string;
  /** Separator note ids, written into `w:footnotePr`/`w:endnotePr` the way Word references them. */
  footnoteSeparators?: readonly string[];
  endnoteSeparators?: readonly string[];
}

function noteProps(
  name: string,
  p: NoteProps | undefined,
  child: string,
  ids: readonly string[] | undefined,
) {
  const xml = notePropsXml(name, p, ids?.length ? { name: child, ids } : undefined);
  return xml ? frag(xml) : undefined;
}

/** `settings.xml`: the original settings with every modelled value written from the model. */
export function settingsPart(input: SettingsInput): string {
  const s = input.settings;
  let root: XmlElement;
  if (input.original) {
    try {
      root = parseFragment(input.original);
    } catch {
      root = el('w:settings');
    }
  } else root = el('w:settings');
  const fresh = !input.original;

  upsert(root, 'w:mirrorMargins', onOffEl('w:mirrorMargins', s.mirrorMargins));
  upsert(root, 'w:trackRevisions', onOffEl('w:trackRevisions', s.trackRevisions));
  if (s.doNotTrackMoves !== undefined)
    upsert(root, 'w:doNotTrackMoves', onOffEl('w:doNotTrackMoves', s.doNotTrackMoves));
  if (s.doNotTrackFormatting !== undefined)
    upsert(root, 'w:doNotTrackFormatting', onOffEl('w:doNotTrackFormatting', s.doNotTrackFormatting));
  if (s.protection) {
    const existing = root.children.find(
      (c): c is XmlElement => c.type === 'element' && c.name === 'w:documentProtection',
    );
    const prot = existing ?? el('w:documentProtection');
    if (s.protection.edit) prot.attrs['w:edit'] = s.protection.edit;
    else delete prot.attrs['w:edit'];
    prot.attrs['w:enforcement'] = s.protection.enforced ? '1' : '0';
    upsert(root, 'w:documentProtection', prot);
  } else upsert(root, 'w:documentProtection', undefined);
  const hasTabStop = root.children.some((c) => c.type === 'element' && c.name === 'w:defaultTabStop');
  if (hasTabStop || fresh || s.defaultTabStop !== 720)
    upsert(root, 'w:defaultTabStop', frag(val('w:defaultTabStop', s.defaultTabStop)));
  if (s.autoHyphenation !== undefined)
    upsert(root, 'w:autoHyphenation', onOffEl('w:autoHyphenation', s.autoHyphenation));
  upsert(root, 'w:evenAndOddHeaders', onOffEl('w:evenAndOddHeaders', s.evenAndOddHeaders));
  if (s.updateFields !== undefined)
    upsert(root, 'w:updateFields', s.updateFields ? frag('<w:updateFields w:val="true"/>') : undefined);
  upsert(
    root,
    'w:footnotePr',
    noteProps('w:footnotePr', s.footnotePr, 'w:footnote', input.footnoteSeparators),
  );
  upsert(root, 'w:endnotePr', noteProps('w:endnotePr', s.endnotePr, 'w:endnote', input.endnoteSeparators));

  // Compatibility mode: Word 2007 files (12) have no compatSetting; keep them that way.
  const compat =
    root.children.find((c): c is XmlElement => c.type === 'element' && c.name === 'w:compat') ??
    el('w:compat');
  const settingsList = compat.children.filter(
    (c): c is XmlElement => c.type === 'element' && c.name === 'w:compatSetting',
  );
  const modeEl = settingsList.find((c) => c.attrs['w:name'] === 'compatibilityMode');
  if (modeEl) modeEl.attrs['w:val'] = String(s.compatibilityMode);
  else if (!(s.compatibilityMode === 12 && root.children.includes(compat))) {
    compat.children.push(
      el('w:compatSetting', {
        'w:name': 'compatibilityMode',
        'w:uri': 'http://schemas.microsoft.com/office/word',
        'w:val': s.compatibilityMode,
      }),
    );
    if (fresh && s.compatibilityMode >= 15) {
      for (const name of [
        'overrideTableStyleFontSizeAndJustification',
        'enableOpenTypeFeatures',
        'doNotFlipMirrorIndents',
        'differentiateMultirowTableHeaders',
      ]) {
        compat.children.push(
          el('w:compatSetting', {
            'w:name': name,
            'w:uri': 'http://schemas.microsoft.com/office/word',
            'w:val': '1',
          }),
        );
      }
    }
  }
  upsert(root, 'w:compat', compat);

  if (s.themeFontLang) {
    upsert(
      root,
      'w:themeFontLang',
      frag(
        tag('w:themeFontLang', [
          ['w:val', s.themeFontLang.val],
          ['w:eastAsia', s.themeFontLang.eastAsia],
          ['w:bidi', s.themeFontLang.bidi],
        ]),
      ),
    );
  }
  if (s.colorSchemeMapping) {
    const list = Object.entries(s.colorSchemeMapping)
      .filter(([, v]) => v !== undefined)
      .map(([k, v]) => [`w:${k}`, v] as const);
    upsert(root, 'w:clrSchemeMapping', frag(`<w:clrSchemeMapping${attrs(list)}/>`));
  }
  if (s.decimalSymbol !== undefined)
    upsert(root, 'w:decimalSymbol', frag(val('w:decimalSymbol', s.decimalSymbol)));
  if (s.listSeparator !== undefined)
    upsert(root, 'w:listSeparator', frag(val('w:listSeparator', s.listSeparator)));

  if (root.attrs['xmlns:w'] !== undefined)
    return buildXml(root, { declaration: true, declareNamespaces: true });
  const body = root.children
    .map((c) => (c.type === 'element' ? buildXml(c, { declaration: false, declareNamespaces: false }) : ''))
    .join('');
  return part('w:settings', body, '', ['w', 'r', 'm', 'w14', 'w15', 'w16cid', 'w16se', 'mc']);
}

// ------------------------------------------------------------------ comments extras, people

export function peoplePart(people: readonly DocxPerson[]): string {
  const body = people
    .map((p) => {
      const info =
        p.providerId || p.userId
          ? tag('w15:presenceInfo', [
              ['w15:providerId', p.providerId],
              ['w15:userId', p.userId],
            ])
          : '';
      return tag('w15:person', [['w15:author', p.author]], info);
    })
    .join('');
  return part('w15:people', body, '', ['w', 'w15', 'w14', 'mc']);
}

export function commentsExtendedPart(
  entries: readonly { paraId: string; parent?: string; done: boolean }[],
): string {
  const body = entries
    .map((e) =>
      tag('w15:commentEx', [
        ['w15:paraId', e.paraId],
        ['w15:paraIdParent', e.parent],
        ['w15:done', e.done ? '1' : '0'],
      ]),
    )
    .join('');
  return part('w15:commentsEx', body, '', ['w', 'w15', 'w14', 'mc']);
}

export { toggle };
