// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shape Format ▸ Shape Styles, Shape Fill ▸ Gradient, Shape Effects and Text Direction, and
//! Picture Format ▸ Crop to Shape and Corrections. Like the other Shape Format commands they edit
//! the drawing's kept DrawingML, which the layout paints (Kiln Deck's DrawingML painter) and the
//! DOCX writer keeps, so Word shows the same.

use kiln_common::{KilnError, Result};
use kiln_doc::{Drawing, DrawingKind};
use kiln_ooxml::xml::{WriteOptions, XmlElement, parse_xml_str, write_element};
use kiln_protocol::commands::write_office::{
    GradientSpec, PictureCorrection, ShapeEffect, ShapeStyleLevel, ShapeStylePreset, TextDirection,
};

use super::super::WriteDoc;
use super::super::features::shapes::{AFTER_FILL, FILLS, check_hex, find_mut};

/// The theme colour of a style preset.
fn scheme(color: u8) -> &'static str {
    [
        "tx1", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6",
    ]
    .get(usize::from(color))
    .copied()
    .unwrap_or("accent1")
}

fn el(xml: &str) -> Result<XmlElement> {
    parse_xml_str(xml)
        .map(|d| d.root)
        .map_err(|e| KilnError::Internal(format!("DrawingML: {e}")))
}

/// `spPr` children (fill, `a:ln`, `a:effectLst`) and the text colour of a Shape Styles preset.
fn style_parts(p: ShapeStylePreset) -> (String, String, Option<String>, &'static str) {
    let c = scheme(p.color);
    let clr = |mods: &str| format!("<a:schemeClr val=\"{c}\">{mods}</a:schemeClr>");
    let solid = |mods: &str| format!("<a:solidFill>{}</a:solidFill>", clr(mods));
    let line = |w: u32, fill: &str| format!("<a:ln w=\"{w}\">{fill}</a:ln>");
    let grad = |a: &str, b: &str| {
        format!(
            "<a:gradFill rotWithShape=\"1\"><a:gsLst><a:gs pos=\"0\">{}</a:gs><a:gs pos=\"100000\">{}</a:gs></a:gsLst><a:lin ang=\"5400000\" scaled=\"0\"/></a:gradFill>",
            clr(a),
            clr(b)
        )
    };
    let shadow = "<a:effectLst><a:outerShdw blurRad=\"57150\" dist=\"19050\" dir=\"5400000\" algn=\"ctr\" rotWithShape=\"0\"><a:srgbClr val=\"000000\"><a:alpha val=\"63000\"/></a:srgbClr></a:outerShdw></a:effectLst>";
    match p.level {
        ShapeStyleLevel::Outline => (
            "<a:solidFill><a:schemeClr val=\"bg1\"/></a:solidFill>".to_owned(),
            line(12_700, &solid("")),
            None,
            "tx1",
        ),
        ShapeStyleLevel::Fill => (
            solid(""),
            line(12_700, &solid("<a:shade val=\"50000\"/>")),
            None,
            "lt1",
        ),
        ShapeStyleLevel::LightOutline => (
            solid(""),
            line(
                38_100,
                "<a:solidFill><a:schemeClr val=\"lt1\"/></a:solidFill>",
            ),
            Some(shadow.to_owned()),
            "lt1",
        ),
        ShapeStyleLevel::Subtle => (
            solid("<a:lumMod val=\"20000\"/><a:lumOff val=\"80000\"/>"),
            line(12_700, &solid("")),
            None,
            "dk1",
        ),
        ShapeStyleLevel::Moderate => (
            grad("<a:tint val=\"50000\"/><a:satMod val=\"300000\"/>", "<a:shade val=\"80000\"/>"),
            "<a:ln><a:noFill/></a:ln>".to_owned(),
            Some(shadow.to_owned()),
            "lt1",
        ),
        ShapeStyleLevel::Intense => (
            grad("<a:satMod val=\"103000\"/><a:lumMod val=\"102000\"/><a:tint val=\"94000\"/>", "<a:shade val=\"78000\"/>"),
            "<a:ln><a:noFill/></a:ln>".to_owned(),
            Some("<a:effectLst><a:outerShdw blurRad=\"57150\" dist=\"38100\" dir=\"5400000\" algn=\"ctr\" rotWithShape=\"0\"><a:srgbClr val=\"000000\"><a:alpha val=\"63000\"/></a:srgbClr></a:outerShdw></a:effectLst>".to_owned()),
            "lt1",
        ),
    }
}

