// SPDX-License-Identifier: AGPL-3.0-or-later
//! The print preview: sheets of real fixtures as Scenes, rendered with `kiln-raster` (the
//! canvas painter's twin) — the paper is white and inked in both themes, two-up sheets hold two
//! pages, grayscale removes colour. `KILN_PREVIEW_DIR=<dir>` writes the PNGs for a visual check.

use kiln_io_pdf::write::{WritePrint, print_layout};
use kiln_io_pdf::{PageSelection, PdfOptions, preview};
use kiln_raster::{PaintOptions, View, render_delta};
use kiln_scene::Palette;

fn open(name: &str) -> kiln_doc::Document {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/docx/features")
        .join(name);
    kiln_io_docx::read_docx(&std::fs::read(path).unwrap())
        .unwrap()
        .document
}

fn render(delta: &kiln_scene::SceneDelta, palette: &Palette, name: &str) -> kiln_raster::Rgba {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let view = View {
        width: (delta.extent.w * delta.scale).ceil() as u32,
        height: (delta.extent.h * delta.scale).ceil() as u32,
        scroll_x: 0.0,
        scroll_y: 0.0,
        options: PaintOptions::default(),
    };
    let img = render_delta(delta, &view, palette).expect("renders");
    if let Some(dir) = std::env::var_os("KILN_PREVIEW_DIR") {
        let path = std::path::Path::new(&dir).join(format!("{name}.png"));
        std::fs::write(path, img.encode_png().unwrap()).unwrap();
    }
    img
}

/// Share of pixels that are dark ink, and whether any pixel is saturated colour.
fn ink(img: &kiln_raster::Rgba) -> (f64, bool) {
    let mut dark = 0usize;
    let mut colour = false;
    let px = img.data.as_chunks::<4>().0;
    for p in px {
        let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
        if p[3] > 200 && r + g + b < 200 {
            dark += 1;
        }
        if p[3] > 200 && (r - g).abs().max((g - b).abs()).max((r - b).abs()) > 60 {
            colour = true;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let share = dark as f64 / px.len().max(1) as f64;
    (share, colour)
}

#[test]
fn sheets_render_with_ink_on_white_paper_in_both_themes() {
    let doc = open("tables.docx");
    let layout = print_layout(&doc, kiln_doc_layout::default_fonts(), true, None);
    let p = WritePrint::new(&doc, &layout, None);
    let sheet = preview(&p, &PdfOptions::default(), 0, 1.0).unwrap();
    assert_eq!(sheet.sheets, layout.pages().len());
    assert_eq!(sheet.pages, vec![0]);
    assert!(sheet.delta.reset && !sheet.delta.atlas.is_empty());
    for palette in [Palette::light(), Palette::dark()] {
        let img = render(&sheet.delta, &palette, &format!("tables-{}", palette.name));
        // The middle of the sheet is paper: white in both themes.
        let mid = ((img.height / 2 * img.width + 30) * 4) as usize;
        assert_eq!(
            &img.data[mid..mid + 3],
            &[255, 255, 255],
            "{}",
            palette.name
        );
        let (share, colour) = ink(&img);
        assert!(share > 0.003, "{}: ink {share}", palette.name);
        assert!(colour, "the table styles are coloured");
    }
}

#[test]
fn two_up_and_grayscale_sheets() {
    let doc = open("long-text.docx");
    let layout = print_layout(&doc, kiln_doc_layout::default_fonts(), true, None);
    let p = WritePrint::new(&doc, &layout, None);
    let options = PdfOptions {
        per_sheet: 2,
        pages: PageSelection::parse("3-6").unwrap(),
        ..PdfOptions::default()
    };
    let sheet = preview(&p, &options, 1, 1.5).unwrap();
    assert_eq!((sheet.sheets, sheet.sheet), (2, 1));
    assert_eq!(sheet.pages, vec![4, 5]);
    assert!(sheet.width_pt > sheet.height_pt, "two up is landscape");
    render(&sheet.delta, &Palette::light(), "long-text-2up");

    let doc = open("images-anchored.docx");
    let layout = print_layout(&doc, kiln_doc_layout::default_fonts(), true, None);
    let p = WritePrint::new(&doc, &layout, None);
    let colour = preview(&p, &PdfOptions::default(), 0, 1.0).unwrap();
    assert!(ink(&render(&colour.delta, &Palette::light(), "images-colour")).1);
    let gray = PdfOptions {
        grayscale: true,
        ..PdfOptions::default()
    };
    let g = preview(&p, &gray, 0, 1.0).unwrap();
    assert!(!g.delta.images.is_empty());
    let (_, coloured) = ink(&render(&g.delta, &Palette::light(), "images-gray"));
    assert!(!coloured, "grayscale has no colour");
}
