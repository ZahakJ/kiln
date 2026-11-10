/**
 * Plain text files: Word's File Conversion question when the encoding of an opened text
 * file is uncertain (choose one, with a preview), and its warning when a save could not hold
 * every character in the file's encoding.
 */

import { t } from '@kiln/i18n';
import type { DocSummary, KilnClient, TextEncodingInfo } from '@kiln/protocol';
import { Button, Dialog, DialogContent } from '@kiln/ui-kit';
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';

async function encodingOf(client: KilnClient, doc: number): Promise<TextEncodingInfo | null> {
  const r = await client.query(doc, { Write: 'TextEncoding' });
  if ('Write' in r && typeof r.Write === 'object' && 'TextEncoding' in r.Write) return r.Write.TextEncoding;
  return null;
}

export function useTextEncoding(
  client: KilnClient | null,
  summary: DocSummary | null,
  enabled: boolean,
  execute: (id: string, args?: unknown) => void,
): ReactNode {
  const [ask, setAsk] = useState<TextEncodingInfo | null>(null);
  const [choice, setChoice] = useState('');
  const [lost, setLost] = useState<TextEncodingInfo | null>(null);
  const asked = useRef<number | null>(null);
  const doc = summary?.doc ?? null;
  const isText = enabled && summary?.format === 'Txt';

  // The File Conversion question, once per opened text file.
  useEffect(() => {
    if (!client || doc === null || !isText || asked.current === doc) return;
    asked.current = doc;
    void encodingOf(client, doc).then(
      (info) => {
        if (info && !info.confident && info.choices.length > 0) {
          setChoice(info.encoding);
          setAsk(info);
        }
      },
      () => undefined,
    );
  }, [client, doc, isText]);

  // After a save: characters the encoding could not hold.
  useEffect(() => {
    if (!client || doc === null || !enabled) return;
    return client.subscribe((e) => {
      if (typeof e !== 'object' || !('Saved' in e)) return;
      const s = e.Saved.summary;
      if (s.doc !== doc || s.format !== 'Txt') return;
      void encodingOf(client, doc).then(
        (info) => {
          if (info && info.unrepresentable.length > 0) setLost(info);
        },
        () => undefined,
      );
    });
  }, [client, doc, enabled]);

  const titleOf = useCallback(
    (info: TextEncodingInfo, name: string) => info.choices.find((c) => c.name === name)?.title ?? name,
    [],
  );
  const preview = ask?.choices.find((c) => c.name === choice)?.preview ?? '';

  return (
    <>
      <Dialog
        open={ask !== null}
        onOpenChange={(open) => {
          if (!open) setAsk(null);
        }}
      >
        <DialogContent
          title={t('File Conversion')}
          description={t('Select the encoding that makes your document readable.')}
          size="lg"
          data-testid="file-conversion"
          footer={
            <>
              <Button
                variant="primary"
                onClick={() => {
                  const was = ask?.encoding;
                  setAsk(null);
                  if (choice && choice !== was) {
                    execute('write.file.text_encoding', { encoding: choice, reload: true });
                  }
                }}
              >
                {t('OK')}
              </Button>
              <Button
                onClick={() => {
                  setAsk(null);
                  execute('suite.file.close');
                }}
              >
                {t('Cancel')}
              </Button>
            </>
          }
        >
          <div className="kiln-file-conversion">
            <label htmlFor="kiln-text-encoding">{t('Text encoding')}</label>
            <select
              id="kiln-text-encoding"
              size={Math.min(8, ask?.choices.length ?? 1)}
              value={choice}
              onChange={(e) => setChoice(e.target.value)}
            >
              {ask?.choices.map((c) => (
                <option key={c.name} value={c.name}>
                  {c.title}
                </option>
              ))}
            </select>
            <span>{t('Preview')}</span>
            <pre className="kiln-file-conversion__preview" data-testid="file-conversion-preview">
              {preview}
            </pre>
          </div>
        </DialogContent>
      </Dialog>
      <Dialog
        open={lost !== null}
        onOpenChange={(open) => {
          if (!open) setLost(null);
        }}
      >
        <DialogContent
          title={t('Encoding Warning')}
          size="md"
          role="alertdialog"
          data-testid="encoding-warning"
          footer={
            <>
              <Button
                variant="primary"
                onClick={() => {
                  setLost(null);
                  execute('write.file.text_encoding', { encoding: 'utf-8', reload: false });
                  execute('suite.file.save');
                }}
              >
                {t('Save as Unicode (UTF-8)')}
              </Button>
              <Button onClick={() => setLost(null)}>{t('OK')}</Button>
            </>
          }
        >
          {lost ? (
            <>
              <p>
                {t(
                  'The document was saved in {encoding}, which cannot hold these characters. They were saved as question marks:',
                  { encoding: titleOf(lost, lost.encoding) },
                )}
              </p>
              <p className="kiln-file-conversion__lost" data-testid="encoding-warning-chars">
                {lost.unrepresentable.join(' ')}
              </p>
              <p>{t('Save it as Unicode (UTF-8) to keep every character.')}</p>
            </>
          ) : null}
        </DialogContent>
      </Dialog>
    </>
  );
}
