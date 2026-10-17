/**
 * The UI state of Kiln Write's advanced features (WP-66): which feature pane is open (Comments,
 * Reviewing, Selection, Spelling), which feature dialog, the comment composer, and the latest
 * answers of the core's `Query::Write` for them — comment threads with their balloons, tracked
 * changes, objects, the misspelling the Spelling pane or the context menu shows. Nothing is
 * computed here: after each change of the document the model asks the core again (once per
 * frame, and only for what is on screen), as the editor's own model does.
 */
import type {
  CommentInfo,
  FeatureState,
  NodeId,
  ObjectInfo,
  RevisionsInfo,
  SpellingHit,
  WriteQuery,
} from '@kiln/protocol';
import { createStore, type StoreApi } from 'zustand/vanilla';
import { afterPaint, type WriteEditorModel, writeResult } from '../model.ts';

export type FeaturePane = 'comments' | 'review' | 'selection' | 'spelling';
export type FeatureDialog =
  | 'toc'
  | 'caption'
  | 'crossRef'
  | 'footnote'
  | 'layout'
  | 'properties'
  | 'wordGoal'
  | 'updateToc';

export interface FeatureUiState {
  /** The open feature pane (right side). */
  pane: FeaturePane | null;
  dialog: { id: FeatureDialog; tab?: string } | null;
  /** Comment threads in document order (replies after their comment), with balloons. */
  comments: CommentInfo[] | null;
  revisions: RevisionsInfo | null;
  objects: ObjectInfo[] | null;
  /** The Spelling pane's word (F7). */
  spelling: SpellingHit | null;
  /** The spelling check found nothing more. */
  spellingDone: boolean;
  /** The misspelled word under the context menu. */
  spellMenu: SpellingHit | null;
  /** The comment composer: a new comment (`parent: null`) or a reply. */
  composer: { parent: NodeId | null } | null;
  /** The comment card the user is reading or editing. */
  activeComment: NodeId | null;
}

/** The document's feature state, as last reported with the formatting. */
export function featureState(model: WriteEditorModel): FeatureState | null {
  return model.state.format?.features ?? null;
}

const INSTANCES = new WeakMap<WriteEditorModel, WriteFeatures>();

/** The features model of an editor model (one per editor). */
export function featuresOf(model: WriteEditorModel): WriteFeatures {
  let f = INSTANCES.get(model);
  if (!f) {
    f = new WriteFeatures(model);
    INSTANCES.set(model, f);
  }
  return f;
}

export class WriteFeatures {
  readonly model: WriteEditorModel;
  readonly store: StoreApi<FeatureUiState>;
  private frame: { cancel(): void } | null = null;
  private disposed = false;
  private readonly unsubscribe: () => void;
  private readonly answers = new Map<string, string>();

  constructor(model: WriteEditorModel) {
    this.model = model;
    this.store = createStore<FeatureUiState>(() => ({
      pane: null,
      dialog: null,
      comments: null,
      revisions: null,
      objects: null,
      spelling: null,
      spellingDone: false,
      spellMenu: null,
      composer: null,
      activeComment: null,
    }));
    // Every new answer of the editor's model (formatting, selection, view) may change what the
    // feature panes and balloons show.
    this.unsubscribe = model.store.subscribe((s, prev) => {
      if (s.format !== prev.format || s.selection !== prev.selection || s.view !== prev.view) this.refresh();
    });
    this.refresh();
  }

  get state(): FeatureUiState {
    return this.store.getState();
  }

  set(patch: Partial<FeatureUiState>): void {
    this.store.setState(patch);
    if ('pane' in patch || 'composer' in patch) this.refresh();
  }

  /** Opens a pane, or closes it when it is open (the ribbon toggles). */
  togglePane(pane: FeaturePane): void {
    this.set({ pane: this.state.pane === pane ? null : pane });
  }

  openDialog(id: FeatureDialog, tab?: string): void {
    this.store.setState({ dialog: { id, ...(tab ? { tab } : {}) } });
  }

