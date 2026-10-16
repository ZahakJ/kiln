// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML text properties ⇄ the model: `a:bodyPr` ⇄ [`BodyProps`], `a:pPr`/`a:lvlNpPr` ⇄
//! kiln-doc `ParagraphProps` + [`DeckParaProps`], `a:rPr`/`a:defRPr` ⇄ kiln-doc `RunProps`,
//! `a:lstStyle` (and the master's `p:titleStyle`/`p:bodyStyle`/`p:otherStyle`) ⇄ [`ListStyle`].
//!
//! The `.pptx` reader and writer and the bundled theme files use these. Units: indents and tab
//! positions are EMU in DrawingML and twips in kiln-doc (635 EMU per twip); font sizes are
//! hundredths of a point in DrawingML and half-points in kiln-doc; character spacing is
//! hundredths of a point and twips.

use kiln_doc::props::Lang;
use kiln_doc::st::{
    Alignment, HighlightColor, TabAlignment, ThemeFontRef, UnderlineStyle, VertAlign,
};
use kiln_doc::{Indent, ParagraphProps, RunFonts, RunProps, TabStop, Underline};
use kiln_ooxml::drawingml::color::{ColorContext, parse_color_child, serialize_color};
use kiln_ooxml::xml::XmlElement;

use super::color::{doc_to_drawing_color, drawing_to_doc_color};
use crate::text::{
    Autofit, BodyProps, Bullet, BulletColor, BulletFont, BulletSize, DeckParaProps, FontAlign,
    LEVELS, LevelStyle, ListStyle, TextAnchor, TextDirection, TextSpacing, TextWrap,
};

/// EMU per twip.
pub const EMU_PER_TWIP: i64 = 635;

fn emu_to_twips(emu: i64) -> i32 {
    let t = if emu >= 0 {
        emu.saturating_add(EMU_PER_TWIP / 2) / EMU_PER_TWIP
    } else {
        emu.saturating_sub(EMU_PER_TWIP / 2) / EMU_PER_TWIP
    };
    i32::try_from(t).unwrap_or(if t < 0 { i32::MIN } else { i32::MAX })
}

fn twips_to_emu(t: i32) -> i64 {
    i64::from(t) * EMU_PER_TWIP
}

fn attr_i64(e: &XmlElement, n: &str) -> Option<i64> {
    e.attr_int(n)
}

fn attr_i32(e: &XmlElement, n: &str) -> Option<i32> {
    e.attr_int(n).and_then(|v| i32::try_from(v).ok())
}

fn attr_u32(e: &XmlElement, n: &str) -> Option<u32> {
    e.attr_int(n).and_then(|v| u32::try_from(v).ok())
}

// ------------------------------------------------------------------ bodyPr

/// Reads `a:bodyPr`.
#[must_use]
pub fn parse_body_props(e: &XmlElement) -> BodyProps {
    let mut b = BodyProps {
        insets: [
            attr_i64(e, "lIns"),
            attr_i64(e, "tIns"),
            attr_i64(e, "rIns"),
            attr_i64(e, "bIns"),
        ],
        anchor: e.attr("anchor").and_then(TextAnchor::parse),
        anchor_center: e.attr_bool("anchorCtr"),
        autofit: None,
        columns: attr_u32(e, "numCol"),
        column_spacing: attr_i64(e, "spcCol"),
        rtl_columns: e.attr_bool("rtlCol"),
        wrap: e.attr("wrap").and_then(|w| match w {
            "square" => Some(TextWrap::Square),
            "none" => Some(TextWrap::None),
            _ => None,
        }),
        direction: e.attr("vert").and_then(TextDirection::parse),
        rotation: attr_i32(e, "rot"),
        upright: e.attr_bool("upright"),
        space_first_last: e.attr_bool("spcFirstLastPara"),
        warp: e
            .child("a:prstTxWarp")
            .and_then(|w| w.attr("prst"))
            .map(str::to_owned),
    };
    for c in e.elements() {
        match c.name.as_str() {
            "a:noAutofit" => b.autofit = Some(Autofit::Off),
            "a:spAutoFit" => b.autofit = Some(Autofit::ResizeShape),
            "a:normAutofit" => {
                b.autofit = Some(Autofit::Shrink {
                    font_scale: attr_u32(c, "fontScale").unwrap_or(100_000),
                    line_reduction: attr_u32(c, "lnSpcReduction").unwrap_or(0),
                });
            }
            _ => {}
        }
    }
    b
}

