// SPDX-License-Identifier: AGPL-3.0-or-later
//! WordprocessingML simple types (`ST_*`) used by the model.
//!
//! Every enumeration knows its schema spelling (`as_str`, `parse`, `Display`) so readers and
//! writers convert without tables of their own. Serialisation uses the schema spelling in
//! self-describing formats (JSON) and the variant index in postcard. "Open" enumerations, whose
//! schema allows values the model does not list (`ST_Border`, `ST_Shd`, `ST_NumberFormat`), keep an
//! unknown value verbatim in `Other` so it round-trips.

/// Defines a closed string enumeration.
macro_rules! st_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident = $s:literal),+ $(,)? }
        default $default:ident
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            serde::Serialize, serde::Deserialize,
        )]
        pub enum $name {
            $(
                #[doc = concat!("`", $s, "`")]
                $(#[$vmeta])*
                #[serde(rename = $s)]
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
            pub fn parse(s: &str) -> Option<Self> {
                match s {
                    $($s => Some($name::$variant),)+
                    _ => None,
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                $name::$default
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

/// Defines an open string enumeration with an `Other(String)` fallback.
macro_rules! st_open_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident = $s:literal),+ $(,)? }
        default $default:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            $(
                #[doc = concat!("`", $s, "`")]
                $(#[$vmeta])*
                #[serde(rename = $s)]
                $variant,
            )+
            /// A value the model does not list, kept verbatim.
            Other(String),
        }

        impl $name {
            /// Every listed value, in schema order.
            pub const KNOWN: &'static [$name] = &[$($name::$variant),+];

            /// The schema spelling.
            #[must_use]
            pub fn as_str(&self) -> &str {
                match self {
                    $($name::$variant => $s,)+
                    $name::Other(s) => s.as_str(),
                }
            }

            /// Parses a schema spelling; unknown values become `Other`.
            #[must_use]
            pub fn parse(s: &str) -> Self {
                match s {
                    $($s => $name::$variant,)+
                    other => $name::Other(other.to_owned()),
                }
            }

            /// True for a value the model lists.
            #[must_use]
            pub fn is_known(&self) -> bool {
                !matches!(self, $name::Other(_))
            }
        }

        impl Default for $name {
            fn default() -> Self {
                $name::$default
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

st_enum! {
    /// `ST_Jc` for paragraphs. `both` is Word's "Justify"; readers normalise `start`/`end` to
    /// `left`/`right`.
    Alignment {
        Left = "left", Center = "center", Right = "right", Both = "both",
        Distribute = "distribute", MediumKashida = "mediumKashida", HighKashida = "highKashida",
        LowKashida = "lowKashida", ThaiDistribute = "thaiDistribute",
    }
    default Left
}

st_enum! {
    /// `ST_LineSpacingRule`: `auto` is a multiple of a line in 240ths; `exact` and `atLeast` are
    /// twips.
    LineRule { Auto = "auto", Exact = "exact", AtLeast = "atLeast" }
    default Auto
}

st_enum! {
    /// `ST_Underline`.
    UnderlineStyle {
        Single = "single", Words = "words", Double = "double", Thick = "thick", Dotted = "dotted",
        DottedHeavy = "dottedHeavy", Dash = "dash", DashedHeavy = "dashedHeavy",
        DashLong = "dashLong", DashLongHeavy = "dashLongHeavy", DotDash = "dotDash",
        DashDotHeavy = "dashDotHeavy", DotDotDash = "dotDotDash",
        DashDotDotHeavy = "dashDotDotHeavy", Wave = "wave", WavyHeavy = "wavyHeavy",
        WavyDouble = "wavyDouble", None = "none",
    }
    default Single
}

st_enum! {
    /// `ST_VerticalAlignRun`.
    VertAlign { Baseline = "baseline", Superscript = "superscript", Subscript = "subscript" }
    default Baseline
}

st_enum! {
    /// `ST_HighlightColor`: the 16 highlighter colours of Word plus `none`.
    HighlightColor {
        Black = "black", Blue = "blue", Cyan = "cyan", Green = "green", Magenta = "magenta",
        Red = "red", Yellow = "yellow", White = "white", DarkBlue = "darkBlue",
        DarkCyan = "darkCyan", DarkGreen = "darkGreen", DarkMagenta = "darkMagenta",
        DarkRed = "darkRed", DarkYellow = "darkYellow", DarkGray = "darkGray",
        LightGray = "lightGray", None = "none",
    }
    default Yellow
}

impl HighlightColor {
    /// The colour Word paints for this highlight (`None` for `none`).
    #[must_use]
    pub const fn rgb(self) -> Option<kiln_common::Color> {
        use kiln_common::Color;
        Some(match self {
            Self::Black => Color::rgb(0x00, 0x00, 0x00),
            Self::Blue => Color::rgb(0x00, 0x00, 0xFF),
            Self::Cyan => Color::rgb(0x00, 0xFF, 0xFF),
            Self::Green => Color::rgb(0x00, 0xFF, 0x00),
            Self::Magenta => Color::rgb(0xFF, 0x00, 0xFF),
            Self::Red => Color::rgb(0xFF, 0x00, 0x00),
            Self::Yellow => Color::rgb(0xFF, 0xFF, 0x00),
            Self::White => Color::rgb(0xFF, 0xFF, 0xFF),
            Self::DarkBlue => Color::rgb(0x00, 0x00, 0x80),
            Self::DarkCyan => Color::rgb(0x00, 0x80, 0x80),
            Self::DarkGreen => Color::rgb(0x00, 0x80, 0x00),
            Self::DarkMagenta => Color::rgb(0x80, 0x00, 0x80),
            Self::DarkRed => Color::rgb(0x80, 0x00, 0x00),
            Self::DarkYellow => Color::rgb(0x80, 0x80, 0x00),
            Self::DarkGray => Color::rgb(0x80, 0x80, 0x80),
            Self::LightGray => Color::rgb(0xC0, 0xC0, 0xC0),
            Self::None => return None,
        })
    }
}

st_enum! {
    /// `ST_ThemeColor`, as used by `w:color/@w:themeColor` and `w:shd/@w:themeFill`.
    ThemeColorName {
        Dark1 = "dark1", Light1 = "light1", Dark2 = "dark2", Light2 = "light2",
        Accent1 = "accent1", Accent2 = "accent2", Accent3 = "accent3", Accent4 = "accent4",
        Accent5 = "accent5", Accent6 = "accent6", Hyperlink = "hyperlink",
        FollowedHyperlink = "followedHyperlink", None = "none", Background1 = "background1",
        Text1 = "text1", Background2 = "background2", Text2 = "text2",
    }
    default Text1
}

st_open_enum! {
    /// `ST_Border`. The common styles are listed; the art borders pass through as `Other`.
    BorderStyle {
        Nil = "nil", None = "none", Single = "single", Thick = "thick", Double = "double",
        Dotted = "dotted", Dashed = "dashed", DotDash = "dotDash", DotDotDash = "dotDotDash",
        Triple = "triple", ThinThickSmallGap = "thinThickSmallGap",
        ThickThinSmallGap = "thickThinSmallGap", ThinThickThinSmallGap = "thinThickThinSmallGap",
        ThinThickMediumGap = "thinThickMediumGap", ThickThinMediumGap = "thickThinMediumGap",
        ThinThickThinMediumGap = "thinThickThinMediumGap",
        ThinThickLargeGap = "thinThickLargeGap", ThickThinLargeGap = "thickThinLargeGap",
        ThinThickThinLargeGap = "thinThickThinLargeGap", Wave = "wave",
        DoubleWave = "doubleWave", DashSmallGap = "dashSmallGap",
        DashDotStroked = "dashDotStroked", ThreeDEmboss = "threeDEmboss",
        ThreeDEngrave = "threeDEngrave", Outset = "outset", Inset = "inset",
    }
    default Single
}

impl BorderStyle {
    /// True for `nil` and `none`, which draw nothing.
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::Nil | Self::None)
    }
}

st_open_enum! {
    /// `ST_Shd`: the shading pattern.
    ShadingPattern {
        Nil = "nil", Clear = "clear", Solid = "solid", HorzStripe = "horzStripe",
        VertStripe = "vertStripe", ReverseDiagStripe = "reverseDiagStripe",
        DiagStripe = "diagStripe", HorzCross = "horzCross", DiagCross = "diagCross",
        ThinHorzStripe = "thinHorzStripe", ThinVertStripe = "thinVertStripe",
        ThinReverseDiagStripe = "thinReverseDiagStripe", ThinDiagStripe = "thinDiagStripe",
        ThinHorzCross = "thinHorzCross", ThinDiagCross = "thinDiagCross", Pct5 = "pct5",
        Pct10 = "pct10", Pct12 = "pct12", Pct15 = "pct15", Pct20 = "pct20", Pct25 = "pct25",
        Pct30 = "pct30", Pct35 = "pct35", Pct37 = "pct37", Pct40 = "pct40", Pct45 = "pct45",
        Pct50 = "pct50", Pct55 = "pct55", Pct60 = "pct60", Pct62 = "pct62", Pct65 = "pct65",
        Pct70 = "pct70", Pct75 = "pct75", Pct80 = "pct80", Pct85 = "pct85", Pct87 = "pct87",
        Pct90 = "pct90", Pct95 = "pct95",
    }
    default Clear
}

st_enum! {
    /// `ST_TabJc`. Readers normalise `start`/`end` to `left`/`right`.
    TabAlignment {
        Left = "left", Center = "center", Right = "right", Decimal = "decimal", Bar = "bar",
        Clear = "clear", Num = "num",
    }
    default Left
}

st_enum! {
    /// `ST_TabTlc`: the tab leader.
    TabLeader {
        None = "none", Dot = "dot", Hyphen = "hyphen", Underscore = "underscore",
        Heavy = "heavy", MiddleDot = "middleDot",
    }
    default None
}

st_enum! {
    /// `ST_TextAlignment`: vertical alignment of characters on a line.
    TextAlignment {
        Top = "top", Center = "center", Baseline = "baseline", Bottom = "bottom", Auto = "auto",
    }
    default Auto
}

st_enum! {
    /// `ST_Theme`: theme font references of `w:rFonts`.
    ThemeFontRef {
        MajorAscii = "majorAscii", MajorHAnsi = "majorHAnsi", MajorEastAsia = "majorEastAsia",
        MajorBidi = "majorBidi", MinorAscii = "minorAscii", MinorHAnsi = "minorHAnsi",
        MinorEastAsia = "minorEastAsia", MinorBidi = "minorBidi",
    }
    default MinorHAnsi
}

impl ThemeFontRef {
    /// True for the heading (major) collection.
    #[must_use]
    pub const fn is_major(self) -> bool {
        matches!(
            self,
            Self::MajorAscii | Self::MajorHAnsi | Self::MajorEastAsia | Self::MajorBidi
        )
    }
}

st_enum! {
    /// `ST_FrameScale`-like drop cap kind (`w:framePr/@w:dropCap`).
    DropCapKind { Drop = "drop", Margin = "margin" }
    default Drop
}

st_enum! {
    /// `ST_TblWidth`: `dxa` = twips, `pct` = fiftieths of a percent (5000 = 100 %).
    WidthType { Dxa = "dxa", Pct = "pct", Auto = "auto", Nil = "nil" }
    default Auto
}

st_enum! {
    /// `ST_JcTable`. Readers normalise `start`/`end` to `left`/`right`.
    TableAlignment { Left = "left", Center = "center", Right = "right" }
    default Left
}

st_enum! {
    /// `ST_TblLayoutType`.
    TableLayout { Fixed = "fixed", Autofit = "autofit" }
    default Autofit
}

st_enum! {
    /// `ST_HeightRule`.
    HeightRule { Auto = "auto", AtLeast = "atLeast", Exact = "exact" }
    default Auto
}

st_enum! {
    /// `ST_VerticalJc` for table cells.
    CellVAlign { Top = "top", Center = "center", Bottom = "bottom" }
    default Top
}

st_enum! {
    /// `ST_TextDirection`.
    TextDirection {
        LrTb = "lrTb", TbRl = "tbRl", BtLr = "btLr", LrTbV = "lrTbV", TbRlV = "tbRlV",
        TbLrV = "tbLrV",
    }
    default LrTb
}

st_enum! {
    /// `ST_Merge`: vertical merge state of a table cell.
    VMerge { Restart = "restart", Continue = "continue" }
    default Continue
}

st_enum! {
    /// `ST_TblStyleOverrideType`: the conditional regions of a table style.
    TableCondition {
        WholeTable = "wholeTable", FirstRow = "firstRow", LastRow = "lastRow",
        FirstCol = "firstCol", LastCol = "lastCol", Band1Vert = "band1Vert",
        Band2Vert = "band2Vert", Band1Horz = "band1Horz", Band2Horz = "band2Horz",
        NeCell = "neCell", NwCell = "nwCell", SeCell = "seCell", SwCell = "swCell",
    }
    default WholeTable
}

st_enum! {
    /// `ST_SectionMark`: how a section starts relative to the previous one.
    SectionBreak {
        NextPage = "nextPage", Continuous = "continuous", EvenPage = "evenPage",
        OddPage = "oddPage", NextColumn = "nextColumn",
    }
    default NextPage
}

st_enum! {
    /// `ST_HdrFtr`: which pages a header or footer is for.
    HeaderFooterKind { Default = "default", First = "first", Even = "even" }
    default Default
}

st_enum! {
    /// `ST_PageOrientation`.
    Orientation { Portrait = "portrait", Landscape = "landscape" }
    default Portrait
}

st_enum! {
    /// `ST_VerticalJc` for sections.
    PageVAlign { Top = "top", Center = "center", Both = "both", Bottom = "bottom" }
    default Top
}

st_enum! {
    /// `ST_LineNumberRestart`.
    LineNumberRestart {
        NewPage = "newPage", NewSection = "newSection", Continuous = "continuous",
    }
    default NewPage
}

st_enum! {
    /// How a floating drawing wraps text (`wp:wrap*`; `behind` and `inFront` are `wp:wrapNone`
    /// with `behindDoc` on or off).
    WrapType {
        Inline = "inline", Square = "square", Tight = "tight", Through = "through",
        TopAndBottom = "topAndBottom", Behind = "behind", InFront = "inFront", None = "none",
    }
    default Square
}

st_enum! {
    /// `ST_WrapText`: which sides text flows on.
    WrapSide { BothSides = "bothSides", Left = "left", Right = "right", Largest = "largest" }
    default BothSides
}

st_enum! {
    /// `ST_RelFromH`.
    RelativeFromH {
        Character = "character", Column = "column", InsideMargin = "insideMargin",
        LeftMargin = "leftMargin", Margin = "margin", OutsideMargin = "outsideMargin",
        Page = "page", RightMargin = "rightMargin",
    }
    default Column
}

st_enum! {
    /// `ST_RelFromV`.
    RelativeFromV {
        BottomMargin = "bottomMargin", InsideMargin = "insideMargin", Line = "line",
        Margin = "margin", OutsideMargin = "outsideMargin", Page = "page",
        Paragraph = "paragraph", TopMargin = "topMargin",
    }
    default Paragraph
}

st_enum! {
    /// `ST_AlignH`.
    AlignH {
        Left = "left", Right = "right", Center = "center", Inside = "inside",
        Outside = "outside",
    }
    default Left
}

st_enum! {
    /// `ST_AlignV`.
    AlignV {
        Top = "top", Bottom = "bottom", Center = "center", Inside = "inside",
        Outside = "outside",
    }
    default Top
}

st_enum! {
    /// `ST_BrType`.
    BreakKind { Page = "page", Column = "column", TextWrapping = "textWrapping" }
    default TextWrapping
}

st_enum! {
    /// `ST_BrClear`.
    BreakClear { None = "none", Left = "left", Right = "right", All = "all" }
    default None
}

st_enum! {
    /// Footnote or endnote.
    NoteKind { Footnote = "footnote", Endnote = "endnote" }
    default Footnote
}

st_enum! {
    /// `ST_FtnEdn`: the special notes of `footnotes.xml`/`endnotes.xml`.
    NoteType {
        Normal = "normal", Separator = "separator",
        ContinuationSeparator = "continuationSeparator",
        ContinuationNotice = "continuationNotice",
    }
    default Normal
}

st_enum! {
    /// `ST_StyleType`.
    StyleType {
        Paragraph = "paragraph", Character = "character", Table = "table",
        Numbering = "numbering",
    }
    default Paragraph
}

st_enum! {
    /// `ST_MultiLevelType`.
    MultiLevelType {
        SingleLevel = "singleLevel", Multilevel = "multilevel",
        HybridMultilevel = "hybridMultilevel",
    }
    default HybridMultilevel
}

st_enum! {
    /// `ST_LevelSuffix`: what follows a list label.
    LevelSuffix { Tab = "tab", Space = "space", Nothing = "nothing" }
    default Tab
}

st_enum! {
    /// `ST_Jc` for list labels (`w:lvlJc`).
    LevelAlignment { Left = "left", Center = "center", Right = "right" }
    default Left
}

st_enum! {
    /// The kind of a content control, derived from `w:sdtPr`.
    SdtKind {
        RichText = "richText", Text = "text", Date = "date", DropDownList = "dropDownList",
        ComboBox = "comboBox", Checkbox = "checkbox", Picture = "picture",
        DocPartObj = "docPartObj", DocPartList = "docPartList", Group = "group",
        Bibliography = "bibliography", Citation = "citation", Equation = "equation",
        RepeatingSection = "repeatingSection", RepeatingSectionItem = "repeatingSectionItem",
        Other = "other",
    }
    default RichText
}

st_open_enum! {
    /// `ST_NumberFormat` (ECMA-376 §17.18.59): list labels, page numbers and field `\*` switches.
    /// Formats the model cannot render fall back to decimal, as Word does for formats a locale
    /// cannot show.
    NumberFormat {
        Decimal = "decimal", DecimalZero = "decimalZero", UpperRoman = "upperRoman",
        LowerRoman = "lowerRoman", UpperLetter = "upperLetter", LowerLetter = "lowerLetter",
        Ordinal = "ordinal", CardinalText = "cardinalText", OrdinalText = "ordinalText",
        Hex = "hex", Chicago = "chicago", Bullet = "bullet", None = "none", Custom = "custom",
        DecimalEnclosedCircle = "decimalEnclosedCircle",
        DecimalEnclosedFullstop = "decimalEnclosedFullstop",
        DecimalEnclosedParen = "decimalEnclosedParen", DecimalFullWidth = "decimalFullWidth",
        DecimalHalfWidth = "decimalHalfWidth", NumberInDash = "numberInDash",
        RussianLower = "russianLower", RussianUpper = "russianUpper", Hebrew1 = "hebrew1",
        Hebrew2 = "hebrew2", ArabicAlpha = "arabicAlpha", ArabicAbjad = "arabicAbjad",
        HindiNumbers = "hindiNumbers", ThaiNumbers = "thaiNumbers",
        IdeographDigital = "ideographDigital", JapaneseCounting = "japaneseCounting",
        ChineseCounting = "chineseCounting",
        ChineseCountingThousand = "chineseCountingThousand", Aiueo = "aiueo", Iroha = "iroha",
        AiueoFullWidth = "aiueoFullWidth", IrohaFullWidth = "irohaFullWidth",
    }
    default Decimal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_enums_round_trip_their_spelling() {
        for a in Alignment::ALL {
            assert_eq!(Alignment::parse(a.as_str()), Some(*a));
        }
        for u in UnderlineStyle::ALL {
            assert_eq!(UnderlineStyle::parse(&u.to_string()), Some(*u));
        }
        assert_eq!(
            TableCondition::parse("nwCell"),
            Some(TableCondition::NwCell)
        );
        assert_eq!(Alignment::parse("justify"), None);
        assert_eq!(Alignment::default(), Alignment::Left);
    }

    #[test]
    fn open_enums_keep_unknown_values() {
        assert_eq!(BorderStyle::parse("single"), BorderStyle::Single);
        let art = BorderStyle::parse("apples");
        assert_eq!(art, BorderStyle::Other("apples".into()));
        assert_eq!(art.as_str(), "apples");
        assert!(!art.is_known());
        assert!(BorderStyle::Nil.is_none() && !BorderStyle::Double.is_none());
        assert_eq!(NumberFormat::parse("lowerRoman"), NumberFormat::LowerRoman);
        assert_eq!(
            NumberFormat::parse("koreanCounting").to_string(),
            "koreanCounting"
        );
        assert!(ShadingPattern::KNOWN.contains(&ShadingPattern::Pct50));
    }

    #[test]
    fn serde_uses_schema_spelling_in_json() {
        let json = serde_json::to_string(&LineRule::AtLeast).unwrap_or_default();
        assert_eq!(json, "\"atLeast\"");
        let back: Option<LineRule> = serde_json::from_str("\"exact\"").ok();
        assert_eq!(back, Some(LineRule::Exact));
        let other = serde_json::to_string(&BorderStyle::Other("x".into())).unwrap_or_default();
        assert_eq!(other, "{\"Other\":\"x\"}");
    }

    #[test]
    fn highlight_colours_are_words() {
        assert_eq!(
            HighlightColor::Yellow.rgb(),
            Some(kiln_common::Color::rgb(255, 255, 0))
        );
        assert_eq!(HighlightColor::None.rgb(), None);
        assert!(ThemeFontRef::MajorBidi.is_major() && !ThemeFontRef::MinorAscii.is_major());
    }
}
