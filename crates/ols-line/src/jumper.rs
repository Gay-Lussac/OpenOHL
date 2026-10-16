// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Jumpers (DESIGN_GOALS §12): slack cables connecting two attachment
//! points — the ends of two sections at a dead-end structure, or a tap from
//! a branch to a through wire at a Y pylon.
//!
//! - **Ends**: the solved clamp position of each end when the attachment is
//!   a point of a solved section (strain assemblies settle with their
//!   conductor, so the jumper follows them), otherwise the attachment's
//!   wire point.
//! - **Cable**: one exact elastic catenary element carrying the weather
//!   load (weight, ice, wind horizontal and perpendicular to the jumper's
//!   chord). Its unstressed length is the given cut length, or the length
//!   that gives the given sag in still air, scaled by
//!   \\( e^{\alpha \Delta t} \\) from its temperature.
//! - **Support string** (optional): a tension-only string from a point on
//!   the structure holding the jumper at a fraction of its length; the
//!   jumper is then two elements and the clamp is found by the Newton
//!   solver of `ols-wirefe`.
//! - **Support clamps** (optional, [`crate::JumperClamp`]): clamps on the
//!   jumper hanging on a structure's strings (an I or a V). A clamp is a
//!   short straight rigid piece of the jumper, held at its middle: free to
//!   swing and turn, the jumper leaving it in the direction it enters.
//!   Always solved as the rod below (also with EI = 0 or hinged ends);
//!   each string is rigid and tension-only.
//!
//! - **Bending stiffness** (default): where a jumper leaves the end clamp
//!   of a section, the clamp holds it along the conductor's axis (the
//!   span's tangent at the clamp, continued) and the cable's bending
//!   stiffness carries that direction into the jumper; the jumper then
//!   cannot swing in the wind as if hinged at its ends. Solved as an
//!   inextensible elastic rod (`rod`); the support string, if any, is
//!   rigid. An end that is not a section's end (a tap on a through wire)
//!   stays hinged; with both ends hinged, or EI = 0, the jumper is the
//!   catenary element above.
//!
//! A jumper's tension (a few hundred N) is negligible for the sections it
//! connects, so sections are solved first and jumpers between their
//! solved ends.

use nalgebra::Vector3;
use ols_cables::Cable;
use ols_core::budget::Budget;
use ols_weather::WeatherCase;
use ols_wirefe::catenary_element::default_element_budget;
use ols_wirefe::{CableElement, CableProperties, Model, Node, TrussElement, default_fe_budget};

use crate::rod::{Rod, RodHold, RodShape, RodString};
use crate::solve::{SectionShape, p, v, wire_load_in_line_wind};
use crate::{Jumper, Line, LineError, Point, SectionPoint};

/// The estimated load each jumper hangs on its ends in weather case
/// `case` (the whole bundle's, N, as a force vector), by section point:
/// the weight, ice and wind (across the line from the case's side, as on
/// the jumper itself) of the jumper between that end and its first
/// support (half the jumper without supports). The jumper's length is its
/// cut length, else its chord between the wire points in still air
/// lengthened by its sag (`L ≈ c + 8 s² / 3c`). `cable` gives a jumper's
/// cable (none: the jumper is left out). The sections ending there carry
/// it at their clamps in that case ([`crate::solve::Coupling::jumpers`]),
/// or the jumper's own pull ([`crate::JumperLoads::Pull`],
/// [`JumperShape::end_forces`]).
pub fn end_loads<'a>(
    line: &Line,
    case: &WeatherCase,
    cable: impl Fn(&str) -> Option<&'a Cable>,
) -> std::collections::BTreeMap<SectionPoint, [f64; 3]> {
    let mut out = std::collections::BTreeMap::new();
    for j in &line.jumpers {
        let Some(c) = cable(&j.cable) else {
            continue;
        };
        let (Some(a), Some(b)) = (line.wire_point(&j.from), line.wire_point(&j.to)) else {
            continue;
        };
        let chord = (0..3)
            .map(|k| (a.position[k] - b.position[k]).powi(2))
            .sum::<f64>()
            .sqrt();
        // Given by its sag: the length giving it between the wire points in
        // still air (as the jumper is cut).
        let from_sag = |sag: f64| {
            let props = CableProperties {
                unstressed_length: 1.0,
                axial_stiffness: c.axial_stiffness().si(),
                load: Vector3::new(0.0, 0.0, -c.weight().si()),
            };
            let budget = Budget::new().max_iterations(200);
            length_for_sag(props, v(a.position), v(b.position), sag, &budget).ok()
        };
        let length = match (j.length, j.sag) {
            _ if j.rigid => chord,
            (Some(l), _) => l,
            (None, Some(s)) if chord > 1e-6 => {
                from_sag(s).unwrap_or(chord + 8.0 * s * s / (3.0 * chord))
            }
            _ => chord,
        };
        // Per metre: its weight, ice and the wind across the line at its
        // middle (as the jumper is solved).
        let d = v(b.position) - v(a.position);
        let along = line.direction_at([
            0.5 * (a.position[0] + b.position[0]),
            0.5 * (a.position[1] + b.position[1]),
        ]);
        let q = wire_load_in_line_wind(case, c, d, along);
        let bundle = f64::from(line.jumper_bundle(j).count.max(1));
        // The supports' places along it, from `from`.
        let mut at: Vec<f64> = j.supports.iter().map(|c| c.at).collect();
        if let Some(sp) = &j.support {
            at.push(sp.fraction);
        }
        let first = at.iter().copied().fold(1.0_f64, f64::min).clamp(0.0, 1.0);
        let last = at.iter().copied().fold(0.0_f64, f64::max).clamp(0.0, 1.0);
        let (from, to) = if at.is_empty() {
            (0.5, 0.5)
        } else {
            (first / 2.0, (1.0 - last) / 2.0)
        };
        for (p, share) in [(&j.from, from), (&j.to, to)] {
            let e = out.entry(p.clone()).or_insert([0.0; 3]);
            for k in 0..3 {
                e[k] += q[k] * length * share * bundle;
            }
        }
    }
    out
}

/// Solved geometry of a jumper.
#[derive(Debug, Clone, PartialEq)]
pub struct JumperShape {
    /// Jumper id.
    pub jumper: String,
    /// Points along the jumper, `from` to `to`.
    pub points: Vec<Point>,
    /// Unstressed length used (at the weather case temperature), m.
    pub unstressed_length: f64,
    /// Largest vertical distance below the end-to-end chord, m.
    pub sag: f64,
    /// Lowest point.
    pub low_point: Point,
    /// Largest tension, N.
    pub max_tension: f64,
    /// The force it exerts on the clamp at `from` and at `to` (the whole
    /// bundle's, N): its tension there, with its stiffness (the moments of
    /// clamped ends left out). The sections carry them with
    /// [`crate::JumperLoads::Pull`].
    pub end_forces: [Point; 2],
    /// Where it is held: its support clamps in order from `from`, or its
    /// support string (one, without structure and wire point).
    pub supports: Vec<JumperSupportShape>,
}

/// A solved support of a jumper: the clamp and the strings holding it.
#[derive(Debug, Clone, PartialEq)]
pub struct JumperSupportShape {
    /// The structure holding it (empty: a jumper's `support` string,
    /// given by its top's coordinates).
    pub structure: String,
    /// Its wire point on that structure (empty: a `support` string).
    pub wire: String,
    /// The clamp's middle (where the strings hold the jumper).
    pub clamp: Point,
    /// The jumper's direction through the clamp, `from` to `to` (unit).
    pub direction: Point,
    /// Its strings.
    pub strings: Vec<JumperStringShape>,
}

/// A solved string holding a jumper's clamp.
#[derive(Debug, Clone, PartialEq)]
pub struct JumperStringShape {
    /// The string's label on its structure (empty: a `support` string).
    pub label: String,
    /// Its top.
    pub top: Point,
    /// Its force on the whole bundle, N (0: slack).
    pub force: f64,
}

/// Where a jumper end is: the solved clamp of the first section through
/// that attachment, otherwise the attachment's wire point.
fn end_position(
    line: &Line,
    shapes: &[SectionShape],
    point: &SectionPoint,
) -> Result<Vector3<f64>, LineError> {
    for shape in shapes {
        let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
            continue;
        };
        if let Some(k) = section.points.iter().position(|q| q == point)
            && let Some(clamp) = shape.clamps.get(k)
        {
            return Ok(v(*clamp));
        }
    }
    line.wire_point(point)
        .map(|w| v(w.position))
        .ok_or_else(|| {
            LineError::Invalid(format!(
                "no wire point with set {} on {}",
                point.set, point.structure
            ))
        })
}

/// Angle of a jumper terminal from its tension string's line (towards the
/// load) when the jumper gives none, degrees. Terminals angled down are
/// usual on dead-end clamps; a terminal straight along a string rising to
/// the structure points the jumper upwards, and the jumper then swings
/// further in the wind than a hinged one (the bend at the clamps eases as
/// it swings).
pub const DEFAULT_TERMINAL_ANGLE_DEG: f64 = 30.0;

/// Least slack of a jumper, as a share of its length: shorter than its
/// clamps' distance by less, it is taut (a jumper's stretch at its tension
/// is ~10⁻⁶; a 4 m jumper with 2 cm of sag has ~7·10⁻⁵). The same floor
/// for cutting a jumper to a sag.
const TAUT: f64 = 1e-5;

/// The direction a section's end clamp holds the jumper in: along its
/// tension string, from the clamp (where the section puts it in the
/// weather case) back towards the string's top on the structure — two
/// strings: towards the middle of their tops; none (the wire clamped to
/// the structure): the conductor's axis out of the section — turned by
/// the terminal angle within the conductor's plane, towards the case's
/// load on it (`load`: weight, ice and wind; straight down in still air,
/// tilted with the wind as the conductor swings), square to that line.
/// `None` when `point` is not the end of a solved section.
fn clamp_direction(
    line: &Line,
    shapes: &[SectionShape],
    point: &SectionPoint,
    terminal_deg: f64,
    load: Vector3<f64>,
) -> Option<Vector3<f64>> {
    let axis = conductor_axis(line, shapes, point)?;
    let base = end_position(line, shapes, point)
        .ok()
        .and_then(|clamp| string_axis(line, point, clamp))
        .unwrap_or(axis);
    let g = if load.norm() > 0.0 {
        load.normalize()
    } else {
        -Vector3::z()
    };
    let across = g - base * base.dot(&g);
    if across.norm() < 1e-9 {
        // The load along the string: no plane to turn in.
        return Some(base);
    }
    let t = terminal_deg.to_radians();
    Some(base * t.cos() + across.normalize() * t.sin())
}

