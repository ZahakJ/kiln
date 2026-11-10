# kiln-collab-wasm

The web's collaboration module: `wasm-bindgen` exports of `kiln-collab` (the CRDT replica of a shared document, the sync protocol client, presence and Version History). The page loads it only when a document is shared or joined, next to the app core that holds the document, so the app cores stay within their size budget (B12). The core passes each applied transaction to `CollabReplica.commit` and applies what `applyRemote`, `undo` and `redo` return.
