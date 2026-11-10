// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML text properties (ECMA-376 Part 1 §21.1.2): `a:bodyPr`, `a:lstStyle`,
//! `a:pPr`/`a:lvlNpPr` and `a:rPr`/`a:defRPr`/`a:endParaRPr`. Unmodelled children are preserved in
//! `extras`, and attributes are written in Office's order.

use super::color::{DrawingColor, parse_color_child, serialize_color};
use super::effects::{EffectList, parse_effect_list, serialize_effect_list};
use super::fill::{FILL_ELEMENTS, Fill, parse_fill, serialize_fill};
use super::line::{Line, parse_line, serialize_line};
use crate::st::{
    TextAlignType, TextAnchoringType, TextAutonumberScheme, TextCapsType, TextFontAlignType,
    TextHorzOverflowType, TextStrikeType, TextTabAlignType, TextUnderlineType,
    TextVertOverflowType, TextVerticalType, TextWrappingType,
};
use crate::units::{parse_percentage, round_to_i64};
use crate::xml::XmlElement;

// ------------------------------------------------------------------ bodyPr

/// How text fits its shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Autofit {
    /// `a:noAutofit`.
    None,
    /// `a:spAutoFit`: the shape grows with its text.
    Shape,
    /// `a:normAutofit`: the text shrinks.
    Normal {
        /// Font scale (1 = 100 %).
        font_scale: f64,
        /// Line spacing reduction (0.1 = 10 %).
        ln_spc_reduction: f64,
    },
}

/// `a:bodyPr`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BodyProperties {
    /// Rotation, 60 000ths of a degree.
    pub rot: Option<i64>,
    /// `spcFirstLastPara`.
    pub spc_first_last_para: Option<bool>,
    /// `vertOverflow`.
    pub vert_overflow: Option<TextVertOverflowType>,
    /// `horzOverflow`.
    pub horz_overflow: Option<TextHorzOverflowType>,
    /// `vert`.
    pub vert: Option<TextVerticalType>,
    /// `wrap`.
    pub wrap: Option<TextWrappingType>,
    /// Left inset, EMU (91 440 when absent).
    pub l_ins: Option<i64>,
    /// Top inset, EMU (45 720 when absent).
    pub t_ins: Option<i64>,
    /// Right inset, EMU (91 440 when absent).
    pub r_ins: Option<i64>,
    /// Bottom inset, EMU (45 720 when absent).
    pub b_ins: Option<i64>,
    /// Number of columns.
    pub num_col: Option<i64>,
    /// Column spacing, EMU.
    pub spc_col: Option<i64>,
    /// Right-to-left columns.
    pub rtl_col: Option<bool>,
    /// `fromWordArt`.
    pub from_word_art: Option<bool>,
    /// Vertical anchor.
    pub anchor: Option<TextAnchoringType>,
    /// Centre the text block horizontally.
    pub anchor_ctr: Option<bool>,
    /// `forceAA`.
    pub force_aa: Option<bool>,
    /// `upright`.
    pub upright: Option<bool>,
    /// `compatLnSpc`.
    pub compat_ln_spc: Option<bool>,
    /// Autofit.
    pub autofit: Option<Autofit>,
    /// `a:prstTxWarp`, `a:scene3d`, `a:sp3d`, `a:flatTx`, `a:extLst`, preserved.
    pub extras: Vec<XmlElement>,
}

/// Office's default body insets in EMU: left, top, right, bottom.
pub const DEFAULT_INSETS: [i64; 4] = [91_440, 45_720, 91_440, 45_720];

fn opt_enum<T>(node: &XmlElement, k: &str, parse: fn(&str) -> Option<T>) -> Option<T> {
    node.attr(k).and_then(parse)
}

