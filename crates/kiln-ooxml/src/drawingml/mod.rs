// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML (ECMA-376 Part 1 §20–21): colours, fills, lines, effects, text properties, themes
//! and shape geometry, shared by documents, workbooks, presentations and charts.
//!
//! Every `parse_*` function has a `serialize_*` twin, and `parse(serialize(x)) == x` holds for
//! every model value (tested). Children Kiln does not model are kept in `extras`/`other` fields
//! and written back in schema order.

pub mod color;
pub mod effects;
pub mod fill;
pub mod geometry;
pub mod line;
pub mod presets;
pub mod text;
pub mod theme;
