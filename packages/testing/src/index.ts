/**
 * @kiln/testing — shared test infrastructure (dev-only, never shipped): the fixture corpus in
 * `fixtures/`, independent oracles (LibreOffice, ExcelJS, mammoth, openpyxl/python-docx/python-pptx),
 * a PNG codec and image comparison for visual tests. Document parsing and the round-trip fidelity
 * scorer live in Rust (`kiln-testing`, WP-60); the v1.0 TypeScript scorer is kept read-only in
 * `reference/ts-core/testing/` as the port oracle.
 */
export const PACKAGE_NAME = '@kiln/testing' as const;

export * from './fixtures.ts';
export * from './image-diff.ts';
export * from './oracles/exceljsReader.ts';
export * from './oracles/mammothReader.ts';
export * from './oracles/pythonReader.ts';
export * from './oracles/soffice.ts';
export * from './paths.ts';
export * from './png.ts';
