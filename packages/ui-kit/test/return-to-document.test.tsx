import { afterEach, describe, expect, it, vi } from 'vitest';
import { returnToDocument } from '../src/chrome/CommandContext.tsx';
import { armReturnToDocument, closeToDocument } from '../src/primitives/returnFocus.ts';

/** The chrome a command ran from, and a menu the user may open afterwards. */
function chrome(): { ribbon: HTMLElement; menu: () => HTMLElement } {
  document.body.innerHTML = '';
  const ribbon = document.createElement('div');
  ribbon.className = 'kiln-ribbon';
  const button = document.createElement('button');
  ribbon.append(button);
  // An inline gallery is a list that is always there: it does not hold the keys back.
  const gallery = document.createElement('div');
  gallery.setAttribute('role', 'listbox');
  ribbon.append(gallery);
  document.body.append(ribbon);
  button.focus();
  return {
    ribbon,
    menu: () => {
      const m = document.createElement('div');
      m.setAttribute('role', 'menu');
      m.className = 'kiln-menu';
      const item = document.createElement('div');
      item.tabIndex = -1;
      m.append(item);
      document.body.append(m);
      item.focus();
      return m;
    },
  };
}

describe('returnToDocument', () => {
  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  it('hands the keys back to the document after a ribbon command', () => {
    vi.useFakeTimers();
    chrome();
    const back = vi.fn();
    window.addEventListener('kiln:return-to-document', back);
    returnToDocument();
    vi.advanceTimersByTime(1000);
    window.removeEventListener('kiln:return-to-document', back);
    expect(back).toHaveBeenCalled();
  });

  it('leaves a menu the user opened since the command alone', () => {
    vi.useFakeTimers();
    const c = chrome();
    const back = vi.fn(() => {
      // The document takes the keys; the next menu opens 60 ms later.
      (document.activeElement as HTMLElement | null)?.blur();
    });
    window.addEventListener('kiln:return-to-document', back);
    returnToDocument();
    vi.advanceTimersByTime(40);
    expect(back).toHaveBeenCalledTimes(1);
    c.menu();
    vi.advanceTimersByTime(2000);
    window.removeEventListener('kiln:return-to-document', back);
    // The first hand-back only: the open menu keeps its keys.
    expect(back).toHaveBeenCalledTimes(1);
    expect(document.activeElement?.closest('[role="menu"]')).not.toBeNull();
  });

  it('keeps the keys in a box the user moved to after pressing Enter in another', () => {
    vi.useFakeTimers();
    const c = chrome();
    const header = document.createElement('input');
    const footer = document.createElement('input');
    footer.setAttribute('role', 'spinbutton');
    c.ribbon.append(header, footer);
    const back = vi.fn();
    window.addEventListener('kiln:return-to-document', back);
    // Enter in the first box runs its command; the command finishes once the user has moved to
    // the second box to type in it.
    header.focus();
    returnToDocument(header);
    vi.advanceTimersByTime(10);
    footer.focus();
    vi.advanceTimersByTime(2000);
    expect(back).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(footer);
    // Still in the box the command came from: the document takes the keys, as after any command.
    returnToDocument(footer);
    vi.advanceTimersByTime(2000);
    window.removeEventListener('kiln:return-to-document', back);
    expect(back).toHaveBeenCalled();
  });
});

describe('closeToDocument', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('closes a menu to the document after a command, unless another menu has the keys', () => {
    const back = vi.fn();
    window.addEventListener('kiln:return-to-document', back);
    armReturnToDocument();
    const e1 = new Event('focus', { cancelable: true });
    closeToDocument()(e1);
    expect(e1.defaultPrevented).toBe(true);
    expect(back).toHaveBeenCalledTimes(1);
    // The next menu is open with the focus when the first one finishes closing.
    const m = document.createElement('div');
    m.setAttribute('role', 'menu');
    const item = document.createElement('div');
    item.tabIndex = -1;
    m.append(item);
    document.body.append(m);
    item.focus();
    armReturnToDocument();
    closeToDocument()(new Event('focus', { cancelable: true }));
    window.removeEventListener('kiln:return-to-document', back);
    expect(back).toHaveBeenCalledTimes(1);
    expect(document.activeElement).toBe(item);
  });

  it('leaves the keys in a dialog the menu opened that took them first; a menu inside a dialog closes as usual', () => {
    const back = vi.fn();
    window.addEventListener('kiln:return-to-document', back);
    // Goal Seek opened from the What-If menu mounted and focused its field before the menu closed.
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    const field = document.createElement('input');
    dialog.append(field);
    document.body.append(dialog);
    field.focus();
    armReturnToDocument();
    const e = new Event('focus', { cancelable: true });
    closeToDocument()(e);
    expect(e.defaultPrevented).toBe(true);
    expect(back).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(field);
    // A menu opened inside the dialog still holds the keys as it closes: its trigger takes them.
    const menu = document.createElement('div');
    menu.setAttribute('role', 'menu');
    const item = document.createElement('div');
    item.tabIndex = -1;
    menu.append(item);
    document.body.append(menu);
    item.focus();
    const inner = new Event('focus', { cancelable: true });
    closeToDocument()(inner);
    window.removeEventListener('kiln:return-to-document', back);
    expect(inner.defaultPrevented).toBe(false);
    expect(back).not.toHaveBeenCalled();
  });
});
