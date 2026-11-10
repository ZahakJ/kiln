// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML colours (ECMA-376 Part 1 §20.1.2.3): the six colour models, their transforms in
//! document order, and resolution to RGBA against a theme and colour map.

use kiln_common::Color;

use crate::st::ColorSchemeIndex;
use crate::units::round_half_up;
use crate::xml::{XmlElement, parse_int};

/// A colour transform in document order: `lumMod 75000`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorTransform {
    /// The transform's local name (`lumMod`, `alpha`, `shade`, …).
    pub name: String,
    /// The raw value (thousandths of a percent, or 60 000ths of a degree for hue); `None` for
    /// the flag transforms (`comp`, `inv`, `gray`, `gamma`, `invGamma`).
    pub val: Option<i64>,
}

impl ColorTransform {
    /// A transform with a value.
    #[must_use]
    pub fn new(name: &str, val: i64) -> Self {
        Self {
            name: name.to_owned(),
            val: Some(val),
        }
    }

    /// A flag transform (no value).
    #[must_use]
    pub fn flag(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            val: None,
        }
    }
}

/// The colour model of a DrawingML colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorModel {
    /// `a:srgbClr`: `RRGGBB`, upper case.
    Srgb(String),
    /// `a:schemeClr`: a theme slot or placeholder (`accent1`, `tx1`, `phClr`, …), kept as
    /// written so unknown slots survive a round trip.
    Scheme(String),
    /// `a:sysClr`: a Windows system colour with the last value Office saw.
    System {
        /// `val` (`windowText`).
        val: String,
        /// `lastClr`, `RRGGBB`.
        last_clr: Option<String>,
    },
    /// `a:prstClr`: a named colour (`black`, `dkBlue`, …).
    Preset(String),
    /// `a:hslClr`: hue in 60 000ths of a degree, saturation and luminance in thousandths of a
    /// percent.
    Hsl {
        /// Hue.
        hue: i64,
        /// Saturation.
        sat: i64,
        /// Luminance.
        lum: i64,
    },
    /// `a:scrgbClr`: linear RGB in thousandths of a percent.
    ScRgb {
        /// Red.
        r: i64,
        /// Green.
        g: i64,
        /// Blue.
        b: i64,
    },
}

/// A DrawingML colour: a model plus transforms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawingColor {
    /// The colour model.
    pub model: ColorModel,
    /// Transforms, applied in order.
    pub transforms: Vec<ColorTransform>,
}

impl DrawingColor {
    /// An sRGB colour (`#a1b2c3`, `A1B2C3` or `FFA1B2C3` accepted).
    #[must_use]
    pub fn srgb(hex: &str) -> Self {
        Self {
            model: ColorModel::Srgb(normalize_hex(hex)),
            transforms: Vec::new(),
        }
    }

    /// A theme colour.
    #[must_use]
    pub fn scheme(slot: &str) -> Self {
        Self {
            model: ColorModel::Scheme(slot.to_owned()),
            transforms: Vec::new(),
        }
    }

    /// Adds a transform (builder style).
    #[must_use]
    pub fn with(mut self, name: &str, val: i64) -> Self {
        self.transforms.push(ColorTransform::new(name, val));
        self
    }

    /// Adds a flag transform (builder style).
    #[must_use]
    pub fn with_flag(mut self, name: &str) -> Self {
        self.transforms.push(ColorTransform::flag(name));
        self
    }
}

/// A resolved colour: channels 0–255 (fractional before rounding), alpha 0–1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    /// Red, 0–255.
    pub r: f64,
    /// Green, 0–255.
    pub g: f64,
    /// Blue, 0–255.
    pub b: f64,
    /// Alpha, 0–1.
    pub a: f64,
}

impl Rgba {
    /// An opaque colour.
    #[must_use]
    pub const fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// The 8-bit colour.
    #[must_use]
    pub fn to_color(self) -> Color {
        let a = clamp255(clamp01(self.a) * 255.0);
        Color::rgba(clamp255(self.r), clamp255(self.g), clamp255(self.b), a)
    }
}

