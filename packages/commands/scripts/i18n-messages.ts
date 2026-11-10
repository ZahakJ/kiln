/** Prints the command system's interface messages as JSON (read by `@kiln/i18n`'s extractor). */
import { commandMessages } from '../src/messages.ts';

process.stdout.write(`${JSON.stringify(commandMessages(), null, 2)}\n`);
