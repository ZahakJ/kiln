// SPDX-License-Identifier: AGPL-3.0-or-later
//! The XML layer: an order-preserving element tree ([`XmlElement`]) over `quick-xml`, with
//! namespace prefixes normalised to Office's table on the way in, query and edit helpers, and a
//! deterministic writer.
//!
//! ```
//! use kiln_ooxml::xml::{parse_xml_str, write_element, WriteOptions, XmlElement};
//!
//! let doc = parse_xml_str(r#"<ns0:p xmlns:ns0="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><ns0:r/></ns0:p>"#).unwrap();
//! assert_eq!(doc.root.name, "w:p");
//! assert!(doc.root.child("w:r").is_some());
//!
//! let fill = XmlElement::new("a:solidFill")
//!     .with_child(XmlElement::new("a:srgbClr").with_attr("val", "4472C4"));
//! assert_eq!(
//!     write_element(&fill, &WriteOptions::fragment()).unwrap(),
//!     r#"<a:solidFill xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:srgbClr val="4472C4"/></a:solidFill>"#
//! );
//! ```

pub mod namespaces;
mod node;
mod parse;
mod write;

pub use node::{
    AttrValue, Attrs, XmlDeclaration, XmlDocument, XmlElement, XmlNode, XmlPi, XmlText,
    format_number, parse_int,
};
pub use parse::{
    ParseOptions, XmlParseError, decode_xml_bytes, parse_str_with, parse_xml, parse_xml_str,
    parse_xml_with,
};
pub use write::{
    Declaration, WriteOptions, XmlWriteError, document_to_bytes, element_to_bytes, escape_attr,
    escape_text, is_xml_char, strip_invalid_xml_chars, undeclared_prefixes, write_document,
    write_element,
};