impl From<Color> for Rgba {
    fn from(c: Color) -> Self {
        Self {
            r: f64::from(c.r),
            g: f64::from(c.g),
            b: f64::from(c.b),
            a: f64::from(c.a) / 255.0,
        }
    }
}

/// The colour elements, in schema order.
pub const COLOR_ELEMENTS: &[&str] = &[
    "a:scrgbClr",
    "a:srgbClr",
    "a:hslClr",
    "a:sysClr",
    "a:schemeClr",
    "a:prstClr",
];

const FLAG_TRANSFORMS: &[&str] = &["comp", "inv", "gray", "gamma", "invGamma"];

// --------------------------------------------------------------- parsing

fn parse_transforms(node: &XmlElement) -> Vec<ColorTransform> {
    node.elements()
        .map(|c| {
            let name = c.name.strip_prefix("a:").unwrap_or(&c.name).to_owned();
            let val = if FLAG_TRANSFORMS.contains(&name.as_str()) {
                None
            } else {
                c.attr_int("val")
            };
            ColorTransform { name, val }
        })
        .collect()
}

fn parse_pct(v: Option<&str>) -> i64 {
    let Some(v) = v.map(str::trim) else { return 0 };
    if let Some(n) = v.strip_suffix('%') {
        return n
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map_or(0, |n| crate::units::round_to_i64(n * 1000.0));
    }
    parse_int(v).unwrap_or_else(|| {
        v.parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map_or(0, crate::units::round_to_i64)
    })
}

/// Normalises a hex colour to `RRGGBB` (upper case); `AARRGGBB` drops the alpha; anything else is
/// black.
#[must_use]
pub fn normalize_hex(v: &str) -> String {
    let h = v.trim().trim_start_matches('#').to_ascii_uppercase();
    let hex = h.bytes().all(|b| b.is_ascii_hexdigit());
    match h.len() {
        6 if hex => h,
        8 if hex => h[2..].to_owned(),
        _ => "000000".to_owned(),
    }
}

/// Parses a colour element (`a:srgbClr`, `a:schemeClr`, …); `None` for other elements.
#[must_use]
pub fn parse_color(node: &XmlElement) -> Option<DrawingColor> {
    let model = match node.name.as_str() {
        "a:srgbClr" => ColorModel::Srgb(normalize_hex(node.attr("val").unwrap_or("000000"))),
        "a:schemeClr" => ColorModel::Scheme(node.attr("val").unwrap_or("tx1").to_owned()),
        "a:sysClr" => ColorModel::System {
            val: node.attr("val").unwrap_or("windowText").to_owned(),
            last_clr: node
                .attr("lastClr")
                .filter(|l| !l.is_empty())
                .map(normalize_hex),
        },
        "a:prstClr" => ColorModel::Preset(node.attr("val").unwrap_or("black").to_owned()),
        "a:hslClr" => ColorModel::Hsl {
            hue: node.attr_int("hue").unwrap_or(0),
            sat: parse_pct(node.attr("sat")),
            lum: parse_pct(node.attr("lum")),
        },
        "a:scrgbClr" => ColorModel::ScRgb {
            r: parse_pct(node.attr("r")),
            g: parse_pct(node.attr("g")),
            b: parse_pct(node.attr("b")),
        },
        _ => return None,
    };
    Some(DrawingColor {
        model,
        transforms: parse_transforms(node),
    })
}

/// The colour inside a container such as `a:solidFill`, `a:fgClr`, `a:outerShdw` or `a:dk1`.
#[must_use]
pub fn parse_color_child(container: Option<&XmlElement>) -> Option<DrawingColor> {
    container?.elements().find_map(parse_color)
}

