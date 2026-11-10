// SPDX-License-Identifier: AGPL-3.0-or-later
//! Property tests: part-name normalisation and XML escaping round trips.

use kiln_ooxml::opc::{
    normalize_part_name, part_key, relative_target, resolve_target, same_part, validate_part_name,
};
use kiln_ooxml::xml::{
    WriteOptions, XmlElement, is_xml_char, parse_xml_str, strip_invalid_xml_chars, write_element,
};
use proptest::prelude::*;

/// A valid part-name segment: no `/`, `%`, `#`, `?`, `\`, not `.`/`..`, not ending with `.`.
fn segment() -> impl Strategy<Value = String> {
    "[A-Za-z0-9_\\-][A-Za-z0-9_\\-.]{0,7}[A-Za-z0-9_\\-]?"
        .prop_filter("no trailing dot", |s| !s.ends_with('.'))
}

fn part_name() -> impl Strategy<Value = String> {
    prop::collection::vec(segment(), 1..5).prop_map(|segs| format!("/{}", segs.join("/")))
}

/// Raw references as found in the wild: slashes either way, `.`/`..`, doubled separators,
/// percent-escapes.
fn wild_reference() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            segment(),
            Just(".".to_owned()),
            Just("..".to_owned()),
            Just(String::new()),
            Just("a%20b".to_owned()),
            Just("caf%C3%A9".to_owned()),
        ],
        0..6,
    )
    .prop_flat_map(|segs| {
        let n = segs.len();
        (Just(segs), prop::collection::vec(prop::bool::ANY, n + 1))
    })
    .prop_map(|(segs, backslash)| {
        let mut s = String::new();
        if backslash[0] {
            s.push('/');
        }
        for (i, seg) in segs.iter().enumerate() {
            if i > 0 {
                s.push(if backslash[i] { '\\' } else { '/' });
            }
            s.push_str(seg);
        }
        s
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn normalisation_is_idempotent_and_yields_valid_names(raw in wild_reference()) {
        if let Ok(n) = normalize_part_name(&raw) {
            prop_assert!(validate_part_name(&n).is_ok(), "{n}");
            prop_assert_eq!(normalize_part_name(&n), Ok(n.clone()));
            prop_assert!(!n.contains('\\') && !n.contains("//") && !n.contains("/./"));
        }
    }

    #[test]
    fn valid_names_normalise_to_themselves(name in part_name()) {
        prop_assert!(validate_part_name(&name).is_ok());
        prop_assert_eq!(normalize_part_name(&name), Ok(name));
    }

    #[test]
    fn relative_targets_resolve_back(source in part_name(), target in part_name()) {
        let rel = relative_target(&source, &target);
        prop_assert!(!rel.starts_with('/'));
        prop_assert_eq!(resolve_target(&source, &rel), target.clone());
        // The package itself as the source gives the path without the leading slash.
        prop_assert_eq!(resolve_target("/", &relative_target("/", &target)), target);
    }

    #[test]
    fn part_keys_compare_case_insensitively(name in part_name()) {
        let upper = name.to_uppercase();
        prop_assert_eq!(part_key(&name), part_key(&upper));
        prop_assert!(same_part(&name, &upper));
    }

    #[test]
    fn text_round_trips_through_escaping(text in any::<String>()) {
        let e = XmlElement::new("t").with_text(text.clone());
        let xml = write_element(&e, &WriteOptions::fragment()).unwrap();
        let back = parse_xml_str(&xml).unwrap().root;
        prop_assert_eq!(back.text(), strip_invalid_xml_chars(&text).into_owned());
    }

    #[test]
    fn attribute_values_round_trip_through_escaping(value in any::<String>()) {
        let e = XmlElement::new("t").with_attr("v", value.as_str());
        let xml = write_element(&e, &WriteOptions::fragment()).unwrap();
        let back = parse_xml_str(&xml).unwrap().root;
        let expected = strip_invalid_xml_chars(&value).into_owned();
        prop_assert_eq!(back.attr("v").unwrap(), expected.as_str());
    }

    #[test]
    fn stripping_leaves_only_xml_characters(s in any::<String>()) {
        let out = strip_invalid_xml_chars(&s);
        prop_assert!(out.chars().all(is_xml_char));
        // Valid input is returned unchanged.
        if s.chars().all(is_xml_char) {
            prop_assert_eq!(out.as_ref(), s.as_str());
        }
    }

    #[test]
    fn markup_like_text_never_escapes_its_element(text in "[<>&\"'a-z ;#/!\\[\\]-]{0,40}") {
        let e = XmlElement::new("w:t").with_text(text.clone());
        let xml = write_element(&e, &WriteOptions::fragment()).unwrap();
        let back = parse_xml_str(&xml).unwrap().root;
        prop_assert_eq!(back.name.as_str(), "w:t");
        prop_assert_eq!(back.text(), text);
        prop_assert!(back.elements().next().is_none());
    }
}
