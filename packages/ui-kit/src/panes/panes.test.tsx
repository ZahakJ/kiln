import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import '../chrome/test-matchers.ts';
import { TaskPaneHost } from './TaskPaneHost.tsx';

const panes = [
  { id: 'styles', title: 'Styles', icon: 'StylesPane', content: <p>Style list</p> },
  { id: 'comments', title: 'Comments', icon: 'ShowComments', content: <p>Comment list</p> },
];

describe('TaskPaneHost', () => {
  it('shows a single pane with its title and closes from the header', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<TaskPaneHost panes={[panes[0] as (typeof panes)[number]]} onClose={onClose} />);
    expect(screen.getByRole('complementary', { name: 'Styles' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Styles' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Close Styles' }));
    expect(onClose).toHaveBeenCalledWith('styles');
  });

  it('tabs several panes with arrow-key navigation', async () => {
    const user = userEvent.setup();
    render(<TaskPaneHost panes={panes} />);
    const styles = screen.getByRole('tab', { name: 'Styles' });
    expect(styles).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tabpanel')).toHaveTextContent('Style list');
    styles.focus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Comments' })).toHaveFocus();
    expect(screen.getByRole('tabpanel')).toHaveTextContent('Comment list');
  });

  it('resizes with the keyboard and pointer, clamped to min/max', () => {
    const onWidthChange = vi.fn();
    render(
      <TaskPaneHost
        panes={panes}
        defaultWidth={300}
        minWidth={240}
        maxWidth={400}
        onWidthChange={onWidthChange}
      />,
    );
    const splitter = screen.getByRole('separator', { name: /Resize/ });
    expect(splitter).toHaveAttribute('aria-valuenow', '300');
    fireEvent.keyDown(splitter, { key: 'ArrowLeft' }); // docked right: ← widens
    expect(splitter).toHaveAttribute('aria-valuenow', '316');
    fireEvent.keyDown(splitter, { key: 'End' });
    expect(splitter).toHaveAttribute('aria-valuenow', '400');
    fireEvent.pointerDown(splitter, { clientX: 500, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 700, pointerId: 1 });
    fireEvent.pointerUp(splitter, { pointerId: 1 });
    expect(splitter).toHaveAttribute('aria-valuenow', '240');
    expect(onWidthChange).toHaveBeenLastCalledWith(240);
  });

  it('moves to the other side from the options menu', async () => {
    const user = userEvent.setup();
    const onSideChange = vi.fn();
    render(<TaskPaneHost panes={panes} onSideChange={onSideChange} />);
    await user.click(screen.getByRole('button', { name: 'Task pane options' }));
    await user.click(await screen.findByRole('menuitem', { name: 'Move to the left' }));
    expect(onSideChange).toHaveBeenCalledWith('left');
  });
});