/// Parses `a:bodyPr`.
#[must_use]
pub fn parse_body_properties(node: &XmlElement) -> BodyProperties {
    let mut bp = BodyProperties {
        rot: node.attr_int("rot"),
        spc_first_last_para: node.attr_bool("spcFirstLastPara"),
        vert_overflow: opt_enum(node, "vertOverflow", TextVertOverflowType::parse),
        horz_overflow: opt_enum(node, "horzOverflow", TextHorzOverflowType::parse),
        vert: opt_enum(node, "vert", TextVerticalType::parse),
        wrap: opt_enum(node, "wrap", TextWrappingType::parse),
        l_ins: node.attr_int("lIns"),
        t_ins: node.attr_int("tIns"),
        r_ins: node.attr_int("rIns"),
        b_ins: node.attr_int("bIns"),
        num_col: node.attr_int("numCol"),
        spc_col: node.attr_int("spcCol"),
        rtl_col: node.attr_bool("rtlCol"),
        from_word_art: node.attr_bool("fromWordArt"),
        anchor: opt_enum(node, "anchor", TextAnchoringType::parse),
        anchor_ctr: node.attr_bool("anchorCtr"),
        force_aa: node.attr_bool("forceAA"),
        upright: node.attr_bool("upright"),
        compat_ln_spc: node.attr_bool("compatLnSpc"),
        autofit: None,
        extras: Vec::new(),
    };
    for c in node.elements() {
        match c.name.as_str() {
            "a:noAutofit" => bp.autofit = Some(Autofit::None),
            "a:spAutoFit" => bp.autofit = Some(Autofit::Shape),
            "a:normAutofit" => {
                bp.autofit = Some(Autofit::Normal {
                    font_scale: c
                        .attr("fontScale")
                        .and_then(parse_percentage)
                        .unwrap_or(1.0),
                    ln_spc_reduction: c
                        .attr("lnSpcReduction")
                        .and_then(parse_percentage)
                        .unwrap_or(0.0),
                });
            }
            _ => bp.extras.push(c.clone()),
        }
    }
    bp
}

/// Serialises `a:bodyPr` (attributes in Office's order; `a:prstTxWarp` before the autofit
/// element, other extras after it).
#[must_use]
pub fn serialize_body_properties(bp: &BodyProperties) -> XmlElement {
    let warp = bp
        .extras
        .iter()
        .filter(|x| x.name == "a:prstTxWarp")
        .cloned();
    let rest = bp
        .extras
        .iter()
        .filter(|x| x.name != "a:prstTxWarp")
        .cloned();
    let autofit = bp.autofit.map(|af| match af {
        Autofit::None => XmlElement::new("a:noAutofit"),
        Autofit::Shape => XmlElement::new("a:spAutoFit"),
        Autofit::Normal {
            font_scale,
            ln_spc_reduction,
        } => XmlElement::new("a:normAutofit")
            .with_opt_attr(
                "fontScale",
                (font_scale != 1.0).then(|| round_to_i64(font_scale * 100_000.0)),
            )
            .with_opt_attr(
                "lnSpcReduction",
                (ln_spc_reduction != 0.0).then(|| round_to_i64(ln_spc_reduction * 100_000.0)),
            ),
    });
    XmlElement::new("a:bodyPr")
        .with_opt_attr("rot", bp.rot)
        .with_opt_attr("spcFirstLastPara", bp.spc_first_last_para)
        .with_opt_attr("vertOverflow", bp.vert_overflow)
        .with_opt_attr("horzOverflow", bp.horz_overflow)
        .with_opt_attr("vert", bp.vert)
        .with_opt_attr("wrap", bp.wrap)
        .with_opt_attr("lIns", bp.l_ins)
        .with_opt_attr("tIns", bp.t_ins)
        .with_opt_attr("rIns", bp.r_ins)
        .with_opt_attr("bIns", bp.b_ins)
        .with_opt_attr("numCol", bp.num_col)
        .with_opt_attr("spcCol", bp.spc_col)
        .with_opt_attr("rtlCol", bp.rtl_col)
        .with_opt_attr("fromWordArt", bp.from_word_art)
        .with_opt_attr("anchor", bp.anchor)
        .with_opt_attr("anchorCtr", bp.anchor_ctr)
        .with_opt_attr("forceAA", bp.force_aa)
        .with_opt_attr("upright", bp.upright)
        .with_opt_attr("compatLnSpc", bp.compat_ln_spc)
        .with_children(warp)
        .with_opt_child(autofit)
        .with_children(rest)
}

// ------------------------------------------------------------------- rPr

/// A font reference (`a:latin`, `a:ea`, `a:cs`, `a:sym`, `a:buFont`, theme fonts).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextFont {
    /// The typeface, or a theme reference such as `+mj-lt`.
    pub typeface: String,
    /// PANOSE classification (hex).
    pub panose: Option<String>,
    /// `pitchFamily`.
    pub pitch_family: Option<String>,
    /// `charset`.
    pub charset: Option<String>,
}

