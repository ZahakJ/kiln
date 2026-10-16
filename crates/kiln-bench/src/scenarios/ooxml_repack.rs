// SPDX-License-Identifier: AGPL-3.0-or-later
//! `ooxml-repack-large`: a 1,000,000-cell workbook package (100,000 rows × 10 columns, 200,000 of
//! them formulas; ≈ 40 MB of XML) is opened, every part inflated and the package saved, as a
//! reader that consumes everything and a writer that passes it through would. Each run also
//! checks that the repacked package is identical part by part. The plain open + save (untouched
//! entries copied verbatim) is reported as `repack_ms`.

use std::time::Instant;

use kiln_ooxml::opc::OpcPackage;

use super::RunOptions;
use crate::harness::{Config, Sample, measure};

pub fn run(options: &RunOptions) -> Result<Sample, String> {
    let (rows, config) = if options.quick {
        (2_000, Config::new(0, 2))
    } else {
        (100_000, Config::new(1, 7))
    };
    let bytes = crate::synth::xlsx(rows, 10, 2)?;
    let e = |e: kiln_ooxml::opc::OpcError| e.to_string();
    let mut uncompressed = 0usize;
    let mut sample = measure("ooxml-repack-large", config, || {
        let pkg = OpcPackage::open(&bytes).map_err(e)?;
        let mut total = 0usize;
        let names: Vec<String> = pkg.all_part_names().map(str::to_owned).collect();
        for name in &names {
            total += pkg.part_bytes(name).map_err(e)?.len();
        }
        let saved = pkg.save().map_err(e)?;
        if saved.len() < bytes.len() / 2 {
            return Err("the repacked package lost data".into());
        }
        uncompressed = total;
        Ok(())
    })?;
    // Correctness: parts identical after the repack.
    let before = OpcPackage::open(&bytes).map_err(e)?;
    let after = OpcPackage::open(&before.save().map_err(e)?).map_err(e)?;
    for name in before.all_part_names() {
        if before.part_bytes(name).map_err(e)? != after.part_bytes(name).map_err(e)? {
            return Err(format!("{name} changed in the repack"));
        }
    }
    let t = Instant::now();
    let pkg = OpcPackage::open(&bytes).map_err(e)?;
    let _ = pkg.save().map_err(e)?;
    sample
        .extra
        .insert("repack_ms".into(), t.elapsed().as_secs_f64() * 1000.0);
    #[allow(clippy::cast_precision_loss)]
    {
        sample
            .extra
            .insert("package_mb".into(), bytes.len() as f64 / 1_048_576.0);
        sample
            .extra
            .insert("xml_mb".into(), uncompressed as f64 / 1_048_576.0);
        sample.extra.insert("cells".into(), f64::from(rows) * 10.0);
    }
    Ok(sample)
}
