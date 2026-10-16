// SPDX-License-Identifier: AGPL-3.0-or-later
//! Document properties: core (`docProps/core.xml`, Dublin Core), extended (`docProps/app.xml`)
//! and custom (`docProps/custom.xml`), per ECMA-376 Part 2 §11 and Part 1 §22.2–22.3.

use crate::opc::{OpcError, OpcPackage, RelationshipOptions, content_type, rel_type};
use crate::xml::{XmlElement, element_to_bytes, format_number, namespaces::ns, parse_int};

macro_rules! core_fields {
    ($($field:ident => $el:literal),+ $(,)?) => {
        /// Core properties. Timestamps are W3CDTF strings (`2026-10-15T09:30:00Z`).
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct CoreProperties {
            $(
                #[doc = concat!("`", $el, "`")]
                pub $field: Option<String>,
            )+
        }

        impl CoreProperties {
            fn fields(&self) -> Vec<(&'static str, Option<&str>)> {
                vec![$(($el, self.$field.as_deref())),+]
            }

            fn set_field(&mut self, el: &str, value: String) {
                match el {
                    $($el => self.$field = Some(value),)+
                    _ => {}
                }
            }

            const ELEMENTS: &'static [&'static str] = &[$($el),+];
        }
    };
}

// Schema order.
core_fields! {
    title => "dc:title",
    subject => "dc:subject",
    creator => "dc:creator",
    keywords => "cp:keywords",
    description => "dc:description",
    last_modified_by => "cp:lastModifiedBy",
    revision => "cp:revision",
    last_printed => "cp:lastPrinted",
    created => "dcterms:created",
    modified => "dcterms:modified",
    category => "cp:category",
    content_status => "cp:contentStatus",
    language => "dc:language",
    identifier => "dc:identifier",
    version => "cp:version",
}

/// A typed `app.xml` value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AppValue<'a> {
    Str(Option<&'a str>),
    Int(Option<i64>),
    Bool(Option<bool>),
}

macro_rules! app_fields {
    ($($field:ident: $ty:ident => $el:literal),+ $(,)?) => {
        /// Extended (application) properties.
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct AppProperties {
            $(
                #[doc = concat!("`", $el, "`")]
                pub $field: Option<app_fields!(@ty $ty)>,
            )+
            /// `HeadingPairs`, `TitlesOfParts`, `HLinks`, `DigSig` and anything unknown,
            /// preserved verbatim.
            pub extras: Vec<XmlElement>,
        }

        impl AppProperties {
            fn values(&self) -> Vec<(&'static str, AppValue<'_>)> {
                vec![$(($el, app_fields!(@get self.$field, $ty))),+]
            }

            /// Sets a field from element text; `false` when the text does not fit its type.
            fn set_field(&mut self, el: &str, text: &str) -> bool {
                match el {
                    $($el => app_fields!(@set self.$field, $ty, text),)+
                    _ => false,
                }
            }

            const ELEMENTS: &'static [&'static str] = &[$($el),+];
        }
    };
    (@ty Str) => { String };
    (@ty Int) => { i64 };
    (@ty Bool) => { bool };
    (@get $e:expr, Str) => { AppValue::Str($e.as_deref()) };
    (@get $e:expr, Int) => { AppValue::Int($e) };
    (@get $e:expr, Bool) => { AppValue::Bool($e) };
    (@set $e:expr, Str, $t:expr) => {{ $e = Some($t.to_owned()); true }};
    (@set $e:expr, Int, $t:expr) => {{
        match parse_int($t) { Some(n) => { $e = Some(n); true } None => false }
    }};
    (@set $e:expr, Bool, $t:expr) => {{
        match $t.trim() {
            "true" | "1" => { $e = Some(true); true }
            "false" | "0" => { $e = Some(false); true }
            _ => false,
        }
    }};
}

// Schema order.
app_fields! {
    template: Str => "Template",
    total_time: Int => "TotalTime",
    pages: Int => "Pages",
    words: Int => "Words",
    characters: Int => "Characters",
    application: Str => "Application",
    doc_security: Int => "DocSecurity",
    presentation_format: Str => "PresentationFormat",
    lines: Int => "Lines",
    paragraphs: Int => "Paragraphs",
    slides: Int => "Slides",
    notes: Int => "Notes",
    hidden_slides: Int => "HiddenSlides",
    mm_clips: Int => "MMClips",
    scale_crop: Bool => "ScaleCrop",
    manager: Str => "Manager",
    company: Str => "Company",
    links_up_to_date: Bool => "LinksUpToDate",
    characters_with_spaces: Int => "CharactersWithSpaces",
    shared_doc: Bool => "SharedDoc",
    hyperlink_base: Str => "HyperlinkBase",
    hyperlinks_changed: Bool => "HyperlinksChanged",
    app_version: Str => "AppVersion",
}

/// Where Office places the structured elements Kiln preserves verbatim (before the named
/// element).
fn app_extra_before(name: &str) -> Option<&'static str> {
    match name {
        "HeadingPairs" | "TitlesOfParts" => Some("Manager"),
        "HLinks" => Some("HyperlinksChanged"),
        "DigSig" => Some("AppVersion"),
        _ => None,
    }
}

