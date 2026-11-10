/**
 * The chrome side of the Transitions, Animations and Slide Show tabs: Preview (a show of
 * one slide on the stage), the Animation Pane and the Animation Painter toggles, Advance After's
 * check box, From Beginning / From Current Slide / Present in Browser Tab / Rehearse Timings /
 * the Audience buttons (the window shows the core's show), Use Presenter View and Set Up Slide
 * Show (the core's settings), and the stage's effect numbers while the Animations tab or the pane
 * is shown. Every change to the presentation is a core command.
 */

import type { CommandDefinition, Disposable } from '@kiln/commands';
import { msg } from '@kiln/i18n';
import type { DeckEditorModel, PresentRequest } from '../model.ts';

/** The ids the Deck chrome implements for the motion and show tabs. */
export const DECK_MOTION_CHROME: readonly string[] = [
  'deck.transitions.preview',
  'deck.animations.preview',
  'deck.animations.animation_pane',
  'deck.animations.animation_painter',
  'ui.deck.advance_after',
  'deck.show.from_beginning',
  'deck.show.from_current',
  'deck.show.in_tab',
  'deck.show.set_up',
  'deck.show.rehearse_timings',
  'deck.show.presenter_view',
  'deck.show.audience_laser',
  'deck.show.audience_pen',
  'deck.show.audience_highlighter',
  'deck.show.audience_blackout',
];

/** Starts a show: through the window's route, or in the core only. */
export async function startShow(
  model: DeckEditorModel,
  req: Omit<PresentRequest, 'presenter'> & { presenter?: boolean },
) {
  // The settings as the core has them now (Set Up Show may have just changed them).
  const s = (await model.fetch()) ?? model.state.deck;
  const settings = s?.motion.settings;
  const full: PresentRequest = {
    ...req,
    presenter: req.presenter ?? settings?.presenter_view ?? false,
    tab: req.tab || settings?.kind === 'Window',
  };
  if (model.present) {
    model.present(full);
    return;
  }
  await model.core('deck.show.start', { from: full.from, mode: full.mode });
}

/** Registers the motion and show chrome commands; dispose to remove them. */
export function registerMotionUi(model: DeckEditorModel): Disposable {
  const st = () => model.state.deck;
  const motion = () => st()?.motion;
  const current = () => st()?.current ?? 0;
  const badges = () => {
    const on = model.state.animPane || model.state.tab === 'animations';
    if ((motion()?.badges ?? false) !== on) void model.core('deck.animations.badges', { on });
  };
  const defs: CommandDefinition<unknown>[] = [
    {
      id: 'deck.transitions.preview',
      title: msg('Preview'),
      run: () => model.set({ preview: 'transition' }),
    },
    {
      id: 'deck.animations.preview',
      title: msg('Preview Animations'),
      isEnabled: () => (motion()?.animations.length ?? 0) > 0,
      run: () => model.set({ preview: 'animations' }),
    },
    {
      id: 'deck.animations.animation_pane',
      title: msg('Animation Pane'),
      isChecked: () => model.state.animPane,
      run: () => {
        model.set({ animPane: !model.state.animPane });
        badges();
      },
    },
    {
      // Animation Painter: picks up the selected shape's effects; the next click on a shape gives
      // them to it.
      id: 'deck.animations.animation_painter',
      title: msg('Animation Painter'),
      isChecked: () => model.state.animPainter,
      run: async () => {
        if (model.state.animPainter) {
          model.set({ animPainter: false });
          return;
        }
        if (await model.core('deck.animations.copy')) model.set({ animPainter: true });
        model.stage?.focus();
      },
    },
    {
      id: 'ui.deck.advance_after',
      title: msg('Advance After'),
      isChecked: () => {
        const t = motion()?.transition;
        return t ? t.after_ms !== null : false;
      },
      run: () => {
        const t = motion()?.transition;
        return model.core('deck.transitions.after', t?.after_ms === null || !t ? { ms: 0 } : { ms: null });
      },
    },
    {
      id: 'deck.show.from_beginning',
      title: msg('From Beginning'),
      run: () => startShow(model, { from: null, mode: 'Show', tab: false }),
    },
    {
      id: 'deck.show.from_current',
      title: msg('From Current Slide'),
      run: () => startShow(model, { from: current(), mode: 'Show', tab: false }),
    },
    {
      id: 'deck.show.in_tab',
      title: msg('Present in Browser Tab'),
      run: () => startShow(model, { from: current(), mode: 'Show', tab: true, presenter: false }),
    },
    {
      id: 'deck.show.rehearse_timings',
      title: msg('Rehearse Timings'),
      run: () => startShow(model, { from: null, mode: 'Rehearse', tab: false, presenter: false }),
    },
    {
      id: 'deck.show.set_up',
      title: msg('Set Up Slide Show'),
      run: () => model.openDialog('setUpShow'),
    },
    {
      id: 'deck.show.presenter_view',
      title: msg('Use Presenter View'),
      isChecked: () => motion()?.settings.presenter_view ?? false,
      run: () => {
        const s = motion()?.settings;
        if (!s) return undefined;
        return model.core('deck.show.settings', { settings: { ...s, presenter_view: !s.presenter_view } });
      },
    },
    ...(
      [
        ['deck.show.audience_laser', msg('Laser Pointer'), 'laser'],
        ['deck.show.audience_pen', msg('Pen'), 'pen'],
        ['deck.show.audience_highlighter', msg('Highlighter'), 'highlighter'],
        ['deck.show.audience_blackout', msg('Blackout'), 'black'],
      ] as const
    ).map(
      ([id, title, tool]): CommandDefinition<unknown> => ({
        id,
        title,
        run: () => startShow(model, { from: current(), mode: 'Show', tab: false, tool }),
      }),
    ),
  ];
  const reg = model.registry;
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const state = reg.getState(d.id);
    if (state.exists && !state.pending) continue;
    disposables.push(reg.register(d));
  }
  // The Animations tab numbers the effects on the stage.
  const onTab = (e: Event) => {
    const tab = (e as CustomEvent<{ tab?: string }>).detail?.tab ?? '';
    if (tab === model.state.tab) return;
    model.set({ tab });
    badges();
  };
  if (typeof window !== 'undefined') window.addEventListener('kiln:ribbon-tab', onTab);
  return {
    dispose() {
      if (typeof window !== 'undefined') window.removeEventListener('kiln:ribbon-tab', onTab);
      for (const d of disposables) d.dispose();
    },
  };
}
