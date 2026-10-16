// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-common
//!
//! Shared foundations for every Kiln crate: stable identifiers ([`DocId`], [`NodeId`]), units
//! ([`Emu`], [`Twip`], [`Pt`], [`Px`]), colours ([`Color`]), geometry re-exported from `kurbo`
//! ([`Point`], [`Rect`], …) and the error type ([`KilnError`]).
//!
//! The `serde` feature adds serialisation for identifiers, units and colours; the `ts` feature adds
//! `ts_rs::TS` so `kiln-protocol` can export them to TypeScript.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod color;
mod error;
mod ids;
mod units;

pub use color::{Color, ParseColorError};
pub use error::{ErrorKind, KilnError, Result};
pub use ids::{DocId, NodeId, NodeIdGen};
pub use units::{
    EMU_PER_INCH, EMU_PER_PT, EMU_PER_PX, Emu, PT_PER_INCH, PX_PER_INCH, Pt, Px, TWIP_PER_INCH,
    Twip,
};

/// Geometry shared by layout, scene, hit testing and the protocol: `kurbo` types in CSS pixels or
/// points depending on the layer that uses them.
pub use kurbo::{Affine, Insets, Point, Rect, Size, Vec2};

/// The suite version, shared by every crate in the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The product name shown in window titles, installers and the About dialog.
pub const PRODUCT_NAME: &str = "Kiln";

/// The application identifier used by Tauri and as the Android `applicationId`.
pub const APP_IDENTIFIER: &str = "io.github.zahakj.kiln";

/// The suite name and version as one line, for example `kiln 0.1.0`.
#[must_use]
pub fn version_line() -> String {
    format!("kiln {VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_names_the_suite_and_version() {
        assert_eq!(
            version_line(),
            format!("kiln {}", env!("CARGO_PKG_VERSION"))
        );
        assert!(!VERSION.is_empty());
        assert_eq!(PRODUCT_NAME, "Kiln");
        assert_eq!(APP_IDENTIFIER, "io.github.zahakj.kiln");
    }

    #[test]
    fn geometry_is_kurbo() {
        let r = Rect::new(0.0, 0.0, 10.0, 20.0);
        assert!(r.contains(Point::new(5.0, 5.0)));
        assert_eq!(r.size(), Size::new(10.0, 20.0));
    }
}
