// SPDX-License-Identifier: AGPL-3.0-or-later
//! Scene types on the wire: re-exported from `kiln-scene` (PLAN.md §3.6), which owns the display
//! list, the tile model and the delta tracker. Frames (`SceneDelta`) travel as postcard bytes; the
//! TypeScript painter (`packages/surface`) decodes them with the schema-driven decoder of
//! `@kiln/protocol`.

pub use kiln_scene::{
    AtlasFormat, AtlasPageId, AtlasUpdate, Bounds, ColorRole, DrawOp, FillRule, GlyphRun, Gradient,
    GradientKind, GradientStop, HandleSide, ImageId, ImageUpload, LineCap, LineJoin, LineStyle,
    Overlay, Paint, Palette, PathData, SceneDelta, SelectionBlend, Shadow, ShadowLayer,
    ShadowStyle, Stroke, Tile, TileId,
};

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values of the scene types, for the postcard test vectors.

    use super::*;
    use crate::samples::Samples;
    use crate::schema::{ProtocolType, Visitor};

    /// Visits every scene type (called from `schema::visit_all`).
    pub fn visit(v: &mut impl Visitor) {
        fn one<T: ProtocolType>(v: &mut impl Visitor, name: &'static str) {
            v.visit::<T>(name, true);
        }
        one::<Bounds>(v, "Bounds");
        one::<ColorRole>(v, "ColorRole");
        one::<Paint>(v, "Paint");
        one::<PathData>(v, "PathData");
        one::<FillRule>(v, "FillRule");
        one::<LineJoin>(v, "LineJoin");
        one::<LineCap>(v, "LineCap");
        one::<Stroke>(v, "Stroke");
        one::<AtlasPageId>(v, "AtlasPageId");
        one::<ImageId>(v, "ImageId");
        one::<GlyphRun>(v, "GlyphRun");
        one::<GradientStop>(v, "GradientStop");
        one::<GradientKind>(v, "GradientKind");
        one::<Gradient>(v, "Gradient");
        one::<ShadowStyle>(v, "ShadowStyle");
        one::<Shadow>(v, "Shadow");
        one::<LineStyle>(v, "LineStyle");
        one::<HandleSide>(v, "HandleSide");
        one::<DrawOp>(v, "DrawOp");
        one::<TileId>(v, "TileId");
        one::<Tile>(v, "Tile");
        one::<AtlasFormat>(v, "AtlasFormat");
        one::<AtlasUpdate>(v, "AtlasUpdate");
        one::<ImageUpload>(v, "ImageUpload");
        one::<Overlay>(v, "Overlay");
        one::<SceneDelta>(v, "SceneDelta");
        one::<ShadowLayer>(v, "ShadowLayer");
        one::<SelectionBlend>(v, "SelectionBlend");
        one::<Palette>(v, "Palette");
    }

    fn path() -> PathData {
        let mut p = PathData::new();
        p.move_to(0.5, -1.25)
            .line_to(10.0, 0.0)
            .quad_to(12.0, 3.0, 10.0, 6.0)
            .cubic_to(8.0, 7.5, 2.0, 7.5, 0.5, 6.0)
            .close();
        p
    }

    fn bounds() -> Bounds {
        Bounds::new(96.0, 1_200.5, 624.0, 18.75)
    }

    /// One of every op.
    pub fn ops() -> Vec<DrawOp> {
        let ink = Paint::Role(ColorRole::Ink);
        vec![
            DrawOp::FillRect {
                rect: bounds(),
                paint: Paint::Role(ColorRole::Page),
                radius: 0.0,
            },
            DrawOp::StrokeRect {
                rect: bounds(),
                paint: Paint::Rgba(0x3159_C6FF),
                width: 1.5,
                radius: 4.0,
            },
            DrawOp::Path {
                path: path(),
                fill: Some(Paint::Rgba(0xE05A_2780)),
                stroke: Some(Stroke {
                    paint: ink,
                    width: 2.0,
                    join: LineJoin::Round,
                    cap: LineCap::Square,
                    dash: vec![4.0, 2.0],
                }),
                rule: FillRule::EvenOdd,
            },
            DrawOp::Glyphs(GlyphRun {
                page: AtlasPageId(0),
                paint: ink,
                quads: vec![0, 0, 9, 12, 192, -3, 10, 0, 7, 12, 201, 300],
            }),
            DrawOp::OutlineGlyphs {
                path: path(),
                paint: ink,
            },
            DrawOp::Image {
                image: ImageId(3),
                src: Bounds::new(0.0, 0.0, 640.0, 480.0),
                dst: bounds(),
                opacity: 0.5,
            },
            DrawOp::PushClip {
                rect: bounds(),
                path: Some(path()),
            },
            DrawOp::PushClip {
                rect: bounds(),
                path: None,
            },
            DrawOp::PopClip,
            DrawOp::PopClip,
            DrawOp::PushTransform {
                matrix: [1.0, 0.0, 0.0, 1.0, 12.5, -4.0],
            },
            DrawOp::PopTransform,
            DrawOp::Gradient {
                rect: bounds(),
                radius: 2.0,
                gradient: Gradient {
                    kind: GradientKind::Linear {
                        x0: 0.0,
                        y0: 0.0,
                        x1: 100.0,
                        y1: 0.0,
                    },
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            paint: Paint::Rgba(0xFF6A_26FF),
                        },
                        GradientStop {
                            offset: 1.0,
                            paint: Paint::Rgba(0xFFB5_47FF),
                        },
                    ],
                },
            },
            DrawOp::Gradient {
                rect: bounds(),
                radius: 0.0,
                gradient: Gradient {
                    kind: GradientKind::Radial {
                        cx: 5.0,
                        cy: 5.0,
                        r: 40.0,
                    },
                    stops: vec![],
                },
            },
            DrawOp::Shadow(Shadow {
                rect: bounds(),
                radius: 0.0,
                style: ShadowStyle::Page,
            }),
            DrawOp::Shadow(Shadow {
                rect: bounds(),
                radius: 6.0,
                style: ShadowStyle::Drop {
                    dx: 0.0,
                    dy: 2.0,
                    blur: 8.0,
                    spread: -1.0,
                    paint: Paint::Rgba(0x0000_0040),
                },
            }),
            DrawOp::Squiggle {
                x: 100.0,
                y: 220.5,
                width: 42.0,
                paint: Paint::Role(ColorRole::Squiggle),
            },
            DrawOp::Underline {
                x: 100.0,
                y: 222.0,
                width: 30.0,
                thickness: 1.0,
                paint: Paint::Role(ColorRole::Composition),
                style: LineStyle::Dotted,
            },
            DrawOp::Caret {
                rect: Bounds::new(301.0, 200.0, 2.0, 19.0),
                paint: Paint::Role(ColorRole::Caret),
                spark: true,
            },
            DrawOp::SelectionRects {
                rects: vec![bounds(), Bounds::new(96.0, 1_219.0, 300.0, 18.75)],
                paint: Paint::Role(ColorRole::Selection),
            },
            DrawOp::Guide {
                x0: 96.0,
                y0: 0.0,
                x1: 96.0,
                y1: 1_056.0,
                paint: Paint::Role(ColorRole::Guide),
                dashed: true,
            },
            DrawOp::Handle {
                x: 301.0,
                y: 219.0,
                size: 22.0,
                side: HandleSide::End,
                paint: Paint::Role(ColorRole::Caret),
            },
        ]
    }

    fn tile() -> Tile {
        Tile::new(
            TileId {
                layer: 0,
                index: 65,
            },
            Bounds::new(296.0, 1_128.0, 864.0, 1_160.0),
            ops(),
        )
    }

    fn delta() -> SceneDelta {
        SceneDelta {
            revision: 1 << 40,
            reset: true,
            scale: 1.25,
            extent: Bounds::new(0.0, 0.0, 1_456.0, 55_000.0),
            background: Paint::Role(ColorRole::Canvas),
            tiles: vec![tile()],
            removed: vec![
                TileId { layer: 0, index: 3 },
                TileId {
                    layer: 1,
                    index: 70_000,
                },
            ],
            atlas: vec![AtlasUpdate {
                page: AtlasPageId(1),
                format: AtlasFormat::A8,
                width: 4,
                height: 4,
                y: 1,
                rows: 2,
                data: vec![0, 1, 127, 128, 200, 254, 255, 9],
            }],
            images: vec![ImageUpload {
                id: ImageId(3),
                width: 1,
                height: 1,
                data: vec![255, 106, 38, 255],
            }],
            overlay: Some(Overlay {
                hash: (1 << 53) - 1,
                ops: vec![ops()[18].clone()],
            }),
        }
    }

    impl Samples for Bounds {
        fn samples() -> Vec<Self> {
            vec![Bounds::ZERO, bounds()]
        }
    }
    impl Samples for ColorRole {
        fn samples() -> Vec<Self> {
            vec![ColorRole::Canvas, ColorRole::Guide]
        }
    }
    impl Samples for Paint {
        fn samples() -> Vec<Self> {
            vec![Paint::Rgba(0xFFD9_B8FF), Paint::Role(ColorRole::Ink)]
        }
    }
    impl Samples for PathData {
        fn samples() -> Vec<Self> {
            vec![PathData::new(), path()]
        }
    }
    impl Samples for FillRule {
        fn samples() -> Vec<Self> {
            vec![FillRule::NonZero, FillRule::EvenOdd]
        }
    }
    impl Samples for LineJoin {
        fn samples() -> Vec<Self> {
            vec![LineJoin::Bevel]
        }
    }
    impl Samples for LineCap {
        fn samples() -> Vec<Self> {
            vec![LineCap::Round]
        }
    }
    impl Samples for Stroke {
        fn samples() -> Vec<Self> {
            vec![Stroke::solid(Paint::BLACK, 0.75)]
        }
    }
    impl Samples for AtlasPageId {
        fn samples() -> Vec<Self> {
            vec![AtlasPageId(0), AtlasPageId(u32::MAX)]
        }
    }
    impl Samples for ImageId {
        fn samples() -> Vec<Self> {
            vec![ImageId(7)]
        }
    }
    impl Samples for GlyphRun {
        fn samples() -> Vec<Self> {
            ops()
                .into_iter()
                .filter_map(|o| {
                    if let DrawOp::Glyphs(g) = o {
                        Some(g)
                    } else {
                        None
                    }
                })
                .collect()
        }
    }
    impl Samples for GradientStop {
        fn samples() -> Vec<Self> {
            vec![GradientStop {
                offset: 0.25,
                paint: Paint::Role(ColorRole::Page),
            }]
        }
    }
    impl Samples for GradientKind {
        fn samples() -> Vec<Self> {
            vec![
                GradientKind::Linear {
                    x0: 0.0,
                    y0: 1.0,
                    x1: 2.0,
                    y1: 3.0,
                },
                GradientKind::Radial {
                    cx: 1.0,
                    cy: 2.0,
                    r: 3.5,
                },
            ]
        }
    }
    impl Samples for Gradient {
        fn samples() -> Vec<Self> {
            ops()
                .into_iter()
                .filter_map(|o| {
                    if let DrawOp::Gradient { gradient, .. } = o {
                        Some(gradient)
                    } else {
                        None
                    }
                })
                .collect()
        }
    }
    impl Samples for ShadowStyle {
        fn samples() -> Vec<Self> {
            vec![
                ShadowStyle::Page,
                ShadowStyle::Drop {
                    dx: 1.0,
                    dy: 2.0,
                    blur: 3.0,
                    spread: -4.0,
                    paint: Paint::BLACK,
                },
            ]
        }
    }
    impl Samples for Shadow {
        fn samples() -> Vec<Self> {
            vec![Shadow {
                rect: bounds(),
                radius: 0.0,
                style: ShadowStyle::Page,
            }]
        }
    }
    impl Samples for LineStyle {
        fn samples() -> Vec<Self> {
            vec![LineStyle::Solid, LineStyle::Dashed]
        }
    }
    impl Samples for HandleSide {
        fn samples() -> Vec<Self> {
            vec![HandleSide::Start, HandleSide::Caret]
        }
    }
    impl Samples for DrawOp {
        fn samples() -> Vec<Self> {
            ops()
        }
    }
    impl Samples for TileId {
        fn samples() -> Vec<Self> {
            vec![
                TileId::content(0),
                TileId {
                    layer: 1,
                    index: u32::MAX,
                },
            ]
        }
    }
    impl Samples for Tile {
        fn samples() -> Vec<Self> {
            vec![tile()]
        }
    }
    impl Samples for AtlasFormat {
        fn samples() -> Vec<Self> {
            vec![AtlasFormat::A8, AtlasFormat::Rgba8]
        }
    }
    impl Samples for AtlasUpdate {
        fn samples() -> Vec<Self> {
            delta().atlas
        }
    }
    impl Samples for ImageUpload {
        fn samples() -> Vec<Self> {
            delta().images
        }
    }
    impl Samples for Overlay {
        fn samples() -> Vec<Self> {
            vec![Overlay::default(), delta().overlay.unwrap_or_default()]
        }
    }
    impl Samples for SceneDelta {
        fn samples() -> Vec<Self> {
            vec![SceneDelta::default(), delta()]
        }
    }
    impl Samples for ShadowLayer {
        fn samples() -> Vec<Self> {
            Palette::dark().page_shadow
        }
    }
    impl Samples for SelectionBlend {
        fn samples() -> Vec<Self> {
            vec![SelectionBlend::Darken, SelectionBlend::Lighten]
        }
    }
    impl Samples for Palette {
        fn samples() -> Vec<Self> {
            vec![Palette::light(), Palette::dark()]
        }
    }

    /// A small frame for `Event::Frame` samples.
    pub fn small_delta() -> SceneDelta {
        SceneDelta {
            revision: 42,
            scale: 2.0,
            ..SceneDelta::default()
        }
    }
}
