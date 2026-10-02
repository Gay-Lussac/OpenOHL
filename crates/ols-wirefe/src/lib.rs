// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Finite-element analysis of wire systems ("semi finite element", in the
//! spirit of SAPS): exact elastic catenary elements for cable spans, truss
//! elements for insulator strings, nodes in 3D, solved by Newton–Raphson.

pub mod catenary_element;
pub mod model;
pub mod section;

pub use catenary_element::{CableProperties, ElementError, ElementState};
pub use model::{CableElement, FeError, Model, Node, Solution, TrussElement, default_fe_budget};
pub use section::{
    Assembly, CableSpec, FeSection, LoadSite, SectionResult, SectionSpec, State, Support,
};
