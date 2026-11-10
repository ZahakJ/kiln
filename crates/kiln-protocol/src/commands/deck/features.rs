// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Deck's Design Ideas, picture export, Outline View, Reuse Slides and media: the
//! arguments and answers of [`super::DeckCommand::ApplyDesignIdea`],
//! [`super::DeckCommand::EditOutline`], [`super::DeckCommand::ReuseSlides`],
//! [`super::DeckCommand::InsertMedia`] and the queries that go with them.

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A suggestion of the Design Ideas pane: a layout or theme variant of the current slide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckDesignIdea {
    /// What [`super::DeckCommand::ApplyDesignIdea`] takes (`band`, `panelLeft`, …).
    pub id: String,
    /// Its name for screen readers ("Title band").
    pub label: String,
    /// The slide as it would look, a PNG.
    #[serde(with = "kiln_scene::bytes")]
    #[ts(type = "Array<number>")]
    pub png: Vec<u8>,
}

/// The picture formats slides export to (File ▸ Export ▸ Change File Type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum DeckPictureFormat {
    /// PNG Portable Network Graphics.
    Png,
    /// JPEG File Interchange Format.
    Jpeg,
}

/// A file the core made for the chrome to save (exported pictures).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckFile {
    /// A file name for it (`Slide1.png`, `Plans.zip`).
    pub name: String,
    /// Its media type.
    pub mime: String,
    /// The file.
    #[serde(with = "kiln_scene::bytes")]
    #[ts(type = "Array<number>")]
    pub bytes: Vec<u8>,
}

/// A body line of Outline View: one paragraph of the slide's body placeholders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckOutlineLine {
    /// List level (0-based).
    pub level: u8,
    /// The text.
    pub text: String,
}

/// One slide in Outline View.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckOutlineEntry {
    /// The slide.
    pub slide: NodeId,
    /// The title (line 0).
    pub title: String,
    /// The body (lines 1…): every paragraph of body placeholders that have text.
    pub lines: Vec<DeckOutlineLine>,
    /// The slide has a title placeholder (its title can be typed).
    pub has_title: bool,
    /// Hidden in the show.
    pub hidden: bool,
    /// The slide holds more than the outline shows (pictures, charts, tables, text boxes and other
    /// shapes, or speaker notes): merging it into the slide before deletes them, so the chrome
    /// asks first.
    pub more: bool,
}

/// An edit in Outline View. `slide` is the slide's index; `line` is 0 for its title and `1 + n`
/// for the n-th body line of [`DeckOutlineEntry::lines`]; offsets count characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum DeckOutlineEdit {
    /// The line's text is now `text` (typing, deleting within the line, pasting plain text).
    Text {
        /// Slide index.
        slide: u32,
        /// Line.
        line: u32,
        /// The new text.
        text: String,
    },
    /// Enter at `offset`: a body line splits in two; a title splits into a new slide after it
    /// that takes the rest of the title and the body lines (at the start, a new slide before).
    Split {
        /// Slide index.
        slide: u32,
        /// Line.
        line: u32,
        /// Where.
        offset: u32,
    },
    /// Tab (`by` = 1) or Shift+Tab (−1): a body line moves a level; a first-level line promoted
    /// becomes a new slide's title with the lines after it; a title demoted becomes a line of
    /// the slide before, its lines one level down, and its slide goes.
    Indent {
        /// Slide index.
        slide: u32,
        /// Line.
        line: u32,
        /// Levels, ±1.
        by: i32,
    },
    /// Backspace at the start of a line: the line joins the one before it (a title joins the
    /// last line of the slide before, whose slide then takes its lines).
    Join {
        /// Slide index.
        slide: u32,
        /// Line.
        line: u32,
    },
}

/// A slide of another presentation offered by Reuse Slides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckReuseSlide {
    /// Its index in that presentation.
    pub index: u32,
    /// Its title.
    pub title: String,
    /// A thumbnail, PNG.
    #[serde(with = "kiln_scene::bytes")]
    #[ts(type = "Array<number>")]
    pub png: Vec<u8>,
}

/// Video or audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum DeckMediaKind {
    /// Video.
    Video,
    /// Audio.
    Audio,
}

