/**
 * @kiln-ref/doc-model — the Kiln Write document model: ProseMirror schema, WordprocessingML
 * property types, style sheet and Word-accurate formatting resolution, list numbering, sections,
 * fields, theme fonts/colours, font substitution, the default template, (de)serialisation,
 * statistics and editing commands.
 */
export const PACKAGE_NAME = '@kiln-ref/doc-model' as const;

export * from './commands.ts';
export * from './fields.ts';
export * from './fonts/substitution.ts';
export * from './marks.ts';
export * from './numbering/formats.ts';
export * from './numbering/labels.ts';
export * from './numbering/numbering.ts';
export * from './schema.ts';
export * from './sections.ts';
export * from './serialize.ts';
export * from './stats.ts';
export * from './styles/merge.ts';
export * from './styles/resolve.ts';
export * from './styles/stylesheet.ts';
export * from './template.ts';
export * from './theme.ts';
export * from './types.ts';
export * from './units.ts';
