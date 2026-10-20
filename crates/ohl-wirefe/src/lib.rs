// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Finite-element analysis of wire systems ("semi finite element", in the
//! spirit of SAPS): exact elastic catenary elements for cable spans, truss
//! elements for insulator strings, nodes in 3D, solved by Newton–Raphson.

pub mod catenary_element;
pub mod model;
pub mod section;

pub use catenary_element::{CableProperties, ElementError, ElementState};
pub use model::{CableElement, FeError, Model, Node, Solution, TrussElement, default_fe_budget};
pub use section::{
    Assembly, CableSpec, FeSection, LoadSite, SectionResult, SectionSpec, SpanState, State, Support,
};
