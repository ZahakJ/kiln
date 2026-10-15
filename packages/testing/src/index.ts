/**
 * @officina/testing — shared test infrastructure (dev-only, never shipped): the fixture corpus,
 * independent oracles (LibreOffice, ExcelJS, mammoth, openpyxl/python-docx/python-pptx) and the
 * round-trip fidelity scorer.
 */
export const PACKAGE_NAME = '@officina/testing' as const;

export * from './fidelity/score.ts';
export * from './fixtures.ts';
export * from './oracles/exceljsReader.ts';
export * from './oracles/mammothReader.ts';
export * from './oracles/pythonReader.ts';
export * from './oracles/soffice.ts';
export * from './paths.ts';
export * from './png.ts';
