// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arguments and answers of the Kiln Write commands that complete Word's ribbon: symbol
//! fonts, AutoCorrect, page movement, hiding objects, balloons, drag and drop of text, Table
//! Properties, grouping, shape styles and effects, text direction in shapes, cropping to a shape,
//! picture corrections, the proofing language and sorting. The commands themselves are variants
//! of [`super::WriteCommand`].

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::edit::{CellVerticalAlign, TextAlign};

/// One AutoCorrect replacement (File ▸ Options ▸ Proofing ▸ AutoCorrect Options): typed text
/// and what replaces it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct AutoCorrectEntry {
    /// What is typed (`teh`, `(c)`).
    pub from: String,
    /// What replaces it (`the`, `©`).
    pub to: String,
}

/// One character of a symbol font for the Symbol dialog.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct SymbolChar {
    /// The font's character code (`w:sym/@w:char`, `F0xx` for the classic symbol fonts).
    pub code: u32,
    /// The Unicode character it shows as.
    pub text: String,
}

/// A picture's image (Read Mode's zoomed view).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PictureData {
    /// MIME type (`image/png`, `image/jpeg`…).
    pub content_type: String,
    /// The bytes.
    pub bytes: Vec<u8>,
    /// Its alt text.
    pub alt_text: Option<String>,
}

/// The encoding of a plain text file (the File Conversion dialog).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TextEncodingInfo {
    /// The encoding the file is read and saved in (a [`TextEncodingChoice::name`]).
    pub encoding: String,
    /// The detection is sure (no File Conversion question when the file opens).
    pub confident: bool,
    /// Every encoding the file could be read in, the likeliest first.
    pub choices: Vec<TextEncodingChoice>,
    /// Characters of the document the encoding cannot hold (saved as `?`), at most 20.
    pub unrepresentable: Vec<String>,
}

/// One encoding of the File Conversion list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TextEncodingChoice {
    /// WHATWG name (`windows-1251`).
    pub name: String,
    /// Word's name (`Cyrillic (Windows)`).
    pub title: String,
    /// The start of the file read in this encoding.
    pub preview: String,
}

/// Where dragged text would drop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct DropTarget {
    /// The position.
    pub pos: crate::selection::TextPos,
    /// The caret there (document coordinates).
    pub caret: crate::geom::Rect,
    /// Inside the selection (a drop there does nothing).
    pub inside: bool,
}

/// A tracked change's balloon in the margin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RevisionBalloonInfo {
    /// The paragraph of the change.
    pub para: kiln_common::NodeId,
    /// Where the change starts there.
    pub offset: u32,
    /// Deleted text (else a formatting change).
    pub deleted: bool,
    /// What the balloon says: "Deleted: …" or "Formatted".
    pub text: String,
    /// The reviewer.
    pub author: String,
    /// The reviewer's colour `RRGGBB`.
    pub color: String,
    /// The balloon (document coordinates).
    pub rect: crate::geom::Rect,
    /// The change in the text (document coordinates).
    pub anchor: crate::geom::Point,
}

/// Show Markup ▸ Balloons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum BalloonMode {
    /// Show Only Comments and Formatting in Balloons (Word's default): insertions and deletions
    /// inline, comments and formatting changes in the margin.
    #[default]
    CommentsAndFormatting,
    /// Show Revisions in Balloons: deleted text and formatting changes move to the margin too.
    Revisions,
    /// Show All Revisions Inline: no balloons; comments are marked in the text.
    Inline,
}

/// A width as Word stores it: points, or a percentage of the available width.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct PreferredWidth {
    /// Points, or percent when `percent`; 0 means no preferred width (automatic).
    pub value: f32,
    /// A percentage.
    pub percent: bool,
}

/// A row height: at least or exactly this many points (0: automatic).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct RowHeightSpec {
    /// Points (0: no height, the row follows its content).
    pub points: f32,
    /// Exactly (else at least).
    pub exact: bool,
}

/// Default cell margins of a table, in points.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct CellMarginsSpec {
    /// Top.
    pub top: f32,
    /// Left.
    pub left: f32,
    /// Bottom.
    pub bottom: f32,
    /// Right.
    pub right: f32,
}