/// Writes `a:bodyPr` (attributes in Office's order).
#[must_use]
pub fn body_props_to_element(b: &BodyProps) -> XmlElement {
    let autofit = b.autofit.map(|a| match a {
        Autofit::Off => XmlElement::new("a:noAutofit"),
        Autofit::ResizeShape => XmlElement::new("a:spAutoFit"),
        Autofit::Shrink {
            font_scale,
            line_reduction,
        } => XmlElement::new("a:normAutofit")
            .with_opt_attr("fontScale", (font_scale != 100_000).then_some(font_scale))
            .with_opt_attr(
                "lnSpcReduction",
                (line_reduction != 0).then_some(line_reduction),
            ),
    });
    let warp = b.warp.as_ref().map(|w| {
        XmlElement::new("a:prstTxWarp")
            .with_attr("prst", w.as_str())
            .with_child(XmlElement::new("a:avLst"))
    });
    XmlElement::new("a:bodyPr")
        .with_opt_attr("rot", b.rotation)
        .with_opt_attr("spcFirstLastPara", b.space_first_last)
        .with_opt_attr("vert", b.direction.map(TextDirection::as_str))
        .with_opt_attr(
            "wrap",
            b.wrap.map(|w| match w {
                TextWrap::Square => "square",
                TextWrap::None => "none",
            }),
        )
        .with_opt_attr("lIns", b.insets[0])
        .with_opt_attr("tIns", b.insets[1])
        .with_opt_attr("rIns", b.insets[2])
        .with_opt_attr("bIns", b.insets[3])
        .with_opt_attr("numCol", b.columns)
        .with_opt_attr("spcCol", b.column_spacing)
        .with_opt_attr("rtlCol", b.rtl_columns)
        .with_opt_attr("anchor", b.anchor.map(TextAnchor::as_str))
        .with_opt_attr("anchorCtr", b.anchor_center)
        .with_opt_attr("upright", b.upright)
        .with_opt_child(warp)
        .with_opt_child(autofit)
}

// ------------------------------------------------------------------ pPr

fn parse_alignment(s: &str) -> Option<Alignment> {
    Some(match s {
        "l" => Alignment::Left,
        "ctr" => Alignment::Center,
        "r" => Alignment::Right,
        "just" => Alignment::Both,
        "justLow" => Alignment::LowKashida,
        "dist" => Alignment::Distribute,
        "thaiDist" => Alignment::ThaiDistribute,
        _ => return None,
    })
}

fn alignment_str(a: Alignment) -> &'static str {
    match a {
        Alignment::Left => "l",
        Alignment::Center => "ctr",
        Alignment::Right => "r",
        Alignment::Both | Alignment::MediumKashida | Alignment::HighKashida => "just",
        Alignment::LowKashida => "justLow",
        Alignment::Distribute => "dist",
        Alignment::ThaiDistribute => "thaiDist",
    }
}

fn parse_spacing(e: Option<&XmlElement>) -> Option<TextSpacing> {
    let e = e?;
    if let Some(p) = e.child("a:spcPct") {
        return attr_i32(p, "val").map(TextSpacing::Percent);
    }
    e.child("a:spcPts")
        .and_then(|p| attr_i32(p, "val"))
        .map(TextSpacing::Points)
}

fn spacing_element(name: &str, s: TextSpacing) -> XmlElement {
    let inner = match s {
        TextSpacing::Percent(v) => XmlElement::new("a:spcPct").with_attr("val", v),
        TextSpacing::Points(v) => XmlElement::new("a:spcPts").with_attr("val", v),
    };
    XmlElement::new(name).with_child(inner)
}

