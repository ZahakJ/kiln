/**
 * Compatibility Checker data (`checkCompatibility`) and the conversions it announces.
 */
import { DefaultTemplate, docSchema } from '@kiln-ref/doc-model';
import { ContentType, OpcPackage, RelType } from '@kiln-ref/ooxml';
import { loadFixture } from '@kiln/testing';
import { Schema } from '@tiptap/pm/model';
import { describe, expect, it } from 'vitest';
import { checkCompatibility, readDocx } from '../../src/index.ts';
import { buildDocx, nodes, P, R, write } from './helpers.ts';

/** The Write schema plus block types an extension (or a future editor feature) might add. */
const extended = new Schema({
  nodes: docSchema.spec.nodes
    .addToEnd('callout', { group: 'block', content: 'block+', attrs: { kind: { default: 'note' } } })
    .addToEnd('codeBlock', { group: 'block', content: 'text*', marks: '', code: true })
    .addToEnd('mention', {
      group: 'inline',
      inline: true,
      atom: true,
      attrs: { name: { default: '' } },
      leafText: (n) => `@${n.attrs.name}`,
    }),
  marks: docSchema.spec.marks.addToEnd('glow', {}),
});

function extendedDoc() {
  const base = DefaultTemplate.create({ schema: extended });
  const s = extended;
  const glow = s.marks.glow?.create();
  const doc = s.node('doc', null, [
    s.node('section', { props: base.doc.firstChild?.attrs.props }, [
      s.node('callout', null, [s.node('paragraph', null, [s.text('Careful')])]),
      s.node('codeBlock', null, [s.text('const a = 1;\nconsole.log(a);')]),
      s.node('paragraph', null, [
        s.text('Hi '),
        s.node('mention', { name: 'Ann' }),
        s.text(' shiny', glow ? [glow] : []),
      ]),
    ]),
  ]);
  return { ...base, doc };
}

describe('checkCompatibility', () => {
  it('finds nothing to report for a document read from Word', () => {
    const model = readDocx(loadFixture('docx/features/tables.docx'));
    expect(checkCompatibility(model)).toEqual([]);
  });

  it('announces conversions and losses for content Word cannot hold, with counts', () => {
    const issues = checkCompatibility(extendedDoc());
    const by = Object.fromEntries(issues.map((i) => [i.code, i]));
    expect(by['callout.bordered']).toMatchObject({ severity: 'change', count: 1, origin: 'save' });
    expect(by['codeBlock.styled']?.message).toContain("'Code' style");
    expect(by['node.text']).toMatchObject({ severity: 'loss', message: expect.stringContaining('mention') });
    expect(by['mark.unsupported']).toMatchObject({
      severity: 'loss',
      message: expect.stringContaining('glow'),
    });
    // Losses first.
    expect(issues[0]?.severity).toBe('loss');
  });

  it('writes callouts as bordered, shaded paragraphs and code blocks in the Code style', () => {
    const b = readDocx(write(extendedDoc()));
    const [callout, code1, code2, text] = nodes(b.doc, 'paragraph');
    expect(callout?.attrs.borders?.top).toMatchObject({ style: 'single' });
    expect(callout?.attrs.shading?.fill?.val).toBe('F2F2F2');
    expect(code1?.attrs.styleId).toBe('Code');
    expect([code1?.textContent, code2?.textContent]).toEqual(['const a = 1;', 'console.log(a);']);
    expect(b.styles.get('Code')?.rPr?.fonts?.ascii).toBe('Consolas');
    expect(text?.textContent).toBe('Hi @Ann shiny');
  });

  it('reports missing pictures, equations without markup and broken note references', () => {
    const base = DefaultTemplate.create();
    const s = base.doc.type.schema;
    const doc = s.node('doc', null, [
      s.node('section', { props: base.doc.firstChild?.attrs.props }, [
        s.node('paragraph', null, [
          s.nodes.image?.create({ mediaId: '/word/media/nope.png' }) as never,
          s.nodes.equation?.create({ omml: '' }) as never,
          s.nodes.footnoteRef?.create({ id: '9' }) as never,
          s.text('bad\u0001char'),
        ]),
      ]),
    ]);
    const codes = checkCompatibility({ ...base, doc }).map((i) => i.code);
    expect(codes).toEqual(
      expect.arrayContaining(['image.missing', 'equation.unsaved', 'note.missing', 'text.invalidChars']),
    );
  });

  it('includes what was already lost or kept read-only when the file was opened', () => {
    const model = readDocx(
      buildDocx({
        body:
          P(`${R('a')}<w:permStart w:id="1" w:edGrp="everyone"/>${R('b')}<w:permEnd w:id="1"/>`) +
          P(
            '<w:r><w:drawing><wp:inline><wp:extent cx="1" cy="1"/><wp:docPr id="1" name="d"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:graphic></wp:inline></w:drawing></w:r>',
          ),
      }),
    );
    const issues = checkCompatibility(model);
    expect(issues.find((i) => i.code === 'permission')).toMatchObject({ severity: 'loss', origin: 'open' });
    expect(issues.find((i) => i.code === 'drawing.smartArt')).toMatchObject({
      severity: 'info',
      origin: 'open',
    });
  });

  it('warns that macros are removed when saving a macro-enabled document as .docx', () => {
    const pkg = OpcPackage.open(buildDocx({ body: P(R('m')) }));
    pkg.setContentType('/word/document.xml', 'application/vnd.ms-word.document.macroEnabled.main+xml');
    pkg.addPart('/word/vbaProject.bin', 'application/vnd.ms-office.vbaProject', new Uint8Array([1]));
    pkg.addRelationship('/word/document.xml', RelType.vbaProject, '/word/vbaProject.bin');
    const model = readDocx(pkg.save());
    expect(checkCompatibility(model, { format: 'docx' }).map((i) => i.code)).toContain('macros.removed');
    expect(checkCompatibility(model, { format: 'docm' }).map((i) => i.code)).not.toContain('macros.removed');
    expect(model.mainContentType).not.toBe(ContentType.docxDocument);
  });
});