/// The line of the tension string(s) holding the wire point of `point`,
/// from `clamp` towards the string's top (two: the middle of their tops);
/// `None` when no string holds it.
fn string_axis(line: &Line, point: &SectionPoint, clamp: Vector3<f64>) -> Option<Vector3<f64>> {
    let structure = line.structure(&point.structure)?;
    let wire = structure.wire_at_set(point.set)?;
    let tops: Vec<Vector3<f64>> = structure
        .strings_to(&wire.label)
        .filter_map(|x| structure.node(&x.top).map(|n| v(n.position())))
        .collect();
    if tops.is_empty() {
        return None;
    }
    let mid = tops.iter().sum::<Vector3<f64>>() / tops.len() as f64;
    let d = mid - clamp;
    (d.norm() > 1e-9).then(|| d.normalize())
}

/// The conductor's axis at a section's end clamp, out of the section (the
/// span's tangent at the clamp, continued).
fn conductor_axis(
    line: &Line,
    shapes: &[SectionShape],
    point: &SectionPoint,
) -> Option<Vector3<f64>> {
    for shape in shapes {
        let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
            continue;
        };
        let Some(k) = section.points.iter().position(|q| q == point) else {
            continue;
        };
        let n = shape.spans.len();
        let d = if k == 0 {
            let p = &shape.spans.first()?.points;
            v(p[0]) - v(*p.get(1)?)
        } else if k == n {
            let p = &shape.spans.last()?.points;
            v(*p.last()?) - v(p[p.len().checked_sub(2)?])
        } else {
            return None;
        };
        return (d.norm() > 0.0).then(|| d.normalize());
    }
    None
}

/// The default reduction factor of a jumper's estimated bending stiffness
/// (its uniform effective EI as a share of the solid bar's).
pub const DEFAULT_STIFFNESS_FACTOR: f64 = 0.2;

/// Bending stiffness EI of a jumper's cable, N·m², when the jumper gives
/// none: `factor` × the cable's solid-bar stiffness
/// ([`Cable::bending_stiffness_max`]), but not below its strands slipping
/// on each other ([`Cable::bending_stiffness_min`], when the wire library
/// gives the strand counts). Drake 26/7 at 0.2: ~460 N·m² (solid ~2 300,
/// slipped ~44).
pub fn estimated_bending_stiffness(cable: &Cable, factor: f64) -> f64 {
    let ei = factor * cable.bending_stiffness_max();
    cable.bending_stiffness_min().map_or(ei, |min| ei.max(min))
}

/// The bending stiffness a jumper is solved with, N·m²: its own, else the
/// estimate from its cable with its factor (else the project's,
/// `defaults`).
pub fn jumper_bending_stiffness(jumper: &Jumper, cable: &Cable, defaults: &crate::Defaults) -> f64 {
    jumper.bending_stiffness.unwrap_or_else(|| {
        estimated_bending_stiffness(
            cable,
            jumper
                .stiffness_factor
                .unwrap_or(defaults.jumper_stiffness_factor),
        )
    })
}

/// Points at equal distances along a polyline (`n + 1` of them).
fn resample(points: &[Vector3<f64>], n: usize) -> Vec<Vector3<f64>> {
    let mut along = vec![0.0];
    for w in points.windows(2) {
        along.push(along.last().copied().unwrap_or(0.0) + (w[1] - w[0]).norm());
    }
    let total = along.last().copied().unwrap_or(0.0);
    let mut k = 0;
    (0..=n)
        .map(|i| {
            let s = total * i as f64 / n as f64;
            while k + 2 < points.len() && along[k + 1] < s {
                k += 1;
            }
            let span = (along[k + 1] - along[k]).max(1e-12);
            let t = ((s - along[k]) / span).clamp(0.0, 1.0);
            points[k] + (points[k + 1] - points[k]) * t
        })
        .collect()
}

/// Points along a polyline at the distances `at` from its start, in
/// shares of its length (increasing, 0 to 1).
fn resample_at(points: &[Vector3<f64>], at: &[f64]) -> Vec<Vector3<f64>> {
    let mut along = vec![0.0];
    for w in points.windows(2) {
        along.push(along.last().copied().unwrap_or(0.0) + (w[1] - w[0]).norm());
    }
    let total = along.last().copied().unwrap_or(0.0);
    let mut k = 0;
    at.iter()
        .map(|f| {
            let s = total * f;
            while k + 2 < points.len() && along[k + 1] < s {
                k += 1;
            }
            let span = (along[k + 1] - along[k]).max(1e-12);
            let t = ((s - along[k]) / span).clamp(0.0, 1.0);
            points[k] + (points[k + 1] - points[k]) * t
        })
        .collect()
}