/// Reads the paragraph part of `a:pPr`, `a:defPPr` or `a:lvlNpPr` (not its `a:defRPr`).
#[must_use]
pub fn parse_paragraph_props(e: &XmlElement) -> (ParagraphProps, DeckParaProps) {
    let mut p = ParagraphProps {
        alignment: e.attr("algn").and_then(parse_alignment),
        bidi: e.attr_bool("rtl"),
        outline_level: e
            .attr_int("lvl")
            .and_then(|l| u8::try_from(l).ok())
            .filter(|l| usize::from(*l) < LEVELS),
        ..ParagraphProps::default()
    };
    let mar_l = attr_i64(e, "marL");
    let mar_r = attr_i64(e, "marR");
    let indent = attr_i64(e, "indent");
    if mar_l.is_some() || mar_r.is_some() || indent.is_some() {
        p.indent = Some(Indent {
            left: mar_l.map(emu_to_twips),
            right: mar_r.map(emu_to_twips),
            first_line: indent.filter(|i| *i >= 0).map(emu_to_twips),
            hanging: indent.filter(|i| *i < 0).map(|i| emu_to_twips(-i)),
        });
    }
    if let Some(tabs) = e.child("a:tabLst") {
        p.tabs = Some(
            tabs.children_named("a:tab")
                .map(|t| TabStop {
                    pos: emu_to_twips(attr_i64(t, "pos").unwrap_or(0)),
                    align: match t.attr("algn") {
                        Some("ctr") => TabAlignment::Center,
                        Some("r") => TabAlignment::Right,
                        Some("dec") => TabAlignment::Decimal,
                        _ => TabAlignment::Left,
                    },
                    leader: None,
                })
                .collect(),
        );
    }
    let mut d = DeckParaProps {
        line_spacing: parse_spacing(e.child("a:lnSpc")),
        space_before: parse_spacing(e.child("a:spcBef")),
        space_after: parse_spacing(e.child("a:spcAft")),
        default_tab: attr_i64(e, "defTabSz"),
        font_align: e.attr("fontAlgn").and_then(|f| match f {
            "auto" => Some(FontAlign::Auto),
            "t" => Some(FontAlign::Top),
            "ctr" => Some(FontAlign::Center),
            "base" => Some(FontAlign::Baseline),
            "b" => Some(FontAlign::Bottom),
            _ => None,
        }),
        east_asian_line_break: e.attr_bool("eaLnBrk"),
        hanging_punctuation: e.attr_bool("hangingPunct"),
        ..DeckParaProps::default()
    };
    for c in e.elements() {
        match c.name.as_str() {
            "a:buClrTx" => d.bullet_color = Some(BulletColor::FollowText),
            "a:buClr" => {
                d.bullet_color = parse_color_child(Some(c)).map(BulletColor::Color);
            }
            "a:buSzTx" => d.bullet_size = Some(BulletSize::FollowText),
            "a:buSzPct" => d.bullet_size = attr_i32(c, "val").map(BulletSize::Percent),
            "a:buSzPts" => d.bullet_size = attr_i32(c, "val").map(BulletSize::Points),
            "a:buFontTx" => d.bullet_font = Some(BulletFont::FollowText),
            "a:buFont" => {
                d.bullet_font = c.attr("typeface").map(|t| BulletFont::Font(t.to_owned()));
            }
            "a:buNone" => d.bullet = Some(Bullet::None),
            "a:buChar" => d.bullet = c.attr("char").map(|ch| Bullet::Char(ch.to_owned())),
            "a:buAutoNum" => {
                d.bullet = Some(Bullet::AutoNumber {
                    scheme: c.attr("type").unwrap_or("arabicPeriod").to_owned(),
                    start_at: attr_u32(c, "startAt").unwrap_or(1).max(1),
                });
            }
            "a:buBlip" => {
                let embed = c
                    .child("a:blip")
                    .and_then(|b| b.attr("r:embed"))
                    .unwrap_or("");
                d.bullet = Some(Bullet::Picture(embed.to_owned()));
            }
            _ => {}
        }
    }
    (p, d)
}

