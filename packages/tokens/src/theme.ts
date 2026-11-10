/**
 * Theme runtime: apply a theme/app accent to the document and keep a tiny observable store of the
 * user's preference that follows `prefers-color-scheme` while the preference is `system`.
 *
 * React bindings (`useTheme`, `ThemeProvider`) live in `@kiln/ui-kit/theme`; this module is
 * framework-free so the desktop/Android shells can call it before React mounts (no flash of the wrong
 * theme).
 */
import {
  APPS,
  type AppId,
  colors,
  type ResolvedTheme,
  THEME_PREFERENCES,
  type ThemePreference,
} from './tokens.ts';

export const THEME_STORAGE_KEY = 'kiln.theme';

export interface ThemeState {
  /** What the user chose. */
  preference: ThemePreference;
  /** What is rendered (system resolved via `prefers-color-scheme`). */
  resolved: ResolvedTheme;
}

export function isThemePreference(value: unknown): value is ThemePreference {
  return typeof value === 'string' && (THEME_PREFERENCES as readonly string[]).includes(value);
}

export function isAppId(value: unknown): value is AppId {
  return typeof value === 'string' && (APPS as readonly string[]).includes(value);
}

function systemPrefersDark(): boolean {
  return typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches;
}

/** The OS asks for more contrast (Windows contrast themes, macOS Increase Contrast). */
function systemPrefersContrast(): boolean {
  return (
    typeof matchMedia === 'function' &&
    (matchMedia('(prefers-contrast: more)').matches || matchMedia('(forced-colors: active)').matches)
  );
}

/**
 * Resolves a preference to the theme that will actually render: `system` follows the OS, high
 * contrast first when the OS asks for more contrast.
 */
export function resolveTheme(
  preference: ThemePreference,
  prefersDark = systemPrefersDark(),
  prefersContrast = systemPrefersContrast(),
): ResolvedTheme {
  if (preference === 'system') return prefersContrast ? 'hc' : prefersDark ? 'dark' : 'light';
  return preference;
}

export interface ApplyThemeOptions {
  theme?: ThemePreference;
  app?: AppId;
  /** Element to annotate; defaults to `<html>`. */
  root?: HTMLElement;
}

/**
 * Applies a theme preference and/or app accent to `root` (default `<html>`):
 * explicit themes set `data-theme`, `system` removes it so CSS follows `prefers-color-scheme` (or
 * sets `hc` with `data-theme-system` when the OS asks for more contrast);
 * `app` sets `data-app`. Also updates `<meta name="theme-color">` when present. Returns the resolved theme.
 */
export function applyTheme(options: ApplyThemeOptions = {}): ResolvedTheme {
  const root = options.root ?? document.documentElement;
  const preference = options.theme ?? readThemeAttribute(root);
  if (options.theme !== undefined) {
    // System + the OS asking for more contrast: the high-contrast theme, marked as the system's
    // so the preference reads back as `system`.
    if (preference === 'system' && resolveTheme('system') === 'hc') {
      root.setAttribute('data-theme', 'hc');
      root.setAttribute('data-theme-system', '');
    } else if (preference === 'system') {
      root.removeAttribute('data-theme');
      root.removeAttribute('data-theme-system');
    } else {
      root.setAttribute('data-theme', preference);
      root.removeAttribute('data-theme-system');
    }
  }
  if (options.app !== undefined) root.setAttribute('data-app', options.app);
  const resolved = resolveTheme(preference);
  const meta = root.ownerDocument.querySelector<HTMLMetaElement>('meta[name="theme-color"]');
  if (meta) meta.content = colors[resolved]['bg-surface-2'];
  return resolved;
}

function readThemeAttribute(root: HTMLElement): ThemePreference {
  if (root.hasAttribute('data-theme-system')) return 'system';
  const attr = root.getAttribute('data-theme');
  return isThemePreference(attr) ? attr : 'system';
}

/** Reads the persisted preference (never throws: storage may be blocked). */
export function loadThemePreference(storage: Storage | undefined = safeLocalStorage()): ThemePreference {
  try {
    const v = storage?.getItem(THEME_STORAGE_KEY);
    return isThemePreference(v) ? v : 'system';
  } catch {
    return 'system';
  }
}

/** Persists the preference (never throws). */
export function saveThemePreference(
  preference: ThemePreference,
  storage: Storage | undefined = safeLocalStorage(),
): void {
  try {
    if (preference === 'system') storage?.removeItem(THEME_STORAGE_KEY);
    else storage?.setItem(THEME_STORAGE_KEY, preference);
  } catch {
    // Storage blocked (private mode, sandboxed iframe): the choice lasts for this session only.
  }
}

function safeLocalStorage(): Storage | undefined {
  try {
    return typeof localStorage === 'undefined' ? undefined : localStorage;
  } catch {
    return undefined;
  }
}

type Listener = (state: ThemeState) => void;

export interface ThemeStore {
  getState(): ThemeState;
  setPreference(preference: ThemePreference): void;
  subscribe(listener: Listener): () => void;
}

/**
 * Creates an observable theme store bound to `root`. While the preference is `system`, OS changes of
 * `prefers-color-scheme`, `prefers-contrast` and `forced-colors` update `resolved` and notify
 * subscribers.
 */
export function createThemeStore(
  initial: ThemePreference = 'system',
  root: HTMLElement = document.documentElement,
): ThemeStore {
  const listeners = new Set<Listener>();
  let state: ThemeState = { preference: initial, resolved: applyTheme({ theme: initial, root }) };
  const queries =
    typeof matchMedia === 'function'
      ? ['(prefers-color-scheme: dark)', '(prefers-contrast: more)', '(forced-colors: active)'].map((q) =>
          matchMedia(q),
        )
      : [];

  const emit = () => {
    for (const l of listeners) l(state);
  };
  const onSystemChange = () => {
    if (state.preference !== 'system') return;
    state = { preference: 'system', resolved: applyTheme({ theme: 'system', root }) };
    emit();
  };

  return {
    getState: () => state,
    setPreference(preference) {
      if (preference === state.preference) return;
      state = { preference, resolved: applyTheme({ theme: preference, root }) };
      emit();
    },
    subscribe(listener) {
      if (listeners.size === 0) for (const q of queries) q.addEventListener('change', onSystemChange);
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0) for (const q of queries) q.removeEventListener('change', onSystemChange);
      };
    },
  };
}

/**
 * Inline script (as a string) for `<head>` that applies the saved theme before first paint.
 * Use: `<script>${THEME_BOOT_SCRIPT}</script>` in index.html templates.
 */
export const THEME_BOOT_SCRIPT = `try{var t=localStorage.getItem(${JSON.stringify(
  THEME_STORAGE_KEY,
)});if(t==='light'||t==='dark'||t==='hc')document.documentElement.setAttribute('data-theme',t)}catch(e){}`;
