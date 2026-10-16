// SPDX-License-Identifier: AGPL-3.0-or-later
//! Serde for kiln-ooxml's DrawingML values through their XML form.
//!
//! kiln-ooxml models fills, lines, effects, colours, geometry and themes losslessly (unknown
//! children are kept) and guarantees `parse(serialize(x)) == x`, but its types carry no serde
//! derives. The model stores them as they are and serialises each one as the XML string of its
//! element, so a `.kdeck` file or an autosave snapshot keeps exactly what a `.pptx` held.

use kiln_ooxml::drawingml::color::{DrawingColor, parse_color, serialize_color};
use kiln_ooxml::drawingml::effects::{EffectList, parse_effect_list, serialize_effect_list};
use kiln_ooxml::drawingml::fill::{Fill, parse_fill, serialize_fill};
use kiln_ooxml::drawingml::geometry::{GeometryDef, parse_geometry, serialize_custom_geometry};
use kiln_ooxml::drawingml::line::{Line, parse_line, serialize_line};
use kiln_ooxml::drawingml::theme::{Theme, parse_theme, serialize_theme};
use kiln_ooxml::xml::{WriteOptions, XmlElement, parse_xml_str, write_element};
use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A DrawingML value with an element form.
pub trait DmlXml: Sized {
    /// What the value is called in error messages.
    const WHAT: &'static str;
    /// The element.
    fn to_element(&self) -> XmlElement;
    /// Reads the element back; `None` when it is not this kind of value.
    fn from_element(e: &XmlElement) -> Option<Self>;
}

impl DmlXml for Fill {
    const WHAT: &'static str = "fill";
    fn to_element(&self) -> XmlElement {
        serialize_fill(self)
    }
    fn from_element(e: &XmlElement) -> Option<Self> {
        parse_fill(e)
    }
}

impl DmlXml for Line {
    const WHAT: &'static str = "line";
    fn to_element(&self) -> XmlElement {
        serialize_line(self)
    }
    fn from_element(e: &XmlElement) -> Option<Self> {
        (e.name == "a:ln").then(|| parse_line(e))
    }
}

impl DmlXml for EffectList {
    const WHAT: &'static str = "effect list";
    fn to_element(&self) -> XmlElement {
        serialize_effect_list(self)
    }
    fn from_element(e: &XmlElement) -> Option<Self> {
        (e.name == "a:effectLst").then(|| parse_effect_list(e))
    }
}

impl DmlXml for DrawingColor {
    const WHAT: &'static str = "colour";
    fn to_element(&self) -> XmlElement {
        serialize_color(self)
    }
    fn from_element(e: &XmlElement) -> Option<Self> {
        parse_color(e)
    }
}

impl DmlXml for GeometryDef {
    const WHAT: &'static str = "custom geometry";
    fn to_element(&self) -> XmlElement {
        serialize_custom_geometry(self)
    }
    fn from_element(e: &XmlElement) -> Option<Self> {
        (e.name == "a:custGeom").then(|| parse_geometry(e))
    }
}

impl<T: DmlXml> DmlXml for Box<T> {
    const WHAT: &'static str = T::WHAT;
    fn to_element(&self) -> XmlElement {
        (**self).to_element()
    }
    fn from_element(e: &XmlElement) -> Option<Self> {
        T::from_element(e).map(Box::new)
    }
}

impl DmlXml for Theme {
    const WHAT: &'static str = "theme";
    fn to_element(&self) -> XmlElement {
        serialize_theme(self).root
    }
    fn from_element(e: &XmlElement) -> Option<Self> {
        let mut t = parse_theme(e).ok()?;
        normalize_theme(&mut t);
        Some(t)
    }
}

/// Puts a theme in the form kiln-ooxml's writer and reader agree on, so a theme survives any
/// number of save/load cycles unchanged: the empty `a:objectDefaults`/`a:extraClrSchemeLst` the
/// writer adds are not kept as extras, and an effect style without effects has an empty list.
pub fn normalize_theme(t: &mut Theme) {
    let empty =
        |e: &XmlElement, name: &str| e.name == name && e.attrs.is_empty() && e.children.is_empty();
    if t.extras.len() == 2
        && empty(&t.extras[0], "a:objectDefaults")
        && empty(&t.extras[1], "a:extraClrSchemeLst")
    {
        t.extras.clear();
    }
    for s in &mut t.format_scheme.effect_styles {
        if s.effects.is_none() {
            s.effects = Some(EffectList::default());
        }
    }
}