/// The colour element for a colour.
#[must_use]
pub fn serialize_color(c: &DrawingColor) -> XmlElement {
    let el = match &c.model {
        ColorModel::Srgb(hex) => XmlElement::new("a:srgbClr").with_attr("val", hex.as_str()),
        ColorModel::Scheme(s) => XmlElement::new("a:schemeClr").with_attr("val", s.as_str()),
        ColorModel::System { val, last_clr } => XmlElement::new("a:sysClr")
            .with_attr("val", val.as_str())
            .with_opt_attr("lastClr", last_clr.as_deref()),
        ColorModel::Preset(p) => XmlElement::new("a:prstClr").with_attr("val", p.as_str()),
        ColorModel::Hsl { hue, sat, lum } => XmlElement::new("a:hslClr")
            .with_attr("hue", *hue)
            .with_attr("sat", *sat)
            .with_attr("lum", *lum),
        ColorModel::ScRgb { r, g, b } => XmlElement::new("a:scrgbClr")
            .with_attr("r", *r)
            .with_attr("g", *g)
            .with_attr("b", *b),
    };
    el.with_children(
        c.transforms
            .iter()
            .map(|t| XmlElement::new(format!("a:{}", t.name)).with_opt_attr("val", t.val)),
    )
}

// ------------------------------------------------------------ resolution

/// `p:clrMap` / `w:clrSchemeMapping`: which theme slot each logical colour uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorMap {
    /// Background 1.
    pub bg1: ColorSchemeIndex,
    /// Text 1.
    pub tx1: ColorSchemeIndex,
    /// Background 2.
    pub bg2: ColorSchemeIndex,
    /// Text 2.
    pub tx2: ColorSchemeIndex,
    /// Accents 1–6.
    pub accents: [ColorSchemeIndex; 6],
    /// Hyperlink.
    pub hlink: ColorSchemeIndex,
    /// Followed hyperlink.
    pub fol_hlink: ColorSchemeIndex,
}

impl Default for ColorMap {
    /// Office's default mapping: `bg1→lt1, tx1→dk1, bg2→lt2, tx2→dk2`, accents and links to
    /// themselves.
    fn default() -> Self {
        use ColorSchemeIndex as C;
        Self {
            bg1: C::Lt1,
            tx1: C::Dk1,
            bg2: C::Lt2,
            tx2: C::Dk2,
            accents: [
                C::Accent1,
                C::Accent2,
                C::Accent3,
                C::Accent4,
                C::Accent5,
                C::Accent6,
            ],
            hlink: C::Hlink,
            fol_hlink: C::FolHlink,
        }
    }
}

/// A slot name in either vocabulary: DrawingML (`lt1`, `accent1`, `hlink`) or WordprocessingML
/// (`light1`, `accent1`, `hyperlink`, `followedHyperlink`).
fn slot_from_name(v: &str) -> Option<ColorSchemeIndex> {
    use ColorSchemeIndex as C;
    ColorSchemeIndex::parse(v).or(match v {
        "dark1" => Some(C::Dk1),
        "light1" => Some(C::Lt1),
        "dark2" => Some(C::Dk2),
        "light2" => Some(C::Lt2),
        "hyperlink" => Some(C::Hlink),
        "followedHyperlink" => Some(C::FolHlink),
        _ => None,
    })
}

impl ColorMap {
    /// Reads `p:clrMap`, `p:clrMapOvr/a:overrideClrMapping` or `w:clrSchemeMapping` (attribute
    /// names with or without a prefix). Missing or unknown attributes keep the default.
    #[must_use]
    pub fn parse(node: &XmlElement) -> Self {
        let mut map = Self::default();
        for (k, v) in node.attrs.iter() {
            let local = crate::xml::namespaces::local_name(k);
            let Some(slot) = slot_from_name(v) else {
                continue;
            };
            map.set(local, slot);
        }
        map
    }

    /// Sets one entry by its attribute name (`bg1`, `accent3`, `followedHyperlink`, …); returns
    /// whether the name is known.
    pub fn set(&mut self, name: &str, slot: ColorSchemeIndex) -> bool {
        match name {
            "bg1" => self.bg1 = slot,
            "t1" | "tx1" => self.tx1 = slot,
            "bg2" => self.bg2 = slot,
            "t2" | "tx2" => self.tx2 = slot,
            "hlink" | "hyperlink" => self.hlink = slot,
            "folHlink" | "followedHyperlink" => self.fol_hlink = slot,
            other => match other
                .strip_prefix("accent")
                .and_then(|n| n.parse::<usize>().ok())
            {
                Some(n @ 1..=6) => self.accents[n - 1] = slot,
                _ => return false,
            },
        }
        true
    }

