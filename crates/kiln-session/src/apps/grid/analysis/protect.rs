// SPDX-License-Identifier: AGPL-3.0-or-later
//! Review ▸ Protect Sheet and Protect Workbook, their removal with the password, Home ▸ Format
//! ▸ Lock Cell, and the password hashes SpreadsheetML stores: the salted, iterated SHA-512 of
//! ISO/IEC 29500 (`algorithmName`, `hashValue`, `saltValue`, `spinCount`; [MS-OFFCRYPTO]
//! 2.4.2.4: H₀ = H(salt ‖ password as UTF-16LE), Hₙ = H(Hₙ₋₁ ‖ n as a 32-bit little-endian
//! number)) for new passwords, and the legacy 16-bit hash files may carry.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_analysis::GridSheetPermissions;
use kiln_sheet::Command;
use kiln_sheet::commands::MetaField;
use kiln_sheet::meta::{PasswordHash, SheetProtection, WorkbookProtection};
use kiln_sheet::styles::{ProtectionPatch, StylePatch};
use sha2::{Digest, Sha256, Sha384, Sha512};

use super::super::GridDoc;
use crate::model::Change;

/// Excel's iteration count for new passwords.
pub(crate) const SPIN_COUNT: u32 = 100_000;

/// Excel's message for a wrong password.
pub(crate) const WRONG_PASSWORD: &str = "The password you supplied is not correct. Verify that \
     the CAPS LOCK key is off and be sure to use the correct capitalization.";

fn iterate<D: Digest>(salt: &[u8], password: &str, spin: u32) -> Vec<u8> {
    let mut h = D::new();
    h.update(salt);
    for unit in password.encode_utf16() {
        h.update(unit.to_le_bytes());
    }
    let mut out = h.finalize().to_vec();
    for i in 0..spin {
        let mut h = D::new();
        h.update(&out);
        h.update(i.to_le_bytes());
        out = h.finalize().to_vec();
    }
    out
}

/// The hash of a password with a salt, by algorithm name (`SHA-512`, `SHA-384`, `SHA-256`);
/// `None` for an algorithm Kiln cannot compute.
pub(crate) fn hash_with(
    algorithm: &str,
    salt: &[u8],
    password: &str,
    spin: u32,
) -> Option<Vec<u8>> {
    let a = algorithm.to_ascii_uppercase().replace('-', "");
    Some(match a.as_str() {
        "SHA512" => iterate::<Sha512>(salt, password, spin),
        "SHA384" => iterate::<Sha384>(salt, password, spin),
        "SHA256" => iterate::<Sha256>(salt, password, spin),
        _ => return None,
    })
}

/// A new password's hash: SHA-512 with a random 16-byte salt and 100,000 iterations.
pub(crate) fn new_hash(password: &str) -> Result<PasswordHash> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt)
        .map_err(|e| KilnError::Internal(format!("no random numbers for the salt: {e}")))?;
    let hash = hash_with("SHA-512", &salt, password, SPIN_COUNT)
        .ok_or_else(|| KilnError::Internal("SHA-512 is unavailable".into()))?;
    Ok(PasswordHash::Modern {
        algorithm: "SHA-512".into(),
        hash: B64.encode(hash),
        salt: B64.encode(salt),
        spin_count: SPIN_COUNT,
    })
}

/// Whether a password matches a stored hash.
pub(crate) fn verify(stored: &PasswordHash, password: &str) -> Result<bool> {
    match stored {
        PasswordHash::Legacy(h) => Ok(*h == kiln_sheet::meta::legacy_password_hash(password)),
        PasswordHash::Modern {
            algorithm,
            hash,
            salt,
            spin_count,
        } => {
            let salt = B64.decode(salt.trim()).map_err(|_| {
                KilnError::InvalidInput("the stored password salt is damaged".into())
            })?;
            let want = B64.decode(hash.trim()).map_err(|_| {
                KilnError::InvalidInput("the stored password hash is damaged".into())
            })?;
            let got = hash_with(algorithm, &salt, password, *spin_count).ok_or_else(|| {
                KilnError::Unsupported(format!(
                    "passwords hashed with {algorithm} (Kiln checks SHA-256, SHA-384 and SHA-512)"
                ))
            })?;
            Ok(got == want)
        }
    }
}

fn permissions(p: &GridSheetPermissions) -> SheetProtection {
    SheetProtection {
        password: None,
        select_locked_cells: p.select_locked,
        select_unlocked_cells: p.select_unlocked,
        format_cells: p.format_cells,
        format_columns: p.format_columns,
        format_rows: p.format_rows,
        insert_columns: p.insert_columns,
        insert_rows: p.insert_rows,
        insert_hyperlinks: p.insert_hyperlinks,
        delete_columns: p.delete_columns,
        delete_rows: p.delete_rows,
        sort: p.sort,
        auto_filter: p.auto_filter,
        pivot_tables: p.pivot_tables,
        objects: !p.edit_objects,
        scenarios: !p.edit_scenarios,
    }
}

