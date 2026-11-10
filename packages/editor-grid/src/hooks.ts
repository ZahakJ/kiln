import { useStore } from 'zustand';
import type { GridEditorModel, GridUiState } from './model.ts';

/** Subscribes a component to part of the editor's UI state. */
export function useGrid<T>(model: GridEditorModel, select: (s: GridUiState) => T): T {
  return useStore(model.store, select);
}
