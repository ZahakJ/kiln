/**
 * Public option types and the file layer's passthrough data stored in `Workbook.passthrough`.
 *
 * The sheet model holds everything a user can see and edit. What it does not model — unknown parts,
 * workbook/sheet protection, VBA, pivot tables, slicers, printer settings, unknown `extLst`s, exact
 * original XML of blocks that did not change — lives here, so a read → write round trip loses
 * nothing ("never destroy what you don't understand", PLAN §3.5).
 */
import type { OpcPackage, PassthroughSet, Theme, XmlDocument, XmlElement } from '@kiln-ref/ooxml';
import type { CellRange, CellValue, Font } from '@kiln-ref/sheet-model';

/** One run of rich text in a cell (shared strings `<r>`). Properties absent = the cell's font. */
export interface RichTextRun {
  text: string;
  font?: Partial<Font>;
}

/** An array formula anchored at a cell (legacy CSE array or a dynamic-array spill). */
export interface ArrayFormulaInfo {
  /** Formula text (model form) the info belongs to; stale info is ignored when the cell changed. */
  formula: string;
  /** Range written in `<f ref>` (the CSE range or the spill range). */
  ref: CellRange;
  /** Dynamic array (`cm` → XLDAPR metadata) rather than a legacy CSE array. */
  dynamic: boolean;
  /**
   * Cached values of the whole range in row-major order (the anchor included), as read from the file.
   * Spilled cells are not stored in the cell store (they would block the spill), so their cached values
   * live here until the formula engine recomputes them.
   */
  cached: CellValue[][];
}

/** XML kept verbatim, with the model snapshot it was parsed into; re-emitted while the model is unchanged. */
export interface Preserved {
  /** Canonical JSON of the model data the XML was read into. */
  snapshot: string;
  xml: XmlElement[];
}

export interface DrawingObjectXml {
  /** The anchor element as read (`xdr:twoCellAnchor`…, or an `mc:AlternateContent` wrapping one). */
  xml: XmlElement;
}

export interface SheetPassthrough {
  /** Part name in the source package. */
  partName: string;
  /** Namespace declarations of the source root (prefix → URI), for verbatim children. */
  namespaces: Record<string, string>;
  /** `mc:Ignorable` prefixes of the source root. */
  ignorable: string[];
  /** Unmodelled worksheet children, by element name, in source order. */
  elements: Map<string, XmlElement[]>;
  /** `ext` elements of the worksheet `extLst` that are not modelled (x14 CF/DV are). */
  extensions: XmlElement[];
  /** Original XML of modelled blocks, re-emitted while unchanged (`sheetViews`, `cf`, `autoFilter`…). */
  preserved: Map<string, Preserved>;
  /** Rich-text cells: key `row,col` → runs (kept while the cell's text still matches). */
  richText: Map<string, RichTextRun[]>;
  /** Array formulas: key `row,col` of the anchor. */
  arrays: Map<string, ArrayFormulaInfo>;
  /** Cell value-metadata indices (`vm`), key `row,col`. */
  valueMeta: Map<string, string>;
  /** `r:id` of `<pageSetup>` (printer settings part) in the source. */
  pageSetupRid?: string;
  /** Source drawing part, its anchors' XML by model anchor id, and the z-order of anchor ids. */
  drawingPart?: string;
  drawingRoot?: XmlElement;
  anchors: Map<string, DrawingObjectXml>;
  drawingOrder: string[];
  /** Source legacy VML part for notes and the non-note shapes it held (form controls, OLE previews). */
  vmlPart?: string;
  vmlOtherShapes: XmlElement[];
  vmlShapeTypes: XmlElement[];
  commentsPart?: string;
  threadedPart?: string;
  /** Source table part name by table id. */
  tableParts: Map<number, string>;
  /** Source table XML by table id, re-emitted while the table is unchanged. */
  tables: Map<number, Preserved>;
  /** Rich text of notes (kept while the note text is unchanged), key `row,col`. */
  noteRuns: Map<string, RichTextRun[]>;
}

export interface StylesPassthrough {
  /** Number of `cellXfs` entries in the source; table entries after them hold `cellStyleXfs`. */
  cellXfsCount: number;
  /** Style-table index of each `cellStyleXfs` entry, in source order. */
  styleXfs: number[];
  /** Number format id by format code, from the source `numFmts`. */
  numFmtIds: Map<string, number>;
  /** The source `numFmtId` of each `cellXfs` entry (for built-in ids without a known code). */
  xfNumFmtIds: number[];
  /** Differential formats whose fill had no `patternType` (Excel's implicit solid). */
  dxfFillNoPattern: Set<number>;
  /** `colors`, `tableStyles` and `extLst` of the stylesheet, verbatim. */
  extras: Map<string, XmlElement>;
  /** Extra attributes on `cellXfs` entries (`pivotButton`, `applyX`…), by table index. */
  xfExtras: Map<number, Record<string, string>>;
}

