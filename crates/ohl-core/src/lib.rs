// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
