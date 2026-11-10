import { describe, expect, it } from 'vitest';
import { locationOf } from './location.ts';

describe('the location of a recent file', () => {
  it('names the folders of a path, and the browser places without their ids', () => {
    expect(locationOf('/data/ana/Studio/Accounts/Budget.xlsx')).toBe('Studio › Accounts');
    expect(locationOf('C:\\Users\\ana\\Report.docx')).toBe('Users › ana');
    expect(locationOf('Report.docx')).toBe('This device');
    expect(locationOf('handle:81ed9e8e-8765/Tables.docx')).toBe('This device');
    expect(locationOf('opfs:documents/Plan.kwrite')).toBe('This browser');
  });
});
