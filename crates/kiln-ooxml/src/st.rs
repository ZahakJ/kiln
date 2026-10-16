// SPDX-License-Identifier: AGPL-3.0-or-later
//! ECMA-376 simple types (`ST_*`) shared by more than one format.
//!
//! Each enumeration knows its schema spelling (`as_str`, `parse`, `Display`, `FromStr`) and lists
//! every value in schema order in `ALL`. Preset shape names (ST_ShapeType) live
//! in [`crate::drawingml::presets`].

use std::fmt;
use std::str::FromStr;

/// A value that is not in a simple type's enumeration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{value}' is not a valid {type_name}")]
pub struct UnknownValue {
    /// The simple type (`ST_LineCap`).
    pub type_name: &'static str,
    /// The rejected value.
    pub value: String,
}

macro_rules! st_enum {
    (
        $(#[$meta:meta])*
        $name:ident ($st:literal) { $($variant:ident = $s:literal),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $(
                #[doc = concat!("`", $s, "`")]
                $variant,
            )+
        }

        impl $name {
            /// Every value, in schema order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The schema spelling.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $s,)+
                }
            }

            /// Parses the schema spelling (case-sensitive, as the schema is).
            #[must_use]
            #[allow(clippy::should_implement_trait)]
            pub fn parse(s: &str) -> Option<Self> {
                match s {
                    $($s => Some($name::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = UnknownValue;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $name::parse(s).ok_or_else(|| UnknownValue {
                    type_name: $st,
                    value: s.to_owned(),
                })
            }
        }

        impl crate::xml::AttrValue for $name {
            fn to_attr(&self) -> String {
                self.as_str().to_owned()
            }
        }
    };
}

st_enum! {
    /// ST_SchemeColorVal: the values of `a:schemeClr/@val`.
    SchemeColorVal("ST_SchemeColorVal") {
        Bg1 = "bg1", Tx1 = "tx1", Bg2 = "bg2", Tx2 = "tx2",
        Accent1 = "accent1", Accent2 = "accent2", Accent3 = "accent3",
        Accent4 = "accent4", Accent5 = "accent5", Accent6 = "accent6",
        Hlink = "hlink", FolHlink = "folHlink", PhClr = "phClr",
        Dk1 = "dk1", Lt1 = "lt1", Dk2 = "dk2", Lt2 = "lt2",
    }
}

st_enum! {
    /// ST_ColorSchemeIndex: the twelve colour slots of `a:clrScheme`, in schema order.
    ColorSchemeIndex("ST_ColorSchemeIndex") {
        Dk1 = "dk1", Lt1 = "lt1", Dk2 = "dk2", Lt2 = "lt2",
        Accent1 = "accent1", Accent2 = "accent2", Accent3 = "accent3",
        Accent4 = "accent4", Accent5 = "accent5", Accent6 = "accent6",
        Hlink = "hlink", FolHlink = "folHlink",
    }
}

impl ColorSchemeIndex {
    /// The slot's position in `a:clrScheme` (0–11).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

st_enum! {
    /// ST_SystemColorVal: Windows system colours (`a:sysClr/@val`).
    SystemColorVal("ST_SystemColorVal") {
        ScrollBar = "scrollBar", Background = "background", ActiveCaption = "activeCaption",
        InactiveCaption = "inactiveCaption", Menu = "menu", Window = "window",
        WindowFrame = "windowFrame", MenuText = "menuText", WindowText = "windowText",
        CaptionText = "captionText", ActiveBorder = "activeBorder",
        InactiveBorder = "inactiveBorder", AppWorkspace = "appWorkspace",
        Highlight = "highlight", HighlightText = "highlightText", BtnFace = "btnFace",
        BtnShadow = "btnShadow", GrayText = "grayText", BtnText = "btnText",
        InactiveCaptionText = "inactiveCaptionText", BtnHighlight = "btnHighlight",
        ThreeDDkShadow = "3dDkShadow", ThreeDLight = "3dLight", InfoText = "infoText",
        InfoBk = "infoBk", HotLight = "hotLight",
        GradientActiveCaption = "gradientActiveCaption",
        GradientInactiveCaption = "gradientInactiveCaption", MenuHighlight = "menuHighlight",
        MenuBar = "menuBar",
    }
}

st_enum! {
    /// ST_TextAlignType: paragraph alignment.
    TextAlignType("ST_TextAlignType") {
        L = "l", Ctr = "ctr", R = "r", Just = "just", JustLow = "justLow", Dist = "dist",
        ThaiDist = "thaiDist",
    }
}

st_enum! {
    /// ST_TextAnchoringType: vertical anchoring of text in a shape.
    TextAnchoringType("ST_TextAnchoringType") {
        T = "t", Ctr = "ctr", B = "b", Just = "just", Dist = "dist",
    }
}

st_enum! {
    /// ST_TextVerticalType: text direction in a body.
    TextVerticalType("ST_TextVerticalType") {
        Horz = "horz", Vert = "vert", Vert270 = "vert270", WordArtVert = "wordArtVert",
        EaVert = "eaVert", MongolianVert = "mongolianVert", WordArtVertRtl = "wordArtVertRtl",
    }
}

st_enum! {
    /// ST_TextWrappingType.
    TextWrappingType("ST_TextWrappingType") { None = "none", Square = "square" }
}

st_enum! {
    /// ST_TextHorzOverflowType.
    TextHorzOverflowType("ST_TextHorzOverflowType") { Overflow = "overflow", Clip = "clip" }
}

st_enum! {
    /// ST_TextVertOverflowType.
    TextVertOverflowType("ST_TextVertOverflowType") {
        Overflow = "overflow", Ellipsis = "ellipsis", Clip = "clip",
    }
}

st_enum! {
    /// ST_TextUnderlineType.
    TextUnderlineType("ST_TextUnderlineType") {
        None = "none", Words = "words", Sng = "sng", Dbl = "dbl", Heavy = "heavy",
        Dotted = "dotted", DottedHeavy = "dottedHeavy", Dash = "dash", DashHeavy = "dashHeavy",
        DashLong = "dashLong", DashLongHeavy = "dashLongHeavy", DotDash = "dotDash",
        DotDashHeavy = "dotDashHeavy", DotDotDash = "dotDotDash",
        DotDotDashHeavy = "dotDotDashHeavy", Wavy = "wavy", WavyHeavy = "wavyHeavy",
        WavyDbl = "wavyDbl",
    }
}

st_enum! {
    /// ST_TextStrikeType.
    TextStrikeType("ST_TextStrikeType") {
        NoStrike = "noStrike", SngStrike = "sngStrike", DblStrike = "dblStrike",
    }
}

st_enum! {
    /// ST_TextCapsType.
    TextCapsType("ST_TextCapsType") { None = "none", Small = "small", All = "all" }
}

st_enum! {
    /// ST_TextFontAlignType.
    TextFontAlignType("ST_TextFontAlignType") {
        Auto = "auto", T = "t", Ctr = "ctr", Base = "base", B = "b",
    }
}

st_enum! {
    /// ST_TextTabAlignType.
    TextTabAlignType("ST_TextTabAlignType") { L = "l", Ctr = "ctr", R = "r", Dec = "dec" }
}

st_enum! {
    /// ST_TextAutonumberScheme: automatic numbering schemes for `a:buAutoNum`.
    TextAutonumberScheme("ST_TextAutonumberScheme") {
        AlphaLcParenBoth = "alphaLcParenBoth", AlphaUcParenBoth = "alphaUcParenBoth",
        AlphaLcParenR = "alphaLcParenR", AlphaUcParenR = "alphaUcParenR",
        AlphaLcPeriod = "alphaLcPeriod", AlphaUcPeriod = "alphaUcPeriod",
        ArabicParenBoth = "arabicParenBoth", ArabicParenR = "arabicParenR",
        ArabicPeriod = "arabicPeriod", ArabicPlain = "arabicPlain",
        RomanLcParenBoth = "romanLcParenBoth", RomanUcParenBoth = "romanUcParenBoth",
        RomanLcParenR = "romanLcParenR", RomanUcParenR = "romanUcParenR",
        RomanLcPeriod = "romanLcPeriod", RomanUcPeriod = "romanUcPeriod",
        CircleNumDbPlain = "circleNumDbPlain", CircleNumWdBlackPlain = "circleNumWdBlackPlain",
        CircleNumWdWhitePlain = "circleNumWdWhitePlain", ArabicDbPeriod = "arabicDbPeriod",
        ArabicDbPlain = "arabicDbPlain", Ea1ChsPeriod = "ea1ChsPeriod",
        Ea1ChsPlain = "ea1ChsPlain", Ea1ChtPeriod = "ea1ChtPeriod", Ea1ChtPlain = "ea1ChtPlain",
        Ea1JpnChsDbPeriod = "ea1JpnChsDbPeriod", Ea1JpnKorPlain = "ea1JpnKorPlain",
        Ea1JpnKorPeriod = "ea1JpnKorPeriod", Arabic1Minus = "arabic1Minus",
        Arabic2Minus = "arabic2Minus", Hebrew2Minus = "hebrew2Minus",
        ThaiAlphaPeriod = "thaiAlphaPeriod", ThaiAlphaParenR = "thaiAlphaParenR",
        ThaiAlphaParenBoth = "thaiAlphaParenBoth", ThaiNumPeriod = "thaiNumPeriod",
        ThaiNumParenR = "thaiNumParenR", ThaiNumParenBoth = "thaiNumParenBoth",
        HindiAlphaPeriod = "hindiAlphaPeriod", HindiNumPeriod = "hindiNumPeriod",
        HindiNumParenR = "hindiNumParenR", HindiAlpha1Period = "hindiAlpha1Period",
    }
}

st_enum! {
    /// ST_LineCap.
    LineCap("ST_LineCap") { Rnd = "rnd", Sq = "sq", Flat = "flat" }
}

st_enum! {
    /// ST_CompoundLine.
    CompoundLine("ST_CompoundLine") {
        Sng = "sng", Dbl = "dbl", ThickThin = "thickThin", ThinThick = "thinThick", Tri = "tri",
    }
}

st_enum! {
    /// ST_PenAlignment.
    PenAlignment("ST_PenAlignment") { Ctr = "ctr", In = "in" }
}

st_enum! {
    /// ST_PresetLineDashVal.
    PresetLineDashVal("ST_PresetLineDashVal") {
        Solid = "solid", Dot = "dot", Dash = "dash", LgDash = "lgDash", DashDot = "dashDot",
        LgDashDot = "lgDashDot", LgDashDotDot = "lgDashDotDot", SysDash = "sysDash",
        SysDot = "sysDot", SysDashDot = "sysDashDot", SysDashDotDot = "sysDashDotDot",
    }
}

st_enum! {
    /// ST_LineEndType.
    LineEndType("ST_LineEndType") {
        None = "none", Triangle = "triangle", Stealth = "stealth", Diamond = "diamond",
        Oval = "oval", Arrow = "arrow",
    }
}

st_enum! {
    /// ST_LineEndWidth / ST_LineEndLength.
    LineEndSize("ST_LineEndWidth") { Sm = "sm", Med = "med", Lg = "lg" }
}

st_enum! {
    /// ST_PathFillMode: how a geometry path is filled.
    PathFillMode("ST_PathFillMode") {
        None = "none", Norm = "norm", Lighten = "lighten", LightenLess = "lightenLess",
        Darken = "darken", DarkenLess = "darkenLess",
    }
}

st_enum! {
    /// ST_RectAlignment.
    RectAlignment("ST_RectAlignment") {
        Tl = "tl", T = "t", Tr = "tr", L = "l", Ctr = "ctr", R = "r", Bl = "bl", B = "b",
        Br = "br",
    }
}

st_enum! {
    /// ST_TileFlipMode.
    TileFlipMode("ST_TileFlipMode") { None = "none", X = "x", Y = "y", Xy = "xy" }
}

st_enum! {
    /// ST_PathShadeType.
    PathShadeType("ST_PathShadeType") { Shape = "shape", Circle = "circle", Rect = "rect" }
}

st_enum! {
    /// ST_PresetPatternVal: the 54 pattern fills.
    PresetPatternVal("ST_PresetPatternVal") {
        Pct5 = "pct5", Pct10 = "pct10", Pct20 = "pct20", Pct25 = "pct25", Pct30 = "pct30",
        Pct40 = "pct40", Pct50 = "pct50", Pct60 = "pct60", Pct70 = "pct70", Pct75 = "pct75",
        Pct80 = "pct80", Pct90 = "pct90", Horz = "horz", Vert = "vert", LtHorz = "ltHorz",
        LtVert = "ltVert", DkHorz = "dkHorz", DkVert = "dkVert", NarHorz = "narHorz",
        NarVert = "narVert", DashHorz = "dashHorz", DashVert = "dashVert", Cross = "cross",
        DnDiag = "dnDiag", UpDiag = "upDiag", LtDnDiag = "ltDnDiag", LtUpDiag = "ltUpDiag",
        DkDnDiag = "dkDnDiag", DkUpDiag = "dkUpDiag", WdDnDiag = "wdDnDiag",
        WdUpDiag = "wdUpDiag", DashDnDiag = "dashDnDiag", DashUpDiag = "dashUpDiag",
        DiagCross = "diagCross", SmCheck = "smCheck", LgCheck = "lgCheck", SmGrid = "smGrid",
        LgGrid = "lgGrid", DotGrid = "dotGrid", SmConfetti = "smConfetti",
        LgConfetti = "lgConfetti", HorzBrick = "horzBrick", DiagBrick = "diagBrick",
        SolidDmnd = "solidDmnd", OpenDmnd = "openDmnd", DotDmnd = "dotDmnd", Plaid = "plaid",
        Sphere = "sphere", Weave = "weave", Divot = "divot", Shingle = "shingle", Wave = "wave",
        Trellis = "trellis", ZigZag = "zigZag",
    }
}

st_enum! {
    /// ST_BlackWhiteMode.
    BlackWhiteMode("ST_BlackWhiteMode") {
        Clr = "clr", Auto = "auto", Gray = "gray", LtGray = "ltGray", InvGray = "invGray",
        GrayWhite = "grayWhite", BlackGray = "blackGray", BlackWhite = "blackWhite",
        Black = "black", White = "white", Hidden = "hidden",
    }
}

st_enum! {
    /// ST_FontCollectionIndex.
    FontCollectionIndex("ST_FontCollectionIndex") { Major = "major", Minor = "minor", None = "none" }
}

st_enum! {
    /// ST_TargetMode: whether a relationship target is inside the package.
    TargetMode("ST_TargetMode") { Internal = "Internal", External = "External" }
}

/// ST_OnOff / `xsd:boolean`: `true|false|1|0|on|off` (any case, surrounding whitespace ignored).
/// Unrecognised values are `None`.
#[must_use]
pub fn parse_on_off(v: &str) -> Option<bool> {
    let t = v.trim();
    if t.eq_ignore_ascii_case("true") || t == "1" || t.eq_ignore_ascii_case("on") {
        Some(true)
    } else if t.eq_ignore_ascii_case("false") || t == "0" || t.eq_ignore_ascii_case("off") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerations_round_trip_their_spelling() {
        for v in PresetPatternVal::ALL {
            assert_eq!(PresetPatternVal::parse(v.as_str()), Some(*v));
        }
        assert_eq!(PresetPatternVal::ALL.len(), 54);
        assert_eq!(TextAutonumberScheme::ALL.len(), 41);
        assert_eq!(SystemColorVal::ALL.len(), 30);
        assert_eq!(
            SystemColorVal::parse("3dDkShadow"),
            Some(SystemColorVal::ThreeDDkShadow)
        );
        assert_eq!(ColorSchemeIndex::Accent1.index(), 4);
        assert_eq!("dbl".parse::<CompoundLine>(), Ok(CompoundLine::Dbl));
        assert!("Dbl".parse::<CompoundLine>().is_err());
        assert_eq!(LineCap::Rnd.to_string(), "rnd");
    }

    #[test]
    fn on_off_accepts_every_spelling() {
        for t in ["true", "1", "on", "On", " TRUE "] {
            assert_eq!(parse_on_off(t), Some(true), "{t}");
        }
        for f in ["false", "0", "off", "OFF"] {
            assert_eq!(parse_on_off(f), Some(false), "{f}");
        }
        assert_eq!(parse_on_off("yes"), None);
    }
}