  closeDialog(): void {
    this.store.setState({ dialog: null });
    this.model.controller?.focus();
  }

  /** Whether comment balloons are on screen (Simple or All Markup with comments shown). */
  balloonsShown(): boolean {
    const f = featureState(this.model);
    return !!f && f.comments > 0 && f.show_comments && (f.markup === 'Simple' || f.markup === 'All');
  }

  /** Asks the core again for what is shown (next frame; several changes make one round trip). */
  refresh(): void {
    if (this.frame || this.disposed) return;
    this.frame = afterPaint(() => {
      this.frame = null;
      void this.flush();
    });
  }

  private async ask(q: WriteQuery) {
    try {
      return writeResult(await this.model.client.query(this.model.doc, { Write: q }));
    } catch {
      return null;
    }
  }

  private async flush(): Promise<void> {
    const s = this.state;
    const patch: Partial<FeatureUiState> = {};
    const jobs: Promise<void>[] = [];
    const f = featureState(this.model);
    if (s.pane === 'comments' || this.balloonsShown() || (f && f.comments > 0 && s.comments === null)) {
      jobs.push(
        this.ask('Comments').then((r) => {
          if (r && 'Comments' in r) patch.comments = r.Comments;
        }),
      );
    } else if (f && f.comments === 0 && s.comments && s.comments.length > 0) {
      patch.comments = [];
    }
    if (s.pane === 'review')
      jobs.push(
        this.ask('Revisions').then((r) => {
          if (r && 'Revisions' in r) patch.revisions = r.Revisions;
        }),
      );
    if (s.pane === 'selection' || f?.object)
      jobs.push(
        this.ask('Objects').then((r) => {
          if (r && 'Objects' in r) patch.objects = r.Objects;
        }),
      );
    await Promise.all(jobs);
    if (this.disposed) return;
    // Unchanged answers keep their objects (nothing re-renders for them).
    for (const key of Object.keys(patch) as (keyof FeatureUiState)[]) {
      const json = JSON.stringify(patch[key]);
      if (this.answers.get(key) === json) delete patch[key];
      else this.answers.set(key, json);
    }
    if (Object.keys(patch).length > 0) this.store.setState(patch);
  }

  /** The Spelling pane's next word (F7): selects it in the document and shows it. */
  async nextMisspelling(): Promise<void> {
    const r = await this.ask({ NextMisspelling: { from: null } });
    const hit = r && 'Spelling' in r ? r.Spelling : null;
    this.store.setState({ spelling: hit, spellingDone: !hit });
    if (hit) await this.model.runCore('write.spell.next');
  }

  /** The misspelled word at a position (the context menu). */
  async spellingAt(block: NodeId, offset: number): Promise<SpellingHit | null> {
    const r = await this.ask({ SpellingAt: { pos: { block, offset } } });
    const hit = r && 'Spelling' in r ? r.Spelling : null;
    this.store.setState({ spellMenu: hit });
    return hit;
  }

  dispose(): void {
    this.disposed = true;
    this.frame?.cancel();
    this.unsubscribe();
    INSTANCES.delete(this.model);
  }
}

/** "2 h", "Yesterday", "3 Oct" — when a comment was written, relative to now. */
export function whenLabel(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return '';
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return '';
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 60) return 'Now';
  if (s < 3600) return `${Math.floor(s / 60)} min`;
  if (s < 86_400) return `${Math.floor(s / 3600)} h`;
  if (s < 2 * 86_400) return 'Yesterday';
  const d = new Date(t);
  return d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

/** Two-letter avatar text for a reviewer. */
export function avatarText(name: string, initials?: string | null): string {
  const i = (initials ?? '').trim();
  if (i) return i.slice(0, 2).toUpperCase();
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const out = parts
    .slice(0, 2)
    .map((p) => p[0] ?? '')
    .join('');
  return (out || '?').toUpperCase();
}
