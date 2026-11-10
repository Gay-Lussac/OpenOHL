// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The profile's side lines: where the two terrain lines beside the axis
//! run (owner, 2026-11-10) — along the corridor's edges, at a fixed
//! distance from the axis, or under the outermost conductors of the
//! lowest arm in still air.
//!
//! # Under the lowest arm
//!
//! On each side of the axis (left and right looking along increasing
//! stations) the line follows one wire per span, chosen where the span
//! starts:
//!
//! * the wires counted are those of the circuits ticked (default: every
//!   circuit with a system, the phase conductors), in the spans along the
//!   route's alignment ([`ohl_line::AlignmentView::along`]);
//! * a wire's side is the sign of the mean offset of its two ends from
//!   the axis (right positive);
//! * of the wires of a side leaving a structure, the **lowest arm**'s are
//!   those whose end at that structure is within [`LEVEL_TOLERANCE`] of
//!   the lowest one's height (the wire's own end, at its string's wire
//!   point): the span's first structure decides, and the wire chosen is
//!   followed to its next structure, so the line is continuous along the
//!   span whatever the other end looks like;
//! * of those, the **outermost** is the one whose two ends are on average
//!   farthest from the axis, a bundle counted to its outer subconductor;
//! * the line is the wire's trace in plan as solved in still air, point
//!   by point (with a bundle: shifted sideways to the outer
//!   subconductor), the ground under it taken from the surface as the
//!   corridor's is (broken where the surface ends);
//! * at a structure the end of one span's trace is joined to the start of
//!   the next by a straight step (in the profile: the ground under that
//!   step); where a side has no counted wire in a span, the line stops
//!   and starts again.

use crate::{Options, Profile, RouteDrawing};
use ohl_line::route::project;
use ohl_line::solve::SectionShape;
use ohl_line::{AlignmentView, Line, Point, Section};
use ohl_terrain::Terrain;

/// How far above the lowest wire end of a side at a structure a wire end
/// may be and still count as on the lowest arm, m.
pub const LEVEL_TOLERANCE: f64 = 0.5;

/// Where the profile's two side terrain lines run.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ProfileLines {
    /// Along the corridor's edges (the default).
    #[default]
    Corridor,
    /// At this distance either side of the axis, m, whatever the
    /// corridor's width.
    Distance(f64),
    /// Under the outermost conductors of the lowest arm, in still air.
    LowestArm,
}

impl ProfileLines {
    /// The template's and the command line's word for it.
    pub fn key(self) -> &'static str {
        match self {
            Self::Corridor => "corridor",
            Self::Distance(_) => "distance",
            Self::LowestArm => "lowest_arm",
        }
    }

    /// From its word and the distance given with it (m); an unknown
    /// word, or `distance` without a positive distance, is refused.
    pub fn parse(word: &str, distance: Option<f64>) -> Result<Self, String> {
        match word {
            "corridor" => Ok(Self::Corridor),
            "lowest_arm" => Ok(Self::LowestArm),
            "distance" => match distance {
                Some(d) if d.is_finite() && d > 0.0 => Ok(Self::Distance(d)),
                Some(d) => Err(format!(
                    "profile lines at {d} m from the axis: give a positive distance"
                )),
                None => Err(
                    "profile lines at a distance from the axis: give the distance (e.g. \"10 m\")"
                        .into(),
                ),
            },
            other => Err(format!(
                "profile lines \"{other}\": give corridor, distance or lowest_arm"
            )),
        }
    }
}

/// One of the profile's side lines when it does not run along a
/// corridor's edge.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SideLine {
    /// Right of the axis looking along increasing stations (else left).
    pub right: bool,
    /// Where it runs in plan: runs of (station, offset) points.
    pub plan: Vec<Vec<[f64; 2]>>,
    /// The ground under it: runs of (station, z) points.
    pub ground: Profile,
}

/// The circuits of `line` the lowest arm may be taken from, in the order
/// of its sections: name ("": wires without a circuit) and whether it
/// counts by default (it carries a system: phase conductors; earth wires
/// and earthed circuits do not).
pub fn circuits(line: &Line) -> Vec<(String, bool)> {
    let potentials = line.potentials().unwrap_or_default();
    let mut out: Vec<(String, bool)> = Vec::new();
    for s in &line.sections {
        let live = is_live(s, &potentials);
        match out.iter_mut().find(|c| c.0 == s.circuit_name) {
            Some(c) => c.1 |= live,
            None => out.push((s.circuit_name.clone(), live)),
        }
    }
    out
}

type Potentials = std::collections::BTreeMap<String, Option<ohl_line::electrical::Potential>>;

fn is_live(s: &Section, potentials: &Potentials) -> bool {
    potentials
        .get(&s.id)
        .map_or(s.system.is_some(), Option::is_some)
}

/// Which of weather cases (`wind[k]`: case `k` has wind) the lowest arm's
/// wires are taken from: the case drawn (`drawn`) when it has no wind,
/// else the first without wind; none: every case has wind. (A wire's
/// trace in plan without wind does not depend on the temperature or the
/// ice but through the strings' swing along the line.)
pub fn still_case(wind: &[bool], drawn: usize) -> Option<usize> {
    if wind.get(drawn) == Some(&false) {
        Some(drawn)
    } else {
        wind.iter().position(|w| !w)
    }
}

