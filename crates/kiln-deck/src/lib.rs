// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-deck
//!
//! The Kiln Deck presentation model (PLAN.md §3.4), shaped like PresentationML.
//!
//! - **Structure:** [`Presentation`] holds the slide size, [`Master`]s (each with its DrawingML
//!   theme, colour map, background, title/body/other text styles and [`Layout`]s), [`Slide`]s,
//!   [`Section`]s, the [`NotesMaster`], the default text style and a parts store for media and
//!   charts. Every slide, layout, master, shape, effect, comment and section has a stable
//!   [`NodeId`].
//! - **Shapes** ([`Shape`]): geometry shapes and text boxes, pictures with crop, groups with a
//!   child coordinate space, tables, chart handles, connectors, media; EMU [`Transform`]s;
//!   fills, lines and effects are kiln-ooxml's DrawingML types. A placeholder is any shape with
//!   [`Shape::placeholder`] set.
//! - **Text** ([`TextBody`]): kiln-doc paragraphs and runs (Write's engine and ops) with
//!   DrawingML body properties (insets, anchor, autofit, columns, wrap, direction), list styles
//!   and paragraph extras (bullets, spacing).
//! - **Inheritance** ([`inherit`]): placeholders resolve position, size, shape and body
//!   properties and text styles slide → layout → master on access, matched by type and index as
//!   PowerPoint does; backgrounds and colour maps likewise.
//! - **Motion:** [`Transition`]s (the twelve of the Transitions tab plus Morph, with a fallback
//!   for every PowerPoint transition) and the animation [`Timeline`] (entrance, emphasis and
//!   exit effects with PowerPoint's preset ids, start modes, duration, delay, schedule).
//! - **Editing:** every change is a [`Transaction`] of [`Op`]s; applying one returns its
//!   inverse ([`UndoStack`] groups typing per word and explicit groups). [`edit`] builds the
//!   transactions for slide, shape, arrange, text, section, motion and theme commands; text
//!   edits delegate to kiln-doc ops.
//! - **Themes** ([`themes`]): the `.kdeck-theme` format (an OOXML theme plus master and layout
//!   definitions), the eleven standard layouts and the twelve bundled themes.
//! - **Storage:** postcard behind a `KDCK` header ([`Presentation::to_bytes`]).
//!
//! No layout or rendering here: `kiln-deck-layout` turns slides into Scenes.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod animation;
pub mod dml;
pub mod edit;
mod error;
pub mod geom;
pub mod inherit;
pub mod ops;
pub mod presentation;
mod serial;
pub mod shape;
pub mod slide;
pub mod text;
pub mod themes;
pub mod transition;
pub mod undo;

pub use animation::{
    AnimationEffect, Effect, EffectClass, EffectStart, EffectTarget, Timeline, TriggerSequence,
};
pub use error::{DeckError, Result};
pub use geom::{ChildSpace, MAX_COORD, Rect, Transform};
pub use inherit::{Chain, ResolvedParagraph, TextStyleKind};
pub use kiln_common::NodeId;
pub use ops::{EditKind, Op, TextTarget, Transaction};
pub use presentation::{Owner, Part, Presentation, ShapePath};
pub use shape::{
    AutoShape, CellProps, ChartRef, Connection, Connector, Crop, Geometry, Group, MAX_TABLE_COLS,
    MAX_TABLE_ROWS, Media, MediaKind, MediaSource, Picture, Placeholder, PlaceholderType, Shape,
    ShapeKind, ShapeProps, ShapeStyle, Table, TableCell, TableRow,
};
pub use slide::{
    Background, ColorMapping, Comment, CommentReply, HeaderFooter, Layout, LayoutKind, Master,
    Notes, NotesMaster, Section, Slide, SlideSize, SlideSizeKind,
};
pub use text::{
    Autofit, BodyProps, Bullet, DeckParaProps, LevelStyle, ListStyle, TextAnchor, TextBody,
    TextSpacing,
};
pub use themes::ThemeFile;
pub use transition::{Transition, TransitionEffect};
pub use undo::UndoStack;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
