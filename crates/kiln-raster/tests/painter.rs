// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pixel-level tests of every draw op through the compositor.

use kiln_raster::{Compositor, PaintOptions, Rgba, View, render_delta};
use kiln_scene::{
    AtlasFormat, AtlasPageId, AtlasUpdate, Bounds, ColorRole, DrawOp, FillRule, GlyphRun, Gradient,
    GradientKind, GradientStop, HandleSide, ImageId, ImageUpload, Overlay, Paint, Palette,
    PathData, SceneDelta, Shadow, ShadowStyle, Tile, TileId,
};

const RED: Paint = Paint::Rgba(0xFF00_00FF);
const BLUE: Paint = Paint::Rgba(0x0000_FFFF);

fn view(w: u32, h: u32) -> View {
    View {
        width: w,
        height: h,
        scroll_x: 0.0,
        scroll_y: 0.0,
        options: PaintOptions::default(),
    }
}

fn delta(scale: f32, tiles: Vec<Tile>, overlay: Vec<DrawOp>) -> SceneDelta {
    SceneDelta {
        revision: 1,
        reset: true,
        scale,
        extent: Bounds::new(0.0, 0.0, 100.0, 100.0),
        background: Paint::Rgba(0xFFFF_FFFF),
        tiles,
        overlay: Some(Overlay {
            hash: 1,
            ops: overlay,
        }),
        ..SceneDelta::default()
    }
}

fn px(img: &Rgba, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    [
        img.data[i],
        img.data[i + 1],
        img.data[i + 2],
        img.data[i + 3],
    ]
}

fn tile(ops: Vec<DrawOp>) -> Tile {
    Tile::new(TileId::content(0), Bounds::new(10.0, 10.0, 50.0, 50.0), ops)
}

#[test]
fn fills_strokes_and_tile_offsets() {
    let d = delta(
        1.0,
        vec![tile(vec![
            DrawOp::FillRect {
                rect: Bounds::new(0.0, 0.0, 20.0, 20.0),
                paint: RED,
                radius: 0.0,
            },
            DrawOp::StrokeRect {
                rect: Bounds::new(25.5, 25.5, 10.0, 10.0),
                paint: BLUE,
                width: 1.0,
                radius: 0.0,
            },
        ])],
        vec![],
    );
    let img = render_delta(&d, &view(80, 80), &Palette::light()).unwrap();
    assert_eq!(
        px(&img, 5, 5),
        [255, 255, 255, 255],
        "outside the tile: background"
    );
    assert_eq!(
        px(&img, 15, 15),
        [255, 0, 0, 255],
        "tile content is offset by the tile origin"
    );
    assert_eq!(
        px(&img, 35, 40),
        [0, 0, 255, 255],
        "left edge of the stroke"
    );
    assert_eq!(
        px(&img, 40, 40),
        [255, 255, 255, 255],
        "inside the stroked rect"
    );
}

#[test]
fn scale_and_scroll_use_device_pixels() {
    let d = delta(
        2.0,
        vec![tile(vec![DrawOp::FillRect {
            rect: Bounds::new(0.0, 0.0, 5.0, 5.0),
            paint: RED,
            radius: 0.0,
        }])],
        vec![],
    );
    let mut c = Compositor::new();
    c.apply(&d).unwrap();
    let img = Rgba::from_pixmap(&c.render(&view(60, 60), &Palette::light()).unwrap());
    assert_eq!(px(&img, 20, 20), [255, 0, 0, 255]);
    assert_eq!(px(&img, 29, 29), [255, 0, 0, 255]);
    assert_eq!(px(&img, 30, 30), [255, 255, 255, 255]);
    let mut v = view(60, 60);
    v.scroll_x = 5.0;
    v.scroll_y = 5.0;
    let img = Rgba::from_pixmap(&c.render(&v, &Palette::light()).unwrap());
    assert_eq!(px(&img, 10, 10), [255, 0, 0, 255]);
    assert_eq!(px(&img, 20, 20), [255, 255, 255, 255]);
}