/** A sheet the model does not represent (chart sheet, dialog sheet, macro sheet), kept in order. */
export interface ForeignSheet {
  /** Position in the workbook's sheet list. */
  index: number;
  name: string;
  sheetId: number;
  state?: string;
  /** Source part name. */
  partName: string;
  /** Source relationship id from the workbook. */
  rid: string;
}

export interface WorkbookPassthrough {
  namespaces: Record<string, string>;
  ignorable: string[];
  /** Unmodelled workbook children by name (`fileVersion`, `workbookProtection`, `pivotCaches`…). */
  elements: Map<string, XmlElement[]>;
  /** Attributes of `workbookPr` other than the modelled ones. */
  workbookPrAttrs: Record<string, string>;
  /** Attributes of `calcPr` other than the modelled ones (`calcId`…). */
  calcPrAttrs: Record<string, string>;
  preserved: Map<string, Preserved>;
}

/** Workbook-level data kept by the file layer (`Workbook.passthrough`). */
export interface XlsxPassthrough {
  kind: 'xlsx';
  /** Parts and relationships the reader did not consume. */
  parts: PassthroughSet;
  /** Main part names in the source package. */
  workbookPart: string;
  stylesPart?: string;
  themePart?: string;
  sharedStringsPart?: string;
  metadataPart?: string;
  personsPart?: string;
  /** The theme object as read and its source bytes (re-emitted while `Workbook.theme` is that object). */
  themeSource?: { theme: Theme; bytes: Uint8Array };
  styles: StylesPassthrough;
  workbook: WorkbookPassthrough;
  foreignSheets: ForeignSheet[];
  /** Per-sheet data by `Sheet.id`. */
  sheets: Map<number, SheetPassthrough>;
  /** Function names that carried `_xlfn.` in the source. */
  futureFunctions: Set<string>;
  /** Images for drawings by `DrawingAnchor.ref` (pictures inserted in the app or read from the file). */
  media: Map<string, { bytes: Uint8Array; contentType: string }>;
  /** Chart models owned by a {@link ChartPartHook}, by `DrawingAnchor.ref`. */
  charts: Map<string, unknown>;
  /** Source `metadata.xml` root (dynamic arrays and value metadata). */
  metadata?: XmlElement;
  /** `cm` value meaning "dynamic array" in the source metadata. */
  dynamicArrayCm?: string;
  /** Persons of threaded comments: id → person attributes. */
  persons: Map<string, Record<string, string>>;
  /** Source `docProps/app.xml` parsed values (for `Company`, `Manager`…). */
  appProps?: Record<string, string>;
  /** Non-fatal problems met while reading. */
  warnings: string[];
}

/** Lets the charts package (WP-20) own chart parts; until then they are preserved as passthrough. */
export interface ChartPartHook {
  /**
   * Called for each chart part a drawing refers to. Return a chart model to take ownership of the part
   * (it is then regenerated by `write`), or `undefined` to keep the part verbatim.
   */
  read(part: { name: string; xml: XmlDocument; bytes: Uint8Array; pkg: OpcPackage }): unknown;
  /** Serialises a chart model owned by the hook. Extra parts (styles, colours) may be added to `pkg`. */
  write(
    model: unknown,
    ctx: { name: string; pkg: OpcPackage; sheetName: string },
  ): Uint8Array | string | XmlDocument;
}

export interface ReadXlsxOptions {
  chartHook?: ChartPartHook;
  /** Font metrics passed to the new workbook (column pixel widths). */
  metrics?: import('@kiln-ref/sheet-model').FontMetrics;
}

/** Computed results the writer uses for formulas (normally a `WorkbookCalc`). */
export interface CalcResults {
  getValue(sheet: number | string, row: number, col: number): CellValue;
  getSpillRange(
    sheet: number | string,
    row: number,
    col: number,
  ): { row: number; col: number; row1: number; col1: number; blocked?: boolean } | undefined;
}

export interface WriteXlsxOptions {
  chartHook?: ChartPartHook;
  /**
   * Computed formula results: cached values and dynamic-array spill ranges come from here when given,
   * else from the cell store and the passthrough.
   */
  calc?: CalcResults;
  /** Timestamp for new document properties (default: now). */
  now?: Date;
  /** Deflate level (default 6). */
  level?: 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;
  /** Package type: `.xlsx` (default), `.xlsm` (keeps VBA), `.xltx`/`.xltm` templates. */
  type?: 'xlsx' | 'xlsm' | 'xltx' | 'xltm';
}
