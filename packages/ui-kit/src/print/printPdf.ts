/**
 * Handing a PDF from the core to the browser: downloading it (the web build's Export ▸ PDF; the
 * desktop saves through the core and its save dialog). Printing goes through the host
 * (`HostOp.PrintPdf` / `HostOp.OpenFile`).
 */

function blobOf(bytes: Uint8Array | readonly number[]): Blob {
  const data = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
  return new Blob([data as Uint8Array<ArrayBuffer>], { type: 'application/pdf' });
}

/** Whether `bytes` start like a PDF file. */
export function isPdf(bytes: Uint8Array | readonly number[]): boolean {
  const sig = [0x25, 0x50, 0x44, 0x46, 0x2d]; // %PDF-
  return sig.every((b, i) => bytes[i] === b);
}

/** A file name for the PDF of a document title (no path separators or reserved characters). */
export function pdfFileName(title: string): string {
  const cleaned = Array.from(title.replace(/\.[a-z0-9]{2,5}$/i, ''), (c) =>
    c.charCodeAt(0) < 0x20 || '\\/:*?"<>|'.includes(c) ? ' ' : c,
  ).join('');
  const base = cleaned.replace(/\s+/g, ' ').trim();
  return `${base || 'Document'}.pdf`;
}

/** Saves the PDF through the browser's download (an `<a download>` click). */
export function downloadPdf(
  bytes: Uint8Array | readonly number[],
  name: string,
  doc: Document = document,
): void {
  const url = URL.createObjectURL(blobOf(bytes));
  const a = doc.createElement('a');
  a.href = url;
  a.download = name;
  a.rel = 'noopener';
  a.style.display = 'none';
  doc.body.append(a);
  a.click();
  a.remove();
  // Revoke later: some browsers read the URL after the click handler returns.
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