#[test]
fn glyph_runs_blit_tinted_coverage_and_colour_glyphs() {
    let mut a8 = vec![0u8; 4 * 4];
    a8[0] = 255;
    a8[1] = 128;
    let mut rgba = vec![0u8; 4 * 4 * 4];
    rgba[..4].copy_from_slice(&[0, 255, 0, 255]);
    let mut d = delta(
        1.0,
        vec![tile(vec![
            DrawOp::Glyphs(GlyphRun {
                page: AtlasPageId(0),
                paint: Paint::Role(ColorRole::Ink),
                quads: vec![0, 0, 2, 1, 3, 4],
            }),
            DrawOp::Glyphs(GlyphRun {
                page: AtlasPageId(1),
                paint: RED,
                quads: vec![0, 0, 1, 1, 8, 4],
            }),
            // Out of the page: ignored, not a panic.
            DrawOp::Glyphs(GlyphRun {
                page: AtlasPageId(0),
                paint: RED,
                quads: vec![3, 3, 9, 9, 0, 0],
            }),
            DrawOp::Glyphs(GlyphRun {
                page: AtlasPageId(7),
                paint: RED,
                quads: vec![0, 0, 1, 1, 0, 0],
            }),
        ])],
        vec![],
    );
    d.atlas = vec![
        AtlasUpdate {
            page: AtlasPageId(0),
            format: AtlasFormat::A8,
            width: 4,
            height: 4,
            y: 0,
            rows: 4,
            data: a8,
        },
        AtlasUpdate {
            page: AtlasPageId(1),
            format: AtlasFormat::Rgba8,
            width: 4,
            height: 4,
            y: 0,
            rows: 4,
            data: rgba,
        },
    ];
    let img = render_delta(&d, &view(40, 40), &Palette::light()).unwrap();
    let ink = Palette::light().ink.to_be_bytes();
    assert_eq!(px(&img, 13, 14), [ink[0], ink[1], ink[2], 255]);
    let half = px(&img, 14, 14);
    assert!(
        half[0] > ink[0] && half[0] < 255,
        "half coverage blends: {half:?}"
    );
    assert_eq!(
        px(&img, 18, 14),
        [0, 255, 0, 255],
        "colour glyphs ignore the paint"
    );
}

#[test]
fn clips_transforms_paths_and_gradients() {
    let mut tri = PathData::new();
    tri.move_to(0.0, 0.0)
        .line_to(20.0, 0.0)
        .line_to(0.0, 20.0)
        .close();
    let mut ring = PathData::rect(Bounds::new(0.0, 0.0, 20.0, 20.0));
    ring.extend(&PathData::rect(Bounds::new(5.0, 5.0, 10.0, 10.0)));
    let d = delta(
        1.0,
        vec![Tile::new(
            TileId::content(0),
            Bounds::new(0.0, 0.0, 100.0, 100.0),
            vec![
                DrawOp::PushClip {
                    rect: Bounds::new(0.0, 0.0, 10.0, 10.0),
                    path: None,
                },
                DrawOp::FillRect {
                    rect: Bounds::new(0.0, 0.0, 30.0, 30.0),
                    paint: RED,
                    radius: 0.0,
                },
                DrawOp::PopClip,
                DrawOp::PushTransform {
                    matrix: [1.0, 0.0, 0.0, 1.0, 40.0, 0.0],
                },
                DrawOp::Path {
                    path: tri,
                    fill: Some(BLUE),
                    stroke: None,
                    rule: FillRule::NonZero,
                },
                DrawOp::PopTransform,
                DrawOp::PushTransform {
                    matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 40.0],
                },
                DrawOp::Path {
                    path: ring,
                    fill: Some(RED),
                    stroke: None,
                    rule: FillRule::EvenOdd,
                },
                DrawOp::PopTransform,
                DrawOp::Gradient {
                    rect: Bounds::new(40.0, 40.0, 40.0, 10.0),
                    radius: 0.0,
                    gradient: Gradient {
                        kind: GradientKind::Linear {
                            x0: 40.0,
                            y0: 0.0,
                            x1: 80.0,
                            y1: 0.0,
                        },
                        stops: vec![
                            GradientStop {
                                offset: 0.0,
                                paint: Paint::Rgba(0x0000_00FF),
                            },
                            GradientStop {
                                offset: 1.0,
                                paint: Paint::Rgba(0xFFFF_FFFF),
                            },
                        ],
                    },
                },
                // Unbalanced pops are ignored.
                DrawOp::PopTransform,
                DrawOp::PopClip,
            ],
        )],
        vec![],
    );
    let img = render_delta(&d, &view(100, 100), &Palette::light()).unwrap();
    assert_eq!(px(&img, 5, 5), [255, 0, 0, 255]);
    assert_eq!(px(&img, 15, 5), [255, 255, 255, 255], "clipped away");
    assert_eq!(px(&img, 42, 2), [0, 0, 255, 255], "translated triangle");
    assert_eq!(
        px(&img, 58, 18),
        [255, 255, 255, 255],
        "outside the triangle"
    );
    assert_eq!(px(&img, 2, 42), [255, 0, 0, 255], "ring");
    assert_eq!(px(&img, 10, 50), [255, 255, 255, 255], "even-odd hole");
    let left = px(&img, 41, 45)[0];
    let right = px(&img, 78, 45)[0];
    assert!(left < 20 && right > 235, "gradient {left} → {right}");
}

