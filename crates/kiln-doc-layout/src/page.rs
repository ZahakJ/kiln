// SPDX-License-Identifier: AGPL-3.0-or-later
//! The result of layout: pages holding positioned items in page coordinates (px from the page's
//! top-left corner).

use std::sync::Arc;

use kiln_common::NodeId;
use kiln_doc::Drawing;
use kiln_scene::Bounds;
use kiln_text::Rgba;

use crate::para::{Obstacle, ParaBox};
use crate::style::Rule;

/// Which story a line belongs to (hit testing and editing stay in the story clicked).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoryKind {
    /// The main text.
    Body,
    /// A header or footer.
    HeaderFooter(NodeId),
    /// A footnote or endnote.
    Note(NodeId),
    /// A text box.
    TextBox(NodeId),
}

/// A line of text on a page.
#[derive(Debug, Clone)]
pub struct LineItem {
    /// The paragraph.
    pub para: Arc<ParaBox>,
    /// Index into `para.lines`.
    pub line: u32,
    /// Page x of the paragraph's text area.
    pub x: f32,
    /// Page y of the paragraph's origin (line tops are relative to it).
    pub y: f32,
    /// The story.
    pub story: StoryKind,
    /// The shading behind the line (paragraph or cell): text in the automatic colour is black
    /// or white against it, as in Word, in both themes.
    pub fill: Option<kiln_text::Rgba>,
    /// Text turned in a shape (Text Direction): the centre it turns around (page coordinates)
    /// and the quarter turns, 1 clockwise (90°) or −1 (270°).
    pub turn: Option<(f32, f32, i8)>,
}

impl LineItem {
    /// The line box in page coordinates: the paragraph's text area width, the line's height.
    #[must_use]
    pub fn rect(&self) -> Bounds {
        let Some(info) = self.para.lines.get(self.line as usize) else {
            return Bounds::ZERO;
        };
        let piece_x = self
            .para
            .pieces
            .get(info.piece as usize)
            .map_or(0.0, |p| p.x);
        let w = self
            .para
            .pieces
            .get(info.piece as usize)
            .map_or(self.para.width, |p| p.layout.width);
        Bounds::new(self.x + piece_x, self.y + info.top, w, info.height)
    }

    /// Where the line shows on the page: its box, turned with its shape's text.
    #[must_use]
    pub fn shown_rect(&self) -> Bounds {
        let r = self.rect();
        match self.turn {
            Some((cx, cy, q)) => turned_bounds(r, cx, cy, q),
            None => r,
        }
    }

    /// The baseline in page coordinates.
    #[must_use]
    pub fn baseline(&self) -> f32 {
        self.para
            .text_line(self.line as usize)
            .map_or(self.y, |(p, l)| self.y + p.y + l.baseline)
    }
}

/// The bounds of a rectangle turned by `quarters` quarter turns (1: 90° clockwise, −1: 270°)
/// around `(cx, cy)`.
#[must_use]
pub fn turned_bounds(r: Bounds, cx: f32, cy: f32, quarters: i8) -> Bounds {
    let turn = |x: f32, y: f32| {
        if quarters >= 0 {
            (cx - (y - cy), cy + (x - cx))
        } else {
            (cx + (y - cy), cy - (x - cx))
        }
    };
    let pts = [
        turn(r.x, r.y),
        turn(r.right(), r.y),
        turn(r.x, r.bottom()),
        turn(r.right(), r.bottom()),
    ];
    let x0 = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let y0 = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let x1 = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let y1 = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    Bounds::from_corners(x0, y0, x1, y1)
}

/// How a drawing sits relative to text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// In the line of text.
    Inline,
    /// Floating, behind the text.
    Behind,
    /// Floating, beside or in front of the text.
    Front,
}

/// A picture or shape on a page.
#[derive(Debug, Clone)]
pub struct DrawingItem {
    /// Its rectangle.
    pub rect: Bounds,
    /// The drawing.
    pub drawing: Arc<Drawing>,
    /// Layer.
    pub layer: Layer,
    /// The paragraph it is anchored in.
    pub para: NodeId,
    /// Model offset of the object in the paragraph.
    pub offset: usize,
    /// A tracked insertion or deletion: the frame's colour.
    pub revision: Option<Rgba>,
}

/// Something drawn on a page.
#[derive(Debug, Clone)]
pub enum Item {
    /// A line of text.
    Line(LineItem),
    /// A filled rectangle (shading, highlights).
    Fill {
        /// Where.
        rect: Bounds,
        /// Colour.
        color: Rgba,
    },
    /// A horizontal or vertical border line centred on `x0,y0 → x1,y1`.
    Rule {
        /// Start x.
        x0: f32,
        /// Start y.
        y0: f32,
        /// End x.
        x1: f32,
        /// End y.
        y1: f32,
        /// The line.
        rule: Rule,
    },
    /// A picture or shape.
    Drawing(DrawingItem),
}

