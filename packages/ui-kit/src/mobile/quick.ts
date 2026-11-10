/**
 * The bottom toolbar's quick commands: the most used commands of what is selected,
 * as ids of the command registry. Each is drawn from the ribbon's own control for that command
 * (the same menu, colour palette and state), so the toolbar defines no command of its own.
 */
import type { AppId, CommandContext } from '@kiln/commands';
import { itemId, leafItems } from '../ribbon/builders.ts';
import type { RibbonLeafItem, RibbonTab } from '../ribbon/types.ts';

/** What the quick commands act on. */
export type QuickContext = 'text' | 'cell' | 'slide' | 'object' | 'table';

const TEXT = [
  'suite.format.bold',
  'suite.format.italic',
  'suite.format.underline',
  'suite.format.font_color',
  'suite.format.highlight',
  'suite.format.bullets',
  'suite.format.numbering',
  'suite.format.align_left',
  'suite.format.align_center',
  'suite.format.align_right',
  'suite.format.outdent',
  'suite.format.indent',
] as const;

/** The quick commands of each context, in toolbar order. */
export const QUICK_COMMANDS: Readonly<Record<QuickContext, readonly string[]>> = {
  text: TEXT,
  cell: [
    'grid.number.number_format',
    'suite.format.bold',
    'suite.format.italic',
    'grid.font.fill_color',
    'grid.font.borders',
    'grid.alignment.merge_center',
    'grid.alignment.wrap_text',
    'suite.format.align_left',
    'suite.format.align_center',
    'suite.format.align_right',
    'suite.format.font_color',
  ],
  slide: [
    'deck.slides.new_slide',
    'deck.slides.layout',
    'suite.insert.text_box',
    'suite.insert.pictures',
    'suite.insert.shapes',
    'suite.insert.table',
    'suite.insert.icons',
  ],
  object: [
    'deck.drawing.shape_fill',
    'deck.drawing.shape_outline',
    'deck.drawing.quick_styles',
    'deck.arrange',
    'suite.edit.copy',
    'suite.edit.paste',
  ],
  table: [
    'suite.format.bold',
    'suite.format.italic',
    'suite.format.underline',
    'suite.format.font_color',
    'suite.format.align_left',
    'suite.format.align_center',
    'suite.format.align_right',
    'suite.edit.copy',
    'suite.edit.paste',
  ],
};

/** The context of the selection, from what the editors put in the registry's context. */
export function quickContextOf(app: AppId, ctx: CommandContext): QuickContext {
  const scopes = (ctx.scopes ?? []) as readonly string[];
  const contexts = (Array.isArray(ctx.contexts) ? ctx.contexts : []) as readonly string[];
  if (app === 'grid') return scopes.includes('cell-edit') ? 'text' : 'cell';
  if (app === 'deck') {
    if (scopes.includes('text-edit')) return 'text';
    if (scopes.includes('object')) return 'object';
    return 'slide';
  }
  if (contexts.includes('table')) return 'table';
  return 'text';
}

/** The id of a control (a drop-down's command, else its own id). */
function commandOf(item: RibbonLeafItem): string | undefined {
  if ('command' in item && item.command) return item.command;
  return 'id' in item ? item.id : undefined;
}

/**
 * The ribbon controls of the quick commands, in order: the first control of each command in the
 * ribbon's tabs (the Home tab first). A split button becomes its main button (its menu stays in
 * the ribbon sheet); a command the ribbon does not show in this build is left out.
 */
export function quickItems(tabs: readonly RibbonTab[], commands: readonly string[]): RibbonLeafItem[] {
  const found = new Map<string, RibbonLeafItem>();
  const ordered = [...tabs.filter((t) => t.id === 'home'), ...tabs.filter((t) => t.id !== 'home')];
  for (const tab of ordered)
    for (const group of tab.groups)
      leafItems(group).forEach((item, i) => {
        if (item.kind === 'spacer' || item.kind === 'custom' || item.kind === 'gallery') return;
        const id = commandOf(item);
        if (id && !found.has(id))
          found.set(id, { ...item, id: `quick:${itemId(item, i)}` } as RibbonLeafItem);
      });
  const out: RibbonLeafItem[] = [];
  for (const id of commands) {
    const item = found.get(id);
    if (!item) continue;
    out.push(
      item.kind === 'split'
        ? {
            kind: 'button',
            id: item.id,
            command: item.command,
            args: item.args,
            icon: item.icon,
            label: item.label,
          }
        : item,
    );
  }
  return out;
}
