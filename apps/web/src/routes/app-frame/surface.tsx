/**
 * Where an app's document surface goes. The editors (Write, Grid, Deck,
 * Notes) mount `@kiln/surface` here; until then the frame shows the empty canvas the app opens on,
 * and says so to assistive technology rather than pretending to be an editor.
 */
import { t } from '@kiln/i18n';
import type { DocSummary } from '@kiln/protocol';
import { APP_NAMES } from '@kiln/ui-kit';
import { FailureCard, type LoadFailure, ProblemCard } from '../../LoadProblem.tsx';
import type { EditorApp } from './AppFrameRoute.tsx';
import './surface.css';

/** Why the window has no document: its core did not load, or this document did not open. */
export type FrameProblem = LoadFailure | { title: string; text: string };

export function SurfaceSlot({
  app,
  summary,
  problem,
  ready,
}: {
  app: EditorApp;
  summary: DocSummary | null;
  problem: FrameProblem | null;
  ready: boolean;
}) {
  const label = summary
    ? t('{title}: the {app} editor attaches here', { title: summary.title, app: APP_NAMES[app] })
    : t('Opening the document');
  return (
    <section
      className={`kiln-slot kiln-slot--${app}`}
      data-app-surface={app}
      data-problem={problem ? true : undefined}
      aria-busy={!problem && (!ready || !summary)}
      aria-label={problem ? t('{app} document', { app: APP_NAMES[app] }) : label}
    >
      {app === 'write' || app === 'notes' ? <div className="kiln-slot__page" /> : null}
      {app === 'grid' ? <div className="kiln-slot__sheet" /> : null}
      {app === 'deck' ? <div className="kiln-slot__slide" /> : null}
      {problem ? (
        <div className="kiln-slot__problem">
          {typeof problem === 'string' ? (
            <FailureCard app={app} failure={problem} />
          ) : (
            <ProblemCard
              app={app}
              failure="document"
              title={problem.title}
              text={problem.text}
              reload={false}
            />
          )}
        </div>
      ) : null}
    </section>
  );
}
