/**
 * Reader throughput on a synthetic 300-page document (PLAN §3.9: a 300-page .docx must show its first
 * page within 3 s; parsing is the first step of that budget).
 */
import { describe, expect, it } from 'vitest';
import { readDocx } from '../../src/index.ts';
import { buildDocx } from './helpers.ts';

function bigBody(pages: number): string {
  const parts: string[] = [];
  for (let page = 0; page < pages; page++) {
    parts.push(
      `<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Chapter ${page + 1}</w:t></w:r></w:p>`,
    );
    for (let i = 0; i < 8; i++) {
      parts.push(
        `<w:p w14:paraId="${(page * 100 + i).toString(16).padStart(8, '0').toUpperCase()}"><w:pPr><w:spacing w:after="160"/></w:pPr>` +
          '<w:r><w:rPr><w:b/><w:noProof/></w:rPr><w:t xml:space="preserve">Lorem ipsum dolor sit amet, </w:t></w:r>' +
          '<w:r><w:rPr><w:i/><w:color w:val="1F3864"/></w:rPr><w:t xml:space="preserve">consectetur adipiscing elit, sed do eiusmod tempor </w:t></w:r>' +
          '<w:r><w:t>incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco.</w:t></w:r></w:p>',
      );
    }
    if (page % 5 === 0) {
      const row = `<w:tr>${'<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc>'.repeat(4)}</w:tr>`;
      parts.push(
        `<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/></w:tblPr><w:tblGrid>${'<w:gridCol w:w="2000"/>'.repeat(4)}</w:tblGrid>${row.repeat(6)}</w:tbl>`,
      );
    }
  }
  return parts.join('');
}

describe('performance', () => {
  it('reads a 300-page document in well under the first-page budget', () => {
    const bytes = buildDocx({ body: bigBody(300) });
    readDocx(bytes); // warm-up (JIT)
    const t0 = performance.now();
    const r = readDocx(bytes);
    const ms = performance.now() - t0;
    expect(r.doc.child(0).childCount).toBe(300 * 9 + 60);
    expect(ms).toBeLessThan(1500);
  });
});
