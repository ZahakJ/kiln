// SPDX-License-Identifier: AGPL-3.0-or-later
//! Themes: the `.kdeck-theme` format, the master and eleven standard layouts built from it, and
//! the twelve bundled themes (`assets/themes/`).
//!
//! A `.kdeck-theme` file is XML in the `urn:kiln:deck-theme:1` namespace that carries a complete
//! DrawingML theme (`a:theme`: colour, font and format schemes — written unchanged into a
//! `.pptx`) plus the master and layout definitions:
//!
//! ```xml
//! <kd:deckTheme id="kiln" name="Kiln" version="1" xmlns:kd="urn:kiln:deck-theme:1" xmlns:a="…">
//!   <kd:description>…</kd:description>
//!   <a:theme name="Kiln">…</a:theme>
//!   <kd:master dark="0">
//!     <kd:background><a:solidFill><a:schemeClr val="bg2"/></a:solidFill></kd:background>
//!     <kd:grid marginX="0.0625" marginTop="0.07" … />
//!     <kd:titleStyle><a:lvl1pPr …><a:defRPr …/></a:lvl1pPr></kd:titleStyle>
//!     <kd:bodyStyle>…</kd:bodyStyle> <kd:otherStyle>…</kd:otherStyle> <kd:footerStyle>…</kd:footerStyle>
//!     <kd:ph type="title"><a:bodyPr anchor="b"/></kd:ph>
//!     <kd:shape name="Rule" prst="rect" x="…" y="…" w="…" h="…"><a:solidFill>…</a:solidFill></kd:shape>
//!   </kd:master>
//!   <kd:layout type="title" dark="1" hideMaster="1">   (dark: 1, 0 or auto)
//!     <kd:background>…</kd:background>
//!     <kd:ph type="ctrTitle" x="…" y="…" w="…" h="…"><a:bodyPr …/><a:lstStyle>…</a:lstStyle></kd:ph>
//!     <kd:shape …/>
//!   </kd:layout>
//!   <kd:variant name="Cobalt" dark="0"><a:clrScheme …/><a:fontScheme …/>?<kd:background>?</kd:variant>
//! </kd:deckTheme>
//! ```
//!
//! Positions are fractions of the slide width (`x`, `w`) and height (`y`, `h`), so one theme
//! serves 16:9, 4:3 and custom sizes. Text styles are DrawingML list styles laid over Kiln's
//! defaults (PowerPoint's indents, sizes and spacing), so a theme states only what it changes.
//! `<kd:ph type="…" n="2">` addresses the second placeholder of a type in a layout. An empty
//! `a:fmtScheme` takes the Office format scheme.

mod bundled;
pub mod layouts;

pub use bundled::{BUNDLED, BundledTheme, bundled, bundled_theme};

use kiln_common::NodeId;
use kiln_doc::st::ThemeFontRef;
use kiln_doc::{Indent, Paragraph, ParagraphProps, RunFonts, RunProps};
use kiln_ooxml::drawingml::color::{ColorContext, DrawingColor, Rgba, resolve_color};
use kiln_ooxml::drawingml::effects::{EffectList, parse_effect_list};
use kiln_ooxml::drawingml::fill::{FILL_ELEMENTS, Fill, parse_fill};
use kiln_ooxml::drawingml::geometry::parse_geometry;
use kiln_ooxml::drawingml::line::{Line, parse_line};
use kiln_ooxml::drawingml::theme::{
    ColorScheme, FontScheme, Theme, office_theme, parse_theme, theme_color_context,
};
use kiln_ooxml::xml::{XmlElement, parse_xml_str};
use std::collections::BTreeMap;

use crate::dml::text::{parse_body_props, parse_list_style};
use crate::error::{DeckError, Result};
use crate::geom::{Rect, Transform};
use crate::presentation::Presentation;
use crate::shape::{
    AutoShape, Geometry, Placeholder, PlaceholderType, Shape, ShapeKind, ShapeProps,
};
use crate::slide::{
    Background, ColorMapping, HeaderFooter, Layout, LayoutKind, Master, NotesMaster, SlideSize,
};
use crate::text::{
    Autofit, BodyProps, Bullet, BulletFont, DeckParaProps, LEVELS, LevelStyle, ListStyle,
    TextAnchor, TextBody, TextSpacing,
};
use layouts::{Frame, Grid, PlaceholderSpec, footer_specs, layout_specs};

/// The theme file namespace.
pub const NAMESPACE: &str = "urn:kiln:deck-theme:1";
/// The id of the default theme.
pub const DEFAULT_THEME: &str = "kiln";

/// A rectangle in fractions of the slide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FracRect {
    /// Left, fraction of the width.
    pub x: f64,
    /// Top, fraction of the height.
    pub y: f64,
    /// Width, fraction of the width.
    pub w: f64,
    /// Height, fraction of the height.
    pub h: f64,
}

impl FracRect {
    fn to_rect(self, size: &SlideSize) -> Rect {
        let f = |v: f64, t: i64| crate::geom::round_emu(v * t as f64);
        Rect::new(
            f(self.x, size.cx),
            f(self.y, size.cy),
            f(self.w, size.cx),
            f(self.h, size.cy),
        )
    }
}

