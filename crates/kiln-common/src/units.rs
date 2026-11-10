// SPDX-License-Identifier: AGPL-3.0-or-later
//! Length units used by Office files, layout and the screen.
//!
//! | Unit | Per inch | Representation | Used by |
//! |---|---|---|---|
//! | [`Emu`] (English Metric Unit) | 914,400 | `i64` | DrawingML, slide geometry |
//! | [`Twip`] (twentieth of a point) | 1,440 | `i32` | WordprocessingML page and paragraph metrics |
//! | [`Pt`] (point) | 72 | `f64` | font sizes, layout |
//! | [`Px`] (CSS pixel at zoom 1) | 96 | `f64` | screen, Scene, hit testing |
//!
//! Conversions into an integer unit round half away from zero and saturate at the integer range
//! (non-finite input becomes 0), so no input can panic.

use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// EMU per inch.
pub const EMU_PER_INCH: i64 = 914_400;
/// Twips per inch.
pub const TWIP_PER_INCH: i32 = 1_440;
/// Points per inch.
pub const PT_PER_INCH: f64 = 72.0;
/// CSS pixels per inch.
pub const PX_PER_INCH: f64 = 96.0;
/// EMU per point (12,700).
pub const EMU_PER_PT: i64 = 12_700;
/// EMU per CSS pixel (9,525).
pub const EMU_PER_PX: i64 = 9_525;
/// EMU per twip (635).
const EMU_PER_TWIP: i64 = 635;

/// English Metric Units (1/914,400 inch).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Emu(#[cfg_attr(feature = "ts", ts(type = "number"))] pub i64);

/// Twentieths of a point (1/1,440 inch).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Twip(pub i32);

/// Typographic points (1/72 inch).
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Pt(pub f64);

/// CSS pixels at zoom 1 (1/96 inch).
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Px(pub f64);

/// Rounds half away from zero into `i64`, saturating; NaN becomes 0.
fn round_i64(v: f64) -> i64 {
    // `as` from float to int saturates and maps NaN to 0 (Rust reference, numeric cast).
    v.round() as i64
}

/// Rounds half away from zero into `i32`, saturating; NaN becomes 0.
fn round_i32(v: f64) -> i32 {
    v.round() as i32
}

impl Emu {
    /// Converts inches to EMU.
    #[must_use]
    pub fn from_inches(inches: f64) -> Self {
        Emu(round_i64(inches * EMU_PER_INCH as f64))
    }
    /// The length in inches.
    #[must_use]
    pub fn to_inches(self) -> f64 {
        self.0 as f64 / EMU_PER_INCH as f64
    }
    /// The length in points (exact up to `f64` precision).
    #[must_use]
    pub fn to_pt(self) -> Pt {
        Pt(self.0 as f64 / EMU_PER_PT as f64)
    }
    /// The length in CSS pixels.
    #[must_use]
    pub fn to_px(self) -> Px {
        Px(self.0 as f64 / EMU_PER_PX as f64)
    }
    /// The length in twips, rounded to the nearest twip.
    #[must_use]
    pub fn to_twip(self) -> Twip {
        let q = self.0 as f64 / EMU_PER_TWIP as f64;
        Twip(round_i32(q))
    }
}

impl Twip {
    /// The length in points (exact).
    #[must_use]
    pub fn to_pt(self) -> Pt {
        Pt(f64::from(self.0) / 20.0)
    }
    /// The length in CSS pixels.
    #[must_use]
    pub fn to_px(self) -> Px {
        Px(f64::from(self.0) / 15.0)
    }
    /// The length in EMU (exact: one twip is 635 EMU).
    #[must_use]
    pub fn to_emu(self) -> Emu {
        Emu(i64::from(self.0) * EMU_PER_TWIP)
    }
    /// The length in inches.
    #[must_use]
    pub fn to_inches(self) -> f64 {
        f64::from(self.0) / f64::from(TWIP_PER_INCH)
    }
}

impl Pt {
    /// The length in CSS pixels (1 pt = 4/3 px).
    #[must_use]
    pub fn to_px(self) -> Px {
        Px(self.0 * PX_PER_INCH / PT_PER_INCH)
    }
    /// The length in EMU, rounded.
    #[must_use]
    pub fn to_emu(self) -> Emu {
        Emu(round_i64(self.0 * EMU_PER_PT as f64))
    }
    /// The length in twips, rounded.
    #[must_use]
    pub fn to_twip(self) -> Twip {
        Twip(round_i32(self.0 * 20.0))
    }
    /// The length in inches.
    #[must_use]
    pub fn to_inches(self) -> f64 {
        self.0 / PT_PER_INCH
    }
}

