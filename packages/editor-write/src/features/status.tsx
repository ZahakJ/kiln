/**
 * Status bar items of Kiln Write's features (WP-66, mockup write-light.png): the word-count goal
 * with its ember progress bar, the proofing state (No issues / spelling errors, opening the
 * Spelling pane), Track Changes (click to switch), and Focus on the right. Every figure is the
 * core's (`FormatState.features`, `StatusBar.words`).
 */
import type { FeatureState } from '@kiln/protocol';
import type { StatusBarItem } from '@kiln/ui-kit';
import type { WriteEditorModel } from '../model.ts';
import { featuresOf } from './model.ts';

const fmt = (n: number) => n.toLocaleString();

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
      label: 'Word count goal',
      title: `${fmt(words)} of ${fmt(fs.word_goal)} words — set the goal`,
      onClick: () => features.openDialog('wordGoal'),
      content: (
        <span className="kiln-goal" data-testid="write-word-goal">
          <span className="kiln-goal__flame" aria-hidden="true" />
          <span
            className="kiln-goal__meter"
            role="progressbar"
            aria-label="Word count goal"
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
      label: 'Proofing',
      title: fs.spelling ? 'Spelling (F7)' : 'Spelling is off',
      onClick: () => void model.run('suite.review.spelling'),
      content: (
        <span
          className={`kiln-proofing${issues > 0 ? ' has-issues' : ''}`}
          data-testid="write-proofing-status"
        >
          <span className="kiln-proofing__dot" aria-hidden="true" />
          {!fs.spelling
            ? 'Spelling off'
            : issues === 0
              ? 'No issues'
              : `${fmt(issues)} spelling ${issues === 1 ? 'error' : 'errors'}`}
        </span>
      ),
    });
  }
  left.push({
    id: 'track',
    label: 'Track Changes',
    content: `Track Changes: ${fs?.tracking ? 'On' : 'Off'}`,
    title: 'Track Changes (Ctrl+Shift+E)',
    onClick: () => void model.run('write.review.track_changes'),
    defaultHidden: true,
  });
  right.push({
    id: 'focus',
    label: 'Focus',
    title: fs?.focus ? 'Leave focus mode' : 'Focus mode: hide everything but the page',
    onClick: () => void model.run('write.view.focus_mode'),
    content: (
      <span className={`kiln-focus-button${fs?.focus ? ' is-on' : ''}`} data-testid="write-focus-button">
        <span className="kiln-focus-button__icon" aria-hidden="true" />
        Focus
      </span>
    ),
  });
  return { left, right };
}
