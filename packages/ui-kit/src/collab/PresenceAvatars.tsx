/**
 * The people in a shared document, in the title bar: an avatar per person in their colour (the
 * colour of their caret and selection), then this person's own; the connection's state when it
 * is not live. Selecting someone goes to where they are in the document.
 */
import { msg, t, tx } from '@kiln/i18n';
import type { CollabPeer, CollabStatus } from '@kiln/protocol';
import { CloudOff, LoaderCircle } from 'lucide-react';
import { Icon } from '../icons/Icon.tsx';
import { Tooltip } from '../primitives/Tooltip.tsx';
import './collab.css';

export interface PresenceAvatarsProps {
  /** The others. */
  peers: readonly CollabPeer[];
  /** This person's initials and colour (`RRGGBB`). */
  me: { name: string; initials: string; colour: string };
  status: CollabStatus;
  /** Goes to where someone is. */
  onGoTo?: (peer: CollabPeer) => void;
  /** Opens the Share dialog (this person's avatar). */
  onOpenShare?: () => void;
  /** How many avatars show before "+n". */
  max?: number;
}

/** A `RRGGBB` colour as CSS. */
export function peerColour(hex: string): string {
  return /^[0-9a-f]{6}$/i.test(hex) ? `#${hex}` : 'var(--app)';
}

/** Text on a `RRGGBB` colour: white on dark colours, near-black on light ones (4.5:1 either way). */
export function peerInk(hex: string): string {
  if (!/^[0-9a-f]{6}$/i.test(hex)) return '#fff';
  const channel = (i: number) => {
    const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const luminance = 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
  // White wins while it contrasts more than black does.
  return 1.05 / (luminance + 0.05) >= (luminance + 0.05) / 0.05 ? '#fff' : '#1b1712';
}

/** The CSS variables of a person's colour and the text on it. */
export function peerStyle(hex: string): React.CSSProperties {
  return { '--peer': peerColour(hex), '--peer-ink': peerInk(hex) } as React.CSSProperties;
}

const STATUS: Partial<Record<CollabStatus, { label: string; icon: typeof CloudOff; spin?: boolean }>> = {
  Connecting: { label: msg('Connecting…'), icon: LoaderCircle, spin: true },
  Offline: {
    label: msg('Offline: your changes are kept and sent when the connection is back'),
    icon: CloudOff,
  },
  Refused: { label: msg('The server refused this document'), icon: CloudOff },
};

export function PresenceAvatars({ peers, me, status, onGoTo, onOpenShare, max = 3 }: PresenceAvatarsProps) {
  const shown = peers.slice(0, max);
  const more = peers.length - shown.length;
  const state = STATUS[status];
  return (
    // biome-ignore lint/a11y/useSemanticElements: a group of avatar buttons, not a form fieldset.
    <div
      className="kiln-presence"
      role="group"
      aria-label={t('People in this document')}
      data-testid="presence"
    >
      {state ? (
        <Tooltip content={tx(state.label)}>
          <span
            className={`kiln-presence__status kiln-presence__status--${status.toLowerCase()}`}
            role="img"
            aria-label={tx(state.label)}
          >
            <Icon icon={state.icon} size={14} className={state.spin ? 'kiln-presence__spin' : undefined} />
          </span>
        </Tooltip>
      ) : null}
      <div className="kiln-presence__stack">
        {shown.map((p) => (
          <Tooltip key={p.peer} content={p.name} description={t('Editing. Select to go to where they are.')}>
            <button
              type="button"
              className="kiln-presence__avatar"
              style={peerStyle(p.colour)}
              // The visible initials begin the name (WCAG 2.5.3, label in name).
              aria-label={t('{value}: {name}, editing', { value: p.initials || '?', name: p.name })}
              data-peer={p.peer}
              onClick={() => onGoTo?.(p)}
            >
              {p.initials || '?'}
            </button>
          </Tooltip>
        ))}
        {more > 0 ? (
          <Tooltip
            content={peers
              .slice(max)
              .map((p) => p.name)
              .join(', ')}
          >
            <span
              className="kiln-presence__avatar kiln-presence__more"
              role="img"
              aria-label={t('{more} more', { more })}
            >
              +{more}
            </span>
          </Tooltip>
        ) : null}
      </div>
      <Tooltip
        content={t('{name} (you)', { name: me.name || t('You') })}
        description={t('Share settings and the link.')}
      >
        <button
          type="button"
          className="kiln-presence__avatar kiln-presence__me"
          style={peerStyle(me.colour)}
          aria-label={t('{initials}: {name} (you), share settings', {
            initials: me.initials || t('ME'),
            name: me.name || t('You'),
          })}
          onClick={onOpenShare}
        >
          {me.initials || t('ME')}
        </button>
      </Tooltip>
    </div>
  );
}
