// SPDX-License-Identifier: AGPL-3.0-or-later
//! Placeholder inheritance, resolved on access: a slide placeholder inherits position, size,
//! shape properties, body properties and list styles from the matching layout placeholder,
//! which inherits from the matching master placeholder; text styles bottom out in the master's
//! title, body or other style and the presentation's default text style. Backgrounds and colour
//! maps inherit slide → layout → master.
//!
//! **Matching (as PowerPoint):** a slide placeholder matches the layout placeholder with the
//! same `idx` (titles match the layout's title whatever their index); failing that, one of the
//! same type. A layout placeholder matches the master placeholder of its master type: titles the
//! title, date/footer/number their own type, everything else the body.

use kiln_common::NodeId;
use kiln_doc::merge::{merge_paragraph_props, merge_run_props};
use kiln_doc::st::ThemeFontRef;
use kiln_doc::{Paragraph, ParagraphProps, RunFonts, RunProps};
use kiln_ooxml::drawingml::color::ColorContext;
use kiln_ooxml::drawingml::theme::{Theme, theme_color_context};

use crate::dml::color::drawing_to_doc_color;
use crate::geom::Transform;
use crate::presentation::{Owner, Presentation};
use crate::shape::{FontCollectionRef, Placeholder, PlaceholderType, Shape, ShapeProps};
use crate::slide::{Background, ColorMapping, Layout, Master};
use crate::text::{BodyProps, DeckParaProps, LevelStyle, ListStyle, ResolvedBodyProps, TextBody};

/// The layout placeholder a slide placeholder inherits from.
#[must_use]
pub fn match_layout_placeholder<'a>(layout: &'a [Shape], ph: &Placeholder) -> Option<&'a Shape> {
    let phs = || {
        layout
            .iter()
            .filter_map(|s| s.placeholder.as_ref().map(|p| (s, p)))
    };
    if ph.kind.is_title() {
        return phs().find(|(_, p)| p.kind.is_title()).map(|(s, _)| s);
    }
    phs()
        .find(|(_, p)| p.idx == ph.idx && !p.kind.is_title())
        .or_else(|| phs().find(|(_, p)| p.kind == ph.kind))
        .map(|(s, _)| s)
}

/// The master placeholder a layout placeholder inherits from.
#[must_use]
pub fn match_master_placeholder<'a>(master: &'a [Shape], ph: &Placeholder) -> Option<&'a Shape> {
    let want = ph.kind.master_type();
    master
        .iter()
        .find(|s| s.placeholder.is_some_and(|p| p.kind.master_type() == want))
}

/// Which master text style a shape's text uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextStyleKind {
    /// `p:titleStyle` (title placeholders).
    Title,
    /// `p:bodyStyle` (body, subtitle and content placeholders).
    Body,
    /// `p:otherStyle` (everything else).
    Other,
}

impl TextStyleKind {
    /// The style a shape uses.
    #[must_use]
    pub fn of(shape: &Shape) -> Self {
        match shape.placeholder.map(|p| p.kind) {
            Some(k) if k.is_title() => Self::Title,
            Some(
                PlaceholderType::Date
                | PlaceholderType::Footer
                | PlaceholderType::SlideNumber
                | PlaceholderType::Header
                | PlaceholderType::SlideImage,
            )
            | None => Self::Other,
            Some(_) => Self::Body,
        }
    }
}

/// A shape and the placeholders it inherits from, nearest first.
#[derive(Debug, Clone, Copy)]
pub struct Chain<'a> {
    /// The shape itself.
    pub shape: &'a Shape,
    /// The layout placeholder (for slide shapes).
    pub layout: Option<&'a Shape>,
    /// The master placeholder (for slide and layout shapes).
    pub master: Option<&'a Shape>,
    /// The master that supplies the text styles and theme.
    pub master_slide: &'a Master,
}

impl<'a> Chain<'a> {
    /// The inherited shapes, nearest first, excluding the shape.
    pub fn ancestors(&self) -> impl Iterator<Item = &'a Shape> {
        self.layout.into_iter().chain(self.master)
    }

    /// The shape and its ancestors, nearest first.
    pub fn all(&self) -> impl Iterator<Item = &'a Shape> {
        std::iter::once(self.shape).chain(self.ancestors())
    }
}

/// Text formatting resolved for one paragraph level, with everything the paragraph itself does
/// not set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedParagraph {
    /// Paragraph properties (alignment, indents, tabs, direction, level).
    pub para: ParagraphProps,
    /// Spacing and bullets.
    pub deck: DeckParaProps,
    /// The run properties a run without direct formatting gets.
    pub run: RunProps,
}

