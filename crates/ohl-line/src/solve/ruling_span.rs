// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The cable's permanent state (creep, load stretch), the section strung,
//! the ruling-span solution, the tensions above the rated tensile strength.

use super::*;

/// The cable's permanent state in a condition: its material state (for
/// the non-linear model) and its permanent stretch, the extra unit length
/// at zero tension compared with the initial state.
#[derive(Debug, Clone)]
pub struct FinalState {
    /// Material state of the non-linear model (`Initial` for the linear
    /// models: their stretch is all in `stretch`).
    pub material: ohl_cables::mechanics::MaterialState,
    /// Permanent stretch, unit strain.
    pub stretch: f64,
    /// Weather case that gave it.
    pub case: String,
}

/// The total load per length of `cable` in `case`, N/m (weight, ice,
/// wind).
pub(super) fn load_magnitude(case: &WeatherCase, cable: &Cable) -> f64 {
    span_load(case, cable, Vector3::new(1.0, 0.0, 0.0)).norm()
}

/// `case` as a sag-tension load state: the resultant per metre, of which
/// the cable's own weight (conserved by the sag-tension solvers, per metre
/// of the cable as strung) and the wind.
pub(crate) fn load_state(case: &WeatherCase, cable: &Cable) -> ohl_sagtension::LoadState {
    use ohl_core::units::ForcePerLength;
    let load = case.wire_load(cable.diameter, cable.weight());
    ohl_sagtension::LoadState {
        load: ForcePerLength::from_si(load_magnitude(case, cable)),
        weight: cable.weight(),
        wind: load.horizontal,
        temperature: case.temperature,
    }
}

/// A section as strung: span geometries, clamps, stringing state,
/// horizontal tension, N.
pub(super) type Strung = (
    Vec<ohl_sagtension::SpanGeometry>,
    Vec<Vector3<f64>>,
    ohl_sagtension::LoadState,
    f64,
);

/// The section as strung, for the sag-tension model: its spans (between
/// the clamps hanging in still air at the stringing temperature), the
/// stringing state and the horizontal tension then (the mean of the
/// spans', from the finite-element model).
pub(crate) fn strung(
    line: &Line,
    section: &Section,
    cable: &Cable,
    budget: &Budget,
) -> Result<Strung, LineError> {
    use ohl_core::units::{Length, Temperature};
    let (layout, _) = estimate_strain_weights(section, holders_of(line, section)?, cable, budget)?;
    let reference = reference(section, &layout, cable, budget)?;
    let still = WeatherCase::still(
        "stringing",
        Temperature::from_celsius(reference.temperature),
    );
    let (built, solution) = solve_built(
        section,
        &layout,
        &reference,
        cable,
        Cases::one(&still),
        budget,
        None,
    )?;
    let clamps: Vec<Vector3<f64>> = built
        .clamps
        .iter()
        .map(|&c| solution.positions[c])
        .collect();
    let h = built
        .span_elements
        .iter()
        .map(|e| solution.cables[e[0]].horizontal)
        .sum::<f64>()
        / built.span_elements.len().max(1) as f64;
    let spans = clamps
        .windows(2)
        .map(|w| {
            let d = w[1] - w[0];
            ohl_sagtension::SpanGeometry::inclined(
                Length::from_si(d.xy().norm()),
                Length::from_si(d.z),
            )
        })
        .collect();
    let state = ohl_sagtension::LoadState::bare(
        cable.weight(),
        Temperature::from_celsius(reference.temperature),
    );
    Ok((spans, clamps, state, h))
}

