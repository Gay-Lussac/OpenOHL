// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A whole zone check, as the **Vertical clearances** and **Radial
//! clearances** windows and `ohl zones` run it ([`run_zone_check`]): the
//! sections of the rules' voltages (in the circuits chosen, by the method
//! chosen) solved in every wind position, then checked against the ground
//! and the points read around the wires.
//!
//! The points come from the caller ([`PointReader`]): this crate does not
//! read point clouds (`ohl_pointcloud::visit_around` reads a project's
//! cloud or a COPC file).

use std::collections::BTreeSet;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use ohl_core::budget::Budget;
use ohl_line::Line;
use ohl_terrain::Terrain;

use crate::rules::CheckScope;
use crate::zone::{
    WindState, Zone, ZoneReport, ZoneRule, ZoneTargets, check_zones, jumpers_needed,
    sections_needed, solve_states,
};

/// Reads the points around the wires: `(polylines, reach, classes, budget,
/// visit)` — the wires in plan (x, y), the distance from them within which
/// points count (m), the LiDAR classes wanted; calls `visit(x, y, z,
/// class)` for each point.
pub type PointReader<'a> = dyn Fn(
        &[Vec<[f64; 2]>],
        f64,
        &[u8],
        &Budget,
        &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<(), String>
    + 'a;

/// What a zone check found.
#[derive(Debug, Clone)]
pub struct ZoneCheckResult {
    /// The rules checked.
    pub rules: Vec<ZoneRule>,
    /// The wind positions.
    pub states: Vec<WindState>,
    /// The sections solved (ids).
    pub sections: Vec<String>,
    /// What was found.
    pub zone: ZoneReport,
    /// Sections that failed to solve.
    pub failed: Vec<String>,
    /// Time to solve the wind positions.
    pub solving: Duration,
    /// Time to check them.
    pub checking: Duration,
}

impl ZoneCheckResult {
    /// The findings below their requirement.
    pub fn violations(&self) -> usize {
        self.zone.findings.len()
    }
}

/// Runs a zone check: the sections the rules need ([`sections_needed`];
/// of `check`'s circuits only, every one by `check`'s method, and of
/// `sections_scope` only when given) solved in every state of `states`
/// ([`solve_states`]; `done` counts them), then checked ([`check_zones`])
/// against `ground` and the points `points` reads around the wires (within
/// the zone's search distance, else as far as the rules reach). Stops when
/// `budget` runs out, with an error.
#[allow(clippy::too_many_arguments)]
pub fn run_zone_check(
    line: &Line,
    library: &ohl_cables::Library,
    zone: Zone,
    check: &CheckScope,
    rules: Vec<ZoneRule>,
    states: Vec<WindState>,
    points: Option<&PointReader>,
    ground: Option<&Terrain>,
    sections_scope: Option<&BTreeSet<String>>,
    budget: &Budget,
    done: &AtomicUsize,
) -> Result<ZoneCheckResult, String> {
    let mut sections = sections_needed(line, &rules);
    // The circuits chosen only.
    if !check.circuits.is_empty() {
        sections.retain(|id| {
            line.sections
                .iter()
                .find(|s| &s.id == id)
                .is_some_and(|s| check.takes_circuit(&s.circuit_name))
        });
        if sections.is_empty() {
            return Err("no section of the rules' voltages in the circuits chosen".into());
        }
    }
    // Every section by the method chosen.
    let own;
    let line = match check.method {
        None => line,
        Some(m) => {
            let mut l = line.clone();
            for s in &mut l.sections {
                s.calculation = ohl_line::condition::Calculation::of(s.calculation.condition(), m);
            }
            own = l;
            &own
        }
    };
    if let Some(on) = sections_scope {
        sections.retain(|s| on.contains(s));
        if sections.is_empty() {
            return Err("no section of the rules' voltages on the route worked on".into());
        }
    }
    let started = Instant::now();
    let solved = solve_states(
        line,
        library,
        &states,
        &sections,
        jumpers_needed(&rules),
        budget,
        done,
    )?;
    let solving = started.elapsed();
    let started = Instant::now();
    // Points around the wires at every wind position, within reach.
    let reach = match zone.search {
        Some(d) => d,
        None => {
            rules.iter().map(|r| r.distance).fold(0.0, f64::max)
                + line.defaults.clearance_search_margin
                + rules
                    .iter()
                    .filter_map(|r| r.width)
                    .fold(zone.width, f64::max)
                + 1.0
        }
    };
    let polylines: Vec<Vec<[f64; 2]>> = solved
        .shapes
        .iter()
        .flatten()
        .flat_map(|s| &s.spans)
        .map(|s| s.points.iter().map(|p| [p[0], p[1]]).collect())
        .chain(
            solved
                .jumpers
                .iter()
                .flatten()
                .map(|j| j.points.iter().map(|p| [p[0], p[1]]).collect()),
        )
        .collect();
    let mut feed = |classes: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| match points {
        Some(read) => read(&polylines, reach, classes, budget, visit),
        None => Ok(()),
    };
    let report = check_zones(
        line,
        &zone,
        &rules,
        &states,
        &solved.shapes,
        &solved.jumpers,
        ZoneTargets {
            ground,
            points: points.is_some().then_some(&mut feed as _),
        },
        budget,
    )?;
    Ok(ZoneCheckResult {
        rules,
        states,
        sections,
        zone: report,
        failed: solved.failed,
        solving,
        checking: started.elapsed(),
    })
}

/// The nominal voltages of the line's sections, kV (0: earthed), lowest
/// first — those [`crate::zone::from_rule_set`] makes rows of.
pub fn line_voltages(line: &Line) -> Vec<f64> {
    let mut v: Vec<f64> = crate::voltages_of(line)
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.map_or(0.0, |p| p.nominal_kv))
        .collect();
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}