impl Presentation {
    /// The master of an owner (the master itself, a layout's master, a slide's layout's master;
    /// the first master for the notes master).
    #[must_use]
    pub fn master_of(&self, owner: Owner) -> Option<&Master> {
        match owner {
            Owner::Master(id) => self.master(id),
            Owner::Layout(id) => self.layout(id).map(|(m, _)| m),
            Owner::Slide(id) => self.layout(self.slide(id)?.layout).map(|(m, _)| m),
            Owner::NotesMaster(_) => self.masters.first(),
        }
    }

    /// The layout of an owner (a slide's layout, or the layout itself).
    #[must_use]
    pub fn layout_of(&self, owner: Owner) -> Option<&Layout> {
        match owner {
            Owner::Layout(id) => self.layout(id).map(|(_, l)| l),
            Owner::Slide(id) => self.layout(self.slide(id)?.layout).map(|(_, l)| l),
            Owner::Master(_) | Owner::NotesMaster(_) => None,
        }
    }

    /// A shape's inheritance chain. Non-placeholders and shapes inside groups inherit nothing
    /// from layouts or masters.
    #[must_use]
    pub fn chain<'a>(&'a self, owner: Owner, shape: &'a Shape) -> Option<Chain<'a>> {
        let master = self.master_of(owner)?;
        let mut chain = Chain {
            shape,
            layout: None,
            master: None,
            master_slide: master,
        };
        let Some(ph) = shape.placeholder else {
            return Some(chain);
        };
        match owner {
            Owner::Slide(_) => {
                let layout = self.layout_of(owner)?;
                chain.layout = match_layout_placeholder(&layout.shapes, &ph);
                let via = chain.layout.and_then(|l| l.placeholder).unwrap_or(ph);
                chain.master = match_master_placeholder(&master.shapes, &via);
            }
            Owner::Layout(_) => chain.master = match_master_placeholder(&master.shapes, &ph),
            Owner::Master(_) | Owner::NotesMaster(_) => {}
        }
        Some(chain)
    }