/// The cable's permanent state after creep or after load
/// ([`crate::condition`]), from the line's creep-stretch weather cases.
///
/// - Non-linear model: after creep, the permanent set of the creep curve
///   at the creep case; after load, the longest permanent set of the
///   initial curve among the load cases.
/// - Linear, creep ∝ tension: the strain of the temperature shift at the
///   shift tension, scaled by the creep case's mean tension; no stretch
///   after load (elastic).
/// - Linear, temperature: the strain of the temperature shift; no stretch
///   after load.
///
/// Expected cost: one still-air section solve and a few sag-tension
/// solves (~1 ms each for 10 spans).
pub fn final_state(
    line: &Line,
    section: &Section,
    cable: &Cable,
    condition: crate::condition::Condition,
    budget: &Budget,
) -> Result<FinalState, LineError> {
    use crate::condition::Condition;
    use ohl_cables::CableModel;
    let cable = &*line.bimetallic.apply(cable);
    let cable = &*strung_cable(section, cable)?;
    use ohl_cables::mechanics::{MaterialState, Mechanics};
    use ohl_core::units::Force;
    use ohl_sagtension::nonlinear::NonlinearSection;
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let st = |e: ohl_sagtension::SagTensionError| invalid(e.to_string());
    let cases = &line.condition_cases;
    // The section's local winds (roadmap O15): its largest factor, the
    // permanent stretch being one for the whole section (on the safe side:
    // more stretch, more sag).
    let local = section.largest_wind_factor();
    let state_of = |c: &WeatherCase| load_state(&c.with_wind_factor(local), cable);
    let (spans, _, strung_state, h) = strung(line, section, cable, budget)?;
    let mechanics = Mechanics::from_cable(cable).map_err(|e| invalid(e.to_string()))?;
    let ns = NonlinearSection::from_known_state(
        spans,
        mechanics.clone(),
        strung_state,
        Force::from_si(h),
        &MaterialState::Initial,
    )
    .map_err(st)?;
    // The permanent stretch is measured at the stringing tension: at zero
    // tension, outer strands that take no compression are slack and only
    // the core's set would count.
    let working_strain = |m: &MaterialState| {
        mechanics
            .strain(Force::from_si(h), strung_state.temperature, m)
            .map_err(|e| invalid(e.to_string()))
    };
    match condition {
        Condition::Creep => {
            let case = cases.creep.as_ref().ok_or_else(|| {
                invalid("no weather case for final after creep (Criteria › Creep-Stretch)".into())
            })?;
            let alpha = cable.thermal_expansion();
            match cable.model {
                CableModel::Nonlinear => {
                    let crept = ns
                        .solve(state_of(case), &MaterialState::Creep, budget)
                        .map_err(st)?;
                    let material = mechanics
                        .permanent_set(
                            Force::from_si(NonlinearSection::section_mean_tension(&crept)),
                            case.temperature,
                            &MaterialState::Creep,
                        )
                        .map_err(|e| invalid(e.to_string()))?;
                    let stretch =
                        working_strain(&material)? - working_strain(&MaterialState::Initial)?;
                    Ok(FinalState {
                        material,
                        stretch: stretch.max(0.0),
                        case: case.name.clone(),
                    })
                }
                CableModel::LinearCreepTemperature => Ok(FinalState {
                    material: MaterialState::Initial,
                    stretch: alpha * cable.creep_temperature_shift.unwrap_or(0.0),
                    case: case.name.clone(),
                }),
                CableModel::LinearCreepTension => {
                    let solved = ns
                        .solve(state_of(case), &MaterialState::Initial, budget)
                        .map_err(st)?;
                    let t = NonlinearSection::section_mean_tension(&solved);
                    let at = cable.creep_shift_tension.map_or(1.0, |f| f.si());
                    Ok(FinalState {
                        material: MaterialState::Initial,
                        stretch: alpha * cable.creep_temperature_shift.unwrap_or(0.0) * t / at,
                        case: case.name.clone(),
                    })
                }
            }
        }
        Condition::Load => {
            if cases.loads.is_empty() {
                return Err(invalid(
                    "no weather case for final after load (Criteria › Creep-Stretch)".into(),
                ));
            }
            if cable.model != CableModel::Nonlinear {
                // Linear elastic: no permanent stretch from a load.
                return Ok(FinalState {
                    material: MaterialState::Initial,
                    stretch: 0.0,
                    case: cases.loads[0].name.clone(),
                });
            }
            // The case giving the most stretch.
            let mut best: Option<FinalState> = None;
            for case in &cases.loads {
                let loaded = ns
                    .solve(state_of(case), &MaterialState::Initial, budget)
                    .map_err(st)?;
                let material = mechanics
                    .permanent_set(
                        Force::from_si(NonlinearSection::section_mean_tension(&loaded)),
                        case.temperature,
                        &MaterialState::Initial,
                    )
                    .map_err(|e| invalid(e.to_string()))?;
                let stretch = (working_strain(&material)?
                    - working_strain(&MaterialState::Initial)?)
                .max(0.0);
                if best.as_ref().is_none_or(|b| stretch > b.stretch) {
                    best = Some(FinalState {
                        material,
                        stretch,
                        case: case.name.clone(),
                    });
                }
            }
            Ok(best.expect("load cases given"))
        }
        Condition::Initial | Condition::MaxSag => Ok(FinalState {
            material: MaterialState::Initial,
            stretch: 0.0,
            case: String::new(),
        }),
    }
}