impl Px {
    /// The length in points (1 px = 0.75 pt).
    #[must_use]
    pub fn to_pt(self) -> Pt {
        Pt(self.0 * PT_PER_INCH / PX_PER_INCH)
    }
    /// The length in EMU, rounded.
    #[must_use]
    pub fn to_emu(self) -> Emu {
        Emu(round_i64(self.0 * EMU_PER_PX as f64))
    }
    /// The length in twips, rounded.
    #[must_use]
    pub fn to_twip(self) -> Twip {
        Twip(round_i32(self.0 * 15.0))
    }
    /// The length in inches.
    #[must_use]
    pub fn to_inches(self) -> f64 {
        self.0 / PX_PER_INCH
    }
}

// Lossless conversions are `From`; rounding ones are the explicit `to_*` methods above.
impl From<Twip> for Emu {
    fn from(v: Twip) -> Self {
        v.to_emu()
    }
}
impl From<Emu> for Pt {
    fn from(v: Emu) -> Self {
        v.to_pt()
    }
}
impl From<Twip> for Pt {
    fn from(v: Twip) -> Self {
        v.to_pt()
    }
}
impl From<Px> for Pt {
    fn from(v: Px) -> Self {
        v.to_pt()
    }
}
impl From<Pt> for Px {
    fn from(v: Pt) -> Self {
        v.to_px()
    }
}
impl From<Emu> for Px {
    fn from(v: Emu) -> Self {
        v.to_px()
    }
}
impl From<Twip> for Px {
    fn from(v: Twip) -> Self {
        v.to_px()
    }
}

macro_rules! int_unit_ops {
    ($t:ident, $inner:ty, $suffix:literal) => {
        impl Add for $t {
            type Output = $t;
            fn add(self, rhs: $t) -> $t {
                $t(self.0.saturating_add(rhs.0))
            }
        }
        impl Sub for $t {
            type Output = $t;
            fn sub(self, rhs: $t) -> $t {
                $t(self.0.saturating_sub(rhs.0))
            }
        }
        impl AddAssign for $t {
            fn add_assign(&mut self, rhs: $t) {
                *self = *self + rhs;
            }
        }
        impl SubAssign for $t {
            fn sub_assign(&mut self, rhs: $t) {
                *self = *self - rhs;
            }
        }
        impl Neg for $t {
            type Output = $t;
            fn neg(self) -> $t {
                $t(self.0.saturating_neg())
            }
        }
        impl Mul<$inner> for $t {
            type Output = $t;
            fn mul(self, rhs: $inner) -> $t {
                $t(self.0.saturating_mul(rhs))
            }
        }
        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", self.0, $suffix)
            }
        }
    };
}

macro_rules! float_unit_ops {
    ($t:ident, $suffix:literal) => {
        impl Add for $t {
            type Output = $t;
            fn add(self, rhs: $t) -> $t {
                $t(self.0 + rhs.0)
            }
        }
        impl Sub for $t {
            type Output = $t;
            fn sub(self, rhs: $t) -> $t {
                $t(self.0 - rhs.0)
            }
        }
        impl AddAssign for $t {
            fn add_assign(&mut self, rhs: $t) {
                self.0 += rhs.0;
            }
        }
        impl SubAssign for $t {
            fn sub_assign(&mut self, rhs: $t) {
                self.0 -= rhs.0;
            }
        }
        impl Neg for $t {
            type Output = $t;
            fn neg(self) -> $t {
                $t(-self.0)
            }
        }
        impl Mul<f64> for $t {
            type Output = $t;
            fn mul(self, rhs: f64) -> $t {
                $t(self.0 * rhs)
            }
        }
        impl Div<f64> for $t {
            type Output = $t;
            fn div(self, rhs: f64) -> $t {
                $t(self.0 / rhs)
            }
        }
        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", self.0, $suffix)
            }
        }
    };
}

