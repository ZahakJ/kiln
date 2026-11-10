// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML glue: serde for kiln-ooxml's DrawingML values ([`serde_xml`]), colour conversion
//! between DrawingML and kiln-doc ([`color`]), and DrawingML text properties ⇄ the model
//! ([`text`]) for the `.pptx` reader and writer and the theme files.

pub mod color;
pub mod serde_xml;
pub mod text;
