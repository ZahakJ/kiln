import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import '../chrome/test-matchers.ts';
import { WelcomeTour } from './WelcomeTour.tsx';

describe('WelcomeTour', () => {
  it('walks through three cards and finishes with its action', async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    const onFinishAction = vi.fn();
    render(
      <WelcomeTour open onDone={onDone} onFinishAction={onFinishAction} finishLabel="Browse templates" />,
    );
    const tour = screen.getByRole('dialog', { name: 'Your Office files, as they are' });
    expect(tour).toHaveAttribute('aria-modal', 'false');
    expect(tour).toHaveTextContent('1 of 3');
    expect(screen.queryByRole('button', { name: 'Back' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(screen.getByRole('dialog', { name: 'The ribbon you already know' })).toHaveTextContent('2 of 3');
    expect(screen.getByRole('button', { name: 'Next' })).toHaveFocus();
    await user.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.getByRole('dialog')).toHaveTextContent('1 of 3');
    await user.click(screen.getByRole('button', { name: 'Next' }));
    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(screen.getByRole('dialog', { name: 'Start from something finished' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Browse templates' }));
    expect(onDone).toHaveBeenCalledWith('finished');
    expect(onFinishAction).toHaveBeenCalled();
  });

  it('can be skipped from any card, with the button, the close button or Escape', async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    const { rerender } = render(<WelcomeTour open onDone={onDone} />);
    await user.click(screen.getByRole('button', { name: 'Skip tour' }));
    expect(onDone).toHaveBeenLastCalledWith('skipped');
    await user.click(screen.getByRole('button', { name: 'Skip the tour' }));
    expect(onDone).toHaveBeenCalledTimes(2);
    fireEvent.keyDown(screen.getByRole('button', { name: 'Next' }), { key: 'Escape' });
    expect(onDone).toHaveBeenCalledTimes(3);
    rerender(<WelcomeTour open={false} onDone={onDone} />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('steps with the arrow keys, mirrored in a right-to-left interface', () => {
    render(
      <div dir="rtl" style={{ direction: 'rtl' }}>
        <WelcomeTour open onDone={() => {}} />
      </div>,
    );
    const next = screen.getByRole('button', { name: 'Next' });
    fireEvent.keyDown(next, { key: 'ArrowLeft' });
    expect(screen.getByRole('dialog')).toHaveTextContent('2 of 3');
    fireEvent.keyDown(screen.getByRole('button', { name: 'Next' }), { key: 'ArrowRight' });
    expect(screen.getByRole('dialog')).toHaveTextContent('1 of 3');
  });
});
