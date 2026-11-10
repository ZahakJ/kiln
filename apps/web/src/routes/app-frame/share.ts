/**
 * Share ▸ a copy of the document, for the phone's overflow menu. The Android
 * app hands the saved file, or a PDF of the document as it is, to the system's share sheet
 * (`shareDocument`). In a browser the Web Share API sends the same files where it can share files;
 * elsewhere the copy is downloaded. A copy that cannot be sent rejects with a sentence to show.
 */
import { t } from '@kiln/i18n';
import { type DocSummary, isTauri, type KilnClient, KilnError } from '@kiln/protocol';
import {
  DEFAULT_PRINT_SETTINGS,
  errorText,
  pdfFileName,
  type ShareCopy,
  type ShareCopyKind,
} from '@kiln/ui-kit';
import { useMemo } from 'react';
import { readSavedFile } from '../../client/index.ts';
import { isAndroid, shareDocument } from '../../desktop.ts';

const MIME: Readonly<Record<string, string>> = {
  docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  docm: 'application/vnd.ms-word.document.macroEnabled.12',
  dotx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.template',
  xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  xlsm: 'application/vnd.ms-excel.sheet.macroEnabled.12',
  pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  pptm: 'application/vnd.ms-powerpoint.presentation.macroEnabled.12',
  odt: 'application/vnd.oasis.opendocument.text',
  ods: 'application/vnd.oasis.opendocument.spreadsheet',
  odp: 'application/vnd.oasis.opendocument.presentation',
  csv: 'text/csv',
  md: 'text/markdown',
  txt: 'text/plain',
  html: 'text/html',
  pdf: 'application/pdf',
};

function mimeOf(name: string): string {
  const ext = name.slice(name.lastIndexOf('.') + 1).toLowerCase();
  return MIME[ext] ?? 'application/octet-stream';
}

/** Whether this browser's share sheet takes files (the Web Share API, level 2). */
export function canShareFiles(): boolean {
  try {
    const probe = new File([new Uint8Array(1)], 'Kiln.pdf', { type: 'application/pdf' });
    return typeof navigator.share === 'function' && navigator.canShare?.({ files: [probe] }) === true;
  } catch {
    return false;
  }
}

/** Saves bytes through the browser's download (an `<a download>` click). */
function download(file: File): void {
  const url = URL.createObjectURL(file);
  const a = document.createElement('a');
  a.href = url;
  a.download = file.name;
  a.rel = 'noopener';
  a.style.display = 'none';
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

/** The file name of a saved document's path ("…/Budget.xlsx" → "Budget.xlsx"). */
function nameOf(path: string): string {
  return path.slice(Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')) + 1);
}

async function copyOf(client: KilnClient, summary: DocSummary, kind: ShareCopyKind): Promise<File> {
  if (kind === 'pdf') {
    const r = await client.query(summary.doc, { Pdf: { settings: DEFAULT_PRINT_SETTINGS } });
    if (!('Pdf' in r)) throw new Error(t('The PDF could not be made. Try again.'));
    const bytes = Uint8Array.from(r.Pdf.bytes);
    return new File([bytes], pdfFileName(summary.title), { type: 'application/pdf' });
  }
  if (!summary.path) throw new Error(t('Save the document first, then share it.'));
  if (summary.dirty)
    throw new Error(t('Save your changes first: the copy is the file as it was last saved.'));
  const bytes = await readSavedFile(summary.path);
  if (!bytes) throw new Error(t('Kiln cannot read the saved file now. Open it again, then share it.'));
  const name = nameOf(summary.path);
  return new File([bytes as Uint8Array<ArrayBuffer>], name, { type: mimeOf(name) });
}

/** The phone's Share ▸ copy for the window's document (none on a desktop app). */
export function useShareCopy(
  client: KilnClient | null,
  summary: DocSummary | null,
  extension: string | undefined,
): ShareCopy | undefined {
  return useMemo(() => {
    if (!client || !summary) return undefined;
    const fileName = summary.path ? nameOf(summary.path) : `${summary.title}${extension ?? ''}`;
    if (isAndroid())
      return {
        mode: 'share',
        fileName,
        send: async (kind) => {
          try {
            await shareDocument(summary.doc, kind === 'pdf');
          } catch (error) {
            throw new Error(errorText(KilnError.from(error)));
          }
        },
      };
    if (isTauri()) return undefined;
    const share = canShareFiles();
    return {
      mode: share ? 'share' : 'download',
      fileName,
      send: async (kind) => {
        const file = await copyOf(client, summary, kind);
        if (!share) {
          download(file);
          return;
        }
        try {
          await navigator.share({ files: [file], title: summary.title });
        } catch (error) {
          // Closing the share sheet is not a failure.
          if (error instanceof DOMException && error.name === 'AbortError') return;
          throw new Error(t('No app took the copy. Try again, or download it instead.'));
        }
      },
    };
  }, [client, summary, extension]);
}
