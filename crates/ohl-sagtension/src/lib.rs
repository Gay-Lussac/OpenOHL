// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Sag-tension calculations for overhead line cables.
//!
//! - [`catenary`]: exact catenary of a single (inclined) span.
//! - [`change_of_state`]: tension at another load and temperature (linear elastic).
//! - [`criteria`]: tension limits and the governing stringing tension.
//! - [`section`]: tension sections — ruling span and exact equal-H solution.
//! - [`loaded`]: spans with additional point and distributed loads (segmented catenary).
//! - [`nonlinear`]: non-linear stress-strain and creep; initial and final conditions.
//! - [`sagging`]: sagging outputs (stopwatch, dynamometer, transit, sag corrections,
//!   clipping offsets) from solved states.

pub mod catenary;
pub mod change_of_state;
pub mod criteria;
pub mod loaded;
pub mod nonlinear;
pub mod sagging;
pub mod section;

pub use catenary::{Catenary, SagTensionError, SpanGeometry, SpanSolution};
pub use change_of_state::{ElasticCable, InstalledSpan, LoadState};
pub use criteria::{Governing, TensionCriterion, TensionLimit, governing_tension};
pub use loaded::{LoadedSolution, LoadedSpan};
pub use section::{SectionSolution, TensionSection, ruling_span};
