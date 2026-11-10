import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { TEMPLATE_STRINGS } from './strings.ts';

interface Entry {
  id: string;
  name: string;
  description: string;
}

const index: { templates: Entry[] } = JSON.parse(
  readFileSync(join(__dirname, '../../../../assets/templates/index.json'), 'utf8'),
);

describe('template names and descriptions', () => {
  it('are marked for translation exactly as the index has them', () => {
    expect(Object.keys(TEMPLATE_STRINGS).sort()).toEqual(index.templates.map((t) => t.id).sort());
    for (const t of index.templates) {
      expect(TEMPLATE_STRINGS[t.id], t.id).toEqual({ name: t.name, description: t.description });
    }
  });
});
