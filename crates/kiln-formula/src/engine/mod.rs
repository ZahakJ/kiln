// SPDX-License-Identifier: AGPL-3.0-or-later
//! The calculation engine: compiled formulas, the dependency graph (interval-tree range nodes, so
//! `SUM(A:A)` is one node), dirty marking, topological levels evaluated in parallel, cycle
//! detection with optional iterative calculation, lazy arguments, dynamic arrays with spilling and
//! `#SPILL!`, implicit intersection for legacy formulas and volatile functions.

pub(crate) mod builtins;
mod core;
mod eval;
mod explain;
mod expr;
mod graph;
mod host;
mod rect_index;
mod scc;
mod structured;
mod types;

pub use builtins::{collect_numbers, force, number_arg, scalar_arg};
pub use core::{
    CellInput, CellRef, Clock, Engine, EngineError, EngineOptions, GraphStats, IterativeOptions,
    NameInfo, RecalcResult, SheetKey, SheetRef, SpillRange, is_valid_name,
};
pub use eval::{
    CallContext, MAX_EVAL_DEPTH, MAX_LAMBDA_DEPTH, MAX_NAME_DEPTH, arithmetic, broadcast_dims,
    broadcast_pick, lift_binary,
};
pub use explain::{ExplainKind, ExplainStep, MAX_ARRAY_PREVIEW, format_scalar, format_value};
pub use expr::{CompileContext, Coord, Expr, FuncRef, NameScope, RelAddress, SheetSel, compile};
pub use host::{EngineHost, HostReadOnly, MemoryHost, TableDef};
pub use types::{
    Area, Arg, CellKey, EvalValue, Lambda, LazyArg, Reference, RowVisibility, Scope, SheetId,
    cell_key, unpack_key,
};