/// Ruling-span solution of a section in `case`: one horizontal tension for
/// all spans (the sag-tension section, non-linear cable model, with the
/// permanent state `final_state`), each span a catenary between its
/// clamps held where they hang in still air at stringing, in the plane of
/// its load (blown out by the wind). Point loads are left out.
pub(super) fn solve_rs(
    line: &Line,
    section: &Section,
    cable: &Cable,
    case: &WeatherCase,
    final_state: Option<&FinalState>,
    budget: &Budget,
) -> Result<SectionShape, LineError> {
    use ohl_cables::mechanics::{MaterialState, Mechanics};
    use ohl_core::units::{Force, Length};
    use ohl_sagtension::nonlinear::NonlinearSection;
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let st = |e: ohl_sagtension::SagTensionError| invalid(e.to_string());
    let (spans, clamps, strung_state, h0) = strung(line, section, cable, budget)?;
    let mechanics = Mechanics::from_cable(cable).map_err(|e| invalid(e.to_string()))?;
    let mut ns = NonlinearSection::from_known_state(
        spans.clone(),
        mechanics,
        strung_state,
        Force::from_si(h0),
        &MaterialState::Initial,
    )
    .map_err(st)?;
    let material = match final_state {
        Some(f) => {
            if matches!(f.material, MaterialState::Initial) {
                // Linear models: the stretch lengthens the cable (adding
                // no weight).
                ns.lengthen(f.stretch);
            }
            f.material.clone()
        }
        None => MaterialState::Initial,
    };
    let solved = ns
        .solve(load_state(case, cable), &material, budget)
        .map_err(st)?;
    // States beyond the stress-strain curves' range (2026-10-22).
    let mut flags: Vec<String> = ns
        .range_warnings(&solved, &load_state(case, cable), &material)
        .map_err(st)?
        .into_iter()
        .map(|w| format!("stress-strain {w}"))
        .collect();
    let h = solved.horizontal_tension.si();
    let mut out = Vec::with_capacity(spans.len());
    for (k, sol) in solved.spans.iter().enumerate() {
        let (a, b) = (clamps[k], clamps[k + 1]);
        let d = b - a;
        let horizontal = d.xy().norm();
        // The span's load per metre (its own weight spread over its
        // stretched length).
        let q = sol.catenary.weight.si();
        // The plane of the load: the sag hangs along it (down, and to the
        // side the wind blows).
        let along_load = {
            let l = span_load_scaled(case, cable, d, q_scale(case, cable, q));
            if l.norm() > 0.0 {
                l / l.norm()
            } else {
                Vector3::new(0.0, 0.0, -1.0)
            }
        };
        let n = 64;
        let mut points = Vec::with_capacity(n + 1);
        let mut sag: f64 = 0.0;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let x = horizontal * t;
            let below = (d.z * t - sol.elevation(Length::from_si(x)).si()).max(0.0);
            sag = sag.max(below);
            points.push(a + d * t + along_load * below);
        }
        let low = *points
            .iter()
            .min_by(|p, q| p.z.total_cmp(&q.z))
            .expect("points");
        out.push(SpanShape {
            polyline_error: polyline_error(&points, h / q),
            points: points.iter().map(|q| p(*q)).collect(),
            horizontal_tension: h,
            max_tension: sol.tension_left().si().max(sol.tension_right().si()),
            mid_point: p(points[n / 2]),
            low_point: p(low),
            sag,
            surface_radius: case.iced_diameter(cable.diameter).si() / 2.0,
        });
    }
    // Above the wire's strength (owner, 2026-10-22).
    flags.extend(rts_flags(&out, cable));
    let n = clamps.len();
    Ok(SectionShape {
        section: section.id.clone(),
        clamps: clamps.iter().map(|c| p(*c)).collect(),
        spans: out,
        strings: vec![Vec::new(); n],
        iterations: 0,
        estimated_weights: vec![None; n],
        attachment_loads: Vec::new(),
        flags,
        pulls: Vec::new(),
        nodes: Vec::new(),
        span_points: Vec::new(),
        spacers: Vec::new(),
        broken: None,
    })
}

