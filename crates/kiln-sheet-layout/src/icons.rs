// SPDX-License-Identifier: AGPL-3.0-or-later
//! Conditional-formatting icons (Excel's 20 icon sets) drawn as vector shapes in a 16 × 16 box.

use kiln_scene::{DrawOp, FillRule, Paint, PathData, Stroke};
use kiln_sheet::cf::IconVisual;
use kiln_sheet::meta::IconSetName;

use crate::look::rgb;

const RED: u32 = 0xD8_3B01;
const YELLOW: u32 = 0xF2_B800;
const GREEN: u32 = 0x37_A346;
const GRAY: u32 = 0x80_8080;
const BLACK: u32 = 0x40_4040;
const PINK: u32 = 0xE6_8A8A;
const STAR: u32 = 0xF2_B800;
const BLUE: u32 = 0x3E_6FB0;

/// Side of an icon in CSS pixels.
pub const ICON: f32 = 14.0;

fn circle(cx: f32, cy: f32, r: f32) -> PathData {
    // Four cubic arcs.
    let k = 0.552_284_8 * r;
    let mut p = PathData::new();
    p.move_to(cx + r, cy)
        .cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r)
        .cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy)
        .cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r)
        .cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy)
        .close();
    p
}

fn poly(points: &[(f32, f32)]) -> PathData {
    let mut p = PathData::new();
    if let Some(&(x, y)) = points.first() {
        p.move_to(x, y);
        for &(x, y) in &points[1..] {
            p.line_to(x, y);
        }
        p.close();
    }
    p
}

fn fill(path: PathData, color: u32) -> DrawOp {
    DrawOp::Path {
        path,
        fill: Some(rgb(color)),
        stroke: None,
        rule: FillRule::NonZero,
    }
}

fn stroke(path: PathData, color: u32, width: f32) -> DrawOp {
    DrawOp::Path {
        path,
        fill: None,
        stroke: Some(Stroke::solid(rgb(color), width)),
        rule: FillRule::NonZero,
    }
}

/// An arrow pointing `dir` (0 up, 1 up-right, 2 right, 3 down-right, 4 down).
fn arrow(x: f32, y: f32, dir: u8, color: u32) -> Vec<DrawOp> {
    // An up arrow in the 14 px box, rotated by 45° steps around the centre.
    let pts = [
        (7.0, 1.0),
        (13.0, 7.0),
        (9.0, 7.0),
        (9.0, 13.0),
        (5.0, 13.0),
        (5.0, 7.0),
        (1.0, 7.0),
    ];
    let a = f32::from(dir) * std::f32::consts::FRAC_PI_4;
    let (s, c) = a.sin_cos();
    let rot: Vec<(f32, f32)> = pts
        .iter()
        .map(|&(px, py)| {
            let (dx, dy) = (px - 7.0, py - 7.0);
            (x + 7.0 + dx * c - dy * s, y + 7.0 + dx * s + dy * c)
        })
        .collect();
    vec![fill(poly(&rot), color)]
}