/// Largest vertical distance of `points` below the chord from `a` to `b`
/// (chord height interpolated by horizontal position along it).
pub(crate) fn sag_below_chord(points: &[Vector3<f64>], a: Vector3<f64>, b: Vector3<f64>) -> f64 {
    let chord = b - a;
    let h2 = chord.xy().norm_squared();
    points
        .iter()
        .map(|q| {
            let t = if h2 > 0.0 {
                ((q - a).xy().dot(&chord.xy()) / h2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            a.z + chord.z * t - q.z
        })
        .fold(0.0, f64::max)
}

/// Shape of one element in still air (absolute points).
fn element_shape(
    props: &CableProperties,
    a: Vector3<f64>,
    d: Vector3<f64>,
) -> Result<(Vec<Vector3<f64>>, f64), String> {
    let state = props
        .solve(d, None, &default_element_budget())
        .map_err(|e| e.to_string())?;
    let points = props
        .shape(d, &state, 64)
        .into_iter()
        .map(|q| a + q)
        .collect();
    Ok((points, state.tension_i.max(state.tension_j)))
}

/// Unstressed length giving `sag` in still air between `a` and `b`, by
/// bisection on the (increasing) sag–length relation.
///
/// Expected cost: ~50 element solves (~1 µs each in release); exit clauses:
/// the budget's iteration limit and a 20× chord upper bound.
fn length_for_sag(
    props: CableProperties,
    a: Vector3<f64>,
    b: Vector3<f64>,
    sag: f64,
    budget: &Budget,
) -> Result<f64, String> {
    let d = b - a;
    let chord = d.norm();
    let sag_of = |l0: f64| -> Result<f64, String> {
        let props = CableProperties {
            unstressed_length: l0,
            ..props
        };
        Ok(sag_below_chord(&element_shape(&props, a, d)?.0, a, b))
    };
    let mut low = chord * (1.0 + TAUT);
    if sag_of(low)? >= sag {
        return Err(format!(
            "a sag of {sag} m is less than the taut jumper's ({:.3} m)",
            sag_of(low)?
        ));
    }
    let mut high = chord * 1.1;
    while sag_of(high)? < sag {
        low = high;
        high *= 1.5;
        if high > 20.0 * chord {
            return Err(format!(
                "a sag of {sag} m needs more than 20× the chord length"
            ));
        }
    }
    let mut run = budget.start();
    while high - low > 1e-7 * chord {
        run.step()
            .map_err(|stop| format!("sag to length: {stop}"))?;
        let mid = 0.5 * (low + high);
        if sag_of(mid)? < sag {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok(0.5 * (low + high))
}

/// Where a rod is held.
#[derive(Debug, Clone, Default)]
enum Held {
    /// Nowhere.
    #[default]
    Free,
    /// A string at the node nearest a share of the length (a jumper's
    /// `support`), with the weight hanging there (N).
    Node {
        fraction: f64,
        weight: f64,
        string: RodString,
    },
    /// Support clamps, in order.
    Clamps(Vec<ClampHold>),
}

/// A support clamp to solve: its middle's share of the length, its length
/// (m), the weight hanging on it (N), its strings.
#[derive(Debug, Clone)]
struct ClampHold {
    at: f64,
    length: f64,
    weight: f64,
    /// The wind on its strings (half of each), N.
    wind: Vector3<f64>,
    strings: Vec<RodString>,
}

/// Pieces of a rod of `length` around `clamps`: each clamp one piece of
/// its length; the stretches between them in pieces of equal length, 16
/// to 40 in all (~0.12 m), at least one each, in proportion to their
/// lengths. Also each clamp's piece.
fn clamp_pieces(length: f64, clamps: &[ClampHold]) -> Result<(Vec<f64>, Vec<usize>), String> {
    let mut stretches = Vec::with_capacity(clamps.len() + 1);
    let mut from = 0.0;
    for c in clamps {
        let start = c.at * length - c.length / 2.0;
        stretches.push(start - from);
        from = start + c.length;
    }
    stretches.push(length - from);
    if stretches.iter().any(|s| *s < 1e-3) {
        return Err(
            "support clamps overlap each other or the jumper's ends at this temperature".into(),
        );
    }
    let free: f64 = stretches.iter().sum();
    let n = ((free / 0.12).ceil() as usize).clamp(16, 40);
    let mut pieces = Vec::new();
    let mut at = Vec::with_capacity(clamps.len());
    for (k, s) in stretches.iter().enumerate() {
        let m = ((n as f64 * s / free).round() as usize).max(1);
        pieces.extend(std::iter::repeat_n(s / m as f64, m));
        if let Some(c) = clamps.get(k) {
            at.push(pieces.len());
            pieces.push(c.length);
        }
    }
    Ok((pieces, at))
}

/// How a stiff jumper is solved: its ends' clamp directions, EI, pieces.
struct Stiff {
    start: Option<Vector3<f64>>,
    end: Option<Vector3<f64>>,
    ei: f64,
}

impl Stiff {
    /// Pieces for a jumper of `length`: ~0.12 m, 16 to 40.
    fn pieces(length: f64) -> usize {
        ((length / 0.12).ceil() as usize).clamp(16, 40)
    }

    /// Solves the rod of `length` under `load` between `a` and `b` (held
    /// as `held` says), starting from the hinged catenary; if that does
    /// not converge, the bending stiffness is brought in in steps.
    fn solve(
        &self,
        a: Vector3<f64>,
        b: Vector3<f64>,
        length: f64,
        load: Vector3<f64>,
        held: &Held,
        ea: f64,
    ) -> Result<RodShape, String> {
        let props = CableProperties {
            unstressed_length: length,
            axial_stiffness: ea,
            load,
        };
        let hinged = element_shape(&props, a, b - a)?.0;
        let (pieces, holds, nodes) = match held {
            Held::Free | Held::Node { .. } => {
                let n = Self::pieces(length);
                let holds = match held {
                    // The string's node: the nearest to its fraction of
                    // the length.
                    Held::Node {
                        fraction,
                        weight,
                        string,
                    } => vec![RodHold {
                        piece: (fraction * n as f64).round().clamp(1.0, (n - 1) as f64) as usize,
                        offset: 0.0,
                        load: Vector3::new(0.0, 0.0, -*weight),
                        strings: vec![*string],
                    }],
                    _ => Vec::new(),
                };
                (vec![length / n as f64; n], holds, resample(&hinged, n))
            }
            Held::Clamps(clamps) => {
                let (pieces, at) = clamp_pieces(length, clamps)?;
                let holds = clamps
                    .iter()
                    .zip(at)
                    .map(|(c, piece)| RodHold {
                        piece,
                        offset: 0.5,
                        load: Vector3::new(0.0, 0.0, -c.weight) + c.wind,
                        strings: c.strings.clone(),
                    })
                    .collect();
                let mut along = vec![0.0];
                for p in &pieces {
                    along.push(along.last().copied().unwrap_or(0.0) + p / length);
                }
                let nodes = resample_at(&hinged, &along);
                (pieces, holds, nodes)
            }
        };
        let mut start: Vec<Vector3<f64>> = nodes.windows(2).map(|w| w[1] - w[0]).collect();
        let rod = |ei: f64| Rod {
            a,
            b,
            pieces: pieces.clone(),
            q: load,
            ei,
            start: self.start,
            end: self.end,
            holds: holds.clone(),
        };
        let budget = Budget::new()
            .max_iterations(150)
            .time_limit(std::time::Duration::from_secs(60));
        match rod(self.ei).solve(&start, &budget) {
            Ok(shape) => Ok(shape),
            Err(first) => {
                for f in [0.0, 0.05, 0.2, 0.5, 1.0] {
                    let shape = rod(self.ei * f)
                        .solve(&start, &budget)
                        .map_err(|e| format!("{first}; with EI in steps: {e}"))?;
                    start = shape.directions.clone();
                    if f == 1.0 {
                        return Ok(shape);
                    }
                }
                unreachable!("the last step returns")
            }
        }
    }
}

/// Unstressed length giving a stiff jumper `sag` in still air, by the
/// Illinois method from the hinged jumper's length.
///
/// Expected cost: ~5–15 rod solves (~1 ms each in release); exit clauses:
/// 40 iterations, a 20× chord upper bound.
fn stiff_length_for_sag(
    stiff: &Stiff,
    still: CableProperties,
    a: Vector3<f64>,
    b: Vector3<f64>,
    sag: f64,
    hinged: f64,
) -> Result<f64, String> {
    let chord = (b - a).norm();
    let f = |l0: f64| -> Result<f64, String> {
        let shape = stiff.solve(a, b, l0, still.load, &Held::Free, still.axial_stiffness)?;
        Ok(sag_below_chord(&shape.points, a, b) - sag)
    };
    let (mut lo, mut hi) = (chord * (1.0 + TAUT), hinged);
    let (mut flo, mut fhi) = (f(lo)?, f(hi)?);
    if flo >= 0.0 {
        return Err(format!(
            "a sag of {sag} m is less than the taut jumper's ({:.3} m)",
            flo + sag
        ));
    }
    while fhi < 0.0 {
        (lo, flo) = (hi, fhi);
        hi *= 1.3;
        if hi > 20.0 * chord {
            return Err(format!(
                "a sag of {sag} m needs more than 20× the chord length"
            ));
        }
        fhi = f(hi)?;
    }
    let mut side = 0;
    for _ in 0..40 {
        let mid = (lo * fhi - hi * flo) / (fhi - flo);
        let fm = f(mid)?;
        if fm.abs() < 1e-8 || hi - lo < 1e-12 * chord {
            return Ok(mid);
        }
        if fm < 0.0 {
            (lo, flo) = (mid, fm);
            if side == -1 {
                fhi /= 2.0;
            }
            side = -1;
        } else {
            (hi, fhi) = (mid, fm);
            if side == 1 {
                flo /= 2.0;
            }
            side = 1;
        }
    }
    Err(format!("sag {sag} m: no length found in 40 iterations"))
}

/// The bending stiffness of a jumper and the directions its end clamps hold
/// it in (`None`: hinged).
fn stiffness(
    line: &Line,
    jumper: &Jumper,
    shapes: &[SectionShape],
    cable: &Cable,
    load: Vector3<f64>,
) -> Option<Stiff> {
    let stiff = rod_ends(line, jumper, shapes, cable, load);
    (stiff.ei > 0.0 && (stiff.start.is_some() || stiff.end.is_some())).then_some(stiff)
}

/// The bending stiffness of a jumper and the directions its end clamps hold
/// it in, whether or not it is stiff (`load`: the case's load on it, per
/// metre: which way its clamps' angles turn).
fn rod_ends(
    line: &Line,
    jumper: &Jumper,
    shapes: &[SectionShape],
    cable: &Cable,
    load: Vector3<f64>,
) -> Stiff {
    let ei = jumper_bending_stiffness(jumper, cable, &line.defaults);
    let angle = line.defaults.jumper_exit_angle_deg;
    Stiff {
        start: clamp_direction(
            line,
            shapes,
            &jumper.from,
            jumper.angle_from_deg.unwrap_or(angle),
            load,
        ),
        // Arriving at the clamp: against the direction it holds.
        end: clamp_direction(
            line,
            shapes,
            &jumper.to,
            jumper.angle_to_deg.unwrap_or(angle),
            load,
        )
        .map(|d| -d),
        ei,
    }
}

/// A jumper's support clamps to solve: each clamp's strings from the
/// structure (tops, lengths) and the weight on it per subconductor (half of
/// each string's, and its wire point's), and the wind on its strings (half
/// of each: `wind` N per m² of wind area, across the line as on the
/// line's suspension strings).
fn clamp_holds(
    line: &Line,
    jumper: &Jumper,
    bundle: u32,
    wind: Vector3<f64>,
) -> Result<Vec<ClampHold>, String> {
    jumper
        .supports
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let id = c.structure_id(jumper);
            let n = k + 1;
            let structure = line
                .structure(id)
                .ok_or_else(|| format!("support clamp {n}: unknown structure {id}"))?;
            let (wire, strings) = crate::clamp_strings(structure, &c.wire)
                .map_err(|m| format!("support clamp {n}: {m}"))?;
            let mut weight = wire.weight;
            let mut push = Vector3::zeros();
            let mut held = Vec::with_capacity(strings.len());
            for s in strings {
                let top = structure.point(&s.top).map(|p| v(p.position));
                let length = structure.string_length(s);
                let (Some(top), Some(length)) = (top, length) else {
                    return Err(format!("support clamp {n}: string {} has no top", s.label));
                };
                weight += s.weight / 2.0;
                push += wind * (0.5 * s.wind_area);
                held.push(RodString {
                    top,
                    length,
                    // A string that may take compression pushes too.
                    push: s.compression.is_some(),
                });
            }
            Ok(ClampHold {
                at: c.at,
                length: c.clamp_length,
                weight: weight / f64::from(bundle),
                wind: push / f64::from(bundle),
                strings: held,
            })
        })
        .collect()
}

/// The support shapes of a solved rod: the clamps, or the support string.
fn support_shapes(
    line: &Line,
    jumper: &Jumper,
    shape: &RodShape,
    bundle: u32,
) -> Vec<JumperSupportShape> {
    let scale = f64::from(bundle);
    if let Some(s) = &jumper.support {
        return shape
            .holds
            .iter()
            .map(|h| JumperSupportShape {
                structure: String::new(),
                wire: String::new(),
                clamp: p(h.point),
                direction: p(h.direction),
                strings: vec![JumperStringShape {
                    label: String::new(),
                    top: s.point,
                    force: h.forces[0] * scale,
                }],
            })
            .collect();
    }
    jumper
        .supports
        .iter()
        .zip(&shape.holds)
        .map(|(c, h)| {
            let id = c.structure_id(jumper);
            let strings = line
                .structure(id)
                .and_then(|st| crate::clamp_strings(st, &c.wire).ok().map(|(_, x)| (st, x)))
                .map(|(st, strings)| {
                    strings
                        .iter()
                        .zip(&h.forces)
                        .map(|(x, f)| JumperStringShape {
                            label: x.label.clone(),
                            top: st.point(&x.top).map_or([0.0; 3], |q| q.position),
                            force: f * scale,
                        })
                        .collect()
                })
                .unwrap_or_default();
            JumperSupportShape {
                structure: id.to_owned(),
                wire: c.wire.clone(),
                clamp: p(h.point),
                direction: p(h.direction),
                strings,
            }
        })
        .collect()
}

/// The unstressed length, at the jumper's temperature, giving its sag in
/// still air between the section ends solved in `shapes` (its design
/// case: still air at that temperature).
fn cut_length(
    line: &Line,
    jumper: &Jumper,
    shapes: &[SectionShape],
    cable: &Cable,
) -> Result<f64, LineError> {
    let invalid = |m: String| LineError::Invalid(format!("jumper {}: {m}", jumper.id));
    let sag = jumper
        .sag
        .ok_or_else(|| invalid("needs a length or a sag".into()))?;
    let a = end_position(line, shapes, &jumper.from).map_err(|e| invalid(e.to_string()))?;
    let b = end_position(line, shapes, &jumper.to).map_err(|e| invalid(e.to_string()))?;
    let still = CableProperties {
        unstressed_length: 1.0,
        axial_stiffness: cable.axial_stiffness().si(),
        load: Vector3::new(0.0, 0.0, -cable.weight().si()),
    };
    let budget = Budget::new().max_iterations(200);
    let hinged = length_for_sag(still, a, b, sag, &budget).map_err(invalid)?;
    match stiffness(line, jumper, shapes, cable, still.load) {
        Some(stiff) => stiff_length_for_sag(&stiff, still, a, b, sag, hinged).map_err(invalid),
        None => Ok(hinged),
    }
}

/// Solves one jumper in a weather case, between the solved section ends.
pub fn solve_jumper(
    line: &Line,
    jumper: &Jumper,
    shapes: &[SectionShape],
    cable: &Cable,
    case: &WeatherCase,
) -> Result<JumperShape, LineError> {
    let invalid = |m: String| LineError::Invalid(format!("jumper {}: {m}", jumper.id));
    // Its ends are the solved clamps: a section through an end that did not
    // solve leaves no clamp (the attachment's data point would be a wrong,
    // plausible-looking end).
    for end in [&jumper.from, &jumper.to] {
        if let Some(s) = line
            .sections
            .iter()
            .find(|s| s.points.contains(end) && !shapes.iter().any(|x| x.section == s.id))
        {
            return Err(invalid(format!(
                "section {} at its end at {} is not solved in this weather case",
                s.id, end.structure
            )));
        }
    }
    let a = end_position(line, shapes, &jumper.from).map_err(|e| invalid(e.to_string()))?;
    let b = end_position(line, shapes, &jumper.to).map_err(|e| invalid(e.to_string()))?;
    let d = b - a;
    if jumper.rigid {
        // A straight member between the clamps.
        let points: Vec<Vector3<f64>> = (0..=16).map(|k| a + d * (k as f64 / 16.0)).collect();
        let low = if a.z <= b.z { a } else { b };
        // Held at both ends: half its weight and ice on each.
        let half = case.wire_load(cable.diameter, cable.weight()).vertical.si()
            * d.norm()
            * f64::from(line.jumper_bundle(jumper).count.max(1))
            / 2.0;
        return Ok(JumperShape {
            jumper: jumper.id.clone(),
            sag: 0.0,
            points: points.iter().map(|q| p(*q)).collect(),
            unstressed_length: d.norm(),
            low_point: p(low),
            max_tension: 0.0,
            end_forces: [[0.0, 0.0, -half]; 2],
            supports: Vec::new(),
        });
    }
    if d.xy().norm() < 0.1 {
        return Err(invalid(
            "ends less than 0.1 m apart horizontally (a vertical jumper is not supported yet)"
                .into(),
        ));
    }
    let ea = cable.axial_stiffness().si();
    // The line's wind, whichever way the jumper was entered: across the
    // line from the case's side (a jumper across the line takes little).
    let along = line.direction_at([0.5 * (a.x + b.x), 0.5 * (a.y + b.y)]);
    let load = wire_load_in_line_wind(case, cable, d, along);
    let stiff = stiffness(line, jumper, shapes, cable, load);
    let reference = match (jumper.length, jumper.sag) {
        (Some(length), _) => length,
        // Given by its sag: cut in this case (the design case; other cases
        // get the length cut there, see [`solve_jumpers`]).
        (None, Some(_)) => cut_length(line, jumper, shapes, cable)?,
        (None, None) => return Err(invalid("needs a length or a sag".into())),
    };
    // Lengthened or shortened like the line's cables: a bimetallic cable
    // by the line's model (its outer strands in compression up to their
    // limit when warmer, then the core alone); unstressed at its
    // temperature.
    let dt = case.temperature.celsius() - jumper.temperature;
    let l0 = reference * line.bimetallic.apply(cable).free_thermal_strain(dt).exp();
    let props = CableProperties {
        unstressed_length: l0,
        axial_stiffness: ea,
        load,
    };

    let bundle = line.jumper_bundle(jumper).count.max(1);
    // Clamps swung apart further than the jumper is long (free strain
    // strings in the wind: 0.6 m on a real line at 650 Pa): it would be
    // taut and hold them back, and the pull of a jumper on its clamps is
    // not modelled — sections are solved first. Said so, instead of a
    // stretched jumper at hundreds of kN or a solver that cannot settle.
    if d.norm() >= l0 * (1.0 - TAUT) {
        return Err(invalid(format!(
            "taut: {l0:.2} m long between clamps {:.2} m apart in this weather case (the \
             clamps swing apart; a jumper holding them back is not modelled yet)",
            d.norm()
        )));
    }
    // Support clamps: always the rod (they keep it straight through them
    // whatever its stiffness).
    let stiff = if jumper.supports.is_empty() {
        stiff
    } else {
        Some(rod_ends(line, jumper, shapes, cable, load))
    };
    if let Some(stiff) = &stiff {
        // Half the string's weight per wire on the jumper.
        let held = match &jumper.support {
            Some(s) => Held::Node {
                fraction: s.fraction,
                weight: s.weight / f64::from(bundle) / 2.0,
                string: RodString {
                    top: v(s.point),
                    length: s.length,
                    push: false,
                },
            },
            None if !jumper.supports.is_empty() => {
                // The wind on the strings, across the line from the case's
                // side (per m² of their wind area).
                let wind = along.map_or(Vector3::zeros(), |a| {
                    Vector3::new(-a[1], a[0], 0.0)
                        * (case.wind_pressure.si() * case.wind_from.to_left())
                });
                Held::Clamps(clamp_holds(line, jumper, bundle, wind).map_err(invalid)?)
            }
            None => Held::Free,
        };
        let shape = stiff.solve(a, b, l0, load, &held, ea).map_err(invalid)?;
        let low = *shape
            .points
            .iter()
            .min_by(|x, y| x.z.total_cmp(&y.z))
            .expect("points");
        return Ok(JumperShape {
            jumper: jumper.id.clone(),
            sag: sag_below_chord(&shape.points, a, b),
            points: shape.points.iter().map(|q| p(*q)).collect(),
            unstressed_length: l0,
            low_point: p(low),
            max_tension: shape.max_tension,
            end_forces: shape.end_forces.map(|f| p(f * f64::from(bundle))),
            supports: support_shapes(line, jumper, &shape, bundle),
        });
    }
    // The pull at an end: its tension there along the jumper.
    let pull = |at: Vector3<f64>, next: Vector3<f64>, t: f64| {
        let u = next - at;
        if u.norm() > 0.0 {
            u.normalize() * (t * f64::from(bundle))
        } else {
            Vector3::zeros()
        }
    };
    let (points, max_tension, supports, end_forces) = match &jumper.support {
        None => {
            let state = props
                .solve(d, None, &default_element_budget())
                .map_err(|e| invalid(e.to_string()))?;
            let points: Vec<Vector3<f64>> = props
                .shape(d, &state, 64)
                .into_iter()
                .map(|q| a + q)
                .collect();
            let n = points.len();
            let ends = [
                pull(points[0], points[1], state.tension_i),
                pull(points[n - 1], points[n - 2], state.tension_j),
            ];
            (
                points,
                state.tension_i.max(state.tension_j),
                Vec::new(),
                ends,
            )
        }
        Some(s) => {
            let top = v(s.point);
            let mut model = Model::default();
            model.nodes.push(Node::fixed(a));
            model.nodes.push(Node::fixed(b));
            model.nodes.push(Node::fixed(top));
            // Start the clamp below the string top.
            model
                .nodes
                .push(Node::free(top - Vector3::new(0.0, 0.0, s.length)));
            for (i, j, length) in [(0, 3, l0 * s.fraction), (3, 1, l0 * (1.0 - s.fraction))] {
                model.cables.push(CableElement {
                    i,
                    j,
                    properties: CableProperties {
                        unstressed_length: length,
                        ..props
                    },
                });
            }
            model.trusses.push(TrussElement {
                i: 2,
                j: 3,
                unstressed_length: s.length,
                axial_stiffness: line.defaults.insulator_stiffness,
                tension_only: true,
                // One subconductor of the bundle: its share of the weight.
                weight: s.weight / f64::from(bundle),
            });
            let solution = model
                .solve(&default_fe_budget())
                .map_err(|e| invalid(e.to_string()))?;
            let pos = &solution.positions;
            let mut points: Vec<Vector3<f64>> = Vec::new();
            let mut max_tension: f64 = 0.0;
            let mut at_clamp = 0;
            for (e, element) in model.cables.iter().enumerate() {
                if e == 1 {
                    at_clamp = points.len() - 1;
                }
                let state = &solution.cables[e];
                max_tension = max_tension.max(state.tension_i).max(state.tension_j);
                let d = pos[element.j] - pos[element.i];
                let skip = usize::from(!points.is_empty());
                points.extend(
                    element
                        .properties
                        .shape(d, state, 64)
                        .into_iter()
                        .skip(skip)
                        .map(|q| pos[element.i] + q),
                );
            }
            // The jumper's direction through the clamp: between its
            // neighbouring points.
            let through = points[(at_clamp + 1).min(points.len() - 1)] - points[at_clamp - 1];
            let n = points.len();
            let ends = [
                pull(points[0], points[1], solution.cables[0].tension_i),
                pull(points[n - 1], points[n - 2], solution.cables[1].tension_j),
            ];
            (
                points,
                max_tension,
                vec![JumperSupportShape {
                    structure: String::new(),
                    wire: String::new(),
                    clamp: p(pos[3]),
                    direction: p(through.normalize()),
                    strings: vec![JumperStringShape {
                        label: String::new(),
                        top: s.point,
                        // For the whole bundle.
                        force: solution.truss_forces[0] * f64::from(bundle),
                    }],
                }],
                ends,
            )
        }
    };
    let low = *points
        .iter()
        .min_by(|x, y| x.z.total_cmp(&y.z))
        .expect("points");
    Ok(JumperShape {
        jumper: jumper.id.clone(),
        sag: sag_below_chord(&points, a, b),
        points: points.iter().map(|q| p(*q)).collect(),
        unstressed_length: l0,
        low_point: p(low),
        max_tension,
        end_forces: end_forces.map(p),
        supports,
    })
}

/// Solves every jumper of the line between the solved sections `shapes`.
///
/// A jumper given by its sag is cut once, in its design case (still air at
/// its temperature, the sections at its ends solved there), and keeps that
/// length in every case: its sag then varies with the weather like a
/// cable's. The cut is kept between solves until the jumper, the sections
/// at its ends, their structures or cables change.
pub fn solve_jumpers(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    shapes: &[SectionShape],
) -> Vec<Result<JumperShape, LineError>> {
    let all: Vec<&Jumper> = line.jumpers.iter().collect();
    in_parallel(&all, |jumper| {
        solve_one_jumper(line, library, case, shapes, jumper)
    })
}

/// `f` of every item, in order, spread over the cores (jumpers are
/// independent of each other). Fewer than 16 items: on this thread.
pub(crate) fn in_parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(items.len() / 8)
        .max(1);
    if threads == 1 {
        return items.iter().map(&f).collect();
    }
    let chunk = items.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| scope.spawn(|| part.iter().map(&f).collect::<Vec<R>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a jumper solve panicked"))
            .collect()
    })
}

