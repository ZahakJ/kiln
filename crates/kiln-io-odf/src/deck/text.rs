// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text in shapes: `text:p`, `text:list` in frames and custom shapes ⇄ [`TextBody`].
//!
//! - **Paragraphs** take their alignment, indents and spacing from the paragraph style
//!   (`fo:margin-top`/`bottom` become DrawingML spacing in points, `fo:line-height` a percent or
//!   points), their runs' formatting from text styles ([`crate::props`]).
//! - **Lists**: the nesting depth of `text:list` is the paragraph level; a list item's list style
//!   level gives the bullet (`text:bullet-char` with its font, colour and size, numbering with
//!   its format, prefix and suffix, picture bullets), `text:list-header` items have none.
//!   Indents of list paragraphs are the level's `text:space-before` and `text:min-label-width`,
//!   as LibreOffice writes them.
//! - **Inline content**: `text:s`, `text:tab`, `text:line-break`, links (`text:a`), slide
//!   numbers, dates and times, footers.
//!
//! Writing goes the other way and writes the formatting each paragraph and run resolves to
//! (level styles, placeholders and direct formatting), so every consumer shows the same text
//! whatever master styles it reads.

use std::collections::BTreeMap;

use kiln_common::NodeId;
use kiln_deck::text::{
    Bullet, BulletColor, BulletFont, BulletSize, DeckParaProps, TextBody, TextSpacing,
};
use kiln_doc::fields::Field;
use kiln_doc::inline::{
    Break, Fragment, Hyperlink, InlineObject, Inlines, LinkTarget, Mark, MarkSpan, RunSpan,
};
use kiln_doc::model::Paragraph;
use kiln_doc::props::ColorValue;
use kiln_doc::props::{DocColor, Indent, ParagraphProps, RunFonts, RunProps, Spacing};
use kiln_doc::st::{BreakKind, LineRule};
use kiln_doc::theme::DocTheme;
use kiln_ooxml::drawingml::color::{ColorContext, DrawingColor};
use kiln_ooxml::drawingml::theme::Theme;
use kiln_ooxml::xml::{XmlElement, XmlNode};

use crate::el::El;
use crate::props::{FontFaces, para_props_el, read_para_props, read_run_props, run_props_el};
use crate::text::styles::{Scope, StyleIndex};
use crate::units::{color, percent, twips, twips_str};
use crate::xml::{XmlOut, kid};

const EMU_PER_TWIP: i64 = 635;

/// Reads text bodies.
#[allow(missing_debug_implementations)]
pub struct TextReader<'a, 'b> {
    /// Styles.
    pub ix: &'b StyleIndex<'a>,
    /// Which automatic styles apply.
    pub scope: Scope,
    /// Font declarations.
    pub fonts: &'b FontFaces,
}

#[derive(Debug, Default)]
struct Para {
    text: String,
    runs: Vec<RunSpan>,
    marks: Vec<MarkSpan>,
    objects: Vec<InlineObject>,
    space: bool,
}

impl Para {
    fn run(&mut self, n: usize, p: &RunProps) {
        if n == 0 {
            return;
        }
        match self.runs.last_mut() {
            Some(r) if r.props == *p => r.len += n,
            _ => self.runs.push(RunSpan::new(n, p.clone())),
        }
    }

    fn text(&mut self, s: &str, p: &RunProps) {
        let mut n = 0;
        for c in s.chars() {
            if matches!(c, ' ' | '\t' | '\n' | '\r') {
                if self.space {
                    continue;
                }
                self.text.push(' ');
                self.space = true;
            } else if c == kiln_doc::OBJECT_CHAR {
                continue;
            } else {
                self.text.push(c);
                self.space = false;
            }
            n += 1;
        }
        self.run(n, p);
    }

    fn literal(&mut self, s: &str, p: &RunProps) {
        self.text.push_str(s);
        self.space = false;
        self.run(s.chars().count(), p);
    }