impl TextFont {
    /// A font by typeface.
    #[must_use]
    pub fn new(typeface: &str) -> Self {
        Self {
            typeface: typeface.to_owned(),
            ..Self::default()
        }
    }

    /// Reads a font element (missing `typeface` reads as empty).
    #[must_use]
    pub fn parse(node: &XmlElement) -> Self {
        let opt = |k: &str| node.attr(k).map(str::to_owned);
        Self {
            typeface: node.attr("typeface").unwrap_or("").to_owned(),
            panose: opt("panose"),
            pitch_family: opt("pitchFamily"),
            charset: opt("charset"),
        }
    }

    /// The font as an element named `name`.
    #[must_use]
    pub fn to_element(&self, name: &str) -> XmlElement {
        XmlElement::new(name)
            .with_attr("typeface", self.typeface.as_str())
            .with_opt_attr("panose", self.panose.as_deref())
            .with_opt_attr("pitchFamily", self.pitch_family.as_deref())
            .with_opt_attr("charset", self.charset.as_deref())
    }
}

/// `a:rPr`, `a:defRPr` or `a:endParaRPr`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunProperties {
    /// `kumimoji`.
    pub kumimoji: Option<bool>,
    /// Language (`en-GB`).
    pub lang: Option<String>,
    /// Alternative language.
    pub alt_lang: Option<String>,
    /// Font size in hundredths of a point.
    pub sz: Option<i64>,
    /// Bold.
    pub b: Option<bool>,
    /// Italic.
    pub i: Option<bool>,
    /// Underline.
    pub u: Option<TextUnderlineType>,
    /// Strikethrough.
    pub strike: Option<TextStrikeType>,
    /// Minimum font size for kerning, hundredths of a point.
    pub kern: Option<i64>,
    /// Capitalisation.
    pub cap: Option<TextCapsType>,
    /// Character spacing, hundredths of a point.
    pub spc: Option<i64>,
    /// `normalizeH`.
    pub normalize_h: Option<bool>,
    /// Superscript/subscript offset, thousandths of a percent (30 000 = superscript).
    pub baseline: Option<i64>,
    /// `noProof`.
    pub no_proof: Option<bool>,
    /// `dirty`.
    pub dirty: Option<bool>,
    /// `err`.
    pub err: Option<bool>,
    /// `smtClean`.
    pub smt_clean: Option<bool>,
    /// `smtId`.
    pub smt_id: Option<i64>,
    /// `bmk`.
    pub bmk: Option<String>,
    /// Text outline.
    pub line: Option<Line>,
    /// Text fill.
    pub fill: Option<Fill>,
    /// Text effects.
    pub effects: Option<EffectList>,
    /// Highlight colour.
    pub highlight: Option<DrawingColor>,
    /// Latin font.
    pub latin: Option<TextFont>,
    /// East Asian font.
    pub ea: Option<TextFont>,
    /// Complex-script font.
    pub cs: Option<TextFont>,
    /// Symbol font.
    pub sym: Option<TextFont>,
    /// `a:hlinkClick` / `a:hlinkMouseOver`, preserved (relationship ids, actions).
    pub hyperlinks: Vec<XmlElement>,
    /// `a:rtl`.
    pub rtl: Option<bool>,
    /// `a:uLnTx`, `a:uLn`, `a:uFillTx`, `a:uFill`, `a:extLst`, …, preserved.
    pub extras: Vec<XmlElement>,
}

