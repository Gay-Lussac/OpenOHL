// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sag-tension calculations for overhead line cables.
//!
//! - [`catenary`]: exact catenary of a single (inclined) span.

pub mod catenary;

pub use catenary::{Catenary, SagTensionError, SpanGeometry, SpanSolution};