/// A wire of a side in one span, start to end along the route.
struct Candidate {
    /// The structure it starts at and that one's station.
    start: String,
    start_station: f64,
    /// The structure it ends at and that one's station.
    end: String,
    end_station: f64,
    /// Its height where it starts, m.
    z: f64,
    /// How far its two ends are from the axis on average, to the outer
    /// subconductor, m.
    outer: f64,
    /// Its trace in plan (the outer subconductor's).
    trace: Vec<[f64; 2]>,
}

/// Half the width of `section`'s bundle across the wire, m.
fn bundle_half_width(section: &Section) -> f64 {
    section
        .bundle_offsets()
        .iter()
        .map(|o| o[0].abs())
        .fold(0.0, f64::max)
}

/// The wire of `section` in the span between structures `a` and `b` with
/// points `points` (first at `a`), as a candidate of its side (the
/// second value: right).
fn candidate(
    centre: &[Point],
    line: &Line,
    section: &Section,
    (a, b): (&str, &str),
    points: &[Point],
) -> Option<(Candidate, bool)> {
    let station = |id: &str| Some(project(centre, line.structure(id)?.position).0);
    let (sa, sb) = (station(a)?, station(b)?);
    let mut trace: Vec<[f64; 2]> = points.iter().map(|p| [p[0], p[1]]).collect();
    let (mut first, mut last) = (*points.first()?, *points.last()?);
    let (mut start, mut end) = ((a, sa), (b, sb));
    if sa > sb {
        trace.reverse();
        std::mem::swap(&mut first, &mut last);
        std::mem::swap(&mut start, &mut end);
    }
    let mean = (project(centre, first).1 + project(centre, last).1) / 2.0;
    let right = mean >= 0.0;
    let half = bundle_half_width(section);
    // The outer subconductor: the trace shifted sideways, away from the
    // axis (right of the wire's direction along the route positive).
    let (dx, dy) = (last[0] - first[0], last[1] - first[1]);
    let len = dx.hypot(dy);
    if half > 0.0 && len > 0.0 {
        let k = if right { half } else { -half } / len;
        for p in &mut trace {
            *p = [p[0] + dy * k, p[1] - dx * k];
        }
    }
    Some((
        Candidate {
            start: start.0.to_owned(),
            start_station: start.1,
            end: end.0.to_owned(),
            end_station: end.1,
            z: first[2],
            outer: mean.abs() + half,
            trace,
        },
        right,
    ))
}

/// The wires counted of `line` along `route`, by side (left, right): as
/// solved in `still`, else (none) straight between their attachment
/// points.
fn candidates(
    line: &Line,
    route: &AlignmentView,
    centre: &[Point],
    still: Option<&[SectionShape]>,
    ticked: Option<&[String]>,
) -> [Vec<Candidate>; 2] {
    let potentials = line.potentials().unwrap_or_default();
    let mut sides = [Vec::new(), Vec::new()];
    for section in &line.sections {
        let counted = match ticked {
            Some(names) => names.contains(&section.circuit_name),
            None => is_live(section, &potentials),
        };
        if !counted {
            continue;
        }
        let shape = still.and_then(|s| s.iter().find(|x| x.section == section.id));
        if still.is_some() && shape.is_none() {
            continue;
        }
        for (k, pair) in section.points.windows(2).enumerate() {
            let (a, b) = (pair[0].structure.as_str(), pair[1].structure.as_str());
            if !route.along(line, a, b) {
                continue;
            }
            let chord: Vec<Point>;
            let points: &[Point] = match shape {
                Some(s) => match s.spans.get(k) {
                    Some(span) => &span.points,
                    None => continue,
                },
                None => {
                    chord = pair
                        .iter()
                        .filter_map(|p| line.wire_point(p).map(|w| w.position))
                        .collect();
                    &chord
                }
            };
            if points.len() < 2 {
                continue;
            }
            if let Some((c, right)) = candidate(centre, line, section, (a, b), points) {
                sides[usize::from(right)].push(c);
            }
        }
    }
    sides
}

/// The wires the line of a side follows, in station order: at the first
/// structure wires leave, the outermost of the lowest arm, followed to
/// its end; then the same among the wires starting from there on.
fn chosen(mut rest: Vec<Candidate>) -> Vec<Candidate> {
    let mut out = Vec::new();
    while let Some(first) = rest
        .iter()
        .min_by(|a, b| a.start_station.total_cmp(&b.start_station))
    {
        let at = first.start.clone();
        let low = rest
            .iter()
            .filter(|c| c.start == at)
            .map(|c| c.z)
            .fold(f64::INFINITY, f64::min);
        let Some(k) = (0..rest.len())
            .filter(|&k| rest[k].start == at && rest[k].z <= low + LEVEL_TOLERANCE)
            .max_by(|&i, &j| rest[i].outer.total_cmp(&rest[j].outer))
        else {
            break;
        };
        let pick = rest.swap_remove(k);
        // What runs beside the wire chosen is left out (1 m: the ends'
        // stations differ by the strings).
        rest.retain(|c| c.start != at && c.start_station >= pick.end_station - 1.0);
        out.push(pick);
    }
    out
}