/// Parses `a:rPr`, `a:defRPr` or `a:endParaRPr`.
#[must_use]
pub fn parse_run_properties(node: &XmlElement) -> RunProperties {
    let s = |k: &str| node.attr(k).map(str::to_owned);
    let mut rp = RunProperties {
        kumimoji: node.attr_bool("kumimoji"),
        lang: s("lang"),
        alt_lang: s("altLang"),
        sz: node.attr_int("sz"),
        b: node.attr_bool("b"),
        i: node.attr_bool("i"),
        u: opt_enum(node, "u", TextUnderlineType::parse),
        strike: opt_enum(node, "strike", TextStrikeType::parse),
        kern: node.attr_int("kern"),
        cap: opt_enum(node, "cap", TextCapsType::parse),
        spc: node.attr_int("spc"),
        normalize_h: node.attr_bool("normalizeH"),
        baseline: node.attr_int("baseline"),
        no_proof: node.attr_bool("noProof"),
        dirty: node.attr_bool("dirty"),
        err: node.attr_bool("err"),
        smt_clean: node.attr_bool("smtClean"),
        smt_id: node.attr_int("smtId"),
        bmk: s("bmk"),
        ..RunProperties::default()
    };
    for c in node.elements() {
        if FILL_ELEMENTS.contains(&c.name.as_str()) {
            if let Some(f) = parse_fill(c) {
                rp.fill = Some(f);
            }
            continue;
        }
        match c.name.as_str() {
            "a:ln" => rp.line = Some(parse_line(c)),
            "a:effectLst" => rp.effects = Some(parse_effect_list(c)),
            "a:highlight" => {
                if let Some(h) = parse_color_child(Some(c)) {
                    rp.highlight = Some(h);
                }
            }
            "a:latin" => rp.latin = Some(TextFont::parse(c)),
            "a:ea" => rp.ea = Some(TextFont::parse(c)),
            "a:cs" => rp.cs = Some(TextFont::parse(c)),
            "a:sym" => rp.sym = Some(TextFont::parse(c)),
            "a:hlinkClick" | "a:hlinkMouseOver" => rp.hyperlinks.push(c.clone()),
            "a:rtl" => rp.rtl = Some(c.attr_bool("val").unwrap_or(true)),
            _ => rp.extras.push(c.clone()),
        }
    }
    rp
}

/// Serialises run properties as `name` (`a:rPr`, `a:defRPr`, `a:endParaRPr`) in schema order:
/// ln, fill, effects, highlight, underline extras, latin, ea, cs, sym, hyperlinks, rtl, extLst.
#[must_use]
pub fn serialize_run_properties(rp: &RunProperties, name: &str) -> XmlElement {
    let font = |n: &str, f: &Option<TextFont>| f.as_ref().map(|f| f.to_element(n));
    XmlElement::new(name)
        .with_opt_attr("kumimoji", rp.kumimoji)
        .with_opt_attr("lang", rp.lang.as_deref())
        .with_opt_attr("altLang", rp.alt_lang.as_deref())
        .with_opt_attr("sz", rp.sz)
        .with_opt_attr("b", rp.b)
        .with_opt_attr("i", rp.i)
        .with_opt_attr("u", rp.u)
        .with_opt_attr("strike", rp.strike)
        .with_opt_attr("kern", rp.kern)
        .with_opt_attr("cap", rp.cap)
        .with_opt_attr("spc", rp.spc)
        .with_opt_attr("normalizeH", rp.normalize_h)
        .with_opt_attr("baseline", rp.baseline)
        .with_opt_attr("noProof", rp.no_proof)
        .with_opt_attr("dirty", rp.dirty)
        .with_opt_attr("err", rp.err)
        .with_opt_attr("smtClean", rp.smt_clean)
        .with_opt_attr("smtId", rp.smt_id)
        .with_opt_attr("bmk", rp.bmk.as_deref())
        .with_opt_child(rp.line.as_ref().map(serialize_line))
        .with_opt_child(rp.fill.as_ref().map(serialize_fill))
        .with_opt_child(rp.effects.as_ref().map(serialize_effect_list))
        .with_opt_child(
            rp.highlight
                .as_ref()
                .map(|h| XmlElement::new("a:highlight").with_child(serialize_color(h))),
        )
        .with_children(rp.extras.iter().filter(|x| x.name != "a:extLst").cloned())
        .with_opt_child(font("a:latin", &rp.latin))
        .with_opt_child(font("a:ea", &rp.ea))
        .with_opt_child(font("a:cs", &rp.cs))
        .with_opt_child(font("a:sym", &rp.sym))
        .with_children(rp.hyperlinks.iter().cloned())
        .with_opt_child(
            rp.rtl
                .map(|r| XmlElement::new("a:rtl").with_opt_attr("val", (!r).then_some("0"))),
        )
        .with_children(rp.extras.iter().filter(|x| x.name == "a:extLst").cloned())
}

// ------------------------------------------------------------------- pPr

