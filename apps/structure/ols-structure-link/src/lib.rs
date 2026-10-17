// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Where the structure application meets the core: the one crate that
//! depends on both. The application's crates (`ols-structure-loads`, …)
//! take and give plain data; this crate turns the core's results into
//! that data and back.
//!
//! - [`line_values`]: the loads the line puts on a placed structure
//!   (`ols_loads::structure_loads`) as [`LineValues`].
//! - [`fe_loads`]: one case of a [`LoadTable`] as the loads of the
//!   structure's finite-element model (`ols_structures_fe`).
//!
//! Later also the section library (member areas) and saving back into the
//! line.

use ols_line::Line;
use ols_loads::CaseLoads;
use ols_structure_loads::{At, LinePoint, LineQuantity, LineValue, LineValues, LoadTable};
use ols_structures_fe::{Loads, Model, NodalLoad};

/// The values the line gives at placed structure `structure`: per point,
/// its sets and phases (the phase labels of the sections hanging there)
/// and, per load case, the wires' force unfactored and factored
/// (transverse, longitudinal, vertical down). The wind on the structure
/// body (no point) is left out; tensions per span, wind on the wires and
/// ice on their own are not given by the line's structure loads yet (the
/// recipes using them get "no line value").
pub fn line_values(loads: &[CaseLoads], line: &Line, structure: &str) -> LineValues {
    let mut out = LineValues::default();
    for case in loads {
        for l in case
            .loads
            .iter()
            .filter(|l| l.structure == structure && !l.point.is_empty())
        {
            if !out.points.iter().any(|p| p.label == l.point) {
                let mut phases: Vec<String> = Vec::new();
                for s in &l.sections {
                    if let Some(sec) = line.sections.iter().find(|x| &x.id == s)
                        && !sec.phase_label.is_empty()
                        && !phases.contains(&sec.phase_label)
                    {
                        phases.push(sec.phase_label.clone());
                    }
                }
                out.points.push(LinePoint {
                    label: l.point.clone(),
                    sets: l.sets.clone(),
                    phases,
                });
            }
            let quantities = [
                (LineQuantity::Transverse, l.local[0]),
                (LineQuantity::Longitudinal, l.local[1]),
                (LineQuantity::Vertical, l.local[2]),
                (LineQuantity::TransverseFactored, l.factored[0]),
                (LineQuantity::LongitudinalFactored, l.factored[1]),
                (LineQuantity::VerticalFactored, l.factored[2]),
            ];
            for (quantity, value) in quantities {
                out.values.push(LineValue {
                    case: case.case.name.clone(),
                    point: Some(l.point.clone()),
                    set: None,
                    quantity,
                    value,
                });
            }
        }
    }
    out
}

/// Case `case` of `table` as the loads of `model` (structure axes, z up):
/// a point's load at the joint carrying it, a joint's at that joint, a
/// member's shared half to each of its joints; rows without a value left
/// out. `self_weight`: the case's factor on the members' weight.
pub fn fe_loads(
    model: &Model,
    table: &LoadTable,
    case: &str,
    self_weight: f64,
) -> Result<Loads, String> {
    let mut nodal = Vec::new();
    let mut add = |node: usize, f: [f64; 3], share: f64| {
        nodal.push(NodalLoad {
            node,
            force: [f[0] * share, f[1] * share, -f[2] * share],
            moment: [0.0; 3],
        });
    };
    for r in table.case(case).filter(|r| r.missing.is_none()) {
        match r.at {
            At::Point => {
                let n = model.attachment_node(&r.label).ok_or_else(|| {
                    format!("point {}: no joint of the model carries it", r.label)
                })?;
                add(n, r.force, 1.0);
            }
            At::Joint => {
                let n = model
                    .nodes
                    .iter()
                    .position(|n| n.id == r.label)
                    .ok_or_else(|| format!("no joint \"{}\" in the model", r.label))?;
                add(n, r.force, 1.0);
            }
            At::Member => {
                let m = model
                    .members
                    .iter()
                    .find(|m| m.id == r.label)
                    .ok_or_else(|| format!("no member \"{}\" in the model", r.label))?;
                add(m.i, r.force, 0.5);
                add(m.j, r.force, 0.5);
            }
        }
    }
    Ok(Loads { nodal, self_weight })
}
