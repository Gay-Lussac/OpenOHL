// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Python bindings for OLS CAD (`import ols`).
//!
//! The Python tier is for scripts, reports, importers/exporters and
//! automation (ADR-0003). Calculations run in Rust; Python only drives them.

// PyO3's generated glue code uses `unsafe` internally.
#![allow(unsafe_code)]

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Converts text such as "1000 ft" of the named quantity kind to the unit `to`.
///
/// >>> ols.convert("length", "1000 ft", "m")
/// 304.8
#[pyfunction]
fn convert(quantity: &str, value: &str, to: &str) -> PyResult<f64> {
    ols_core::units::convert(quantity, value, to).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Names of the quantity kinds accepted by `convert`.
#[pyfunction]
fn quantity_kinds() -> Vec<&'static str> {
    ols_core::units::QUANTITY_KINDS.to_vec()
}

/// OLS CAD — Open Line System Computer Assisted Design.
#[pymodule]
fn ols(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(convert, m)?)?;
    m.add_function(wrap_pyfunction!(quantity_kinds, m)?)?;
    Ok(())
}