/// Line or paragraph spacing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextSpacing {
    /// A fraction of single spacing (`a:spcPct`, 1 = 100 %).
    Percent(f64),
    /// An absolute size in hundredths of a point (`a:spcPts`).
    Points(i64),
}

/// A bullet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bullet {
    /// `a:buNone`.
    None,
    /// `a:buChar`.
    Char(String),
    /// `a:buAutoNum`.
    AutoNum {
        /// The numbering scheme.
        scheme: TextAutonumberScheme,
        /// The first number.
        start_at: Option<i64>,
    },
    /// `a:buBlip`: a picture bullet.
    Blip {
        /// Relationship id of the picture.
        embed: Option<String>,
    },
}

/// Bullet colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulletColor {
    /// `a:buClrTx`: follows the text.
    FollowText,
    /// `a:buClr`.
    Color(DrawingColor),
}

/// Bullet size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BulletSize {
    /// `a:buSzTx`: follows the text.
    FollowText,
    /// `a:buSzPct` (1 = 100 %).
    Percent(f64),
    /// `a:buSzPts`, hundredths of a point.
    Points(i64),
}

/// Bullet font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulletFont {
    /// `a:buFontTx`: follows the text.
    FollowText,
    /// `a:buFont`.
    Font(TextFont),
}

/// A tab stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabStop {
    /// Position, EMU.
    pub pos: i64,
    /// Alignment.
    pub algn: TextTabAlignType,
}

/// `a:pPr` or a list-style level (`a:lvl1pPr` … `a:lvl9pPr`, `a:defPPr`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParagraphProperties {
    /// Left margin, EMU.
    pub mar_l: Option<i64>,
    /// Right margin, EMU.
    pub mar_r: Option<i64>,
    /// Outline level 0–8.
    pub lvl: Option<i64>,
    /// First-line indent, EMU (negative = hanging).
    pub indent: Option<i64>,
    /// Alignment.
    pub algn: Option<TextAlignType>,
    /// Default tab size, EMU.
    pub def_tab_sz: Option<i64>,
    /// Right-to-left.
    pub rtl: Option<bool>,
    /// `eaLnBrk`.
    pub ea_ln_brk: Option<bool>,
    /// `fontAlgn`.
    pub font_algn: Option<TextFontAlignType>,
    /// `latinLnBrk`.
    pub latin_ln_brk: Option<bool>,
    /// `hangingPunct`.
    pub hanging_punct: Option<bool>,
    /// Line spacing.
    pub ln_spc: Option<TextSpacing>,
    /// Space before.
    pub spc_bef: Option<TextSpacing>,
    /// Space after.
    pub spc_aft: Option<TextSpacing>,
    /// Bullet colour.
    pub bu_clr: Option<BulletColor>,
    /// Bullet size.
    pub bu_sz: Option<BulletSize>,
    /// Bullet font.
    pub bu_font: Option<BulletFont>,
    /// The bullet.
    pub bullet: Option<Bullet>,
    /// Tab stops.
    pub tabs: Option<Vec<TabStop>>,
    /// Default run properties.
    pub def_rpr: Option<Box<RunProperties>>,
    /// Other children, preserved.
    pub extras: Vec<XmlElement>,
}

fn parse_spacing(node: &XmlElement) -> Option<TextSpacing> {
    if let Some(p) = node.child("a:spcPct") {
        return Some(TextSpacing::Percent(
            p.attr("val").and_then(parse_percentage).unwrap_or(1.0),
        ));
    }
    node.child("a:spcPts")
        .map(|p| TextSpacing::Points(p.attr_int("val").unwrap_or(0)))
}

fn spacing_element(name: &str, s: Option<TextSpacing>) -> Option<XmlElement> {
    s.map(|s| {
        XmlElement::new(name).with_child(match s {
            TextSpacing::Percent(p) => {
                XmlElement::new("a:spcPct").with_attr("val", round_to_i64(p * 100_000.0))
            }
            TextSpacing::Points(v) => XmlElement::new("a:spcPts").with_attr("val", v),
        })
    })
}

