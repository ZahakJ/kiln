/**
 * Undo/redo for a workbook. Every command applied through the log records its inverse; commands can
 * be grouped into transactions (one undo step), and the history serialises to JSON for journals.
 *
 * Redo re-applies the original command (computing a fresh inverse), so undo → redo → undo cycles stay
 * exact even when inverses depend on state such as style indices.
 */
import { ModelError } from './address.ts';
import { applyCommand, type Command, parseCommand, resolveCommand, serializeCommand } from './commands.ts';
import type { Workbook } from './workbook.ts';

export interface LogEntry {
  label: string;
  forward: Command;
  inverse: Command;
}

export interface CommandLogOptions {
  /** Maximum undo depth (oldest entries are dropped); default 1000. */
  limit?: number;
}

export type LogListener = (e: { kind: 'exec' | 'undo' | 'redo' | 'clear'; entry?: LogEntry }) => void;

export class CommandLog {
  private undoStack: LogEntry[] = [];
  private redoStack: LogEntry[] = [];
  private txn: { label: string; forward: Command[]; inverse: Command[] }[] = [];
  private readonly limit: number;
  private readonly listeners = new Set<LogListener>();

  constructor(
    readonly workbook: Workbook,
    opts: CommandLogOptions = {},
  ) {
    this.limit = opts.limit ?? 1000;
  }

  on(l: LogListener): () => void {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  }

  private notify(kind: 'exec' | 'undo' | 'redo' | 'clear', entry?: LogEntry): void {
    for (const l of [...this.listeners]) l(entry ? { kind, entry } : { kind });
  }

  get canUndo(): boolean {
    return this.undoStack.length > 0 && this.txn.length === 0;
  }

  get canRedo(): boolean {
    return this.redoStack.length > 0 && this.txn.length === 0;
  }

  /** Label of the step Undo would revert ("Undo Typing 'abc' in A1"). */
  get undoLabel(): string | undefined {
    return this.undoStack.at(-1)?.label;
  }

  get redoLabel(): string | undefined {
    return this.redoStack.at(-1)?.label;
  }

  get undoDepth(): number {
    return this.undoStack.length;
  }

  get redoDepth(): number {
    return this.redoStack.length;
  }

  get inTransaction(): boolean {
    return this.txn.length > 0;
  }

  /** Applies a command as one undo step (or as part of the open transaction). */
  exec(command: Command, label: string = command.type): void {
    const cmd = resolveCommand(this.workbook, command);
    const inverse = applyCommand(this.workbook, cmd);
    const open = this.txn.at(-1);
    if (open) {
      open.forward.push(cmd);
      open.inverse.push(inverse);
      return;
    }
    this.push({ label, forward: cmd, inverse });
  }

  private push(entry: LogEntry): void {
    this.undoStack.push(entry);
    if (this.undoStack.length > this.limit) this.undoStack.shift();
    this.redoStack = [];
    this.notify('exec', entry);
  }

  /** Opens a transaction: commands until the matching `commit` become one undo step. Nestable. */
  begin(label = 'Edit'): void {
    this.txn.push({ label, forward: [], inverse: [] });
  }

  commit(): void {
    const t = this.txn.pop();
    if (!t) throw new ModelError('No open transaction');
    if (t.forward.length === 0) return;
    const forward: Command =
      t.forward.length === 1
        ? (t.forward[0] as Command)
        : { type: 'batch', commands: t.forward, label: t.label };
    const inverse: Command =
      t.inverse.length === 1
        ? (t.inverse[0] as Command)
        : { type: 'batch', commands: [...t.inverse].reverse(), label: t.label };
    const outer = this.txn.at(-1);
    if (outer) {
      outer.forward.push(forward);
      outer.inverse.push(inverse);
      return;
    }
    this.push({ label: t.label, forward, inverse });
  }

  /** Reverts everything applied in the innermost open transaction and closes it. */
  rollback(): void {
    const t = this.txn.pop();
    if (!t) throw new ModelError('No open transaction');
    this.workbook.transact(() => {
      for (let i = t.inverse.length - 1; i >= 0; i--) applyCommand(this.workbook, t.inverse[i] as Command);
    });
  }

  /** Runs `fn` in a transaction; rolls back and rethrows if it throws. */
  transaction<T>(label: string, fn: () => T): T {
    this.begin(label);
    let result: T;
    try {
      result = this.workbook.transact(fn);
    } catch (e) {
      this.rollback();
      throw e;
    }
    this.commit();
    return result;
  }

  undo(): boolean {
    if (this.txn.length > 0) throw new ModelError('Cannot undo during a transaction');
    const entry = this.undoStack.pop();
    if (!entry) return false;
    applyCommand(this.workbook, entry.inverse);
    this.redoStack.push(entry);
    this.notify('undo', entry);
    return true;
  }

  redo(): boolean {
    if (this.txn.length > 0) throw new ModelError('Cannot redo during a transaction');
    const entry = this.redoStack.pop();
    if (!entry) return false;
    const inverse = applyCommand(this.workbook, entry.forward);
    const next = { ...entry, inverse };
    this.undoStack.push(next);
    this.notify('redo', next);
    return true;
  }

  clear(): void {
    this.undoStack = [];
    this.redoStack = [];
    this.notify('clear');
  }

  /** The history as JSON (for journals and crash recovery). */
  serialize(): string {
    return JSON.stringify({
      undo: this.undoStack.map((e) => ({ label: e.label, forward: e.forward, inverse: e.inverse })),
      redo: this.redoStack.map((e) => ({ label: e.label, forward: e.forward, inverse: e.inverse })),
    });
  }

  /** Restores a history produced by `serialize` (the workbook must be in the matching state). */
  load(json: string): void {
    const data = JSON.parse(json) as { undo: LogEntry[]; redo: LogEntry[] };
    const rebuild = (e: LogEntry): LogEntry => ({
      label: String(e.label),
      forward: parseCommand(serializeCommand(e.forward)),
      inverse: parseCommand(serializeCommand(e.inverse)),
    });
    this.undoStack = data.undo.map(rebuild);
    this.redoStack = data.redo.map(rebuild);
  }
}