#[test]
fn shadows_paint_outside_the_caster_only() {
    let d = delta(
        1.0,
        vec![Tile::new(
            TileId::content(0),
            Bounds::new(0.0, 0.0, 100.0, 100.0),
            vec![DrawOp::Shadow(Shadow {
                rect: Bounds::new(30.0, 30.0, 40.0, 40.0),
                radius: 0.0,
                style: ShadowStyle::Drop {
                    dx: 0.0,
                    dy: 4.0,
                    blur: 8.0,
                    spread: 0.0,
                    paint: Paint::Rgba(0x0000_00FF),
                },
            })],
        )],
        vec![],
    );
    let img = render_delta(&d, &view(100, 100), &Palette::light()).unwrap();
    assert_eq!(
        px(&img, 50, 50),
        [255, 255, 255, 255],
        "the caster covers its own shadow"
    );
    let near = px(&img, 50, 72)[0];
    let far = px(&img, 50, 84)[0];
    assert!(near < 160, "dark right under the caster: {near}");
    assert!(far > near && far < 255, "fades with distance: {far}");
    assert_eq!(px(&img, 50, 99), [255, 255, 255, 255]);
    // The page style follows the theme.
    let page = delta(
        1.0,
        vec![Tile::new(
            TileId::content(0),
            Bounds::new(0.0, 0.0, 100.0, 100.0),
            vec![DrawOp::Shadow(Shadow {
                rect: Bounds::new(20.0, 20.0, 60.0, 40.0),
                radius: 0.0,
                style: ShadowStyle::Page,
            })],
        )],
        vec![],
    );
    let light = render_delta(&page, &view(100, 100), &Palette::light()).unwrap();
    let dark = render_delta(&page, &view(100, 100), &Palette::dark()).unwrap();
    assert_ne!(px(&light, 50, 70), px(&dark, 50, 70));
}

#[test]
fn overlay_selection_caret_spark_and_handles() {
    let overlay = vec![
        DrawOp::SelectionRects {
            rects: vec![Bounds::new(0.0, 0.0, 40.0, 20.0)],
            paint: Paint::Role(ColorRole::Selection),
        },
        DrawOp::Caret {
            rect: Bounds::new(60.0, 20.0, 2.0, 20.0),
            paint: Paint::Role(ColorRole::Caret),
            spark: true,
        },
        DrawOp::Handle {
            x: 30.0,
            y: 60.0,
            size: 16.0,
            side: HandleSide::End,
            paint: Paint::Role(ColorRole::Caret),
        },
        DrawOp::Squiggle {
            x: 0.0,
            y: 50.0,
            width: 20.0,
            paint: Paint::Role(ColorRole::Squiggle),
        },
        DrawOp::Underline {
            x: 0.0,
            y: 90.0,
            width: 30.0,
            thickness: 2.0,
            paint: Paint::Role(ColorRole::Composition),
            style: kiln_scene::LineStyle::Solid,
        },
    ];
    let d = delta(
        1.0,
        vec![Tile::new(
            TileId::content(0),
            Bounds::new(0.0, 0.0, 100.0, 100.0),
            vec![DrawOp::FillRect {
                rect: Bounds::new(0.0, 0.0, 10.0, 10.0),
                paint: Paint::Rgba(0x0000_00FF),
                radius: 0.0,
            }],
        )],
        overlay,
    );
    let pal = Palette::light();
    let mut c = Compositor::new();
    c.apply(&d).unwrap();
    let img = Rgba::from_pixmap(&c.render(&view(100, 100), &pal).unwrap());
    let sel = pal.selection.to_be_bytes();
    assert_eq!(
        px(&img, 20, 10),
        [sel[0], sel[1], sel[2], 255],
        "the wash takes the page"
    );
    assert_eq!(
        px(&img, 5, 5),
        [0, 0, 0, 255],
        "text under the wash keeps its ink"
    );
    let caret = pal.caret.to_be_bytes();
    assert_eq!(px(&img, 60, 30), [caret[0], caret[1], caret[2], 255]);
    let spark = px(&img, 61, 17);
    assert!(
        spark[0] > 200 && spark[2] < 120,
        "spark above the caret: {spark:?}"
    );
    assert_eq!(
        px(&img, 38, 68),
        [255, 255, 255, 255],
        "no handles without touch"
    );
    let comp = pal.composition.to_be_bytes();
    assert_eq!(px(&img, 10, 90), [comp[0], comp[1], comp[2], 255]);
    // Blink off + touch on.
    let mut v = view(100, 100);
    v.options = PaintOptions {
        caret_visible: false,
        touch: true,
    };
    let img = Rgba::from_pixmap(&c.render(&v, &pal).unwrap());
    assert_eq!(px(&img, 60, 30), [255, 255, 255, 255], "caret hidden");
    assert_eq!(
        px(&img, 38, 68),
        [caret[0], caret[1], caret[2], 255],
        "teardrop handle"
    );
    // Dark theme: the wash lightens instead.
    let dark = Palette::dark();
    let img = Rgba::from_pixmap(&c.render(&view(100, 100), &dark).unwrap());
    assert_eq!(
        px(&img, 5, 5)[..3],
        dark.selection.to_be_bytes()[..3],
        "lighten: max(ink, wash)"
    );
    assert_eq!(
        px(&img, 20, 10),
        [255, 255, 255, 255],
        "lighten keeps a lighter page"
    );
}

