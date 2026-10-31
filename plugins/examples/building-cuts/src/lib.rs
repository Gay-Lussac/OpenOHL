// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Example OpenOHL field-presets plugin (`field-preset-plugin` world;
//! owner 2026-10-31: "a plugin that can cluster buildings (groups of
//! survey data points) and then preset the calculations of the cuts in
//! the electric and magnetic field to export the 2D cuts at the exact
//! points it's needed"). Built, in active development, not approved yet.
//!
//! From the points the host gathers (the point cloud's building and
//! ground classes, the survey points near the line) and the solved
//! model it finds the buildings near the line ([`cluster`]: a plan
//! occupancy grid, cells closer than a gap joined; surveyed outlines by
//! breakline), their footprints, heights and floors, and gives the cuts
//! across the line at each (the governing cut where the building comes
//! closest to a phase conductor, the cut at its point nearest the axis)
//! and its places of sensitive use (façade corners and midpoints facing
//! the line, a point per floor) ([`analysis`]). Its report ([`report`])
//! turns one field's results into CSV, SVG and GeoJSON files.
//!
//! The logic is plain Rust, tested natively (`cargo test` in this
//! folder); only this file touches the WebAssembly interface.

pub mod analysis;
pub mod cluster;
pub mod geom;
pub mod model;
pub mod params;
pub mod report;

#[cfg(test)]
mod tests;

// The WebAssembly interface: compiled for the plugin only (natively the
// component's export names cannot be linked; the tests need none).
#[cfg(target_arch = "wasm32")]
mod wasm;