    /// The theme slot a scheme colour name uses after mapping (`bg1` → `lt1`); direct slot names
    /// (`dk1`, `lt2`) map to themselves.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<ColorSchemeIndex> {
        Some(match name {
            "bg1" => self.bg1,
            "tx1" => self.tx1,
            "bg2" => self.bg2,
            "tx2" => self.tx2,
            "hlink" => self.hlink,
            "folHlink" => self.fol_hlink,
            "dk1" => ColorSchemeIndex::Dk1,
            "lt1" => ColorSchemeIndex::Lt1,
            "dk2" => ColorSchemeIndex::Dk2,
            "lt2" => ColorSchemeIndex::Lt2,
            other => match other
                .strip_prefix("accent")
                .and_then(|n| n.parse::<usize>().ok())
            {
                Some(n @ 1..=6) => self.accents[n - 1],
                _ => return None,
            },
        })
    }
}

/// What a colour resolves against.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColorContext {
    /// Theme colour slots, already resolved, indexed by [`ColorSchemeIndex`].
    pub theme_colors: [Option<Rgba>; 12],
    /// The colour map.
    pub color_map: ColorMap,
    /// The placeholder colour (`phClr`) substituted inside theme style matrices.
    pub ph_clr: Option<Rgba>,
}

/// Windows default system colours, used when `a:sysClr` has no `lastClr`.
pub const SYSTEM_COLORS: &[(&str, &str)] = &[
    ("scrollBar", "C8C8C8"),
    ("background", "000000"),
    ("activeCaption", "99B4D1"),
    ("inactiveCaption", "BFCDDB"),
    ("menu", "F0F0F0"),
    ("window", "FFFFFF"),
    ("windowFrame", "646464"),
    ("menuText", "000000"),
    ("windowText", "000000"),
    ("captionText", "000000"),
    ("activeBorder", "B4B4B4"),
    ("inactiveBorder", "F4F7FC"),
    ("appWorkspace", "ABABAB"),
    ("highlight", "0078D7"),
    ("highlightText", "FFFFFF"),
    ("btnFace", "F0F0F0"),
    ("btnShadow", "A0A0A0"),
    ("grayText", "6D6D6D"),
    ("btnText", "000000"),
    ("inactiveCaptionText", "000000"),
    ("btnHighlight", "FFFFFF"),
    ("3dDkShadow", "696969"),
    ("3dLight", "E3E3E3"),
    ("infoText", "000000"),
    ("infoBk", "FFFFE1"),
    ("hotLight", "0066CC"),
    ("gradientActiveCaption", "B9D1EA"),
    ("gradientInactiveCaption", "D7E4F2"),
    ("menuHighlight", "3399FF"),
    ("menuBar", "F0F0F0"),
];