/// Table Properties (Table Layout ▸ Properties): every field that is `Some` is set. The row,
/// column and cell fields apply to the rows, columns and cells of the selection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct TablePropertiesSpec {
    /// Table ▸ Preferred width.
    pub width: Option<PreferredWidth>,
    /// Table ▸ Alignment.
    pub alignment: Option<TextAlign>,
    /// Table ▸ Indent from left, points.
    pub indent: Option<f32>,
    /// Table ▸ Text wrapping: Around (a floating table) or None.
    pub wrap_around: Option<bool>,
    /// Options ▸ Automatically resize to fit contents.
    pub autofit: Option<bool>,
    /// Options ▸ Default cell margins.
    pub cell_margins: Option<CellMarginsSpec>,
    /// Options ▸ Allow spacing between cells, points (0: none).
    pub cell_spacing: Option<f32>,
    /// Row ▸ Specify height.
    pub row_height: Option<RowHeightSpec>,
    /// Row ▸ Allow row to break across pages.
    pub row_can_break: Option<bool>,
    /// Row ▸ Repeat as header row at the top of each page.
    pub row_header: Option<bool>,
    /// Column ▸ Preferred width.
    pub column_width: Option<PreferredWidth>,
    /// Cell ▸ Preferred width.
    pub cell_width: Option<PreferredWidth>,
    /// Cell ▸ Vertical alignment.
    pub cell_v_align: Option<CellVerticalAlign>,
    /// Cell ▸ Options ▸ Wrap text.
    pub cell_wrap: Option<bool>,
    /// Alt Text ▸ Title.
    pub alt_title: Option<String>,
    /// Alt Text ▸ Description.
    pub alt_description: Option<String>,
}

/// Shape Format ▸ Shape Styles: a theme colour and one of Word's style levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct ShapeStylePreset {
    /// The theme colour: 0 dark 1 (text), 1–6 accent 1–6.
    pub color: u8,
    /// The level.
    pub level: ShapeStyleLevel,
}

/// The rows of Word's Shape Styles gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ShapeStyleLevel {
    /// Colored Outline: white fill, coloured outline.
    Outline,
    /// Colored Fill: solid colour, darker outline.
    Fill,
    /// Light 1 Outline, Colored Fill: solid colour, white outline.
    LightOutline,
    /// Subtle Effect: a light tint of the colour.
    Subtle,
    /// Moderate Effect: a gradient of the colour.
    Moderate,
    /// Intense Effect: a strong gradient of the colour with a shadow.
    Intense,
}

/// Shape Fill ▸ Gradient: a two-colour linear gradient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GradientSpec {
    /// Start colour `RRGGBB`.
    pub from: String,
    /// End colour `RRGGBB`.
    pub to: String,
    /// Direction in degrees (0: left to right, 90: top to bottom).
    pub angle: f32,
}

/// Shape Format ▸ Shape Effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ShapeEffect {
    /// No effect.
    None,
    /// Shadow ▸ Outer, offset bottom right.
    Shadow,
    /// Glow in the shape's colour.
    Glow,
    /// Soft edges.
    SoftEdges,
}

/// Shape Format ▸ Text Direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum TextDirection {
    /// Horizontal.
    #[default]
    Horizontal,
    /// Rotate all text 90°.
    Rotate90,
    /// Rotate all text 270°.
    Rotate270,
}

/// Picture Format ▸ Corrections: brightness and contrast in percent (−40…+40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct PictureCorrection {
    /// Brightness.
    pub brightness: i8,
    /// Contrast.
    pub contrast: i8,
}

/// What Sort compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum SortKind {
    /// Text, in alphabetical order.
    #[default]
    Text,
    /// Numbers.
    Number,
    /// Dates.
    Date,
}

