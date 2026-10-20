// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Spotting along a route of a line: the [`Problem`] from the route's
//! centreline, a terrain surface, the structure types and a cost file
//! ([`route_problem`]); the result as a new line ([`spotted_line`]).

use std::collections::BTreeSet;

use ohl_bom::CostData;
use ohl_line::route::{Alignment, AlignmentView, Route, point_at};
use ohl_line::{Line, Point, Section, SectionPoint, Stringing, Structure};
use ohl_structures::{InsulatorLibrary, StructureLibrary};
use ohl_terrain::Terrain;

use crate::{Candidate, Problem, RequiredPoint, Spotting};

/// What the user chooses for a spotting run along a route.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteOptions {
    /// Route id.
    pub route: String,
    /// Cable id (wire library).
    pub cable: String,
    /// Horizontal tension of the wire in the design (maximum sag)
    /// condition, N.
    pub horizontal_tension: f64,
    /// Temperature of that condition (still air), °C: the sections of the
    /// spotted line are strung to the tension at it.
    pub temperature: f64,
    /// Smallest vertical ground clearance, m.
    pub clearance: f64,
    /// Distance between candidate stations, m.
    pub step: f64,
    /// Longest span, m (none: the largest allowable wind span of the
    /// candidates, else 1000 m).
    pub max_span: Option<f64>,
    /// Shortest span, m.
    pub min_span: f64,
    /// Station ranges where no structure may stand, m.
    pub forbidden: Vec<[f64; 2]>,
    /// Wires per span priced per metre (none: the fewest attachments of a
    /// candidate type).
    pub conductors: Option<u32>,
}

/// A spotting problem built from a route.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteProblem {
    /// The problem.
    pub problem: Problem,
    /// The route's centreline (stations are along it).
    pub centreline: Vec<Point>,
    /// The alignment spotted along.
    pub route: AlignmentView,
    /// What was assumed or left out.
    pub notes: Vec<String>,
}

fn length2(a: Point, b: Point) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// The ground under a polyline: (station, elevation), every edge of the
/// surface crossed; holes in the surface are bridged in a straight line
/// and counted (the second value).
pub fn ground_profile(centre: &[Point], terrain: &Terrain) -> (Vec<[f64; 2]>, usize) {
    let mut out: Vec<[f64; 2]> = Vec::new();
    let mut gaps = 0;
    let mut start = 0.0;
    for w in centre.windows(2) {
        let len = length2(w[0], w[1]);
        if len == 0.0 {
            continue;
        }
        for run in terrain.profile([w[0][0], w[0][1]], [w[1][0], w[1][1]]) {
            for (k, g) in run.iter().enumerate() {
                let p = [start + g.t * len, g.point[2]];
                match out.last() {
                    Some(q) if p[0] - q[0] < 1e-6 => {}
                    Some(q) => {
                        if k == 0 && p[0] - q[0] > 1e-3 {
                            gaps += 1;
                        }
                        out.push(p);
                    }
                    None => out.push(p),
                }
            }
        }
        start += len;
    }
    (out, gaps)
}

/// Deflection at the inner points of a polyline, degrees (right turns
/// positive).
fn deflections(centre: &[Point]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut start = 0.0;
    for k in 1..centre.len().saturating_sub(1) {
        start += length2(centre[k - 1], centre[k]);
        let (ax, ay) = (
            centre[k][0] - centre[k - 1][0],
            centre[k][1] - centre[k - 1][1],
        );
        let (bx, by) = (
            centre[k + 1][0] - centre[k][0],
            centre[k + 1][1] - centre[k][1],
        );
        let angle = -(ax * by - ay * bx).atan2(ax * bx + ay * by).to_degrees();
        out.push((start, angle));
    }
    out
}

/// The candidates: every type of `types` with a price in `cost`, as it is
/// and with each priced extension.
pub fn candidates(
    types: &StructureLibrary,
    insulators: &InsulatorLibrary,
    cost: &CostData,
    notes: &mut Vec<String>,
) -> Vec<(Candidate, usize)> {
    let mut out = Vec::new();
    for t in types.iter() {
        let extensions =
            std::iter::once(None).chain(t.extensions.iter().map(|e| Some(e.name.as_str())));
        for ext in extensions {
            let Some(price) = cost.structure_total(&t.id, ext) else {
                continue;
            };
            let placed = match t.wire_ends([0.0; 3], 0.0, ext, insulators) {
                Ok(p) if !p.is_empty() => p,
                Ok(_) => {
                    notes.push(format!("{}: no attachments, not used", t.id));
                    continue;
                }
                Err(e) => {
                    notes.push(format!("{e}; not used"));
                    continue;
                }
            };
            let height = placed
                .iter()
                .map(|(_, wire, _)| wire[2])
                .fold(f64::INFINITY, f64::min);
            out.push((
                Candidate {
                    structure_type: t.id.clone(),
                    extension: ext.map(str::to_owned),
                    height,
                    cost: price,
                    dead_end: placed.iter().all(|(_, _, strain)| *strain),
                    capacity: t.capacity.clone(),
                },
                placed.len(),
            ));
        }
    }
    out
}

