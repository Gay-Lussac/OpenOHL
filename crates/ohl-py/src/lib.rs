// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Python bindings for OpenOHL (`import ohl`; this is its compiled part,
//! `ohl._ohl`, around which `python/ohl` is the package).
//!
//! The Python tier is for scripts, reports, importers/exporters and
//! automation (ADR-0003). Calculations run in Rust; Python only drives them.

// PyO3's generated glue code uses `unsafe` internally.
#![allow(unsafe_code)]

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Converts text such as "1000 ft" of the named quantity kind to the unit `to`.
///
/// >>> ohl.convert("length", "1000 ft", "m")
/// 304.8
#[pyfunction]
fn convert(quantity: &str, value: &str, to: &str) -> PyResult<f64> {
    ohl_core::units::convert(quantity, value, to).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Names of the quantity kinds accepted by `convert`.
#[pyfunction]
fn quantity_kinds() -> Vec<&'static str> {
    ohl_core::units::QUANTITY_KINDS.to_vec()
}

/// OpenOHL — open-source overhead line design (the compiled part of `ohl`).
#[pymodule]
fn _ohl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(convert, m)?)?;
    m.add_function(wrap_pyfunction!(quantity_kinds, m)?)?;
    Ok(())
}