/// Home ▸ Sort: the selected paragraphs, or the rows of the table at the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct SortSpec {
    /// Compare as.
    pub kind: SortKind,
    /// Descending.
    pub descending: bool,
    /// The table column to sort by (0-based; paragraphs ignore it).
    pub column: u16,
    /// The first row (or paragraph) is a header and stays first.
    pub header: bool,
    /// Case sensitive.
    pub case_sensitive: bool,
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values (see `crate::samples`).

    use kiln_common::NodeId;

    use super::*;
    use crate::commands::Command;
    use crate::commands::write::WriteCommand as W;
    use crate::samples::Samples;
    use crate::selection::TextPos;

    /// Every command.
    pub fn commands() -> Vec<Command> {
        vec![
            W::InsertSymbol {
                font: "Wingdings".into(),
                code: 0xF04A,
            },
            W::ToggleUnicode,
            W::SetAutoCorrect {
                entries: vec![AutoCorrectEntry {
                    from: "teh".into(),
                    to: "the".into(),
                }],
            },
            W::PageMovement { side_to_side: true },
            W::SplitRegion {
                region: Some(crate::geom::Rect::new(0.0, 1200.0, 900.0, 400.0)),
            },
            W::SplitRegion { region: None },
            W::ObjectVisible {
                id: Some(NodeId(12)),
                visible: false,
            },
            W::ObjectVisible {
                id: None,
                visible: true,
            },
            W::Balloons {
                mode: BalloonMode::Revisions,
            },
            W::MoveSelection {
                to: TextPos {
                    block: NodeId(3),
                    offset: 7,
                },
                copy: true,
            },
            W::TableProperties {
                spec: Box::new(TablePropertiesSpec {
                    width: Some(PreferredWidth {
                        value: 100.0,
                        percent: true,
                    }),
                    alignment: Some(TextAlign::Center),
                    row_height: Some(RowHeightSpec {
                        points: 18.0,
                        exact: false,
                    }),
                    cell_v_align: Some(CellVerticalAlign::Center),
                    alt_title: Some("Prices «2026»".into()),
                    ..TablePropertiesSpec::default()
                }),
            },
            W::GroupObjects { ungroup: false },
            W::GroupObjects { ungroup: true },
            W::TextEncoding {
                encoding: "windows-1251".into(),
                reload: true,
            },
            W::ShapeStyle {
                preset: ShapeStylePreset {
                    color: 1,
                    level: ShapeStyleLevel::Moderate,
                },
            },
            W::ShapeGradient {
                gradient: Some(GradientSpec {
                    from: "4472C4".into(),
                    to: "1F3864".into(),
                    angle: 90.0,
                }),
            },
            W::ShapeEffect {
                effect: ShapeEffect::Shadow,
            },
            W::ShapeTextDirection {
                direction: TextDirection::Rotate270,
            },
            W::CropToShape {
                preset: "ellipse".into(),
            },
            W::PictureCorrections {
                correction: PictureCorrection {
                    brightness: 20,
                    contrast: -20,
                },
            },
            W::SetLanguage {
                lang: "fr-FR".into(),
                no_proof: false,
            },
            W::InsertEquation {
                linear: "e^(i\\pi) + 1 = 0".into(),
            },
            W::Sort {
                spec: SortSpec {
                    kind: SortKind::Number,
                    descending: true,
                    column: 1,
                    header: true,
                    case_sensitive: false,
                },
            },
        ]
        .into_iter()
        .map(Command::Write)
        .collect()
    }

    crate::samples::samples_for! {
        AutoCorrectEntry => vec![AutoCorrectEntry { from: "(c)".into(), to: "©".into() }];
        SymbolChar => vec![SymbolChar { code: 0xF04A, text: "☺".into() }];
        DropTarget => vec![DropTarget {
            pos: TextPos { block: NodeId(4), offset: 2 },
            caret: crate::geom::Rect::new(10.0, 20.0, 1.0, 18.0),
            inside: false,
        }];
        BalloonMode => vec![BalloonMode::CommentsAndFormatting, BalloonMode::Inline];
        RevisionBalloonInfo => vec![RevisionBalloonInfo {
            para: NodeId(9),
            offset: 3,
            deleted: true,
            text: "Deleted: «glaze»".into(),
            author: "Zoë".into(),
            color: "2B579A".into(),
            rect: crate::geom::Rect::new(830.0, 120.0, 240.0, 36.0),
            anchor: crate::geom::Point::new(200.0, 130.0),
        }];
        PreferredWidth => vec![PreferredWidth { value: 0.0, percent: false }];
        RowHeightSpec => vec![RowHeightSpec { points: 14.5, exact: true }];
        CellMarginsSpec => vec![CellMarginsSpec { top: 0.0, left: 5.4, bottom: 0.0, right: 5.4 }];
        TablePropertiesSpec => vec![TablePropertiesSpec::default()];
        ShapeStylePreset => vec![ShapeStylePreset { color: 0, level: ShapeStyleLevel::Outline }];
        ShapeStyleLevel => vec![ShapeStyleLevel::Fill, ShapeStyleLevel::Intense];
        GradientSpec => vec![GradientSpec { from: "FFFFFF".into(), to: "000000".into(), angle: 45.0 }];
        ShapeEffect => vec![ShapeEffect::None, ShapeEffect::Glow];
        TextDirection => vec![TextDirection::Horizontal, TextDirection::Rotate90];
        PictureCorrection => vec![PictureCorrection::default()];
        SortKind => vec![SortKind::Text, SortKind::Date];
        SortSpec => vec![SortSpec::default()];
    }

    /// Registers the types with the schema and test-vector visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            AutoCorrectEntry, SymbolChar, DropTarget, RevisionBalloonInfo, BalloonMode, PreferredWidth, RowHeightSpec,
            CellMarginsSpec, TablePropertiesSpec, ShapeStylePreset, ShapeStyleLevel, GradientSpec,
            ShapeEffect, TextDirection, PictureCorrection, SortKind, SortSpec,
        );
    }
}
