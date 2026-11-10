/**
 * The spelling dictionary of the web core: the Deck core does not carry the en-US Hunspell
 * dictionary (budget B12), so the page fetches it once and hands it over
 * (`deck.spell.load_dictionary`), with this device's own words (Add to Dictionary, shared with
 * Write); native cores have it built in.
 */
import affUrl from '../../../../assets/dictionaries/en-US/en_US.aff?url';
import dicUrl from '../../../../assets/dictionaries/en-US/en_US.dic?url';
import type { DeckEditorModel } from '../model.ts';

const USER_KEY = 'kiln.spell.user';

const loaded = new WeakMap<DeckEditorModel, Promise<boolean>>();

/** This device's words (Write's Add to Dictionary list). */
export function userWords(): string[] {
  try {
    const raw = globalThis.localStorage?.getItem(USER_KEY);
    const v: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(v) ? v.filter((w): w is string => typeof w === 'string') : [];
  } catch {
    return [];
  }
}

/** Remembers a word of Add to Dictionary on this device. */
export function rememberWord(word: string): void {
  const words = userWords();
  if (words.includes(word)) return;
  words.push(word);
  try {
    globalThis.localStorage?.setItem(USER_KEY, JSON.stringify(words.slice(-10_000)));
  } catch {
    // Preferences are best-effort (private windows).
  }
}

/** Gives the core the dictionary and the user's words (once per editor). */
export function ensureDeckDictionary(model: DeckEditorModel): Promise<boolean> {
  let p = loaded.get(model);
  if (!p) {
    p = (async () => {
      try {
        const [aff, dic] = await Promise.all([
          fetch(affUrl).then((r) => (r.ok ? r.text() : Promise.reject(new Error(String(r.status))))),
          fetch(dicUrl).then((r) => (r.ok ? r.text() : Promise.reject(new Error(String(r.status))))),
        ]);
        await model.core('deck.spell.load_dictionary', { aff, dic });
        await model.core('deck.spell.user_dictionary', { words: userWords() });
        return true;
      } catch {
        loaded.delete(model);
        return false;
      }
    })();
    loaded.set(model, p);
  }
  return p;
}