    fn object(&mut self, o: InlineObject, p: &RunProps) {
        self.text.push(kiln_doc::OBJECT_CHAR);
        self.objects.push(o);
        self.space = false;
        self.run(1, p);
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn finish(mut self, id: NodeId) -> Paragraph {
        // Trailing collapsible white space is dropped.
        if self.space && self.text.ends_with(' ') {
            self.text.pop();
            if let Some(r) = self.runs.last_mut() {
                r.len -= 1;
            }
            self.runs.retain(|r| r.len > 0);
        }
        let len = self.len();
        self.marks.retain(|m| m.start < m.end && m.end <= len);
        let fragment = Fragment {
            text: self.text,
            runs: self.runs,
            marks: self.marks,
            objects: self.objects,
        };
        let mut p = Paragraph::new(id);
        p.content = Inlines::from_fragment(fragment.clone()).unwrap_or_else(|_| {
            Inlines::from_fragment(Fragment::text(
                &fragment.text.replace(kiln_doc::OBJECT_CHAR, ""),
                RunProps::default(),
            ))
            .unwrap_or_default()
        });
        p
    }
}

/// A list level's bullet and indents.
#[derive(Debug, Clone, Default)]
struct LevelInfo {
    bullet: Option<Bullet>,
    color: Option<BulletColor>,
    size: Option<BulletSize>,
    font: Option<BulletFont>,
    space_before: Option<i32>,
    label_width: Option<i32>,
}

fn num_scheme(format: &str, prefix: &str, suffix: &str) -> Option<&'static str> {
    let base = match format {
        "1" => "arabic",
        "a" => "alphaLc",
        "A" => "alphaUc",
        "i" => "romanLc",
        "I" => "romanUc",
        _ => return None,
    };
    Some(match (base, prefix, suffix) {
        ("arabic", "", ".") => "arabicPeriod",
        ("arabic", "", ")") => "arabicParenR",
        ("arabic", "(", ")") => "arabicParenBoth",
        ("arabic", _, _) => "arabicPlain",
        ("alphaLc", "", ".") => "alphaLcPeriod",
        ("alphaLc", "(", ")") => "alphaLcParenBoth",
        ("alphaLc", _, _) => "alphaLcParenR",
        ("alphaUc", "", ".") => "alphaUcPeriod",
        ("alphaUc", "(", ")") => "alphaUcParenBoth",
        ("alphaUc", _, _) => "alphaUcParenR",
        ("romanLc", "", ".") => "romanLcPeriod",
        ("romanLc", "(", ")") => "romanLcParenBoth",
        ("romanLc", _, _) => "romanLcParenR",
        ("romanUc", "", ".") => "romanUcPeriod",
        ("romanUc", "(", ")") => "romanUcParenBoth",
        _ => "romanUcParenR",
    })
}

/// `(num-format, prefix, suffix)` of a DrawingML numbering scheme.
fn scheme_parts(scheme: &str) -> (&'static str, &'static str, &'static str) {
    let format = if scheme.starts_with("arabic") {
        "1"
    } else if scheme.starts_with("alphaLc") {
        "a"
    } else if scheme.starts_with("alphaUc") {
        "A"
    } else if scheme.starts_with("romanLc") {
        "i"
    } else if scheme.starts_with("romanUc") {
        "I"
    } else {
        "1"
    };
    let (prefix, suffix) = if scheme.ends_with("ParenBoth") {
        ("(", ")")
    } else if scheme.ends_with("ParenR") {
        ("", ")")
    } else if scheme.ends_with("Period") {
        ("", ".")
    } else {
        ("", "")
    };
    (format, prefix, suffix)
}

