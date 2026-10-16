// SPDX-License-Identifier: AGPL-3.0-or-later
//! The function registry: how worksheet functions plug into the engine.
//!
//! A [`FunctionSpec`] describes one function — name, arity, volatility, which arguments are passed
//! unevaluated (lazy) and which are lifted over arrays, the implementation and a handle to its
//! documentation. A [`FunctionRegistry`] resolves names to specs; [`FunctionTable`] is the standard
//! implementation. This crate registers only the core set used by its own tests (`SUM`, `IF`,
//! `AND`, `OR`, `NOT`, `TRUE`, `FALSE`; see [`core_functions`]); the full library adds its families
//! with [`FunctionTable::register`].

use std::fmt;

use rustc_hash::FxHashMap;

use crate::engine::{Arg, CallContext, EvalValue};

/// Index of a function in its registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionId(pub u32);

/// A function implementation. It receives the arguments (lazy ones unevaluated) and the call
/// context; it never panics on user input and returns errors as values.
pub type FunctionImpl = fn(&mut [Arg<'_>], &mut dyn CallContext) -> EvalValue;

/// A set of argument positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArgMask {
    bits: u64,
    /// Every position at or after this index is in the set (`u16::MAX` for none).
    from: u16,
}

impl ArgMask {
    /// No positions.
    pub const NONE: ArgMask = ArgMask {
        bits: 0,
        from: u16::MAX,
    };
    /// Every position.
    pub const ALL: ArgMask = ArgMask { bits: 0, from: 0 };

    /// Positions `0..64` given as bits (bit `i` is position `i`).
    pub const fn bits(bits: u64) -> ArgMask {
        ArgMask {
            bits,
            from: u16::MAX,
        }
    }

    /// The listed positions (each < 64).
    pub const fn of(positions: &[u8]) -> ArgMask {
        let mut bits = 0u64;
        let mut i = 0;
        while i < positions.len() {
            bits |= 1u64 << (positions[i] & 63);
            i += 1;
        }
        ArgMask::bits(bits)
    }

    /// Every position from `start` on, plus those already in `self`.
    pub const fn and_from(self, start: u16) -> ArgMask {
        ArgMask {
            bits: self.bits,
            from: start,
        }
    }

    /// True when position `i` is in the set.
    pub const fn contains(self, i: usize) -> bool {
        (i < 64 && (self.bits >> i) & 1 == 1) || i >= self.from as usize
    }

    /// True when no position is in the set.
    pub const fn is_empty(self) -> bool {
        self.bits == 0 && self.from == u16::MAX
    }
}

/// Function categories, as in Excel's Insert Function dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FunctionCategory {
    /// Math & Trig.
    MathTrig,
    /// Statistical.
    Statistical,
    /// Logical.
    Logical,
    /// Text.
    Text,
    /// Date & Time.
    DateTime,
    /// Lookup & Reference.
    Lookup,
    /// Information.
    Information,
    /// Financial.
    Financial,
    /// Engineering.
    Engineering,
    /// Database.
    Database,
    /// Web.
    Web,
    /// Compatibility.
    Compatibility,
    /// Dynamic arrays.
    DynamicArray,
}

/// Documentation of one argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArgDoc {
    /// Name as shown in the signature.
    pub name: &'static str,
    /// What it is.
    pub description: &'static str,
    /// May be omitted.
    pub optional: bool,
    /// May repeat.
    pub repeating: bool,
}

/// Documentation shown by autocompletion, the Insert Function dialog and the Formula Lab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FunctionDocs {
    /// Category.
    pub category: FunctionCategory,
    /// One sentence.
    pub summary: &'static str,
    /// The signature, e.g. `IF(logical_test, value_if_true, [value_if_false])`.
    pub syntax: &'static str,
    /// Arguments.
    pub args: &'static [ArgDoc],
    /// `(formula, displayed result)` pairs that evaluate as stated.
    pub examples: &'static [(&'static str, &'static str)],
}

/// Where a function's documentation lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocsHandle {
    /// Not documented.
    None,
    /// Documentation compiled in next to the implementation.
    Inline(&'static FunctionDocs),
    /// A key into an external documentation table (the function library's docs JSON).
    Key(&'static str),
}

/// Excel's maximum number of arguments; a spec with this `max_args` is variadic.
pub const MAX_ARGS: u8 = 255;

/// One worksheet function.
#[derive(Clone)]
pub struct FunctionSpec {
    /// Upper-case name without file-format prefixes, e.g. `XLOOKUP` or `STDEV.S`.
    pub name: &'static str,
    /// Fewest arguments.
    pub min_args: u8,
    /// Most arguments ([`MAX_ARGS`] for variadic functions).
    pub max_args: u8,
    /// Recalculated on every recalculation (NOW, RAND, OFFSET, INDIRECT…).
    pub volatile: bool,
    /// Arguments passed unevaluated (IF branches, LET, LAMBDA, CHOOSE…).
    pub lazy: ArgMask,
    /// Arguments that take single values: arrays there make the call element-wise (Excel's
    /// array lifting); lifted arguments reach the implementation as scalars.
    pub lift: ArgMask,
    /// The implementation.
    pub implementation: FunctionImpl,
    /// Documentation.
    pub docs: DocsHandle,
}

impl fmt::Debug for FunctionSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FunctionSpec")
            .field("name", &self.name)
            .field("min_args", &self.min_args)
            .field("max_args", &self.max_args)
            .field("volatile", &self.volatile)
            .field("lazy", &self.lazy)
            .field("lift", &self.lift)
            .finish_non_exhaustive()
    }
}

