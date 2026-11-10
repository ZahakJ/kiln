import type { PrintPreview as PrintPreviewData, PrintSettings } from '@kiln/protocol';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import '../../chrome/test-matchers.ts';
import { isPdf, pdfFileName } from '../../print/printPdf.ts';
import { ExportPdfPane } from './ExportPdfPane.tsx';
import { PrintPane } from './PrintPane.tsx';
import { DEFAULT_PRINT_SETTINGS, pageMode, sheetLabel } from './settings.ts';

vi.setConfig({ testTimeout: 20_000 });

function preview(sheets: number, sheet = 0): PrintPreviewData {
  return {
    sheets,
    sheet,
    width_pt: 612,
    height_pt: 792,
    pages: [sheet],
    page_count: sheets,
    scene: {
      revision: 0,
      reset: true,
      scale: 1,
      extent: { x: 0, y: 0, w: 864, h: 1104 },
      background: { Rgba: 0 },
      tiles: [],
      removed: [],
      atlas: [],
      images: [],
      overlay: null,
    },
  };
}

describe('print settings', () => {
  it('labels sheets and page modes like Word', () => {
    expect(sheetLabel(0, 12, [0], 1)).toBe('Page 1 of 12');
    expect(sheetLabel(1, 6, [2, 3], 2)).toBe('Sheet 2 of 6 (pages 3–4)');
    expect(pageMode(DEFAULT_PRINT_SETTINGS)).toBe('all');
    expect(pageMode({ ...DEFAULT_PRINT_SETTINGS, pages: '2-3' })).toBe('custom');
    expect(pageMode({ ...DEFAULT_PRINT_SETTINGS, parity: 'Even' })).toBe('even');
    expect(pdfFileName('Q3: plan.docx')).toBe('Q3 plan.pdf');
    expect(pdfFileName('')).toBe('Document.pdf');
    expect(isPdf([0x25, 0x50, 0x44, 0x46, 0x2d, 0x31])).toBe(true);
    expect(isPdf([0x50, 0x4b])).toBe(false);
  });
});

describe('PrintPane', () => {
  it('asks the core for previews, follows the page box and prints with the settings', async () => {
    const user = userEvent.setup();
    const requests: { settings: PrintSettings; sheet: number }[] = [];
    const requestPreview = vi.fn(async (settings: PrintSettings, sheet: number) => {
      requests.push({ settings, sheet });
      return preview(settings.pages ? 2 : 5, Math.min(sheet, settings.pages ? 1 : 4));
    });
    const onPrint = vi.fn(async (_settings: PrintSettings) => {});
    render(<PrintPane requestPreview={requestPreview} onPrint={onPrint} onSavePdf={vi.fn()} />);
    await waitFor(() => expect(screen.getByTestId('print-sheet')).toHaveTextContent('Page 1 of 5'));
    await user.click(screen.getByRole('button', { name: 'Next page' }));
    await waitFor(() => expect(screen.getByTestId('print-sheet')).toHaveTextContent('Page 2 of 5'));

    const box = screen.getByTestId('print-pages');
    await user.clear(box);
    await user.type(box, '3-4{Enter}');
    await waitFor(() => expect(screen.getByTestId('print-sheet')).toHaveTextContent('Page 1 of 2'));
    expect(requests.at(-1)?.settings.pages).toBe('3-4');
    expect(requests.at(-1)?.sheet).toBe(0);

    await user.click(screen.getByTestId('print-go'));
    await waitFor(() => expect(onPrint).toHaveBeenCalledTimes(1));
    expect(onPrint.mock.calls[0]?.[0]).toMatchObject({ pages: '3-4', copies: 1 });
    expect(await screen.findByRole('status')).toHaveTextContent('Sent to the print dialog.');
  });

  it('shows what the core says about a page box it cannot read', async () => {
    const user = userEvent.setup();
    const requestPreview = vi.fn(async (settings: PrintSettings) => {
      if (settings.pages === 'x') throw new Error('"x" is not a page or page range (try 1-3, 5)');
      return preview(3);
    });
    render(<PrintPane requestPreview={requestPreview} onPrint={vi.fn()} onSavePdf={vi.fn()} />);
    await waitFor(() => expect(screen.getByTestId('print-sheet')).toHaveTextContent('Page 1 of 3'));
    const box = screen.getByTestId('print-pages');
    await user.type(box, 'x{Enter}');
    expect(await screen.findByRole('alert')).toHaveTextContent('not a page or page range');
    expect(box).toHaveAttribute('aria-invalid', 'true');
  });
});

describe('ExportPdfPane', () => {
  it('exports with the options and reports accessibility', async () => {
    const user = userEvent.setup();
    const onExport = vi.fn(async (_settings: PrintSettings) => ({
      name: 'Report.pdf',
      pdfUa: false,
      uaIssues: ['a figure has no alt text'],
    }));
    render(<ExportPdfPane onExport={onExport} />);
    await user.click(screen.getByRole('checkbox', { name: /Create bookmarks/ }));
    await user.click(screen.getByTestId('pdf-export-go'));
    await waitFor(() => expect(onExport).toHaveBeenCalledTimes(1));
    expect(onExport.mock.calls[0]?.[0]).toMatchObject({ bookmarks: false, tagged: true });
    const result = await screen.findByTestId('pdf-export-result');
    expect(result).toHaveTextContent('Saved Report.pdf');
    expect(result).toHaveTextContent('a figure has no alt text');
  });
});