impl TextReader<'_, '_> {
    fn level_info(
        &self,
        list_style: Option<&str>,
        level: usize,
        media: &mut dyn FnMut(&str) -> Option<String>,
    ) -> LevelInfo {
        let Some(name) = list_style else {
            return LevelInfo::default();
        };
        let el = self
            .ix
            .list_style(self.scope, name)
            .and_then(|i| self.ix.list_styles.get(i))
            .map(|(_, _, e)| *e);
        let Some(el) = el else {
            return LevelInfo::default();
        };
        let want = (level + 1).to_string();
        let Some(lvl) = el
            .elements()
            .find(|e| e.attr("text:level") == Some(want.as_str()))
        else {
            return LevelInfo::default();
        };
        let mut info = LevelInfo::default();
        if let Some(lp) = kid(lvl, "style:list-level-properties") {
            info.space_before = lp.attr("text:space-before").and_then(twips);
            info.label_width = lp.attr("text:min-label-width").and_then(twips);
        }
        if let Some(tp) = kid(lvl, "style:text-properties") {
            if tp.attr("style:use-window-font-color") == Some("true") {
                info.color = Some(BulletColor::FollowText);
            } else if let Some(c) = tp.attr("fo:color").and_then(color) {
                info.color = Some(BulletColor::Color(DrawingColor::srgb(&format!(
                    "{:02x}{:02x}{:02x}",
                    c.r, c.g, c.b
                ))));
            }
            if let Some(p) = tp.attr("fo:font-size").and_then(percent) {
                info.size = Some(BulletSize::Percent((p * 1000.0).round() as i32));
            }
            if let Some(f) = tp
                .attr("fo:font-family")
                .or_else(|| tp.attr("style:font-name"))
            {
                info.font = Some(BulletFont::Font(crate::props::unquote(
                    &self.fonts.family(f),
                )));
            }
        }
        info.bullet = match lvl.name.as_str() {
            "text:list-level-style-bullet" => Some(match lvl.attr("text:bullet-char") {
                Some(c) if !c.trim().is_empty() => Bullet::Char(c.to_owned()),
                _ => Bullet::None,
            }),
            "text:list-level-style-number" => {
                let format = lvl.attr("style:num-format").unwrap_or_default();
                match num_scheme(
                    format,
                    lvl.attr("style:num-prefix").unwrap_or_default(),
                    lvl.attr("style:num-suffix").unwrap_or_default(),
                ) {
                    Some(s) => Some(Bullet::AutoNumber {
                        scheme: s.to_owned(),
                        start_at: lvl
                            .attr("text:start-value")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(1)
                            .max(1),
                    }),
                    None => Some(Bullet::None),
                }
            }
            "text:list-level-style-image" => lvl
                .attr("xlink:href")
                .and_then(media)
                .map(Bullet::Picture)
                .or(Some(Bullet::None)),
            _ => None,
        };
        info
    }

    fn para_style(&self, name: Option<&str>) -> (ParagraphProps, DeckParaProps, RunProps) {
        let mut pp = ParagraphProps::default();
        let mut dp = DeckParaProps::default();
        let mut rp = RunProps::default();
        let Some(st) = name.and_then(|n| self.ix.get(self.scope, "paragraph", n)) else {
            return (pp, dp, rp);
        };
        if let Some(p) = kid(st, "style:paragraph-properties") {
            pp = read_para_props(p);
            if let Some(sp) = pp.spacing.take() {
                if let Some(b) = sp.before {
                    dp.space_before = Some(TextSpacing::Points(b * 5));
                }
                if let Some(a) = sp.after {
                    dp.space_after = Some(TextSpacing::Points(a * 5));
                }
                if let Some(l) = sp.line {
                    dp.line_spacing = Some(match sp.line_rule {
                        Some(LineRule::Auto) | None => {
                            TextSpacing::Percent(((i64::from(l) * 100_000) / 240) as i32)
                        }
                        _ => TextSpacing::Points(l * 5),
                    });
                }
            }
            if let Some(t) = p
                .attr("loext:tab-stop-distance")
                .or_else(|| p.attr("style:tab-stop-distance"))
                .and_then(twips)
            {
                dp.default_tab = Some(i64::from(t) * EMU_PER_TWIP);
            }
        }
        if let Some(t) = kid(st, "style:text-properties") {
            rp = read_run_props(t, self.fonts);
        }
        (pp, dp, rp)
    }