/// Solves one jumper of the line between the solved sections `shapes`
/// (see [`solve_jumpers`]).
pub fn solve_one_jumper(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    shapes: &[SectionShape],
    jumper: &Jumper,
) -> Result<JumperShape, LineError> {
    let cable = library.get(&jumper.cable).ok_or_else(|| {
        LineError::Invalid(format!(
            "jumper {}: cable \"{}\" not in the libraries",
            jumper.id, jumper.cable
        ))
    })?;
    if jumper.length.is_none() && jumper.sag.is_some() {
        let mut cut = jumper.clone();
        cut.length = Some(cut_once(line, library, jumper, cable)?);
        return solve_jumper(line, &cut, shapes, cable, case);
    }
    solve_jumper(line, jumper, shapes, cable, case)
}

/// Cut lengths of the jumpers given by their sag, kept between solves (and
/// weather cases) by what they depend on ([`design_key`]).
static CUTS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, f64>>> =
    std::sync::OnceLock::new();

/// Lengths kept at most (then forgotten all at once).
const MAX_CUTS: usize = 20_000;

/// Everything a jumper's cut depends on: the jumper; the sections at its
/// ends, the structures they and the jumper hang on and the cables (they
/// are solved in its design case); the line's creep-stretch and bimetallic
/// settings and creep cases. Equal keys, equal cuts.
fn design_key(line: &Line, library: &ols_cables::Library, jumper: &Jumper) -> String {
    let ends = |p: &SectionPoint| *p == jumper.from || *p == jumper.to;
    let sections: Vec<&crate::Section> = line
        .sections
        .iter()
        .filter(|s| s.points.iter().any(ends))
        .collect();
    let mut ids: std::collections::BTreeSet<&str> = sections
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.structure.as_str()))
        .collect();
    ids.insert(&jumper.from.structure);
    ids.insert(&jumper.to.structure);
    for c in &jumper.supports {
        ids.insert(c.structure_id(jumper));
    }
    let structures: Vec<&crate::Structure> = line
        .structures
        .iter()
        .filter(|s| ids.contains(s.id.as_str()))
        .collect();
    let cables: Vec<Option<&Cable>> = std::iter::once(&jumper.cable)
        .chain(sections.iter().map(|s| &s.cable))
        .map(|c| library.get(c))
        .collect();
    format!(
        "{:?}",
        (
            jumper,
            sections,
            structures,
            cables,
            &line.creep_stretch,
            &line.bimetallic,
            &line.condition_cases
        )
    )
}

