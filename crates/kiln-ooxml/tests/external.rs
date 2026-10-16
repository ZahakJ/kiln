// SPDX-License-Identifier: AGPL-3.0-or-later
//! Real-world files from `tools/fetch-corpus.sh` (Apache POI's test documents). Skipped when the
//! corpus has not been downloaded (fresh clones, CI). Ported from
//! `reference/ts-core/ooxml/test/external.test.ts`.

mod common;

use common::{external_fixtures, fixture, unzip};
use kiln_ooxml::drawingml::geometry::{adjust_values, parse_geometry};
use kiln_ooxml::drawingml::presets::{is_preset_shape_type, preset_geometry};
use kiln_ooxml::drawingml::theme::{parse_theme_bytes, serialize_theme};
use kiln_ooxml::opc::{OpcError, OpcPackage, RelationshipOptions, rel_type};
use kiln_ooxml::passthrough::{CollectOptions, PassthroughSet};
use kiln_ooxml::props::read_document_properties;
use kiln_ooxml::xml::{document_to_bytes, parse_xml};

/// POI's deliberately hostile document: 15 000 nested elements. It must be refused cleanly.
const HOSTILE: &[&str] = &["external/docx/poi-deep-table-cell.docx"];

fn finite_svg(d: &str) -> bool {
    !d.contains("NaN") && !d.contains("inf")
}

#[test]
fn real_world_corpus_round_trips_and_parses_fully() {
    let files = external_fixtures();
    if files.is_empty() {
        eprintln!("fixtures/external is absent: run tools/fetch-corpus.sh to enable this test");
        return;
    }
    for name in &files {
        let bytes = fixture(name);
        let pkg = OpcPackage::open(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        // Byte-identical repack.
        let src = unzip(&bytes);
        let out = unzip(&pkg.save().unwrap());
        for e in &src {
            let o = out.iter().find(|o| o.name == e.name).unwrap();
            assert!(o.data == e.data && o.raw == e.raw, "{name}: {}", e.name);
        }
        let main = pkg.main_document_part().unwrap().expect("a main part");
        if HOSTILE.contains(&name.as_str()) {
            let err = pkg.part_xml(&main).unwrap_err();
            assert!(matches!(err, OpcError::Xml { .. }), "{name}");
            assert!(err.to_string().contains("nested"), "{name}: {err}");
            continue;
        }
        let names: Vec<String> = pkg.part_names().map(str::to_owned).collect();
        for part in &names {
            if !pkg.content_type(part).unwrap_or("").ends_with("xml") {
                continue;
            }
            // Every XML part parses, and its serialisation parses to the same tree.
            let doc = pkg
                .part_xml(part)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .unwrap();
            let again = parse_xml(&document_to_bytes(&doc).unwrap()).unwrap();
            assert_eq!(again.root, doc.root, "{name}: {part}");
            // Themes go through the model losslessly.
            if part.contains("/theme") && part.ends_with(".xml") {
                let t = parse_theme_bytes(&pkg.part_bytes(part).unwrap()).unwrap();
                let back =
                    parse_theme_bytes(&document_to_bytes(&serialize_theme(&t)).unwrap()).unwrap();
                assert_eq!(back, t, "{name}: {part}");
            }
            // Every preset and custom geometry evaluates to finite paths.
            for g in doc.root.descendants("a:prstGeom") {
                let prst = g.attr("prst").unwrap_or("");
                assert!(is_preset_shape_type(prst), "{name}: {part}: {prst}");
                let adj = adjust_values(g.child("a:avLst"));
                let pairs: Vec<(&str, f64)> = adj.iter().map(|(n, v)| (n.as_str(), *v)).collect();
                let geo = preset_geometry(prst, 914_400.0, 457_200.0, &pairs).unwrap();
                assert!(geo.paths.iter().all(|p| finite_svg(&p.to_svg())), "{prst}");
            }
            for g in doc.root.descendants("a:custGeom") {
                let geo = parse_geometry(g)
                    .resolve(914_400.0, 457_200.0, &[])
                    .unwrap();
                assert!(geo.paths.iter().all(|p| finite_svg(&p.to_svg())), "{name}");
            }
        }
        read_document_properties(&pkg).unwrap();
        // Passthrough with only the main part consumed re-creates every other part
        // byte-for-byte.
        let set =
            PassthroughSet::collect(&pkg, &CollectOptions::consumed([main.as_str()])).unwrap();
        let mut target = OpcPackage::new();
        target
            .add_part(
                &main,
                pkg.content_type(&main).unwrap(),
                pkg.part_bytes(&main).unwrap().into_owned(),
            )
            .unwrap();
        target
            .add_relationship(
                "/",
                rel_type::OFFICE_DOCUMENT,
                &main,
                RelationshipOptions::default(),
            )
            .unwrap();
        set.emit(&mut target, |n| Some(n.to_owned())).unwrap();
        let rebuilt = OpcPackage::open(&target.save().unwrap()).unwrap();
        for p in &names {
            assert_eq!(
                rebuilt.part_bytes(p).unwrap(),
                pkg.part_bytes(p).unwrap(),
                "{name}: {p}"
            );
        }
    }
}