int_unit_ops!(Emu, i64, "emu");
int_unit_ops!(Twip, i32, "tw");
float_unit_ops!(Pt, "pt");
float_unit_ops!(Px, "px");

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn one_inch_in_every_unit() {
        let inch = Emu(EMU_PER_INCH);
        assert_eq!(inch.to_pt(), Pt(72.0));
        assert_eq!(inch.to_px(), Px(96.0));
        assert_eq!(inch.to_twip(), Twip(1440));
        assert_eq!(Twip(1440).to_emu(), inch);
        assert_eq!(Pt(72.0).to_px(), Px(96.0));
        assert_eq!(Px(96.0).to_pt(), Pt(72.0));
        assert_eq!(Emu::from_inches(1.0), inch);
        assert!((inch.to_inches() - 1.0).abs() < f64::EPSILON);
        assert!((Twip(720).to_inches() - 0.5).abs() < f64::EPSILON);
        assert!((Pt(36.0).to_inches() - 0.5).abs() < f64::EPSILON);
        assert!((Px(48.0).to_inches() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn office_reference_values() {
        // A 16:9 PowerPoint slide is 12,192,000 x 6,858,000 EMU = 1280 x 720 px.
        assert_eq!(Emu(12_192_000).to_px(), Px(1280.0));
        assert_eq!(Emu(6_858_000).to_px(), Px(720.0));
        // Word's default 1-inch margins and 11 pt body text.
        assert_eq!(Twip(1440).to_pt(), Pt(72.0));
        assert_eq!(Pt(11.0).to_twip(), Twip(220));
        assert_eq!(Pt(11.0).to_emu(), Emu(139_700));
        assert_eq!(Px(1.0).to_emu(), Emu(9_525));
        assert_eq!(Twip(15).to_px(), Px(1.0));
        assert_eq!(Px(1.0).to_twip(), Twip(15));
    }

    #[test]
    fn conversions_into_integers_round_and_saturate() {
        assert_eq!(Pt(0.025).to_twip(), Twip(1)); // 0.5 twip rounds away from zero
        assert_eq!(Pt(-0.025).to_twip(), Twip(-1));
        assert_eq!(Pt(f64::NAN).to_emu(), Emu(0));
        assert_eq!(Pt(f64::INFINITY).to_emu(), Emu(i64::MAX));
        assert_eq!(Px(f64::NEG_INFINITY).to_twip(), Twip(i32::MIN));
        assert_eq!(Emu(i64::MAX).to_twip(), Twip(i32::MAX));
    }

    #[test]
    fn arithmetic_saturates_for_integers() {
        assert_eq!(Emu(i64::MAX) + Emu(1), Emu(i64::MAX));
        assert_eq!(-Twip(i32::MIN), Twip(i32::MAX));
        assert_eq!(Twip(10) * 3, Twip(30));
        let mut e = Emu(5);
        e += Emu(2);
        e -= Emu(1);
        assert_eq!(e, Emu(6));
        assert_eq!(Emu(5) - Emu(7), Emu(-2));
        let mut p = Pt(1.5) * 2.0 / 3.0;
        p += Pt(1.0);
        p -= Pt(0.5);
        assert_eq!(p, Pt(1.5));
        assert_eq!(-Px(2.0) + Px(3.0) - Px(1.0), Px(0.0));
        assert_eq!(Pt(11.5).to_string(), "11.5pt");
        assert_eq!(Emu(3).to_string(), "3emu");
        assert_eq!(Twip(3).to_string(), "3tw");
        assert_eq!(Px(2.0).to_string(), "2px");
    }

    #[test]
    fn lossless_from_conversions() {
        assert_eq!(Emu::from(Twip(1)), Emu(635));
        assert_eq!(Pt::from(Twip(30)), Pt(1.5));
        assert_eq!(Pt::from(Emu(12_700)), Pt(1.0));
        assert_eq!(Px::from(Pt(3.0)), Px(4.0));
        assert_eq!(Pt::from(Px(4.0)), Pt(3.0));
        assert_eq!(Px::from(Emu(9_525)), Px(1.0));
        assert_eq!(Px::from(Twip(30)), Px(2.0));
    }

    proptest! {
        #[test]
        fn twips_round_trip_through_emu_and_points(t in -10_000_000i32..10_000_000) {
            prop_assert_eq!(Twip(t).to_emu().to_twip(), Twip(t));
            prop_assert_eq!(Twip(t).to_pt().to_twip(), Twip(t));
            prop_assert_eq!(Twip(t).to_px().to_twip(), Twip(t));
        }

        #[test]
        fn emu_round_trip_through_points(e in -1_000_000_000_000i64..1_000_000_000_000) {
            prop_assert_eq!(Emu(e).to_pt().to_emu(), Emu(e));
            prop_assert_eq!(Emu(e).to_px().to_emu(), Emu(e));
        }
    }
}