/// The spotting problem along a route of `line`.
pub fn route_problem(
    line: &Line,
    terrain: &Terrain,
    types: &StructureLibrary,
    insulators: &InsulatorLibrary,
    cables: &ohl_cables::Library,
    cost: &CostData,
    options: &RouteOptions,
) -> Result<RouteProblem, String> {
    let route = line
        .alignment_views()
        .into_iter()
        .find(|r| r.id == options.route)
        .ok_or_else(|| format!("no route \"{}\" in the line", options.route))?;
    let centreline = route.centreline(line);
    if centreline.len() < 2 {
        return Err(format!("route {}: needs two points or more", route.id));
    }
    let total: f64 = centreline.windows(2).map(|w| length2(w[0], w[1])).sum();
    let mut notes = Vec::new();
    let (ground, gaps) = ground_profile(&centreline, terrain);
    if gaps > 0 {
        notes.push(format!(
            "{gaps} hole(s) in the terrain along the route bridged in a straight line"
        ));
    }
    let covered = |s: f64| {
        ground.first().is_some_and(|g| g[0] <= s + 1e-3)
            && ground.last().is_some_and(|g| g[0] >= s - 1e-3)
    };
    if !(covered(0.0) && covered(total)) {
        return Err(format!(
            "route {}: the terrain does not cover it from end to end ({total:.0} m)",
            route.id
        ));
    }
    let cable = cables
        .get(&options.cable)
        .ok_or_else(|| format!("cable {} not in the wire library", options.cable))?;
    let found = candidates(types, insulators, cost, &mut notes);
    if found.is_empty() {
        return Err("no structure type has a price in the cost file".into());
    }
    let conductors = options
        .conductors
        .unwrap_or_else(|| found.iter().map(|(_, n)| *n as u32).min().unwrap_or(1));
    let cable_price = cost.cable(&options.cable).map_or_else(
        || {
            notes.push(format!(
                "cable {}: no price, wire not costed",
                options.cable
            ));
            0.0
        },
        |p| p.price_per_m + p.stringing_per_m.unwrap_or(0.0),
    );
    let max_span = options.max_span.unwrap_or_else(|| {
        let widest = found
            .iter()
            .filter_map(|(c, _)| c.capacity.as_ref()?.spans.first().map(|s| s.wind_span.si()))
            .fold(0.0, f64::max);
        if widest > 0.0 { 2.0 * widest } else { 1000.0 }
    });
    let required = deflections(&centreline)
        .into_iter()
        .filter(|(_, a)| a.abs() > 1e-3)
        .map(|(station, angle_deg)| RequiredPoint { station, angle_deg })
        .collect();
    Ok(RouteProblem {
        problem: Problem {
            ground,
            required,
            forbidden: options.forbidden.clone(),
            candidates: found.into_iter().map(|(c, _)| c).collect(),
            step: options.step,
            max_span,
            min_span: options.min_span,
            clearance: options.clearance,
            horizontal_tension: options.horizontal_tension,
            weight: cable.weight().si(),
            span_cost: cost.labour.per_span.unwrap_or(0.0),
            wire_cost_per_m: cable_price * f64::from(conductors),
        },
        centreline,
        route,
        notes,
    })
}

/// Direction of the centreline at a station (bisector at its points),
/// degrees counter-clockwise from east.
fn heading(centre: &[Point], station: f64) -> f64 {
    let mut start = 0.0;
    let mut dirs = Vec::new();
    for w in centre.windows(2) {
        let len = length2(w[0], w[1]);
        if len == 0.0 {
            continue;
        }
        let d = [(w[1][0] - w[0][0]) / len, (w[1][1] - w[0][1]) / len];
        if station >= start - 1e-6 && station <= start + len + 1e-6 {
            dirs.push(d);
        }
        start += len;
    }
    let (x, y) = dirs
        .iter()
        .fold((0.0, 0.0), |(x, y), d| (x + d[0], y + d[1]));
    y.atan2(x).to_degrees()
}

