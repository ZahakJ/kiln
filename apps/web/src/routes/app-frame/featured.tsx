/**
 * File ▸ Home's "New" row: a blank document of the window's app and the
 * featured templates, the first one of that app; the full gallery is File ▸ New.
 */
import { APP_KINDS, type HomeApp, type HomeTemplate, TemplateCard } from '@kiln/home';
import { t, tx } from '@kiln/i18n';
import { AppTile } from '@kiln/ui-kit';
import { TEMPLATES } from '../../templates.ts';

export function BackstageFeatured({
  app,
  onCreate,
  onOpenTemplate,
}: {
  app: string;
  onCreate: (app: HomeApp) => void;
  onOpenTemplate: (template: HomeTemplate) => void;
}) {
  const own = app as HomeApp;
  const featured = [
    ...TEMPLATES.filter((tpl) => tpl.app === own && tpl.featured),
    ...TEMPLATES.filter((tpl) => tpl.app !== own && tpl.featured),
  ].slice(0, 4);
  if (featured.length === 0) return null;
  return (
    <section className="kiln-bs-featured" aria-labelledby="kiln-bs-featured">
      <h2 id="kiln-bs-featured" className="kiln-bs-h2">
        {t('New')}
      </h2>
      <ul className="kiln-bs-featured__row">
        <li>
          <button
            type="button"
            className="kiln-tcard kiln-tcard--blank"
            data-blank={own}
            onClick={() => onCreate(own)}
          >
            <span className="kiln-tcard__thumb kiln-tcard__blank" data-app={own} aria-hidden="true">
              <span className="kiln-tcard__sheet" data-app={own}>
                <AppTile app={own} size={30} />
              </span>
            </span>
            <span className="kiln-tcard__name">
              <span>{tx(APP_KINDS[own]?.blank ?? '')}</span>
            </span>
          </button>
        </li>
        {featured.map((tpl) => (
          <li key={tpl.id}>
            <TemplateCard template={tpl} onOpen={onOpenTemplate} compact />
          </li>
        ))}
      </ul>
    </section>
  );
}