/// A video or audio clip on the slide a show (or the stage) shows, for the chrome to play
/// over the slide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckMediaItem {
    /// The shape.
    pub shape: NodeId,
    /// Video or audio.
    pub kind: DeckMediaKind,
    /// The clip's part (for [`super::DeckQuery::MediaBytes`]); `None` for a linked clip.
    pub part: Option<String>,
    /// Its media type (`video/mp4`).
    pub mime: String,
    /// Where the shape is, slide px.
    pub rect: super::DeckRect,
    /// Trim from the start, milliseconds.
    pub trim_start_ms: u32,
    /// Trim from the end, milliseconds.
    pub trim_end_ms: u32,
}

/// The clips of a slide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckSlideMedia {
    /// The slide (index).
    pub slide: u32,
    /// Slide width, px.
    pub width: f32,
    /// Slide height, px.
    pub height: f32,
    /// Its clips, in z-order.
    pub items: Vec<DeckMediaItem>,
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values (see `crate::samples`).

    use super::*;
    use crate::samples::Samples;

    macro_rules! samples_of {
        ($($ty:ty => [$($v:expr),* $(,)?];)*) => {
            $(impl Samples for $ty {
                fn samples() -> Vec<Self> {
                    vec![$($v),*]
                }
            })*
        };
    }

    /// Every outline edit.
    #[must_use]
    pub fn edits() -> Vec<DeckOutlineEdit> {
        vec![
            DeckOutlineEdit::Text {
                slide: 0,
                line: 0,
                text: "Firing schedule".into(),
            },
            DeckOutlineEdit::Split {
                slide: 1,
                line: 2,
                offset: 5,
            },
            DeckOutlineEdit::Indent {
                slide: 2,
                line: 0,
                by: 1,
            },
            DeckOutlineEdit::Join { slide: 2, line: 1 },
        ]
    }

    /// A clip.
    #[must_use]
    pub fn media() -> DeckMediaItem {
        DeckMediaItem {
            shape: NodeId(30),
            kind: DeckMediaKind::Video,
            part: Some("/ppt/media/media1.mp4".into()),
            mime: "video/mp4".into(),
            rect: super::super::DeckRect {
                x: 320.0,
                y: 180.0,
                width: 640.0,
                height: 360.0,
            },
            trim_start_ms: 0,
            trim_end_ms: 1500,
        }
    }

    /// An outline entry.
    #[must_use]
    pub fn entry() -> DeckOutlineEntry {
        DeckOutlineEntry {
            slide: NodeId(12),
            title: "Plans".into(),
            lines: vec![
                DeckOutlineLine {
                    level: 0,
                    text: "Clay".into(),
                },
                DeckOutlineLine {
                    level: 1,
                    text: String::new(),
                },
            ],
            has_title: true,
            hidden: false,
            more: true,
        }
    }

    impl Samples for DeckOutlineEdit {
        fn samples() -> Vec<Self> {
            edits()
        }
    }

    samples_of! {
        DeckDesignIdea => [DeckDesignIdea { id: "band".into(), label: "Title band".into(), png: vec![137, 80, 78, 71] }];
        DeckPictureFormat => [DeckPictureFormat::Png, DeckPictureFormat::Jpeg];
        DeckFile => [DeckFile { name: "Slide1.png".into(), mime: "image/png".into(), bytes: vec![137, 80] }];
        DeckOutlineLine => [entry().lines[0].clone()];
        DeckOutlineEntry => [entry()];
        DeckReuseSlide => [DeckReuseSlide { index: 3, title: "Glazes".into(), png: Vec::new() }];
        DeckMediaKind => [DeckMediaKind::Video, DeckMediaKind::Audio];
        DeckMediaItem => [media()];
        DeckSlideMedia => [DeckSlideMedia { slide: 2, width: 1280.0, height: 720.0, items: vec![media()] }];
    }

    /// Registers the types with the schema visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            DeckDesignIdea, DeckPictureFormat, DeckFile, DeckOutlineLine, DeckOutlineEntry,
            DeckOutlineEdit, DeckReuseSlide, DeckMediaKind, DeckMediaItem, DeckSlideMedia,
        );
    }
}