fn replace_fill(sp_pr: &mut XmlElement, fill: XmlElement) {
    for f in FILLS {
        sp_pr.remove_children(f);
    }
    sp_pr.upsert_child(fill, &AFTER_FILL);
}

fn sp_pr(root: &mut XmlElement) -> Result<&mut XmlElement> {
    find_mut(root, "spPr").ok_or_else(|| KilnError::Unsupported("shapes without spPr".into()))
}

/// A picture's DrawingML: the kept XML, or the XML the writer makes for a new picture (the writer
/// patches the size, position, picture relationship and cropping from the model on save).
fn picture_xml(d: &Drawing) -> String {
    if let Some(x) = &d.xml {
        return x.clone();
    }
    format!(
        "<w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{w}\" cy=\"{h}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:docPr id=\"1\" name=\"{name}\"/><wp:cNvGraphicFramePr/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"{name}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr bwMode=\"auto\"><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>",
        w = d.width,
        h = d.height,
        name = d.name.replace(['"', '<', '>', '&'], "")
    )
}

impl WriteDoc {
    /// Shape Styles gallery.
    pub(super) fn shape_style(&mut self, p: ShapeStylePreset) -> Result<()> {
        if p.color > 6 {
            return Err(KilnError::InvalidInput(format!(
                "theme colour {} is not one of the gallery's 0–6",
                p.color
            )));
        }
        let (fill, line, effects, font) = style_parts(p);
        self.edit_shape_xml(|root| {
            let sp = sp_pr(root)?;
            replace_fill(sp, el(&fill)?);
            sp.remove_children("a:ln");
            sp.upsert_child(el(&line)?, &AFTER_FILL[1..]);
            sp.remove_children("a:effectLst");
            if let Some(e) = &effects {
                sp.upsert_child(el(e)?, &AFTER_FILL[2..]);
            }
            // The text colour follows the style (Word's fontRef).
            if let Some(style) = find_mut(root, "style")
                && let Some(f) = style.elements_mut().find(|c| c.name.ends_with("fontRef"))
            {
                f.children.clear();
                f.children
                    .push(kiln_ooxml::xml::XmlNode::Element(el(&format!(
                        "<a:schemeClr val=\"{font}\"/>"
                    ))?));
            }
            Ok(())
        })
    }

    /// Shape Fill ▸ Gradient (two colours, linear).
    pub(super) fn shape_gradient(&mut self, g: Option<&GradientSpec>) -> Result<()> {
        let Some(g) = g else {
            // Back to solid: the gradient's first colour.
            let first = self.selected_object().and_then(|o| o.2.xml).and_then(|x| {
                let i = x.find("<a:gs pos=\"0\">")?;
                let v = x[i..].find("val=\"")? + i + 5;
                Some(x[v..v + 6].to_owned())
            });
            return self.shape_fill(first.as_deref().or(Some("4472C4")));
        };
        let (from, to) = (
            check_hex(Some(&g.from))?.unwrap_or_default(),
            check_hex(Some(&g.to))?.unwrap_or_default(),
        );
        if !g.angle.is_finite() {
            return Err(KilnError::InvalidInput(
                "a gradient angle is a number".into(),
            ));
        }
        #[allow(clippy::cast_possible_truncation)]
        let ang = (f64::from(g.angle).rem_euclid(360.0) * 60_000.0).round() as i64;
        let xml = format!(
            "<a:gradFill rotWithShape=\"1\"><a:gsLst><a:gs pos=\"0\"><a:srgbClr val=\"{from}\"/></a:gs><a:gs pos=\"100000\"><a:srgbClr val=\"{to}\"/></a:gs></a:gsLst><a:lin ang=\"{ang}\" scaled=\"0\"/></a:gradFill>"
        );
        self.edit_shape_xml(|root| {
            replace_fill(sp_pr(root)?, el(&xml)?);
            Ok(())
        })
    }