/// The ops of one icon with its box's top-left at `(x, y)`.
#[must_use]
pub fn icon_ops(icon: IconVisual, x: f32, y: f32) -> Vec<DrawOp> {
    use IconSetName::*;
    let n = icon.set.len().max(1) as u32;
    // Index 0 is the lowest values' icon; `top` counts from the best one.
    let top = n.saturating_sub(1).saturating_sub(icon.index.min(n - 1));
    let c = |cx: f32, cy: f32, r: f32, color: u32| fill(circle(x + cx, y + cy, r), color);
    match icon.set {
        NoIcons => Vec::new(),
        Arrows3 => {
            let (dir, col) = [(0, GREEN), (2, YELLOW), (4, RED)][top as usize];
            arrow(x, y, dir, col)
        }
        ArrowsGray3 => arrow(x, y, [0, 2, 4][top as usize], GRAY),
        Arrows4 => {
            let (dir, col) = [(0, GREEN), (1, YELLOW), (3, YELLOW), (4, RED)][top as usize];
            arrow(x, y, dir, col)
        }
        ArrowsGray4 => arrow(x, y, [0, 1, 3, 4][top as usize], GRAY),
        Arrows5 => {
            let (dir, col) =
                [(0, GREEN), (1, YELLOW), (2, YELLOW), (3, YELLOW), (4, RED)][top as usize];
            arrow(x, y, dir, col)
        }
        ArrowsGray5 => arrow(x, y, [0, 1, 2, 3, 4][top as usize], GRAY),
        TrafficLights3 | Signs3 => vec![c(7.0, 7.0, 6.0, [GREEN, YELLOW, RED][top as usize])],
        TrafficLightsRimmed3 => vec![
            fill(
                {
                    let mut p = PathData::new();
                    p.move_to(x + 1.0, y + 1.0)
                        .line_to(x + 13.0, y + 1.0)
                        .line_to(x + 13.0, y + 13.0)
                        .line_to(x + 1.0, y + 13.0)
                        .close();
                    p
                },
                BLACK,
            ),
            c(7.0, 7.0, 5.0, [GREEN, YELLOW, RED][top as usize]),
        ],
        TrafficLights4 => vec![c(7.0, 7.0, 6.0, [GREEN, YELLOW, RED, BLACK][top as usize])],
        RedToBlack4 => vec![c(7.0, 7.0, 6.0, [RED, PINK, GRAY, BLACK][top as usize])],
        Flags3 => {
            let col = [GREEN, YELLOW, RED][top as usize];
            vec![
                stroke(poly(&[(x + 3.0, y + 1.0), (x + 3.0, y + 13.0)]), BLACK, 1.5),
                fill(
                    poly(&[(x + 3.5, y + 1.5), (x + 12.5, y + 4.5), (x + 3.5, y + 7.5)]),
                    col,
                ),
            ]
        }
        Symbols3 | SymbolsUncircled3 => {
            let col = [GREEN, YELLOW, RED][top as usize];
            let mut ops = Vec::new();
            let mark_color = if icon.set == Symbols3 {
                ops.push(c(7.0, 7.0, 6.5, col));
                0xFF_FFFF
            } else {
                col
            };
            let mut p = PathData::new();
            match top {
                0 => {
                    p.move_to(x + 3.5, y + 7.5)
                        .line_to(x + 6.0, y + 10.0)
                        .line_to(x + 10.5, y + 4.5);
                }
                1 => {
                    p.move_to(x + 7.0, y + 3.0).line_to(x + 7.0, y + 8.5);
                    p.move_to(x + 7.0, y + 10.0).line_to(x + 7.0, y + 11.0);
                }
                _ => {
                    p.move_to(x + 4.5, y + 4.5).line_to(x + 9.5, y + 9.5);
                    p.move_to(x + 9.5, y + 4.5).line_to(x + 4.5, y + 9.5);
                }
            }
            ops.push(stroke(p, mark_color, 2.0));
            ops
        }
        Stars3 => {
            // Full, half and empty stars.
            let star: Vec<(f32, f32)> = (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { 6.5 } else { 2.8 };
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    (x + 7.0 + r * a.cos(), y + 7.5 + r * a.sin())
                })
                .collect();
            let outline = stroke(poly(&star), STAR, 1.0);
            match top {
                0 => vec![fill(poly(&star), STAR)],
                1 => {
                    let half: Vec<(f32, f32)> =
                        star.iter().map(|&(px, py)| (px.min(x + 7.0), py)).collect();
                    vec![fill(poly(&half), STAR), outline]
                }
                _ => vec![outline],
            }
        }
        Triangles3 => match top {
            0 => vec![fill(
                poly(&[
                    (x + 7.0, y + 2.0),
                    (x + 13.0, y + 11.0),
                    (x + 1.0, y + 11.0),
                ]),
                GREEN,
            )],
            1 => vec![fill(
                poly(&[
                    (x + 2.0, y + 5.5),
                    (x + 12.0, y + 5.5),
                    (x + 12.0, y + 8.5),
                    (x + 2.0, y + 8.5),
                ]),
                YELLOW,
            )],
            _ => vec![fill(
                poly(&[(x + 1.0, y + 3.0), (x + 13.0, y + 3.0), (x + 7.0, y + 12.0)]),
                RED,
            )],
        },
        Rating4 | Rating5 => {
            // Signal bars: `n - top` of them dark.
            let bars = n;
            let lit = n - top;
            (0..bars)
                .map(|i| {
                    let h = 3.0 + i as f32 * (10.0 / bars as f32);
                    let w = 12.0 / bars as f32;
                    let bx = x + 1.0 + i as f32 * w;
                    fill(
                        poly(&[
                            (bx, y + 13.0 - h),
                            (bx + w - 1.0, y + 13.0 - h),
                            (bx + w - 1.0, y + 13.0),
                            (bx, y + 13.0),
                        ]),
                        if i < lit { BLUE } else { 0xC8_D3E3 },
                    )
                })
                .collect()
        }
        Quarters5 => {
            let mut ops = vec![stroke(circle(x + 7.0, y + 7.0, 6.0), BLACK, 1.0)];
            let frac = (4 - top.min(4)) as f32 / 4.0;
            if frac >= 1.0 {
                ops.push(c(7.0, 7.0, 6.0, BLACK));
            } else if frac > 0.0 {
                let mut p = PathData::new();
                let a1 = -std::f32::consts::FRAC_PI_2 + frac * std::f32::consts::TAU;
                p.move_to(x + 7.0, y + 7.0).line_to(x + 7.0, y + 1.0);
                let steps = 16;
                for i in 1..=steps {
                    let a = -std::f32::consts::FRAC_PI_2
                        + (a1 + std::f32::consts::FRAC_PI_2) * i as f32 / steps as f32;
                    p.line_to(x + 7.0 + 6.0 * a.cos(), y + 7.0 + 6.0 * a.sin());
                }
                p.close();
                ops.push(fill(p, BLACK));
            }
            ops
        }
        Boxes5 => {
            let lit = 4 - top.min(4);
            (0..4)
                .map(|i| {
                    let (bx, by) = (
                        x + 1.0 + (i % 2) as f32 * 6.5,
                        y + 1.0 + (i / 2) as f32 * 6.5,
                    );
                    fill(
                        poly(&[
                            (bx, by),
                            (bx + 5.5, by),
                            (bx + 5.5, by + 5.5),
                            (bx, by + 5.5),
                        ]),
                        if i < lit { BLUE } else { 0xC8_D3E3 },
                    )
                })
                .collect()
        }
    }
}

/// A paint for data bars.
#[must_use]
pub fn bar_paint(rgb_color: u32) -> Paint {
    rgb(rgb_color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_set_draws_every_index() {
        use IconSetName::*;
        for set in [
            Arrows3,
            ArrowsGray3,
            Flags3,
            TrafficLights3,
            TrafficLightsRimmed3,
            Signs3,
            Symbols3,
            SymbolsUncircled3,
            Stars3,
            Triangles3,
            Arrows4,
            ArrowsGray4,
            RedToBlack4,
            Rating4,
            TrafficLights4,
            Arrows5,
            ArrowsGray5,
            Rating5,
            Quarters5,
            Boxes5,
        ] {
            for index in 0..set.len() as u32 {
                assert!(
                    !icon_ops(IconVisual { set, index }, 0.0, 0.0).is_empty(),
                    "{set:?} {index}"
                );
            }
        }
        assert!(
            icon_ops(
                IconVisual {
                    set: NoIcons,
                    index: 0
                },
                0.0,
                0.0
            )
            .is_empty()
        );
    }
}