/// ST_PresetColorVal (CSS/X11 names; DrawingML also accepts `dk`/`lt`/`med` abbreviations).
const PRESET_COLORS: &[(&str, &str)] = &[
    ("aliceBlue", "F0F8FF"),
    ("antiqueWhite", "FAEBD7"),
    ("aqua", "00FFFF"),
    ("aquamarine", "7FFFD4"),
    ("azure", "F0FFFF"),
    ("beige", "F5F5DC"),
    ("bisque", "FFE4C4"),
    ("black", "000000"),
    ("blanchedAlmond", "FFEBCD"),
    ("blue", "0000FF"),
    ("blueViolet", "8A2BE2"),
    ("brown", "A52A2A"),
    ("burlyWood", "DEB887"),
    ("cadetBlue", "5F9EA0"),
    ("chartreuse", "7FFF00"),
    ("chocolate", "D2691E"),
    ("coral", "FF7F50"),
    ("cornflowerBlue", "6495ED"),
    ("cornsilk", "FFF8DC"),
    ("crimson", "DC143C"),
    ("cyan", "00FFFF"),
    ("darkBlue", "00008B"),
    ("darkCyan", "008B8B"),
    ("darkGoldenrod", "B8860B"),
    ("darkGray", "A9A9A9"),
    ("darkGrey", "A9A9A9"),
    ("darkGreen", "006400"),
    ("darkKhaki", "BDB76B"),
    ("darkMagenta", "8B008B"),
    ("darkOliveGreen", "556B2F"),
    ("darkOrange", "FF8C00"),
    ("darkOrchid", "9932CC"),
    ("darkRed", "8B0000"),
    ("darkSalmon", "E9967A"),
    ("darkSeaGreen", "8FBC8F"),
    ("darkSlateBlue", "483D8B"),
    ("darkSlateGray", "2F4F4F"),
    ("darkSlateGrey", "2F4F4F"),
    ("darkTurquoise", "00CED1"),
    ("darkViolet", "9400D3"),
    ("deepPink", "FF1493"),
    ("deepSkyBlue", "00BFFF"),
    ("dimGray", "696969"),
    ("dimGrey", "696969"),
    ("dodgerBlue", "1E90FF"),
    ("firebrick", "B22222"),
    ("floralWhite", "FFFAF0"),
    ("forestGreen", "228B22"),
    ("fuchsia", "FF00FF"),
    ("gainsboro", "DCDCDC"),
    ("ghostWhite", "F8F8FF"),
    ("gold", "FFD700"),
    ("goldenrod", "DAA520"),
    ("gray", "808080"),
    ("grey", "808080"),
    ("green", "008000"),
    ("greenYellow", "ADFF2F"),
    ("honeydew", "F0FFF0"),
    ("hotPink", "FF69B4"),
    ("indianRed", "CD5C5C"),
    ("indigo", "4B0082"),
    ("ivory", "FFFFF0"),
    ("khaki", "F0E68C"),
    ("lavender", "E6E6FA"),
    ("lavenderBlush", "FFF0F5"),
    ("lawnGreen", "7CFC00"),
    ("lemonChiffon", "FFFACD"),
    ("lightBlue", "ADD8E6"),
    ("lightCoral", "F08080"),
    ("lightCyan", "E0FFFF"),
    ("lightGoldenrodYellow", "FAFAD2"),
    ("lightGray", "D3D3D3"),
    ("lightGrey", "D3D3D3"),
    ("lightGreen", "90EE90"),
    ("lightPink", "FFB6C1"),
    ("lightSalmon", "FFA07A"),
    ("lightSeaGreen", "20B2AA"),
    ("lightSkyBlue", "87CEFA"),
    ("lightSlateGray", "778899"),
    ("lightSlateGrey", "778899"),
    ("lightSteelBlue", "B0C4DE"),
    ("lightYellow", "FFFFE0"),
    ("lime", "00FF00"),
    ("limeGreen", "32CD32"),
    ("linen", "FAF0E6"),
    ("magenta", "FF00FF"),
    ("maroon", "800000"),
    ("mediumAquamarine", "66CDAA"),
    ("mediumBlue", "0000CD"),
    ("mediumOrchid", "BA55D3"),
    ("mediumPurple", "9370DB"),
    ("mediumSeaGreen", "3CB371"),
    ("mediumSlateBlue", "7B68EE"),
    ("mediumSpringGreen", "00FA9A"),
    ("mediumTurquoise", "48D1CC"),
    ("mediumVioletRed", "C71585"),
    ("midnightBlue", "191970"),
    ("mintCream", "F5FFFA"),
    ("mistyRose", "FFE4E1"),
    ("moccasin", "FFE4B5"),
    ("navajoWhite", "FFDEAD"),
    ("navy", "000080"),
    ("oldLace", "FDF5E6"),
    ("olive", "808000"),
    ("oliveDrab", "6B8E23"),
    ("orange", "FFA500"),
    ("orangeRed", "FF4500"),
    ("orchid", "DA70D6"),
    ("paleGoldenrod", "EEE8AA"),
    ("paleGreen", "98FB98"),
    ("paleTurquoise", "AFEEEE"),
    ("paleVioletRed", "DB7093"),
    ("papayaWhip", "FFEFD5"),
    ("peachPuff", "FFDAB9"),
    ("peru", "CD853F"),
    ("pink", "FFC0CB"),
    ("plum", "DDA0DD"),
    ("powderBlue", "B0E0E6"),
    ("purple", "800080"),
    ("red", "FF0000"),
    ("rosyBrown", "BC8F8F"),
    ("royalBlue", "4169E1"),
    ("saddleBrown", "8B4513"),
    ("salmon", "FA8072"),
    ("sandyBrown", "F4A460"),
    ("seaGreen", "2E8B57"),
    ("seaShell", "FFF5EE"),
    ("sienna", "A0522D"),
    ("silver", "C0C0C0"),
    ("skyBlue", "87CEEB"),
    ("slateBlue", "6A5ACD"),
    ("slateGray", "708090"),
    ("slateGrey", "708090"),
    ("snow", "FFFAFA"),
    ("springGreen", "00FF7F"),
    ("steelBlue", "4682B4"),
    ("tan", "D2B48C"),
    ("teal", "008080"),
    ("thistle", "D8BFD8"),
    ("tomato", "FF6347"),
    ("turquoise", "40E0D0"),
    ("violet", "EE82EE"),
    ("wheat", "F5DEB3"),
    ("white", "FFFFFF"),
    ("whiteSmoke", "F5F5F5"),
    ("yellow", "FFFF00"),
    ("yellowGreen", "9ACD32"),
];

