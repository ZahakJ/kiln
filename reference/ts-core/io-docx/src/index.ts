/**
 * @kiln-ref/io-docx — reads (and, from WP-13, writes) `.docx`/`.docm`/`.dotx`/`.dotm` files to and from
 * the Kiln Write document model.
 */
export const PACKAGE_NAME = '@kiln-ref/io-docx' as const;

export { symbolText, symbolToUnicode } from './common/symbols.ts';
export type * from './common/types.ts';
export { parseHalfPoints, parseTableWidth, parseTwips } from './common/units.ts';
export type { DocxWarning, DocxWarningCode, DocxWarningSeverity } from './common/warnings.ts';
export { fragmentXml, parseFragment } from './common/xml.ts';
export { DocxFormatError, type ReadDocxOptions, readDocx, readDocxStream } from './read/index.ts';
export { GRID_PLACEHOLDER } from './read/tables.ts';
