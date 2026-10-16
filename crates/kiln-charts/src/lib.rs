// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-charts
//!
//! Charts for every Kiln app (PLAN.md §3.2): the chart model, its layout and rendering into a
//! Scene, reading and writing Office chart parts, and chart editing.
//!
//! - [`model`]: [`ChartSpace`] → [`Chart`] → [`PlotArea`] with chart groups (column/bar, line,
//!   area, pie, doughnut, scatter, bubble, radar, treemap, sunburst; combinations with a
//!   secondary axis), series with references and cached values (or literal data for Write and
//!   Deck), axes, titles, legend, data labels, trendlines and the formatting of every element.
//! - [`read_chart`] / [`write_chart`]: `c:chartSpace` (DrawingML charts) and `cx:chartSpace`
//!   (chartex) parts. Unknown elements are kept in place and written back; unedited formatting
//!   is written back as read.
//! - [`data`]: resolving series references against a `kiln-sheet` workbook (Grid sheets, or the
//!   embedded workbook of a Write or Deck chart read by the host through `kiln-io-xlsx`).
//! - [`layout`] and [`render`]: plot area, axis scales with "nice" ticks, label collision
//!   avoidance, legend wrapping; drawing into Scene ops with labels shaped by `kiln-text`.
//! - [`edit`]: the chart commands (insert from a selection with Recommended Charts, change type,
//!   switch row/column, select data, add or remove elements, quick layouts, styles and colours).
//! - [`facts`]: structural facts for the fidelity scorer.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod chartex;
pub mod data;
pub mod edit;
pub mod facts;
pub mod format;
pub mod layout;
pub mod model;
pub mod package;
pub mod palette;
pub mod read;
pub mod render;
pub mod scale;
pub mod state;
pub mod text;
pub mod write;
mod xmlx;

pub use model::*;
pub use read::read_chart;
pub use write::write_chart;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