/// The traces of `wires` (in station order) joined into runs: one span's
/// end to the next one's start at the same structure, a new run where
/// the line stops.
fn joined(wires: Vec<Candidate>) -> Vec<Vec<[f64; 2]>> {
    let mut runs: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut end: Option<String> = None;
    for w in wires {
        match runs.last_mut() {
            Some(run) if end.as_deref() == Some(w.start.as_str()) => run.extend(w.trace),
            _ => runs.push(w.trace),
        }
        end = Some(w.end);
    }
    for run in &mut runs {
        run.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9);
    }
    runs.retain(|r| r.len() > 1);
    runs
}

/// The ground under polyline `run` (project coordinates), each point at
/// its station along `centre` (projected, as the wires' points are):
/// broken where the surface ends, as the corridor's edges' is.
fn ground_under(centre: &[Point], run: &[[f64; 2]], terrain: &Terrain) -> Profile {
    let mut out: Profile = Vec::new();
    // Whether the last piece reached the end of its segment on ground.
    let mut open = false;
    for w in run.windows(2) {
        let pieces = terrain.profile(w[0], w[1]);
        let mut reached = false;
        for (i, piece) in pieces.iter().enumerate() {
            let points = piece
                .iter()
                .map(|g| [project(centre, g.point).0, g.point[2]]);
            let starts = piece.first().is_some_and(|g| g.t < 1e-9);
            match out.last_mut() {
                Some(last) if i == 0 && open && starts => last.extend(points.skip(1)),
                _ => out.push(points.collect()),
            }
            reached = piece.last().is_some_and(|g| g.t > 1.0 - 1e-9);
        }
        open = reached;
    }
    out
}

/// The side lines at `distance` m either side of the axis of a route of
/// length `length` (the template's near side first), with the ground
/// along them from `ground` (offset → profile).
pub(crate) fn at_distance(
    options: &Options,
    distance: f64,
    length: f64,
    ground: impl Fn(f64) -> Profile,
) -> Vec<SideLine> {
    let near = options.template.corridor.near_offset(distance);
    [near, -near]
        .into_iter()
        .map(|o| SideLine {
            right: o >= 0.0,
            plan: vec![vec![[0.0, o], [length, o]]],
            ground: ground(o),
        })
        .collect()
}

impl RouteDrawing {
    /// Sets the profile's side lines under the outermost conductors of
    /// the lowest arm (see the [module](self)) when `options` ask for
    /// them ([`Options::profile_line_mode`]; else nothing is done): the
    /// wires of `line` along `route` as solved in still air, `still`
    /// (none: no case without wind is solved — the lines then follow the
    /// straight chord between the wires' attachment points, and a note
    /// says so on the sheets), the ground under them from `terrain`
    /// (none: the lines are drawn in the plan only).
    ///
    /// Expected cost: one ground profile per piece of the wires followed
    /// (as the corridor's edges': microseconds per triangle crossed), each
    /// ground point projected on the centreline.
    pub fn set_lowest_arm_lines(
        &mut self,
        line: &Line,
        route: &AlignmentView,
        still: Option<&[SectionShape]>,
        terrain: Option<&Terrain>,
        options: &Options,
    ) {
        if options.profile_line_mode() != ProfileLines::LowestArm
            || !options.template.corridor.profile
        {
            return;
        }
        let centre = &self.centreline;
        let [left, right] = candidates(line, route, centre, still, options.profile_circuits());
        let near_right = options.template.corridor.near_offset(1.0) > 0.0;
        let mut lines = Vec::new();
        for (right, wires) in [(false, left), (true, right)] {
            let runs = joined(chosen(wires));
            if runs.is_empty() {
                continue;
            }
            lines.push(SideLine {
                right,
                ground: terrain.map_or_else(Vec::new, |t| {
                    runs.iter()
                        .flat_map(|r| ground_under(centre, r, t))
                        .collect()
                }),
                plan: runs
                    .iter()
                    .map(|r| {
                        r.iter()
                            .map(|p| {
                                let (s, o) = project(centre, [p[0], p[1], 0.0]);
                                [s, o]
                            })
                            .collect()
                    })
                    .collect(),
            });
        }
        // The template's near side first, as the corridor's edges.
        lines.sort_by_key(|l| l.right != near_right);
        self.profile_line_notes.clear();
        if still.is_none() && !lines.is_empty() {
            self.profile_line_notes.push(
                "the profile's side lines under the lowest arm: no weather case without wind is \
                 solved, so they follow the straight chord between the wires' attachment points \
                 (no swing of the strings) — add a still-air case"
                    .into(),
            );
        }
        if lines.is_empty() {
            self.profile_line_notes.push(
                "the profile's side lines under the lowest arm: no conductor of the circuits \
                 counted along this route, no side line drawn"
                    .into(),
            );
        }
        self.profile_lines = lines;
    }
}
