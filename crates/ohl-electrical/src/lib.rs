// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Electrical calculations of the line (roadmap O18; built, in active
//! development, not approved yet): headless, no GUI, no plugin calls.
//!
//! - [`cross_section`]: the line in the plane across it — every conductor
//!   and earth wire with its place, bundle, wire data and system — at a
//!   station or as a span's mean ([`cross_section::CrossSection`], the
//!   record the electrical calculations share).
//! - [`earth`]: impedance between two wires with earth return (Carson's
//!   series, the complex depth, Carson's first terms) and their potential
//!   coefficient.
//! - [`matrices`]: the primitive matrices, bundles as equivalent wires,
//!   Kron's reduction of the earth wires, symmetrical components.
//! - [`report`]: the results as table rows and CSV.
//! - [`parameters`]: per span and along the line per circuit — phase
//!   matrices, sequence values, coupling between circuits.
//!
//! [`run`] does all of it for a line solved in one weather case.
//!
//! Assumptions (each said in the theory page, *Line parameters*): a
//! homogeneous earth of one resistivity; the earth wires continuously
//! earthed (at zero voltage along the span); a bundle as one equivalent
//! wire at its centre; the wires parallel to the ground at their mean
//! height in each span and the ground level across each wire; air of
//! permittivity \\( \varepsilon_0 \\) and a perfectly conducting earth for
//! the capacitances; each span's horizontal length as its electrical
//! length.

pub mod cross_section;
pub mod earth;
pub mod matrices;
pub mod parameters;
pub mod report;

use ohl_line::Line;
use ohl_line::solve::SectionShape;

pub use cross_section::{CrossSection, CrossWire, Sources};
pub use parameters::{CircuitParameters, LineParameters};

/// The line parameters of `line` solved in one weather case (`shapes`, its
/// sections in that case; `temperature` the case's, for the resistances),
/// by the line's electrical settings ([`Line::electrical`]). `ground`: the
/// terrain's elevation at a plan point, when there is one.
pub fn run(
    line: &Line,
    library: &ohl_cables::Library,
    shapes: &[SectionShape],
    temperature: ohl_core::units::Temperature,
    ground: Option<&dyn Fn(f64, f64) -> Option<f64>>,
) -> LineParameters {
    let src = Sources {
        line,
        library,
        shapes,
        temperature,
        ground,
    };
    let settings = line.electrical;
    let (spans, mut notes) = cross_section::span_cross_sections(&src, settings.mean_height);
    let (rho, assumed) = settings.soil_resistivity();
    if assumed {
        notes.insert(
            0,
            format!(
                "soil resistivity not set: {rho} Ω·m assumed (the IEEE distribution test feeders' value; an assumption — set the line's own)"
            ),
        );
    }
    if !line.sections.iter().any(|s| s.system.is_some()) {
        notes.push(
            "no section carries a system's phase: assign the systems (Line › Electrical systems) to get circuits"
                .into(),
        );
    }
    parameters::line_parameters(spans, settings.earth_return, notes)
}
