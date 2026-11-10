/**
 * Double tap to zoom: the first double tap fits the content's width to the screen;
 * a double tap at that zoom looks closer (twice the width's zoom, about the finger), and the next
 * one goes back to the width, as on every phone.
 */
export function doubleTapZoom(current: number, fitWidth: number, max = 4): number {
  const fit = Math.max(0.1, fitWidth);
  if (Math.abs(current - fit) > fit * 0.03) return fit;
  return Math.min(max, Math.round(fit * 2 * 100) / 100);
}
