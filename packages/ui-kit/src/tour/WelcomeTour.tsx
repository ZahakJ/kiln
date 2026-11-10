/**
 * The welcome tour: three cards that show a newcomer what Kiln keeps from Office and where
 * to start. It is docked at the bottom of Kiln Home and does not block it (a non-modal dialog), is
 * shown once (the host remembers it through `onDone`), can be skipped at any card, and is reached
 * first with Tab. Left and Right (mirrored in a right-to-left interface) step through the cards;
 * Escape skips. Each card's change is announced.
 */
import { msg, readingStep, t, tx } from '@kiln/i18n';
import { ArrowLeft, ArrowRight, X } from 'lucide-react';
import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef, useState } from 'react';
import { AppTile, Flame, Spark } from '../icons/brand/BrandMark.tsx';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { cx } from '../primitives/cx.ts';
import { Kbd } from '../primitives/Kbd.tsx';
import './tour.css';

interface Card {
  eyebrow: string;
  title: string;
  body: string;
  art: () => ReactNode;
}

const CARDS: readonly Card[] = [
  {
    eyebrow: msg('Welcome to Kiln'),
    title: msg('Your Office files, as they are'),
    body: msg(
      'Open a .docx, .xlsx or .pptx and Kiln keeps everything in it, even the parts it does not edit. Save back to the same file whenever you like.',
    ),
    art: () => (
      <div className="kiln-tour__art kiln-tour__art--files" aria-hidden="true">
        {/* i18n-ignore: file extensions */}
        <span className="kiln-tour__file" data-app="write" dir="ltr">
          .docx
        </span>
        {/* i18n-ignore: file extensions */}
        <span className="kiln-tour__file" data-app="grid" dir="ltr">
          .xlsx
        </span>
        {/* i18n-ignore: file extensions */}
        <span className="kiln-tour__file" data-app="deck" dir="ltr">
          .pptx
        </span>
        <Icon icon={ArrowRight} size={22} className="kiln-tour__arrow" />
        <AppTile app="home" size={64} />
      </div>
    ),
  },
  {
    eyebrow: msg('Feels like home'),
    title: msg('The ribbon you already know'),
    body: msg(
      'The tabs, the File menu and the Office shortcuts are where you left them. Press Alt for Key Tips, or search any command by its Office name.',
    ),
    art: () => (
      <div className="kiln-tour__art kiln-tour__art--ribbon" aria-hidden="true">
        <div className="kiln-tour__ribbon">
          <span className="is-on">
            {t('Home')}
            <b>{/* i18n-ignore: a Key Tip */}H</b>
          </span>
          <span>
            {t('Insert')}
            <b>{/* i18n-ignore: a Key Tip */}N</b>
          </span>
          <span>
            {t('Layout')}
            <b>{/* i18n-ignore: a Key Tip */}P</b>
          </span>
        </div>
        <div className="kiln-tour__palette">
          <Spark size={13} />
          <span>{t('cmd.grid.alignment.merge_center')}</span>
          <Kbd shortcut="Mod+Shift+P" />
        </div>
      </div>
    ),
  },
  {
    eyebrow: msg('Ready when you are'),
    title: msg('Start from something finished'),
    body: msg(
      'Templates for letters, budgets, pitches and more open as new documents. Your files stay on this device unless you choose to share them.',
    ),
    art: () => (
      <div className="kiln-tour__art kiln-tour__art--templates" aria-hidden="true">
        {(['write', 'grid', 'deck'] as const).map((app) => (
          <span key={app} className="kiln-tour__card" data-app={app}>
            <i />
            <i />
            <i />
            <AppTile app={app} size={20} />
          </span>
        ))}
        <span className="kiln-tour__local">
          <Flame size={12} />
          {t('Local first')}
        </span>
      </div>
    ),
  },
];

export interface WelcomeTourProps {
  open: boolean;
  /** Called when the tour is finished or skipped; the host remembers it was seen. */
  onDone: (how: 'finished' | 'skipped') => void;
  /** The last card's action (Kiln Home opens its templates). */
  onFinishAction?: () => void;
  /** The last card's button text. */
  finishLabel?: string;
  className?: string;
}

/** The three-card welcome tour, docked at the bottom of the window. */
export function WelcomeTour({ open, onDone, onFinishAction, finishLabel, className }: WelcomeTourProps) {
  const [step, setStep] = useState(0);
  const id = useId();
  const nextRef = useRef<HTMLButtonElement>(null);
  const moved = useRef(false);
  const card = CARDS[step] ?? CARDS[0];
  const last = step === CARDS.length - 1;

  // After a step changes by a button, the next button keeps the focus (it moved under the pointer).
  useEffect(() => {
    if (moved.current) nextRef.current?.focus({ preventScroll: true });
    moved.current = false;
  }, [step]);

  if (!open || !card) return null;
  const go = (n: number) => {
    moved.current = true;
    setStep(Math.max(0, Math.min(CARDS.length - 1, n)));
  };
  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onDone('skipped');
      return;
    }
    const s = readingStep(e.key, e.currentTarget);
    if (!s || (e.target as HTMLElement).matches('input, textarea')) return;
    e.preventDefault();
    go(step + s);
  };

  return (
    <section
      role="dialog"
      aria-modal="false"
      aria-labelledby={`${id}-title`}
      aria-describedby={`${id}-body`}
      className={cx('kiln-tour', className)}
      data-step={step + 1}
      onKeyDown={onKeyDown}
    >
      <div className="kiln-tour__visual">
        {card.art()}
        <button
          type="button"
          className="kiln-tour__close"
          aria-label={t('Skip the tour')}
          onClick={() => onDone('skipped')}
        >
          <Icon icon={X} size={16} />
        </button>
      </div>
      <div className="kiln-tour__text">
        <p className="kiln-tour__eyebrow">
          <Spark size={12} />
          <span>{tx(card.eyebrow)}</span>
          <span className="kiln-tour__count" aria-live="polite">
            {t('tour.step', { step: step + 1, total: CARDS.length })}
          </span>
        </p>
        <h2 id={`${id}-title`} className="kiln-tour__title">
          {tx(card.title)}
        </h2>
        <p id={`${id}-body`} className="kiln-tour__body">
          {tx(card.body)}
        </p>
      </div>
      <div className="kiln-tour__foot">
        <span className="kiln-tour__dots" aria-hidden="true">
          {CARDS.map((c, i) => (
            <i key={c.title} className={cx(i === step && 'is-on')} />
          ))}
        </span>
        <Button variant="ghost" size="sm" onClick={() => onDone('skipped')}>
          {t('Skip tour')}
        </Button>
        {step > 0 ? (
          <Button size="sm" icon={<Icon icon={ArrowLeft} size={14} />} onClick={() => go(step - 1)}>
            {t('Back')}
          </Button>
        ) : null}
        <Button
          ref={nextRef}
          variant="ember"
          size="sm"
          iconAfter={last ? undefined : <Icon icon={ArrowRight} size={14} />}
          onClick={() => {
            if (!last) go(step + 1);
            else {
              onDone('finished');
              onFinishAction?.();
            }
          }}
        >
          {last ? (finishLabel ?? t('Start making')) : t('Next')}
        </Button>
      </div>
    </section>
  );
}