/// Parses `a:pPr` or a list-style level.
#[must_use]
pub fn parse_paragraph_properties(node: &XmlElement) -> ParagraphProperties {
    let mut pp = ParagraphProperties {
        mar_l: node.attr_int("marL"),
        mar_r: node.attr_int("marR"),
        lvl: node.attr_int("lvl"),
        indent: node.attr_int("indent"),
        algn: opt_enum(node, "algn", TextAlignType::parse),
        def_tab_sz: node.attr_int("defTabSz"),
        rtl: node.attr_bool("rtl"),
        ea_ln_brk: node.attr_bool("eaLnBrk"),
        font_algn: opt_enum(node, "fontAlgn", TextFontAlignType::parse),
        latin_ln_brk: node.attr_bool("latinLnBrk"),
        hanging_punct: node.attr_bool("hangingPunct"),
        ..ParagraphProperties::default()
    };
    for c in node.elements() {
        match c.name.as_str() {
            "a:lnSpc" => pp.ln_spc = parse_spacing(c),
            "a:spcBef" => pp.spc_bef = parse_spacing(c),
            "a:spcAft" => pp.spc_aft = parse_spacing(c),
            "a:buClrTx" => pp.bu_clr = Some(BulletColor::FollowText),
            "a:buClr" => {
                if let Some(col) = parse_color_child(Some(c)) {
                    pp.bu_clr = Some(BulletColor::Color(col));
                }
            }
            "a:buSzTx" => pp.bu_sz = Some(BulletSize::FollowText),
            "a:buSzPct" => {
                pp.bu_sz = Some(BulletSize::Percent(
                    c.attr("val").and_then(parse_percentage).unwrap_or(1.0),
                ));
            }
            "a:buSzPts" => pp.bu_sz = Some(BulletSize::Points(c.attr_int("val").unwrap_or(0))),
            "a:buFontTx" => pp.bu_font = Some(BulletFont::FollowText),
            "a:buFont" => pp.bu_font = Some(BulletFont::Font(TextFont::parse(c))),
            "a:buNone" => pp.bullet = Some(Bullet::None),
            "a:buChar" => pp.bullet = Some(Bullet::Char(c.attr("char").unwrap_or("•").to_owned())),
            "a:buAutoNum" => {
                pp.bullet = Some(Bullet::AutoNum {
                    scheme: c
                        .attr("type")
                        .and_then(TextAutonumberScheme::parse)
                        .unwrap_or(TextAutonumberScheme::ArabicPeriod),
                    start_at: c.attr_int("startAt"),
                });
            }
            "a:buBlip" => {
                pp.bullet = Some(Bullet::Blip {
                    embed: c
                        .child("a:blip")
                        .and_then(|b| b.attr("r:embed"))
                        .filter(|e| !e.is_empty())
                        .map(str::to_owned),
                });
            }
            "a:tabLst" => {
                pp.tabs = Some(
                    c.children_named("a:tab")
                        .map(|t| TabStop {
                            pos: t.attr_int("pos").unwrap_or(0),
                            algn: t
                                .attr("algn")
                                .and_then(TextTabAlignType::parse)
                                .unwrap_or(TextTabAlignType::L),
                        })
                        .collect(),
                );
            }
            "a:defRPr" => pp.def_rpr = Some(Box::new(parse_run_properties(c))),
            _ => pp.extras.push(c.clone()),
        }
    }
    pp
}