/// The cut length of a jumper given by its sag: sagged once in its design
/// case (still air at its temperature, the sections at its ends solved
/// there), then kept until something it depends on changes.
///
/// Expected cost: a lookup when kept; else the end sections' solves and
/// ~5–15 rod solves (a few ms).
fn cut_once(
    line: &Line,
    library: &ols_cables::Library,
    jumper: &Jumper,
    cable: &Cable,
) -> Result<f64, LineError> {
    let key = design_key(line, library, jumper);
    let cuts = CUTS.get_or_init(Default::default);
    if let Some(l) = cuts.lock().ok().and_then(|m| m.get(&key).copied()) {
        return Ok(l);
    }
    let design = design_shapes(line, library, jumper)?;
    let l = cut_length(line, jumper, &design, cable)?;
    if let Ok(mut m) = cuts.lock() {
        if m.len() >= MAX_CUTS {
            m.clear();
        }
        m.insert(key, l);
    }
    Ok(l)
}

/// The sections at a jumper's ends solved in its design case.
fn design_shapes(
    line: &Line,
    library: &ols_cables::Library,
    jumper: &Jumper,
) -> Result<Vec<SectionShape>, LineError> {
    let case = WeatherCase::still(
        "jumper design",
        ols_core::units::Temperature::from_celsius(jumper.temperature),
    );
    line.sections
        .iter()
        .filter(|s| {
            s.points
                .iter()
                .any(|p| *p == jumper.from || *p == jumper.to)
        })
        .map(|s| crate::solve::solve_one(line, s, library, &case))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JumperSupport;
    use crate::solve::solve_line;
    use ols_core::units::{Stress, Temperature};
    use std::path::Path;

    fn library() -> ols_cables::Library {
        ols_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    fn still(t: f64) -> WeatherCase {
        WeatherCase::still("t", Temperature::from_celsius(t))
    }

    fn solve_all(line: &Line, case: &WeatherCase) -> (Vec<SectionShape>, Vec<JumperShape>) {
        let lib = library();
        let shapes: Vec<_> = solve_line(line, &lib, case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let jumpers = solve_jumpers(line, &lib, case, &shapes)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        (shapes, jumpers)
    }

    /// The estimate: the factor × the solid bar, never below the slipped
    /// strands; a jumper's own EI or factor wins.
    #[test]
    fn bending_stiffness_estimate_and_its_floor() {
        let lib = library();
        let mut cable = lib.get("acsr-drake-26-7").unwrap().clone();
        let max = cable.bending_stiffness_max();
        let min = cable
            .bending_stiffness_min()
            .expect("the shipped Drake has its strands");
        assert!((estimated_bending_stiffness(&cable, 0.2) - 0.2 * max).abs() < 1e-9);
        // A tiny factor: the slipped strands.
        assert_eq!(estimated_bending_stiffness(&cable, 1e-4), min);
        // Without strand counts, no floor.
        cable.outer.strands = None;
        assert!((estimated_bending_stiffness(&cable, 1e-4) - 1e-4 * max).abs() < 1e-12);
        let cable = lib.get("acsr-drake-26-7").unwrap();
        let mut j = crate::tests::example().jumpers[0].clone();
        let mut d = crate::Defaults::default();
        assert!(
            (jumper_bending_stiffness(&j, cable, &d) - DEFAULT_STIFFNESS_FACTOR * max).abs() < 1e-9
        );
        // The project's factor where the jumper gives none.
        d.jumper_stiffness_factor = 0.3;
        assert!((jumper_bending_stiffness(&j, cable, &d) - 0.3 * max).abs() < 1e-9);
        j.stiffness_factor = Some(0.35);
        assert!((jumper_bending_stiffness(&j, cable, &d) - 0.35 * max).abs() < 1e-9);
        j.bending_stiffness = Some(100.0);
        assert_eq!(jumper_bending_stiffness(&j, cable, &d), 100.0);
    }

    /// A bimetallic jumper lengthens like the line's cables: with the
    /// default model (outer strands in compression) at the composite
    /// expansion; as a bird cage, above its temperature, at the core's.
    /// Cooler, both the same. (`--nocapture` prints the comparison.)
    #[test]
    fn a_bimetallic_jumper_follows_the_lines_bimetallic_model() {
        let lib = library();
        let cable = lib.get("acsr-drake-26-7").unwrap().clone();
        let line = crate::tests::example();
        let t0 = line.jumpers[0].temperature;
        let mut bird = line.clone();
        bird.bimetallic.behavior = ols_cables::BimetallicBehavior::BirdCage;
        let core = cable.core.as_ref().unwrap().thermal_expansion;
        let composite = cable.thermal_expansion();
        for t in [-20.0, 50.0, 80.0] {
            let started = std::time::Instant::now();
            let (_, a) = solve_all(&line, &still(t));
            let ta = started.elapsed();
            let started = std::time::Instant::now();
            let (_, b) = solve_all(&bird, &still(t));
            let tb = started.elapsed();
            let cut = a[0].unstressed_length / (composite * (t - t0)).exp();
            assert!(
                (b[0].unstressed_length
                    - cut * (if t > t0 { core } else { composite } * (t - t0)).exp())
                .abs()
                    < 1e-9
            );
            println!(
                "{t} °C: composite L {:.4} m sag {:.4} m ({ta:.1?}); bird cage L {:.4} m sag {:.4} m ({tb:.1?})",
                a[0].unstressed_length, a[0].sag, b[0].unstressed_length, b[0].sag
            );
        }
    }

    /// A nearly straight jumper (hinged; 5·10⁻⁵ longer than its clamps'
    /// distance: ~0.5 mm on ~10.8 m) solves (the old floor, 10⁻³, called
    /// it taut); under the floor it is taut.
    #[test]
    fn a_nearly_straight_jumper_solves_until_it_is_taut() {
        let mut line = crate::tests::example();
        let lib = library();
        let t0 = line.jumpers[0].temperature;
        let shapes = |line: &Line, t: f64| -> Vec<SectionShape> {
            solve_line(line, &lib, &still(t))
                .into_iter()
                .map(Result::unwrap)
                .collect()
        };
        let first = solve_jumpers(&line, &lib, &still(t0), &shapes(&line, t0))
            .remove(0)
            .unwrap();
        let (mut a, mut b) = (v(first.points[0]), v(*first.points.last().unwrap()));
        line.jumpers[0].sag = None;
        line.jumpers[0].bending_stiffness = Some(0.0);
        // Its weight on the clamps moves them a little with its length:
        // the clamps where they settle for a nearly straight jumper.
        for _ in 0..4 {
            line.jumpers[0].length = Some((b - a).norm() * (1.0 + 5e-5));
            let shapes = shapes(&line, t0);
            let ends = |p: &crate::SectionPoint| super::end_position(&line, &shapes, p).unwrap();
            (a, b) = (ends(&line.jumpers[0].from), ends(&line.jumpers[0].to));
        }
        line.jumpers[0].length = Some((b - a).norm() * (1.0 + 5e-5));
        let j = solve_jumpers(&line, &lib, &still(t0), &shapes(&line, t0))
            .remove(0)
            .unwrap();
        assert!(j.sag > 0.0 && j.sag < 0.1, "{}", j.sag);
        // Under the floor (5·10⁻⁶ longer): taut (between the same clamps).
        let between = shapes(&line, t0);
        line.jumpers[0].length = Some((b - a).norm() * (1.0 + 5e-6));
        let err = solve_jumpers(&line, &lib, &still(t0), &between)
            .remove(0)
            .unwrap_err();
        assert!(err.to_string().contains("taut"), "{err}");
    }

    /// The wind blows to the same side whichever way a section or a jumper
    /// was entered: the side is the line's (its direction from the routes).
    #[test]
    fn the_wind_side_does_not_depend_on_the_order_entered() {
        let line = crate::tests::example();
        let mut windy = still(15.0);
        windy.wind_pressure = Stress::from_pascals(500.0);
        let (shapes, jumpers) = solve_all(&line, &windy);
        // S1 entered from its last structure to its first.
        let mut rev = line.clone();
        let s1 = rev.sections.iter_mut().find(|s| s.id == "S1").unwrap();
        s1.points.reverse();
        // Its jumper too, from `to` to `from`.
        let j = &mut rev.jumpers[0];
        std::mem::swap(&mut j.from, &mut j.to);
        std::mem::swap(&mut j.angle_from_deg, &mut j.angle_to_deg);
        let (rshapes, rjumpers) = solve_all(&rev, &windy);
        // Each span's sideways offset from its chord (its centroid against
        // the chord's middle, to the left of the line): the same side, and
        // nearly the same (it is strung from its other end).
        let a = shapes.iter().find(|s| s.section == "S1").unwrap();
        let b = rshapes.iter().find(|s| s.section == "S1").unwrap();
        let n = a.spans.len();
        let offset = |s: &SectionShape, k: usize, left: Vector3<f64>| -> f64 {
            let pts = &s.spans[k].points;
            let c = pts.iter().map(|p| v(*p)).sum::<Vector3<f64>>() / pts.len() as f64;
            let m = 0.5 * (v(pts[0]) + v(*pts.last().unwrap()));
            (c - m).dot(&left)
        };
        for k in 0..n {
            let pts = &a.spans[k].points;
            let d = v(*pts.last().unwrap()) - v(pts[0]);
            let left = Vector3::new(-d.y, d.x, 0.0).normalize();
            let (f, r) = (offset(a, k, left), offset(b, n - 1 - k, left));
            assert!(f.abs() > 0.1, "span {k} is blown: {f}");
            assert!(
                f * r > 0.0 && (f - r).abs() < 0.1 * f.abs(),
                "span {k}: {f} {r}"
            );
        }
        // The jumper's low point: the same place.
        let (p, q) = (jumpers[0].low_point, rjumpers[0].low_point);
        // (reversed before, it swung ~1.3 m the other way; a few mm from S1
        // strung from its other end)
        assert!((v(p) - v(q)).norm() < 0.01, "{p:?} {q:?}");
    }

    /// A section through a jumper's end that did not solve: the jumper
    /// fails, naming it (not solved between data points).
    #[test]
    fn a_jumper_fails_when_a_section_at_its_end_is_not_solved() {
        let line = crate::tests::example();
        let (shapes, _) = solve_all(&line, &still(15.0));
        let j = &line.jumpers[0];
        let end = line
            .sections
            .iter()
            .find(|s| s.points.contains(&j.from))
            .unwrap()
            .id
            .clone();
        let without: Vec<SectionShape> = shapes.into_iter().filter(|s| s.section != end).collect();
        let err = solve_jumpers(&line, &library(), &still(15.0), &without)
            .remove(0)
            .unwrap_err();
        assert!(err.to_string().contains(&format!("section {end}")), "{err}");
    }

    /// A rigid jumper: straight between its solved clamps in every case,
    /// whatever the temperature — never taut; it takes no length or sag.
    #[test]
    fn a_rigid_jumper_is_straight_between_its_clamps() {
        let mut line = crate::tests::example();
        line.jumpers[0].sag = None;
        line.jumpers[0].length = None;
        line.jumpers[0].rigid = true;
        line.validate().unwrap();
        for t in [-40.0, 15.0, 80.0] {
            let (_, j) = solve_all(&line, &still(t));
            let (a, b) = (v(j[0].points[0]), v(*j[0].points.last().unwrap()));
            assert_eq!(j[0].sag, 0.0);
            assert!(((b - a).norm() - j[0].unstressed_length).abs() < 1e-9);
            for q in &j[0].points {
                let q = v(*q);
                let along = (q - a).dot(&(b - a)) / (b - a).norm_squared();
                assert!((q - (a + (b - a) * along)).norm() < 1e-9);
            }
        }
        line.jumpers[0].sag = Some(0.1);
        assert!(
            line.validate()
                .unwrap_err()
                .to_string()
                .contains("no length or sag")
        );
    }

    /// The cut a solve keeps (or makes) for jumper 0, and one made afresh.
    fn kept_and_fresh(line: &Line) -> (f64, f64) {
        let lib = library();
        let jumper = &line.jumpers[0];
        let cable = lib.get(&jumper.cable).unwrap();
        let kept = cut_once(line, &lib, jumper, cable).unwrap();
        let design = design_shapes(line, &lib, jumper).unwrap();
        (kept, cut_length(line, jumper, &design, cable).unwrap())
    }

    /// A jumper given by its sag is cut once, kept for every weather case,
    /// and cut again when what it depends on changes — here a structure at
    /// the far end of a section it joins.
    #[test]
    fn a_jumper_cut_is_kept_until_what_it_depends_on_changes() {
        let mut line = crate::tests::example();
        assert!(line.jumpers[0].length.is_none() && line.jumpers[0].sag.is_some());
        solve_all(&line, &still(15.0));
        let (kept, fresh) = kept_and_fresh(&line);
        assert_eq!(kept, fresh);
        // In another case: the same cut, stretched by the temperature.
        let (_, hot) = solve_all(&line, &still(60.0));
        let alpha = library()
            .get(&line.jumpers[0].cable)
            .unwrap()
            .thermal_expansion();
        let t0 = line.jumpers[0].temperature;
        assert!((hot[0].unstressed_length - kept * (alpha * (60.0 - t0)).exp()).abs() < 1e-12);
        // The last structure of a section at the jumper raised 2 m: its
        // clamps move, the cut follows.
        let s = line
            .sections
            .iter()
            .find(|s| {
                s.points
                    .iter()
                    .any(|p| *p == line.jumpers[0].to || *p == line.jumpers[0].from)
            })
            .unwrap();
        let far = s.points.last().unwrap().structure.clone();
        assert!(far != line.jumpers[0].from.structure);
        let st = line.structures.iter_mut().find(|t| t.id == far).unwrap();
        st.position[2] += 2.0;
        for a in st
            .points
            .iter_mut()
            .map(|p| &mut p.position)
            .chain(st.wires.iter_mut().map(|w| &mut w.position))
        {
            a[2] += 2.0;
        }
        let (kept2, fresh2) = kept_and_fresh(&line);
        assert_eq!(kept2, fresh2);
        assert!(kept2 != kept);
    }

    #[test]
    fn example_tap_jumper_has_its_sag_and_follows_the_clamps() {
        // The example joins the branch (S3, strain at T3) to the main L1
        // wire (S1, suspension clamp at T3) with a jumper of 1.5 m sag.
        let line = crate::tests::example();
        let (shapes, jumpers) = solve_all(&line, &still(15.0));
        let j = &jumpers[0];
        assert!((j.sag - 1.5).abs() < 1e-5, "{}", j.sag);
        // Ends at the solved clamps: S3's first point, S1's T3 point.
        let (first, last) = (v(j.points[0]), v(*j.points.last().unwrap()));
        assert!((first - v(shapes[2].clamps[0])).norm() < 1e-6);
        assert!((last - v(shapes[0].clamps[2])).norm() < 1e-6);
        // Cut once at its temperature (15 °C): in the heat only its thermal
        // stretch changes the length; the clamps move, so the sag is not
        // 1.5 m any more.
        let (_, hot) = solve_all(&line, &still(80.0));
        let lib = library();
        let alpha = lib.get(&line.jumpers[0].cable).unwrap().thermal_expansion();
        let expected = j.unstressed_length * (alpha * (80.0 - line.jumpers[0].temperature)).exp();
        assert!(
            (hot[0].unstressed_length - expected).abs() < 1e-9,
            "{} vs {expected}",
            hot[0].unstressed_length
        );
        assert!((hot[0].sag - 1.5).abs() > 0.01, "{}", hot[0].sag);
        // Wind blows it sideways.
        let mut windy = still(15.0);
        windy.wind_pressure = Stress::from_pascals(500.0);
        let (_, blown) = solve_all(&line, &windy);
        let shift = (v(blown[0].low_point) - v(j.low_point)).xy().norm();
        assert!(shift > 0.2, "{shift}");
    }

    #[test]
    fn given_length_reproduces_the_sag_length() {
        let mut line = crate::tests::example();
        let (_, jumpers) = solve_all(&line, &still(15.0));
        // Cutting the jumper to the length found from the sag gives that sag.
        line.jumpers[0].sag = None;
        line.jumpers[0].length = Some(jumpers[0].unstressed_length);
        let (_, again) = solve_all(&line, &still(15.0));
        // (Its weight on the clamps takes its length from still air when
        // it is given by its sag: the clamps settle a few mm otherwise.)
        assert!((again[0].sag - 1.5).abs() < 0.02, "{}", again[0].sag);
        line.validate().unwrap();
    }

    #[test]
    fn support_string_lifts_the_jumper() {
        let mut line = crate::tests::example();
        let (_, free) = solve_all(&line, &still(15.0));
        let length = free[0].unstressed_length;
        // A 1 m string 2 m above the free jumper's half-length point (its
        // middle point: pieces of equal length), holding it there: the
        // string lifts that point by 1 m. (Above the lowest point instead,
        // which is nearer the lower end, the fraction-0.5 point could not
        // reach without stretching the far half by 23 %: the first version
        // of this test did that.)
        let low = free[0].points[free[0].points.len() / 2];
        line.jumpers[0].sag = None;
        line.jumpers[0].length = Some(length);
        line.jumpers[0].support = Some(JumperSupport {
            point: [low[0], low[1], low[2] + 2.0],
            length: 1.0,
            weight: 100.0,
            fraction: 0.5,
        });
        line.validate().unwrap();
        let (_, held) = solve_all(&line, &still(15.0));
        let s = &held[0].supports[0];
        let (top, clamp, force) = (s.strings[0].top, s.clamp, s.strings[0].force);
        // The string is taut at its length and carries the jumper's weight
        // near its middle: less sag than the free jumper.
        assert!(((v(top) - v(clamp)).norm() - 1.0).abs() < 1e-3);
        assert!(force > 50.0 && force < 5_000.0, "{force}");
        assert!(held[0].sag < free[0].sag);
    }

    /// A jumper's weight hangs on the clamps of the sections at its ends,
    /// in the weather case: heavier with ice; half of it at each end
    /// without supports, the share between an end and its support with
    /// one; the clamps it hangs on sit lower than without it.
    #[test]
    fn a_jumper_weighs_on_the_clamps_it_hangs_on() {
        let mut line = crate::tests::example();
        let lib = library();
        let j = line.jumpers[0].clone();
        let calm = still(15.0);
        let down = |l: &Line, c: &WeatherCase| -> std::collections::BTreeMap<SectionPoint, f64> {
            end_loads(l, c, |id| lib.get(id))
                .into_iter()
                .map(|(k, f)| (k, -f[2]))
                .collect()
        };
        let w = down(&line, &calm);
        let (a, b) = (w[&j.from], w[&j.to]);
        assert!(a > 0.0 && (a - b).abs() < 1e-9, "{a} {b}");
        let mut iced = calm.clone();
        iced.ice_thickness = ols_core::units::Length::from_si(0.02);
        assert!(down(&line, &iced)[&j.from] > 2.0 * a);
        // A support a quarter along: the first end carries an eighth.
        line.jumpers[0].support = Some(JumperSupport {
            point: [0.0, 0.0, 0.0],
            length: 1.0,
            weight: 0.0,
            fraction: 0.25,
        });
        let w = down(&line, &calm);
        assert!((w[&j.from] / (2.0 * a) - 0.125).abs() < 1e-9);
        assert!((w[&j.to] / (2.0 * a) - 0.375).abs() < 1e-9);
        // The clamp it hangs on, lower than without it.
        line.jumpers[0].support = None;
        let (with, _) = solve_all(&line, &calm);
        let mut without = line.clone();
        without.jumpers.clear();
        let (alone, _) = solve_all(&without, &calm);
        let at = |shapes: &[SectionShape]| {
            let (k, sec) = line
                .sections
                .iter()
                .enumerate()
                .find(|(_, s)| s.points.contains(&j.from))
                .unwrap();
            let i = sec.points.iter().position(|p| *p == j.from).unwrap();
            shapes[k].clamps[i]
        };
        assert!(
            at(&with)[2] < at(&alone)[2] - 1e-4,
            "{:?} {:?}",
            at(&with),
            at(&alone)
        );
    }

    /// The jumpers on the clamps: the estimate (the default) takes the
    /// wind on the jumper too, across the line — a push besides the same
    /// weight in a wind case; the jumper's own pull: its end forces carry
    /// its whole weight (still air: their vertical parts add up to it,
    /// their horizontal parts cancel), the sections and jumpers settle in
    /// a few passes, the clamps elsewhere than with the estimate; read
    /// back from the line file.
    #[test]
    fn the_jumpers_pull_on_the_clamps_as_an_option() {
        use crate::JumperLoads;
        let mut line = crate::tests::example();
        let lib = library();
        let j = line.jumpers[0].clone();
        assert_eq!(line.solver.jumper_loads, JumperLoads::Estimate);
        let calm = still(15.0);
        let mut windy = still(15.0);
        windy.wind_pressure = ols_core::units::Stress::from_pascals(600.0);
        let quiet = end_loads(&line, &calm, |id| lib.get(id))[&j.from];
        let blown = end_loads(&line, &windy, |id| lib.get(id))[&j.from];
        assert!(quiet[0] == 0.0 && quiet[1] == 0.0 && quiet[2] < 0.0);
        assert!((blown[2] - quiet[2]).abs() < 1e-9, "the same weight");
        assert!(blown[0].hypot(blown[1]) > 1.0, "a push: {blown:?}");
        // The pull: the jumper's whole weight on its two clamps.
        let (shapes, jumpers) = solve_all(&line, &calm);
        let s = &jumpers[0];
        let [fa, fb] = s.end_forces;
        let cable = lib.get(&j.cable).unwrap();
        let weight = cable.weight().si()
            * s.unstressed_length
            * f64::from(line.jumper_bundle(&j).count.max(1));
        assert!(fa[2] < 0.0 && fb[2] < 0.0, "{fa:?} {fb:?}");
        assert!(
            ((fa[2] + fb[2]).abs() / weight - 1.0).abs() < 0.03,
            "{} of {weight}",
            -(fa[2] + fb[2])
        );
        assert!((fa[0] + fb[0]).abs() < 0.03 * weight && (fa[1] + fb[1]).abs() < 0.03 * weight);
        // Solved with it: settled, the clamp elsewhere than with the
        // estimate.
        let clamp = |shapes: &[SectionShape]| {
            let (k, sec) = line
                .sections
                .iter()
                .enumerate()
                .find(|(_, s)| s.points.contains(&j.from))
                .unwrap();
            let i = sec.points.iter().position(|p| *p == j.from).unwrap();
            shapes[k].clamps[i]
        };
        let estimated = clamp(&shapes);
        line.solver.jumper_loads = JumperLoads::Pull;
        let mut results = solve_line(&line, &lib, &calm);
        let pulled: Vec<SectionShape> = results.iter().map(|r| r.clone().unwrap()).collect();
        let moved = clamp(&pulled);
        assert!(
            (0..3)
                .map(|k| (moved[k] - estimated[k]).powi(2))
                .sum::<f64>()
                .sqrt()
                > 1e-5,
            "{moved:?} {estimated:?}"
        );
        // Settled: one more pass changes nothing.
        let estimate = end_loads(&line, &calm, |id| lib.get(id));
        let before = results.clone();
        let used = crate::solve::jumper_pull(
            &line,
            &lib,
            &calm,
            &ols_wirefe::default_fe_budget(),
            &estimate,
            estimate.clone(),
            &mut results,
        );
        let again = crate::solve::jumper_pull(
            &line,
            &lib,
            &calm,
            &ols_wirefe::default_fe_budget(),
            &estimate,
            used,
            &mut results,
        );
        assert!(!again.is_empty());
        let after: Vec<SectionShape> = results.iter().map(|r| r.clone().unwrap()).collect();
        let first: Vec<SectionShape> = before.iter().map(|r| r.clone().unwrap()).collect();
        assert!((0..3).all(|k| (clamp(&after)[k] - clamp(&first)[k]).abs() < 1e-3));
        // Written and read back with the line.
        let text = serde_json::to_string(&line).unwrap();
        assert!(text.contains("\"jumper_loads\":\"pull\""), "{text}");
        let back: Line = serde_json::from_str(&text).unwrap();
        assert_eq!(back.solver.jumper_loads, JumperLoads::Pull);
        assert_eq!(JumperLoads::parse("estimate"), Some(JumperLoads::Estimate));
    }

    #[test]
    fn a_bundled_jumper_shares_its_support_weight() {
        // As above, then the jumper as a 3-bundle under a string 3 × as
        // heavy: the same shape (per subconductor), 3 × the string force.
        let mut line = crate::tests::example();
        let (_, free) = solve_all(&line, &still(15.0));
        let low = free[0].points[free[0].points.len() / 2];
        line.jumpers[0].sag = None;
        line.jumpers[0].length = Some(free[0].unstressed_length);
        line.jumpers[0].support = Some(JumperSupport {
            point: [low[0], low[1], low[2] + 2.0],
            length: 1.0,
            weight: 100.0,
            fraction: 0.5,
        });
        let (shapes, one) = solve_all(&line, &still(15.0));
        line.jumpers[0].bundle = Some(3);
        line.jumpers[0].bundle_spacing = Some(0.4);
        line.jumpers[0].support.as_mut().unwrap().weight = 300.0;
        line.validate().unwrap();
        // Between the same clamps (three times the jumper weighs more on
        // them, which moves them a little).
        let three: Vec<JumperShape> = solve_jumpers(&line, &library(), &still(15.0), &shapes)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        for (p, q) in one[0].points.iter().zip(&three[0].points) {
            assert!((v(*p) - v(*q)).norm() < 1e-9);
        }
        let (f1, f3) = (
            one[0].supports[0].strings[0].force,
            three[0].supports[0].strings[0].force,
        );
        assert!((3.0 * f1 - f3).abs() < 1e-6 * f3, "{f1} {f3}");
    }

    #[test]
    fn bad_jumpers_are_rejected() {
        let mut line = crate::tests::example();
        line.jumpers[0].length = Some(10.0);
        assert!(line.validate().unwrap_err().to_string().contains("either"));
        let mut line = crate::tests::example();
        line.jumpers[0].to = line.jumpers[0].from.clone();
        assert!(line.validate().is_err());
        let mut line = crate::tests::example();
        line.jumpers[0].sag = Some(0.0001);
        let lib = library();
        let shapes: Vec<_> = solve_line(&line, &lib, &still(15.0))
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let err = solve_jumpers(&line, &lib, &still(15.0), &shapes).remove(0);
        assert!(err.unwrap_err().to_string().contains("taut"));
    }

    /// The example's dead-end jumper leaves its clamp at the set angle
    /// below the conductor and swings less in the wind than with hinged
    /// ends (EI = 0, the jumper before bending stiffness).
    #[test]
    fn a_stiff_jumper_leaves_its_clamp_at_its_angle_and_swings_less() {
        let mut line = crate::tests::example();
        let mut windy = still(15.0);
        windy.wind_pressure = Stress::from_pascals(500.0);
        let moved = |line: &Line| {
            let (_, a) = solve_all(line, &still(15.0));
            let (_, b) = solve_all(line, &windy);
            let m = a[0].points.len() / 2;
            (v(b[0].points[m]) - v(a[0].points[m])).norm()
        };
        line.jumpers[0].angle_from_deg = Some(45.0);
        line.jumpers[0].angle_to_deg = Some(45.0);
        line.validate().unwrap();
        let (shapes, jumpers) = solve_all(&line, &still(15.0));
        // From its tension string's line (clamp to top).
        let clamp = end_position(&line, &shapes, &line.jumpers[0].from).unwrap();
        let axis = string_axis(&line, &line.jumpers[0].from, clamp).unwrap();
        // The tangent at the clamp, from the first two pieces.
        let p = &jumpers[0].points;
        let (d0, d1) = (
            (v(p[1]) - v(p[0])).normalize(),
            (v(p[2]) - v(p[1])).normalize(),
        );
        let first = d0 * 1.5 - d1 * 0.5;
        let elevation = |d: Vector3<f64>| d.z.atan2(d.xy().norm()).to_degrees();
        let below = elevation(axis) - elevation(first);
        assert!((below - 45.0).abs() < 3.0, "{below}");
        let stiff = moved(&line);
        line.jumpers[0].bending_stiffness = Some(0.0);
        let hinged = moved(&line);
        // One clamped end (the other a tap on a through wire) on a 10.8 m
        // jumper: ~10 % less (1.31 m against 1.45 m when written).
        assert!(stiff < 0.95 * hinged, "{stiff} vs {hinged}");
    }

    /// A clamp holds its jumper at its angle from its tension string's line
    /// (clamp to the string's top), turned within the conductor's plane:
    /// down in still air; towards the wind's side as the wind blows (the
    /// conductor swings); a steeper string, a steeper jumper.
    #[test]
    fn a_jumper_leaves_at_its_angle_from_its_tension_string() {
        let line = crate::tests::example();
        let (shapes, _) = solve_all(&line, &still(15.0));
        let j = &line.jumpers[0];
        let clamp = end_position(&line, &shapes, &j.from).unwrap();
        let s = string_axis(&line, &j.from, clamp).expect("a tension string");
        let down = Vector3::new(0.0, 0.0, -15.0);
        let d = clamp_direction(&line, &shapes, &j.from, 30.0, down).unwrap();
        assert!((d.angle(&s).to_degrees() - 30.0).abs() < 1e-9);
        // In the vertical plane through the string, below it.
        let n = s.cross(&Vector3::z());
        assert!(d.dot(&n).abs() < 1e-9 && d.z < s.z, "{d:?} {s:?}");
        // Wind across the string: turned towards where the load pushes.
        let across = Vector3::new(-s.y, s.x, 0.0).normalize();
        let windy = clamp_direction(&line, &shapes, &j.from, 30.0, down + across * 8.0).unwrap();
        assert!((windy.angle(&s).to_degrees() - 30.0).abs() < 1e-9);
        assert!(windy.dot(&across) > 0.1, "{windy:?}");
        // 0°: along the string.
        let along = clamp_direction(&line, &shapes, &j.from, 0.0, down).unwrap();
        assert!(along.angle(&s) < 1e-9);
        // The string's top 2 m higher: the jumper leaves steeper upwards.
        let mut steep = line.clone();
        let st = steep.structure(&j.from.structure).unwrap();
        let w = st.wire_at_set(j.from.set).unwrap().label.clone();
        let top = st.strings_to(&w).next().unwrap().top.clone();
        let st = steep
            .structures
            .iter_mut()
            .find(|x| x.id == j.from.structure)
            .unwrap();
        st.points
            .iter_mut()
            .find(|p| p.label == top)
            .unwrap()
            .position[2] += 2.0;
        let up = clamp_direction(&steep, &shapes, &j.from, 30.0, down).unwrap();
        assert!(up.z > d.z, "{} vs {}", up.z, d.z);
    }

    /// The example's jumper cut to its length, with strings on T3 for
    /// support clamps: for each `(label, at, legs)`, a wire point `label`
    /// 1 m above the free jumper's point at `at` of its length, hanging
    /// on one string 1 m long (I) or two from 0.4 m either side across the
    /// jumper and 1 m up (V, legs √1.16 m). Each string weighs 100 N. Also the free
    /// jumper.
    fn with_clamps(clamps: &[(&str, f64, bool)]) -> (Line, JumperShape) {
        use crate::{InsulatorString, JumperClamp, StructurePoint, WirePoint};
        let mut line = crate::tests::example();
        let (_, free) = solve_all(&line, &still(15.0));
        let free = free[0].clone();
        let (a, b) = (v(free.points[0]), v(*free.points.last().unwrap()));
        let across = Vector3::new(a.y - b.y, b.x - a.x, 0.0).normalize();
        line.jumpers[0].sag = None;
        line.jumpers[0].length = Some(free.unstressed_length);
        let st = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        for &(label, at, v_string) in clamps {
            let k = ((free.points.len() - 1) as f64 * at).round() as usize;
            let q = v(free.points[k]);
            let w = q + Vector3::new(0.0, 0.0, 1.0);
            st.wires.push(WirePoint {
                description: Default::default(),
                label: label.into(),
                position: p(w),
                set: None,
                weight: 0.0,
            });
            let tops: Vec<Vector3<f64>> = if v_string {
                let up = Vector3::new(0.0, 0.0, 1.0);
                vec![w + across * 0.4 + up, w - across * 0.4 + up]
            } else {
                vec![w + Vector3::new(0.0, 0.0, 1.0)]
            };
            for (n, top) in tops.into_iter().enumerate() {
                let top_label = format!("{label}-top{n}");
                st.points.push(StructurePoint {
                    description: Default::default(),
                    label: top_label.clone(),
                    position: p(top),
                });
                st.strings.push(InsulatorString {
                    label: format!("{label}-string{n}"),
                    top: top_label,
                    wire: label.into(),
                    weight: 100.0,
                    ..Default::default()
                });
            }
            line.jumpers[0].supports.push(JumperClamp::new(label, at));
        }
        line.validate().unwrap();
        (line, free)
    }

    /// The clamp's piece in a solved jumper: straight through, its length
    /// as given, its middle the clamp. The angle between the directions
    /// in and out, radians.
    fn through_the_clamp(j: &JumperShape, s: &JumperSupportShape, length: f64) -> f64 {
        let k = j
            .points
            .windows(2)
            .position(|w| ((v(w[0]) + v(w[1])) * 0.5 - v(s.clamp)).norm() < 1e-9)
            .expect("a piece with its middle at the clamp");
        let piece = v(j.points[k + 1]) - v(j.points[k]);
        assert!((piece.norm() - length).abs() < 1e-9, "{}", piece.norm());
        let d = v(s.direction);
        assert!((d.norm() - 1.0).abs() < 1e-12);
        piece.normalize().angle(&d)
    }

    /// One I-held clamp in the middle of the example's jumper (stiff, and
    /// hinged with EI = 0): the string at its length, carrying weight; the
    /// jumper straight through the clamp and less sagged than free.
    #[test]
    fn a_support_clamp_holds_the_jumper_straight_through_it() {
        let (mut line, free) = with_clamps(&[("JW", 0.5, false)]);
        for ei in [None, Some(0.0)] {
            line.jumpers[0].bending_stiffness = ei;
            let started = std::time::Instant::now();
            let (_, held) = solve_all(&line, &still(15.0));
            let took = started.elapsed();
            let j = &held[0];
            assert_eq!(j.supports.len(), 1);
            let s = &j.supports[0];
            assert_eq!((s.structure.as_str(), s.wire.as_str()), ("T3", "JW"));
            assert_eq!(s.strings.len(), 1);
            let string = &s.strings[0];
            assert_eq!(string.label, "JW-string0");
            assert!(((v(string.top) - v(s.clamp)).norm() - 1.0).abs() < 1e-9);
            assert!(string.force > 50.0, "{}", string.force);
            let angle = through_the_clamp(j, s, crate::DEFAULT_CLAMP_LENGTH);
            assert!(angle < 1e-9, "{angle}");
            assert!(j.sag < free.sag, "{} vs {}", j.sag, free.sag);
            println!(
                "EI {ei:?}: string {:.1} N, sag {:.3} m (free {:.3} m), direction error {angle:.1e} rad, {took:.1?}",
                string.force, j.sag, free.sag
            );
        }
    }

    /// Two I-held clamps, each straight through, each string taut at its
    /// length; the jumper in order from `from` through both.
    #[test]
    fn two_support_clamps_hold_the_jumper() {
        let (line, _) = with_clamps(&[("JA", 0.3, false), ("JB", 0.7, false)]);
        let (_, held) = solve_all(&line, &still(15.0));
        let j = &held[0];
        assert_eq!(j.supports.len(), 2);
        for s in &j.supports {
            assert!(through_the_clamp(j, s, 0.1) < 1e-9);
            let string = &s.strings[0];
            assert!(((v(string.top) - v(s.clamp)).norm() - 1.0).abs() < 1e-9);
            assert!(string.force > 0.0, "{}", string.force);
        }
        // In order along the jumper.
        let along = |s: &JumperSupportShape| {
            j.points
                .iter()
                .position(|q| (v(*q) - v(s.clamp)).norm() < 0.06)
                .unwrap()
        };
        assert!(along(&j.supports[0]) < along(&j.supports[1]));
    }

    /// A V-held clamp: both legs hold in still air; in a strong wind
    /// across the clamp swings and the leg to leeward goes slack (no
    /// string pushes), the jumper still straight through the clamp.
    #[test]
    fn a_v_held_clamp_lets_a_leg_go_slack_in_the_wind() {
        let (line, _) = with_clamps(&[("JV", 0.5, true)]);
        let (_, calm) = solve_all(&line, &still(15.0));
        let s = &calm[0].supports[0];
        assert_eq!(s.strings.len(), 2);
        assert!(s.strings.iter().all(|x| x.force > 1.0), "{:?}", s.strings);
        let mut windy = still(15.0);
        windy.wind_pressure = Stress::from_pascals(800.0);
        let started = std::time::Instant::now();
        let (_, blown) = solve_all(&line, &windy);
        let took = started.elapsed();
        let w = &blown[0].supports[0];
        let forces: Vec<f64> = w.strings.iter().map(|x| x.force).collect();
        assert!(
            forces.iter().filter(|f| **f == 0.0).count() == 1 && forces.iter().any(|f| *f > 1.0),
            "{forces:?}"
        );
        for x in &w.strings {
            let d = (v(x.top) - v(w.clamp)).norm();
            if x.force > 0.0 {
                assert!((d - 1.16_f64.sqrt()).abs() < 1e-9, "{d}");
            } else {
                assert!(d < 1.16_f64.sqrt(), "{d}");
            }
        }
        assert!((v(w.clamp) - v(s.clamp)).xy().norm() > 0.05);
        assert!(through_the_clamp(&blown[0], w, 0.1) < 1e-9);
        println!("V in the wind: {forces:?} N, {took:.1?}");
    }

    /// The wind on a helping string's wind area pushes its clamp further
    /// across (half the string's area at the clamp), the string still at
    /// its length.
    #[test]
    fn the_wind_on_a_helping_string_swings_it_further() {
        let (mut line, _) = with_clamps(&[("JW", 0.5, false)]);
        let mut windy = still(15.0);
        windy.wind_pressure = Stress::from_pascals(600.0);
        let (_, calm) = solve_all(&line, &still(15.0));
        let (_, bare) = solve_all(&line, &windy);
        let structure = line.jumpers[0].supports[0]
            .structure_id(&line.jumpers[0])
            .to_owned();
        let s = line
            .structures
            .iter_mut()
            .find(|s| s.id == structure)
            .unwrap();
        for x in s.strings.iter_mut().filter(|x| x.wire == "JW") {
            x.wind_area = 0.5;
        }
        let (_, blown) = solve_all(&line, &windy);
        let at = |j: &JumperShape| v(j.supports[0].clamp);
        let rest = at(&calm[0]);
        let (a, b) = (
            (at(&bare[0]) - rest).xy().norm(),
            (at(&blown[0]) - rest).xy().norm(),
        );
        assert!(
            b > a + 0.01,
            "without {a:.3} m, with its wind area {b:.3} m"
        );
        let x = &blown[0].supports[0].strings[0];
        assert!(x.force > 0.0);
        assert!(through_the_clamp(&blown[0], &blown[0].supports[0], 0.1) < 1e-9);
    }

    /// A V helping string whose legs may take compression keeps its
    /// clamp at both legs' lengths: one leg may push (negative force);
    /// tension-only legs may go slack instead.
    #[test]
    fn a_v_helping_string_that_may_push_holds_its_clamp() {
        let (mut line, _) = with_clamps(&[("JV", 0.5, true)]);
        let mut windy = still(15.0);
        windy.wind_pressure = Stress::from_pascals(800.0);
        let st = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        for x in st.strings.iter_mut().filter(|x| x.wire == "JV") {
            x.compression = Some(50_000.0);
        }
        let (_, blown) = solve_all(&line, &windy);
        let s = &blown[0].supports[0];
        let forces: Vec<f64> = s.strings.iter().map(|x| x.force).collect();
        assert!(
            forces.iter().any(|f| *f < -1.0) && forces.iter().any(|f| *f > 1.0),
            "{forces:?}"
        );
        for x in &s.strings {
            assert!(((v(x.top) - v(s.clamp)).norm() - 1.16_f64.sqrt()).abs() < 1e-6);
        }
    }

    /// Support clamps that do not fit: each refused with what is wrong.
    #[test]
    fn bad_support_clamps_are_rejected() {
        use crate::{InsulatorString, JumperSupport, WirePoint};
        let (line, _) = with_clamps(&[("JW", 0.5, false)]);
        let err = |f: &dyn Fn(&mut Line)| {
            let mut l = line.clone();
            f(&mut l);
            l.validate().unwrap_err().to_string()
        };
        assert!(
            err(&|l| {
                l.jumpers[0].support = Some(JumperSupport {
                    point: [0.0; 3],
                    length: 1.0,
                    weight: 0.0,
                    fraction: 0.5,
                })
            })
            .contains("not both")
        );
        assert!(
            err(&|l| {
                l.jumpers[0].length = None;
                l.jumpers[0].sag = Some(1.0);
            })
            .contains("needs a length")
        );
        assert!(err(&|l| l.jumpers[0].supports[0].at = 1.0).contains("between 0 and 1"));
        assert!(
            err(&|l| l.jumpers[0].supports[0].at = 0.001).contains("overlaps the jumper's start")
        );
        assert!(err(&|l| l.jumpers[0].supports[0].clamp_length = 0.0).contains("above 0"));
        assert!(
            err(&|l| {
                let c = l.jumpers[0].supports[0].clone();
                l.jumpers[0]
                    .supports
                    .push(crate::JumperClamp { at: 0.4, ..c });
            })
            .contains("overlaps clamp 1")
        );
        assert!(
            err(&|l| l.jumpers[0].supports[0].wire = "nothing".into())
                .contains("no wire point \"nothing\"")
        );
        assert!(
            err(&|l| l.jumpers[0].supports[0].structure = Some("T9".into()))
                .contains("unknown structure T9")
        );
        // A section's wire point (with a set).
        assert!(err(&|l| l.jumpers[0].supports[0].wire = "W1".into()).contains("has set"));
        fn t3(l: &mut Line) -> &mut crate::Structure {
            l.structures.iter_mut().find(|s| s.id == "T3").unwrap()
        }
        // Three strings.
        assert!(
            err(&|l| {
                let st = t3(l);
                for k in 1..3 {
                    let mut s = st.strings.last().unwrap().clone();
                    s.label = format!("extra{k}");
                    st.strings.push(s);
                }
            })
            .contains("3 strings")
        );
        // Hanging below another string.
        assert!(
            err(&|l| {
                let st = t3(l);
                let jw = st.wire("JW").unwrap().position;
                st.wires.push(WirePoint {
                    description: Default::default(),
                    label: "JX".into(),
                    position: [jw[0], jw[1], jw[2] - 0.5],
                    set: None,
                    weight: 0.0,
                });
                st.strings.push(InsulatorString {
                    label: "below".into(),
                    top: "JW".into(),
                    wire: "JX".into(),
                    ..Default::default()
                });
                l.jumpers[0].supports[0].wire = "JX".into();
            })
            .contains("not a structure point")
        );
    }
}
