/**
 * The documents of this page with unsaved changes, from the cores' events, and what leaving the
 * page does to them. With "Keep recovery info" on (Options ▸ Save) their journals bring them back
 * after any close. With it off nothing would, so the browser asks before the page goes.
 */
import type { DocSummary, KilnClient } from '@kiln/protocol';
import { loadOptions, subscribeOptions } from '@kiln/ui-kit';
import { useSyncExternalStore } from 'react';

export interface UnsavedDocument {
  doc: number;
  /** The file's name, or the title of a document never saved. */
  name: string;
}

const dirty = new Map<number, string>();
let documents: readonly UnsavedDocument[] = [];
const listeners = new Set<() => void>();
/** The page is going on purpose: the question was answered, or nothing is at stake. */
let leaving = false;

function changed(): void {
  documents = [...dirty].map(([doc, name]) => ({ doc, name }));
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function note(summary: DocSummary): void {
  const name = summary.path?.split(/[/\\]/).pop() ?? summary.title;
  if (summary.dirty ? dirty.get(summary.doc) === name : !dirty.has(summary.doc)) return;
  if (summary.dirty) dirty.set(summary.doc, name);
  else dirty.delete(summary.doc);
  changed();
}

/** The open documents with unsaved changes. */
export function unsavedDocuments(): readonly UnsavedDocument[] {
  return documents;
}

/** Whether a reload brings unsaved changes back (their journals, Options ▸ Save). */
export function keepsRecovery(): boolean {
  return loadOptions().save.keepRecoveryInfo;
}

/** What a reload does to the page's work right now, kept up to date while the user types. */
export function useReloadCost(): 'nothing' | 'recovery' | 'asks' {
  const unsaved = useSyncExternalStore(subscribe, () => documents.length > 0);
  const recovery = useSyncExternalStore(subscribeOptions, keepsRecovery);
  if (!unsaved) return 'nothing';
  return recovery ? 'recovery' : 'asks';
}

/** Follows the documents' unsaved state through the page's client. */
export function watchDocuments(client: KilnClient): () => void {
  return client.subscribe((event) => {
    if ('Opened' in event) note(event.Opened.summary);
    else if ('Changed' in event) note(event.Changed.summary);
    else if ('Saved' in event) note(event.Saved.summary);
    else if ('Closed' in event && dirty.delete(event.Closed.doc)) changed();
  });
}

/** The next unload is meant: the user answered for the unsaved documents. */
export function allowLeaving(): void {
  leaving = true;
}

/**
 * Asks before the page goes while unsaved work has no journal to come back from, or while a journal
 * write that could not go ahead of the page (`store.ts`: a large checkpoint) is still under way.
 */
export function guardLeaving(writing: () => boolean): void {
  window.addEventListener('beforeunload', (event) => {
    if (leaving) return;
    if ((documents.length > 0 && !keepsRecovery()) || writing()) event.preventDefault();
  });
}
