/**
 * Renders the ribbon definitions of `@kiln/commands` (pure data) as ui-kit ribbon
 * specs: gallery tiles get their typographic previews, font lists show each face, and contextual
 * tabs appear while the core reports their context (`ctx.contexts`, e.g. `table`).
 */
import type {
  CommandContext,
  RibbonData,
  RibbonDataGroup,
  RibbonDataLeaf,
  RibbonDataTab,
  RibbonGalleryEntry,
} from '@kiln/commands';
import { tx } from '@kiln/i18n';
import type { CSSProperties } from 'react';
import type {
  RibbonGalleryEntryItem,
  RibbonGroup,
  RibbonItem,
  RibbonLeafItem,
  RibbonSpec,
  RibbonTab,
} from './types.ts';

const FONTS: Record<NonNullable<NonNullable<RibbonGalleryEntry['look']>['font']>, string> = {
  ui: 'var(--kiln-font-ui)',
  display: 'var(--kiln-font-display)',
  serif: 'var(--kiln-font-display)',
  mono: 'var(--kiln-font-mono, ui-monospace, monospace)',
};

/** The preview of a gallery tile, from its data hints. */
export function GallerySample({ entry }: { entry: RibbonGalleryEntry }) {
  const look = entry.look ?? {};
  if (look.image) {
    return (
      <span className="kiln-gallery-picture" aria-hidden="true">
        <img src={look.image} alt="" draggable={false} />
        {/* An empty sample: the picture alone (sparkline and PivotTable styles, as in Excel). */}
        {entry.sample === '' ? null : <span className="kiln-gallery-name">{tx(entry.label)}</span>}
      </span>
    );
  }
  if (look.swatch && look.swatch.length > 0) {
    return (
      <span className="kiln-gallery-picture" aria-hidden="true">
        <span className="kiln-gallery-swatch">
          {look.swatch.map((c, i) => (
            <span key={i} style={{ background: c }} />
          ))}
        </span>
        {entry.sample ? <span className="kiln-gallery-name">{entry.sample}</span> : null}
      </span>
    );
  }
  const style: CSSProperties = {
    fontFamily: look.font ? FONTS[look.font] : undefined,
    fontSize: look.size ? `${look.size}px` : undefined,
    fontWeight: look.weight,
    fontStyle: look.italic ? 'italic' : undefined,
    textDecoration: look.underline ? 'underline' : undefined,
    textTransform: look.caps ? 'uppercase' : undefined,
    letterSpacing: look.caps ? '0.04em' : undefined,
  };
  return (
    <span className="kiln-gallery-tile">
      <span
        className="kiln-gallery-sample"
        data-accent={look.accent || undefined}
        data-muted={look.muted || undefined}
        style={style}
      >
        {entry.sample ?? tx(entry.label)}
      </span>
      {entry.sample ? <span className="kiln-gallery-name">{tx(entry.label)}</span> : null}
    </span>
  );
}

function galleryItems(entries: readonly RibbonGalleryEntry[]): RibbonGalleryEntryItem[] {
  return entries.map((e) => ({
    id: e.id,
    label: e.label,
    preview: <GallerySample entry={e} />,
    ...(e.tier ? { tier: e.tier } : {}),
  }));
}

function leaf(item: RibbonDataLeaf): RibbonLeafItem {
  switch (item.kind) {
    case 'gallery':
      return { ...item, items: galleryItems(item.items) };
    case 'combobox':
      return {
        ...item,
        options: item.options.map((o) => ({
          value: o.value,
          ...(o.label ? { label: o.label } : {}),
          ...(o.group ? { group: o.group } : {}),
          ...(o.hint ? { hint: o.hint } : {}),
          ...(o.fontFamily ? { style: { fontFamily: `"${o.fontFamily}", var(--kiln-font-ui)` } } : {}),
        })),
        ...(item.accepts === 'number'
          ? { parse: (text: string) => (/^\d+(\.\d+)?$/.test(text.trim()) ? text.trim() : null) }
          : {}),
      };
    default:
      return item;
  }
}

function group(g: RibbonDataGroup): RibbonGroup {
  const items: RibbonItem[] = g.items.map((it) =>
    it.kind === 'row'
      ? { kind: 'row', ...(it.id ? { id: it.id } : {}), items: it.items.map(leaf) }
      : leaf(it),
  );
  return { ...g, items };
}

/** Contexts the core reports for the selection (`table`, `picture`…), read by contextual tabs. */
export function activeContexts(ctx: CommandContext): readonly string[] {
  const c = ctx.contexts;
  return Array.isArray(c) ? (c as string[]) : [];
}

function tab(t: RibbonDataTab): RibbonTab {
  const { contextual, groups, ...rest } = t;
  const out: RibbonTab = { ...rest, groups: groups.map(group) };
  if (contextual) {
    out.contextual = { set: contextual.set, label: contextual.label };
    const name = contextual.context;
    out.when = (ctx) => activeContexts(ctx).includes(name);
  }
  return out;
}

/** A renderable ribbon spec from an app's ribbon data. */
export function ribbonFromData(data: RibbonData, extraTabs: readonly RibbonTab[] = []): RibbonSpec {
  return { app: data.app, tabs: [...data.tabs.map(tab), ...extraTabs] };
}
