// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sag-tension calculations for overhead line cables.
//!
//! - [`catenary`]: exact catenary of a single (inclined) span.
//! - [`change_of_state`]: tension at another load and temperature (linear elastic).
//! - [`criteria`]: tension limits and the governing stringing tension.
//! - [`section`]: tension sections — ruling span and exact equal-H solution.
//! - [`loaded`]: spans with additional point and distributed loads (segmented catenary).
//! - [`nonlinear`]: non-linear stress-strain and creep; initial and final conditions.

pub mod catenary;
pub mod change_of_state;
pub mod criteria;
pub mod loaded;
pub mod nonlinear;
pub mod section;

pub use catenary::{Catenary, SagTensionError, SpanGeometry, SpanSolution};
pub use change_of_state::{ElasticCable, InstalledSpan, LoadState};
pub use criteria::{Governing, TensionCriterion, TensionLimit, governing_tension};
pub use loaded::{LoadedSolution, LoadedSpan};
pub use section::{SectionSolution, TensionSection, ruling_span};