/// Writes the attributes and children of a paragraph-properties element (`a:pPr`,
/// `a:lvlNpPr`) in schema order; `def_rpr` is appended as its `a:defRPr`.
#[must_use]
pub fn paragraph_props_to_element(
    name: &str,
    p: &ParagraphProps,
    d: &DeckParaProps,
    def_rpr: Option<XmlElement>,
) -> XmlElement {
    let ind = p.indent.unwrap_or_default();
    let indent = match (ind.first_line, ind.hanging) {
        (_, Some(h)) => Some(-twips_to_emu(h)),
        (Some(f), None) => Some(twips_to_emu(f)),
        (None, None) => None,
    };
    let mut el = XmlElement::new(name)
        .with_opt_attr("marL", ind.left.map(twips_to_emu))
        .with_opt_attr("marR", ind.right.map(twips_to_emu))
        .with_opt_attr("lvl", p.outline_level.filter(|l| *l > 0))
        .with_opt_attr("indent", indent)
        .with_opt_attr("algn", p.alignment.map(alignment_str))
        .with_opt_attr("defTabSz", d.default_tab)
        .with_opt_attr("rtl", p.bidi)
        .with_opt_attr("eaLnBrk", d.east_asian_line_break)
        .with_opt_attr(
            "fontAlgn",
            d.font_align.map(|f| match f {
                FontAlign::Auto => "auto",
                FontAlign::Top => "t",
                FontAlign::Center => "ctr",
                FontAlign::Baseline => "base",
                FontAlign::Bottom => "b",
            }),
        )
        .with_opt_attr("hangingPunct", d.hanging_punctuation);
    if let Some(s) = d.line_spacing {
        el = el.with_child(spacing_element("a:lnSpc", s));
    }
    if let Some(s) = d.space_before {
        el = el.with_child(spacing_element("a:spcBef", s));
    }
    if let Some(s) = d.space_after {
        el = el.with_child(spacing_element("a:spcAft", s));
    }
    match &d.bullet_color {
        Some(BulletColor::FollowText) => el = el.with_child(XmlElement::new("a:buClrTx")),
        Some(BulletColor::Color(c)) => {
            el = el.with_child(XmlElement::new("a:buClr").with_child(serialize_color(c)));
        }
        None => {}
    }
    match d.bullet_size {
        Some(BulletSize::FollowText) => el = el.with_child(XmlElement::new("a:buSzTx")),
        Some(BulletSize::Percent(v)) => {
            el = el.with_child(XmlElement::new("a:buSzPct").with_attr("val", v));
        }
        Some(BulletSize::Points(v)) => {
            el = el.with_child(XmlElement::new("a:buSzPts").with_attr("val", v));
        }
        None => {}
    }
    match &d.bullet_font {
        Some(BulletFont::FollowText) => el = el.with_child(XmlElement::new("a:buFontTx")),
        Some(BulletFont::Font(f)) => {
            el = el.with_child(XmlElement::new("a:buFont").with_attr("typeface", f.as_str()));
        }
        None => {}
    }
    match &d.bullet {
        Some(Bullet::None) => el = el.with_child(XmlElement::new("a:buNone")),
        Some(Bullet::Char(c)) => {
            el = el.with_child(XmlElement::new("a:buChar").with_attr("char", c.as_str()));
        }
        Some(Bullet::AutoNumber { scheme, start_at }) => {
            el = el.with_child(
                XmlElement::new("a:buAutoNum")
                    .with_attr("type", scheme.as_str())
                    .with_opt_attr("startAt", (*start_at != 1).then_some(*start_at)),
            );
        }
        Some(Bullet::Picture(embed)) => {
            el = el.with_child(
                XmlElement::new("a:buBlip")
                    .with_child(XmlElement::new("a:blip").with_attr("r:embed", embed.as_str())),
            );
        }
        None => {}
    }
    if let Some(tabs) = &p.tabs {
        el = el.with_child(
            XmlElement::new("a:tabLst").with_children(tabs.iter().map(|t| {
                XmlElement::new("a:tab")
                    .with_attr("pos", twips_to_emu(t.pos))
                    .with_attr(
                        "algn",
                        match t.align {
                            TabAlignment::Center => "ctr",
                            TabAlignment::Right => "r",
                            TabAlignment::Decimal => "dec",
                            _ => "l",
                        },
                    )
            })),
        );
    }
    el.with_opt_child(def_rpr)
}

// ------------------------------------------------------------------ rPr

fn parse_underline(s: &str) -> UnderlineStyle {
    match s {
        "none" => UnderlineStyle::None,
        "words" => UnderlineStyle::Words,
        "dbl" => UnderlineStyle::Double,
        "heavy" => UnderlineStyle::Thick,
        "dotted" => UnderlineStyle::Dotted,
        "dottedHeavy" => UnderlineStyle::DottedHeavy,
        "dash" => UnderlineStyle::Dash,
        "dashHeavy" => UnderlineStyle::DashedHeavy,
        "dashLong" => UnderlineStyle::DashLong,
        "dashLongHeavy" => UnderlineStyle::DashLongHeavy,
        "dotDash" => UnderlineStyle::DotDash,
        "dotDashHeavy" => UnderlineStyle::DashDotHeavy,
        "dotDotDash" => UnderlineStyle::DotDotDash,
        "dotDotDashHeavy" => UnderlineStyle::DashDotDotHeavy,
        "wavy" => UnderlineStyle::Wave,
        "wavyHeavy" => UnderlineStyle::WavyHeavy,
        "wavyDbl" => UnderlineStyle::WavyDouble,
        _ => UnderlineStyle::Single,
    }
}

fn underline_str(u: UnderlineStyle) -> &'static str {
    match u {
        UnderlineStyle::None => "none",
        UnderlineStyle::Words => "words",
        UnderlineStyle::Double => "dbl",
        UnderlineStyle::Thick => "heavy",
        UnderlineStyle::Dotted => "dotted",
        UnderlineStyle::DottedHeavy => "dottedHeavy",
        UnderlineStyle::Dash => "dash",
        UnderlineStyle::DashedHeavy => "dashHeavy",
        UnderlineStyle::DashLong => "dashLong",
        UnderlineStyle::DashLongHeavy => "dashLongHeavy",
        UnderlineStyle::DotDash => "dotDash",
        UnderlineStyle::DashDotHeavy => "dotDashHeavy",
        UnderlineStyle::DotDotDash => "dotDotDash",
        UnderlineStyle::DashDotDotHeavy => "dotDotDashHeavy",
        UnderlineStyle::Wave => "wavy",
        UnderlineStyle::WavyHeavy => "wavyHeavy",
        UnderlineStyle::WavyDouble => "wavyDbl",
        UnderlineStyle::Single => "sng",
    }
}