fn lookup(table: &[(&str, &'static str)], name: &str) -> Option<&'static str> {
    table.iter().find(|(n, _)| *n == name).map(|(_, h)| *h)
}

/// The hex value of an ST_PresetColorVal name (`dkBlue`, `ltGray`, `medPurple` included).
#[must_use]
pub fn preset_color_hex(name: &str) -> Option<&'static str> {
    if let Some(h) = lookup(PRESET_COLORS, name) {
        return Some(h);
    }
    let expanded = if let Some(rest) = name.strip_prefix("dk") {
        format!("dark{rest}")
    } else if let Some(rest) = name.strip_prefix("lt") {
        format!("light{rest}")
    } else {
        format!("medium{}", name.strip_prefix("med")?)
    };
    lookup(PRESET_COLORS, &expanded)
}

/// The default value of a Windows system colour.
#[must_use]
pub fn system_color_hex(name: &str) -> Option<&'static str> {
    lookup(SYSTEM_COLORS, name)
}

/// Parses `RRGGBB` (see [`normalize_hex`]) with alpha `a`.
#[must_use]
pub fn hex_to_rgba(hex: &str, a: f64) -> Rgba {
    let h = normalize_hex(hex);
    let ch = |i: usize| f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0));
    Rgba {
        r: ch(0),
        g: ch(2),
        b: ch(4),
        a,
    }
}

fn clamp01(x: f64) -> f64 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn clamp255(x: f64) -> u8 {
    // Clamped to 0..=255 and rounded first, so the cast is exact.
    if x.is_nan() {
        0
    } else {
        round_half_up(x.clamp(0.0, 255.0)) as u8
    }
}

/// `RRGGBB` (upper case) of a resolved colour; alpha is ignored.
#[must_use]
pub fn rgba_to_hex(c: Rgba) -> String {
    format!(
        "{:02X}{:02X}{:02X}",
        clamp255(c.r),
        clamp255(c.g),
        clamp255(c.b)
    )
}

/// A CSS colour: `#rrggbb` when opaque, `rgba(r, g, b, a)` otherwise.
#[must_use]
pub fn rgba_to_css(c: Rgba) -> String {
    if c.a >= 1.0 {
        return format!("#{}", rgba_to_hex(c).to_ascii_lowercase());
    }
    let a = round_half_up(clamp01(c.a) * 1000.0) / 1000.0;
    format!(
        "rgba({}, {}, {}, {})",
        clamp255(c.r),
        clamp255(c.g),
        clamp255(c.b),
        crate::xml::format_number(a)
    )
}

