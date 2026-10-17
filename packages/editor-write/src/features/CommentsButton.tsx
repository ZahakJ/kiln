/**
 * The tab strip's Comments button for a Write document (mockup write-light.png): the frame's
 * outline button with the number of open comment threads beside it, from the core's
 * `FormatState.features.open_comments` (resolved threads are not counted).
 */
import { StripButton } from '@kiln/ui-kit';
import { useWrite } from '../hooks.ts';
import type { WriteEditorModel } from '../model.ts';

export function WriteCommentsButton({ model }: { model: WriteEditorModel }) {
  const open = useWrite(model, (s) => s.format?.features.open_comments ?? 0);
  return (
    <StripButton
      command="suite.review.show_comments"
      label="Comments"
      badge={open > 0 ? open.toLocaleString() : undefined}
    />
  );
}