fn theme_ref(typeface: &str) -> Option<ThemeFontRef> {
    Some(match typeface {
        "+mj-lt" => ThemeFontRef::MajorHAnsi,
        "+mn-lt" => ThemeFontRef::MinorHAnsi,
        "+mj-ea" => ThemeFontRef::MajorEastAsia,
        "+mn-ea" => ThemeFontRef::MinorEastAsia,
        "+mj-cs" => ThemeFontRef::MajorBidi,
        "+mn-cs" => ThemeFontRef::MinorBidi,
        _ => return None,
    })
}

fn theme_ref_str(r: ThemeFontRef) -> &'static str {
    match r {
        ThemeFontRef::MajorAscii | ThemeFontRef::MajorHAnsi => "+mj-lt",
        ThemeFontRef::MinorAscii | ThemeFontRef::MinorHAnsi => "+mn-lt",
        ThemeFontRef::MajorEastAsia => "+mj-ea",
        ThemeFontRef::MinorEastAsia => "+mn-ea",
        ThemeFontRef::MajorBidi => "+mj-cs",
        ThemeFontRef::MinorBidi => "+mn-cs",
    }
}

/// Reads `a:rPr`, `a:defRPr` or `a:endParaRPr`. With a colour context, theme colours cache
/// their exact rendering.
#[must_use]
pub fn parse_run_props(e: &XmlElement, ctx: Option<&ColorContext>) -> RunProps {
    let mut r = RunProps {
        size: attr_i64(e, "sz").and_then(|s| u32::try_from(s.saturating_add(25) / 50).ok()),
        bold: e.attr_bool("b"),
        italic: e.attr_bool("i"),
        underline: e.attr("u").map(|u| Underline::new(parse_underline(u))),
        kern: attr_i64(e, "kern").and_then(|k| u32::try_from(k.saturating_add(25) / 50).ok()),
        spacing: attr_i64(e, "spc").and_then(|s| {
            let t = if s >= 0 {
                s.saturating_add(2) / 5
            } else {
                s.saturating_sub(2) / 5
            };
            i32::try_from(t).ok()
        }),
        vert_align: attr_i64(e, "baseline").map(|b| match b {
            b if b > 0 => VertAlign::Superscript,
            b if b < 0 => VertAlign::Subscript,
            _ => VertAlign::Baseline,
        }),
        lang: e.attr("lang").map(|l| Lang {
            val: Some(l.to_owned()),
            ..Lang::default()
        }),
        ..RunProps::default()
    };
    match e.attr("strike") {
        Some("sngStrike") => r.strike = Some(true),
        Some("dblStrike") => r.dstrike = Some(true),
        Some("noStrike") => {
            r.strike = Some(false);
            r.dstrike = Some(false);
        }
        _ => {}
    }
    match e.attr("cap") {
        Some("all") => r.caps = Some(true),
        Some("small") => r.small_caps = Some(true),
        Some("none") => {
            r.caps = Some(false);
            r.small_caps = Some(false);
        }
        _ => {}
    }
    let mut fonts = RunFonts::default();
    let mut any_font = false;
    for c in e.elements() {
        match c.name.as_str() {
            "a:solidFill" => {
                r.color = parse_color_child(Some(c)).map(|d| drawing_to_doc_color(&d, ctx));
            }
            "a:highlight" => {
                r.highlight = parse_color_child(Some(c)).and_then(|d| {
                    let hex = match &d.model {
                        kiln_ooxml::drawingml::color::ColorModel::Srgb(h) => h.clone(),
                        _ => return None,
                    };
                    HighlightColor::ALL.iter().copied().find(|h| {
                        h.rgb()
                            .is_some_and(|c| format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b) == hex)
                    })
                });
            }
            "a:latin" | "a:ea" | "a:cs" => {
                let Some(tf) = c.attr("typeface").filter(|t| !t.is_empty()) else {
                    continue;
                };
                any_font = true;
                let theme = theme_ref(tf);
                match c.name.as_str() {
                    "a:latin" => {
                        if theme.is_some() {
                            fonts.ascii_theme = theme;
                            fonts.h_ansi_theme = theme;
                        } else {
                            fonts.ascii = Some(tf.to_owned());
                            fonts.h_ansi = Some(tf.to_owned());
                        }
                    }
                    "a:ea" => {
                        if theme.is_some() {
                            fonts.east_asia_theme = theme;
                        } else {
                            fonts.east_asia = Some(tf.to_owned());
                        }
                    }
                    _ => {
                        if theme.is_some() {
                            fonts.cs_theme = theme;
                        } else {
                            fonts.cs = Some(tf.to_owned());
                        }
                    }
                }
            }
            "a:rtl" => r.rtl = Some(c.attr_bool("val").unwrap_or(true)),
            _ => {}
        }
    }
    if any_font {
        r.fonts = Some(fonts);
    }
    r
}

