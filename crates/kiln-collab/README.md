# kiln-collab

Collaboration and version history on [Loro](https://loro.dev) 1.16 (MIT): the CRDT behind shared Kiln Write, Kiln Notes and Kiln Deck documents. See `docs/adr/0004-crdt.md` for the choice of Loro and the measurements.

| Module | What it gives |
|---|---|
| `crdt` | `Crdt` (apply, version, export_from, subscribe, undo/redo, snapshot, shallow_snapshot, checkout) and `LoroCrdt`, with an undo manager scoped to the local peer |
| `write` | `WriteBinding`: a `kiln_doc::Document` mapped onto Loro (paragraph text as `LoroText` with marks, block lists, tables, sections and stories as movable lists, property sets field by field); `share`, `join`, `commit` (an applied `Transaction` → one change), `apply_remote` (an update → model ops, `WriteChange`), `undo`/`redo` of local edits, `anchor`/`resolve` for presence, `replace` |
| `deck` | `DeckBinding`: a `kiln_deck::Presentation` (slides and sections as a `LoroTree`, shapes as movable lists, text bodies with the Write mapping, effects as a list), with the same API and `DeckChange` |
| `protocol` | `Frame` (hello, welcome, refused, update, awareness, left, snapshot request and snapshot) with a versioned binary encoding |
| `client` | `SyncClient` (the client's side of a connection, transport-free), `OfflineQueue` (what waits, as one update) and `update_frames` (a large update in parts) |
| `room` | `Room`: the relay's state of one shared document (merged CRDT, kind, the next id lane; a lane is never given out twice) |
| `presence` | `Presence`, `Anchor` (a Loro cursor), `Peers`, default colours |
| `history` | `VersionHistory`: versions stored in the shared document, `checkout_write`/`checkout_deck`, `restore_write`/`restore_deck`, `rename`, `export`, thinning to 200, `deck_changes` |
| `diff` | `word_diff` and `text_diff`: the word-level comparison of two Write documents |
| `replica` | `Replica` and `CoreChange`: both mappings behind postcard bytes |

Replicas of one document allocate model ids in different lanes (`kiln_doc::IdLanes`, given by the relay and never twice), so concurrent inserts never collide. A merge that leaves a model invalid (a list emptied by concurrent deletes, a comment range whose comment was deleted, an animation of a deleted shape) is repaired by the replica that merged it, as a change everyone receives; so is text moved by a split or join at the same moment as other edits to it (`moves`), by any replica, whoever is still connected.

Property tests: `tests/write_convergence.rs` and `tests/deck_convergence.rs` (random concurrent edits on two and three replicas, exchanged in random orders and merged in different orders; typed characters counted so that none is lost or doubled, also with a replica that leaves for good; slides and shapes checked against what was made and deleted; 128 cases each, `KILN_PROPTEST_CASES` for more); the mapping round-trips every `.docx` and `.pptx` fixture.
