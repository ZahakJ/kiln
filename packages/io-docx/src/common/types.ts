/**
 * Types shared by the DOCX reader and writer. They extend the doc-model types with the WordprocessingML
 * details a faithful round trip needs but the editor does not interpret (raw XML of unmodelled
 * properties, page borders, note properties, …). Extensions are plain JSON so they live inside node
 * attributes and survive `toJSON()`.
 */
import type {
  Border,
  DocComment,
  DocSettings,
  ParagraphProps,
  RevisionInfo,
  RunProps,
  SectionProps,
  WriteDocument,
} from '@officina/doc-model';
import type { DocumentProperties, PassthroughSet } from '@officina/ooxml';
import type { Node as PMNode } from '@tiptap/pm/model';
import type { DocxWarning } from './warnings.ts';

/** Run properties plus file-only details carried in the `runProps` mark / style `rPr`. */
export interface DocxRunProps extends RunProps {
  /** Unmodelled `w:rPr` children (`w:noProof`, `w:em`, `w14:ligatures`, …) as raw XML, in file order. */
  xml?: string[];
  /** The run was a `w:sym` symbol character: original font and `w:char` code (hex). */
  sym?: { font: string; char: string };
  /** `w:rFonts/@w:hint` (`eastAsia`, `cs`, `default`). */
  fontHint?: string;
}

/** Paragraph properties plus unmodelled `w:pPr` children, as used in styles and numbering levels. */
export interface DocxParagraphProps extends ParagraphProps {
  xml?: string[];
}

/** `w:pgBorders`. */
export interface PageBorders {
  offsetFrom?: 'page' | 'text';
  display?: 'allPages' | 'firstPage' | 'notFirstPage';
  zOrder?: 'front' | 'back';
  top?: Border;
  left?: Border;
  bottom?: Border;
  right?: Border;
}

/** `w:footnotePr` / `w:endnotePr` (section or document level). */
export interface NoteProps {
  position?: string;
  numFmt?: string;
  numStart?: number;
  numRestart?: 'continuous' | 'eachSect' | 'eachPage';
}

/** Section properties plus the section details the editor does not lay out yet. */
export interface DocxSectionProps extends SectionProps {
  pageBorders?: PageBorders;
  footnotePr?: NoteProps;
  endnotePr?: NoteProps;
  textDirection?: string;
  rtlGutter?: boolean;
  formProt?: boolean;
  noEndnote?: boolean;
  /** Tracked change of the section properties (`w:sectPrChange`), raw XML. */
  sectPrChange?: string;
  /** Any other `w:sectPr` children, raw XML. */
  xml?: string[];
}

/** `settings.xml` values beyond doc-model's `DocSettings`. */
export interface DocxSettings extends DocSettings {
  /** `w:decimalSymbol` (used by field number formatting and decimal tabs). */
  decimalSymbol?: string;
  /** `w:listSeparator`. */
  listSeparator?: string;
  /** `w:footnotePr` / `w:endnotePr` at document level. */
  footnotePr?: NoteProps;
  endnotePr?: NoteProps;
  /** `w:themeFontLang`. */
  themeFontLang?: { val?: string; eastAsia?: string; bidi?: string };
  /** `w:documentProtection` edit restriction (the hash is kept in `settingsXml`). */
  protection?: { edit?: string; enforced: boolean };
  /** `w:doNotTrackMoves`, `w:doNotTrackFormatting`. */
  doNotTrackMoves?: boolean;
  doNotTrackFormatting?: boolean;
  /** `w:autoHyphenation`. */
  autoHyphenation?: boolean;
  /** Page colour (`w:document/w:background/@w:color`, `RRGGBB`), shown when the settings ask for it. */
  background?: string;
}

/** A footnote or endnote story. */
export interface DocxNote {
  id: string;
  kind: 'footnote' | 'endnote';
  /** The `footnote` node as placed at the end of `doc`. */
  node: PMNode;
}

/** A comment author from `people.xml`. */
export interface DocxPerson {
  author: string;
  providerId?: string;
  userId?: string;
}

/** A chart object in the document, kept for the charts package (WP-20). */
export interface DocxChartRef {
  /** Part holding the drawing (`/word/document.xml`, a header part, …). */
  sourcePart: string;
  /** Relationship id of the chart from `sourcePart`. */
  relId: string;
  /** Resolved chart part (`/word/charts/chart1.xml`). */
  chartPart?: string;
  /** Extent in EMU. */
  width: number;
  height: number;
  placement: 'inline' | 'anchor';
  name?: string;
  description?: string;
}

/** Resolved relationship target (part name for internal targets, the URL for external ones). */
export interface DocxRelTarget {
  type: string;
  target: string;
  external: boolean;
}

/** Raw XML the writer needs to reproduce parts the model only partly covers. */
export interface DocxExtras {
  /** `w:latentStyles` of styles.xml. */
  latentStyles?: string;
  /** The full settings.xml root, so a writer can patch modelled values and keep the rest. */
  settingsXml?: string;
  /** `w:numPicBullet` definitions from numbering.xml. */
  numPicBullets?: string[];
  /** `w:background` of document.xml. */
  background?: string;
  /** Namespace declarations and `mc:Ignorable` of the document root, for a faithful writer. */
  documentRootAttrs?: Record<string, string>;
}

/** Everything `readDocx` produces. A `WriteDocument` plus the DOCX-specific side data. */
export interface ReadDocxResult extends WriteDocument {
  settings: DocxSettings;
  /** Section properties in document order (the same objects as the section nodes' `props`). */
  sections: DocxSectionProps[];
  footnotes: DocxNote[];
  endnotes: DocxNote[];
  comments: DocComment[];
  people: DocxPerson[];
  properties: DocumentProperties;
  /** Parts and relationships the reader did not model, for the writer to re-emit. */
  passthrough: PassthroughSet;
  charts: DocxChartRef[];
  /** Relationship id → target, per source part, for ids referenced from preserved raw XML. */
  relationships: Map<string, Map<string, DocxRelTarget>>;
  /** Part name of the main document (`/word/document.xml` normally). */
  mainPart: string;
  /** Content type of the main part (tells .docx, .docm, .dotx and .dotm apart). */
  mainContentType: string;
  extras: DocxExtras;
  warnings: DocxWarning[];
}

export type { RevisionInfo };