#[test]
fn images_and_bad_input() {
    let mut d = delta(
        1.0,
        vec![tile(vec![DrawOp::Image {
            image: ImageId(4),
            src: Bounds::new(0.0, 0.0, 2.0, 2.0),
            dst: Bounds::new(0.0, 0.0, 20.0, 20.0),
            opacity: 1.0,
        }])],
        vec![],
    );
    d.images.push(ImageUpload {
        id: ImageId(4),
        width: 2,
        height: 2,
        data: [0, 128, 255, 255].repeat(4),
    });
    let img = render_delta(&d, &view(40, 40), &Palette::light()).unwrap();
    assert_eq!(px(&img, 20, 20), [0, 128, 255, 255]);
    let mut c = Compositor::new();
    let mut bad = d.clone();
    bad.images[0].data.pop();
    assert!(c.apply(&bad).is_err());
    let mut bad = d.clone();
    bad.scale = 0.0;
    assert!(c.apply(&bad).is_err());
    let mut bad = d;
    bad.atlas.push(AtlasUpdate {
        page: AtlasPageId(0),
        format: AtlasFormat::A8,
        width: 4,
        height: 4,
        y: 3,
        rows: 2,
        data: vec![0; 8],
    });
    assert!(c.apply(&bad).is_err());
    assert!(c.render(&view(0, 10), &Palette::light()).is_err());
    assert!(c.tile_ids().is_empty(), "failed applies change nothing");
}

#[test]
fn incremental_deltas_update_and_remove_tiles() {
    let mut c = Compositor::new();
    c.apply(&delta(
        1.0,
        vec![tile(vec![DrawOp::FillRect {
            rect: Bounds::new(0.0, 0.0, 50.0, 50.0),
            paint: RED,
            radius: 0.0,
        }])],
        vec![],
    ))
    .unwrap();
    let pal = Palette::light();
    let img = Rgba::from_pixmap(&c.render(&view(80, 80), &pal).unwrap());
    assert_eq!(px(&img, 30, 30), [255, 0, 0, 255]);
    let mut next = delta(
        1.0,
        vec![tile(vec![DrawOp::FillRect {
            rect: Bounds::new(0.0, 0.0, 50.0, 50.0),
            paint: BLUE,
            radius: 0.0,
        }])],
        vec![],
    );
    next.reset = false;
    next.overlay = None;
    c.apply(&next).unwrap();
    let img = Rgba::from_pixmap(&c.render(&view(80, 80), &pal).unwrap());
    assert_eq!(px(&img, 30, 30), [0, 0, 255, 255]);
    let mut gone = SceneDelta {
        removed: vec![TileId::content(0)],
        scale: 1.0,
        ..SceneDelta::default()
    };
    gone.background = Paint::Rgba(0xFFFF_FFFF);
    c.apply(&gone).unwrap();
    assert!(c.tile_ids().is_empty());
}