/// A placeholder override: position, body properties, list style, prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceholderDef {
    /// The type it applies to.
    pub kind: PlaceholderType,
    /// The `n`-th of that type (1-based).
    pub ordinal: usize,
    /// New position.
    pub rect: Option<FracRect>,
    /// Body properties.
    pub body: BodyProps,
    /// List style.
    pub list_style: ListStyle,
    /// Prompt text.
    pub prompt: Option<String>,
}

/// A decorative shape on a master or layout.
#[derive(Debug, Clone, PartialEq)]
pub struct DecorDef {
    /// Its name.
    pub name: String,
    /// Where.
    pub rect: FracRect,
    /// Rotation, 60 000ths of a degree.
    pub rot: i32,
    /// Mirrored horizontally.
    pub flip_h: bool,
    /// Mirrored vertically.
    pub flip_v: bool,
    /// Geometry.
    pub geometry: Geometry,
    /// Fill (none when absent).
    pub fill: Option<Fill>,
    /// Outline (none when absent).
    pub line: Option<Line>,
    /// Effects.
    pub effects: Option<EffectList>,
}

/// The master definition.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterDef {
    /// Background.
    pub background: Background,
    /// Light text on a dark background.
    pub dark: bool,
    /// Layout grid.
    pub grid: Grid,
    /// `p:titleStyle` (over Kiln's defaults).
    pub title_style: ListStyle,
    /// `p:bodyStyle`.
    pub body_style: ListStyle,
    /// `p:otherStyle`.
    pub other_style: ListStyle,
    /// The date, footer and number placeholders' list style.
    pub footer_style: ListStyle,
    /// Overrides of the master's placeholders.
    pub placeholders: Vec<PlaceholderDef>,
    /// Decoration.
    pub shapes: Vec<DecorDef>,
}

/// How a layout maps its text colours (`kd:layout/@dark`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutTone {
    /// Dark text on a light background (`dark="0"`).
    Light,
    /// Light text on a dark background (`dark="1"`).
    Dark,
    /// Whichever of the two reads better on the layout's background in the colours in use
    /// (`dark="auto"`): an accent-filled title slide carries light text on a deep accent and
    /// dark text when a dark variant lightens the accent.
    Auto,
}

/// A layout definition (overrides of a standard layout).
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutDef {
    /// Which layout.
    pub kind: LayoutKind,
    /// Light or dark text (`Some`), or the master's mapping (`None`).
    pub dark: Option<LayoutTone>,
    /// Hide the master's decoration.
    pub hide_master: bool,
    /// Background.
    pub background: Option<Background>,
    /// Placeholder overrides.
    pub placeholders: Vec<PlaceholderDef>,
    /// Decoration.
    pub shapes: Vec<DecorDef>,
}

/// A variant (the Design tab's Variants gallery): another colour scheme, font scheme or
/// background.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    /// The name.
    pub name: String,
    /// Colours.
    pub colors: Option<ColorScheme>,
    /// Fonts.
    pub fonts: Option<FontScheme>,
    /// Dark mapping.
    pub dark: Option<bool>,
    /// Background.
    pub background: Option<Background>,
}

/// A parsed `.kdeck-theme`.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeFile {
    /// Stable id (`kiln`, `tenmoku`, …).
    pub id: String,
    /// Display name.
    pub name: String,
    /// One sentence for the gallery tooltip.
    pub description: String,
    /// The DrawingML theme.
    pub theme: Theme,
    /// The master.
    pub master: MasterDef,
    /// Layout overrides.
    pub layouts: Vec<LayoutDef>,
    /// Variants.
    pub variants: Vec<Variant>,
}

fn theme_err(m: impl Into<String>) -> DeckError {
    DeckError::Theme(m.into())
}

fn frac(e: &XmlElement, name: &str) -> Result<Option<f64>> {
    match e.attr(name) {
        None => Ok(None),
        Some(v) => v
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite() && (-1.0..=2.0).contains(f))
            .map(Some)
            .ok_or_else(|| theme_err(format!("{}/@{name}: '{v}' is not a fraction", e.name))),
    }
}

fn frac_rect(e: &XmlElement) -> Result<Option<FracRect>> {
    match (frac(e, "x")?, frac(e, "y")?, frac(e, "w")?, frac(e, "h")?) {
        (Some(x), Some(y), Some(w), Some(h)) => Ok(Some(FracRect { x, y, w, h })),
        (None, None, None, None) => Ok(None),
        _ => Err(theme_err(format!("{} needs all of x, y, w and h", e.name))),
    }
}

fn parse_background(e: &XmlElement) -> Result<Background> {
    if let Some(r) = e.child("kd:bgRef") {
        return Ok(Background::Theme {
            idx: r
                .attr_int("idx")
                .and_then(|i| u32::try_from(i).ok())
                .unwrap_or(1001),
            color: kiln_ooxml::drawingml::color::parse_color_child(Some(r)),
        });
    }
    let fill = e
        .elements()
        .find_map(parse_fill)
        .ok_or_else(|| theme_err("kd:background needs a fill"))?;
    Ok(Background::Fill {
        fill,
        shade_to_title: false,
    })
}

