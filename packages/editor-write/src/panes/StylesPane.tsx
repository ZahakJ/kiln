/**
 * The Styles pane (Home ▸ Styles ↘): every recommended style with a preview in its own
 * formatting, the current one marked; click to apply, and per style Update to Match Selection
 * and Modify…; New Style from the selection's formatting. The list and previews come from the
 * core (`Query::Write(Styles)`).
 */
import type { StyleInfo } from '@kiln/protocol';
import { Button, Icon, Menu, MenuContent, MenuItem, MenuTrigger } from '@kiln/ui-kit';
import { MoreHorizontal, Plus } from 'lucide-react';
import { useState } from 'react';
import { useWrite } from '../hooks.ts';
import type { WriteEditorModel } from '../model.ts';

const KIND: Record<StyleInfo['kind'], string> = {
  Paragraph: '¶',
  Character: 'a',
  Linked: '¶a',
  Table: '▦',
  Numbering: '≡',
};

export function StylesPane({ model }: { model: WriteEditorModel }) {
  const styles = useWrite(model, (s) => s.styles);
  const [preview, setPreview] = useState(true);
  if (!styles) return <p className="kiln-write-styles__empty">Reading the styles…</p>;
  return (
    <div className="kiln-write-styles" data-testid="write-styles-pane">
      <ul className="kiln-write-styles__list" aria-label="Styles">
        {styles.map((s) => (
          <li key={s.id} className="kiln-write-styles__item" data-current={s.current || undefined}>
            <button
              type="button"
              className="kiln-write-styles__apply"
              aria-pressed={s.current}
              title={s.based_on ? `${s.name} (based on ${s.based_on})` : s.name}
              onClick={() => void model.run('write.styles.apply', { style_id: s.id })}
            >
              <span
                className="kiln-write-styles__name"
                style={
                  preview
                    ? {
                        fontFamily: `"${s.font}", var(--kiln-font-doc)`,
                        fontWeight: s.bold ? 700 : 400,
                        fontStyle: s.italic ? 'italic' : 'normal',
                        // The style's colour, mixed with the pane's text colour (write.css) so
                        // a light or dark document colour stays readable in both themes.
                        ...(s.color ? { '--style-color': `#${s.color}` } : {}),
                        // Never below the pane's body size: small styles (Heading 5–9, Quote,
                        // emphasis) must stay legible in their own fonts.
                        fontSize: `${Math.min(20, Math.max(13, s.size * 1.15))}px`,
                      }
                    : undefined
                }
              >
                {s.name}
              </span>
              <span className="kiln-write-styles__kind" aria-hidden="true">
                {KIND[s.kind]}
              </span>
            </button>
            <Menu modal={false}>
              <MenuTrigger asChild>
                <button type="button" className="kiln-write-styles__more" aria-label={`${s.name} options`}>
                  <Icon icon={MoreHorizontal} size={14} />
                </button>
              </MenuTrigger>
              <MenuContent align="end">
                <MenuItem onSelect={() => void model.run('write.styles.apply', { style_id: s.id })}>
                  Apply {s.name}
                </MenuItem>
                <MenuItem
                  onSelect={() => void model.run('write.styles.update_from_selection', { style_id: s.id })}
                >
                  Update {s.name} to Match Selection
                </MenuItem>
                <MenuItem onSelect={() => model.openDialog('modifyStyle', undefined, s.id)}>Modify…</MenuItem>
              </MenuContent>
            </Menu>
          </li>
        ))}
      </ul>
      <div className="kiln-write-styles__footer">
        <label className="kiln-write-styles__toggle">
          <input type="checkbox" checked={preview} onChange={(e) => setPreview(e.target.checked)} />
          Show preview
        </label>
        <Button size="sm" icon={<Icon icon={Plus} size={14} />} onClick={() => model.openDialog('newStyle')}>
          New Style
        </Button>
      </div>
    </div>
  );
}