/// Whether a span of `shape` exceeds its wire's rated tensile strength
/// (its warning, [`rts_flags`]).
pub fn above_rts(shape: &SectionShape) -> bool {
    shape.flags.iter().any(|f| f.contains(RTS_WARNING))
}

/// What the warnings above the wire's strength say ([`rts_flags`]).
pub const RTS_WARNING: &str = "exceeds the wire's rated tensile strength";

/// The warnings of the spans whose tension at their most tensioned
/// support (`SpanShape::max_tension`, per wire) exceeds the wire type's
/// rated tensile strength (owner, 2026-10-22: always warned, whatever the
/// range warnings do), in every solver — "span 2: tension 41.20 kN at
/// the most tensioned support exceeds the wire's rated tensile strength
/// (RTS 40.00 kN, 103.0 %)". Nothing for a wire type without an RTS (0).
pub fn rts_flags(spans: &[SpanShape], cable: &Cable) -> Vec<String> {
    let rts = cable.rated_tensile_strength.si();
    if rts.is_nan() || rts <= 0.0 {
        return Vec::new();
    }
    spans
        .iter()
        .enumerate()
        .filter(|(_, s)| s.max_tension > rts)
        .map(|(k, s)| {
            format!(
                "span {}: tension {:.2} kN at the most tensioned support exceeds the wire's \
                 rated tensile strength (RTS {:.2} kN, {:.1} %)",
                k + 1,
                s.max_tension / 1000.0,
                rts / 1000.0,
                100.0 * s.max_tension / rts
            )
        })
        .collect()
}

/// The factor on the cable's own weight that makes its load per metre in
/// `case` equal to `q` (the ruling-span solution's per-metre load: the
/// weight spread over the stretched span).
pub(super) fn q_scale(case: &WeatherCase, cable: &Cable, q: f64) -> f64 {
    let load = case.wire_load(cable.diameter, cable.weight());
    let (h, v, w) = (
        load.horizontal.si(),
        load.vertical.si(),
        cable.weight().si(),
    );
    let vertical = (q - h).max(0.0).sqrt() * (q + h).sqrt();
    if w > 0.0 {
        (vertical - (v - w)) / w
    } else {
        1.0
    }
}

/// The cable's stress-strain curves, when its model uses them (the
/// non-linear model with curves given).
pub(super) fn curves_of(cable: &Cable) -> Option<ohl_cables::mechanics::Mechanics> {
    if cable.model != ohl_cables::CableModel::Nonlinear {
        return None;
    }
    ohl_cables::mechanics::Mechanics::from_cable(cable)
        .ok()
        .filter(|m| m.is_nonlinear())
}

/// Mean tension of each span's elements (weighted by their unstressed
/// lengths), N.
pub(super) fn span_tensions(built: &Built, solution: &ohl_wirefe::Solution) -> Vec<f64> {
    built
        .span_elements
        .iter()
        .map(|elements| {
            let (mut sum, mut length) = (0.0, 0.0);
            for &e in elements {
                let l = built.model.cables[e].properties.unstressed_length;
                let st = &solution.cables[e];
                sum += 0.5 * (st.tension_i + st.tension_j) * l;
                length += l;
            }
            if length > 0.0 { sum / length } else { 0.0 }
        })
        .collect()
}
