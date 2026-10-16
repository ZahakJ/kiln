// SPDX-License-Identifier: AGPL-3.0-or-later
//! Golden PNGs of the scratch document (`fixtures/golden/scene/*.png`): the whole path from text
//! through layout, frame production and `SceneDelta` to the raster painter. Regenerate with
//! `KILN_UPDATE_GOLDEN=1 cargo nextest run -p kiln-session golden` and review the images.

use std::path::PathBuf;

use kiln_protocol::geom::Point;
use kiln_protocol::{
    Command, DocKind, OpenHint, SelectUnit, SuiteCommand, ViewMode, Viewport, WriteCommand,
};
use kiln_raster::{Compositor, PaintOptions, Rgba, View, golden};
use kiln_scene::{DrawOp, Palette};

use crate::{DeferredHost, Session};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/golden/scene")
        .join(name)
}

struct Shot {
    session: Session,
    doc: kiln_common::DocId,
    compositor: Compositor,
}

impl Shot {
    fn open() -> Self {
        let mut session = Session::new(Box::new(DeferredHost::new(|| 1_792_000_000_000)));
        let bytes = std::fs::read(fixture("lorem.txt")).expect("lorem.txt");
        let doc = session
            .open(
                &bytes,
                OpenHint {
                    name: Some("lorem.txt".into()),
                    ..OpenHint::default()
                },
            )
            .expect("open")
            .doc;
        Self {
            session,
            doc,
            compositor: Compositor::new(),
        }
    }

    fn run(&mut self, cmd: impl Into<Command>) {
        self.session
            .execute(Some(self.doc), cmd.into())
            .expect("command");
    }

    fn render(
        &mut self,
        w: f32,
        h: f32,
        scroll: Point,
        zoom: f32,
        dpr: f32,
        palette: &Palette,
    ) -> Rgba {
        let vp = Viewport {
            width_px: w,
            height_px: h,
            scroll,
            zoom,
            dpr,
            mode: ViewMode::PrintLayout,
        };
        self.session.set_viewport(self.doc, vp).expect("viewport");
        let delta = self.session.frame(self.doc).expect("frame");
        // Through postcard, exactly as the bridges deliver it.
        let bytes = delta.to_postcard().expect("encode");
        let delta = kiln_scene::SceneDelta::from_postcard(&bytes).expect("decode");
        self.compositor.apply(&delta).expect("apply");
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let view = View {
            width: (w * dpr).round() as u32,
            height: (h * dpr).round() as u32,
            scroll_x: scroll.x,
            scroll_y: scroll.y,
            options: PaintOptions::default(),
        };
        Rgba::from_pixmap(&self.compositor.render(&view, palette).expect("render"))
    }
}

/// The page centred in a 1000 px wide surface.
const CENTRED: Point = Point { x: -28.0, y: 0.0 };

#[test]
fn golden_lorem_light() {
    let mut s = Shot::open();
    let img = s.render(1000.0, 900.0, CENTRED, 1.0, 1.0, &Palette::light());
    golden::check(&img, &fixture("lorem.png")).unwrap();
}

#[test]
fn golden_lorem_dark() {
    let mut s = Shot::open();
    let img = s.render(1000.0, 900.0, CENTRED, 1.0, 1.0, &Palette::dark());
    golden::check(&img, &fixture("lorem-dark.png")).unwrap();
}

#[test]
fn golden_selection_at_2x() {
    let mut s = Shot::open();
    // Double-click "Vestibulum" in the second paragraph, then Shift+double-click-drag into the
    // third: the selection grows by whole words.
    s.run(WriteCommand::SelectAt {
        point: Point::new(302.0, 207.0),
        extend: false,
        unit: SelectUnit::Word,
    });
    s.run(WriteCommand::SelectAt {
        point: Point::new(392.0, 295.0),
        extend: true,
        unit: SelectUnit::Word,
    });
    let img = s.render(
        600.0,
        420.0,
        Point::new(100.0, 140.0),
        1.0,
        2.0,
        &Palette::light(),
    );
    golden::check(&img, &fixture("selection-2x.png")).unwrap();
}

