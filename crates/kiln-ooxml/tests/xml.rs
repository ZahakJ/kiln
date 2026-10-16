// SPDX-License-Identifier: AGPL-3.0-or-later
//! The XML layer. Ported from `reference/ts-core/ooxml/test/xml.test.ts`.

use kiln_ooxml::xml::namespaces::{canonical_prefix, ns, split_qname};
use kiln_ooxml::xml::{
    Declaration, ParseOptions, WriteOptions, XmlDeclaration, XmlElement, XmlNode, XmlPi,
    decode_xml_bytes, parse_str_with, parse_xml, parse_xml_str, strip_invalid_xml_chars,
    undeclared_prefixes, write_document, write_element,
};

fn w() -> String {
    format!(r#"xmlns:w="{}""#, ns::W)
}

fn fragment(e: &XmlElement) -> String {
    write_element(e, &WriteOptions::fragment()).unwrap()
}

#[test]
fn round_trips_a_tree_keeping_attribute_order_and_the_declaration() {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document {} xmlns:mc=\"{}\" xmlns:w14=\"{}\" mc:Ignorable=\"w14\"><w:body><w:p w14:paraId=\"1A2B3C4D\" w:rsidR=\"00AB\"><w:r><w:t xml:space=\"preserve\"> a &amp; b &lt;c&gt; \"q\" </w:t></w:r></w:p></w:body></w:document>",
        w(),
        ns::MC,
        ns::W14
    );
    let doc = parse_xml_str(&xml).unwrap();
    assert_eq!(doc.declaration, Some(XmlDeclaration::office()));
    assert_eq!(write_document(&doc, &WriteOptions::default()).unwrap(), xml);
}

