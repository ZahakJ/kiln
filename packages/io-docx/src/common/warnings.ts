/**
 * Import diagnostics. Every construct the reader cannot model is reported with the part and an
 * XPath-like location, so the Compatibility Checker can tell the user what will look different or be
 * lost.
 *
 * Severity:
 * - `warning`: content or formatting was dropped or approximated while reading, so it will not be
 *   written back;
 * - `info`: the construct is preserved verbatim for saving but cannot be edited (and may be shown as a
 *   placeholder).
 */

export type DocxWarningSeverity = 'warning' | 'info';

export type DocxWarningCode =
  | 'part.missing'
  | 'part.invalidXml'
  | 'drawing.unsupported'
  | 'drawing.group'
  | 'drawing.smartArt'
  | 'drawing.canvas'
  | 'drawing.ink'
  | 'drawing.vml'
  | 'image.linked'
  | 'image.missing'
  | 'object.embedded'
  | 'field.unterminated'
  | 'field.multiParagraph'
  | 'field.unwrapped'
  | 'field.formField'
  | 'comment.unanchored'
  | 'comment.missing'
  | 'note.missing'
  | 'revision.move'
  | 'revision.cell'
  | 'revision.table'
  | 'table.irregular'
  | 'element.unknown'
  | 'altChunk'
  | 'subDocument'
  | 'permission'
  | 'glossary';

export interface DocxWarning {
  code: DocxWarningCode;
  severity: DocxWarningSeverity;
  message: string;
  /** Part name (`/word/document.xml`). */
  part: string;
  /** Location in the part, e.g. `/w:document/w:body/w:p[3]/w:r[2]/w:object[1]`. */
  path: string;
}

/** Collects warnings, merging repeats of the same code at the same location. */
export class WarningCollector {
  readonly #list: DocxWarning[] = [];
  readonly #seen = new Set<string>();

  add(w: DocxWarning): void {
    const key = `${w.code}|${w.part}|${w.path}`;
    if (this.#seen.has(key)) return;
    this.#seen.add(key);
    this.#list.push(w);
  }

  get list(): DocxWarning[] {
    return [...this.#list];
  }
}