/// The dialog's view of a protection (Excel's defaults when the sheet is not protected).
pub(crate) fn permissions_out(p: Option<&SheetProtection>) -> GridSheetPermissions {
    let d = SheetProtection::default();
    let p = p.unwrap_or(&d);
    GridSheetPermissions {
        select_locked: p.select_locked_cells,
        select_unlocked: p.select_unlocked_cells,
        format_cells: p.format_cells,
        format_columns: p.format_columns,
        format_rows: p.format_rows,
        insert_columns: p.insert_columns,
        insert_rows: p.insert_rows,
        insert_hyperlinks: p.insert_hyperlinks,
        delete_columns: p.delete_columns,
        delete_rows: p.delete_rows,
        sort: p.sort,
        auto_filter: p.auto_filter,
        pivot_tables: p.pivot_tables,
        edit_objects: !p.objects,
        edit_scenarios: !p.scenarios,
    }
}

impl GridDoc {
    /// Protect Sheet / Unprotect Sheet.
    pub(crate) fn protect_sheet(
        &mut self,
        protect: bool,
        password: &str,
        allow: Option<&GridSheetPermissions>,
    ) -> Result<Change> {
        let sid = self.sid()?;
        let current = self
            .workbook
            .sheet(sid)
            .and_then(|s| s.meta().protection.clone());
        let next = if protect {
            if current.is_some() {
                return Ok(Change::None);
            }
            let mut p = allow.map_or_else(SheetProtection::default, permissions);
            if !password.is_empty() {
                p.password = Some(new_hash(password)?);
            }
            Some(p)
        } else {
            let Some(cur) = current else {
                return Ok(Change::None);
            };
            if let Some(h) = &cur.password
                && !verify(h, password)?
            {
                return Err(KilnError::InvalidInput(WRONG_PASSWORD.into()));
            }
            None
        };
        self.exec(
            "grid.review.set_sheet_protection",
            Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(MetaField::Protection(next)),
            },
        )?;
        Ok(Change::Content)
    }

    /// Protect Workbook (structure) and its removal.
    pub(crate) fn protect_workbook(&mut self, protect: bool, password: &str) -> Result<Change> {
        let current = self.workbook.protection().cloned();
        let next = if protect {
            if current.as_ref().is_some_and(|p| p.lock_structure) {
                return Ok(Change::None);
            }
            Some(WorkbookProtection {
                lock_structure: true,
                lock_windows: false,
                password: if password.is_empty() {
                    None
                } else {
                    Some(new_hash(password)?)
                },
            })
        } else {
            let Some(cur) = current else {
                return Ok(Change::None);
            };
            if let Some(h) = &cur.password
                && !verify(h, password)?
            {
                return Err(KilnError::InvalidInput(WRONG_PASSWORD.into()));
            }
            None
        };
        self.exec(
            "grid.review.set_workbook_protection",
            Command::SetWorkbookProtection { protection: next },
        )?;
        Ok(Change::Content)
    }

    /// Format ▸ Lock Cell: the selection's Locked setting (allowed on an unprotected sheet).
    pub(crate) fn lock_cells(&mut self, locked: bool) -> Result<Change> {
        let sid = self.sid()?;
        let ranges = self.view().sel.ranges.clone();
        self.exec(
            "grid.cells.lock_cell",
            Command::SetStyle {
                sheet: sid,
                ranges,
                patch: Box::new(StylePatch {
                    protection: Some(ProtectionPatch {
                        locked: Some(locked),
                        hidden: None,
                    }),
                    ..StylePatch::default()
                }),
            },
        )?;
        Ok(Change::Content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha512_follows_the_iso_method() {
        // H0 = SHA-512(salt ‖ UTF-16LE password); no iterations: compare with one digest.
        let salt = [1u8, 2, 3, 4];
        let mut h = Sha512::new();
        h.update(salt);
        h.update([b'k', 0, b'i', 0]);
        let want = h.finalize().to_vec();
        assert_eq!(hash_with("SHA-512", &salt, "ki", 0), Some(want.clone()));
        // One iteration appends the counter 0 as four bytes.
        let mut h = Sha512::new();
        h.update(&want);
        h.update([0u8; 4]);
        assert_eq!(
            hash_with("sha-512", &salt, "ki", 1),
            Some(h.finalize().to_vec())
        );
        assert_eq!(hash_with("MD5", &salt, "ki", 1), None);
    }

    #[test]
    fn new_hashes_verify_and_wrong_passwords_do_not() {
        let h = new_hash("Kiln 2026").expect("hash");
        assert!(verify(&h, "Kiln 2026").expect("verify"));
        assert!(!verify(&h, "kiln 2026").expect("verify"));
        let PasswordHash::Modern {
            salt, spin_count, ..
        } = &h
        else {
            panic!("a SHA-512 hash");
        };
        assert_eq!(*spin_count, SPIN_COUNT);
        assert_eq!(B64.decode(salt).map(|s| s.len()).ok(), Some(16));
        let legacy = PasswordHash::Legacy(kiln_sheet::meta::legacy_password_hash("abc"));
        assert!(verify(&legacy, "abc").expect("verify"));
        assert!(!verify(&legacy, "abd").expect("verify"));
    }
}
