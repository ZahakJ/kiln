/**
 * Where an app's document surface goes. The editors (WP-64 Write, WP-71 Grid, WP-78 Deck, WP-86
 * Notes) mount `@kiln/surface` here; until then the frame shows the empty canvas the app opens on,
 * and says so to assistive technology rather than pretending to be an editor.
 */
import type { DocSummary } from '@kiln/protocol';
import type { EditorApp } from './AppFrameRoute.tsx';
import './surface.css';

export function SurfaceSlot({
  app,
  summary,
  problem,
  ready,
}: {
  app: EditorApp;
  summary: DocSummary | null;
  problem: string | null;
  ready: boolean;
}) {
  const label = summary ? `${summary.title}: the ${app} editor attaches here` : 'Opening the document';
  return (
    <section
      className={`kiln-slot kiln-slot--${app}`}
      data-app-surface={app}
      data-problem={problem ? true : undefined}
      aria-busy={!problem && (!ready || !summary)}
      aria-label={problem ? `${app} document` : label}
    >
      {app === 'write' || app === 'notes' ? <div className="kiln-slot__page" /> : null}
      {app === 'grid' ? <div className="kiln-slot__sheet" /> : null}
      {app === 'deck' ? <div className="kiln-slot__slide" /> : null}
      {problem ? (
        <div className="kiln-slot__problem" role="status">
          <p>{problem}</p>
          <a href="#/home">Back to Kiln Home</a>
        </div>
      ) : null}
    </section>
  );
}
