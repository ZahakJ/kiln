// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML themes, `a:theme` (ECMA-376 Part 1 §20.1.6): colour scheme, font scheme and format
//! scheme (the style matrix). Unmodelled children (`a:objectDefaults`, `a:extraClrSchemeLst`,
//! `a:custClrLst`, `a:extLst`, scene and 3-D properties inside effect styles) are preserved.

use super::color::{
    ColorContext, ColorMap, ColorModel, ColorTransform, DrawingColor, Rgba, parse_color_child,
    resolve_color, serialize_color,
};
use super::effects::{EffectList, OuterShadow, parse_effect_list, serialize_effect_list};
use super::fill::{Fill, GradientFill, GradientStop, LinearShade, parse_fill, serialize_fill};
use super::line::{Line, LineDash, LineJoin, parse_line, serialize_line};
use super::text::TextFont;
use crate::st::{
    ColorSchemeIndex, CompoundLine, LineCap, PenAlignment, PresetLineDashVal, RectAlignment,
};
use crate::xml::{XmlDocument, XmlElement, XmlParseError, namespaces::ns, parse_xml};

/// A font collection (`a:majorFont`, `a:minorFont`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FontCollection {
    /// Latin font.
    pub latin: TextFont,
    /// East Asian font.
    pub ea: TextFont,
    /// Complex-script font.
    pub cs: TextFont,
    /// Per-script fonts (`<a:font script="Jpan" typeface="游ゴシック Light"/>`): (script,
    /// typeface).
    pub scripts: Vec<(String, String)>,
    /// `a:extLst`, preserved.
    pub ext_lst: Option<XmlElement>,
}

/// An effect style of the format scheme.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectStyle {
    /// The effect list.
    pub effects: Option<EffectList>,
    /// `a:effectDag`, `a:scene3d`, `a:sp3d`, preserved.
    pub extras: Vec<XmlElement>,
}

/// `a:clrScheme`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorScheme {
    /// The scheme name.
    pub name: String,
    /// The twelve slots, indexed by [`ColorSchemeIndex`].
    pub colors: [DrawingColor; 12],
}

impl ColorScheme {
    /// The colour of a slot.
    #[must_use]
    pub fn get(&self, slot: ColorSchemeIndex) -> &DrawingColor {
        &self.colors[slot.index()]
    }
}

/// `a:fontScheme`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FontScheme {
    /// The scheme name.
    pub name: String,
    /// Headings.
    pub major: FontCollection,
    /// Body.
    pub minor: FontCollection,
}

/// `a:fmtScheme`: the style matrix.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FormatScheme {
    /// The scheme name.
    pub name: String,
    /// `a:fillStyleLst`.
    pub fill_styles: Vec<Fill>,
    /// `a:lnStyleLst`.
    pub line_styles: Vec<Line>,
    /// `a:effectStyleLst`.
    pub effect_styles: Vec<EffectStyle>,
    /// `a:bgFillStyleLst`.
    pub bg_fill_styles: Vec<Fill>,
}

/// A theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// The theme name.
    pub name: String,
    /// Colours.
    pub color_scheme: ColorScheme,
    /// Fonts.
    pub font_scheme: FontScheme,
    /// Fills, lines and effects.
    pub format_scheme: FormatScheme,
    /// Children of `a:theme` after `a:themeElements`, preserved verbatim.
    pub extras: Vec<XmlElement>,
    /// Children of `a:themeElements` after `a:fmtScheme` (such as `a:extLst`).
    pub element_extras: Vec<XmlElement>,
}

/// A part that is not a theme.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    /// The XML is malformed.
    #[error(transparent)]
    Xml(#[from] XmlParseError),
    /// The root element is not `a:theme`.
    #[error("expected a:theme, found {0}")]
    NotATheme(String),
}

fn parse_font_collection(node: Option<&XmlElement>) -> FontCollection {
    let Some(node) = node else {
        return FontCollection::default();
    };
    let font = |n: &str| node.child(n).map(TextFont::parse).unwrap_or_default();
    FontCollection {
        latin: font("a:latin"),
        ea: font("a:ea"),
        cs: font("a:cs"),
        scripts: node
            .children_named("a:font")
            .map(|f| {
                (
                    f.attr("script").unwrap_or("").to_owned(),
                    f.attr("typeface").unwrap_or("").to_owned(),
                )
            })
            .collect(),
        ext_lst: node.child("a:extLst").cloned(),
    }
}

