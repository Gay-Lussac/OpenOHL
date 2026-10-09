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

use crate::rod::{Rod, RodSupport};
use crate::solve::{DEFAULT_INSULATOR_STIFFNESS, SectionShape, p, span_load, v};
use crate::{Jumper, Line, LineError, Point, SectionPoint};

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
    /// Support string: its top and the clamp on the jumper, and its force, N.
    pub support: Option<(Point, Point, f64)>,
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
    line.attachment(point)
        .map(|a| v(a.wire_point))
        .ok_or_else(|| LineError::Invalid(format!("no attachment {point:?}")))
}

/// Angle of a jumper terminal below the conductor's axis when the jumper
/// gives none, degrees. Terminals angled down are usual on dead-end
/// clamps; a terminal straight on along a span rising to the structure
/// points the jumper upwards, and the jumper then swings further in the
/// wind than a hinged one (the bend at the clamps eases as it swings).
pub const DEFAULT_TERMINAL_ANGLE_DEG: f64 = 30.0;

/// The direction a section's end clamp holds the jumper in: the
/// conductor's axis out of the section (the span's tangent at the clamp,
/// continued), turned down by the terminal angle; `None` when `point` is
/// not the end of a solved section.
fn clamp_direction(
    line: &Line,
    shapes: &[SectionShape],
    point: &SectionPoint,
    terminal_deg: f64,
) -> Option<Vector3<f64>> {
    let axis = conductor_axis(line, shapes, point)?;
    let flat = axis.xy().norm();
    if flat < 1e-9 {
        return Some(axis);
    }
    let down = axis.z.atan2(flat) - terminal_deg.to_radians();
    let h = axis.xy() / flat;
    Some(Vector3::new(h.x * down.cos(), h.y * down.cos(), down.sin()))
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

/// Bending stiffness EI of a cable, N·m², when the jumper gives none: the
/// stiffness of its wires bending each on its own (they slip on each
/// other), \( \sum E A d^2 / 16 \), with a wire diameter d of a sixth of
/// the cable's (the wires' sizes are not in the wire library). Drake
/// 26/7: ~48 N·m² (from its real wires: ~37 N·m²); a cable bent with its
/// wires stuck together is up to tens of times stiffer.
pub fn estimated_bending_stiffness(cable: &Cable) -> f64 {
    let d = cable.diameter.si() / 6.0;
    cable.axial_stiffness().si() * d * d / 16.0
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
    let mut low = chord * (1.0 + 1e-5);
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

    /// Solves the rod of `length` under `load` between `a` and `b` (a
    /// support string at a fraction of the length),
    /// starting from the hinged catenary; if that does not converge, the
    /// bending stiffness is brought in in steps.
    fn solve(
        &self,
        a: Vector3<f64>,
        b: Vector3<f64>,
        length: f64,
        load: Vector3<f64>,
        support: Option<(RodSupport, f64)>,
        ea: f64,
    ) -> Result<crate::rod::RodShape, String> {
        let n = Self::pieces(length);
        let props = CableProperties {
            unstressed_length: length,
            axial_stiffness: ea,
            load,
        };
        let hinged = element_shape(&props, a, b - a)?.0;
        let mut start: Vec<Vector3<f64>> = resample(&hinged, n)
            .windows(2)
            .map(|w| w[1] - w[0])
            .collect();
        // The string's node: the nearest to its fraction of the length.
        let support = support.map(|(s, fraction)| RodSupport {
            node: (fraction * n as f64).round().clamp(1.0, (n - 1) as f64) as usize,
            ..s
        });
        let rod = |ei: f64| Rod {
            a,
            b,
            length,
            n,
            q: load,
            ei,
            start: self.start,
            end: self.end,
            support,
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
        let shape = stiff.solve(a, b, l0, still.load, None, still.axial_stiffness)?;
        Ok(sag_below_chord(&shape.points, a, b) - sag)
    };
    let (mut lo, mut hi) = (chord * (1.0 + 1e-4), hinged);
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
) -> Option<Stiff> {
    let ei = jumper
        .bending_stiffness
        .unwrap_or_else(|| estimated_bending_stiffness(cable));
    let stiff = Stiff {
        start: clamp_direction(
            line,
            shapes,
            &jumper.from,
            jumper.angle_from_deg.unwrap_or(DEFAULT_TERMINAL_ANGLE_DEG),
        ),
        // Arriving at the clamp: against the direction it holds.
        end: clamp_direction(
            line,
            shapes,
            &jumper.to,
            jumper.angle_to_deg.unwrap_or(DEFAULT_TERMINAL_ANGLE_DEG),
        )
        .map(|d| -d),
        ei,
    };
    (ei > 0.0 && (stiff.start.is_some() || stiff.end.is_some())).then_some(stiff)
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
    match stiffness(line, jumper, shapes, cable) {
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
    let a = end_position(line, shapes, &jumper.from).map_err(|e| invalid(e.to_string()))?;
    let b = end_position(line, shapes, &jumper.to).map_err(|e| invalid(e.to_string()))?;
    let d = b - a;
    if d.xy().norm() < 0.1 {
        return Err(invalid(
            "ends less than 0.1 m apart horizontally (a vertical jumper is not supported yet)"
                .into(),
        ));
    }
    let ea = cable.axial_stiffness().si();
    let stiff = stiffness(line, jumper, shapes, cable);
    let reference = match (jumper.length, jumper.sag) {
        (Some(length), _) => length,
        // Given by its sag: cut in this case (the design case; other cases
        // get the length cut there, see [`solve_jumpers`]).
        (None, Some(_)) => cut_length(line, jumper, shapes, cable)?,
        (None, None) => return Err(invalid("needs a length or a sag".into())),
    };
    let l0 = reference
        * (cable.thermal_expansion() * (case.temperature.celsius() - jumper.temperature)).exp();
    let load = span_load(case, cable, d);
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
    if d.norm() >= l0 * (1.0 - 1e-3) {
        return Err(invalid(format!(
            "taut: {l0:.2} m long between clamps {:.2} m apart in this weather case (the \
             clamps swing apart; a jumper holding them back is not modelled yet)",
            d.norm()
        )));
    }
    if let Some(stiff) = &stiff {
        // Half the string's weight per wire on the jumper.
        let support = jumper.support.as_ref().map(|s| {
            let string = RodSupport {
                top: v(s.point),
                length: s.length,
                node: 0,
                weight: s.weight / f64::from(bundle) / 2.0,
            };
            (string, s.fraction)
        });
        let shape = stiff.solve(a, b, l0, load, support, ea).map_err(invalid)?;
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
            support: shape
                .support
                .map(|(clamp, force)| (s_point(jumper), p(clamp), force * f64::from(bundle))),
        });
    }
    let (points, max_tension, support) = match &jumper.support {
        None => {
            let (points, t) = element_shape(&props, a, d).map_err(invalid)?;
            (points, t, None)
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
                axial_stiffness: DEFAULT_INSULATOR_STIFFNESS,
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
            for (e, element) in model.cables.iter().enumerate() {
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
            (
                points,
                max_tension,
                // The string's force for the whole bundle.
                Some((
                    s.point,
                    p(pos[3]),
                    solution.truss_forces[0] * f64::from(bundle),
                )),
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
        support,
    })
}

fn s_point(jumper: &Jumper) -> Point {
    jumper.support.as_ref().map_or([0.0; 3], |s| s.point)
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
    line.jumpers
        .iter()
        .map(|jumper| solve_one_jumper(line, library, case, shapes, jumper))
        .collect()
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
        for a in &mut st.attachments {
            a.insulator_point[2] += 2.0;
            a.wire_point[2] += 2.0;
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
        assert!((again[0].sag - 1.5).abs() < 1e-6, "{}", again[0].sag);
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
        let (top, clamp, force) = held[0].support.unwrap();
        // The string is taut at its length and carries the jumper's weight
        // near its middle: less sag than the free jumper.
        assert!(((v(top) - v(clamp)).norm() - 1.0).abs() < 1e-3);
        assert!(force > 50.0 && force < 5_000.0, "{force}");
        assert!(held[0].sag < free[0].sag);
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
        let (_, one) = solve_all(&line, &still(15.0));
        line.jumpers[0].bundle = Some(3);
        line.jumpers[0].bundle_spacing = Some(0.4);
        line.jumpers[0].support.as_mut().unwrap().weight = 300.0;
        line.validate().unwrap();
        let (_, three) = solve_all(&line, &still(15.0));
        for (p, q) in one[0].points.iter().zip(&three[0].points) {
            assert!((v(*p) - v(*q)).norm() < 1e-9);
        }
        let (f1, f3) = (one[0].support.unwrap().2, three[0].support.unwrap().2);
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
        let axis = conductor_axis(&line, &shapes, &line.jumpers[0].from).unwrap();
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
}
