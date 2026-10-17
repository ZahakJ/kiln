/**
 * Spelling dictionaries on the UI side (WP-66): the web core does not carry the en-US Hunspell
 * dictionary (budget B12, PLAN.md §3.9), so the page fetches it once as a static asset and hands
 * it to the core (`write.spell.load_dictionary`); native cores have it built in. The user's own
 * words (Add to Dictionary) are a preference of this device, sent to the core when an editor
 * opens.
 */
import affUrl from '../../../../assets/dictionaries/en-US/en_US.aff?url';
import dicUrl from '../../../../assets/dictionaries/en-US/en_US.dic?url';
import type { WriteEditorModel } from '../model.ts';

const USER_KEY = 'kiln.spell.user';
const MAX_USER_WORDS = 10_000;

let loading: Promise<boolean> | null = null;

/** Fetches the en-US dictionary and gives it to the core (once per page). */
export function ensureDictionary(model: WriteEditorModel): Promise<boolean> {
  if (!loading) {
    loading = (async () => {
      try {
        const [aff, dic] = await Promise.all([
          fetch(affUrl).then((r) => (r.ok ? r.text() : Promise.reject(new Error(String(r.status))))),
          fetch(dicUrl).then((r) => (r.ok ? r.text() : Promise.reject(new Error(String(r.status))))),
        ]);
        return await model.runCore('write.spell.load_dictionary', { aff, dic });
      } catch {
        loading = null;
        return false;
      }
    })();
  }
  return loading;
}

/** The user's dictionary on this device. */
export function loadUserWords(): string[] {
  try {
    const raw = globalThis.localStorage?.getItem(USER_KEY);
    const v: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(v)
      ? v.filter((w): w is string => typeof w === 'string').slice(0, MAX_USER_WORDS)
      : [];
  } catch {
    return [];
  }
}

/** Remembers a word of Add to Dictionary on this device. */
export function rememberUserWord(word: string): void {
  const words = loadUserWords();
  if (words.includes(word)) return;
  words.push(word);
  try {
    globalThis.localStorage?.setItem(USER_KEY, JSON.stringify(words.slice(-MAX_USER_WORDS)));
  } catch {
    // Preferences are best-effort (private windows).
  }
}

/** Add to Dictionary: the core's speller and this device's list. */
export async function addToDictionary(model: WriteEditorModel, word: string): Promise<boolean> {
  const ok = await model.run('write.spell.add_word', { word });
  if (ok) rememberUserWord(word);
  return ok;
}
