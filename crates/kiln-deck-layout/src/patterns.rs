// SPDX-License-Identifier: AGPL-3.0-or-later
//! The 8 × 8 bitmaps of DrawingML's preset pattern fills (`a:pattFill prst`), one bit per pixel
//! as Office and LibreOffice draw them; a set bit is the foreground colour.

use kiln_ooxml::st::PresetPatternVal as P;

/// An 8 × 8 Bayer matrix: the order in which a percentage pattern's pixels turn on.
const BAYER: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

fn percent(p: u32) -> [u8; 8] {
    let on = (p * 64).div_ceil(100);
    let mut out = [0u8; 8];
    for (y, row) in BAYER.iter().enumerate() {
        for (x, &t) in row.iter().enumerate() {
            if u32::from(t) < on {
                out[y] |= 0x80 >> x;
            }
        }
    }
    out
}

const fn rep4(a: u8, b: u8, c: u8, d: u8) -> [u8; 8] {
    [a, b, c, d, a, b, c, d]
}

/// Mirrors a pattern left to right (downward diagonals become upward ones).
fn mirror(b: [u8; 8]) -> [u8; 8] {
    b.map(u8::reverse_bits)
}

/// Turns a pattern a quarter (horizontal lines become vertical ones).
fn transpose(b: [u8; 8]) -> [u8; 8] {
    let mut out = [0u8; 8];
    for (y, row) in b.iter().enumerate() {
        for (x, col) in out.iter_mut().enumerate() {
            if row & (0x80 >> x) != 0 {
                *col |= 0x80 >> y;
            }
        }
    }
    out
}

