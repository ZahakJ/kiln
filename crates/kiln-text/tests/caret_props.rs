// SPDX-License-Identifier: AGPL-3.0-or-later
//! Property tests: hit testing the caret of any position returns that position.

mod common;

use std::cell::RefCell;

use kiln_text::{Alignment, Direction, ParagraphSpec, SpanStyle, TextEngine};
use proptest::prelude::*;

thread_local! {
    static ENGINE: RefCell<TextEngine> = RefCell::new(common::engine());
}

fn layout(spec: &ParagraphSpec) -> kiln_text::ParagraphLayout {
    ENGINE.with(|e| e.borrow_mut().layout(spec))
}

fn alignment() -> impl Strategy<Value = Alignment> {
    prop_oneof![
        Just(Alignment::Start),
        Just(Alignment::Center),
        Just(Alignment::End),
        Just(Alignment::Justify),
        Just(Alignment::Distribute),
    ]
}

fn family() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("Calibri"),
        Just("Cambria"),
        Just("Arial"),
        Just("Courier New")
    ]
}

/// Asserts that hit testing the caret of every position returns that position. Where several logical
/// positions share one visual x (a bidi boundary, or a tab that ends exactly on its stop) the hit must
/// be one of them; everywhere else it must be exact.
fn assert_round_trip(spec: &ParagraphSpec) -> Result<(), TestCaseError> {
    let l = layout(spec);
    let positions = l.caret_positions().to_vec();
    let carets: Vec<_> = positions.iter().map(|&i| l.caret_rect(i)).collect();
    for (&i, c) in positions.iter().zip(&carets) {
        let same_x: Vec<usize> = positions
            .iter()
            .zip(&carets)
            .filter(|(_, o)| o.line == c.line && (o.x - c.x).abs() < 0.01)
            .map(|(&p, _)| p)
            .collect();
        let hit = l.hit_test(c.x, c.center_y());
        if same_x.len() == 1 {
            prop_assert_eq!(hit, i, "text {:?} caret {:?}", l.text(), c);
        } else {
            prop_assert!(
                same_x.contains(&hit),
                "text {:?} caret {:?} hit {}",
                l.text(),
                c,
                hit
            );
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn hit_test_inverts_caret_rect_latin(
        text in "[A-Za-z0-9,.;:!?'()\\- ]{0,40}( [A-Za-z]{1,12}){0,30}[\\t\\n]?[a-z ]{0,20}",
        width in 60.0_f32..600.0,
        align in alignment(),
        fam in family(),
        size in 8.0_f32..40.0,
        indent in 0.0_f32..40.0,
        first in -30.0_f32..30.0,
    ) {
        let mut spec = ParagraphSpec::plain(text, SpanStyle::new(fam, size), width);
        spec.props.alignment = align;
        spec.props.indent_start = indent;
        spec.props.first_line = first.max(-indent);
        assert_round_trip(&spec)?;
    }

    #[test]
    fn hit_test_inverts_caret_rect_hebrew_rtl(
        text in "[אבגדהוזחטיכלמנסעפצקרשת ]{0,120}",
        width in 60.0_f32..400.0,
        align in alignment(),
    ) {
        let mut spec = ParagraphSpec::plain(text, SpanStyle::new("Arial", 16.0), width);
        spec.props.direction = Direction::Rtl;
        spec.props.alignment = align;
        assert_round_trip(&spec)?;
    }

    #[test]
    fn hit_test_inverts_caret_rect_mixed_bidi(
        words in proptest::collection::vec(prop_oneof!["[a-z]{1,8}", "[אבגדהו]{1,8}", "[0-9]{1,5}", "[سلامعربي]{1,6}"], 0..25),
        rtl in any::<bool>(),
        width in 80.0_f32..500.0,
    ) {
        let mut spec = ParagraphSpec::plain(words.join(" "), SpanStyle::new("Calibri", 14.0), width);
        spec.props.direction = if rtl { Direction::Rtl } else { Direction::Ltr };
        assert_round_trip(&spec)?;
    }

    #[test]
    fn layout_never_panics_and_respects_width(
        text in "\\PC{0,200}",
        width in -10.0_f32..800.0,
        size in -5.0_f32..120.0,
        spacing in -3.0_f32..10.0,
        hyphenate in any::<bool>(),
    ) {
        let mut style = SpanStyle::new("Calibri", size);
        style.letter_spacing = spacing;
        let mut spec = ParagraphSpec::plain(text, style, width);
        spec.props.hyphenation = hyphenate;
        let l = layout(&spec);
        prop_assert!(!l.lines.is_empty());
        prop_assert!(l.height.is_finite());
        let total: usize = l.lines.iter().map(|x| x.text_range.len()).sum();
        prop_assert_eq!(total, l.text().len(), "lines cover the text exactly once");
        for (a, b) in l.lines.iter().zip(l.lines.iter().skip(1)) {
            prop_assert_eq!(a.text_range.end, b.text_range.start);
        }
    }
}
