/**
 * Status bar items of Kiln Write's features (mockup write-light.png): the word-count goal
 * with its ember progress bar, the proofing state (No issues / spelling errors, opening the
 * Spelling pane), Track Changes (click to switch), and Focus on the right. Every figure is the
 * core's (`FormatState.features`, `StatusBar.words`).
 */
import { t } from '@kiln/i18n';
import type { FeatureState } from '@kiln/protocol';
import type { StatusBarItem } from '@kiln/ui-kit';
import type { WriteEditorModel } from '../model.ts';
import { featuresOf } from './model.ts';

/** Percent of the goal reached (0–100). */
export function goalPercent(words: number, goal: number): number {
  if (goal <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((words / goal) * 100)));
}

export function featureStatusItems(
  model: WriteEditorModel,
  fs: FeatureState | null,
  words: number | null,
): { left: StatusBarItem[]; right: StatusBarItem[] } {
  const left: StatusBarItem[] = [];
  const right: StatusBarItem[] = [];
  const features = featuresOf(model);
  if (fs && fs.word_goal > 0 && words !== null) {
    const pct = goalPercent(words, fs.word_goal);
    left.push({
      id: 'goal',
      label: t('Word count goal'),
      title: t('{words, number} of {goal, number} words — set the goal', { words, goal: fs.word_goal }),
      onClick: () => features.openDialog('wordGoal'),
      content: (
        <span className="kiln-goal" data-testid="write-word-goal">
          <span className="kiln-goal__flame" aria-hidden="true" />
          <span
            className="kiln-goal__meter"
            role="progressbar"
            aria-label={t('Word count goal')}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={pct}
          >
            <span className="kiln-goal__fill" style={{ width: `${pct}%` }} />
          </span>
          <span className="kiln-goal__pct">{pct}%</span>
        </span>
      ),
    });
  }
  if (fs) {
    const issues = fs.spelling ? fs.misspellings : 0;
    left.push({
      id: 'proofing',
      label: t('Proofing'),
      title: fs.spelling ? t('Spelling (F7)') : t('Spelling is off'),
      onClick: () => void model.run('suite.review.spelling'),
      content: (
        <span
          className={`kiln-proofing${issues > 0 ? ' has-issues' : ''}`}
          data-testid="write-proofing-status"
        >
          <span className="kiln-proofing__dot" aria-hidden="true" />
          {!fs.spelling
            ? t('Spelling off')
            : issues === 0
              ? t('No issues')
              : t('write.spellingErrors', { count: issues })}
        </span>
      ),
    });
  }
  left.push({
    id: 'track',
    label: t('Track Changes'),
    content: fs?.tracking ? t('Track Changes: On') : t('Track Changes: Off'),
    title: t('Track Changes (Ctrl+Shift+E)'),
    onClick: () => void model.run('write.review.track_changes'),
    defaultHidden: true,
  });
  right.push({
    id: 'focus',
    label: t('Focus'),
    title: fs?.focus ? t('Leave focus mode') : t('Focus mode: hide everything but the page'),
    onClick: () => void model.run('write.view.focus_mode'),
    content: (
      <span className={`kiln-focus-button${fs?.focus ? ' is-on' : ''}`} data-testid="write-focus-button">
        <span className="kiln-focus-button__icon" aria-hidden="true" />
        {t('Focus')}
      </span>
    ),
  });
  return { left, right };
}