/// Hue (degrees 0–360), saturation and lightness (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsl {
    /// Hue in degrees.
    pub h: f64,
    /// Saturation, 0–1.
    pub s: f64,
    /// Lightness, 0–1.
    pub l: f64,
}

/// RGB (0–255) to HSL.
#[must_use]
pub fn rgb_to_hsl(r: f64, g: f64, b: f64) -> Hsl {
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return Hsl { h: 0.0, s: 0.0, l };
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    Hsl { h: h * 60.0, s, l }
}

/// HSL to RGB (0–255).
#[must_use]
pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let hh = h.rem_euclid(360.0) / 360.0;
    if s == 0.0 {
        return (l * 255.0, l * 255.0, l * 255.0);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let ch = |t: f64| {
        let mut x = t;
        if x < 0.0 {
            x += 1.0;
        }
        if x > 1.0 {
            x -= 1.0;
        }
        if x < 1.0 / 6.0 {
            p + (q - p) * 6.0 * x
        } else if x < 0.5 {
            q
        } else if x < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - x) * 6.0
        } else {
            p
        }
    };
    (
        ch(hh + 1.0 / 3.0) * 255.0,
        ch(hh) * 255.0,
        ch(hh - 1.0 / 3.0) * 255.0,
    )
}

fn to_linear(c: f64) -> f64 {
    let x = c / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(x: f64) -> f64 {
    let c = clamp01(x);
    (if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }) * 255.0
}

/// Applies DrawingML colour transforms in document order. Unknown transforms are ignored rather
/// than failing the render.
#[must_use]
pub fn apply_color_transforms(base: Rgba, transforms: &[ColorTransform]) -> Rgba {
    let Rgba {
        mut r,
        mut g,
        mut b,
        mut a,
    } = base;
    for t in transforms {
        #[allow(clippy::cast_precision_loss)]
        let raw = t.val.unwrap_or(0) as f64;
        let f = raw / 100_000.0;
        let mut hsl_op = |op: &dyn Fn(&mut Hsl)| {
            let mut hsl = rgb_to_hsl(r, g, b);
            op(&mut hsl);
            (r, g, b) = hsl_to_rgb(hsl.h, clamp01(hsl.s), clamp01(hsl.l));
        };
        match t.name.as_str() {
            "alpha" => a = clamp01(f),
            "alphaMod" => a = clamp01(a * f),
            "alphaOff" => a = clamp01(a + f),
            "lum" => hsl_op(&|x| x.l = f),
            "lumMod" => hsl_op(&|x| x.l *= f),
            "lumOff" => hsl_op(&|x| x.l += f),
            "sat" => hsl_op(&|x| x.s = f),
            "satMod" => hsl_op(&|x| x.s *= f),
            "satOff" => hsl_op(&|x| x.s += f),
            "hue" => hsl_op(&|x| x.h = raw / 60_000.0),
            "hueMod" => hsl_op(&|x| x.h *= f),
            "hueOff" => hsl_op(&|x| x.h += raw / 60_000.0),
            "comp" => hsl_op(&|x| x.h += 180.0),
            "inv" => {
                r = 255.0 - r;
                g = 255.0 - g;
                b = 255.0 - b;
            }
            "gray" => {
                let y = r * 0.22 + g * 0.72 + b * 0.06;
                (r, g, b) = (y, y, y);
            }
            // Tint lightens toward white and shade darkens toward black in linear RGB (matches
            // PowerPoint's and LibreOffice's rendering).
            "tint" => {
                let k = f;
                let lin = |c: f64| from_linear(1.0 - (1.0 - to_linear(c)) * k);
                (r, g, b) = (lin(r), lin(g), lin(b));
            }
            "shade" => {
                let k = f;
                let lin = |c: f64| from_linear(to_linear(c) * k);
                (r, g, b) = (lin(r), lin(g), lin(b));
            }
            name @ ("red" | "green" | "blue" | "redMod" | "greenMod" | "blueMod" | "redOff"
            | "greenOff" | "blueOff") => {
                let mut lin = [to_linear(r), to_linear(g), to_linear(b)];
                let i = if name.starts_with("red") {
                    0
                } else if name.starts_with("green") {
                    1
                } else {
                    2
                };
                lin[i] = if name.ends_with("Mod") {
                    lin[i] * f
                } else if name.ends_with("Off") {
                    lin[i] + f
                } else {
                    f
                };
                (r, g, b) = (
                    from_linear(lin[0]),
                    from_linear(lin[1]),
                    from_linear(lin[2]),
                );
            }
            "gamma" => {
                r = from_linear(r / 255.0);
                g = from_linear(g / 255.0);
                b = from_linear(b / 255.0);
            }
            "invGamma" => {
                r = to_linear(r) * 255.0;
                g = to_linear(g) * 255.0;
                b = to_linear(b) * 255.0;
            }
            _ => {}
        }
    }
    Rgba {
        r: f64::from(clamp255(r)),
        g: f64::from(clamp255(g)),
        b: f64::from(clamp255(b)),
        a,
    }
}

