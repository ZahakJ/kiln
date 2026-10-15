/** mammoth as an independent .docx reader (dev-only oracle). */
import mammoth from 'mammoth';

export interface MammothResult {
  /** Plain text, paragraphs separated by blank lines. */
  text: string;
  /** Semantic HTML (headings, lists, tables, emphasis). */
  html: string;
  /** Warnings mammoth raised about content it could not map. */
  messages: string[];
}

export async function readDocxWithMammoth(bytes: Uint8Array): Promise<MammothResult> {
  const buffer = Buffer.from(bytes);
  const [raw, html] = await Promise.all([
    mammoth.extractRawText({ buffer }),
    mammoth.convertToHtml({ buffer }),
  ]);
  return {
    text: raw.value,
    html: html.value,
    messages: [...raw.messages, ...html.messages].map((m) => `${m.type}: ${m.message}`),
  };
}
