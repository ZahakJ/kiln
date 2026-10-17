/**
 * Printing a PDF from the core in a browser (WP-67): the bytes are loaded into a hidden frame and
 * the frame's built-in PDF viewer prints them with the browser's print dialog. The web host
 * answers `HostOp.PrintPdf` with it; the desktop opens the file in the system viewer instead
 * (`HostOp.OpenFile`).
 */

/** Prints `bytes` (a PDF) through a hidden frame of `doc`. Resolves once the dialog was asked for. */
export function printPdfInFrame(
  bytes: Uint8Array | readonly number[],
  doc: Document = document,
): Promise<void> {
  const data = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
  const url = URL.createObjectURL(new Blob([data as Uint8Array<ArrayBuffer>], { type: 'application/pdf' }));
  const frame = doc.createElement('iframe');
  frame.title = 'Print';
  frame.setAttribute('aria-hidden', 'true');
  frame.tabIndex = -1;
  Object.assign(frame.style, {
    position: 'fixed',
    right: '0',
    bottom: '0',
    width: '1px',
    height: '1px',
    border: '0',
    opacity: '0',
  });
  const cleanup = () => {
    frame.remove();
    URL.revokeObjectURL(url);
  };
  return new Promise<void>((resolve, reject) => {
    frame.addEventListener(
      'load',
      () => {
        try {
          const win = frame.contentWindow;
          if (!win) throw new Error('the print frame has no window');
          win.focus();
          win.print();
          resolve();
          // The dialog is modal in most browsers; keep the frame a while for those where it is not.
          setTimeout(cleanup, 120_000);
        } catch (error) {
          cleanup();
          reject(error instanceof Error ? error : new Error(String(error)));
        }
      },
      { once: true },
    );
    frame.src = url;
    doc.body.append(frame);
  });
}
