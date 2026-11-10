import { t } from '@kiln/i18n';

/**
 * What a failed command says to people. A fault inside Kiln (`Internal`, or a command sent with
 * the wrong arguments: a bug, never the user's doing) gets written words, never the core's or the
 * registry's own text, which goes to the console; any other error's message is a sentence the
 * core wrote for this case.
 */
export function errorText(error: unknown): string {
  const e =
    error && typeof error === 'object'
      ? (error as { kind?: unknown; message?: unknown; name?: unknown })
      : {};
  // A command sent with arguments the chrome got wrong is a fault inside Kiln too.
  if (e.kind === 'Internal' || e.name === 'CommandArgsError') {
    console.error('Kiln: a command failed inside the core', error);
    return t(
      'Kiln could not finish this. Your work is kept: try again, and reload Kiln if it happens again.',
    );
  }
  if (typeof e.message === 'string' && e.message.trim() !== '')
    return e.message.replace(/^invalid input: /, '');
  return t('The command could not run.');
}
