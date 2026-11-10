/**
 * The user's custom fill lists (File ▸ Options ▸ Advanced ▸ Edit Custom Lists in Excel). Like
 * Excel's, they belong to the user, not to the workbook: they are kept in this browser and given
 * to the core when a workbook opens and whenever they change (`grid.editing.custom_lists`). The
 * Sort dialog reads the same key.
 */

export const LISTS_KEY = 'kiln.grid.custom_lists';

/** Excel's built-in lists, which cannot be edited. */
export const BUILT_IN_LISTS: readonly (readonly string[])[] = [
  ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'],
  ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'],
  ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'],
  [
    'January',
    'February',
    'March',
    'April',
    'May',
    'June',
    'July',
    'August',
    'September',
    'October',
    'November',
    'December',
  ],
];

/** The user's lists (empty when storage is unavailable or damaged). */
export function loadUserLists(): string[][] {
  try {
    const v = JSON.parse(localStorage.getItem(LISTS_KEY) ?? '[]') as unknown;
    if (Array.isArray(v) && v.every((l) => Array.isArray(l) && l.every((x) => typeof x === 'string')))
      return (v as string[][]).filter((l) => l.length >= 2);
  } catch {
    // Private mode or a damaged entry.
  }
  return [];
}

/** Keeps the user's lists (for this session only when storage is unavailable). */
export function saveUserLists(lists: readonly (readonly string[])[]): void {
  try {
    localStorage.setItem(LISTS_KEY, JSON.stringify(lists));
  } catch {
    // Private mode.
  }
}