/// Writes run properties as `name` (`a:rPr`, `a:defRPr`, `a:endParaRPr`).
#[must_use]
pub fn run_props_to_element(name: &str, r: &RunProps) -> XmlElement {
    let strike = match (r.strike, r.dstrike) {
        (_, Some(true)) => Some("dblStrike"),
        (Some(true), _) => Some("sngStrike"),
        (Some(false), _) | (_, Some(false)) => Some("noStrike"),
        _ => None,
    };
    let cap = match (r.caps, r.small_caps) {
        (Some(true), _) => Some("all"),
        (_, Some(true)) => Some("small"),
        (Some(false), _) | (_, Some(false)) => Some("none"),
        _ => None,
    };
    let baseline = r.vert_align.map(|v| match v {
        VertAlign::Superscript => 30_000,
        VertAlign::Subscript => -25_000,
        VertAlign::Baseline => 0,
    });
    let mut el = XmlElement::new(name)
        .with_opt_attr("lang", r.lang.as_ref().and_then(|l| l.val.as_deref()))
        .with_opt_attr("sz", r.size.map(|s| i64::from(s) * 50))
        .with_opt_attr("b", r.bold)
        .with_opt_attr("i", r.italic)
        .with_opt_attr("u", r.underline.as_ref().map(|u| underline_str(u.style)))
        .with_opt_attr("strike", strike)
        .with_opt_attr("kern", r.kern.map(|k| i64::from(k) * 50))
        .with_opt_attr("cap", cap)
        .with_opt_attr("spc", r.spacing.map(|s| i64::from(s) * 5))
        .with_opt_attr("baseline", baseline);
    if let Some(c) = &r.color {
        el = el.with_child(
            XmlElement::new("a:solidFill").with_child(serialize_color(&doc_to_drawing_color(c))),
        );
    }
    if let Some(rgb) = r.highlight.and_then(HighlightColor::rgb) {
        el = el.with_child(
            XmlElement::new("a:highlight").with_child(
                XmlElement::new("a:srgbClr")
                    .with_attr("val", format!("{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b)),
            ),
        );
    }
    if let Some(f) = &r.fonts {
        let latin = f
            .h_ansi_theme
            .or(f.ascii_theme)
            .map(|t| theme_ref_str(t).to_owned())
            .or_else(|| f.h_ansi.clone().or_else(|| f.ascii.clone()));
        let ea = f
            .east_asia_theme
            .map(|t| theme_ref_str(t).to_owned())
            .or_else(|| f.east_asia.clone());
        let cs = f
            .cs_theme
            .map(|t| theme_ref_str(t).to_owned())
            .or_else(|| f.cs.clone());
        for (n, v) in [("a:latin", latin), ("a:ea", ea), ("a:cs", cs)] {
            if let Some(v) = v {
                el = el.with_child(XmlElement::new(n).with_attr("typeface", v));
            }
        }
    }
    if let Some(rtl) = r.rtl {
        el = el.with_child(XmlElement::new("a:rtl").with_attr("val", rtl));
    }
    el
}

// ------------------------------------------------------------------ list styles

/// Reads one level (`a:lvlNpPr`, `a:defPPr`) with its `a:defRPr`.
#[must_use]
pub fn parse_level(e: &XmlElement, ctx: Option<&ColorContext>) -> LevelStyle {
    let (para, deck) = parse_paragraph_props(e);
    LevelStyle {
        para,
        deck,
        run: e
            .child("a:defRPr")
            .map(|r| parse_run_props(r, ctx))
            .unwrap_or_default(),
    }
}

/// Writes one level as `name`.
#[must_use]
pub fn level_to_element(name: &str, l: &LevelStyle) -> XmlElement {
    let def = (!l.run.is_empty()).then(|| run_props_to_element("a:defRPr", &l.run));
    paragraph_props_to_element(name, &l.para, &l.deck, def)
}

/// Reads `a:lstStyle`, `p:titleStyle`, `p:bodyStyle`, `p:otherStyle`, `p:notesStyle` or
/// `p:defaultTextStyle`.
#[must_use]
pub fn parse_list_style(e: &XmlElement, ctx: Option<&ColorContext>) -> ListStyle {
    let mut s = ListStyle::default();
    s.set_default_level(e.child("a:defPPr").map(|d| parse_level(d, ctx)));
    for i in 0..LEVELS {
        if let Some(l) = e.child(&format!("a:lvl{}pPr", i + 1)) {
            s.set_level(i, parse_level(l, ctx));
        }
    }
    s
}

/// Writes a list style as `name`.
#[must_use]
pub fn list_style_to_element(name: &str, s: &ListStyle) -> XmlElement {
    let mut el = XmlElement::new(name);
    if let Some(d) = s.default_level() {
        el = el.with_child(level_to_element("a:defPPr", d));
    }
    for (i, l) in s.levels() {
        el = el.with_child(level_to_element(&format!("a:lvl{}pPr", i + 1), l));
    }
    el
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::DocColor;
    use kiln_doc::st::ThemeColorName;
    use kiln_ooxml::drawingml::color::DrawingColor;
    use kiln_ooxml::xml::parse_xml_str;

    fn el(xml: &str) -> XmlElement {
        let wrapped = xml.replacen(
            ' ',
            " xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
            1,
        );
        parse_xml_str(&wrapped).unwrap().root
    }

    #[test]
    fn body_props_round_trip() {
        let e = el(
            r#"<a:bodyPr rot="60000" vert="vert270" wrap="none" lIns="0" tIns="1" rIns="2" bIns="3" numCol="2" spcCol="91440" rtlCol="1" anchor="ctr" anchorCtr="1" upright="1"><a:prstTxWarp prst="textArchUp"><a:avLst/></a:prstTxWarp><a:normAutofit fontScale="92500" lnSpcReduction="10000"/></a:bodyPr>"#,
        );
        let b = parse_body_props(&e);
        assert_eq!(b.insets, [Some(0), Some(1), Some(2), Some(3)]);
        assert_eq!(b.anchor, Some(TextAnchor::Middle));
        assert_eq!(
            b.autofit,
            Some(Autofit::Shrink {
                font_scale: 92_500,
                line_reduction: 10_000
            })
        );
        assert_eq!(b.direction, Some(TextDirection::Rotate270));
        assert_eq!(b.wrap, Some(TextWrap::None));
        assert_eq!(b.warp.as_deref(), Some("textArchUp"));
        assert_eq!(parse_body_props(&body_props_to_element(&b)), b);
        for a in [Autofit::Off, Autofit::ResizeShape, Autofit::SHRINK] {
            let b = BodyProps {
                autofit: Some(a),
                ..BodyProps::default()
            };
            assert_eq!(parse_body_props(&body_props_to_element(&b)), b);
        }
    }

    #[test]
    fn paragraph_props_round_trip() {
        let e = el(
            r#"<a:lvl2pPr marL="685800" indent="-228600" algn="just" defTabSz="914400" rtl="0" eaLnBrk="1" fontAlgn="base" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPts val="500"/></a:spcBef><a:spcAft><a:spcPct val="20000"/></a:spcAft><a:buClr><a:schemeClr val="accent1"/></a:buClr><a:buSzPct val="80000"/><a:buFont typeface="Arial"/><a:buChar char="–"/><a:tabLst><a:tab pos="457200" algn="ctr"/></a:tabLst><a:defRPr sz="2000" b="1" u="dbl" strike="sngStrike" cap="small" spc="-50" baseline="30000" kern="1200" lang="en-GB"><a:solidFill><a:schemeClr val="tx1"><a:lumMod val="75000"/></a:schemeClr></a:solidFill><a:highlight><a:srgbClr val="FFFF00"/></a:highlight><a:latin typeface="+mj-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="Arial"/></a:defRPr></a:lvl2pPr>"#,
        );
        let l = parse_level(&e, None);
        assert_eq!(l.para.alignment, Some(Alignment::Both));
        let ind = l.para.indent.unwrap();
        assert_eq!(
            (ind.left, ind.hanging, ind.first_line),
            (Some(1080), Some(360), None)
        );
        assert_eq!(l.deck.line_spacing, Some(TextSpacing::Percent(90_000)));
        assert_eq!(l.deck.space_before, Some(TextSpacing::Points(500)));
        assert_eq!(l.deck.bullet, Some(Bullet::Char("–".into())));
        assert_eq!(l.deck.bullet_size, Some(BulletSize::Percent(80_000)));
        assert_eq!(l.deck.bullet_font, Some(BulletFont::Font("Arial".into())));
        assert_eq!(l.para.tabs.as_ref().unwrap()[0].pos, 720);
        assert_eq!(l.run.size, Some(40));
        assert_eq!(l.run.bold, Some(true));
        assert_eq!(l.run.small_caps, Some(true));
        assert_eq!(l.run.spacing, Some(-10));
        assert_eq!(l.run.vert_align, Some(VertAlign::Superscript));
        assert_eq!(l.run.kern, Some(24));
        assert_eq!(l.run.highlight, Some(HighlightColor::Yellow));
        let c = l.run.color.clone().unwrap();
        assert_eq!(c.theme_color, Some(ThemeColorName::Text1));
        assert_eq!(c.theme_shade, Some(191));
        let fonts = l.run.fonts.clone().unwrap();
        assert_eq!(fonts.h_ansi_theme, Some(ThemeFontRef::MajorHAnsi));
        assert_eq!(fonts.east_asia_theme, Some(ThemeFontRef::MinorEastAsia));
        assert_eq!(fonts.cs.as_deref(), Some("Arial"));
        // The model survives a write and a read.
        let again = parse_level(&level_to_element("a:lvl2pPr", &l), None);
        assert_eq!(again, l);
    }

    #[test]
    fn bullets_of_every_kind_round_trip() {
        for b in [
            Bullet::None,
            Bullet::Char("•".into()),
            Bullet::AutoNumber {
                scheme: "romanUcPeriod".into(),
                start_at: 3,
            },
            Bullet::AutoNumber {
                scheme: "arabicPeriod".into(),
                start_at: 1,
            },
            Bullet::Picture("rId4".into()),
        ] {
            for (color, size, font) in [
                (
                    Some(BulletColor::FollowText),
                    Some(BulletSize::FollowText),
                    Some(BulletFont::FollowText),
                ),
                (
                    Some(BulletColor::Color(DrawingColor::srgb("123456"))),
                    Some(BulletSize::Points(1200)),
                    Some(BulletFont::Font("+mj-lt".into())),
                ),
            ] {
                let d = DeckParaProps {
                    bullet: Some(b.clone()),
                    bullet_color: color,
                    bullet_size: size,
                    bullet_font: font,
                    font_align: Some(FontAlign::Center),
                    ..DeckParaProps::default()
                };
                let e = paragraph_props_to_element("a:pPr", &ParagraphProps::default(), &d, None);
                assert_eq!(parse_paragraph_props(&e).1, d);
            }
        }
    }

    #[test]
    fn list_styles_round_trip() {
        let mut s = ListStyle::default();
        s.set_default_level(Some(LevelStyle {
            run: RunProps {
                size: Some(36),
                color: Some(DocColor::theme(0, ThemeColorName::Text1).with_shade(128)),
                ..RunProps::default()
            },
            ..LevelStyle::default()
        }));
        s.set_level(
            0,
            LevelStyle {
                para: ParagraphProps {
                    indent: Some(Indent {
                        left: Some(0),
                        first_line: Some(0),
                        ..Indent::default()
                    }),
                    ..ParagraphProps::default()
                },
                deck: DeckParaProps {
                    bullet: Some(Bullet::None),
                    ..DeckParaProps::default()
                },
                run: RunProps {
                    caps: Some(true),
                    strike: Some(false),
                    dstrike: Some(false),
                    ..RunProps::default()
                },
            },
        );
        s.set_level(8, LevelStyle::default());
        let e = list_style_to_element("a:lstStyle", &s);
        let back = parse_list_style(&e, None);
        // Colours cache `auto` without a context; the theme reference and shade survive.
        let mut expect = s.clone();
        let mut d = expect.default_level().cloned().unwrap();
        d.run.color = Some(DocColor {
            val: kiln_doc::ColorValue::Auto,
            ..DocColor::theme(0, ThemeColorName::Text1).with_shade(128)
        });
        expect.set_default_level(Some(d));
        assert_eq!(back, expect);
    }

    #[test]
    fn unit_conversions_round() {
        assert_eq!(emu_to_twips(635), 1);
        assert_eq!(emu_to_twips(-952), -1);
        assert_eq!(emu_to_twips(i64::MAX), i32::MAX);
        assert_eq!(twips_to_emu(-2), -1270);
        let r = parse_run_props(
            &el(
                r#"<a:rPr sz="1050" strike="noStrike" cap="none" baseline="-25000"><a:rtl/></a:rPr>"#,
            ),
            None,
        );
        assert_eq!(r.size, Some(21));
        assert_eq!((r.strike, r.dstrike), (Some(false), Some(false)));
        assert_eq!((r.caps, r.small_caps), (Some(false), Some(false)));
        assert_eq!(r.vert_align, Some(VertAlign::Subscript));
        assert_eq!(r.rtl, Some(true));
        assert_eq!(parse_run_props(&run_props_to_element("a:rPr", &r), None), r);
        for u in UnderlineStyle::ALL.iter().copied() {
            assert_eq!(parse_underline(underline_str(u)), u, "{u:?}");
        }
    }
}
