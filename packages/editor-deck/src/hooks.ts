import { useStore } from 'zustand';
import type { DeckEditorModel, DeckUiState } from './model.ts';

/** Subscribes a component to part of the editor's UI state. */
export function useDeck<T>(model: DeckEditorModel, select: (s: DeckUiState) => T): T {
  return useStore(model.store, select);
}
