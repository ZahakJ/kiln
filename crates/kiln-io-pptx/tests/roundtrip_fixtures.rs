// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every deck of the corpus (`fixtures/pptx` and, when present, the external corpus) through
//! read → write → read: the model facts, the text and the python-pptx view come back unchanged,
//! every part the writer did not regenerate is byte-identical, writes are deterministic and a
//! second round trip changes nothing; slides whose animations did not change keep their original
//! `p:timing` byte for byte, including effects Kiln does not play.

mod read_support;

use std::collections::BTreeMap;

use kiln_io_pptx::{
    PptxRead, WriteOptions, WriteSource, plain_text, read_pptx, rewrite_pptx, snapshot_facts,
    write_pptx,
};
use kiln_ooxml::opc::OpcPackage;
use kiln_testing::fidelity::package::compare_packages;
use read_support::{deck_with_slide, decks, oracle_view, read_package_bytes, slide_xml, sp};

fn all_decks() -> Vec<std::path::PathBuf> {
    let mut d = decks("pptx");
    d.extend(decks("external/pptx"));
    d
}

/// Facts as a multiset.
fn facts(read: &PptxRead) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for f in snapshot_facts(read) {
        *m.entry(f).or_insert(0) += 1;
    }
    m
}

#[test]
fn every_deck_round_trips_with_its_facts_text_and_untouched_parts() {
    let decks = all_decks();
    assert!(decks.len() >= 19, "the feature corpus is missing");
    for path in decks {
        let name = path.display().to_string();
        let bytes = std::fs::read(&path).expect("fixture");
        let before = read_pptx(&bytes).expect("reads");
        let out = rewrite_pptx(&bytes, &WriteOptions::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let after = read_pptx(&out.bytes).unwrap_or_else(|e| panic!("{name}: re-read {e}"));
        assert_eq!(facts(&before), facts(&after), "{name}: model facts");
        assert_eq!(plain_text(&before), plain_text(&after), "{name}: text");
        assert_eq!(
            oracle_view(&before),
            oracle_view(&after),
            "{name}: python-pptx view"
        );
        assert_eq!(
            before.passthrough.len(),
            after.passthrough.len(),
            "{name}: passthrough parts"
        );
        // Every part the writer did not produce is the original's bytes.
        let mut skip = out.regenerated.clone();
        skip.extend(out.dropped.iter().cloned());
        let cmp = compare_packages(
            &OpcPackage::open(&bytes).expect("in"),
            &OpcPackage::open(&out.bytes).expect("out"),
            &skip,
        );
        assert!(
            cmp.differing.is_empty() && cmp.missing.is_empty(),
            "{name}: {cmp:?}"
        );
        assert!(out.dropped.is_empty(), "{name}: dropped {:?}", out.dropped);
        assert!(
            out.issues.iter().all(|i| i.code != "relationship.missing"),
            "{name}: {:?}",
            out.issues
        );
    }
}

#[test]
fn writes_are_deterministic_and_idempotent() {
    for path in all_decks() {
        let name = path.display().to_string();
        let bytes = std::fs::read(&path).expect("fixture");
        let first = rewrite_pptx(&bytes, &WriteOptions::default()).expect("first");
        let again = rewrite_pptx(&bytes, &WriteOptions::default()).expect("again");
        assert_eq!(first.bytes, again.bytes, "{name}: two writes differ");
        // Writing what Kiln wrote changes no part.
        let second = rewrite_pptx(&first.bytes, &WriteOptions::default()).expect("second");
        let a = OpcPackage::open(&first.bytes).expect("a");
        let b = OpcPackage::open(&second.bytes).expect("b");
        let cmp = compare_packages(&a, &b, &[]);
        assert!(
            cmp.differing.is_empty() && cmp.missing.is_empty() && cmp.added.is_empty(),
            "{name}: {cmp:?}"
        );
    }
}

/// The `<p:timing>…</p:timing>` of a slide part.
fn timing_of(xml: &str) -> Option<&str> {
    let start = xml.find("<p:timing")?;
    let end = xml[start..].find("</p:timing>")? + start + "</p:timing>".len();
    Some(&xml[start..end])
}

fn slide_text(pkg: &OpcPackage, part: &str) -> String {
    String::from_utf8(
        pkg.part(part)
            .expect("readable")
            .expect("present")
            .into_owned(),
    )
    .expect("utf-8")
}

#[test]
fn unchanged_slide_keeps_original_p_timing_bytes() {
    // A deck with an effect Kiln plays (Fade), one it does not (a motion path, kept as XML),
    // a trigger sequence and a build list.
    let timing = r#"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst><p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="4" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="5" presetID="10" presetClass="entr" presetSubtype="0" fill="hold" grpId="0" nodeType="clickEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:cTn id="6" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="2"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="visible"/></p:to></p:set><p:animEffect transition="in" filter="fade"><p:cBhvr><p:cTn id="7" dur="500"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:animEffect></p:childTnLst></p:cTn></p:par><p:par><p:cTn id="8" presetID="42" presetClass="path" presetSubtype="0" accel="50000" decel="50000" fill="hold" nodeType="withEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:animMotion origin="layout" path="M 0 0 L 0.25 0.1 E" pathEditMode="relative" ptsTypes=""><p:cBhvr><p:cTn id="9" dur="2000" fill="hold"/><p:tgtEl><p:spTgt spid="3"/></p:tgtEl><p:attrNameLst><p:attrName>ppt_x</p:attrName><p:attrName>ppt_y</p:attrName></p:attrNameLst></p:cBhvr></p:animMotion></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn><p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst><p:bldLst><p:bldP spid="2" grpId="0" animBg="1"/></p:bldLst></p:timing>"#;
    let tree = format!(
        "{}{}",
        sp(2, "Box", "<a:p><a:r><a:t>fades in</a:t></a:r></a:p>"),
        sp(3, "Path", "<a:p><a:r><a:t>moves</a:t></a:r></a:p>")
    );
    let pkg = deck_with_slide(&slide_xml("", &tree, timing), &[]);
    let read = read_package_bytes(&pkg);
    let slide = &read.presentation.slides[0];
    assert_eq!(slide.timeline.sequence.len(), 2);
    assert!(matches!(
        slide.timeline.sequence[1].effect,
        kiln_deck::AnimationEffect::Other { .. }
    ));
    let original = pkg.save().expect("save");
    let opened = OpcPackage::open(&original).expect("opens");
    let source = WriteSource::from_read(&read).with_package(&opened);
    let out =
        write_pptx(&read.presentation, Some(&source), &WriteOptions::default()).expect("writes");
    let written = OpcPackage::open(&out.bytes).expect("output opens");
    let before = slide_text(&opened, "/ppt/slides/slide1.xml");
    let after = slide_text(&written, "/ppt/slides/slide1.xml");
    assert_eq!(
        timing_of(&after),
        timing_of(&before),
        "p:timing bytes changed"
    );
    assert_eq!(timing_of(&after), Some(timing));
    // The fixture with PowerPoint's own timing keeps it too.
    let bytes = std::fs::read(read_support::fixtures().join("pptx/features/animations.pptx"))
        .expect("fixture");
    let out = rewrite_pptx(&bytes, &WriteOptions::default()).expect("writes");
    let (a, b) = (
        OpcPackage::open(&bytes).expect("in"),
        OpcPackage::open(&out.bytes).expect("out"),
    );
    for part in ["/ppt/slides/slide1.xml", "/ppt/slides/slide2.xml"] {
        let (x, y) = (slide_text(&a, part), slide_text(&b, part));
        assert!(timing_of(&x).is_some(), "{part}");
        assert_eq!(timing_of(&x), timing_of(&y), "{part}");
    }
}

#[test]
fn edited_animations_are_regenerated_and_read_back() {
    let bytes = std::fs::read(read_support::fixtures().join("pptx/features/animations.pptx"))
        .expect("fixture");
    let pkg = OpcPackage::open(&bytes).expect("opens");
    let read = kiln_io_pptx::read_package(&pkg).expect("reads");
    let mut pres = read.presentation.clone();
    // Slower fade, and the wipe removed.
    let seq = &mut pres.slides[0].timeline.sequence;
    assert_eq!(seq.len(), 3);
    seq[1].duration_ms = 1250;
    seq[1].delay_ms = 300;
    seq.remove(2);
    let expected = pres.slides[0].timeline.clone();
    let source = WriteSource::from_read(&read).with_package(&pkg);
    let out = write_pptx(&pres, Some(&source), &WriteOptions::default()).expect("writes");
    let back = read_pptx(&out.bytes).expect("reads back");
    let got = &back.presentation.slides[0].timeline;
    assert_eq!(got.sequence.len(), 2);
    for (g, e) in got.sequence.iter().zip(&expected.sequence) {
        assert_eq!(g.effect, e.effect);
        assert_eq!(g.start, e.start);
        assert_eq!(g.duration_ms, e.duration_ms);
        assert_eq!(g.delay_ms, e.delay_ms);
    }
    let xml = slide_text(
        &OpcPackage::open(&out.bytes).expect("out"),
        "/ppt/slides/slide1.xml",
    );
    let timing = timing_of(&xml).expect("timing");
    assert!(timing.contains("dur=\"1250\""), "{timing}");
    assert!(!timing.contains("wipe"), "{timing}");
    // The slide nobody touched keeps PowerPoint's original.
    let orig2 = slide_text(&pkg, "/ppt/slides/slide2.xml");
    let out2 = slide_text(
        &OpcPackage::open(&out.bytes).expect("out"),
        "/ppt/slides/slide2.xml",
    );
    assert_eq!(timing_of(&orig2), timing_of(&out2));
}

#[test]
fn transitions_kiln_does_not_play_are_kept_with_their_sounds() {
    let tail = r#"<mc:AlternateContent><mc:Choice Requires="p14"><p:transition spd="slow" p14:dur="1250"><p14:vortex dir="r"/><p:sndAc><p:stSnd><p:snd r:embed="rId5" name="chime.wav"/></p:stSnd></p:sndAc></p:transition></mc:Choice><mc:Fallback><p:transition spd="slow"><p:fade/></p:transition></mc:Fallback></mc:AlternateContent>"#;
    let mut pkg = deck_with_slide(
        &slide_xml(
            "",
            &sp(2, "Box", "<a:p><a:r><a:t>x</a:t></a:r></a:p>"),
            tail,
        ),
        &[(
            "rId5",
            &read_support::rt("audio"),
            "/ppt/media/audio1.wav",
            false,
        )],
    );
    pkg.add_part("/ppt/media/audio1.wav", "audio/wav", b"RIFF".to_vec())
        .expect("sound");
    let bytes = pkg.save().expect("save");
    let opened = OpcPackage::open(&bytes).expect("opens");
    let read = kiln_io_pptx::read_package(&opened).expect("reads");
    let t = read.presentation.slides[0]
        .transition
        .clone()
        .expect("transition");
    assert!(t.original.is_some(), "vortex is shown as a relative");
    let source = WriteSource::from_read(&read).with_package(&opened);
    let out =
        write_pptx(&read.presentation, Some(&source), &WriteOptions::default()).expect("writes");
    let written = OpcPackage::open(&out.bytes).expect("output opens");
    let xml = slide_text(&written, "/ppt/slides/slide1.xml");
    assert!(xml.contains("<p14:vortex dir=\"r\"/>"), "{xml}");
    assert!(xml.contains("<p:snd r:embed=\"rId5\""), "{xml}");
    let rels = written
        .relationships("/ppt/slides/slide1.xml")
        .expect("rels");
    let snd = rels.get("rId5").expect("the sound's relationship");
    assert_eq!(rels.resolve(snd).as_deref(), Some("/ppt/media/audio1.wav"));
    let back = read_pptx(&out.bytes).expect("reads back");
    assert_eq!(back.presentation.slides[0].transition.as_ref(), Some(&t));
    // Changing it to a transition Kiln plays replaces the original.
    let mut pres = read.presentation.clone();
    pres.slides[0].transition = Some(kiln_deck::Transition::new(
        kiln_deck::TransitionEffect::Wipe {
            dir: kiln_deck::transition::Side::Up,
        },
    ));
    let out = write_pptx(&pres, Some(&source), &WriteOptions::default()).expect("writes");
    let xml = slide_text(
        &OpcPackage::open(&out.bytes).expect("opens"),
        "/ppt/slides/slide1.xml",
    );
    assert!(!xml.contains("vortex"), "{xml}");
    assert!(xml.contains("<p:wipe dir=\"u\"/>"), "{xml}");
}
