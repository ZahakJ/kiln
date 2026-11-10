// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-collab-wasm
//!
//! The web's collaboration module, loaded in the app core's Worker only when a
//! document is shared or joined. [`CollabEngines`] serves the core's calls to the engines of its
//! shared documents (`kiln_collab::Engines`): an `EngineRequest` in, a
//! `Result<EngineReply, String>` out, both postcard (`kiln_protocol::collab::engine`). The core
//! reaches it through the function the Worker gives `KilnSession.setCollab`.
//!
//! `#![forbid(unsafe_code)]` is absent for the same reason as in `kiln-bridge-wasm`: the
//! `#[wasm_bindgen]` glue expands to `unsafe` ABI conversions.
#![warn(missing_docs)]

use kiln_collab::Engines;
use wasm_bindgen::prelude::wasm_bindgen;

/// Installs the panic hook so a Rust panic shows its message in the console.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// The version of the module (the suite's).
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    kiln_collab::VERSION.to_owned()
}

/// The engines of the shared documents of one app core.
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct CollabEngines {
    engines: Engines,
}

#[wasm_bindgen]
impl CollabEngines {
    /// No engines yet.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> CollabEngines {
        CollabEngines::default()
    }

    /// One call: a postcard `EngineRequest`; returns a postcard `Result<EngineReply, String>`.
    pub fn call(&mut self, request: &[u8]) -> Vec<u8> {
        self.engines.request_bytes(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_protocol::DocKind;
    use kiln_protocol::collab::engine::{EngineCall, EngineReply, EngineRequest, EngineValue};

    fn call(e: &mut CollabEngines, handle: u32, call: EngineCall) -> Result<EngineReply, String> {
        let bytes = postcard::to_allocvec(&EngineRequest { handle, call }).unwrap_or_default();
        postcard::from_bytes(&e.call(&bytes)).unwrap_or_else(|e| Err(e.to_string()))
    }

    #[test]
    fn calls_and_replies_cross_as_postcard() {
        let mut e = CollabEngines::new();
        let model = kiln_doc::Document::new().to_bytes().unwrap_or_default();
        let created = call(
            &mut e,
            0,
            EngineCall::Share {
                kind: DocKind::Write,
                model,
                room: "r".into(),
                token: "t".into(),
                peer: 5,
                name: "Ana".into(),
                now_ms: 1,
            },
        );
        let Ok(EngineReply {
            value: EngineValue::Created(handle),
            ..
        }) = created
        else {
            panic!("not created: {created:?}");
        };
        let hello = call(&mut e, handle, EngineCall::Hello);
        assert!(hello.is_ok_and(|r| r.frames.len() == 1));
        assert!(call(&mut e, handle + 1, EngineCall::Hello).is_err());
        assert!(call(&mut e, handle, EngineCall::Close).is_ok());
        assert!(call(&mut e, handle, EngineCall::Hello).is_err());
    }
}
