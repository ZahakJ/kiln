/**
 * Minimal DOM matchers for the chrome tests (the kit does not depend on jest-dom).
 * Importing this file registers them with Vitest's `expect`.
 */
import { expect } from 'vitest';

interface DomMatchers<R = unknown> {
  toBeInTheDocument(): R;
  toHaveAttribute(name: string, value?: string): R;
  toHaveFocus(): R;
  toHaveTextContent(text: string | RegExp): R;
  toHaveClass(className: string): R;
  toHaveValue(value: string): R;
  toContainElement(el: Element | null): R;
}

declare module 'vitest' {
  interface Assertion<R, T> extends DomMatchers<R> {
    /** Keeps the type parameters identical to Vitest's declaration. */
    readonly __domMatchersSubject?: T;
  }
  interface AsymmetricMatchersContaining extends DomMatchers {}
}

const describe = (el: unknown) =>
  el instanceof Element
    ? `<${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : ''} class="${el.className}">`
    : String(el);

expect.extend({
  toBeInTheDocument(received: Element | null) {
    const pass = !!received && received.ownerDocument.contains(received);
    return {
      pass,
      message: () => `expected ${describe(received)} ${pass ? 'not ' : ''}to be in the document`,
    };
  },
  toHaveAttribute(received: Element | null, name: string, value?: string) {
    const has = !!received?.hasAttribute(name);
    const actual = received?.getAttribute(name);
    const pass = value === undefined ? has : has && actual === value;
    return {
      pass,
      message: () =>
        `expected ${describe(received)} ${pass ? 'not ' : ''}to have attribute ${name}${value === undefined ? '' : `="${value}"`} (got ${actual})`,
    };
  },
  toHaveFocus(received: Element | null) {
    const pass = !!received && received.ownerDocument.activeElement === received;
    return {
      pass,
      message: () =>
        `expected ${describe(received)} ${pass ? 'not ' : ''}to have focus (focused: ${describe(document.activeElement)})`,
    };
  },
  toHaveTextContent(received: Element | null, text: string | RegExp) {
    const content = received?.textContent ?? '';
    const pass = typeof text === 'string' ? content.includes(text) : text.test(content);
    return { pass, message: () => `expected "${content}" ${pass ? 'not ' : ''}to contain ${String(text)}` };
  },
  toHaveClass(received: Element | null, className: string) {
    const pass = !!received?.classList.contains(className);
    return {
      pass,
      message: () => `expected ${describe(received)} ${pass ? 'not ' : ''}to have class ${className}`,
    };
  },
  toHaveValue(received: Element | null, value: string) {
    const actual = (received as HTMLInputElement | null)?.value;
    const pass = actual === value;
    return { pass, message: () => `expected value "${actual}" ${pass ? 'not ' : ''}to be "${value}"` };
  },
  toContainElement(received: Element | null, el: Element | null) {
    const pass = !!received && !!el && received.contains(el);
    return {
      pass,
      message: () => `expected ${describe(received)} ${pass ? 'not ' : ''}to contain ${describe(el)}`,
    };
  },
});