/// A new line with the spotted structures along the route (`T1`, `T2`, …,
/// typed, local x to the right of the line) and one section per
/// attachment (set, phase) every structure has, strung at the design
/// tension and temperature in still air. The rest of `line` (other routes,
/// its structures, sections and electrical systems) is not carried over;
/// its name and coordinate system are.
pub fn spotted_line(
    line: &Line,
    rp: &RouteProblem,
    result: &Spotting,
    types: &StructureLibrary,
    options: &RouteOptions,
) -> Line {
    let mut out = line.clone();
    out.name = format!("{} (spotted)", line.name);
    out.structures = result
        .structures
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let c = &rp.problem.candidates[p.candidate];
            let xy = point_at(&rp.centreline, p.station, 0.0);
            Structure {
                orientation_deg: heading(&rp.centreline, p.station) - 90.0,
                structure_type: Some(c.structure_type.clone()),
                extension: c.extension.clone(),
                ..Structure::new(format!("T{}", k + 1), [xy[0], xy[1], p.ground])
            }
        })
        .collect();
    // Wire points every chosen type has, by set.
    let mut common: Option<BTreeSet<u32>> = None;
    for s in &out.structures {
        let set: BTreeSet<u32> = s
            .structure_type
            .as_deref()
            .and_then(|t| types.get(t))
            .map(|t| t.sets().into_iter().collect())
            .unwrap_or_default();
        common = Some(match common {
            None => set,
            Some(c) => c.intersection(&set).copied().collect(),
        });
    }
    out.sections = common
        .unwrap_or_default()
        .into_iter()
        .map(|set| Section {
            description: Default::default(),
            id: format!("W{set}"),
            cable: options.cable.clone(),
            circuit_name: String::new(),
            phase_label: String::new(),
            system: None,
            bundle: 1,
            bundle_spacing: None,
            bundle_rotation_deg: None,
            points: out
                .structures
                .iter()
                .map(|s| SectionPoint::new(s.id.clone(), set))
                .collect(),
            stringing: Stringing::Tension {
                horizontal_tension: options.horizontal_tension,
                temperature: options.temperature,
            },
            color: None,
            calculation: Default::default(),
            fields: Default::default(),
            spreads: Default::default(),
            span_points: Default::default(),
        })
        .collect();
    out.jumpers.clear();
    out.systems.clear();
    out.alignment.clear();
    // The alignment spotted along, on its route; the structures stand on
    // it.
    let route = if rp.route.route.is_empty() {
        rp.route.id.clone()
    } else {
        rp.route.route.clone()
    };
    out.routes = vec![
        line.routes
            .iter()
            .find(|x| x.id == route)
            .cloned()
            .unwrap_or_else(|| Route::new(&route, &rp.route.name)),
    ];
    out.alignment_points.clear();
    let mut a = Alignment::new(&rp.route.id, &route, Vec::new());
    a.name = rp.route.name.clone();
    out.alignments = vec![a];
    out.set_alignment_points(&rp.route.id, rp.centreline.iter().map(|p| (*p, None)));
    out
}

/// The result as a table: one row per structure and the costs.
pub fn report(rp: &RouteProblem, result: &Spotting, currency: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "  {:>4}  {:>9}  {:<12} {:<5} {:>7}  {:>8}  {:>9}  {:>11}",
        "#", "station m", "type", "ext", "angle°", "span m", "clear. m", "cost"
    );
    for (k, p) in result.structures.iter().enumerate() {
        let c = &rp.problem.candidates[p.candidate];
        let span = result.spans.get(k);
        let _ = writeln!(
            out,
            "  {:>4}  {:>9.1}  {:<12} {:<5} {:>7.1}  {:>8}  {:>9}  {:>11.2}",
            k + 1,
            p.station,
            c.structure_type,
            c.extension.as_deref().unwrap_or(""),
            p.angle_deg,
            span.map_or(String::new(), |s| format!("{:.1}", s.length)),
            span.map_or(String::new(), |s| format!("{:.2}", s.clearance)),
            c.cost
        );
    }
    let _ = writeln!(
        out,
        "Structures {:.2} + spans {:.2} + wire {:.2} = {:.2} {currency}",
        result.structure_cost, result.span_cost, result.wire_cost, result.total
    );
    let _ = writeln!(
        out,
        "{} structures, {} candidate stations, {} states, {:.2?}",
        result.structures.len(),
        result.stations,
        result.states,
        result.elapsed
    );
    for n in &rp.notes {
        let _ = writeln!(out, "note: {n}");
    }
    out
}
