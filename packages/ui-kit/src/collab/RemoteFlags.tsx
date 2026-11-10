/**
 * The others' names over the document, at their carets (the core draws the carets and
 * selections themselves in the overlay). Laid over the document area; flags outside it are
 * clipped.
 */
import type { CollabPeer } from '@kiln/protocol';
import { useLayoutEffect, useRef, useState } from 'react';
import { peerStyle } from './PresenceAvatars.tsx';
import './collab.css';

/** A person's caret on the screen (client coordinates). */
export interface PlacedPeer {
  peer: CollabPeer;
  rect: { left: number; top: number; width: number; height: number };
}

export interface RemoteFlagsProps {
  placed: readonly PlacedPeer[];
}

export function RemoteFlags({ placed }: RemoteFlagsProps) {
  const root = useRef<HTMLDivElement>(null);
  const [origin, setOrigin] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    const r = root.current?.getBoundingClientRect();
    if (r && (r.left !== origin?.left || r.top !== origin?.top)) setOrigin({ left: r.left, top: r.top });
  });
  return (
    <div ref={root} className="kiln-remote-flags" aria-hidden="true" data-testid="remote-flags">
      {origin
        ? placed.map(({ peer, rect }) => (
            <span
              key={peer.peer}
              className="kiln-remote-flag"
              data-peer={peer.peer}
              style={
                {
                  ...peerStyle(peer.colour),
                  left: `${Math.round(rect.left - origin.left)}px`,
                  top: `${Math.round(rect.top - origin.top)}px`,
                } as React.CSSProperties
              }
            >
              {peer.name}
            </span>
          ))
        : null}
    </div>
  );
}
