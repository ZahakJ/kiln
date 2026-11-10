/**
 * Every stylesheet of the interface, in the order of the single stylesheet the build made before
 * the editors and the slide show were split into chunks loaded on demand (budget B12).
 * Imported once, before the app, so the cascade is the same on every screen, in the dev server
 * and in the build, whichever chunk a rule's component is in. A stylesheet a new component adds
 * still loads with its own chunk; list it here to give it its place in the cascade.
 */
import '../../../packages/ui-kit/src/a11y/a11y.css';
import '../../../packages/ui-kit/src/backstage/backstage.css';
import '../../../packages/ui-kit/src/backstage/print/print.css';
import '../../../packages/ui-kit/src/a11y/forced-colors.css';
import '../../../packages/i18n/src/fonts.css';
import '../../../packages/ui-kit/src/keytips/keytips.css';
import '../../../packages/ui-kit/src/mobile/mobile.css';
import '../../../packages/ui-kit/src/ribbon/ribbon.css';
import '../../../packages/ui-kit/src/palette/palette.css';
import '../../../packages/ui-kit/src/chrome/options.css';
import '../../../packages/ui-kit/src/chrome/appframe.css';
import '../../../packages/ui-kit/src/collab/collab.css';
import '../../../packages/ui-kit/src/compat/compat.css';
import '../../../packages/ui-kit/src/minitoolbar/minitoolbar.css';
import '../../../packages/ui-kit/src/contextmenu/contextmenu.css';
import '../../../packages/ui-kit/src/panes/panes.css';
import '../../../packages/ui-kit/src/primitives/primitives.css';
import '../../../packages/ui-kit/src/recovery/recovery.css';
import '../../../packages/ui-kit/src/statusbar/statusbar.css';
import '../../../packages/ui-kit/src/tour/tour.css';
import '../../../packages/home/src/templates/gallery.css';
import '../../../packages/home/src/home.css';
import '../../../packages/home/src/mobile/mobile.css';
import '../../../packages/editor-deck/src/mobile/mobile.css';
import '../../../packages/editor-deck/src/show/show.css';
import '../../../packages/editor-deck/src/views/views.css';
import '../../../packages/editor-deck/src/review/review.css';
import '../../../packages/editor-deck/src/deck.css';
import '../../../packages/editor-deck/src/features/features.css';
import '../../../packages/editor-deck/src/presenter/presenter.css';
import '../../../packages/editor-grid/src/home/home.css';
import '../../../packages/editor-grid/src/analysis/analysis.css';
import '../../../packages/editor-grid/src/features/features.css';
import '../../../packages/editor-grid/src/mobile/mobile.css';
import '../../../packages/editor-grid/src/grid.css';
import '../../../packages/editor-write/src/mobile/mobile.css';
import '../../../packages/editor-write/src/features/features.css';
import '../../../packages/editor-write/src/write.css';
import './routes/app-frame/surface.css';
import './routes/present/present.css';