/// The bitmap of a preset (row 0 first, the most significant bit leftmost).
#[must_use]
pub fn bits(preset: P) -> [u8; 8] {
    match preset {
        P::Pct5 => percent(5),
        P::Pct10 => percent(10),
        P::Pct20 => percent(20),
        P::Pct25 => percent(25),
        P::Pct30 => percent(30),
        P::Pct40 => percent(40),
        P::Pct50 => percent(50),
        P::Pct60 => percent(60),
        P::Pct70 => percent(70),
        P::Pct75 => percent(75),
        P::Pct80 => percent(80),
        P::Pct90 => percent(90),
        P::Horz => [0xFF, 0, 0, 0, 0, 0, 0, 0],
        P::LtHorz => rep4(0xFF, 0, 0, 0),
        P::NarHorz => rep4(0xFF, 0, 0xFF, 0),
        P::DkHorz => rep4(0xFF, 0xFF, 0, 0),
        P::DashHorz => [0xF0, 0, 0, 0, 0x0F, 0, 0, 0],
        P::Vert => transpose(bits(P::Horz)),
        P::LtVert => transpose(bits(P::LtHorz)),
        P::NarVert => transpose(bits(P::NarHorz)),
        P::DkVert => transpose(bits(P::DkHorz)),
        P::DashVert => transpose(bits(P::DashHorz)),
        P::LtDnDiag => rep4(0x88, 0x44, 0x22, 0x11),
        P::DnDiag => rep4(0xCC, 0x66, 0x33, 0x99),
        P::DkDnDiag => rep4(0xEE, 0x77, 0xBB, 0xDD),
        P::WdDnDiag => [0xE0, 0x70, 0x38, 0x1C, 0x0E, 0x07, 0x83, 0xC1],
        P::DashDnDiag => [0x88, 0x44, 0x22, 0x11, 0, 0, 0, 0],
        P::LtUpDiag => mirror(bits(P::LtDnDiag)),
        P::UpDiag => mirror(bits(P::DnDiag)),
        P::DkUpDiag => mirror(bits(P::DkDnDiag)),
        P::WdUpDiag => mirror(bits(P::WdDnDiag)),
        P::DashUpDiag => mirror(bits(P::DashDnDiag)),
        P::Cross | P::LgGrid => [0xFF, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
        P::SmGrid => rep4(0xFF, 0x88, 0x88, 0x88),
        P::DotGrid => [0xAA, 0, 0x80, 0, 0x80, 0, 0x80, 0],
        P::DiagCross => [0x81, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x81],
        P::SmCheck => rep4(0xCC, 0xCC, 0x33, 0x33),
        P::LgCheck => [0xF0, 0xF0, 0xF0, 0xF0, 0x0F, 0x0F, 0x0F, 0x0F],
        P::SmConfetti => [0x80, 0x10, 0x02, 0x20, 0x01, 0x08, 0x40, 0x04],
        P::LgConfetti => [0x4C, 0x0C, 0xC0, 0xC8, 0x03, 0x23, 0x30, 0x31],
        P::HorzBrick => [0xFF, 0x80, 0x80, 0x80, 0xFF, 0x08, 0x08, 0x08],
        P::DiagBrick => [0x80, 0x40, 0x20, 0x10, 0x18, 0x24, 0x42, 0x81],
        P::SolidDmnd => [0x10, 0x38, 0x7C, 0xFE, 0x7C, 0x38, 0x10, 0x00],
        P::OpenDmnd => [0x80, 0x41, 0x22, 0x14, 0x08, 0x14, 0x22, 0x41],
        P::DotDmnd => [0x80, 0x00, 0x22, 0x00, 0x08, 0x00, 0x22, 0x00],
        P::Plaid => [0xAA, 0x55, 0xAA, 0x55, 0xF0, 0xF0, 0xF0, 0xF0],
        P::Sphere => [0x77, 0x98, 0xF8, 0xF8, 0x77, 0x89, 0x8F, 0x8F],
        P::Weave => [0x88, 0x54, 0x22, 0x45, 0x88, 0x14, 0x22, 0x51],
        P::Divot => [0x00, 0x08, 0x04, 0x08, 0x00, 0x80, 0x40, 0x80],
        P::Shingle => [0x03, 0x84, 0x48, 0x30, 0x0C, 0x02, 0x01, 0x01],
        P::Wave => [0x00, 0x18, 0xA4, 0x03, 0x00, 0x18, 0xA4, 0x03],
        P::Trellis => [0xFF, 0x66, 0xFF, 0x99, 0xFF, 0x66, 0xFF, 0x99],
        P::ZigZag => [0x81, 0x42, 0x24, 0x18, 0x81, 0x42, 0x24, 0x18],
    }
}

/// Side of the tile a pattern is drawn with, in pixels (the bitmap repeated).
pub const TILE: u32 = 64;

/// The straight-alpha RGBA pixels of a `TILE` × `TILE` tile of `preset` in `fg` on `bg`
/// (`0xRRGGBBAA`).
#[must_use]
pub fn tile(preset: P, fg: u32, bg: u32) -> Vec<u8> {
    let b = bits(preset);
    let px = |c: u32| c.to_be_bytes();
    let (f, g) = (px(fg), px(bg));
    let mut out = Vec::with_capacity((TILE * TILE * 4) as usize);
    for y in 0..TILE {
        let row = b[(y % 8) as usize];
        for x in 0..TILE {
            out.extend_from_slice(if row & (0x80 >> (x % 8)) != 0 { &f } else { &g });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn density(b: [u8; 8]) -> u32 {
        b.iter().map(|r| r.count_ones()).sum()
    }

    #[test]
    fn percentages_have_their_density_and_lines_their_period() {
        assert_eq!(density(bits(P::Pct50)), 32);
        assert_eq!(density(bits(P::Pct25)), 16);
        assert_eq!(density(bits(P::Pct5)), 4);
        // Upward diagonals mirror downward ones; vertical lines turn horizontal ones.
        assert_eq!(bits(P::WdUpDiag)[0], 0x07);
        assert_eq!(bits(P::Vert), [0x80; 8]);
        for p in P::ALL {
            let d = density(bits(*p));
            assert!(d > 0 && d < 64, "{p:?}");
        }
        assert_eq!(
            tile(P::Pct50, 0xFF00_00FF, 0x0000_00FF).len(),
            (TILE * TILE * 4) as usize
        );
    }
}
