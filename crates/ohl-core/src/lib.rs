// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OpenOHL core: data model, units, geometry and plugin registry.
//!
//! See `docs/ARCHITECTURE.md` for how this crate fits into the platform.

pub mod budget;
pub mod env;
pub mod files;
pub mod geometry;
pub mod grid;
pub mod pattern;
pub mod text;
pub mod units;

/// Version of the OpenOHL core.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
