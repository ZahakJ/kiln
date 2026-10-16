// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML colours, themes, fills, lines, effects and text properties. Ported from
//! `reference/ts-core/ooxml/test/drawingml.test.ts`.

mod common;

use common::fixture;
use kiln_ooxml::drawingml::color::{
    ColorContext, ColorMap, ColorModel, DrawingColor, apply_excel_tint, hex_to_rgba, parse_color,
    parse_color_child, preset_color_hex, resolve_color, resolve_color_hex, rgb_to_hsl, rgba_to_css,
    serialize_color,
};
use kiln_ooxml::drawingml::effects::{parse_effect_list, serialize_effect_list};
use kiln_ooxml::drawingml::fill::{
    BlipFillMode, Fill, RelativeRect, fill_to_css, find_fill, parse_fill, serialize_fill,
    serialize_fill_named,
};
use kiln_ooxml::drawingml::line::{LineDash, LineEnd, LineJoin, parse_line, serialize_line};
use kiln_ooxml::drawingml::text::{
    Autofit, Bullet, BulletFont, BulletSize, TabStop, TextSpacing, merge_paragraph_properties,
    merge_run_properties, parse_body_properties, parse_list_style, parse_paragraph_properties,
    parse_run_properties, serialize_body_properties, serialize_list_style,
    serialize_paragraph_properties, serialize_run_properties,
};
use kiln_ooxml::drawingml::theme::{
    Theme, office_theme, parse_theme, parse_theme_bytes, resolve_theme_font, serialize_theme,
    theme_color_context, theme_color_for_index,
};
use kiln_ooxml::opc::{OpcPackage, rel_type};
use kiln_ooxml::st::{
    ColorSchemeIndex, CompoundLine, LineCap, LineEndSize, LineEndType, PenAlignment,
    PresetLineDashVal, RectAlignment, TextAlignType, TextAnchoringType, TextAutonumberScheme,
    TextCapsType, TextStrikeType, TextTabAlignType, TextUnderlineType, TextVerticalType,
};
use kiln_ooxml::xml::{WriteOptions, XmlElement, namespaces::ns, parse_xml_str, write_element};

