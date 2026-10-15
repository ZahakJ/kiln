/** Relationship types (transitional URIs). `relTypeMatches` also accepts the Strict equivalents. */

const OD = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
const PKG = 'http://schemas.openxmlformats.org/package/2006/relationships';
const MS = 'http://schemas.microsoft.com/office';

export const RelType = {
  officeDocument: `${OD}/officeDocument`,
  coreProperties: `${PKG}/metadata/core-properties`,
  extendedProperties: `${OD}/extended-properties`,
  customProperties: `${OD}/custom-properties`,
  thumbnail: `${PKG}/metadata/thumbnail`,
  digitalSignatureOrigin: `${PKG}/digital-signature/origin`,
  customXml: `${OD}/customXml`,
  customXmlProps: `${OD}/customXmlProps`,
  theme: `${OD}/theme`,
  styles: `${OD}/styles`,
  stylesWithEffects: `${MS}/2007/relationships/stylesWithEffects`,
  numbering: `${OD}/numbering`,
  settings: `${OD}/settings`,
  webSettings: `${OD}/webSettings`,
  fontTable: `${OD}/fontTable`,
  font: `${OD}/font`,
  header: `${OD}/header`,
  footer: `${OD}/footer`,
  footnotes: `${OD}/footnotes`,
  endnotes: `${OD}/endnotes`,
  comments: `${OD}/comments`,
  commentsExtended: `${MS}/2011/relationships/commentsExtended`,
  commentsIds: `${MS}/2016/09/relationships/commentsIds`,
  commentsExtensible: `${MS}/2018/08/relationships/commentsExtensible`,
  people: `${MS}/2011/relationships/people`,
  glossaryDocument: `${OD}/glossaryDocument`,
  image: `${OD}/image`,
  hyperlink: `${OD}/hyperlink`,
  oleObject: `${OD}/oleObject`,
  package: `${OD}/package`,
  chart: `${OD}/chart`,
  chartUserShapes: `${OD}/chartUserShapes`,
  chartStyle: `${MS}/2011/relationships/chartStyle`,
  chartColorStyle: `${MS}/2011/relationships/chartColorStyle`,
  diagramData: `${OD}/diagramData`,
  diagramLayout: `${OD}/diagramLayout`,
  diagramQuickStyle: `${OD}/diagramQuickStyle`,
  diagramColors: `${OD}/diagramColors`,
  diagramDrawing: `${MS}/2007/relationships/diagramDrawing`,
  vbaProject: `${MS}/2006/relationships/vbaProject`,
  printerSettings: `${OD}/printerSettings`,
  video: `${OD}/video`,
  audio: `${OD}/audio`,
  media: `${MS}/2007/relationships/media`,
  // SpreadsheetML
  worksheet: `${OD}/worksheet`,
  chartsheet: `${OD}/chartsheet`,
  dialogsheet: `${OD}/dialogsheet`,
  sharedStrings: `${OD}/sharedStrings`,
  calcChain: `${OD}/calcChain`,
  drawing: `${OD}/drawing`,
  vmlDrawing: `${OD}/vmlDrawing`,
  table: `${OD}/table`,
  pivotTable: `${OD}/pivotTable`,
  pivotCacheDefinition: `${OD}/pivotCacheDefinition`,
  pivotCacheRecords: `${OD}/pivotCacheRecords`,
  externalLink: `${OD}/externalLink`,
  queryTable: `${OD}/queryTable`,
  connections: `${OD}/connections`,
  sheetMetadata: `${OD}/sheetMetadata`,
  threadedComment: `${MS}/2017/10/relationships/threadedComment`,
  person: `${MS}/2017/10/relationships/person`,
  // PresentationML
  slide: `${OD}/slide`,
  slideLayout: `${OD}/slideLayout`,
  slideMaster: `${OD}/slideMaster`,
  notesSlide: `${OD}/notesSlide`,
  notesMaster: `${OD}/notesMaster`,
  handoutMaster: `${OD}/handoutMaster`,
  presProps: `${OD}/presProps`,
  viewProps: `${OD}/viewProps`,
  tableStyles: `${OD}/tableStyles`,
  commentAuthors: `${OD}/commentAuthors`,
  slideComments: `${OD}/comments`,
  modernComment: `${MS}/2018/10/relationships/comments`,
  authors: `${MS}/2018/10/relationships/authors`,
  tags: `${OD}/tags`,
} as const;

export type RelTypeName = keyof typeof RelType;

const STRICT_OD = 'http://purl.oclc.org/ooxml/officeDocument/relationships';

/** Maps a Strict relationship type URI to its transitional equivalent; other URIs are returned as is. */
export function toTransitionalRelType(type: string): string {
  if (type.startsWith(`${STRICT_OD}/`)) {
    const tail = type.slice(STRICT_OD.length + 1);
    if (tail === 'metadata/core-properties') return RelType.coreProperties;
    return `${OD}/${tail === 'extendedProperties' ? 'extended-properties' : tail === 'customProperties' ? 'custom-properties' : tail}`;
  }
  return type;
}

/** True when `actual` is `expected` or its Strict equivalent. */
export function relTypeMatches(actual: string, expected: string): boolean {
  return actual === expected || toTransitionalRelType(actual) === expected;
}