fn parse_ph(e: &XmlElement) -> Result<PlaceholderDef> {
    let t = e.attr("type").unwrap_or("obj");
    let kind = PlaceholderType::parse(t)
        .ok_or_else(|| theme_err(format!("kd:ph: unknown placeholder type '{t}'")))?;
    Ok(PlaceholderDef {
        kind,
        ordinal: e
            .attr_int("n")
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(1)
            .max(1),
        rect: frac_rect(e)?,
        body: e
            .child("a:bodyPr")
            .map(parse_body_props)
            .unwrap_or_default(),
        list_style: e
            .child("a:lstStyle")
            .map(|l| parse_list_style(l, None))
            .unwrap_or_default(),
        prompt: e.child("kd:prompt").map(XmlElement::text),
    })
}

fn parse_decor(e: &XmlElement) -> Result<DecorDef> {
    let rect = frac_rect(e)?.ok_or_else(|| theme_err("kd:shape needs x, y, w and h"))?;
    let geometry = if let Some(c) = e.child("a:custGeom") {
        Geometry::Custom(parse_geometry(c))
    } else {
        let prst = e.attr("prst").unwrap_or("rect");
        if !kiln_ooxml::drawingml::presets::is_preset_shape_type(prst) {
            return Err(theme_err(format!("kd:shape: unknown preset '{prst}'")));
        }
        let adjust = e
            .attr("adj")
            .map(|a| {
                a.split(';')
                    .filter_map(|kv| {
                        let (k, v) = kv.split_once('=')?;
                        Some((k.trim().to_owned(), v.trim().parse::<i64>().ok()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Geometry::Preset {
            name: prst.to_owned(),
            adjust,
        }
    };
    Ok(DecorDef {
        name: e.attr("name").unwrap_or("Decoration").to_owned(),
        rect,
        rot: e
            .attr_int("rot")
            .and_then(|r| i32::try_from(r).ok())
            .unwrap_or(0),
        flip_h: e.attr_bool("flipH").unwrap_or(false),
        flip_v: e.attr_bool("flipV").unwrap_or(false),
        geometry,
        fill: e
            .elements()
            .find(|c| FILL_ELEMENTS.contains(&c.name.as_str()))
            .and_then(parse_fill),
        line: e.child("a:ln").map(parse_line),
        effects: e.child("a:effectLst").map(parse_effect_list),
    })
}

fn parse_style(e: Option<&XmlElement>) -> ListStyle {
    e.map(|s| parse_list_style(s, None)).unwrap_or_default()
}

/// Wraps scheme elements into a theme so kiln-ooxml parses them.
fn parse_schemes(
    colors: Option<&XmlElement>,
    fonts: Option<&XmlElement>,
) -> Result<(Option<ColorScheme>, Option<FontScheme>)> {
    let elements = XmlElement::new("a:themeElements")
        .with_opt_child(colors.cloned())
        .with_opt_child(fonts.cloned());
    let theme = parse_theme(&XmlElement::new("a:theme").with_child(elements))
        .map_err(|e| theme_err(e.to_string()))?;
    Ok((
        colors.map(|_| theme.color_scheme),
        fonts.map(|_| theme.font_scheme),
    ))
}

impl ThemeFile {
    /// Parses a `.kdeck-theme` document.
    ///
    /// # Errors
    /// [`DeckError::Theme`] for malformed XML, another root, a missing theme or master, an
    /// unknown layout or placeholder type, or a bad position.
    pub fn parse(xml: &str) -> Result<Self> {
        let doc = parse_xml_str(xml).map_err(|e| theme_err(e.to_string()))?;
        let root = &doc.root;
        if root.name != "kd:deckTheme" {
            return Err(theme_err(format!(
                "expected kd:deckTheme, found {}",
                root.name
            )));
        }
        if root.attr("version").is_some_and(|v| v != "1") {
            return Err(theme_err("unsupported theme file version"));
        }
        let id = root
            .attr("id")
            .filter(|i| !i.is_empty())
            .ok_or_else(|| theme_err("kd:deckTheme needs an id"))?
            .to_owned();
        let mut theme = parse_theme(
            root.child("a:theme")
                .ok_or_else(|| theme_err("missing a:theme"))?,
        )
        .map_err(|e| theme_err(e.to_string()))?;
        crate::dml::serde_xml::normalize_theme(&mut theme);
        if theme.format_scheme.fill_styles.is_empty() {
            let name = theme.format_scheme.name.clone();
            theme.format_scheme = office_theme().format_scheme;
            if !name.is_empty() {
                theme.format_scheme.name = name;
            }
        }
        let m = root
            .child("kd:master")
            .ok_or_else(|| theme_err("missing kd:master"))?;
        let mut grid = Grid::default();
        if let Some(g) = m.child("kd:grid") {
            let set = |slot: &mut f64, n: &str| -> Result<()> {
                if let Some(v) = frac(g, n)? {
                    *slot = v;
                }
                Ok(())
            };
            set(&mut grid.margin_x, "marginX")?;
            set(&mut grid.margin_top, "marginTop")?;
            set(&mut grid.margin_bottom, "marginBottom")?;
            set(&mut grid.title_height, "titleHeight")?;
            set(&mut grid.gap, "gap")?;
            set(&mut grid.footer_height, "footerHeight")?;
            set(&mut grid.gutter, "gutter")?;
        }
        let master = MasterDef {
            background: m
                .child("kd:background")
                .map(parse_background)
                .transpose()?
                .unwrap_or_else(|| Background::solid(DrawingColor::scheme("bg1"))),
            dark: m.attr_bool("dark").unwrap_or(false),
            grid,
            title_style: parse_style(m.child("kd:titleStyle")),
            body_style: parse_style(m.child("kd:bodyStyle")),
            other_style: parse_style(m.child("kd:otherStyle")),
            footer_style: parse_style(m.child("kd:footerStyle")),
            placeholders: m
                .children_named("kd:ph")
                .map(parse_ph)
                .collect::<Result<_>>()?,
            shapes: m
                .children_named("kd:shape")
                .map(parse_decor)
                .collect::<Result<_>>()?,
        };
        let mut layouts = Vec::new();
        for l in root.children_named("kd:layout") {
            let kind = LayoutKind::from_ooxml(l.attr("type"));
            if !LayoutKind::STANDARD.contains(&kind) {
                return Err(theme_err(format!(
                    "kd:layout: '{}' is not a standard layout",
                    kind.as_ooxml()
                )));
            }
            layouts.push(LayoutDef {
                kind,
                dark: if l.attr("dark").is_some_and(|v| v.trim() == "auto") {
                    Some(LayoutTone::Auto)
                } else {
                    l.attr_bool("dark").map(|d| {
                        if d {
                            LayoutTone::Dark
                        } else {
                            LayoutTone::Light
                        }
                    })
                },
                hide_master: l.attr_bool("hideMaster").unwrap_or(false),
                background: l.child("kd:background").map(parse_background).transpose()?,
                placeholders: l
                    .children_named("kd:ph")
                    .map(parse_ph)
                    .collect::<Result<_>>()?,
                shapes: l
                    .children_named("kd:shape")
                    .map(parse_decor)
                    .collect::<Result<_>>()?,
            });
        }
        let mut variants = Vec::new();
        for v in root.children_named("kd:variant") {
            let (colors, fonts) = parse_schemes(v.child("a:clrScheme"), v.child("a:fontScheme"))?;
            variants.push(Variant {
                name: v.attr("name").unwrap_or("Variant").to_owned(),
                colors,
                fonts,
                dark: v.attr_bool("dark"),
                background: v.child("kd:background").map(parse_background).transpose()?,
            });
        }
        Ok(Self {
            name: root
                .attr("name")
                .map_or_else(|| theme.name.clone(), str::to_owned),
            description: root
                .child("kd:description")
                .map(XmlElement::text)
                .unwrap_or_default()
                .trim()
                .to_owned(),
            id,
            theme,
            master,
            layouts,
            variants,
        })
    }

    /// A minimal theme from code (the Office theme with Kiln's default styles), used only if a
    /// bundled file could not be read.
    #[must_use]
    pub fn fallback() -> Self {
        let mut theme = office_theme();
        theme.name = "Kiln Basic".to_owned();
        Self {
            id: "basic".to_owned(),
            name: "Kiln Basic".to_owned(),
            description: String::new(),
            theme,
            master: MasterDef {
                background: Background::Theme {
                    idx: 1001,
                    color: Some(DrawingColor::scheme("bg1")),
                },
                dark: false,
                grid: Grid::default(),
                title_style: ListStyle::default(),
                body_style: ListStyle::default(),
                other_style: ListStyle::default(),
                footer_style: ListStyle::default(),
                placeholders: Vec::new(),
                shapes: Vec::new(),
            },
            layouts: Vec::new(),
            variants: Vec::new(),
        }
    }

    /// The DrawingML theme with a variant applied (`None`: the theme as designed).
    #[must_use]
    pub fn variant_theme(&self, variant: Option<usize>) -> Theme {
        let mut t = self.theme.clone();
        if let Some(v) = variant.and_then(|i| self.variants.get(i)) {
            if let Some(c) = &v.colors {
                t.color_scheme = c.clone();
            }
            if let Some(f) = &v.fonts {
                t.font_scheme = f.clone();
            }
        }
        t
    }

    fn layout_def(&self, kind: &LayoutKind) -> Option<&LayoutDef> {
        self.layouts.iter().find(|l| &l.kind == kind)
    }

    /// The colour mapping a layout of this theme gets in a theme's colours (`None`: the
    /// master's). [`LayoutTone::Auto`] picks light text when the scheme's `lt1` contrasts more
    /// with the layout background than `dk1` does.
    #[must_use]
    pub fn layout_color_map(&self, kind: &LayoutKind, theme: &Theme) -> Option<ColorMapping> {
        let d = self.layout_def(kind)?;
        let dark = match d.dark? {
            LayoutTone::Light => false,
            LayoutTone::Dark => true,
            LayoutTone::Auto => {
                let ctx = theme_color_context(theme, ColorMapping::default().to_ooxml(), None);
                let bg = background_rgba(d.background.as_ref()?, &ctx)?;
                let lt1 = resolve_color(&DrawingColor::scheme("lt1"), &ctx);
                let dk1 = resolve_color(&DrawingColor::scheme("dk1"), &ctx);
                contrast(lt1, bg) >= contrast(dk1, bg)
            }
        };
        Some(if dark {
            ColorMapping::dark()
        } else {
            ColorMapping::default()
        })
    }

    /// Builds the master (with its theme, styles, placeholders, decoration and the eleven
    /// standard layouts) for a slide size, allocating ids from `pres`.
    #[must_use]
    pub fn build_master(
        &self,
        pres: &mut Presentation,
        size: &SlideSize,
        variant: Option<usize>,
    ) -> Master {
        let v = variant.and_then(|i| self.variants.get(i));
        let frame = Frame::new(&self.master.grid, size.cx, size.cy);
        let dark = v.and_then(|v| v.dark).unwrap_or(self.master.dark);
        let mut shapes: Vec<Shape> = self
            .master
            .shapes
            .iter()
            .map(|d| decor_shape(pres.alloc_id(), d, size))
            .collect();
        // The master's placeholders: title, body, date, footer, number.
        let mut master_specs = vec![
            PlaceholderSpec {
                ph: Placeholder::new(PlaceholderType::Title, 0),
                rect: frame.title(),
                prompt: layouts::TITLE_PROMPT,
                ordinal: 1,
            },
            PlaceholderSpec {
                ph: Placeholder::new(PlaceholderType::Body, 1),
                rect: frame.body(),
                prompt: layouts::BODY_PROMPT,
                ordinal: 1,
            },
        ];
        let mut footers = footer_specs(&frame);
        for (f, idx) in footers.iter_mut().zip([2, 3, 4]) {
            f.ph.idx = idx;
        }
        master_specs.extend(footers);
        for spec in &master_specs {
            let mut s = placeholder_shape(pres.alloc_id(), spec, true);
            if let Some(t) = s.text_mut() {
                // PowerPoint's master: titles centred vertically, body text shrinks on overflow.
                match spec.ph.kind {
                    PlaceholderType::Title => {
                        t.body.anchor = Some(TextAnchor::Middle);
                        t.body.autofit = Some(Autofit::SHRINK);
                    }
                    PlaceholderType::Body => t.body.autofit = Some(Autofit::SHRINK),
                    _ => {}
                }
            }
            if spec.ph.kind.is_footer_area()
                && let Some(t) = s.text_mut()
            {
                t.list_style = self.master.footer_style.clone();
                t.body.anchor = Some(TextAnchor::Middle);
                if spec.ph.kind == PlaceholderType::SlideNumber {
                    *t = with_style(TextBody::from_text("‹#›"), t);
                }
            }
            apply_overrides(&mut s, spec, &self.master.placeholders, size);
            shapes.push(s);
        }
        let mut theme = self.variant_theme(variant);
        theme.name.clone_from(&self.theme.name);
        let mut layouts = Vec::new();
        for kind in LayoutKind::STANDARD {
            layouts.push(self.build_layout(pres, &kind, &frame, size, &theme));
        }
        Master {
            id: pres.alloc_id(),
            name: self.name.clone(),
            theme,
            color_map: if dark {
                ColorMapping::dark()
            } else {
                ColorMapping::default()
            },
            background: v
                .and_then(|v| v.background.clone())
                .unwrap_or_else(|| self.master.background.clone()),
            shapes,
            title_style: self.master.title_style.over(&default_title_style()),
            body_style: self.master.body_style.over(&default_body_style()),
            other_style: self.master.other_style.over(&default_other_style()),
            layouts,
            header_footer: HeaderFooter {
                slide_number: true,
                header: false,
                footer: true,
                date: true,
            },
            preserve: true,
        }
    }

    fn build_layout(
        &self,
        pres: &mut Presentation,
        kind: &LayoutKind,
        frame: &Frame,
        size: &SlideSize,
        theme: &Theme,
    ) -> Layout {
        let def = self.layout_def(kind);
        let mut layout = Layout::new(pres.alloc_id(), kind.clone());
        if let Some(d) = def {
            layout.shapes.extend(
                d.shapes
                    .iter()
                    .map(|s| decor_shape(pres.alloc_id(), s, size)),
            );
            layout.background.clone_from(&d.background);
            layout.color_map = self.layout_color_map(kind, theme);
            layout.show_master_shapes = !d.hide_master;
        }
        let overrides = def.map(|d| d.placeholders.as_slice()).unwrap_or_default();
        for spec in layout_specs(kind, frame).iter().chain(&footer_specs(frame)) {
            let mut s = placeholder_shape(pres.alloc_id(), spec, false);
            if let Some(t) = s.text_mut() {
                standard_layout_text(kind, spec, t);
            }
            apply_overrides(&mut s, spec, overrides, size);
            layout.shapes.push(s);
        }
        layout
    }

    /// The notes master (slide image above, notes body below).
    #[must_use]
    pub fn build_notes_master(&self, pres: &mut Presentation, notes: &SlideSize) -> NotesMaster {
        let w = notes.cx;
        let h = notes.cy;
        let img = Rect::new(w / 8, h / 12, w * 3 / 4, h * 3 / 8);
        let body = Rect::new(w / 10, h / 2, w * 4 / 5, h * 2 / 5);
        let mut shapes = Vec::new();
        let mut s = Shape::new(
            pres.alloc_id(),
            "Slide Image Placeholder",
            Some(Transform::from_rect(img)),
            ShapeKind::Geometry(AutoShape {
                text_box: false,
                props: ShapeProps {
                    geometry: Some(Geometry::preset("rect")),
                    fill: Some(Fill::None),
                    line: Some(Line {
                        width: Some(12_700),
                        fill: Some(Fill::Solid(DrawingColor::srgb("000000"))),
                        ..Line::default()
                    }),
                    ..ShapeProps::default()
                },
                text: None,
            }),
        );
        s.placeholder = Some(Placeholder::new(PlaceholderType::SlideImage, 2));
        shapes.push(s);
        let spec = PlaceholderSpec {
            ph: Placeholder::new(PlaceholderType::Body, 3),
            rect: body,
            prompt: layouts::BODY_PROMPT,
            ordinal: 1,
        };
        shapes.push(placeholder_shape(pres.alloc_id(), &spec, true));
        let mut notes_style = ListStyle::default();
        for i in 0..LEVELS {
            notes_style.set_level(
                i,
                LevelStyle {
                    para: ParagraphProps {
                        indent: Some(Indent {
                            left: Some(i32::try_from(i).unwrap_or(0) * 720),
                            ..Indent::default()
                        }),
                        ..ParagraphProps::default()
                    },
                    run: RunProps {
                        size: Some(24),
                        fonts: Some(theme_fonts(false)),
                        color: Some(crate::dml::color::drawing_to_doc_color(
                            &DrawingColor::scheme("tx1"),
                            None,
                        )),
                        ..RunProps::default()
                    },
                    deck: DeckParaProps::default(),
                },
            );
        }
        NotesMaster {
            id: pres.alloc_id(),
            shapes,
            background: Background::Theme {
                idx: 1001,
                color: Some(DrawingColor::scheme("bg1")),
            },
            notes_style,
            color_map: ColorMapping::default(),
        }
    }
}

/// The text treatment PowerPoint's own layouts give their placeholders (themes may override):
/// unbulleted subtitles, section text and captions, bold comparison headings, bottom-anchored
/// caption titles, a centred picture prompt.
fn standard_layout_text(kind: &LayoutKind, spec: &PlaceholderSpec, t: &mut TextBody) {
    use PlaceholderType as T;
    let first = |size: Option<u32>, bold: Option<bool>, align: Option<kiln_doc::st::Alignment>| {
        let mut l = LevelStyle {
            deck: DeckParaProps {
                bullet: Some(Bullet::None),
                ..DeckParaProps::default()
            },
            ..LevelStyle::default()
        };
        l.para.indent = Some(Indent {
            left: Some(0),
            first_line: Some(0),
            ..Indent::default()
        });
        l.para.alignment = align;
        l.run.size = size.map(|pt| pt * 2);
        l.run.bold = bold;
        l
    };
    let center = Some(kiln_doc::st::Alignment::Center);
    match (kind, spec.ph.kind) {
        (LayoutKind::Title, T::Subtitle) => {
            t.list_style.set_level(0, first(Some(24), None, center));
        }
        (LayoutKind::Title, T::CenteredTitle) => {
            t.body.anchor = Some(TextAnchor::Bottom);
            t.list_style.set_level(0, first(Some(60), None, center));
        }
        (LayoutKind::SectionHeader, T::Title) => {
            t.body.anchor = Some(TextAnchor::Bottom);
            t.list_style.set_level(0, first(Some(60), None, None));
        }
        (LayoutKind::SectionHeader, T::Body) => {
            let mut l = first(Some(24), None, None);
            l.run.color = Some(crate::dml::color::drawing_to_doc_color(
                &DrawingColor::scheme("tx1")
                    .with("lumMod", 75_000)
                    .with("lumOff", 25_000),
                None,
            ));
            t.list_style.set_level(0, l);
        }
        (LayoutKind::Comparison, T::Body) => {
            t.body.anchor = Some(TextAnchor::Bottom);
            t.list_style.set_level(0, first(Some(24), Some(true), None));
        }
        (LayoutKind::ContentCaption | LayoutKind::PictureCaption, T::Title) => {
            t.body.anchor = Some(TextAnchor::Bottom);
            t.list_style.set_level(0, first(Some(32), None, None));
        }
        (LayoutKind::ContentCaption | LayoutKind::PictureCaption, T::Body) => {
            t.list_style.set_level(0, first(Some(16), None, None));
        }
        (LayoutKind::PictureCaption, T::Picture) => {
            t.body.anchor = Some(TextAnchor::Middle);
            t.list_style.set_level(0, first(Some(32), None, center));
        }
        _ => {}
    }
}

fn with_style(mut t: TextBody, from: &TextBody) -> TextBody {
    t.body = from.body.clone();
    t.list_style = from.list_style.clone();
    t
}

/// A text body with one paragraph per line; with `levels`, line `i` is at list level `i`.
fn prompt_body(prompt: &str, levels: bool) -> TextBody {
    let mut body = TextBody::from_text(prompt);
    if levels {
        let paragraphs: Vec<Paragraph> = body
            .paragraphs()
            .enumerate()
            .map(|(i, p)| {
                let mut p = p.clone();
                if i > 0 {
                    p.props.outline_level = u8::try_from(i.min(LEVELS - 1)).ok();
                }
                p
            })
            .collect();
        if let Ok(b) = TextBody::from_paragraphs(paragraphs, BTreeMap::new()) {
            body = b;
        }
    }
    body
}

fn placeholder_name(kind: PlaceholderType) -> &'static str {
    match kind {
        PlaceholderType::Title | PlaceholderType::CenteredTitle => "Title",
        PlaceholderType::Subtitle => "Subtitle",
        PlaceholderType::Date => "Date Placeholder",
        PlaceholderType::Footer => "Footer Placeholder",
        PlaceholderType::SlideNumber => "Slide Number Placeholder",
        PlaceholderType::Header => "Header Placeholder",
        PlaceholderType::Picture => "Picture Placeholder",
        PlaceholderType::Chart => "Chart Placeholder",
        PlaceholderType::Table => "Table Placeholder",
        PlaceholderType::Media => "Media Placeholder",
        PlaceholderType::SlideImage => "Slide Image Placeholder",
        PlaceholderType::Body => "Text Placeholder",
        _ => "Content Placeholder",
    }
}

/// A placeholder shape on a master or layout: explicit position, prompt text, no fill.
fn placeholder_shape(id: NodeId, spec: &PlaceholderSpec, master: bool) -> Shape {
    let levels = spec.prompt.contains('\n');
    let mut text = prompt_body(spec.prompt, levels);
    if spec.ph.vertical {
        text.body.direction = Some(crate::text::TextDirection::EastAsianVertical);
    }
    let mut s = Shape::new(
        id,
        format!("{} {}", placeholder_name(spec.ph.kind), id.0),
        Some(Transform::from_rect(spec.rect)),
        ShapeKind::Geometry(AutoShape {
            text_box: false,
            props: if master {
                ShapeProps {
                    geometry: Some(Geometry::preset("rect")),
                    ..ShapeProps::default()
                }
            } else {
                ShapeProps::default()
            },
            text: Some(text),
        }),
    );
    s.placeholder = Some(spec.ph);
    s
}

fn apply_overrides(
    s: &mut Shape,
    spec: &PlaceholderSpec,
    defs: &[PlaceholderDef],
    size: &SlideSize,
) {
    let Some(d) = defs
        .iter()
        .find(|d| d.kind == spec.ph.kind && d.ordinal == spec.ordinal)
    else {
        return;
    };
    if let Some(r) = d.rect {
        s.xfrm = Some(Transform::from_rect(r.to_rect(size)));
    }
    if let Some(prompt) = &d.prompt {
        let old = s.text().cloned().unwrap_or_default();
        if let Some(t) = s.text_mut() {
            *t = with_style(prompt_body(prompt, prompt.contains('\n')), &old);
        }
    }
    if let Some(t) = s.text_mut() {
        t.body = d.body.over(&t.body);
        t.list_style = d.list_style.over(&t.list_style);
    }
}

fn decor_shape(id: NodeId, d: &DecorDef, size: &SlideSize) -> Shape {
    let mut x = Transform::from_rect(d.rect.to_rect(size)).with_rotation(d.rot);
    x.flip_h = d.flip_h;
    x.flip_v = d.flip_v;
    Shape::new(
        id,
        d.name.clone(),
        Some(x),
        ShapeKind::Geometry(AutoShape {
            text_box: false,
            props: ShapeProps {
                geometry: Some(d.geometry.clone()),
                fill: Some(d.fill.clone().unwrap_or(Fill::None)),
                line: Some(d.line.clone().unwrap_or(Line {
                    fill: Some(Fill::None),
                    ..Line::default()
                })),
                effects: d.effects.clone(),
                style: None,
            },
            text: None,
        }),
    )
}

// ------------------------------------------------------------------ default styles

fn theme_fonts(major: bool) -> RunFonts {
    let (l, e, c) = if major {
        (
            ThemeFontRef::MajorHAnsi,
            ThemeFontRef::MajorEastAsia,
            ThemeFontRef::MajorBidi,
        )
    } else {
        (
            ThemeFontRef::MinorHAnsi,
            ThemeFontRef::MinorEastAsia,
            ThemeFontRef::MinorBidi,
        )
    };
    RunFonts {
        ascii_theme: Some(l),
        h_ansi_theme: Some(l),
        east_asia_theme: Some(e),
        cs_theme: Some(c),
        ..RunFonts::default()
    }
}

fn text_color() -> kiln_doc::DocColor {
    crate::dml::color::drawing_to_doc_color(&DrawingColor::scheme("tx1"), None)
}

fn level(
    mar_l: i64,
    indent: i64,
    size_pt: u32,
    before_pts: i32,
    bullet: Option<&str>,
    major: bool,
) -> LevelStyle {
    let tw = |emu: i64| i32::try_from(emu / 635).unwrap_or(0);
    LevelStyle {
        para: ParagraphProps {
            alignment: Some(kiln_doc::st::Alignment::Left),
            indent: Some(Indent {
                left: Some(tw(mar_l)),
                hanging: (indent < 0).then(|| tw(-indent)),
                first_line: (indent >= 0).then_some(0),
                ..Indent::default()
            }),
            ..ParagraphProps::default()
        },
        deck: DeckParaProps {
            bullet: Some(match bullet {
                Some(c) => Bullet::Char(c.to_owned()),
                None => Bullet::None,
            }),
            bullet_font: bullet.map(|_| BulletFont::Font("Arial".to_owned())),
            line_spacing: Some(TextSpacing::Percent(90_000)),
            space_before: Some(TextSpacing::Points(before_pts)),
            default_tab: Some(914_400),
            east_asian_line_break: Some(true),
            hanging_punctuation: Some(true),
            ..DeckParaProps::default()
        },
        run: RunProps {
            size: Some(size_pt * 2),
            kern: Some(24),
            fonts: Some(theme_fonts(major)),
            color: Some(text_color()),
            ..RunProps::default()
        },
    }
}

/// Kiln's default title style (PowerPoint's: 44 pt heading font, 90 % line spacing).
#[must_use]
pub fn default_title_style() -> ListStyle {
    let mut s = ListStyle::default();
    s.set_level(0, level(0, 0, 44, 0, None, true));
    s
}

/// Kiln's default body style (PowerPoint's indents, sizes 28/24/20/18 pt, bullets).
#[must_use]
pub fn default_body_style() -> ListStyle {
    let mut s = ListStyle::default();
    let sizes = [28, 24, 20, 18, 18, 18, 18, 18, 18];
    for (i, size) in sizes.into_iter().enumerate() {
        let i64i = i64::try_from(i).unwrap_or(0);
        let before = if i == 0 { 1000 } else { 500 };
        s.set_level(
            i,
            level(
                228_600 + 457_200 * i64i,
                -228_600,
                size,
                before,
                Some("•"),
                false,
            ),
        );
    }
    s
}

/// Kiln's default other style (text boxes and shapes: 18 pt, no bullets).
#[must_use]
pub fn default_other_style() -> ListStyle {
    let mut s = ListStyle::default();
    for i in 0..LEVELS {
        let mut l = level(
            457_200 * i64::try_from(i).unwrap_or(0),
            0,
            18,
            0,
            None,
            false,
        );
        l.deck.line_spacing = Some(TextSpacing::Percent(100_000));
        s.set_level(i, l);
    }
    s
}

impl Presentation {
    /// A new presentation in Kiln's default theme: 16:9, one Title Slide.
    #[must_use]
    pub fn new() -> Self {
        let theme = bundled_theme(DEFAULT_THEME).unwrap_or_else(ThemeFile::fallback);
        let mut p = Self::with_theme(&theme, SlideSize::WIDESCREEN, None);
        let layout = p.masters[0]
            .layout_of_kind(&LayoutKind::Title)
            .map_or(p.masters[0].layouts[0].id, |l| l.id);
        let slide = crate::edit::new_slide(&mut p, layout);
        p.slides.push(slide);
        p
    }

    /// A presentation with one master built from a theme and no slides.
    #[must_use]
    pub fn with_theme(theme: &ThemeFile, size: SlideSize, variant: Option<usize>) -> Self {
        let mut p = Self::empty();
        p.slide_size = size.clone();
        let master = theme.build_master(&mut p, &size, variant);
        let notes = theme.build_notes_master(&mut p, &SlideSize::NOTES);
        p.masters.push(master);
        p.notes_master = Some(notes);
        p.default_text_style = default_other_style();
        p
    }
}

impl Default for Presentation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;

/// WCAG relative luminance of a resolved colour (channels 0–255).
fn luminance(c: Rgba) -> f64 {
    let lin = |v: f64| {
        let v = (v / 255.0).clamp(0.0, 1.0);
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio.
fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The colour a background shows on average: a solid fill exactly, a gradient by the mean of
/// its stops. `None` for pictures, patterns and theme styles.
fn background_rgba(bg: &Background, ctx: &ColorContext) -> Option<Rgba> {
    let Background::Fill { fill, .. } = bg else {
        return None;
    };
    match fill {
        Fill::Solid(c) => Some(resolve_color(c, ctx)),
        Fill::Gradient(g) if !g.stops.is_empty() => {
            let n = g.stops.len() as f64;
            let (r, gr, b) = g.stops.iter().fold((0.0, 0.0, 0.0), |a, s| {
                let c = resolve_color(&s.color, ctx);
                (a.0 + c.r, a.1 + c.g, a.2 + c.b)
            });
            Some(Rgba::rgb(r / n, gr / n, b / n))
        }
        _ => None,
    }
}