impl Item {
    /// Sets the background of a line that has none yet (cell shading under its paragraphs).
    pub fn set_fill(&mut self, fill: Option<Rgba>) {
        if let (Item::Line(l), Some(_)) = (self, fill)
            && l.fill.is_none()
        {
            l.fill = fill;
        }
    }

    /// Moves the item.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Item::Line(l) => {
                l.x += dx;
                l.y += dy;
                if let Some((cx, cy, _)) = &mut l.turn {
                    *cx += dx;
                    *cy += dy;
                }
            }
            Item::Fill { rect, .. } => *rect = rect.translate(dx, dy),
            Item::Rule { x0, y0, x1, y1, .. } => {
                *x0 += dx;
                *x1 += dx;
                *y0 += dy;
                *y1 += dy;
            }
            Item::Drawing(d) => d.rect = d.rect.translate(dx, dy),
        }
    }

    /// Bounds in page coordinates.
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        match self {
            Item::Line(l) => l.rect(),
            Item::Fill { rect, .. } => *rect,
            Item::Rule {
                x0,
                y0,
                x1,
                y1,
                rule,
            } => {
                Bounds::from_corners(*x0, *y0, *x1, *y1).inflate(rule.width / 2.0, rule.width / 2.0)
            }
            Item::Drawing(d) => d.rect,
        }
    }
}

/// A comment balloon in the markup area right of the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Balloon {
    /// The comment (for a revision balloon: the paragraph of the change).
    pub comment: NodeId,
    /// A tracked change's balloon instead of a comment's.
    pub revision: Option<RevisionBalloon>,
    /// The anchor point in the text (page coordinates).
    pub anchor: (f32, f32),
    /// The balloon rectangle (page coordinates, right of the page).
    pub rect: Bounds,
}

/// What a revision balloon says (Show Markup ▸ Balloons).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionBalloon {
    /// Where the change starts in its paragraph.
    pub offset: usize,
    /// Deleted text, or a formatting change.
    pub deleted: bool,
    /// The deleted text (empty for formatting changes).
    pub text: String,
    /// Who made it.
    pub author: String,
}

/// One page.
#[derive(Debug, Clone, Default)]
pub struct Page {
    /// Section index.
    pub section: usize,
    /// Page number.
    pub number: i64,
    /// The number as displayed (section number format).
    pub number_text: String,
    /// Index of the page within its section (0 = first).
    pub index_in_section: usize,
    /// Page width.
    pub width: f32,
    /// Page height.
    pub height: f32,
    /// Position of the page in document coordinates (set by the view).
    pub origin: (f32, f32),
    /// The body area.
    pub body: Bounds,
    /// Column areas (x, width) of the body.
    pub columns: Vec<(f32, f32)>,
    /// Body content.
    pub items: Vec<Item>,
    /// Floating objects behind the text.
    pub behind: Vec<Item>,
    /// Floating objects beside or in front of the text.
    pub front: Vec<Item>,
    /// The header.
    pub header: Vec<Item>,
    /// The footer.
    pub footer: Vec<Item>,
    /// Footnotes (with their separator).
    pub notes: Vec<Item>,
    /// Comment balloons.
    pub balloons: Vec<Balloon>,
    /// Text flows around these.
    pub obstacles: Vec<Obstacle>,
    /// A blank page inserted by an odd/even section break.
    pub blank: bool,
    /// Separator lines between columns: (x, top, bottom).
    pub column_rules: Vec<(f32, f32, f32)>,
    /// Vertical offset of the body items (page vertical alignment: centre, bottom).
    pub body_dy: f32,
}

impl Page {
    /// The page rectangle in document coordinates.
    #[must_use]
    pub fn rect(&self) -> Bounds {
        Bounds::new(self.origin.0, self.origin.1, self.width, self.height)
    }

    /// Every line on the page in paint order (header, body, notes, footer).
    pub fn lines(&self) -> impl Iterator<Item = &LineItem> {
        self.header
            .iter()
            .chain(&self.behind)
            .chain(&self.items)
            .chain(&self.front)
            .chain(&self.notes)
            .chain(&self.footer)
            .filter_map(|i| match i {
                Item::Line(l) => Some(l),
                _ => None,
            })
    }

    /// Body lines only.
    pub fn body_lines(&self) -> impl Iterator<Item = &LineItem> {
        self.items.iter().filter_map(|i| match i {
            Item::Line(l) if l.story == StoryKind::Body => Some(l),
            _ => None,
        })
    }
}
