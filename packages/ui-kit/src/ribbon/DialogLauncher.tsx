import {
  useCommandMeta,
  useCommandShortcut,
  useCommandState,
  useExecute,
} from '../chrome/CommandContext.tsx';
import { Tooltip } from '../primitives/Tooltip.tsx';

/** Office's group-corner launcher icon: a corner bracket with a diagonal arrow. */
function LauncherIcon() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 12 12"
      aria-hidden="true"
      focusable="false"
      className="kiln-icon"
    >
      <path d="M1.5 4.5V1.5h3" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
      <path
        d="M4 4l6 6M10 6.5V10H6.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export interface DialogLauncherProps {
  /** Command that opens the dialog (`suite.format.font_dialog`). */
  command: string;
  /** Accessible name / tooltip; default the command title ("Font…"). */
  label?: string;
  /** Group name, used in the default label ("Font Settings"). */
  groupLabel?: string;
  keyTip?: string;
  keyTipLevel?: string;
}

/** The ↘ button in a ribbon group's corner that opens the group's big dialog (Font, Paragraph…). */
export function DialogLauncher({ command, label, groupLabel, keyTip, keyTipLevel }: DialogLauncherProps) {
  const meta = useCommandMeta(command);
  const state = useCommandState(command);
  const shortcut = useCommandShortcut(command);
  const execute = useExecute();
  if (!state.visible) return null;
  const name = label ?? (groupLabel ? `${groupLabel} Settings` : (meta?.title ?? command));
  return (
    <Tooltip content={name} shortcut={shortcut} description={meta?.description}>
      <button
        type="button"
        className="kiln-ribbon__launcher"
        aria-label={name}
        aria-disabled={!state.enabled || undefined}
        data-command={command}
        data-keytip={keyTip}
        data-keytip-level={keyTipLevel}
        onClick={() => state.enabled && execute(command)}
      >
        <LauncherIcon />
      </button>
    </Tooltip>
  );
}
