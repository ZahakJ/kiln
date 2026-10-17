import { useTheme } from '@kiln/ui-kit';
import { useEffect, useRef } from 'react';
import { storePref } from './prefs.ts';

/** Stores the theme through the host whenever the user changes it. */
export function usePersistTheme(): void {
  const { preference } = useTheme();
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    void storePref('theme', preference);
  }, [preference]);
}