/// The XML text of a value.
///
/// # Errors
/// The element uses a namespace prefix the writer cannot declare (only possible for unknown
/// children kept from a file).
pub fn to_xml_string<T: DmlXml>(v: &T) -> Result<String, String> {
    write_element(&v.to_element(), &WriteOptions::fragment()).map_err(|e| e.to_string())
}

/// Reads a value from its XML text.
///
/// # Errors
/// Malformed XML or an element of another kind.
pub fn from_xml_str<T: DmlXml>(s: &str) -> Result<T, String> {
    let doc = parse_xml_str(s).map_err(|e| format!("{}: {e}", T::WHAT))?;
    T::from_element(&doc.root).ok_or_else(|| format!("not a {}: {}", T::WHAT, doc.root.name))
}

/// `#[serde(with = "xml")]` for a DrawingML value.
pub mod xml {
    use super::*;

    /// Serialises the value as its XML text.
    ///
    /// # Errors
    /// See [`to_xml_string`].
    pub fn serialize<T: DmlXml, S: Serializer>(v: &T, s: S) -> Result<S::Ok, S::Error> {
        to_xml_string(v).map_err(S::Error::custom)?.serialize(s)
    }

    /// Reads the value from its XML text.
    ///
    /// # Errors
    /// See [`from_xml_str`].
    pub fn deserialize<'de, T: DmlXml, D: Deserializer<'de>>(d: D) -> Result<T, D::Error> {
        let s = String::deserialize(d)?;
        from_xml_str(&s).map_err(D::Error::custom)
    }
}

/// `#[serde(with = "xml_opt")]` for an optional DrawingML value.
pub mod xml_opt {
    use super::*;

    /// Serialises the value as optional XML text.
    ///
    /// # Errors
    /// See [`to_xml_string`].
    pub fn serialize<T: DmlXml, S: Serializer>(v: &Option<T>, s: S) -> Result<S::Ok, S::Error> {
        v.as_ref()
            .map(to_xml_string)
            .transpose()
            .map_err(S::Error::custom)?
            .serialize(s)
    }

    /// Reads the value from optional XML text.
    ///
    /// # Errors
    /// See [`from_xml_str`].
    pub fn deserialize<'de, T: DmlXml, D: Deserializer<'de>>(d: D) -> Result<Option<T>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|s| from_xml_str(&s))
            .transpose()
            .map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::drawingml::fill::{GradientFill, GradientStop, LinearShade};
    use kiln_ooxml::drawingml::theme::office_theme;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Holder {
        #[serde(with = "xml")]
        fill: Fill,
        #[serde(with = "xml_opt")]
        line: Option<Line>,
        #[serde(with = "xml_opt")]
        none: Option<EffectList>,
        #[serde(with = "xml")]
        theme: Theme,
    }

    #[test]
    fn drawingml_values_survive_postcard_and_json() {
        let h = Holder {
            fill: Fill::Gradient(GradientFill {
                stops: vec![
                    GradientStop {
                        pos: 0.0,
                        color: DrawingColor::scheme("accent1").with("lumMod", 75_000),
                    },
                    GradientStop {
                        pos: 1.0,
                        color: DrawingColor::srgb("FFB547"),
                    },
                ],
                linear: Some(LinearShade {
                    angle: 45.5,
                    scaled: false,
                }),
                ..GradientFill::default()
            }),
            line: Some(Line {
                width: Some(12_700),
                fill: Some(Fill::Solid(DrawingColor::scheme("tx1"))),
                ..Line::default()
            }),
            none: None,
            theme: office_theme(),
        };
        let bytes = postcard::to_allocvec(&h).unwrap();
        let back: Holder = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back, h);
        let json = serde_json::to_string(&h).unwrap();
        let back: Holder = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn wrong_elements_are_refused() {
        assert!(from_xml_str::<Line>("<a:solidFill/>").is_err());
        assert!(from_xml_str::<Fill>("<a:ln/>").is_err());
        assert!(from_xml_str::<Theme>("<a:ln/>").is_err());
        assert!(from_xml_str::<GeometryDef>("<a:prstGeom/>").is_err());
        assert!(from_xml_str::<DrawingColor>("not xml").is_err());
        assert!(from_xml_str::<EffectList>("<a:effectLst/>").is_ok());
    }
}