/// A custom property's value.
#[derive(Debug, Clone, PartialEq)]
pub enum CustomPropertyValue {
    /// `vt:lpwstr` (also read from `vt:bstr`).
    Text(String),
    /// `vt:i4` (also read from `vt:int`).
    Int(i64),
    /// `vt:r8` (also read from `vt:r4`).
    Number(f64),
    /// `vt:bool`.
    Bool(bool),
    /// `vt:filetime`, a W3CDTF timestamp.
    FileTime(String),
    /// Any other variant type, preserved verbatim.
    Other(XmlElement),
}

/// One custom property.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomProperty {
    /// The property name.
    pub name: String,
    /// Its value.
    pub value: CustomPropertyValue,
    /// `linkTarget`: a bookmark the property is linked to.
    pub link_target: Option<String>,
}

/// All three property sets.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentProperties {
    /// `docProps/core.xml`.
    pub core: CoreProperties,
    /// `docProps/app.xml`.
    pub app: AppProperties,
    /// `docProps/custom.xml`.
    pub custom: Vec<CustomProperty>,
}

/// The format id of user-defined custom properties.
pub const CUSTOM_FMTID: &str = "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}";

// ------------------------------------------------------------------ read

/// Reads `cp:coreProperties`.
#[must_use]
pub fn parse_core_properties(root: &XmlElement) -> CoreProperties {
    let mut out = CoreProperties::default();
    for el in CoreProperties::ELEMENTS {
        if let Some(c) = root.child(el) {
            out.set_field(el, c.text());
        }
    }
    out
}

/// Reads the extended properties root (`Properties`).
#[must_use]
pub fn parse_app_properties(root: &XmlElement) -> AppProperties {
    let mut out = AppProperties::default();
    for c in root.elements() {
        // Strings keep their text exactly; numbers and booleans are trimmed by `set_field`.
        let known = AppProperties::ELEMENTS.contains(&c.name.as_str());
        if !(known && out.set_field(&c.name, &c.text())) {
            out.extras.push(c.clone());
        }
    }
    out
}

/// Reads the custom properties root (`Properties`).
#[must_use]
pub fn parse_custom_properties(root: &XmlElement) -> Vec<CustomProperty> {
    root.children_named("property")
        .map(|p| {
            let v = p.first_element();
            let text = v.map(XmlElement::text).unwrap_or_default();
            let other = || {
                CustomPropertyValue::Other(
                    v.cloned().unwrap_or_else(|| XmlElement::new("vt:empty")),
                )
            };
            let value = match v.map(|e| e.name.as_str()) {
                Some("vt:lpwstr" | "vt:bstr") => CustomPropertyValue::Text(text),
                Some("vt:i4" | "vt:int") => {
                    parse_int(&text).map_or_else(other, CustomPropertyValue::Int)
                }
                Some("vt:r8" | "vt:r4") => text
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .map_or_else(other, CustomPropertyValue::Number),
                Some("vt:bool") => match text.trim() {
                    "true" | "1" => CustomPropertyValue::Bool(true),
                    "false" | "0" => CustomPropertyValue::Bool(false),
                    _ => other(),
                },
                Some("vt:filetime") => CustomPropertyValue::FileTime(text),
                _ => other(),
            };
            CustomProperty {
                name: p.attr("name").unwrap_or("").to_owned(),
                value,
                link_target: p
                    .attr("linkTarget")
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned),
            }
        })
        .collect()
}

/// Reads core, extended and custom properties (missing parts give empty values).
///
/// # Errors
/// [`OpcError`] when a properties part exists but is unreadable.
pub fn read_document_properties(pkg: &OpcPackage) -> Result<DocumentProperties, OpcError> {
    let read = |t: &str| -> Result<Option<XmlElement>, OpcError> {
        let Some(name) = pkg.related_part("/", t)? else {
            return Ok(None);
        };
        Ok(pkg.part_xml(&name)?.map(|d| d.root))
    };
    Ok(DocumentProperties {
        core: read(rel_type::CORE_PROPERTIES)?
            .map(|r| parse_core_properties(&r))
            .unwrap_or_default(),
        app: read(rel_type::EXTENDED_PROPERTIES)?
            .map(|r| parse_app_properties(&r))
            .unwrap_or_default(),
        custom: read(rel_type::CUSTOM_PROPERTIES)?
            .map(|r| parse_custom_properties(&r))
            .unwrap_or_default(),
    })
}

// ----------------------------------------------------------------- write

/// `cp:coreProperties` for these properties.
#[must_use]
pub fn serialize_core_properties(core: &CoreProperties) -> XmlElement {
    let mut root = XmlElement::new("cp:coreProperties")
        .with_attr("xmlns:cp", ns::CP)
        .with_attr("xmlns:dc", ns::DC)
        .with_attr("xmlns:dcterms", ns::DCTERMS)
        .with_attr("xmlns:dcmitype", ns::DCMITYPE)
        .with_attr("xmlns:xsi", ns::XSI);
    for (el, value) in core.fields() {
        let Some(v) = value else { continue };
        let mut e = XmlElement::new(el);
        if el.starts_with("dcterms:") {
            e = e.with_attr("xsi:type", "dcterms:W3CDTF");
        }
        root = root.with_child(e.with_text(v));
    }
    root
}