/// Serialises paragraph properties as `name` (`a:pPr`, `a:lvl1pPr`, `a:defPPr`, …).
#[must_use]
pub fn serialize_paragraph_properties(pp: &ParagraphProperties, name: &str) -> XmlElement {
    let bu_clr = pp.bu_clr.as_ref().map(|c| match c {
        BulletColor::FollowText => XmlElement::new("a:buClrTx"),
        BulletColor::Color(col) => XmlElement::new("a:buClr").with_child(serialize_color(col)),
    });
    let bu_sz = pp.bu_sz.map(|s| match s {
        BulletSize::FollowText => XmlElement::new("a:buSzTx"),
        BulletSize::Percent(p) => {
            XmlElement::new("a:buSzPct").with_attr("val", round_to_i64(p * 100_000.0))
        }
        BulletSize::Points(v) => XmlElement::new("a:buSzPts").with_attr("val", v),
    });
    let bu_font = pp.bu_font.as_ref().map(|f| match f {
        BulletFont::FollowText => XmlElement::new("a:buFontTx"),
        BulletFont::Font(f) => f.to_element("a:buFont"),
    });
    let bullet = pp.bullet.as_ref().map(|b| match b {
        Bullet::None => XmlElement::new("a:buNone"),
        Bullet::AutoNum { scheme, start_at } => XmlElement::new("a:buAutoNum")
            .with_attr("type", *scheme)
            .with_opt_attr("startAt", *start_at),
        Bullet::Char(ch) => XmlElement::new("a:buChar").with_attr("char", ch.as_str()),
        Bullet::Blip { embed } => XmlElement::new("a:buBlip")
            .with_child(XmlElement::new("a:blip").with_opt_attr("r:embed", embed.as_deref())),
    });
    let tabs = pp.tabs.as_ref().map(|tabs| {
        XmlElement::new("a:tabLst").with_children(tabs.iter().map(|t| {
            XmlElement::new("a:tab")
                .with_attr("pos", t.pos)
                .with_attr("algn", t.algn)
        }))
    });
    XmlElement::new(name)
        .with_opt_attr("marL", pp.mar_l)
        .with_opt_attr("marR", pp.mar_r)
        .with_opt_attr("lvl", pp.lvl)
        .with_opt_attr("indent", pp.indent)
        .with_opt_attr("algn", pp.algn)
        .with_opt_attr("defTabSz", pp.def_tab_sz)
        .with_opt_attr("rtl", pp.rtl)
        .with_opt_attr("eaLnBrk", pp.ea_ln_brk)
        .with_opt_attr("fontAlgn", pp.font_algn)
        .with_opt_attr("latinLnBrk", pp.latin_ln_brk)
        .with_opt_attr("hangingPunct", pp.hanging_punct)
        .with_opt_child(spacing_element("a:lnSpc", pp.ln_spc))
        .with_opt_child(spacing_element("a:spcBef", pp.spc_bef))
        .with_opt_child(spacing_element("a:spcAft", pp.spc_aft))
        .with_opt_child(bu_clr)
        .with_opt_child(bu_sz)
        .with_opt_child(bu_font)
        .with_opt_child(bullet)
        .with_opt_child(tabs)
        .with_opt_child(
            pp.def_rpr
                .as_deref()
                .map(|r| serialize_run_properties(r, "a:defRPr")),
        )
        .with_children(pp.extras.iter().cloned())
}

// -------------------------------------------------------------- lstStyle

/// `a:lstStyle` (and the master text styles that share its shape).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListStyle {
    /// `a:defPPr`.
    pub def: Option<ParagraphProperties>,
    /// `a:lvl1pPr` … `a:lvl9pPr` at indexes 0–8.
    pub levels: [Option<ParagraphProperties>; 9],
    /// `a:extLst`, preserved.
    pub ext_lst: Option<XmlElement>,
}

const LEVEL_NAMES: [&str; 9] = [
    "a:lvl1pPr",
    "a:lvl2pPr",
    "a:lvl3pPr",
    "a:lvl4pPr",
    "a:lvl5pPr",
    "a:lvl6pPr",
    "a:lvl7pPr",
    "a:lvl8pPr",
    "a:lvl9pPr",
];

/// Parses `a:lstStyle`.
#[must_use]
pub fn parse_list_style(node: &XmlElement) -> ListStyle {
    let mut ls = ListStyle {
        def: node.child("a:defPPr").map(parse_paragraph_properties),
        ext_lst: node.child("a:extLst").cloned(),
        ..ListStyle::default()
    };
    for (slot, name) in ls.levels.iter_mut().zip(LEVEL_NAMES) {
        *slot = node.child(name).map(parse_paragraph_properties);
    }
    ls
}

/// Serialises a list style as `name`.
#[must_use]
pub fn serialize_list_style(ls: &ListStyle, name: &str) -> XmlElement {
    XmlElement::new(name)
        .with_opt_child(
            ls.def
                .as_ref()
                .map(|d| serialize_paragraph_properties(d, "a:defPPr")),
        )
        .with_children(
            ls.levels
                .iter()
                .zip(LEVEL_NAMES)
                .filter_map(|(l, n)| l.as_ref().map(|l| serialize_paragraph_properties(l, n))),
        )
        .with_opt_child(ls.ext_lst.clone())
}

// ---------------------------------------------------------- inheritance

fn pick<T: Clone>(over: &Option<T>, base: &Option<T>) -> Option<T> {
    over.clone().or_else(|| base.clone())
}

