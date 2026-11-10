/**
 * File ▸ Export ▸ Change File Type for presentations: the slides as PNG or JPEG pictures at a
 * size, every slide (a `.zip` of a folder named after the presentation) or just the current one,
 * drawn by the core and saved as a download.
 */
import { msg, t, tx } from '@kiln/i18n';
import type { DeckPictureFormat } from '@kiln/protocol';
import { Button, Icon, Select } from '@kiln/ui-kit';
import { ImageDown } from 'lucide-react';
import { useId, useState } from 'react';
import type { DeckEditorModel } from '../model.ts';
import { messageOf } from '../model.ts';

/** The sizes offered, by width in pixels (the height follows the slide). */
export const PICTURE_WIDTHS = [
  { value: '1280', label: msg('1280 px wide (HD)') },
  { value: '1920', label: msg('1920 px wide (Full HD)') },
  { value: '3840', label: msg('3840 px wide (4K)') },
] as const;

/** Saves bytes as a file through the browser's downloads. */
export function download(bytes: readonly number[], name: string, mime: string): void {
  const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: mime }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.rel = 'noopener';
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

export function ExportPicturesPane({ model }: { model: DeckEditorModel }) {
  const ids = useId();
  const [format, setFormat] = useState<DeckPictureFormat>('Png');
  const [which, setWhich] = useState<'all' | 'current'>('all');
  const [width, setWidth] = useState('1920');
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  const run = async () => {
    setBusy(true);
    setDone(null);
    setProblem(null);
    try {
      const current = model.state.deck?.current ?? 0;
      const r = await model.client.query(model.doc, {
        Deck: {
          ExportPictures: { slides: which === 'all' ? null : [current], format, width: Number(width) },
        },
      });
      const file =
        typeof r === 'object' && r !== null && 'Deck' in r && 'File' in r.Deck ? r.Deck.File : null;
      if (!file) throw new Error('The core did not return the pictures.');
      download(file.bytes, file.name, file.mime);
      setDone(t('Saved {name}', { name: file.name }));
    } catch (error) {
      setProblem(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="kiln-pdf-export" data-testid="deck-export-pictures">
      <section className="kiln-pdf-export__card" aria-labelledby={`${ids}-h`}>
        <h2 id={`${ids}-h`} className="kiln-bs-h2">
          {t('Export slides as pictures')}
        </h2>
        <p className="kiln-bs-muted">{t('Every slide as a picture file, or just the one you are on.')}</p>
        <div className="kiln-print__field">
          <span className="kiln-print__label" id={`${ids}-format`}>
            {t('File type')}
          </span>
          <Select
            labelId={`${ids}-format`}
            value={format}
            onValueChange={(v) => setFormat(v as DeckPictureFormat)}
            options={[
              { value: 'Png', label: t('PNG Portable Network Graphics') },
              { value: 'Jpeg', label: t('JPEG File Interchange Format') },
            ]}
            width="100%"
          />
        </div>
        <div className="kiln-print__field">
          <span className="kiln-print__label" id={`${ids}-which`}>
            {t('Slides')}
          </span>
          <Select
            labelId={`${ids}-which`}
            value={which}
            onValueChange={(v) => setWhich(v as 'all' | 'current')}
            options={[
              { value: 'all', label: t('All Slides') },
              { value: 'current', label: t('Just This One') },
            ]}
            width="100%"
          />
        </div>
        <div className="kiln-print__field">
          <span className="kiln-print__label" id={`${ids}-size`}>
            {t('Size')}
          </span>
          <Select
            labelId={`${ids}-size`}
            value={width}
            onValueChange={setWidth}
            options={PICTURE_WIDTHS.map((w) => ({ value: w.value, label: tx(w.label) }))}
            width="100%"
          />
        </div>
        <div>
          <Button
            variant="primary"
            icon={<Icon icon={ImageDown} size={16} />}
            onClick={() => void run()}
            busy={busy}
            disabled={busy}
            data-testid="deck-export-pictures-go"
          >
            {t('Export Pictures')}
          </Button>
        </div>
        {done ? (
          <p className="kiln-print__done" role="status">
            {done}
          </p>
        ) : null}
        {problem ? (
          <p className="kiln-print__error" role="alert">
            {problem}
          </p>
        ) : null}
      </section>
    </div>
  );
}