#[test]
fn decodes_character_references_and_escapes_on_output() {
    let doc = parse_xml_str(r#"<a t="x&#10;y&quot;">&#x41;&#66;&amp;</a>"#).unwrap();
    assert_eq!(doc.root.attr("t"), Some("x\ny\""));
    assert_eq!(doc.root.text(), "AB&");
    assert_eq!(fragment(&doc.root), r#"<a t="x&#10;y&quot;">AB&amp;</a>"#);
}

#[test]
fn normalises_literal_newlines_in_attributes_and_text() {
    let doc = parse_xml_str("<a t=\"x\ny\">l1\r\nl2</a>").unwrap();
    assert_eq!(doc.root.attr("t"), Some("x y"));
    assert_eq!(doc.root.text(), "l1\nl2");
}

#[test]
fn keeps_significant_whitespace_and_drops_indentation_between_elements() {
    let doc = parse_xml_str(&format!(
        "<w:p {}>\n  <w:r>\n    <w:t xml:space=\"preserve\">  two  </w:t>\n  </w:r>\n  <w:r><w:t>  </w:t></w:r>\n</w:p>",
        w()
    ))
    .unwrap();
    assert_eq!(doc.root.children.len(), 2);
    assert_eq!(doc.root.text(), "  two    ");
}

#[test]
fn keeps_whitespace_inside_preserve_scopes() {
    let doc = parse_xml_str(r#"<x xml:space="preserve"> <y/> </x>"#).unwrap();
    assert_eq!(doc.root.children.len(), 3);
}

#[test]
fn round_trips_comments_cdata_and_processing_instructions() {
    let xml =
        r#"<?mso-application progid="Word.Document"?><a><!-- note --><b><![CDATA[x < y]]></b></a>"#;
    let doc = parse_xml_str(xml).unwrap();
    assert_eq!(
        doc.prolog[0],
        XmlNode::Pi(XmlPi {
            target: "mso-application".into(),
            content: r#"progid="Word.Document""#.into()
        })
    );
    let opts = WriteOptions {
        declaration: Declaration::Omit,
        declare_namespaces: true,
    };
    assert_eq!(write_document(&doc, &opts).unwrap(), xml);
}

#[test]
fn rejects_dtds_and_malformed_input() {
    assert!(parse_xml_str(r#"<!DOCTYPE x [<!ENTITY a "b">]><x>&a;</x>"#).is_err());
    assert!(parse_xml_str("").is_err());
    assert!(parse_xml_str("<a/><b/>").is_err());
    assert!(parse_xml_str("<a><b></a>").is_err());
    assert!(
        parse_xml_str("<a>&nbsp;</a>").is_err(),
        "no entities beyond the predefined five"
    );
    assert!(
        parse_xml_str("<a x='1' x='2'/>").is_err(),
        "duplicate attributes"
    );
    assert!(parse_xml_str("<a>").is_err());
    assert!(parse_xml_str("text<a/>").is_err());
}

#[test]
fn refuses_billion_laughs_without_expanding_anything() {
    let lol = r#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">]><lolz>&lol2;</lolz>"#;
    let err = parse_xml_str(lol).unwrap_err();
    assert!(err.to_string().contains("DTD"), "{err}");
    let xxe =
        r#"<?xml version="1.0"?><!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><x>&e;</x>"#;
    assert!(parse_xml_str(xxe).is_err());
}

#[test]
fn caps_nesting_depth() {
    let deep = format!("{}{}", "<a>".repeat(3000), "</a>".repeat(3000));
    let err = parse_xml_str(&deep).unwrap_err();
    assert!(err.to_string().contains("nested"), "{err}");
    let ok = format!("{}{}", "<a>".repeat(1500), "</a>".repeat(1500));
    let doc = parse_xml_str(&ok).unwrap();
    // The writer is iterative, so deep trees serialise without recursion.
    let expected = format!("{}<a/>{}", "<a>".repeat(1499), "</a>".repeat(1499));
    assert!(fragment(&doc.root) == expected, "deep tree round trip");
    let shallow = ParseOptions {
        max_depth: 10,
        ..ParseOptions::default()
    };
    assert!(parse_str_with(&"<a>".repeat(11), shallow).is_err());
}

#[test]
fn normalises_non_canonical_prefixes_to_the_office_table() {
    let doc = parse_xml_str(&format!(
        r#"<ns0:document xmlns:ns0="{}" xmlns:mc="{}" xmlns:x1="{}" mc:Ignorable="x1"><ns0:body><ns0:p x1:paraId="1"/></ns0:body></ns0:document>"#,
        ns::W,
        ns::MC,
        ns::W14
    ))
    .unwrap();
    assert_eq!(doc.root.name, "w:document");
    assert_eq!(doc.root.attr("xmlns:w"), Some(ns::W));
    assert_eq!(doc.root.attr("mc:Ignorable"), Some("w14"));
    assert_eq!(
        doc.root
            .child_path(&["w:body", "w:p"])
            .unwrap()
            .attr("w14:paraId"),
        Some("1")
    );
    // The result is still valid, namespace-correct XML.
    assert!(undeclared_prefixes(&doc.root).is_empty());
}

#[test]
fn moves_a_default_wordprocessingml_namespace_onto_the_w_prefix() {
    let doc = parse_xml_str(&format!(
        r#"<document xmlns="{}"><body><p/></body></document>"#,
        ns::W
    ))
    .unwrap();
    assert_eq!(doc.root.name, "w:document");
    assert!(doc.root.child("w:body").is_some());
    assert_eq!(doc.root.attr("xmlns:w"), Some(ns::W));
}

#[test]
fn keeps_spreadsheetml_default_and_maps_strict_uris() {
    let sheet = parse_xml_str(&format!(
        r#"<x:worksheet xmlns:x="{}"><x:sheetData/></x:worksheet>"#,
        ns::X
    ))
    .unwrap();
    assert_eq!(sheet.root.name, "worksheet");
    assert_eq!(sheet.root.attr("xmlns"), Some(ns::X));
    let strict = parse_xml_str(
        r#"<w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main"/>"#,
    )
    .unwrap();
    assert_eq!(strict.root.name, "w:document");
    assert_eq!(
        canonical_prefix("http://purl.oclc.org/ooxml/drawingml/main"),
        Some("a")
    );
}

#[test]
fn a_prefix_already_bound_elsewhere_is_not_reused() {
    // `w` is bound to a foreign namespace, so the WordprocessingML namespace keeps its own prefix.
    let doc = parse_xml_str(&format!(
        r#"<w:root xmlns:w="urn:other"><q:p xmlns:q="{}"/></w:root>"#,
        ns::W
    ))
    .unwrap();
    assert_eq!(doc.root.name, "w:root");
    assert_eq!(doc.root.first_element().unwrap().name, "q:p");
    assert!(undeclared_prefixes(&doc.root).is_empty());
}

#[test]
fn can_leave_prefixes_untouched() {
    let doc = parse_str_with(
        &format!(r#"<ns0:a xmlns:ns0="{}"/>"#, ns::W),
        ParseOptions {
            normalize_prefixes: false,
            ..ParseOptions::default()
        },
    )
    .unwrap();
    assert_eq!(doc.root.name, "ns0:a");
}

#[test]
fn declares_used_namespaces_on_the_root_only_when_used_in_table_order() {
    let tree = XmlElement::new("w:document").with_child(
        XmlElement::new("w:body").with_child(
            XmlElement::new("w:p")
                .with_attr("w14:paraId", "1")
                .with_child(XmlElement::new("m:oMath")),
        ),
    );
    assert_eq!(undeclared_prefixes(&tree), ["w", "w14", "m"]);
    let xml = fragment(&tree);
    assert_eq!(
        xml,
        format!(
            r#"<w:document xmlns:w="{}" xmlns:w14="{}" xmlns:m="{}"><w:body><w:p w14:paraId="1"><m:oMath/></w:p></w:body></w:document>"#,
            ns::W,
            ns::W14,
            ns::M
        )
    );
    assert!(!xml.contains("xmlns:a="));
}

#[test]
fn treats_prefixes_named_by_mc_ignorable_as_used() {
    let tree = XmlElement::new("w:document").with_attr("mc:Ignorable", "w14 wp14");
    let xml = fragment(&tree);
    assert!(xml.contains(&format!(r#"xmlns:mc="{}""#, ns::MC)));
    assert!(xml.contains(&format!(r#"xmlns:w14="{}""#, ns::W14)));
    assert!(xml.contains(&format!(r#"xmlns:wp14="{}""#, ns::WP14)));
    assert!(xml.find("xmlns:").unwrap() < xml.find("mc:Ignorable").unwrap());
}

#[test]
fn refuses_unknown_undeclared_prefixes() {
    let err = write_element(&XmlElement::new("zz:x"), &WriteOptions::default()).unwrap_err();
    assert!(err.to_string().contains("zz"));
}

#[test]
fn strips_characters_xml_forbids() {
    assert_eq!(
        strip_invalid_xml_chars("a\u{1}b\u{8}c\tok\u{FFFE}x😀"),
        "abc\tokx😀"
    );
    let e = XmlElement::new("t")
        .with_attr("v", "x\u{2}")
        .with_text("y\u{0}");
    assert_eq!(fragment(&e), r#"<t v="x">y</t>"#);
}

#[test]
fn decodes_utf16_and_bom_prefixed_parts() {
    let s = "<a>é</a>";
    let le: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain(s.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    assert_eq!(decode_xml_bytes(&le), s);
    let be: Vec<u8> = [0xfe, 0xff]
        .into_iter()
        .chain(s.encode_utf16().flat_map(u16::to_be_bytes))
        .collect();
    assert_eq!(decode_xml_bytes(&be), s);
    let no_bom: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert_eq!(decode_xml_bytes(&no_bom), s);
    let u8bom: Vec<u8> = [0xef, 0xbb, 0xbf].into_iter().chain(s.bytes()).collect();
    assert_eq!(decode_xml_bytes(&u8bom), s);
    assert_eq!(parse_xml(&le).unwrap().root.text(), "é");
    // A UTF-16 part that declares itself UTF-16 parses too.
    let declared = r#"<?xml version="1.0" encoding="UTF-16"?><a>ü</a>"#;
    let bytes: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain(declared.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    assert_eq!(parse_xml(&bytes).unwrap().root.text(), "ü");
}

#[test]
fn attribute_names_are_plain_data() {
    // There is no prototype to pollute: odd names are ordinary attributes.
    let doc = parse_xml_str(r#"<a __proto__="x" constructor="y" val="z"/>"#).unwrap();
    assert_eq!(doc.root.attr("__proto__"), Some("x"));
    assert_eq!(doc.root.attr("toString"), None);
    assert_eq!(doc.root.attrs.len(), 3);
}

mod queries {
    use super::*;

    fn p() -> XmlElement {
        parse_xml_str(&format!(
            r#"<w:p {}><w:pPr><w:jc w:val="center"/><w:keepNext/><w:widowControl w:val="0"/></w:pPr><w:r><w:t>A</w:t></w:r><w:r><w:t>B</w:t></w:r></w:p>"#,
            w()
        ))
        .unwrap()
        .root
    }

    #[test]
    fn navigates_children() {
        let p = p();
        assert_eq!(p.child("w:pPr").unwrap().name, "w:pPr");
        assert!(p.child("w:nope").is_none());
        assert_eq!(p.children_named("w:r").count(), 2);
        assert_eq!(p.elements().count(), 3);
        assert_eq!(p.children_named_any(&["w:r", "w:pPr"]).count(), 3);
        let texts: Vec<String> = p.descendants("w:t").iter().map(|t| t.text()).collect();
        assert_eq!(texts, ["A", "B"]);
        assert_eq!(
            p.child_path(&["w:pPr", "w:jc"]).unwrap().attr("w:val"),
            Some("center")
        );
        assert!(p.child_path(&["w:pPr", "w:nope", "x"]).is_none());
    }

    #[test]
    fn reads_typed_attributes_and_toggles() {
        let e = XmlElement::new("x")
            .with_attr("i", "42")
            .with_attr("n", "-1.5")
            .with_attr("b1", "on")
            .with_attr("b0", "false")
            .with_attr("bad", "x1");
        assert_eq!(e.attr_int("i"), Some(42));
        assert_eq!(e.attr_int("n"), None);
        assert_eq!(e.attr_num("n"), Some(-1.5));
        assert_eq!(e.attr_bool("b1"), Some(true));
        assert_eq!(e.attr_bool("b0"), Some(false));
        assert!(e.attr_bool("missing").unwrap_or(true));
        assert_eq!(e.attr_num("bad"), None);
        let p = p();
        let ppr = p.child("w:pPr").unwrap();
        assert_eq!(ppr.toggle("w:keepNext", "w:val"), Some(true));
        assert_eq!(ppr.toggle("w:widowControl", "w:val"), Some(false));
        assert_eq!(ppr.toggle("w:keepLines", "w:val"), None);
    }

    #[test]
    fn edits_trees() {
        let p = p();
        let mut copy = p.clone();
        copy.set_attr("w:rsidR", Some("00FF"));
        copy.set_attr::<&str>("w:rsidR", None);
        assert_eq!(copy.attr("w:rsidR"), None);
        assert_eq!(copy.remove_children("w:r"), 2);
        copy.upsert_child(
            XmlElement::new("w:r").with_child(XmlElement::new("w:t").with_text("C")),
            &[],
        );
        copy.upsert_child(XmlElement::new("w:pPr"), &["w:r"]);
        let names: Vec<&str> = copy.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["w:pPr", "w:r"]);
        assert_eq!(copy.text(), "C");
        // The original is untouched.
        assert_eq!(p.children_named("w:r").count(), 2);
        // Upsert inserts before the first schema-later sibling.
        let mut q = XmlElement::new("w:p").with_child(XmlElement::new("w:r"));
        q.upsert_child(XmlElement::new("w:pPr"), &["w:r"]);
        assert_eq!(q.first_element().unwrap().name, "w:pPr");
    }

    #[test]
    fn selects_alternate_content_branches() {
        let ac = parse_xml_str(&format!(
            r#"<mc:AlternateContent xmlns:mc="{}" xmlns:wps="{}"><mc:Choice Requires="wps"><wps:wsp/></mc:Choice><mc:Fallback><w:pict {}/></mc:Fallback></mc:AlternateContent>"#,
            ns::MC,
            ns::WPS,
            w()
        ))
        .unwrap()
        .root;
        assert_eq!(ac.select_alternate_content(&["wps"])[0].name, "wps:wsp");
        assert_eq!(ac.select_alternate_content(&[])[0].name, "w:pict");
    }

    #[test]
    fn splits_qualified_names() {
        assert_eq!(split_qname("w:p"), ("w", "p"));
        assert_eq!(split_qname("sheetData"), ("", "sheetData"));
    }
}

fn keep_raw(max_depth: usize) -> ParseOptions {
    ParseOptions {
        max_depth,
        keep_too_deep_as_raw: true,
        ..ParseOptions::default()
    }
}

#[test]
fn keeps_elements_beyond_the_depth_limit_as_verbatim_markup() {
    let xml = format!(
        r#"<ns0:a xmlns:ns0="{}" xmlns="urn:d"><ns0:b><ns0:c x="1">t &amp; &#65;<d/><!--k--></ns0:c><e/></ns0:b></ns0:a>"#,
        ns::W
    );
    let doc = parse_str_with(&xml, keep_raw(2)).unwrap();
    // The tree is normalised down to the limit; below it the source is kept as written, with the
    // bindings in scope declared on its root so it means the same wherever it is written.
    assert_eq!(doc.root.name, "w:a");
    let b = doc.root.elements().next().unwrap();
    assert_eq!(b.name, "w:b");
    let raws: Vec<&str> = b
        .children
        .iter()
        .filter_map(|c| match c {
            XmlNode::Raw(r) => Some(r.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        raws,
        [format!(
            r#"<ns0:c xmlns:ns0="{}" xmlns="urn:d" x="1">t &amp; &#65;<d/><!--k--></ns0:c>"#,
            ns::W
        )
        .as_str()]
    );
    // Empty elements at the limit are leaves and stay in the tree.
    assert_eq!(
        b.elements().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        ["e"]
    );
    // Written back unchanged, and the result parses to the same text.
    let written = fragment(&doc.root);
    assert!(written.contains(raws[0]), "{written}");
    let again = parse_xml_str(&written).unwrap();
    assert_eq!(again.root.text(), "t & A");
}

#[test]
fn verbatim_markup_keeps_its_own_declarations_and_any_depth() {
    let deep = format!(
        r#"<a xmlns:x="urn:x"><b xmlns:x="urn:y">{}{}</b></a>"#,
        "<x:c>".repeat(50_000),
        "</x:c>".repeat(50_000)
    );
    // On a small stack: the over-deep part is counted, never built.
    let doc = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || parse_str_with(&deep, keep_raw(1)).unwrap())
        .unwrap()
        .join()
        .unwrap();
    let Some(XmlNode::Raw(raw)) = doc.root.children.first() else {
        panic!("raw node expected");
    };
    // `b` redeclares `x`, so the outer binding is not added a second time.
    assert!(
        raw.starts_with(r#"<b xmlns:x="urn:y"><x:c>"#),
        "{}",
        &raw[..40]
    );
    assert!(raw.ends_with("</x:c></b>"));
}

#[test]
fn verbatim_markup_is_still_checked() {
    for bad in [
        "<a><b><c></b></c></a>",
        "<a><b><c>&nope;</c></b></a>",
        "<a><b><c><!DOCTYPE x></c></b></a>",
        "<a><b><c>",
    ] {
        assert!(parse_str_with(bad, keep_raw(1)).is_err(), "{bad}");
    }
    // Without the option the limit still refuses the part.
    let strict = ParseOptions {
        max_depth: 1,
        ..ParseOptions::default()
    };
    assert!(parse_str_with("<a><b></b></a>", strict).is_err());
    assert!(parse_str_with("<a><b></b></a>", keep_raw(1)).is_ok());
}