    /// The chain of a shape by id (top-level shapes of slides, layouts and masters).
    #[must_use]
    pub fn chain_of(&self, shape: NodeId) -> Option<Chain<'_>> {
        let path = self.locate(shape)?;
        let s = self.shape_at(&path)?;
        if path.path.len() > 1 {
            // Inside a group: no placeholder inheritance.
            let mut c = self.chain(path.owner, s)?;
            c.layout = None;
            c.master = None;
            return Some(c);
        }
        self.chain(path.owner, s)
    }

    /// The effective transform: the shape's own, else the nearest placeholder's.
    #[must_use]
    pub fn resolved_xfrm(&self, shape: NodeId) -> Option<Transform> {
        let c = self.chain_of(shape)?;
        c.all().find_map(|s| s.xfrm)
    }

    /// The effective shape properties, property by property along the chain.
    #[must_use]
    pub fn resolved_props(&self, shape: NodeId) -> Option<ShapeProps> {
        let c = self.chain_of(shape)?;
        let mut out = ShapeProps::default();
        for s in c.all() {
            if let Some(p) = s.props() {
                out = out.over(p);
            }
        }
        Some(out)
    }

    /// The body properties of a shape's text, with inheritance, unresolved.
    #[must_use]
    pub fn inherited_body(&self, shape: NodeId) -> Option<BodyProps> {
        let c = self.chain_of(shape)?;
        let mut out = BodyProps::default();
        for s in c.all() {
            if let Some(t) = s.text() {
                out = out.over(&t.body);
            }
        }
        Some(out)
    }

    /// The body properties of a shape's text with every value decided.
    #[must_use]
    pub fn resolved_body(&self, shape: NodeId) -> Option<ResolvedBodyProps> {
        self.inherited_body(shape).map(|b| b.resolve())
    }

    /// The list-level defaults a paragraph at `level` of a shape's text starts from: the
    /// default text style, the master's title/body/other style, the shape style's font, the
    /// master and layout placeholders' list styles and the shape's own.
    #[must_use]
    pub fn level_style(&self, shape: NodeId, level: usize) -> Option<LevelStyle> {
        let c = self.chain_of(shape)?;
        Some(self.level_style_in(&c, level))
    }

    fn level_style_in(&self, c: &Chain<'_>, level: usize) -> LevelStyle {
        let m = c.master_slide;
        let mut out = self.default_text_style.level(level);
        let master_style: &ListStyle = match TextStyleKind::of(c.shape) {
            TextStyleKind::Title => &m.title_style,
            TextStyleKind::Body => &m.body_style,
            TextStyleKind::Other => &m.other_style,
        };
        out = master_style.level(level).over(&out);
        // A Quick Style's font reference (`p:style/a:fontRef`) sets the font and colour.
        if let Some(st) = c
            .all()
            .find_map(|s| s.props().and_then(|p| p.style.as_ref()))
        {
            let mut run = RunProps::default();
            let theme_font = match st.font {
                FontCollectionRef::Major => Some((
                    ThemeFontRef::MajorHAnsi,
                    ThemeFontRef::MajorEastAsia,
                    ThemeFontRef::MajorBidi,
                )),
                FontCollectionRef::Minor => Some((
                    ThemeFontRef::MinorHAnsi,
                    ThemeFontRef::MinorEastAsia,
                    ThemeFontRef::MinorBidi,
                )),
                FontCollectionRef::None => None,
            };
            if let Some((latin, ea, cs)) = theme_font {
                run.fonts = Some(RunFonts {
                    ascii_theme: Some(latin),
                    h_ansi_theme: Some(latin),
                    east_asia_theme: Some(ea),
                    cs_theme: Some(cs),
                    ..RunFonts::default()
                });
            }
            run.color = st
                .font_color
                .as_ref()
                .map(|c| drawing_to_doc_color(c, None));
            out.run = merge_run_props(&out.run, Some(&run));
        }
        let lists: Vec<&ListStyle> = c
            .all()
            .filter_map(|s| s.text().map(|t| &t.list_style))
            .collect();
        for l in lists.into_iter().rev() {
            out = l.level(level).over(&out);
        }
        out
    }

    /// The formatting a paragraph of a shape's text resolves to before its runs' direct
    /// formatting: level defaults, then the paragraph's own properties and DrawingML extras.
    #[must_use]
    pub fn resolved_paragraph(
        &self,
        shape: NodeId,
        body: &TextBody,
        para: &Paragraph,
    ) -> Option<ResolvedParagraph> {
        let c = self.chain_of(shape)?;
        let lvl = TextBody::level_of(para);
        let base = self.level_style_in(&c, lvl);
        let deck = body
            .deck_props(para.id)
            .map_or_else(|| base.deck.clone(), |d| d.over(&base.deck));
        Some(ResolvedParagraph {
            para: merge_paragraph_props(&base.para, Some(&para.props)),
            deck,
            run: base.run,
        })
    }

    /// The effective run properties at a char offset of a paragraph (paragraph defaults under
    /// the run's direct formatting). Past the end, the paragraph mark's properties apply.
    #[must_use]
    pub fn resolved_run(
        &self,
        shape: NodeId,
        body: &TextBody,
        para: &Paragraph,
        offset: usize,
    ) -> Option<RunProps> {
        let r = self.resolved_paragraph(shape, body, para)?;
        let direct = para.content.props_at(offset).unwrap_or(&para.mark_props);
        Some(merge_run_props(&r.run, Some(direct)))
    }

    /// The background a slide, layout or master shows.
    #[must_use]
    pub fn resolved_background(&self, owner: Owner) -> Option<&Background> {
        let master = self.master_of(owner)?;
        let from_layout = self.layout_of(owner).and_then(|l| l.background.as_ref());
        let from_slide = match owner {
            Owner::Slide(id) => self.slide(id)?.background.as_ref(),
            _ => None,
        };
        Some(from_slide.or(from_layout).unwrap_or(&master.background))
    }

    /// The colour map in force (slide override, layout override, master map).
    #[must_use]
    pub fn resolved_color_map(&self, owner: Owner) -> Option<ColorMapping> {
        let master = self.master_of(owner)?;
        let from_layout = self.layout_of(owner).and_then(|l| l.color_map);
        let from_slide = match owner {
            Owner::Slide(id) => self.slide(id)?.color_map,
            _ => None,
        };
        Some(from_slide.or(from_layout).unwrap_or(master.color_map))
    }

    /// The theme an owner uses.
    #[must_use]
    pub fn theme_of(&self, owner: Owner) -> Option<&Theme> {
        self.master_of(owner).map(|m| &m.theme)
    }

    /// The colour context (theme colours and colour map) for resolving DrawingML colours on an
    /// owner.
    #[must_use]
    pub fn color_context(&self, owner: Owner) -> Option<ColorContext> {
        let theme = self.theme_of(owner)?;
        let map = self.resolved_color_map(owner)?;
        Some(theme_color_context(theme, map.to_ooxml(), None))
    }

    /// Whether the master's (and layout's) decoration shows behind a slide.
    #[must_use]
    pub fn shows_master_shapes(&self, slide: NodeId) -> bool {
        let Some(s) = self.slide(slide) else {
            return false;
        };
        s.show_master_shapes
            && self
                .layout(s.layout)
                .is_none_or(|(_, l)| l.show_master_shapes)
    }

    /// The layout placeholders a new slide on this layout gets (everything but the footer area,
    /// in layout order).
    #[must_use]
    pub fn new_slide_placeholders(layout: &Layout) -> Vec<&Shape> {
        layout
            .shapes
            .iter()
            .filter(|s| s.placeholder.is_some_and(|p| !p.kind.is_footer_area()))
            .collect()
    }
}

#[cfg(test)]
mod tests;