fn pick_vec(over: &[XmlElement], base: &[XmlElement]) -> Vec<XmlElement> {
    if over.is_empty() { base } else { over }.to_vec()
}

/// Merges run properties: values in `over` win and `base` fills the gaps (master → layout →
/// slide → paragraph → run inheritance). Preserved children come from `over` when it has any.
#[must_use]
pub fn merge_run_properties(
    base: Option<&RunProperties>,
    over: Option<&RunProperties>,
) -> RunProperties {
    let (b, o) = match (base, over) {
        (None, None) => return RunProperties::default(),
        (None, Some(o)) => return o.clone(),
        (Some(b), None) => return b.clone(),
        (Some(b), Some(o)) => (b, o),
    };
    RunProperties {
        kumimoji: o.kumimoji.or(b.kumimoji),
        lang: pick(&o.lang, &b.lang),
        alt_lang: pick(&o.alt_lang, &b.alt_lang),
        sz: o.sz.or(b.sz),
        b: o.b.or(b.b),
        i: o.i.or(b.i),
        u: o.u.or(b.u),
        strike: o.strike.or(b.strike),
        kern: o.kern.or(b.kern),
        cap: o.cap.or(b.cap),
        spc: o.spc.or(b.spc),
        normalize_h: o.normalize_h.or(b.normalize_h),
        baseline: o.baseline.or(b.baseline),
        no_proof: o.no_proof.or(b.no_proof),
        dirty: o.dirty.or(b.dirty),
        err: o.err.or(b.err),
        smt_clean: o.smt_clean.or(b.smt_clean),
        smt_id: o.smt_id.or(b.smt_id),
        bmk: pick(&o.bmk, &b.bmk),
        line: pick(&o.line, &b.line),
        fill: pick(&o.fill, &b.fill),
        effects: pick(&o.effects, &b.effects),
        highlight: pick(&o.highlight, &b.highlight),
        latin: pick(&o.latin, &b.latin),
        ea: pick(&o.ea, &b.ea),
        cs: pick(&o.cs, &b.cs),
        sym: pick(&o.sym, &b.sym),
        hyperlinks: pick_vec(&o.hyperlinks, &b.hyperlinks),
        rtl: o.rtl.or(b.rtl),
        extras: pick_vec(&o.extras, &b.extras),
    }
}

/// Merges paragraph properties the same way (`def_rpr` merged recursively).
#[must_use]
pub fn merge_paragraph_properties(
    base: Option<&ParagraphProperties>,
    over: Option<&ParagraphProperties>,
) -> ParagraphProperties {
    let (b, o) = match (base, over) {
        (None, None) => return ParagraphProperties::default(),
        (None, Some(o)) => return o.clone(),
        (Some(b), None) => return b.clone(),
        (Some(b), Some(o)) => (b, o),
    };
    let def_rpr = match (&b.def_rpr, &o.def_rpr) {
        (None, None) => None,
        (bd, od) => Some(Box::new(merge_run_properties(bd.as_deref(), od.as_deref()))),
    };
    ParagraphProperties {
        mar_l: o.mar_l.or(b.mar_l),
        mar_r: o.mar_r.or(b.mar_r),
        lvl: o.lvl.or(b.lvl),
        indent: o.indent.or(b.indent),
        algn: o.algn.or(b.algn),
        def_tab_sz: o.def_tab_sz.or(b.def_tab_sz),
        rtl: o.rtl.or(b.rtl),
        ea_ln_brk: o.ea_ln_brk.or(b.ea_ln_brk),
        font_algn: o.font_algn.or(b.font_algn),
        latin_ln_brk: o.latin_ln_brk.or(b.latin_ln_brk),
        hanging_punct: o.hanging_punct.or(b.hanging_punct),
        ln_spc: o.ln_spc.or(b.ln_spc),
        spc_bef: o.spc_bef.or(b.spc_bef),
        spc_aft: o.spc_aft.or(b.spc_aft),
        bu_clr: pick(&o.bu_clr, &b.bu_clr),
        bu_sz: o.bu_sz.or(b.bu_sz),
        bu_font: pick(&o.bu_font, &b.bu_font),
        bullet: pick(&o.bullet, &b.bullet),
        tabs: pick(&o.tabs, &b.tabs),
        def_rpr,
        extras: pick_vec(&o.extras, &b.extras),
    }
}