    /// Shape Effects (shadow, glow, soft edges, none).
    pub(super) fn shape_effect(&mut self, effect: ShapeEffect) -> Result<()> {
        let xml = match effect {
            ShapeEffect::None => None,
            ShapeEffect::Shadow => Some(
                "<a:effectLst><a:outerShdw blurRad=\"50800\" dist=\"38100\" dir=\"2700000\" algn=\"tl\" rotWithShape=\"0\"><a:prstClr val=\"black\"><a:alpha val=\"40000\"/></a:prstClr></a:outerShdw></a:effectLst>",
            ),
            ShapeEffect::Glow => Some(
                "<a:effectLst><a:glow rad=\"101600\"><a:schemeClr val=\"accent1\"><a:satMod val=\"175000\"/><a:alpha val=\"40000\"/></a:schemeClr></a:glow></a:effectLst>",
            ),
            ShapeEffect::SoftEdges => {
                Some("<a:effectLst><a:softEdge rad=\"63500\"/></a:effectLst>")
            }
        };
        self.edit_shape_xml(|root| {
            let sp = sp_pr(root)?;
            sp.remove_children("a:effectLst");
            if let Some(x) = xml {
                sp.upsert_child(el(x)?, &AFTER_FILL[2..]);
            }
            Ok(())
        })
    }

    /// Text Direction: horizontal, rotated 90° or 270°.
    pub(super) fn shape_text_direction(&mut self, dir: TextDirection) -> Result<()> {
        let v = match dir {
            TextDirection::Horizontal => "horz",
            TextDirection::Rotate90 => "vert",
            TextDirection::Rotate270 => "vert270",
        };
        self.edit_shape_xml(|root| {
            let body = find_mut(root, "bodyPr")
                .ok_or_else(|| KilnError::Unsupported("shapes without text".into()))?;
            body.set_attr("vert", Some(v));
            Ok(())
        })
    }

    /// Edits the selected picture's DrawingML.
    fn edit_picture_xml(&mut self, f: impl FnOnce(&mut XmlElement) -> Result<()>) -> Result<()> {
        self.update_object(None, |_, d, _| {
            if !matches!(d.kind, DrawingKind::Picture { .. }) {
                return Err(KilnError::InvalidInput("select a picture first".into()));
            }
            let mut parsed = parse_xml_str(&picture_xml(d)).map_err(|e| {
                KilnError::Internal(format!("the picture's DrawingML could not be read: {e}"))
            })?;
            f(&mut parsed.root)?;
            d.xml = Some(
                write_element(&parsed.root, &WriteOptions::fragment()).map_err(|e| {
                    KilnError::Internal(format!(
                        "the picture's DrawingML could not be written: {e}"
                    ))
                })?,
            );
            Ok(())
        })
    }

    /// Crop ▸ Crop to Shape.
    pub(super) fn crop_to_shape(&mut self, preset: &str) -> Result<()> {
        let preset = preset.trim();
        if kiln_ooxml::drawingml::presets::preset_geometry(preset, 100.0, 100.0, &[]).is_err() {
            return Err(KilnError::InvalidInput(format!("{preset} is not a shape")));
        }
        self.edit_picture_xml(|root| {
            let sp = find_mut(root, "spPr")
                .ok_or_else(|| KilnError::Unsupported("pictures without spPr".into()))?;
            sp.remove_children("a:custGeom");
            match sp.child_mut("a:prstGeom") {
                Some(g) => {
                    g.set_attr("prst", Some(preset));
                    g.children.clear();
                    g.children
                        .push(kiln_ooxml::xml::XmlNode::Element(XmlElement::new(
                            "a:avLst",
                        )));
                }
                None => sp.upsert_child(
                    el(&format!(
                        "<a:prstGeom prst=\"{preset}\"><a:avLst/></a:prstGeom>"
                    ))?,
                    &["a:noFill", "a:solidFill", "a:ln", "a:effectLst", "a:extLst"],
                ),
            }
            Ok(())
        })
    }

    /// Corrections: brightness and contrast.
    pub(super) fn picture_corrections(&mut self, c: PictureCorrection) -> Result<()> {
        if !(-100..=100).contains(&c.brightness) || !(-100..=100).contains(&c.contrast) {
            return Err(KilnError::InvalidInput(
                "brightness and contrast go from −100 % to 100 %".into(),
            ));
        }
        self.edit_picture_xml(|root| {
            let blip = find_mut(root, "blip")
                .ok_or_else(|| KilnError::Unsupported("pictures without a blip".into()))?;
            blip.remove_children("a:lum");
            if c.brightness != 0 || c.contrast != 0 {
                let lum = XmlElement::new("a:lum")
                    .with_attr("bright", i32::from(c.brightness) * 1000)
                    .with_attr("contrast", i32::from(c.contrast) * 1000);
                blip.children
                    .insert(0, kiln_ooxml::xml::XmlNode::Element(lum));
            }
            Ok(())
        })
    }
}
