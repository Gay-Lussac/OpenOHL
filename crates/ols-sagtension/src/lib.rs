// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sag-tension calculations for overhead line cables.
//!
//! - [`catenary`]: exact catenary of a single (inclined) span.
//! - [`change_of_state`]: tension at another load and temperature (linear elastic).

pub mod catenary;
pub mod change_of_state;

pub use catenary::{Catenary, SagTensionError, SpanGeometry, SpanSolution};
pub use change_of_state::{ElasticCable, InstalledSpan, LoadState};