    fn span_props(&self, name: &str, outer: &RunProps) -> RunProps {
        match self
            .ix
            .get(self.scope, "text", name)
            .and_then(|s| kid(s, "style:text-properties"))
        {
            Some(t) => {
                kiln_doc::merge::merge_run_props(outer, Some(&read_run_props(t, self.fonts)))
            }
            None => outer.clone(),
        }
    }

    fn inline(&self, el: &XmlElement, props: &RunProps, p: &mut Para) {
        for c in &el.children {
            match c {
                XmlNode::Text(t) => p.text(&t.text, props),
                XmlNode::Element(e) => match e.name.as_str() {
                    "text:span" => {
                        let rp = e
                            .attr("text:style-name")
                            .map_or_else(|| props.clone(), |n| self.span_props(n, props));
                        self.inline(e, &rp, p);
                    }
                    "text:s" => {
                        let n = e
                            .attr("text:c")
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(1)
                            .clamp(1, 1000);
                        p.literal(&" ".repeat(n), props);
                    }
                    "text:tab" => p.literal("\t", props),
                    "text:line-break" => p.object(
                        InlineObject::Break(Break {
                            kind: BreakKind::TextWrapping,
                            clear: None,
                        }),
                        props,
                    ),
                    "text:a" => {
                        let start = p.len();
                        let rp = e
                            .attr("text:style-name")
                            .map_or_else(|| props.clone(), |n| self.span_props(n, props));
                        self.inline(e, &rp, p);
                        let end = p.len();
                        if let Some(href) = e.attr("xlink:href")
                            && end > start
                        {
                            let target = match href.strip_prefix('#') {
                                Some(b) => LinkTarget::Bookmark(b.to_owned()),
                                None => LinkTarget::Url(href.to_owned()),
                            };
                            p.marks.push(MarkSpan::new(
                                start,
                                end,
                                Mark::Link(Hyperlink {
                                    target,
                                    tooltip: None,
                                    frame: None,
                                    history: true,
                                }),
                            ));
                        }
                    }
                    "text:page-number" => {
                        let r = crate::xml::all_text(e);
                        p.object(InlineObject::Field(Field::new("slidenum", &r)), props);
                    }
                    "text:date" | "presentation:date-time" => {
                        let r = crate::xml::all_text(e);
                        p.object(InlineObject::Field(Field::new("datetime1", &r)), props);
                    }
                    "presentation:footer" | "presentation:header" => {}
                    "text:bookmark"
                    | "text:bookmark-start"
                    | "text:bookmark-end"
                    | "office:annotation"
                    | "office:annotation-end" => {}
                    _ => self.inline(e, props, p),
                },
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paragraph(
        &self,
        e: &XmlElement,
        level: usize,
        bullet: Option<LevelInfo>,
        alloc: &mut dyn FnMut() -> NodeId,
        out: &mut Vec<(Paragraph, DeckParaProps)>,
    ) {
        let (mut pp, mut dp, rp) = self.para_style(e.attr("text:style-name"));
        let mut b = Para {
            space: true,
            ..Para::default()
        };
        self.inline(e, &rp, &mut b);
        let mut para = b.finish(alloc());
        if level > 0 {
            pp.outline_level = u8::try_from(level.min(8)).ok();
        }
        if let Some(info) = bullet {
            if info.bullet.is_some() {
                dp.bullet = info.bullet;
                dp.bullet_color = info.color;
                dp.bullet_size = info.size;
                dp.bullet_font = info.font;
            }
            if info.space_before.is_some() || info.label_width.is_some() {
                let own = pp.indent.unwrap_or_default();
                let label = info.label_width.unwrap_or(0);
                pp.indent = Some(Indent {
                    left: Some(own.left.unwrap_or(0) + info.space_before.unwrap_or(0) + label),
                    right: own.right,
                    first_line: None,
                    hanging: (label > 0).then_some(label),
                });
            }
        }
        para.props = pp;
        para.mark_props = rp;
        out.push((para, dp));
    }

    fn list(
        &self,
        el: &XmlElement,
        depth: usize,
        style: Option<&str>,
        alloc: &mut dyn FnMut() -> NodeId,
        media: &mut dyn FnMut(&str) -> Option<String>,
        out: &mut Vec<(Paragraph, DeckParaProps)>,
    ) {
        let style = el.attr("text:style-name").or(style);
        for item in el.elements() {
            let header = item.name == "text:list-header";
            if !header && item.name != "text:list-item" {
                continue;
            }
            let mut first = true;
            for c in item.elements() {
                match c.name.as_str() {
                    "text:p" | "text:h" => {
                        let mut info = self.level_info(style, depth, media);
                        if header || !first {
                            info.bullet = Some(Bullet::None);
                        }
                        self.paragraph(c, depth, Some(info), alloc, out);
                        first = false;
                    }
                    "text:list" => {
                        self.list(c, depth + 1, style, alloc, media, out);
                        first = false;
                    }
                    _ => {}
                }
            }
        }
    }

    /// The text of a frame's `draw:text-box` or a shape's own paragraphs (`None` when there is
    /// none).
    pub fn body(
        &self,
        el: &XmlElement,
        alloc: &mut dyn FnMut() -> NodeId,
        media: &mut dyn FnMut(&str) -> Option<String>,
    ) -> Option<TextBody> {
        let mut paras = Vec::new();
        for c in el.elements() {
            match c.name.as_str() {
                "text:p" | "text:h" => self.paragraph(c, 0, None, alloc, &mut paras),
                "text:list" => self.list(c, 0, None, alloc, media, &mut paras),
                _ => {}
            }
        }
        if paras.is_empty() {
            return None;
        }
        let mut props = BTreeMap::new();
        let mut list = Vec::new();
        for (p, d) in paras {
            if !d.is_empty() {
                props.insert(p.id, d);
            }
            list.push(p);
        }
        TextBody::from_paragraphs(list, props).ok()
    }
}

// ------------------------------------------------------------------ writing

/// A run's colours and theme fonts resolved for writing.
#[must_use]
pub fn flatten_run(r: &RunProps, ctx: &ColorContext, theme: &Theme) -> RunProps {
    let mut r = r.clone();
    if let Some(c) = &r.color
        && (c.theme_color.is_some() || !matches!(c.val, ColorValue::Rgb(_)))
    {
        let d = kiln_deck::dml::color::doc_to_drawing_color(c);
        let rgba = kiln_ooxml::drawingml::color::resolve_color(&d, ctx).to_color();
        r.color = Some(DocColor {
            val: ColorValue::Rgb(kiln_common::Color::rgb(rgba.r, rgba.g, rgba.b)),
            theme_color: None,
            theme_tint: None,
            theme_shade: None,
        });
    } else if let Some(c) = &mut r.color {
        c.theme_color = None;
        c.theme_tint = None;
        c.theme_shade = None;
    }
    if let Some(f) = &r.fonts {
        let pick = |t: Option<kiln_doc::st::ThemeFontRef>| -> Option<String> {
            Some(if t?.is_major() {
                theme.font_scheme.major.latin.typeface.clone()
            } else {
                theme.font_scheme.minor.latin.typeface.clone()
            })
            .filter(|s| !s.is_empty())
        };
        let latin = f
            .ascii
            .clone()
            .or_else(|| pick(f.ascii_theme))
            .or_else(|| pick(f.h_ansi_theme));
        if let Some(l) = latin {
            r.fonts = Some(RunFonts::latin(&l));
        } else {
            r.fonts = None;
        }
    }
    r
}

/// Automatic styles of one part, shared by equal definitions.
#[derive(Debug, Default)]
pub struct Autos {
    /// The styles, in creation order.
    pub list: Vec<El>,
    index: std::collections::HashMap<El, String>,
    counters: std::collections::HashMap<String, usize>,
}

impl Autos {
    /// Registers an element without its `style:name` and returns the name.
    pub fn add(&mut self, prefix: &str, el: El) -> String {
        if let Some(n) = self.index.get(&el) {
            return n.clone();
        }
        let c = self.counters.entry(prefix.to_owned()).or_insert(0);
        *c += 1;
        let name = format!("{prefix}{c}");
        let mut named = el.clone();
        let attr = "style:name";
        named.attrs.insert(0, (attr.to_owned(), name.clone()));
        self.list.push(named);
        self.index.insert(el, name.clone());
        name
    }
}

/// What a paragraph resolves to.
#[derive(Debug, Clone, Default)]
pub struct ResolvedPara {
    /// Paragraph properties.
    pub para: ParagraphProps,
    /// DrawingML extras (bullets, spacing).
    pub deck: DeckParaProps,
    /// The level's run defaults.
    pub run: RunProps,
}

/// Writes a text body's paragraphs. `resolve` gives each paragraph's resolved formatting.
#[allow(missing_debug_implementations)]
pub struct TextWriter<'a> {
    /// The colour context of the shape's slide.
    pub ctx: &'a ColorContext,
    /// The theme (fonts).
    pub theme: &'a Theme,
    /// Automatic styles of the part.
    pub autos: &'a mut Autos,
    /// Maps a picture part to its package path.
    pub image: &'a mut dyn FnMut(&str) -> Option<String>,
}

fn spacing_twips(s: TextSpacing, size_hp: Option<u32>) -> i32 {
    match s {
        TextSpacing::Points(p) => p / 5,
        // Percent of a line: approximated with 1.2 × the font size.
        TextSpacing::Percent(pc) => {
            let line = f64::from(size_hp.unwrap_or(36)) * 10.0 * 1.2;
            (line * f64::from(pc) / 100_000.0).round() as i32
        }
    }
}

impl TextWriter<'_> {
    fn list_style(
        &mut self,
        level: usize,
        deck: &DeckParaProps,
        indent: &Indent,
        run: &RunProps,
    ) -> Option<String> {
        let bullet = deck.bullet.clone()?;
        if bullet == Bullet::None {
            return None;
        }
        let lvl = (level + 1).to_string();
        let hanging = indent.hanging.unwrap_or(0).max(0);
        let left = indent.left.unwrap_or(0);
        let mut props = El::new("style:list-level-properties")
            .with(
                "text:space-before",
                twips_str(i64::from((left - hanging).max(0))),
            )
            .with("text:min-label-width", twips_str(i64::from(hanging)));
        if hanging == 0 {
            props.set("text:min-label-width", "0in");
        }
        let mut tp = El::new("style:text-properties");
        match &deck.bullet_color {
            Some(BulletColor::Color(c)) => {
                tp.set("fo:color", super::graphic::css(c, self.ctx).0);
            }
            _ => match run.color.as_ref().map(|c| c.val) {
                Some(ColorValue::Rgb(c)) => tp.set("fo:color", crate::units::color_str(c)),
                _ => tp.set("style:use-window-font-color", "true"),
            },
        }
        match deck.bullet_size {
            Some(BulletSize::Percent(p)) => tp.set("fo:font-size", format!("{}%", p / 1000)),
            _ => tp.set("fo:font-size", "100%"),
        }
        let mut el = match &bullet {
            Bullet::Char(c) => {
                if let Some(BulletFont::Font(f)) = &deck.bullet_font
                    && !f.starts_with('+')
                {
                    tp.set("fo:font-family", f.clone());
                }
                El::new("text:list-level-style-bullet")
                    .with("text:level", lvl)
                    .with(
                        "text:bullet-char",
                        c.chars().next().map_or("•".to_owned(), |c| c.to_string()),
                    )
            }
            Bullet::AutoNumber { scheme, start_at } => {
                let (format, prefix, suffix) = scheme_parts(scheme);
                let mut e = El::new("text:list-level-style-number")
                    .with("text:level", lvl)
                    .with("style:num-format", format);
                if !prefix.is_empty() {
                    e.set("style:num-prefix", prefix);
                }
                if !suffix.is_empty() {
                    e.set("style:num-suffix", suffix);
                }
                if *start_at > 1 {
                    e.set("text:start-value", start_at.to_string());
                }
                e
            }
            Bullet::Picture(key) => {
                let path = (self.image)(key)?;
                El::new("text:list-level-style-image")
                    .with("text:level", lvl)
                    .with("xlink:href", path)
                    .with("xlink:type", "simple")
                    .with("xlink:show", "embed")
                    .with("xlink:actuate", "onLoad")
            }
            Bullet::None => return None,
        };
        el.children.push(props);
        el.children.push(tp);
        Some(
            self.autos
                .add("L", El::new("text:list-style").with_child(el)),
        )
    }

    /// Writes the paragraphs of a body.
    pub fn write(
        &mut self,
        w: &mut XmlOut,
        body: &TextBody,
        resolve: &dyn Fn(&Paragraph) -> ResolvedPara,
    ) {
        let mut last_list: Option<String> = None;
        for p in body.paragraphs() {
            let r = resolve(p);
            let level = TextBody::level_of(p);
            let base_run = flatten_run(&r.run, self.ctx, self.theme);
            let indent = r.para.indent.unwrap_or_default();
            let list = self.list_style(level, &r.deck, &indent, &base_run);
            // Paragraph style.
            let mut pp = r.para.clone();
            pp.style_id = None;
            pp.numbering = None;
            pp.outline_level = None;
            pp.spacing = Some(Spacing {
                before: r.deck.space_before.map(|s| spacing_twips(s, base_run.size)),
                after: r.deck.space_after.map(|s| spacing_twips(s, base_run.size)),
                line: r.deck.line_spacing.map(|s| match s {
                    TextSpacing::Percent(pc) => ((i64::from(pc) * 240) / 100_000) as i32,
                    TextSpacing::Points(p) => p / 5,
                }),
                line_rule: r.deck.line_spacing.map(|s| match s {
                    TextSpacing::Percent(_) => LineRule::Auto,
                    TextSpacing::Points(_) => LineRule::Exact,
                }),
                ..Spacing::default()
            });
            if list.is_some() || (level > 0 && r.deck.bullet.is_none()) {
                // Indents of list paragraphs are the list level's.
                pp.indent = None;
            }
            let theme = DocTheme::default();
            let mut st = El::new("style:style").with("style:family", "paragraph");
            st.push_nonempty(para_props_el(&pp, &theme));
            st.push_nonempty(run_props_el(&base_run, &theme));
            let pname = self.autos.add("P", st);
            // Lists: one per paragraph, nested to the level; numbering continues across them.
            let in_list = list.is_some() || level > 0;
            if in_list {
                w.start("text:list");
                if let Some(l) = &list {
                    w.attr("text:style-name", l);
                    if last_list.as_deref() == Some(l.as_str()) {
                        w.attr("text:continue-numbering", "true");
                    }
                }
                for _ in 0..level {
                    w.start("text:list-item").start("text:list");
                }
                w.start(if list.is_some() {
                    "text:list-item"
                } else {
                    "text:list-header"
                });
            }
            last_list = list.clone();
            w.start("text:p").attr("text:style-name", pname);
            self.inlines(w, p, &r.run);
            w.end();
            if in_list {
                w.end();
                for _ in 0..level {
                    w.end().end();
                }
                w.end();
            }
        }
    }

    fn inlines(&mut self, w: &mut XmlOut, p: &Paragraph, base: &RunProps) {
        let theme = DocTheme::default();
        let chars: Vec<char> = p.content.raw_text().chars().collect();
        let objects: Vec<(usize, &InlineObject)> = p.content.objects_with_offsets().collect();
        let marks = p.content.marks();
        let mut i = 0;
        let mut last_space = true;
        while i < chars.len() {
            // A run of equal formatting and link.
            let props = p.content.props_at(i).cloned().unwrap_or_default();
            let link = marks
                .iter()
                .find(|m| m.start <= i && i < m.end && matches!(m.mark, Mark::Link(_)));
            let mut j = i + 1;
            while j < chars.len()
                && p.content.props_at(j) == Some(&props)
                && marks
                    .iter()
                    .find(|m| m.start <= j && j < m.end && matches!(m.mark, Mark::Link(_)))
                    .map(|m| (m.start, m.end))
                    == link.map(|m| (m.start, m.end))
            {
                j += 1;
            }
            let merged = flatten_run(
                &kiln_doc::merge::merge_run_props(base, Some(&props)),
                self.ctx,
                self.theme,
            );
            let mut st = El::new("style:style").with("style:family", "text");
            st.push_nonempty(run_props_el(&merged, &theme));
            let tname = self.autos.add("T", st);
            // The span holds the link, as LibreOffice writes it (Impress shows no text for a
            // span inside a link in a shape).
            w.start("text:span").attr("text:style-name", tname);
            let mut linked = false;
            if let Some(m) = link
                && let Mark::Link(h) = &m.mark
            {
                let href = match &h.target {
                    LinkTarget::Url(u) => u.clone(),
                    LinkTarget::Bookmark(b) => format!("#{b}"),
                };
                w.start("text:a")
                    .attr("xlink:href", href)
                    .attr("xlink:type", "simple");
                linked = true;
            }
            let mut buf = String::new();
            let mut spaces = 0usize;
            let flush = |w: &mut XmlOut, buf: &mut String, spaces: &mut usize| {
                if !buf.is_empty() {
                    w.text(buf);
                    buf.clear();
                }
                if *spaces > 0 {
                    w.start("text:s");
                    if *spaces > 1 {
                        w.attr("text:c", spaces.to_string());
                    }
                    w.end();
                    *spaces = 0;
                }
            };
            for (off, c) in chars.iter().enumerate().take(j).skip(i) {
                match *c {
                    ' ' => {
                        if last_space {
                            if !buf.is_empty() {
                                w.text(&buf);
                                buf.clear();
                            }
                            spaces += 1;
                        } else {
                            buf.push(' ');
                            last_space = true;
                        }
                    }
                    '\t' => {
                        flush(w, &mut buf, &mut spaces);
                        w.start("text:tab").end();
                        last_space = false;
                    }
                    kiln_doc::OBJECT_CHAR => {
                        flush(w, &mut buf, &mut spaces);
                        match objects.iter().find(|(o, _)| *o == off).map(|(_, o)| *o) {
                            Some(InlineObject::Break(_)) => {
                                w.start("text:line-break").end();
                            }
                            Some(InlineObject::Field(f)) => {
                                let t = f.instr.trim().to_ascii_lowercase();
                                if t.starts_with("slidenum") {
                                    w.start("text:page-number").text(&f.result).end();
                                } else if t.starts_with("datetime") {
                                    w.start("text:date").text(&f.result).end();
                                } else {
                                    w.text(&f.result);
                                }
                            }
                            _ => {}
                        }
                        last_space = false;
                    }
                    c => {
                        if spaces > 0 {
                            flush(w, &mut buf, &mut spaces);
                        }
                        buf.push(c);
                        last_space = false;
                    }
                }
            }
            // Trailing spaces of the paragraph are explicit.
            if j == chars.len() && buf.ends_with(' ') {
                buf.pop();
                spaces += 1;
            }
            flush(w, &mut buf, &mut spaces);
            if linked {
                w.end();
            }
            w.end();
            i = j;
        }
    }
}