impl FunctionSpec {
    /// A non-volatile function with no lazy or lifted arguments.
    pub const fn new(
        name: &'static str,
        min_args: u8,
        max_args: u8,
        implementation: FunctionImpl,
    ) -> FunctionSpec {
        FunctionSpec {
            name,
            min_args,
            max_args,
            volatile: false,
            lazy: ArgMask::NONE,
            lift: ArgMask::NONE,
            implementation,
            docs: DocsHandle::None,
        }
    }

    /// Words for the argument count, for error messages (`2 to 3 arguments`).
    pub fn describe_arity(&self) -> String {
        let plural = |n: u8| if n == 1 { "" } else { "s" };
        if self.min_args == self.max_args {
            format!("{} argument{}", self.min_args, plural(self.min_args))
        } else if self.max_args == MAX_ARGS {
            format!(
                "at least {} argument{}",
                self.min_args,
                plural(self.min_args)
            )
        } else {
            format!("{} to {} arguments", self.min_args, self.max_args)
        }
    }
}

/// Resolves function names to specs. Implementations must be cheap to query from many threads.
pub trait FunctionRegistry: Send + Sync + fmt::Debug {
    /// The function with this (already normalised, upper-case) name.
    fn lookup(&self, normalized_name: &str) -> Option<FunctionId>;
    /// The spec of a function returned by [`FunctionRegistry::lookup`].
    fn spec(&self, id: FunctionId) -> Option<&FunctionSpec>;
    /// Every function, in no particular order.
    fn specs(&self) -> Vec<&FunctionSpec>;
}

const PREFIXES: [&str; 4] = ["_XLFN._XLWS.", "_XLFN.", "_XLWS.", "_XLETA."];

/// Upper-cases a function name and strips the file-format prefixes Excel writes for newer
/// functions (`_xlfn.XLOOKUP` → `XLOOKUP`).
pub fn normalize_function_name(name: &str) -> String {
    let upper = name.to_uppercase();
    for p in PREFIXES {
        if let Some(rest) = upper.strip_prefix(p) {
            return rest.to_owned();
        }
    }
    upper
}

/// The standard registry: a table of specs indexed by name. Registering a name again replaces
/// the earlier spec (so a library can override the core set).
#[derive(Debug, Clone, Default)]
pub struct FunctionTable {
    specs: Vec<Option<FunctionSpec>>,
    by_name: FxHashMap<String, FunctionId>,
}

impl FunctionTable {
    /// An empty table.
    pub fn new() -> FunctionTable {
        FunctionTable::default()
    }

    /// A table with the core functions of this crate.
    pub fn with_core() -> FunctionTable {
        let mut t = FunctionTable::new();
        t.register_all(core_functions());
        t
    }

    /// Adds or replaces a function.
    pub fn register(&mut self, spec: FunctionSpec) -> FunctionId {
        let key = normalize_function_name(spec.name);
        if let Some(&id) = self.by_name.get(&key) {
            self.specs[id.0 as usize] = Some(spec);
            return id;
        }
        let id = FunctionId(self.specs.len() as u32);
        self.specs.push(Some(spec));
        self.by_name.insert(key, id);
        id
    }

    /// Adds or replaces several functions.
    pub fn register_all(&mut self, specs: impl IntoIterator<Item = FunctionSpec>) {
        for s in specs {
            self.register(s);
        }
    }

    /// Removes a function; formulas using it become `#NAME?` when next compiled.
    pub fn unregister(&mut self, name: &str) -> bool {
        match self.by_name.remove(&normalize_function_name(name)) {
            Some(id) => {
                self.specs[id.0 as usize] = None;
                true
            }
            None => false,
        }
    }

    /// Number of registered functions.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// True when nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

impl FunctionRegistry for FunctionTable {
    fn lookup(&self, normalized_name: &str) -> Option<FunctionId> {
        self.by_name.get(normalized_name).copied()
    }

    fn spec(&self, id: FunctionId) -> Option<&FunctionSpec> {
        self.specs.get(id.0 as usize).and_then(Option::as_ref)
    }

    fn specs(&self) -> Vec<&FunctionSpec> {
        self.specs.iter().flatten().collect()
    }
}

/// The core functions: `SUM`, `IF`, `AND`, `OR`, `NOT`, `TRUE`, `FALSE`.
pub fn core_functions() -> Vec<FunctionSpec> {
    crate::engine::builtins::core_specs()
}