/// Parses a theme part.
///
/// # Errors
/// [`ThemeError`] for malformed XML or another root element.
pub fn parse_theme_bytes(bytes: &[u8]) -> Result<Theme, ThemeError> {
    parse_theme(&parse_xml(bytes)?.root)
}

/// Reads an `a:theme` element.
///
/// # Errors
/// [`ThemeError::NotATheme`] for another element.
pub fn parse_theme(root: &XmlElement) -> Result<Theme, ThemeError> {
    if root.name != "a:theme" {
        return Err(ThemeError::NotATheme(root.name.clone()));
    }
    let te = root.child("a:themeElements");
    let cs = te.and_then(|t| t.child("a:clrScheme"));
    let colors: [DrawingColor; 12] = std::array::from_fn(|i| {
        let slot = ColorSchemeIndex::ALL[i];
        parse_color_child(cs.and_then(|c| c.child(&format!("a:{slot}")))).unwrap_or_else(|| {
            DrawingColor::srgb(
                if matches!(slot, ColorSchemeIndex::Lt1 | ColorSchemeIndex::Lt2) {
                    "FFFFFF"
                } else {
                    "000000"
                },
            )
        })
    });
    let fs = te.and_then(|t| t.child("a:fontScheme"));
    let fmt = te.and_then(|t| t.child("a:fmtScheme"));
    let list = |name: &str| fmt.and_then(|f| f.child(name));
    let fills = |name: &str| -> Vec<Fill> {
        list(name)
            .map(|l| l.elements().filter_map(parse_fill).collect())
            .unwrap_or_default()
    };
    const KNOWN: [&str; 3] = ["a:clrScheme", "a:fontScheme", "a:fmtScheme"];
    Ok(Theme {
        name: root.attr("name").unwrap_or("").to_owned(),
        color_scheme: ColorScheme {
            name: cs.and_then(|c| c.attr("name")).unwrap_or("").to_owned(),
            colors,
        },
        font_scheme: FontScheme {
            name: fs.and_then(|f| f.attr("name")).unwrap_or("").to_owned(),
            major: parse_font_collection(fs.and_then(|f| f.child("a:majorFont"))),
            minor: parse_font_collection(fs.and_then(|f| f.child("a:minorFont"))),
        },
        format_scheme: FormatScheme {
            name: fmt.and_then(|f| f.attr("name")).unwrap_or("").to_owned(),
            fill_styles: fills("a:fillStyleLst"),
            line_styles: list("a:lnStyleLst")
                .map(|l| l.children_named("a:ln").map(parse_line).collect())
                .unwrap_or_default(),
            effect_styles: list("a:effectStyleLst")
                .map(|l| {
                    l.children_named("a:effectStyle")
                        .map(|es| EffectStyle {
                            effects: es.child("a:effectLst").map(parse_effect_list),
                            extras: es
                                .elements()
                                .filter(|c| c.name != "a:effectLst")
                                .cloned()
                                .collect(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            bg_fill_styles: fills("a:bgFillStyleLst"),
        },
        extras: root
            .elements()
            .filter(|c| c.name != "a:themeElements")
            .cloned()
            .collect(),
        element_extras: te
            .map(|t| {
                t.elements()
                    .filter(|c| !KNOWN.contains(&c.name.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn serialize_font_collection(name: &str, fc: &FontCollection) -> XmlElement {
    XmlElement::new(name)
        .with_child(fc.latin.to_element("a:latin"))
        .with_child(fc.ea.to_element("a:ea"))
        .with_child(fc.cs.to_element("a:cs"))
        .with_children(fc.scripts.iter().map(|(script, typeface)| {
            XmlElement::new("a:font")
                .with_attr("script", script.as_str())
                .with_attr("typeface", typeface.as_str())
        }))
        .with_opt_child(fc.ext_lst.clone())
}

/// Serialises a theme as an `a:theme` document.
#[must_use]
pub fn serialize_theme(theme: &Theme) -> XmlDocument {
    let f = &theme.format_scheme;
    let extras = if theme.extras.is_empty() {
        vec![
            XmlElement::new("a:objectDefaults"),
            XmlElement::new("a:extraClrSchemeLst"),
        ]
    } else {
        theme.extras.clone()
    };
    let root = XmlElement::new("a:theme")
        .with_attr("xmlns:a", ns::A)
        .with_attr("name", theme.name.as_str())
        .with_child(
            XmlElement::new("a:themeElements")
                .with_child(
                    XmlElement::new("a:clrScheme")
                        .with_attr("name", theme.color_scheme.name.as_str())
                        .with_children(ColorSchemeIndex::ALL.iter().map(|slot| {
                            XmlElement::new(format!("a:{slot}"))
                                .with_child(serialize_color(theme.color_scheme.get(*slot)))
                        })),
                )
                .with_child(
                    XmlElement::new("a:fontScheme")
                        .with_attr("name", theme.font_scheme.name.as_str())
                        .with_child(serialize_font_collection(
                            "a:majorFont",
                            &theme.font_scheme.major,
                        ))
                        .with_child(serialize_font_collection(
                            "a:minorFont",
                            &theme.font_scheme.minor,
                        )),
                )
                .with_child(
                    XmlElement::new("a:fmtScheme")
                        .with_attr("name", f.name.as_str())
                        .with_child(
                            XmlElement::new("a:fillStyleLst")
                                .with_children(f.fill_styles.iter().map(serialize_fill)),
                        )
                        .with_child(
                            XmlElement::new("a:lnStyleLst")
                                .with_children(f.line_styles.iter().map(serialize_line)),
                        )
                        .with_child(XmlElement::new("a:effectStyleLst").with_children(
                            f.effect_styles.iter().map(|s| {
                                XmlElement::new("a:effectStyle")
                                    .with_child(serialize_effect_list(
                                        s.effects.as_ref().unwrap_or(&EffectList::default()),
                                    ))
                                    .with_children(s.extras.iter().cloned())
                            }),
                        ))
                        .with_child(
                            XmlElement::new("a:bgFillStyleLst")
                                .with_children(f.bg_fill_styles.iter().map(serialize_fill)),
                        ),
                )
                .with_children(theme.element_extras.iter().cloned()),
        )
        .with_children(extras);
    XmlDocument::new(root)
}

/// Theme colour slots resolved to RGBA, indexed by [`ColorSchemeIndex`].
#[must_use]
pub fn theme_color_table(theme: &Theme) -> [Rgba; 12] {
    let ctx = ColorContext::default();
    theme
        .color_scheme
        .colors
        .each_ref()
        .map(|c| resolve_color(c, &ctx))
}

/// A colour context for a theme, with a colour map (`p:clrMap`) and an optional placeholder
/// colour.
#[must_use]
pub fn theme_color_context(
    theme: &Theme,
    color_map: ColorMap,
    ph_clr: Option<Rgba>,
) -> ColorContext {
    ColorContext {
        theme_colors: theme_color_table(theme).map(Some),
        color_map,
        ph_clr,
    }
}

/// Resolves a theme font reference: `+mj-lt`, `+mn-ea`, `+mj-cs` (DrawingML) or `majorHAnsi`,
/// `minorEastAsia`, `minorBidi` (WordprocessingML `w:asciiTheme` values). Other names pass
/// through.
#[must_use]
pub fn resolve_theme_font<'a>(name: &'a str, theme: &'a Theme) -> &'a str {
    let fs = &theme.font_scheme;
    let pick = |major: bool, kind: &str| {
        let c = if major { &fs.major } else { &fs.minor };
        match kind {
            "ea" => c.ea.typeface.as_str(),
            "cs" => c.cs.typeface.as_str(),
            _ => c.latin.typeface.as_str(),
        }
    };
    if let Some(rest) = name.strip_prefix('+')
        && let Some((coll, kind)) = rest.split_once('-')
        && (coll == "mj" || coll == "mn")
        && matches!(kind, "lt" | "ea" | "cs")
    {
        return pick(coll == "mj", kind);
    }
    for (prefix, major) in [("major", true), ("minor", false)] {
        if let Some(kind) = name.strip_prefix(prefix) {
            match kind {
                "Ascii" | "HAnsi" => return pick(major, "lt"),
                "EastAsia" => return pick(major, "ea"),
                "Bidi" => return pick(major, "cs"),
                _ => {}
            }
        }
    }
    name
}

fn ph(transforms: &[(&str, i64)]) -> DrawingColor {
    DrawingColor {
        model: ColorModel::Scheme("phClr".to_owned()),
        transforms: transforms
            .iter()
            .map(|(n, v)| ColorTransform::new(n, *v))
            .collect(),
    }
}

/// The Office theme of Office 2013–2022 ("Office Theme": Calibri Light / Calibri, the 4472C4
/// accent row), which Word 2016–2021 documents and Kiln's new documents use.
#[must_use]
pub fn office_theme() -> Theme {
    let sys = |val: &str, last: &str| DrawingColor {
        model: ColorModel::System {
            val: val.to_owned(),
            last_clr: Some(last.to_owned()),
        },
        transforms: Vec::new(),
    };
    let fonts = |latin: &str| {
        let light = latin == "Calibri Light";
        let scripts: &[(&str, &str)] = &[
            (
                "Jpan",
                if light {
                    "游ゴシック Light"
                } else {
                    "游明朝"
                },
            ),
            ("Hang", "맑은 고딕"),
            ("Hans", "等线 Light"),
            ("Hant", "新細明體"),
            ("Arab", "Times New Roman"),
            ("Hebr", "Times New Roman"),
            ("Thai", "Angsana New"),
            ("Ethi", "Nyala"),
            ("Beng", "Vrinda"),
            ("Gujr", "Shruti"),
            ("Khmr", "MoolBoran"),
            ("Knda", "Tunga"),
            ("Guru", "Raavi"),
            ("Cans", "Euphemia"),
            ("Cher", "Plantagenet Cherokee"),
            ("Yiii", "Microsoft Yi Baiti"),
            ("Tibt", "Microsoft Himalaya"),
            ("Thaa", "MV Boli"),
            ("Deva", "Mangal"),
            ("Telu", "Gautami"),
            ("Taml", "Latha"),
            ("Syrc", "Estrangelo Edessa"),
            ("Orya", "Kalinga"),
            ("Mlym", "Kartika"),
            ("Laoo", "DokChampa"),
            ("Sinh", "Iskoola Pota"),
            ("Mong", "Mongolian Baiti"),
            ("Viet", "Times New Roman"),
            ("Uigh", "Microsoft Uighur"),
            ("Geor", "Sylfaen"),
        ];
        FontCollection {
            latin: TextFont {
                typeface: latin.to_owned(),
                panose: Some(
                    if light {
                        "020F0302020204030204"
                    } else {
                        "020F0502020204030204"
                    }
                    .to_owned(),
                ),
                ..TextFont::default()
            },
            ea: TextFont::new(""),
            cs: TextFont::new(""),
            scripts: scripts
                .iter()
                .map(|(s, t)| ((*s).to_owned(), (*t).to_owned()))
                .collect(),
            ext_lst: None,
        }
    };
    let grad = |stops: Vec<(f64, DrawingColor)>| {
        Fill::Gradient(GradientFill {
            stops: stops
                .into_iter()
                .map(|(pos, color)| GradientStop { pos, color })
                .collect(),
            linear: Some(LinearShade {
                angle: 90.0,
                scaled: false,
            }),
            rot_with_shape: Some(true),
            ..GradientFill::default()
        })
    };
    let line = |w: i64| Line {
        width: Some(w),
        cap: Some(LineCap::Flat),
        compound: Some(CompoundLine::Sng),
        align: Some(PenAlignment::Ctr),
        fill: Some(Fill::Solid(ph(&[]))),
        dash: Some(LineDash::Preset(PresetLineDashVal::Solid)),
        join: Some(LineJoin::Miter(Some(8.0))),
        head: None,
        tail: None,
    };
    let empty_effects = || EffectStyle {
        effects: Some(EffectList::default()),
        extras: Vec::new(),
    };
    Theme {
        name: "Office Theme".to_owned(),
        color_scheme: ColorScheme {
            name: "Office".to_owned(),
            colors: [
                sys("windowText", "000000"),
                sys("window", "FFFFFF"),
                DrawingColor::srgb("44546A"),
                DrawingColor::srgb("E7E6E6"),
                DrawingColor::srgb("4472C4"),
                DrawingColor::srgb("ED7D31"),
                DrawingColor::srgb("A5A5A5"),
                DrawingColor::srgb("FFC000"),
                DrawingColor::srgb("5B9BD5"),
                DrawingColor::srgb("70AD47"),
                DrawingColor::srgb("0563C1"),
                DrawingColor::srgb("954F72"),
            ],
        },
        font_scheme: FontScheme {
            name: "Office".to_owned(),
            major: fonts("Calibri Light"),
            minor: fonts("Calibri"),
        },
        format_scheme: FormatScheme {
            name: "Office".to_owned(),
            fill_styles: vec![
                Fill::Solid(ph(&[])),
                grad(vec![
                    (
                        0.0,
                        ph(&[("lumMod", 110_000), ("satMod", 105_000), ("tint", 67_000)]),
                    ),
                    (
                        0.5,
                        ph(&[("lumMod", 105_000), ("satMod", 103_000), ("tint", 73_000)]),
                    ),
                    (
                        1.0,
                        ph(&[("lumMod", 105_000), ("satMod", 109_000), ("tint", 81_000)]),
                    ),
                ]),
                grad(vec![
                    (
                        0.0,
                        ph(&[("satMod", 103_000), ("lumMod", 102_000), ("tint", 94_000)]),
                    ),
                    (
                        0.5,
                        ph(&[("satMod", 110_000), ("lumMod", 100_000), ("shade", 100_000)]),
                    ),
                    (
                        1.0,
                        ph(&[("lumMod", 99_000), ("satMod", 120_000), ("shade", 78_000)]),
                    ),
                ]),
            ],
            line_styles: vec![line(6_350), line(12_700), line(19_050)],
            effect_styles: vec![
                empty_effects(),
                empty_effects(),
                EffectStyle {
                    effects: Some(EffectList {
                        outer_shadow: Some(OuterShadow {
                            blur_rad: 57_150,
                            dist: 19_050,
                            dir: 90.0,
                            sx: 1.0,
                            sy: 1.0,
                            kx: 0.0,
                            ky: 0.0,
                            algn: RectAlignment::Ctr,
                            rot_with_shape: false,
                            color: DrawingColor::srgb("000000").with("alpha", 63_000),
                        }),
                        ..EffectList::default()
                    }),
                    extras: Vec::new(),
                },
            ],
            bg_fill_styles: vec![
                Fill::Solid(ph(&[])),
                Fill::Solid(ph(&[("tint", 95_000), ("satMod", 170_000)])),
                grad(vec![
                    (
                        0.0,
                        ph(&[
                            ("tint", 93_000),
                            ("satMod", 150_000),
                            ("shade", 98_000),
                            ("lumMod", 102_000),
                        ]),
                    ),
                    (
                        0.5,
                        ph(&[
                            ("tint", 98_000),
                            ("satMod", 130_000),
                            ("shade", 90_000),
                            ("lumMod", 103_000),
                        ]),
                    ),
                    (1.0, ph(&[("shade", 63_000), ("satMod", 120_000)])),
                ]),
            ],
        },
        extras: Vec::new(),
        element_extras: Vec::new(),
    }
}

/// The theme colour for a SpreadsheetML/WordprocessingML theme index (`<color theme="4">`). The
/// indexes follow `a:clrScheme` order with the dark/light pairs swapped (0 = `lt1`, 1 = `dk1`).
#[must_use]
pub fn theme_color_for_index(index: usize) -> DrawingColor {
    const ORDER: [&str; 12] = [
        "lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5",
        "accent6", "hlink", "folHlink",
    ];
    DrawingColor::scheme(ORDER.get(index).copied().unwrap_or("dk1"))
}
