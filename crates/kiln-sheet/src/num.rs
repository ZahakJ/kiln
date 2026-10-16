// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`F64`]: a number that can be hashed and compared for equality, so formats containing sizes,
//! tints and angles can be interned.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// An `f64` with total equality, ordering and hashing (`-0.0` equals `0.0`; NaN equals NaN).
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct F64(pub f64);

impl F64 {
    fn key(self) -> u64 {
        if self.0 == 0.0 {
            0
        } else if self.0.is_nan() {
            u64::MAX
        } else {
            self.0.to_bits()
        }
    }
}

impl PartialEq for F64 {
    fn eq(&self, other: &F64) -> bool {
        self.key() == other.key()
    }
}

impl Eq for F64 {}

impl Hash for F64 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

impl PartialOrd for F64 {
    fn partial_cmp(&self, other: &F64) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for F64 {
    fn cmp(&self, other: &F64) -> Ordering {
        if self == other {
            Ordering::Equal
        } else {
            self.0.total_cmp(&other.0)
        }
    }
}

impl fmt::Debug for F64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl From<f64> for F64 {
    fn from(v: f64) -> F64 {
        F64(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashSet;

    #[test]
    fn zero_signs_and_nan_are_equal() {
        assert_eq!(F64(0.0), F64(-0.0));
        assert_eq!(F64(f64::NAN), F64(f64::NAN));
        let mut s = FxHashSet::default();
        s.insert(F64(0.0));
        assert!(s.contains(&F64(-0.0)));
        assert!(F64(1.0) < F64(2.0));
    }
}
