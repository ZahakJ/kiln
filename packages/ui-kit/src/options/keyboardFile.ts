import type { KeymapOverrides } from '@kiln/commands';

/** The kind written into an exported keyboard file. */
export const KEYBOARD_FILE_KIND = 'kiln-keyboard';

/** The name an exported keyboard file is offered under. */
export const KEYBOARD_FILE_NAME = 'kiln-shortcuts.json';

/** Options ▸ Keyboard ▸ Export: the user's remapping as a small JSON file. */
export function keyboardFile(overrides: KeymapOverrides): string {
  return `${JSON.stringify({ kind: KEYBOARD_FILE_KIND, version: 1, overrides }, null, 2)}\n`;
}

/**
 * Options ▸ Keyboard ▸ Import: the remapping in a file exported by Kiln (or the bare overrides
 * object Kiln stores), or `null` when the text is not one. Shortcuts are checked by the keymap
 * when it takes them; this only checks the shape.
 */
export function parseKeyboardFile(text: string): KeymapOverrides | null {
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    return null;
  }
  if (!data || typeof data !== 'object') return null;
  const wrapped = data as { kind?: unknown; overrides?: unknown };
  const inner = wrapped.kind === KEYBOARD_FILE_KIND ? wrapped.overrides : data;
  if (!inner || typeof inner !== 'object') return null;
  const o = inner as { version?: unknown; layers?: unknown };
  if (o.version !== 1 || !o.layers || typeof o.layers !== 'object' || Array.isArray(o.layers)) return null;
  for (const map of Object.values(o.layers as Record<string, unknown>)) {
    if (!map || typeof map !== 'object' || Array.isArray(map)) return null;
    for (const keys of Object.values(map as Record<string, unknown>)) {
      if (!Array.isArray(keys) || !keys.every((k) => typeof k === 'string')) return null;
    }
  }
  return inner as KeymapOverrides;
}

/** How many commands a remapping changes. */
export function changedCommands(overrides: KeymapOverrides): number {
  const ids = new Set<string>();
  for (const map of Object.values(overrides.layers)) for (const id of Object.keys(map ?? {})) ids.add(id);
  return ids.size;
}