/// Resolves a colour to RGBA. Unknown scheme slots resolve to black, as Office does when a theme
/// lacks the slot.
#[must_use]
pub fn resolve_color(color: &DrawingColor, ctx: &ColorContext) -> Rgba {
    let base = match &color.model {
        ColorModel::Srgb(hex) => hex_to_rgba(hex, 1.0),
        ColorModel::Scheme(name) => {
            let slot = if name == "phClr" {
                ctx.ph_clr
            } else {
                ctx.color_map
                    .lookup(name)
                    .and_then(|s| ctx.theme_colors[s.index()])
            };
            slot.unwrap_or(Rgba::rgb(0.0, 0.0, 0.0))
        }
        ColorModel::System { val, last_clr } => hex_to_rgba(
            last_clr
                .as_deref()
                .or_else(|| system_color_hex(val))
                .unwrap_or("000000"),
            1.0,
        ),
        ColorModel::Preset(p) => hex_to_rgba(preset_color_hex(p).unwrap_or("000000"), 1.0),
        #[allow(clippy::cast_precision_loss)]
        ColorModel::Hsl { hue, sat, lum } => {
            let (r, g, b) = hsl_to_rgb(
                *hue as f64 / 60_000.0,
                clamp01(*sat as f64 / 100_000.0),
                clamp01(*lum as f64 / 100_000.0),
            );
            Rgba::rgb(r, g, b)
        }
        #[allow(clippy::cast_precision_loss)]
        ColorModel::ScRgb { r, g, b } => Rgba::rgb(
            from_linear(*r as f64 / 100_000.0),
            from_linear(*g as f64 / 100_000.0),
            from_linear(*b as f64 / 100_000.0),
        ),
    };
    apply_color_transforms(base, &color.transforms)
}

/// `RRGGBB` of a resolved colour.
#[must_use]
pub fn resolve_color_hex(color: &DrawingColor, ctx: &ColorContext) -> String {
    rgba_to_hex(resolve_color(color, ctx))
}

/// SpreadsheetML/WordprocessingML `tint` on a colour (−1…1), applied in HLS the way Excel does:
/// negative values darken, positive values lighten.
#[must_use]
pub fn apply_excel_tint(hex: &str, tint: f64) -> String {
    let c = hex_to_rgba(hex, 1.0);
    let mut hsl = rgb_to_hsl(c.r, c.g, c.b);
    hsl.l = if tint < 0.0 {
        hsl.l * (1.0 + tint)
    } else {
        hsl.l * (1.0 - tint) + tint
    };
    let (r, g, b) = hsl_to_rgb(hsl.h, hsl.s, clamp01(hsl.l));
    rgba_to_hex(Rgba::rgb(r, g, b))
}
