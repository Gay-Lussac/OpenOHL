// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A section's attachment points moved, and its stiffness there
//! (calculation levels 3 and 4 of the line solve, plan of 2026-11-15,
//! phase 2): what the coupling of structures and wires (in active
//! development, not built yet) asks a section — "solved with these
//! points moved by these displacements, what are your forces on them and
//! how do they change when the points move?".
//!
//! - **Moved points** ([`super::Coupling::moved`]): each point of a
//!   structure that holds the section (a string's top on the structure, a
//!   wire point fixed to it, a post's top) placed at its position plus a
//!   given displacement (line axes, m), by structure and label — the
//!   labels the section reports its loads with
//!   ([`super::SectionShape::attachment_loads`]). The reference lengths
//!   (the stringing) stay as they are: displacements are measured from
//!   the structure as strung. None given (or all zero): the section is
//!   solved exactly as without, bit for bit.
//! - **Support stiffness** ([`super::Coupling::stiffness`],
//!   [`SupportStiffness`]): the wire model's tangent condensed to those
//!   points ([`ohl_wirefe::Model::support_stiffness`]), the whole
//!   bundle's.

use std::collections::BTreeMap;

use crate::{Point, Section};

use super::{Built, Holder, v};

/// Displacements of structure points (line axes, m), by structure id and
/// point label (as [`super::Support::label`] names them).
pub type PointMoves = BTreeMap<(String, String), Point>;

/// A point of a structure in a section's support stiffness.
#[derive(Debug, Clone, PartialEq)]
pub struct SupportPoint {
    /// The section point (index) it holds.
    pub point: usize,
    /// Its structure.
    pub structure: String,
    /// Its label (as the section's attachment loads name it).
    pub label: String,
}

/// A section's stiffness at the structure points holding it, as solved:
/// moving them by δ (line axes, m; x, y, z per point in the order of
/// `points`) changes the force the section puts on them by −K δ. The
/// whole bundle's (the model is one subconductor's, times the bundle).
///
/// The tangent of the solved model: exact for the elements as solved; it
/// leaves out what the model re-solves around them — the sheaves'
/// sharing of a section in the blocks (each case shares its length out
/// again), the secant on stress-strain curves, the ice and wind per
/// stretched metre's dependence on the stretch, the jumpers' pull (a
/// load of its own pass). The coupling's iteration needs only an
/// approximate stiffness: an error costs iterations, never accuracy.
#[derive(Debug, Clone, PartialEq)]
pub struct SupportStiffness {
    /// The points, in the matrix's order (3 rows each).
    pub points: Vec<SupportPoint>,
    /// K, N/m: 3 × points square, symmetric.
    pub matrix: nalgebra::DMatrix<f64>,
}

/// The holders with their structure points moved by `moves` (none for a
/// holder's points: as they are). Only points on a structure move — not
/// another section's clamp held where given. A zero displacement is not
/// added (the positions stay the same bit for bit).
pub(super) fn move_points(holders: &mut [Holder], section: &Section, moves: &PointMoves) {
    if moves.is_empty() {
        return;
    }
    for (holder, point) in holders.iter_mut().zip(&section.points) {
        for (label, position, other) in &mut holder.fixed {
            if other.is_some() {
                continue;
            }
            if let Some(u) = moves.get(&(point.structure.clone(), label.clone()))
                && *u != [0.0; 3]
            {
                *position += v(*u);
            }
        }
    }
}

/// The support stiffness of a solved section (`built`, `solution`) at
/// the structure points of the holders that report their loads (as
/// [`super::SectionShape::attachment_loads`] does), times `bundle`. Err:
/// why it could not be found (the wire model's error).
pub(super) fn stiffness(
    built: &Built,
    solution: &ohl_wirefe::Solution,
    holders: &[Holder],
    section: &Section,
    bundle: f64,
) -> Result<SupportStiffness, String> {
    let mut points = Vec::new();
    let mut nodes = Vec::new();
    for (k, ((supports, holder), dead)) in built
        .supports
        .iter()
        .zip(holders)
        .zip(&built.dead)
        .enumerate()
    {
        if !holder.reports || *dead {
            continue;
        }
        for (label, node) in supports {
            points.push(SupportPoint {
                point: k,
                structure: section.points[k].structure.clone(),
                label: label.clone(),
            });
            nodes.push(*node);
        }
    }
    let matrix = built
        .model
        .support_stiffness(solution, &nodes)
        .map_err(|e| e.to_string())?
        * bundle;
    Ok(SupportStiffness { points, matrix })
}