/// Parses a DrawingML fragment, declaring `a:` on its root.
fn x(s: &str) -> XmlElement {
    let i = s.find([' ', '>', '/']).unwrap();
    let with_ns = format!(r#"{} xmlns:a="{}"{}"#, &s[..i], ns::A, &s[i..]);
    parse_xml_str(&with_ns).unwrap().root
}

fn xml(e: &XmlElement) -> String {
    write_element(e, &WriteOptions::fragment()).unwrap()
}

/// Serialise → parse: the round trip every DrawingML mapper must survive.
fn reparse(e: &XmlElement) -> XmlElement {
    parse_xml_str(&xml(e)).unwrap().root
}

fn ctx() -> ColorContext {
    theme_color_context(&office_theme(), ColorMap::default(), None)
}

fn scheme(slot: &str, t: &[(&str, i64)]) -> DrawingColor {
    t.iter()
        .fold(DrawingColor::scheme(slot), |c, (n, v)| c.with(n, *v))
}

mod colors {
    use super::*;

    #[test]
    fn theme_colour_resolution_matches_office() {
        type Case = (&'static str, &'static [(&'static str, i64)], &'static str);
        let cases: &[Case] = &[
            ("accent1", &[], "4472C4"),
            ("accent1", &[("lumMod", 20000), ("lumOff", 80000)], "DAE3F3"),
            ("accent1", &[("lumMod", 40000), ("lumOff", 60000)], "B4C7E7"),
            ("accent1", &[("lumMod", 60000), ("lumOff", 40000)], "8FAADC"),
            ("accent1", &[("lumMod", 75000)], "2F5597"),
            ("accent1", &[("lumMod", 50000)], "203864"),
            ("accent2", &[("lumMod", 20000), ("lumOff", 80000)], "FBE5D6"),
            ("accent2", &[("lumMod", 60000), ("lumOff", 40000)], "F4B183"),
            ("accent2", &[("lumMod", 75000)], "C55A11"),
            ("accent6", &[("lumMod", 40000), ("lumOff", 60000)], "C5E0B4"),
            ("accent6", &[("lumMod", 75000)], "548235"),
            // The colour map: bg/tx → lt/dk.
            ("bg1", &[("lumMod", 95000)], "F2F2F2"),
            ("bg1", &[("lumMod", 85000)], "D9D9D9"),
            ("bg1", &[("lumMod", 75000)], "BFBFBF"),
            ("bg1", &[("lumMod", 65000)], "A6A6A6"),
            ("tx1", &[("lumMod", 95000), ("lumOff", 5000)], "0D0D0D"),
            ("tx1", &[("lumMod", 85000), ("lumOff", 15000)], "262626"),
            ("tx1", &[("lumMod", 75000), ("lumOff", 25000)], "404040"),
            ("tx1", &[("lumMod", 65000), ("lumOff", 35000)], "595959"),
            ("bg2", &[("lumMod", 90000)], "D0CECE"),
            ("tx2", &[("lumMod", 60000), ("lumOff", 40000)], "8497B0"),
        ];
        let ctx = ctx();
        for (slot, t, hex) in cases {
            assert_eq!(
                resolve_color_hex(&scheme(slot, t), &ctx),
                *hex,
                "{slot} {t:?}"
            );
        }
    }

    #[test]
    fn matches_office_within_one_step_where_office_quantises_hsl() {
        // Office shows 843C0C; exact HSL arithmetic gives 843C0B.
        let got = hex_to_rgba(
            &resolve_color_hex(&scheme("accent2", &[("lumMod", 50000)]), &ctx()),
            1.0,
        );
        let want = hex_to_rgba("843C0C", 1.0);
        assert!((got.r - want.r).abs() <= 1.0);
        assert!((got.g - want.g).abs() <= 1.0);
        assert!((got.b - want.b).abs() <= 1.0);
    }

    #[test]
    fn honours_a_custom_colour_map_and_ph_clr() {
        let theme = office_theme();
        let mut map = ColorMap::default();
        map.set("bg1", ColorSchemeIndex::Dk1);
        map.set("tx1", ColorSchemeIndex::Lt1);
        let inverted = theme_color_context(&theme, map, None);
        assert_eq!(
            resolve_color_hex(&DrawingColor::scheme("bg1"), &inverted),
            "000000"
        );
        assert_eq!(
            resolve_color_hex(&DrawingColor::scheme("tx1"), &inverted),
            "FFFFFF"
        );
        let with_ph = theme_color_context(
            &theme,
            ColorMap::default(),
            Some(hex_to_rgba("ED7D31", 1.0)),
        );
        assert_eq!(
            resolve_color_hex(&scheme("phClr", &[("shade", 50000)]), &with_ph),
            resolve_color_hex(
                &DrawingColor::srgb("ED7D31").with("shade", 50000),
                &ColorContext::default()
            )
        );
        assert_eq!(
            resolve_color_hex(&DrawingColor::scheme("accent9"), &ctx()),
            "000000"
        );
    }

    #[test]
    fn reads_colour_maps_in_both_vocabularies() {
        let clr_map = parse_xml_str(r#"<p:clrMap bg1="dk1" tx1="lt1" bg2="dk2" tx2="lt2" accent1="accent2" hlink="folHlink" folHlink="hlink"/>"#.replace("<p:clrMap", "<clrMap").as_str()).unwrap().root;
        let m = ColorMap::parse(&clr_map);
        assert_eq!(m.bg1, ColorSchemeIndex::Dk1);
        assert_eq!(m.accents[0], ColorSchemeIndex::Accent2);
        assert_eq!(m.hlink, ColorSchemeIndex::FolHlink);
        let word = parse_xml_str(&format!(
            r#"<w:clrSchemeMapping xmlns:w="{}" w:bg1="dark1" w:t1="light1" w:hyperlink="followedHyperlink"/>"#,
            ns::W
        ))
        .unwrap()
        .root;
        let m = ColorMap::parse(&word);
        assert_eq!(m.bg1, ColorSchemeIndex::Dk1);
        assert_eq!(m.tx1, ColorSchemeIndex::Lt1);
        assert_eq!(m.hlink, ColorSchemeIndex::FolHlink);
        assert_eq!(
            m.bg2,
            ColorSchemeIndex::Lt2,
            "unset entries keep the default"
        );
    }

    #[test]
    fn applies_tint_shade_alpha_and_other_transforms() {
        let none = ColorContext::default();
        let hex = |c: DrawingColor| resolve_color_hex(&c, &none);
        assert_eq!(
            hex(DrawingColor::srgb("000000").with("tint", 50000)),
            "BCBCBC"
        );
        assert_eq!(
            hex(DrawingColor::srgb("FFFFFF").with("shade", 50000)),
            "BCBCBC"
        );
        assert_eq!(
            hex(DrawingColor::srgb("4472C4").with("tint", 100000)),
            "4472C4"
        );
        let a = resolve_color(&DrawingColor::srgb("FF0000").with("alpha", 40000), &none);
        assert!((a.a - 0.4).abs() < 1e-9);
        assert_eq!(rgba_to_css(a), "rgba(255, 0, 0, 0.4)");
        let a2 = resolve_color(
            &DrawingColor::srgb("FF0000")
                .with("alpha", 50000)
                .with("alphaMod", 50000),
            &none,
        );
        assert!((a2.a - 0.25).abs() < 1e-9);
        assert_eq!(
            hex(DrawingColor::srgb("FF0000").with_flag("comp")),
            "00FFFF"
        );
        assert_eq!(hex(DrawingColor::srgb("FF0000").with_flag("inv")), "00FFFF");
        assert_eq!(
            hex(DrawingColor::srgb("FF0000").with_flag("gray")),
            "383838"
        );
        assert_eq!(
            hex(DrawingColor::srgb("FF0000").with("hueOff", 7_200_000)),
            "00FF00"
        );
        assert_eq!(
            hex(DrawingColor::srgb("808080").with("satMod", 200_000)),
            "808080"
        );
        assert_eq!(
            hex(DrawingColor::srgb("336699").with("red", 100_000)),
            "FF6699"
        );
        assert_eq!(
            hex(DrawingColor::srgb("123456").with("unknownTransform", 1)),
            "123456"
        );
    }

    #[test]
    fn resolves_system_preset_hsl_and_scrgb_colours() {
        let none = ColorContext::default();
        let r = |s: &str| resolve_color_hex(&parse_color(&x(s)).unwrap(), &none);
        assert_eq!(
            r(r#"<a:sysClr val="windowText" lastClr="111111"/>"#),
            "111111"
        );
        assert_eq!(r(r#"<a:sysClr val="window"/>"#), "FFFFFF");
        assert_eq!(r(r#"<a:prstClr val="dkBlue"/>"#), "00008B");
        assert_eq!(preset_color_hex("medPurple"), Some("9370DB"));
        assert_eq!(preset_color_hex("ltGray"), Some("D3D3D3"));
        assert_eq!(preset_color_hex("nope"), None);
        assert_eq!(
            r(r#"<a:hslClr hue="14400000" sat="100000" lum="50000"/>"#),
            "0000FF"
        );
        assert_eq!(r(r#"<a:scrgbClr r="100000" g="0" b="21586"/>"#), "FF0080");
        // Strict documents write percentages with a percent sign.
        assert_eq!(
            r(r#"<a:hslClr hue="14400000" sat="100%" lum="50%"/>"#),
            "0000FF"
        );
    }

    #[test]
    fn round_trips_colour_xml_with_transforms_in_order() {
        let src = x(
            r#"<a:schemeClr val="accent1"><a:lumMod val="60000"/><a:lumOff val="40000"/><a:alpha val="50000"/></a:schemeClr>"#,
        );
        let c = parse_color(&src).unwrap();
        let names: Vec<&str> = c.transforms.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["lumMod", "lumOff", "alpha"]);
        assert_eq!(parse_color(&reparse(&serialize_color(&c))), Some(c));
        assert_eq!(
            parse_color_child(Some(&x(
                r##"<a:solidFill><a:srgbClr val="#a1b2c3"/></a:solidFill>"##
            ))),
            Some(DrawingColor::srgb("A1B2C3"))
        );
        // Every colour model survives the round trip.
        for s in [
            r#"<a:srgbClr val="FF0000"><a:comp/></a:srgbClr>"#,
            r#"<a:sysClr val="windowText" lastClr="000000"/>"#,
            r#"<a:prstClr val="black"><a:alpha val="40000"/></a:prstClr>"#,
            r#"<a:hslClr hue="1" sat="2" lum="3"/>"#,
            r#"<a:scrgbClr r="1" g="2" b="3"/>"#,
        ] {
            let c = parse_color(&x(s)).unwrap();
            assert_eq!(
                parse_color(&reparse(&serialize_color(&c))).as_ref(),
                Some(&c),
                "{s}"
            );
        }
        assert!(matches!(
            parse_color(&x(r#"<a:sysClr val="window"/>"#))
                .unwrap()
                .model,
            ColorModel::System { last_clr: None, .. }
        ));
    }

    #[test]
    fn applies_spreadsheetml_tints_like_excel() {
        assert_eq!(
            apply_excel_tint("4472C4", 0.799_981_688_894_314_4),
            "DAE3F3"
        );
        assert_eq!(apply_excel_tint("4472C4", -0.249_977_111_117_893), "2F5597");
        assert_eq!(
            apply_excel_tint("FFFFFF", -0.149_998_474_074_526_2),
            "D9D9D9"
        );
        assert_eq!(theme_color_for_index(0), DrawingColor::scheme("lt1"));
        assert_eq!(theme_color_for_index(4), DrawingColor::scheme("accent1"));
        assert_eq!(theme_color_for_index(99), DrawingColor::scheme("dk1"));
    }

    #[test]
    fn converts_rgb_to_hsl() {
        let hsl = rgb_to_hsl(68.0, 114.0, 196.0);
        assert!((hsl.h - 218.44).abs() < 0.05);
        assert!((hsl.s - 0.5203).abs() < 0.0005);
        assert!((hsl.l - 0.5176).abs() < 0.0005);
    }

    #[test]
    fn converts_to_kiln_colours() {
        let c = resolve_color(
            &DrawingColor::srgb("4472C4").with("alpha", 50000),
            &ColorContext::default(),
        );
        let k = c.to_color();
        assert_eq!((k.r, k.g, k.b, k.a), (0x44, 0x72, 0xC4, 128));
    }
}

mod themes {
    use super::*;

    fn docx_theme() -> Vec<u8> {
        let pkg = OpcPackage::open(&fixture("docx/features/styles.docx")).unwrap();
        let part = pkg
            .related_part("/word/document.xml", rel_type::THEME)
            .unwrap()
            .unwrap();
        pkg.part_bytes(&part).unwrap().into_owned()
    }

    fn round_trip(t: &Theme) -> Theme {
        let doc = serialize_theme(t);
        let bytes = kiln_ooxml::xml::document_to_bytes(&doc).unwrap();
        parse_theme_bytes(&bytes).unwrap()
    }

    #[test]
    fn parses_colour_font_and_format_schemes_from_a_real_theme() {
        let t = parse_theme_bytes(&docx_theme()).unwrap();
        assert_eq!(t.name, "Office Theme");
        assert!(
            matches!(&t.color_scheme.get(ColorSchemeIndex::Dk1).model, ColorModel::System { val, .. } if val == "windowText")
        );
        assert!(
            matches!(&t.color_scheme.get(ColorSchemeIndex::Lt1).model, ColorModel::System { val, .. } if val == "window")
        );
        assert!(!t.font_scheme.major.latin.typeface.is_empty());
        assert!(t.font_scheme.minor.scripts.len() > 10);
        assert_eq!(t.format_scheme.fill_styles.len(), 3);
        assert_eq!(t.format_scheme.line_styles.len(), 3);
        assert_eq!(t.format_scheme.effect_styles.len(), 3);
        assert_eq!(t.format_scheme.bg_fill_styles.len(), 3);
        assert!(matches!(t.format_scheme.fill_styles[1], Fill::Gradient(_)));
    }

    #[test]
    fn round_trips_real_theme_parts_from_all_three_formats() {
        let parts = [
            docx_theme(),
            OpcPackage::open(&fixture("xlsx/features/styles.xlsx"))
                .unwrap()
                .part_bytes("/xl/theme/theme1.xml")
                .unwrap()
                .into_owned(),
            OpcPackage::open(&fixture("pptx/features/theme-colors.pptx"))
                .unwrap()
                .part_bytes("/ppt/theme/theme1.xml")
                .unwrap()
                .into_owned(),
        ];
        for bytes in parts {
            let t = parse_theme_bytes(&bytes).unwrap();
            assert_eq!(round_trip(&t), t);
        }
    }

    #[test]
    fn serialises_the_built_in_office_theme_and_reads_it_back() {
        let theme = office_theme();
        let s = String::from_utf8(
            kiln_ooxml::xml::document_to_bytes(&serialize_theme(&theme)).unwrap(),
        )
        .unwrap();
        assert!(s.contains(r#"<a:srgbClr val="4472C4"/>"#));
        assert!(s.contains(r#"typeface="Calibri Light""#));
        let back = round_trip(&theme);
        assert_eq!(
            back,
            Theme {
                extras: back.extras.clone(),
                ..theme
            }
        );
        let names: Vec<&str> = back.extras.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a:objectDefaults", "a:extraClrSchemeLst"]);
    }

    #[test]
    fn resolves_theme_font_references() {
        let t = office_theme();
        assert_eq!(resolve_theme_font("+mj-lt", &t), "Calibri Light");
        assert_eq!(resolve_theme_font("+mn-lt", &t), "Calibri");
        assert_eq!(resolve_theme_font("minorHAnsi", &t), "Calibri");
        assert_eq!(resolve_theme_font("majorAscii", &t), "Calibri Light");
        assert_eq!(resolve_theme_font("+mn-ea", &t), "");
        assert_eq!(resolve_theme_font("Georgia", &t), "Georgia");
        assert_eq!(resolve_theme_font("+zz-lt", &t), "+zz-lt");
    }

    #[test]
    fn rejects_other_xml() {
        let err = parse_theme(&parse_xml_str(r#"<a:foo xmlns:a="x"/>"#).unwrap().root).unwrap_err();
        assert!(err.to_string().contains("a:theme"));
    }
}

mod fills {
    use super::*;

    #[test]
    fn every_fill_type_round_trips() {
        let cases = [
            ("none", "<a:noFill/>".to_owned()),
            (
                "solid",
                r#"<a:solidFill><a:schemeClr val="accent2"><a:lumMod val="75000"/></a:schemeClr></a:solidFill>"#.to_owned(),
            ),
            (
                "linear gradient",
                r#"<a:gradFill flip="none" rotWithShape="1"><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="1"/><a:tileRect/></a:gradFill>"#.to_owned(),
            ),
            (
                "path gradient",
                r#"<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FFFFFF"/></a:gs><a:gs pos="100000"><a:srgbClr val="000000"/></a:gs></a:gsLst><a:path path="circle"><a:fillToRect l="50000" t="50000" r="50000" b="50000"/></a:path></a:gradFill>"#.to_owned(),
            ),
            (
                "pattern",
                r#"<a:pattFill prst="dkDnDiag"><a:fgClr><a:srgbClr val="4472C4"/></a:fgClr><a:bgClr><a:srgbClr val="FFFFFF"/></a:bgClr></a:pattFill>"#.to_owned(),
            ),
            (
                "picture (stretch, crop)",
                format!(
                    r#"<a:blipFill dpi="0" rotWithShape="1"><a:blip xmlns:r="{}" r:embed="rId2"><a:alphaModFix amt="50000"/><a:lum bright="20000"/></a:blip><a:srcRect l="10000" t="15000" r="20000" b="5000"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"#,
                    ns::R
                ),
            ),
            (
                "picture (tile)",
                format!(
                    r#"<a:blipFill><a:blip xmlns:r="{}" r:embed="rId3"/><a:tile tx="0" ty="0" sx="50000" sy="50000" flip="xy" algn="ctr"/></a:blipFill>"#,
                    ns::R
                ),
            ),
            ("group", "<a:grpFill/>".to_owned()),
        ];
        for (label, s) in cases {
            let fill = parse_fill(&x(&s)).unwrap_or_else(|| panic!("{label}"));
            assert_eq!(
                parse_fill(&reparse(&serialize_fill(&fill))).as_ref(),
                Some(&fill),
                "{label}"
            );
        }
    }

    #[test]
    fn models_picture_fill_details() {
        let f = parse_fill(&x(&format!(
            r#"<a:blipFill><a:blip xmlns:r="{}" r:embed="rId2"><a:alphaModFix amt="50000"/></a:blip><a:srcRect l="10000" r="20000"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"#,
            ns::R
        )))
        .unwrap();
        let Fill::Blip(b) = &f else {
            panic!("not a picture fill")
        };
        assert_eq!(b.embed.as_deref(), Some("rId2"));
        assert_eq!(b.mode, BlipFillMode::Stretch);
        assert_eq!(b.alpha, Some(0.5));
        assert_eq!(
            b.src_rect,
            Some(RelativeRect {
                l: 0.1,
                t: 0.0,
                r: 0.2,
                b: 0.0
            })
        );
        assert_eq!(serialize_fill_named(&f, "p:blipFill").name, "p:blipFill");
    }

    #[test]
    fn renders_css_backgrounds() {
        let none = ColorContext::default();
        assert_eq!(
            fill_to_css(&Fill::None, &none).as_deref(),
            Some("transparent")
        );
        assert_eq!(
            fill_to_css(&Fill::Solid(DrawingColor::scheme("accent1")), &ctx()).as_deref(),
            Some("#4472c4")
        );
        let lin = parse_fill(&x(r#"<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst><a:lin ang="5400000"/></a:gradFill>"#)).unwrap();
        assert_eq!(
            fill_to_css(&lin, &none).as_deref(),
            Some("linear-gradient(180deg, #ff0000 0%, #0000ff 100%)")
        );
        let rad = parse_fill(&x(r#"<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FFFFFF"/></a:gs><a:gs pos="100000"><a:srgbClr val="000000"/></a:gs></a:gsLst><a:path path="circle"><a:fillToRect l="50000" t="50000" r="50000" b="50000"/></a:path></a:gradFill>"#)).unwrap();
        assert_eq!(
            fill_to_css(&rad, &none).as_deref(),
            Some("radial-gradient(circle at 50% 50%, #ffffff 0%, #000000 100%)")
        );
        assert_eq!(
            find_fill(&x(
                r#"<a:spPr><a:xfrm/><a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></a:spPr>"#
            )),
            Some(Fill::Solid(DrawingColor::srgb("00FF00")))
        );
    }
}

mod lines {
    use super::*;

    const XML: &str = r#"<a:ln w="28575" cap="rnd" cmpd="dbl" algn="ctr"><a:solidFill><a:srgbClr val="C00000"/></a:solidFill><a:prstDash val="dashDot"/><a:miter lim="800000"/><a:headEnd type="oval" w="sm" len="sm"/><a:tailEnd type="triangle" w="lg" len="lg"/></a:ln>"#;

    #[test]
    fn parses_width_dash_caps_joins_and_ends() {
        let line = parse_line(&x(XML));
        assert_eq!(line.width, Some(28575));
        assert_eq!(line.cap, Some(LineCap::Rnd));
        assert_eq!(line.compound, Some(CompoundLine::Dbl));
        assert_eq!(line.align, Some(PenAlignment::Ctr));
        assert_eq!(line.fill, Some(Fill::Solid(DrawingColor::srgb("C00000"))));
        assert_eq!(
            line.dash,
            Some(LineDash::Preset(PresetLineDashVal::DashDot))
        );
        assert_eq!(line.join, Some(LineJoin::Miter(Some(8.0))));
        assert_eq!(
            line.head,
            Some(LineEnd {
                end_type: LineEndType::Oval,
                w: Some(LineEndSize::Sm),
                len: Some(LineEndSize::Sm)
            })
        );
        assert_eq!(
            line.tail,
            Some(LineEnd {
                end_type: LineEndType::Triangle,
                w: Some(LineEndSize::Lg),
                len: Some(LineEndSize::Lg)
            })
        );
        assert_eq!(parse_line(&reparse(&serialize_line(&line))), line);
        assert_eq!(
            xml(&serialize_line(&line)),
            XML.replace("<a:ln ", &format!(r#"<a:ln xmlns:a="{}" "#, ns::A))
        );
    }

    #[test]
    fn handles_custom_dashes_and_maps_to_strokes() {
        let line = parse_line(&x(
            r#"<a:ln w="12700"><a:custDash><a:ds d="300000" sp="100000"/></a:custDash><a:round/></a:ln>"#,
        ));
        assert_eq!(line.dash, Some(LineDash::Custom(vec![(300.0, 100.0)])));
        assert_eq!(line.dash_array_px(), Some(vec![4.0, 1.33]));
        let (cap, join) = line.cap_join();
        assert_eq!((cap, join), (kurbo::Cap::Butt, kurbo::Join::Round));
        assert_eq!(parse_line(&reparse(&serialize_line(&line))), line);
        let dot = kiln_ooxml::drawingml::line::Line {
            width: Some(12700),
            dash: Some(LineDash::Preset(PresetLineDashVal::SysDot)),
            ..Default::default()
        };
        assert_eq!(dot.dash_array_px(), Some(vec![1.33, 1.33]));
        let solid = kiln_ooxml::drawingml::line::Line {
            width: Some(12700),
            dash: Some(LineDash::Preset(PresetLineDashVal::Solid)),
            ..Default::default()
        };
        assert_eq!(solid.dash_array_px(), None);
        let stroke = parse_line(&x(XML)).to_stroke(1.0);
        assert!((stroke.width - 3.0).abs() < 1e-9);
        assert_eq!(stroke.miter_limit, 8.0);
        assert_eq!(stroke.dash_pattern.len(), 4);
    }
}

mod effects {
    use super::*;

    const XML: &str = r#"<a:effectLst><a:glow rad="101600"><a:schemeClr val="accent1"><a:alpha val="40000"/></a:schemeClr></a:glow><a:outerShdw blurRad="50800" dist="38100" dir="2700000" algn="tl" rotWithShape="0"><a:prstClr val="black"><a:alpha val="40000"/></a:prstClr></a:outerShdw><a:reflection blurRad="6350" stA="52000" endA="300" endPos="35000" dir="5400000" sy="-100000" algn="bl" rotWithShape="0"/><a:softEdge rad="31750"/></a:effectLst>"#;

    #[test]
    fn parses_and_round_trips_shadow_glow_reflection_and_soft_edges() {
        let fx = parse_effect_list(&x(XML));
        let s = fx.outer_shadow.as_ref().unwrap();
        assert_eq!((s.blur_rad, s.dist, s.dir), (50800, 38100, 45.0));
        assert_eq!(s.algn, RectAlignment::Tl);
        assert!(!s.rot_with_shape);
        assert_eq!(fx.glow.as_ref().unwrap().rad, 101_600);
        let r = fx.reflection.unwrap();
        assert!((r.st_a - 0.52).abs() < 1e-12);
        assert!((r.end_a - 0.003).abs() < 1e-12);
        assert!((r.end_pos - 0.35).abs() < 1e-12);
        assert!((r.sy + 1.0).abs() < 1e-12);
        assert_eq!(r.algn, RectAlignment::Bl);
        assert_eq!(fx.soft_edge, Some(31750));
        assert_eq!(parse_effect_list(&reparse(&serialize_effect_list(&fx))), fx);
        assert_eq!(
            xml(&serialize_effect_list(&fx)),
            XML.replace(
                "<a:effectLst>",
                &format!(r#"<a:effectLst xmlns:a="{}">"#, ns::A)
            )
        );
    }

    #[test]
    fn preserves_unmodelled_effects() {
        let fx = parse_effect_list(&x(
            r#"<a:effectLst><a:prstShdw prst="shdw1" dist="1"><a:srgbClr val="000000"/></a:prstShdw></a:effectLst>"#,
        ));
        assert_eq!(fx.other[0].name, "a:prstShdw");
        assert!(xml(&serialize_effect_list(&fx)).contains(r#"<a:prstShdw prst="shdw1" dist="1">"#));
    }

    #[test]
    fn renders_the_outer_shadow_as_css() {
        let fx = parse_effect_list(&x(XML));
        let css = fx.outer_shadow.unwrap().to_css(&ColorContext::default());
        assert!((css.x - 2.83).abs() < 0.005);
        assert!((css.y - 2.83).abs() < 0.005);
        assert!((css.blur - 5.33).abs() < 0.005);
        assert_eq!(css.color, "rgba(0, 0, 0, 0.4)");
        assert_eq!(
            css.filter,
            "drop-shadow(2.83px 2.83px 2.67px rgba(0, 0, 0, 0.4))"
        );
    }
}

mod text {
    use super::*;

    #[test]
    fn round_trips_body_properties_with_autofit_and_preserved_warp() {
        let src = x(
            r#"<a:bodyPr rot="0" vert="vert270" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" numCol="2" spcCol="182880" rtlCol="0" anchor="ctr" anchorCtr="0"><a:prstTxWarp prst="textArchUp"><a:avLst/></a:prstTxWarp><a:normAutofit fontScale="92500" lnSpcReduction="10000"/></a:bodyPr>"#,
        );
        let bp = parse_body_properties(&src);
        assert_eq!(bp.vert, Some(TextVerticalType::Vert270));
        assert_eq!(bp.num_col, Some(2));
        assert_eq!(bp.anchor, Some(TextAnchoringType::Ctr));
        assert_eq!(bp.anchor_ctr, Some(false));
        assert_eq!(bp.rtl_col, Some(false));
        assert_eq!(
            bp.autofit,
            Some(Autofit::Normal {
                font_scale: 0.925,
                ln_spc_reduction: 0.1
            })
        );
        let extras: Vec<&str> = bp.extras.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(extras, ["a:prstTxWarp"]);
        assert_eq!(xml(&serialize_body_properties(&bp)), xml(&src));
    }

    #[test]
    fn round_trips_run_properties() {
        let src = x(&format!(
            r#"<a:rPr lang="en-GB" altLang="de-DE" sz="2400" b="1" i="0" u="dbl" strike="sngStrike" kern="1200" cap="small" spc="-10" baseline="30000" dirty="0"><a:ln w="9525"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:ln><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:effectLst><a:outerShdw blurRad="38100"><a:srgbClr val="000000"/></a:outerShdw></a:effectLst><a:highlight><a:srgbClr val="FFFF00"/></a:highlight><a:latin typeface="+mj-lt"/><a:ea typeface="+mj-ea"/><a:cs typeface="Arial" panose="020B0604020202020204" pitchFamily="34" charset="0"/><a:hlinkClick xmlns:r="{}" r:id="rId4"/><a:rtl val="0"/></a:rPr>"#,
            ns::R
        ));
        let rp = parse_run_properties(&src);
        assert_eq!(rp.lang.as_deref(), Some("en-GB"));
        assert_eq!(rp.sz, Some(2400));
        assert_eq!((rp.b, rp.i), (Some(true), Some(false)));
        assert_eq!(rp.u, Some(TextUnderlineType::Dbl));
        assert_eq!(rp.strike, Some(TextStrikeType::SngStrike));
        assert_eq!(rp.cap, Some(TextCapsType::Small));
        assert_eq!(rp.baseline, Some(30000));
        assert_eq!(rp.rtl, Some(false));
        assert_eq!(rp.latin.as_ref().unwrap().typeface, "+mj-lt");
        assert_eq!(rp.hyperlinks.len(), 1);
        assert_eq!(
            parse_run_properties(&reparse(&serialize_run_properties(&rp, "a:rPr"))),
            rp
        );
        // Attribute and child order are Office's, so the XML itself round-trips.
        assert_eq!(xml(&serialize_run_properties(&rp, "a:rPr")), xml(&src));
    }

    #[test]
    fn round_trips_paragraph_properties_with_bullets_spacing_and_tabs() {
        let src = x(
            r#"<a:pPr marL="342900" lvl="1" indent="-342900" algn="just" rtl="0"><a:lnSpc><a:spcPct val="150000"/></a:lnSpc><a:spcBef><a:spcPts val="1200"/></a:spcBef><a:buClr><a:srgbClr val="FF0000"/></a:buClr><a:buSzPct val="80000"/><a:buFont typeface="Wingdings" pitchFamily="2" charset="2"/><a:buChar char="§"/><a:tabLst><a:tab pos="914400" algn="dec"/></a:tabLst><a:defRPr sz="1800"/></a:pPr>"#,
        );
        let pp = parse_paragraph_properties(&src);
        assert_eq!(pp.mar_l, Some(342_900));
        assert_eq!(pp.lvl, Some(1));
        assert_eq!(pp.indent, Some(-342_900));
        assert_eq!(pp.algn, Some(TextAlignType::Just));
        assert_eq!(pp.ln_spc, Some(TextSpacing::Percent(1.5)));
        assert_eq!(pp.spc_bef, Some(TextSpacing::Points(1200)));
        assert_eq!(pp.bullet, Some(Bullet::Char("§".into())));
        assert_eq!(pp.bu_sz, Some(BulletSize::Percent(0.8)));
        assert_eq!(
            pp.tabs,
            Some(vec![TabStop {
                pos: 914_400,
                algn: TextTabAlignType::Dec
            }])
        );
        assert_eq!(pp.def_rpr.as_ref().unwrap().sz, Some(1800));
        assert_eq!(
            parse_paragraph_properties(&reparse(&serialize_paragraph_properties(&pp, "a:pPr"))),
            pp
        );
        assert_eq!(
            xml(&serialize_paragraph_properties(&pp, "a:pPr")),
            xml(&src)
        );
        let auto = parse_paragraph_properties(&x(
            r#"<a:pPr><a:buFontTx/><a:buAutoNum type="romanUcPeriod" startAt="3"/></a:pPr>"#,
        ));
        assert_eq!(
            auto.bullet,
            Some(Bullet::AutoNum {
                scheme: TextAutonumberScheme::RomanUcPeriod,
                start_at: Some(3)
            })
        );
        assert_eq!(auto.bu_font, Some(BulletFont::FollowText));
    }

    #[test]
    fn round_trips_list_styles_and_merges_inherited_properties() {
        let src = x(
            r#"<a:lstStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr><a:lvl1pPr marL="0" algn="l"><a:defRPr sz="2800" b="1"/></a:lvl1pPr><a:lvl3pPr marL="914400"><a:buNone/></a:lvl3pPr></a:lstStyle>"#,
        );
        let ls = parse_list_style(&src);
        assert_eq!(ls.levels[0].as_ref().unwrap().algn, Some(TextAlignType::L));
        assert!(ls.levels[1].is_none());
        assert_eq!(ls.levels[2].as_ref().unwrap().bullet, Some(Bullet::None));
        assert_eq!(xml(&serialize_list_style(&ls, "a:lstStyle")), xml(&src));
        let over = parse_paragraph_properties(&x(r#"<a:pPr algn="ctr"><a:defRPr i="1"/></a:pPr>"#));
        let merged = merge_paragraph_properties(ls.levels[0].as_ref(), Some(&over));
        assert_eq!(merged.algn, Some(TextAlignType::Ctr));
        assert_eq!(merged.mar_l, Some(0));
        let d = merged.def_rpr.unwrap();
        assert_eq!((d.sz, d.b, d.i), (Some(2800), Some(true), Some(true)));
        assert_eq!(merge_run_properties(None, None), Default::default());
        assert_eq!(merge_paragraph_properties(None, None), Default::default());
    }

    #[test]
    fn reads_text_properties_from_a_real_slide() {
        let pkg = OpcPackage::open(&fixture("pptx/features/text-formatting.pptx")).unwrap();
        let slide = pkg
            .part_xml("/ppt/slides/slide1.xml")
            .unwrap()
            .unwrap()
            .root;
        let bodies: Vec<_> = slide
            .descendants("a:bodyPr")
            .into_iter()
            .map(parse_body_properties)
            .collect();
        assert!(
            bodies
                .iter()
                .any(|b| b.num_col == Some(2) && matches!(b.autofit, Some(Autofit::Normal { .. })))
        );
        assert!(
            bodies
                .iter()
                .any(|b| b.vert == Some(TextVerticalType::Vert270))
        );
    }
}
