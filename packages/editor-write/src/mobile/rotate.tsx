/**
 * The rotation handle of a selected floating picture, shape or text box: on a touch
 * screen the frame's handles grow to 28 px and this one shows above the top edge; dragging it turns
 * the object about its centre (snapping to every 45°), and the turn is set with Layout ▸ Rotation
 * (`write.arrange.layout`, the Layout dialog's own command) when the finger lifts.
 */

import { Icon } from '@kiln/ui-kit';
import { RotateCw } from 'lucide-react';
import { type PointerEvent, useRef } from 'react';
import './mobile.css';

/** Degrees within this of a multiple of 45° snap to it. */
const SNAP = 4;

/** The rotation (0–360°, clockwise) of a handle dragged to `p` about `center`. */
export function rotationAt(center: { x: number; y: number }, p: { x: number; y: number }): number {
  // The handle sits above the centre at 0°: the angle from straight up, clockwise.
  let deg = (Math.atan2(p.y - center.y, p.x - center.x) * 180) / Math.PI + 90;
  deg = ((deg % 360) + 360) % 360;
  const near = Math.round(deg / 45) * 45;
  if (Math.abs(deg - near) <= SNAP) deg = near % 360;
  return Math.round(deg * 10) / 10;
}

export function RotateHandle({
  onTurn,
  onTurned,
}: {
  /** The turn while the handle is held (`null` when let go). */
  onTurn: (deg: number | null) => void;
  /** The finger lifted at this turn. */
  onTurned: (deg: number) => void;
}) {
  const drag = useRef<{ id: number; center: { x: number; y: number }; deg: number | null } | null>(null);
  const down = (e: PointerEvent<HTMLSpanElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const frame = e.currentTarget.parentElement?.getBoundingClientRect();
    if (!frame) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = {
      id: e.pointerId,
      center: { x: frame.left + frame.width / 2, y: frame.top + frame.height / 2 },
      deg: null,
    };
  };
  const move = (e: PointerEvent<HTMLSpanElement>) => {
    const d = drag.current;
    if (!d || d.id !== e.pointerId) return;
    e.stopPropagation();
    d.deg = rotationAt(d.center, { x: e.clientX, y: e.clientY });
    onTurn(d.deg);
  };
  const up = (e: PointerEvent<HTMLSpanElement>) => {
    const d = drag.current;
    if (!d || d.id !== e.pointerId) return;
    e.stopPropagation();
    drag.current = null;
    onTurn(null);
    if (d.deg !== null) onTurned(d.deg);
  };
  return (
    <span
      className="kiln-object-frame__rotate"
      data-handle="rotate"
      aria-hidden="true"
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
    >
      <Icon icon={RotateCw} size={14} />
    </span>
  );
}
