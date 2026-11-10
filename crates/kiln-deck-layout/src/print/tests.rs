// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

/// Counts what pages paint.
#[derive(Default)]
struct Count {
    texts: usize,
    text: String,
    ops: usize,
    open: Option<Mark>,
    marks: usize,
}

impl PageSink for Count {
    fn op(&mut self, op: &DrawOp) {
        assert!(
            !matches!(op, DrawOp::Glyphs(_)),
            "a glyph bitmap reached the page"
        );
        self.ops += 1;
    }
    fn text(&mut self, run: &TextRun<'_>) {
        self.texts += 1;
        self.text.push_str(&run.text[run.run.text_range.clone()]);
        self.text.push(' ');
    }
    fn begin(&mut self, mark: Mark) {
        assert!(self.open.is_none(), "marks do not nest");
        self.open = Some(mark);
        self.marks += 1;
    }
    fn end(&mut self) {
        assert!(self.open.take().is_some());
    }
}

fn paint_all(print: &DeckPrint<'_>) -> Count {
    let mut c = Count::default();
    for i in 0..print.page_count() {
        print.paint(i, &mut c);
    }
    c
}

fn fixture(name: &str) -> Presentation {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/pptx/features")
        .join(name);
    kiln_io_pptx::read_pptx(&std::fs::read(path).expect("fixture"))
        .expect("reads")
        .presentation
}

fn renderer() -> Renderer {
    Renderer::new(crate::fonts::default_fonts(), crate::fonts::default_fonts())
}

fn pages_of(pres: &Presentation, opts: PrintOptions) -> usize {
    let mut r = renderer();
    let print = DeckPrint::new(&mut r, pres, opts);
    paint_all(&print);
    print.page_count()
}

#[test]
fn every_layout_has_its_page_count() {
    let pres = fixture("placeholders.pptx");
    let n = pres.slides.iter().filter(|s| !s.hidden).count();
    assert!(n >= 3, "{n}");
    for (layout, want) in [
        (Layout::Slides, n),
        (Layout::Notes, n),
        (Layout::handouts(1, false), n),
        (Layout::handouts(2, false), n.div_ceil(2)),
        (Layout::handouts(3, false), n.div_ceil(3)),
        (Layout::handouts(4, true), n.div_ceil(4)),
        (Layout::handouts(6, false), n.div_ceil(6)),
        (Layout::handouts(9, false), n.div_ceil(9)),
    ] {
        let pages = pages_of(
            &pres,
            PrintOptions {
                layout,
                ..PrintOptions::default()
            },
        );
        assert_eq!(pages, want, "{layout:?}");
    }
    assert_eq!(Layout::handouts(5, false), Layout::handouts(6, false));
    assert_eq!(
        Layout::handouts(12, true),
        Layout::Handouts {
            per_page: 9,
            vertical: true
        }
    );
}

#[test]
fn slide_text_is_real_text_and_tagged() {
    let pres = fixture("placeholders.pptx");
    let mut r = renderer();
    let print = DeckPrint::new(&mut r, &pres, PrintOptions::default());
    // Every run became a recorded run: no glyph bitmaps are left in the ops.
    let runs: usize = print.printed.iter().map(|p| p.runs.len()).sum();
    assert!(runs > 0);
    for p in &print.printed {
        for s in &p.drawn.shapes {
            for op in &s.ops {
                if let DrawOp::Glyphs(_) = op {
                    assert!(recorded_index(op).is_some());
                }
            }
        }
    }
    let tree = print.structure();
    assert!(
        tree.nodes
            .iter()
            .any(|n| matches!(n.kind, StructKind::Heading { .. }))
    );
    assert_eq!(print.bookmarks().len(), print.slide_count());
    let c = paint_all(&print);
    // Turned text is drawn as outlines; the rest is text.
    assert!(
        c.texts > 0 && c.texts <= runs && c.marks > 0,
        "{} texts",
        c.texts
    );
    assert!(c.text.contains(&print.printed[0].title), "{}", c.text);
}

#[test]
fn hidden_slides_print_only_when_asked() {
    let mut pres = fixture("placeholders.pptx");
    let n = pres.slides.len();
    pres.slides[0].hidden = true;
    let without = pages_of(&pres, PrintOptions::default());
    let with = pages_of(
        &pres,
        PrintOptions {
            hidden: true,
            ..PrintOptions::default()
        },
    );
    assert_eq!((without, with), (n - 1, n));
    let chosen = pages_of(
        &pres,
        PrintOptions {
            hidden: true,
            slides: vec![1, 0],
            ..PrintOptions::default()
        },
    );
    assert_eq!(chosen, 2);
}

#[test]
fn handout_cells_fit_the_page_in_order() {
    let page = (720.0, 960.0);
    let slide = (960.0, 540.0);
    for per in Layout::HANDOUTS {
        let cells = handout_cells(per, false, page, slide);
        assert_eq!(cells.len(), usize::from(per));
        for c in &cells {
            assert!(c.x >= 47.0 && c.right() <= 673.0 && c.y >= 71.0 && c.bottom() <= 889.0);
            assert!((c.w / c.h - 16.0 / 9.0).abs() < 0.01, "{per}: {c:?}");
        }
    }
    let rows = handout_cells(6, false, page, slide);
    assert!(rows[1].x > rows[0].x && (rows[1].y - rows[0].y).abs() < 0.1);
    let cols = handout_cells(6, true, page, slide);
    assert!(cols[1].y > cols[0].y && (cols[1].x - cols[0].x).abs() < 0.1);
    // Three per page leave the right half for notes.
    assert!(
        handout_cells(3, false, page, slide)
            .iter()
            .all(|c| c.right() <= 360.0)
    );
}

#[test]
fn notes_pages_carry_the_notes() {
    let pres = fixture("notes.pptx");
    let mut r = renderer();
    let print = DeckPrint::new(
        &mut r,
        &pres,
        PrintOptions {
            layout: Layout::Notes,
            ..PrintOptions::default()
        },
    );
    let with_text = print
        .pages
        .iter()
        .filter(|b| matches!(b, Body::Notes { text, .. } if !text.is_empty()))
        .count();
    assert!(with_text > 0);
    let Some(Body::Notes { image, .. }) = print.pages.first() else {
        panic!("a notes page")
    };
    // The slide image sits in the top half, the notes under it.
    assert!(image.bottom() < 0.5 * print.page.1);
}

#[test]
fn the_outline_flows_over_pages() {
    let pres = fixture("many-slides.pptx");
    let mut r = renderer();
    let print = DeckPrint::new(
        &mut r,
        &pres,
        PrintOptions {
            layout: Layout::Outline,
            ..PrintOptions::default()
        },
    );
    let headings = print
        .tree
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, StructKind::Heading { .. }))
        .count();
    assert_eq!(headings, pres.slides.iter().filter(|s| !s.hidden).count());
    assert!(print.page_count() >= 1);
    let c = paint_all(&print);
    assert!(c.text.contains(&print.printed[1].title), "{}", c.text);
}