/// The extended properties root for these properties.
#[must_use]
pub fn serialize_app_properties(app: &AppProperties) -> XmlElement {
    let mut kids: Vec<XmlElement> = Vec::new();
    for (el, value) in app.values() {
        kids.extend(
            app.extras
                .iter()
                .filter(|x| app_extra_before(&x.name) == Some(el))
                .cloned(),
        );
        let text = match value {
            AppValue::Str(Some(s)) => s.to_owned(),
            AppValue::Int(Some(n)) => n.to_string(),
            AppValue::Bool(Some(b)) => if b { "true" } else { "false" }.to_owned(),
            _ => continue,
        };
        kids.push(XmlElement::new(el).with_text(text));
    }
    kids.extend(
        app.extras
            .iter()
            .filter(|x| app_extra_before(&x.name).is_none())
            .cloned(),
    );
    XmlElement::new("Properties")
        .with_attr("xmlns", ns::EP)
        .with_attr("xmlns:vt", ns::VT)
        .with_children(kids)
}

/// The custom properties root for these properties (`pid` numbered from 2, as Office does).
#[must_use]
pub fn serialize_custom_properties(props: &[CustomProperty]) -> XmlElement {
    XmlElement::new("Properties")
        .with_attr("xmlns", ns::OP)
        .with_attr("xmlns:vt", ns::VT)
        .with_children(props.iter().enumerate().map(|(i, p)| {
            let value = match &p.value {
                CustomPropertyValue::Text(s) => XmlElement::new("vt:lpwstr").with_text(s.as_str()),
                CustomPropertyValue::Int(n) => XmlElement::new("vt:i4").with_text(n.to_string()),
                CustomPropertyValue::Number(n) => {
                    XmlElement::new("vt:r8").with_text(format_number(*n))
                }
                CustomPropertyValue::Bool(b) => {
                    XmlElement::new("vt:bool").with_text(if *b { "true" } else { "false" })
                }
                CustomPropertyValue::FileTime(s) => {
                    XmlElement::new("vt:filetime").with_text(s.as_str())
                }
                CustomPropertyValue::Other(e) => e.clone(),
            };
            XmlElement::new("property")
                .with_attr("fmtid", CUSTOM_FMTID)
                .with_attr("pid", i + 2)
                .with_attr("name", p.name.as_str())
                .with_opt_attr("linkTarget", p.link_target.as_deref())
                .with_child(value)
        }))
}

/// Which property sets to write; `None` leaves a set as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertiesUpdate {
    /// New core properties.
    pub core: Option<CoreProperties>,
    /// New extended properties.
    pub app: Option<AppProperties>,
    /// New custom properties; an empty list removes `custom.xml`.
    pub custom: Option<Vec<CustomProperty>>,
}

fn upsert_props_part(
    pkg: &mut OpcPackage,
    rel: &str,
    default_name: &str,
    ct: &str,
    root: Option<&XmlElement>,
) -> Result<(), OpcError> {
    let existing = pkg.related_part("/", rel)?;
    let Some(root) = root else {
        if let Some(name) = existing {
            pkg.remove_part(&name, true)?;
        }
        return Ok(());
    };
    let bytes = element_to_bytes(root)?;
    match existing {
        Some(name) if pkg.has(&name) => pkg.put_part(&name, ct, bytes),
        _ => {
            pkg.put_part(default_name, ct, bytes)?;
            if pkg.related_part("/", rel)?.is_none() {
                pkg.add_relationship("/", rel, default_name, RelationshipOptions::default())?;
            }
            Ok(())
        }
    }
}

/// Writes properties into a package, creating `docProps/*.xml` and their package relationships
/// as needed.
///
/// # Errors
/// [`OpcError`] when the package relationships are unreadable.
pub fn write_document_properties(
    pkg: &mut OpcPackage,
    update: &PropertiesUpdate,
) -> Result<(), OpcError> {
    if let Some(core) = &update.core {
        upsert_props_part(
            pkg,
            rel_type::CORE_PROPERTIES,
            "/docProps/core.xml",
            content_type::CORE_PROPERTIES,
            Some(&serialize_core_properties(core)),
        )?;
    }
    if let Some(app) = &update.app {
        upsert_props_part(
            pkg,
            rel_type::EXTENDED_PROPERTIES,
            "/docProps/app.xml",
            content_type::EXTENDED_PROPERTIES,
            Some(&serialize_app_properties(app)),
        )?;
    }
    if let Some(custom) = &update.custom {
        let root = (!custom.is_empty()).then(|| serialize_custom_properties(custom));
        upsert_props_part(
            pkg,
            rel_type::CUSTOM_PROPERTIES,
            "/docProps/custom.xml",
            content_type::CUSTOM_PROPERTIES,
            root.as_ref(),
        )?;
    }
    Ok(())
}
