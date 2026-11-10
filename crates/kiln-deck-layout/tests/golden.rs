// SPDX-License-Identifier: AGPL-3.0-or-later
//! Golden renders of the feature fixtures' slides (`fixtures/golden/deck/`), painted by
//! kiln-raster from the same Scene the stage shows. `KILN_UPDATE_GOLDEN=1` rewrites them.

use std::path::{Path, PathBuf};

use kiln_deck::{Owner, Presentation};
use kiln_deck_layout::render::{Options, Renderer};
use kiln_deck_layout::scene::slide_delta;
use kiln_raster::{PaintOptions, View, render_delta};
use kiln_scene::Palette;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn open(name: &str) -> Presentation {
    let bytes = std::fs::read(root().join("fixtures/pptx/features").join(name)).expect("fixture");
    kiln_io_pptx::read_pptx(&bytes).expect("read").presentation
}

fn renderer() -> Renderer {
    Renderer::new(
        kiln_deck_layout::fonts::default_fonts(),
        kiln_deck_layout::fonts::default_fonts(),
    )
}

fn check(pres: &Presentation, index: usize, golden: &str, opts: &Options) {
    let mut r = renderer();
    let id = pres.slides[index].id;
    let delta = slide_delta(&mut r, pres, id, 1.0, opts);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (w, h) = (delta.extent.w.round() as u32, delta.extent.h.round() as u32);
    let px = render_delta(
        &delta,
        &View {
            width: w,
            height: h,
            scroll_x: 0.0,
            scroll_y: 0.0,
            options: PaintOptions {
                caret_visible: false,
                touch: false,
            },
        },
        &Palette::light().for_app("deck"),
    )
    .expect("render");
    let path = root()
        .join("fixtures/golden/deck")
        .join(format!("{golden}.png"));
    kiln_raster::golden::check(&px, &path).unwrap_or_else(|e| panic!("{golden}: {e}"));
}

const CASES: &[(&str, usize, &str)] = &[
    ("preset-shapes.pptx", 0, "preset-shapes-1"),
    ("preset-shapes.pptx", 1, "preset-shapes-2"),
    ("placeholders.pptx", 0, "placeholders-title"),
    ("placeholders.pptx", 1, "placeholders-title-content"),
    ("placeholders.pptx", 9, "placeholders-vertical-text"),
    ("shape-styles.pptx", 0, "shape-styles"),
    ("tables.pptx", 0, "tables"),
    ("charts.pptx", 0, "charts-column"),
    ("pictures.pptx", 0, "pictures"),
    ("groups.pptx", 0, "groups"),
    ("connectors.pptx", 0, "connectors"),
    ("text-formatting.pptx", 1, "text-bullets"),
    ("theme-colors.pptx", 0, "theme-colors"),
    ("backgrounds.pptx", 1, "background-gradient"),
];

#[test]
fn feature_slides_match_their_golden_renders() {
    assert!(CASES.len() >= 8);
    for (file, index, golden) in CASES {
        let pres = open(file);
        check(&pres, *index, golden, &Options::default());
    }
}

#[test]
fn a_new_kiln_presentation_shows_its_prompts_in_the_editing_view() {
    let pres = Presentation::new();
    check(
        &pres,
        0,
        "new-kiln-title-edit",
        &Options {
            edit: true,
            ..Options::default()
        },
    );
    let _ = Owner::Slide(pres.slides[0].id);
}

#[test]
fn every_preset_geometry_renders() {
    let mut r = renderer();
    let mut pres = Presentation::new();
    let slide = pres.slides[0].id;
    let names: Vec<&str> = kiln_ooxml::drawingml::presets::preset_names().collect();
    assert!(names.len() >= 180);
    let mut shapes = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let id = pres.alloc_id();
        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let (x, y) = ((i % 16) as i64 * 762_000, (i / 16) as i64 * 571_500);
        shapes.push(kiln_deck::Shape::geometry(
            id,
            name,
            kiln_deck::Rect::new(x, y, 685_800, 500_000),
        ));
    }
    let tx = kiln_deck::edit::insert_shapes(&mut pres, slide, shapes, None).expect("insert");
    pres.apply(tx).expect("apply");
    let mut g = kiln_deck_layout::Glyphs::new(1.0);
    let drawn = r.draw(&pres, Owner::Slide(slide), &Options::default(), &mut g);
    // Every preset draws something (the title and subtitle placeholders are empty).
    assert_eq!(drawn.shapes.len(), names.len());
    assert!(drawn.shapes.iter().all(|s| !s.ops.is_empty()));
}
