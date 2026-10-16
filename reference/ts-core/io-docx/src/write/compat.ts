/**
 * Compatibility Checker data for saving as Word (File ▸ Info ▸ Check for Issues ▸ Check Compatibility):
 * what in the document will be lost or changed by `writeDocx`, grouped by feature with a count of
 * occurrences, in the spirit of Word's own checker.
 */
import type { Node as PMNode } from '@tiptap/pm/model';
import type { DocxWarning } from '../common/warnings.ts';
import type { DocxFormat, DocxWriteModel } from './index.ts';

export type CompatibilitySeverity =
  /** Content or formatting will be lost. */
  | 'loss'
  /** Kept, but saved in a different form (it may look or behave differently in Word). */
  | 'change'
  /** Kept as it was read; nothing new is lost by saving. */
  | 'info';

export interface CompatibilityIssue {
  code: string;
  severity: CompatibilitySeverity;
  /** One-line summary for the checker list. */
  message: string;
  /** Number of occurrences in the document. */
  count: number;
  /** `save` for issues caused by saving, `open` for what was already lost or kept read-only on open. */
  origin: 'save' | 'open';
}

export interface CheckCompatibilityOptions {
  format?: DocxFormat;
}

/** Node types the DOCX writer maps one-to-one. */
const NATIVE_NODES = new Set([
  'doc',
  'section',
  'header',
  'footer',
  'paragraph',
  'table',
  'tableRow',
  'tableCell',
  'toc',
  'footnote',
  'opaqueBlock',
  'text',
  'tab',
  'softBreak',
  'pageBreak',
  'columnBreak',
  'image',
  'shape',
  'field',
  'footnoteRef',
  'equation',
  'bookmarkStart',
  'bookmarkEnd',
  'opaqueInline',
]);

/** Marks the DOCX writer maps one-to-one (`spellError` is editor state and never saved). */
const NATIVE_MARKS = new Set([
  'runStyle',
  'link',
  'bold',
  'italic',
  'underline',
  'strike',
  'dstrike',
  'caps',
  'smallCaps',
  'hidden',
  'vertAlign',
  'color',
  'highlight',
  'runShading',
  'fontFamily',
  'fontSize',
  'charSpacing',
  'runProps',
  'sdt',
  'comment',
  'insertion',
  'deletion',
  'formatChange',
  'spellError',
]);

const CONVERSIONS: Record<string, { code: string; message: string }> = {
  callout: {
    code: 'callout.bordered',
    message: 'Callout blocks are saved as bordered, shaded paragraphs.',
  },
  codeBlock: {
    code: 'codeBlock.styled',
    message: "Code blocks are saved as paragraphs in the 'Code' style with shading.",
  },
};

// biome-ignore lint/suspicious/noControlCharactersInRegex: XML 1.0 forbids these characters.
const INVALID_XML = /[\u0000-\u0008\u000B\u000C\u000E-\u001F￾￿]/;

/** Lists what saving `model` as a Word file will lose or change. */
export function checkCompatibility(
  model: DocxWriteModel,
  options: CheckCompatibilityOptions = {},
): CompatibilityIssue[] {
  const counts = new Map<string, CompatibilityIssue>();
  const add = (
    code: string,
    severity: CompatibilitySeverity,
    message: string,
    origin: 'save' | 'open' = 'save',
  ) => {
    const key = `${origin}|${code}|${message}`;
    const existing = counts.get(key);
    if (existing) existing.count += 1;
    else counts.set(key, { code, severity, message, count: 1, origin });
  };

  const media = model.media ?? new Map();
  const visit = (n: PMNode) => {
    const name = n.type.name;
    if (!NATIVE_NODES.has(name)) {
      const conv = CONVERSIONS[name];
      if (conv) add(conv.code, 'change', conv.message);
      else if (n.isInline && n.isLeaf)
        add('node.text', 'loss', `"${name}" objects are saved as their text only.`);
      else add('node.flattened', 'change', `"${name}" blocks are saved as their contents only.`);
    }
    for (const m of n.marks) {
      if (!NATIVE_MARKS.has(m.type.name))
        add('mark.unsupported', 'loss', `"${m.type.name}" formatting is not saved.`);
    }
    switch (name) {
      case 'image': {
        const id = n.attrs.mediaId as string | null;
        const src = (n.attrs.src as string | null) ?? '';
        if (!(id && media.has(id)) && !src.startsWith('data:')) {
          add('image.missing', 'loss', 'Pictures whose image data is not available are not saved.');
        }
        if (n.attrs.placement === 'anchor' && n.attrs.wrap === 'none') {
          add(
            'image.wrapNone',
            'change',
            'Floating pictures without wrapping are saved in front of the text.',
          );
        }
        break;
      }
      case 'equation':
        if (!n.attrs.omml)
          add('equation.unsaved', 'loss', 'Equations without Office Math markup are not saved.');
        break;
      case 'shape': {
        const xml = (n.attrs.xml as string | null) ?? '';
        if (xml.startsWith('<mc:AlternateContent')) {
          add(
            'shape.fallback',
            'info',
            'Changed shapes lose their compatibility picture for Word 2007 and older.',
          );
        }
        break;
      }
      case 'text':
        if (INVALID_XML.test(n.text ?? '')) {
          add('text.invalidChars', 'loss', 'Control characters that Word files cannot contain are removed.');
        }
        break;
      case 'footnoteRef': {
        const kind = n.attrs.kind === 'endnote' ? 'endnote' : 'footnote';
        let found = false;
        model.doc.forEach((c) => {
          if (c.type.name === 'footnote' && c.attrs.kind === kind && c.attrs.id === n.attrs.id) found = true;
        });
        if (!found) add('note.missing', 'loss', `References to ${kind}s that no longer exist are removed.`);
        break;
      }
      default:
        break;
    }
    n.forEach(visit);
  };
  visit(model.doc);

  for (const c of model.comments ?? []) {
    if (c.parentId && !(model.comments ?? []).some((p) => p.id === c.parentId)) {
      add(
        'comment.orphanReply',
        'change',
        'Replies whose first comment was deleted are saved as separate comments.',
      );
    }
  }

  const format = options.format;
  const macroModel = /macroEnabled/i.test(model.mainContentType ?? '');
  if (format && macroModel && format !== 'docm' && format !== 'dotm') {
    add('macros.removed', 'loss', 'Macros (VBA) are removed: this format cannot contain them.');
  }

  for (const w of (model as { warnings?: DocxWarning[] }).warnings ?? []) {
    add(w.code, w.severity === 'warning' ? 'loss' : 'info', w.message, 'open');
  }

  const rank: Record<CompatibilitySeverity, number> = { loss: 0, change: 1, info: 2 };
  return [...counts.values()].sort(
    (a, b) =>
      rank[a.severity] - rank[b.severity] || (a.origin === b.origin ? 0 : a.origin === 'save' ? -1 : 1),
  );
}