#[test]
fn golden_composition_and_caret() {
    let mut s = Shot::open();
    s.run(WriteCommand::SelectAt {
        point: Point::new(400.0, 140.0),
        extend: false,
        unit: SelectUnit::Char,
    });
    s.run(WriteCommand::SetComposition {
        text: "kiln glaze".into(),
        caret: 4,
    });
    let img = s.render(
        560.0,
        300.0,
        Point::new(140.0, 90.0),
        1.5,
        1.0,
        &Palette::light(),
    );
    golden::check(&img, &fixture("composition.png")).unwrap();
}

#[test]
fn frames_are_incremental_after_typing() {
    let mut s = Shot::open();
    let _ = s.render(1000.0, 900.0, CENTRED, 1.0, 1.0, &Palette::light());
    s.run(WriteCommand::InsertText {
        text: "Kiln ".into(),
    });
    let delta = s.session.frame(s.doc).expect("frame");
    assert!(!delta.reset);
    assert_eq!(delta.tiles.len(), 1, "only the edited page is resent");
    assert!(delta.overlay.is_some(), "the caret moved");
    let again = s.session.frame(s.doc).expect("frame");
    assert!(again.is_empty(), "nothing changed: {again:?}");
    s.run(SuiteCommand::Repaint);
    let full = s.session.frame(s.doc).expect("frame");
    assert!(full.reset && !full.tiles.is_empty() && !full.atlas.is_empty());
}

#[test]
fn caret_overlay_tracks_the_selection() {
    let mut s = Shot::open();
    let _ = s.render(1000.0, 900.0, CENTRED, 1.0, 1.0, &Palette::light());
    s.run(WriteCommand::MoveCaret {
        motion: kiln_protocol::CaretMotion::LineDown,
        extend: false,
    });
    let d = s.session.frame(s.doc).expect("frame");
    let overlay = d.overlay.expect("overlay");
    let caret = overlay
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Caret { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("caret");
    // One line below the first line's top (page top 32 + margin 96).
    assert!(
        caret.y > 128.0 + 10.0 && caret.y < 128.0 + 30.0,
        "{caret:?}"
    );
}

#[test]
fn new_documents_paint_an_empty_page() {
    let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = session.new_document(DocKind::Scratch).unwrap().doc;
    session
        .set_viewport(
            doc,
            Viewport {
                width_px: 800.0,
                height_px: 600.0,
                ..Viewport::default()
            },
        )
        .unwrap();
    let d = session.frame(doc).unwrap();
    assert!(d.reset);
    assert_eq!(d.tiles.len(), 1);
    assert!(d.atlas.is_empty(), "no glyphs on an empty page");
}

/// `fixtures/golden/scene/lorem-frame.bin`: the postcard bytes of the first frame of `lorem.txt`
/// in a 1000 × 900 surface, which the TypeScript tests decode and paint with a recording canvas.
/// Rewritten with `KILN_UPDATE_GOLDEN=1`; otherwise it must decode and describe the same tiles.
#[test]
fn frame_fixture_decodes_and_matches_the_layout() {
    let mut s = Shot::open();
    let vp = Viewport {
        width_px: 1000.0,
        height_px: 900.0,
        scroll: CENTRED,
        zoom: 1.0,
        dpr: 1.0,
        mode: ViewMode::PrintLayout,
    };
    s.session.set_viewport(s.doc, vp).expect("viewport");
    let delta = s.session.frame(s.doc).expect("frame");
    let path = fixture("lorem-frame.bin");
    if std::env::var_os("KILN_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, delta.to_postcard().expect("encode")).expect("write");
    }
    let bytes = std::fs::read(&path).expect("lorem-frame.bin (run with KILN_UPDATE_GOLDEN=1)");
    let on_disk = kiln_scene::SceneDelta::from_postcard(&bytes).expect("decodes");
    assert_eq!(on_disk.extent, delta.extent);
    assert_eq!(
        on_disk
            .tiles
            .iter()
            .map(|t| (t.id, t.rect))
            .collect::<Vec<_>>(),
        delta
            .tiles
            .iter()
            .map(|t| (t.id, t.rect))
            .collect::<Vec<_>>()
    );
    assert!(on_disk.reset && !on_disk.atlas.is_empty() && on_disk.overlay.is_some());
}
