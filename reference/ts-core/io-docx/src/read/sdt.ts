/** Content controls (`w:sdt`, ECMA-376 Part 1 §17.5.2): `w:sdtPr` → `SdtInfo`. */
import type { SdtInfo, SdtKind } from '@kiln-ref/doc-model';
import { child, children, type XmlElement } from '@kiln-ref/ooxml';
import { parseOnOff } from '../common/units.ts';
import { attr } from '../common/xml.ts';
import type { RawXml } from './props.ts';

const KIND_ELEMENTS: [string, SdtKind][] = [
  ['w:richText', 'richText'],
  ['w:text', 'text'],
  ['w:date', 'date'],
  ['w:dropDownList', 'dropDownList'],
  ['w:comboBox', 'comboBox'],
  ['w14:checkbox', 'checkbox'],
  ['w:picture', 'picture'],
  ['w:docPartObj', 'docPartObj'],
  ['w:docPartList', 'docPartList'],
  ['w:group', 'group'],
  ['w:bibliography', 'bibliography'],
  ['w:citation', 'citation'],
  ['w:equation', 'equation'],
  ['w15:repeatingSection', 'repeatingSection'],
  ['w15:repeatingSectionItem', 'repeatingSectionItem'],
];

/** Elements of `w:sdtPr` that are not a type marker. */
const COMMON = new Set([
  'w:rPr',
  'w:alias',
  'w:tag',
  'w:id',
  'w:lock',
  'w:placeholder',
  'w:temporary',
  'w:showingPlcHdr',
  'w:dataBinding',
  'w:label',
  'w:tabIndex',
  'w15:color',
  'w15:appearance',
  'w15:webExtensionLinked',
  'w15:webExtensionCreated',
  'w15:dataBinding',
  'w16sdtdh:storeItemChecksum',
]);

export function parseSdtInfo(sdt: XmlElement, level: SdtInfo['level'], key: string, raw: RawXml): SdtInfo {
  const pr = child(sdt, 'w:sdtPr');
  const endPr = child(sdt, 'w:sdtEndPr');
  let kind: SdtKind = 'richText';
  let marker: XmlElement | undefined;
  for (const c of children(pr)) {
    const k = KIND_ELEMENTS.find(([n]) => n === c.name);
    if (k) {
      kind = k[1];
      marker = c;
      break;
    }
    if (!COMMON.has(c.name)) kind = 'other';
  }
  const info: SdtInfo = { key, level, kind, pr: pr ? raw(pr) : '<w:sdtPr/>' };
  if (endPr) info.endPr = raw(endPr);
  const id = attr(child(pr, 'w:id'), 'w:val');
  if (id !== undefined) info.id = id;
  const tag = attr(child(pr, 'w:tag'), 'w:val');
  if (tag !== undefined) info.tag = tag;
  const alias = attr(child(pr, 'w:alias'), 'w:val');
  if (alias !== undefined) info.alias = alias;
  if (child(pr, 'w:showingPlcHdr'))
    info.showingPlaceholder = parseOnOff(attr(child(pr, 'w:showingPlcHdr'), 'w:val'));
  if (marker) {
    switch (kind) {
      case 'docPartObj':
      case 'docPartList': {
        const gallery = attr(child(marker, 'w:docPartGallery'), 'w:val');
        if (gallery) info.docPartGallery = gallery;
        break;
      }
      case 'checkbox':
        info.checked = parseOnOff(attr(child(marker, 'w14:checked'), 'w14:val'), false);
        break;
      case 'dropDownList':
      case 'comboBox':
        info.listItems = children(marker, 'w:listItem').map((li) => {
          const value = attr(li, 'w:value') ?? '';
          return { displayText: attr(li, 'w:displayText') ?? value, value };
        });
        break;
      case 'date': {
        const fmt = attr(child(marker, 'w:dateFormat'), 'w:val');
        if (fmt) info.dateFormat = fmt;
        break;
      }
      default:
        break;
    }
  }
  return info;
}
