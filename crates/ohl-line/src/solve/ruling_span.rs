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
    /// The creep at elevated temperature on top of it, unit strain
    /// (roadmap O22 item 6; after creep only, 0 without a time at high
    /// temperature or a law of the wire type): a lengthening of the whole
    /// conductor that adds no weight, in every model
    /// ([`FinalState::lengthening`]).
    pub hot_creep: f64,
    /// What every result in this state says of it: the law used — the
    /// user's coefficients and their source — or that none was given.
    pub notes: Vec<String>,
}

impl Default for FinalState {
    /// The initial state: no set, no stretch.
    fn default() -> Self {
        Self {
            material: ohl_cables::mechanics::MaterialState::Initial,
            stretch: 0.0,
            case: String::new(),
            hot_creep: 0.0,
            notes: Vec::new(),
        }
    }
}

impl FinalState {
    /// The lengthening of the cable's unstressed length the models apply
    /// besides the material state, unit strain: the creep at elevated
    /// temperature, and for the linear models (their material state is
    /// the initial one) their permanent stretch too.
    pub fn lengthening(&self) -> f64 {
        use ohl_cables::mechanics::MaterialState;
        match self.material {
            MaterialState::Initial => self.stretch + self.hot_creep,
            _ => self.hot_creep,
        }
    }
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
    // The holders and the reference: from the memo once worked out.
    let prepared = super::memo::prepared(line, section, cable, budget)?;
    let (layout, reference) = (&prepared.layout, &prepared.reference);
    let still = WeatherCase::still(
        "stringing",
        Temperature::from_celsius(reference.temperature),
    );
    let (built, solution) = solve_built(
        section,
        layout,
        reference,
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

/// A section's sag-tension section as strung, its clamps, the stringing
/// state and the horizontal tension then, N ([`strung_section`]).
pub(crate) type StrungSection = (
    ohl_sagtension::nonlinear::NonlinearSection,
    Vec<Vector3<f64>>,
    ohl_sagtension::LoadState,
    f64,
);

/// The sag-tension section of `section` as strung (initial), its clamps
/// hanging in still air at stringing, the stringing state and the
/// horizontal tension then ([`strung`]). A **gap-type conductor**
/// ([`ohl_cables::Cable::is_gap`]) is strung with its outer strands
/// unloaded in that state — the section's own knee point
/// ([`ohl_sagtension::nonlinear::NonlinearSection::from_known_state_gap`]);
/// with a stringing worked back from a final condition, in the initial
/// state found.
///
/// Expected cost: [`strung`] (one still-air section solve) and the
/// section's length; 2–3 times that length for a gap type.
pub(crate) fn strung_section(
    line: &Line,
    section: &Section,
    cable: &Cable,
    budget: &Budget,
) -> Result<StrungSection, LineError> {
    use ohl_cables::mechanics::{MaterialState, Mechanics};
    use ohl_core::units::Force;
    use ohl_sagtension::nonlinear::NonlinearSection;
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let (spans, clamps, strung_state, h) = strung(line, section, cable, budget)?;
    let mechanics = Mechanics::from_cable(cable).map_err(|e| invalid(e.to_string()))?;
    let ns = if cable.is_gap() {
        NonlinearSection::from_known_state_gap(spans, mechanics, strung_state, Force::from_si(h))
    } else {
        NonlinearSection::from_known_state(
            spans,
            mechanics,
            strung_state,
            Force::from_si(h),
            &MaterialState::Initial,
        )
    }
    .map_err(|e| invalid(e.to_string()))?;
    Ok((ns, clamps, strung_state, h))
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
/// After creep, the line's **time at high temperature**
/// ([`crate::condition::CreepStretch::hot`]) adds the creep at elevated
/// temperature of a wire type that gives a law
/// ([`ohl_cables::hot_creep::HotCreep`]) on top ([`hot_creep_of`]).
///
/// Expected cost: one still-air section solve and a few sag-tension
/// solves (~1 ms each for 10 spans); one more per stage of time at high
/// temperature.
pub fn final_state(
    line: &Line,
    section: &Section,
    cable: &Cable,
    condition: crate::condition::Condition,
    budget: &Budget,
) -> Result<FinalState, LineError> {
    use crate::condition::Condition;
    let cable = &*line.bimetallic.apply(cable);
    let cable = &*strung_cable(section, cable)?;
    let (ns, _, strung_state, h) = strung_section(line, section, cable, budget)?;
    let strung_at = (strung_state, h);
    let mut state = permanent_set(
        line,
        section,
        cable,
        condition,
        &ns,
        &ns.mechanics,
        strung_at,
        budget,
    )?;
    if condition == Condition::Creep && !line.creep_stretch.hot.is_empty() {
        hot_creep_of(line, section, cable, &ns, &mut state, budget)?;
    }
    Ok(state)
}

/// The permanent set of [`final_state`] without the creep at elevated
/// temperature: of `section` strung with `cable` (as strung), its
/// sag-tension section `ns` and mechanics, strung at `strung_at` (the
/// stringing state and the horizontal tension then, N).
#[allow(clippy::too_many_arguments)]
fn permanent_set(
    line: &Line,
    section: &Section,
    cable: &Cable,
    condition: crate::condition::Condition,
    ns: &ohl_sagtension::nonlinear::NonlinearSection,
    mechanics: &ohl_cables::mechanics::Mechanics,
    (strung_state, h): (ohl_sagtension::LoadState, f64),
    budget: &Budget,
) -> Result<FinalState, LineError> {
    use crate::condition::Condition;
    use ohl_cables::CableModel;
    use ohl_cables::mechanics::MaterialState;
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
                        ..FinalState::default()
                    })
                }
                CableModel::LinearCreepTemperature => Ok(FinalState {
                    material: MaterialState::Initial,
                    stretch: alpha * cable.creep_temperature_shift.unwrap_or(0.0),
                    case: case.name.clone(),
                    ..FinalState::default()
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
                        ..FinalState::default()
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
                    ..FinalState::default()
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
                        ..FinalState::default()
                    });
                }
            }
            Ok(best.expect("load cases given"))
        }
        Condition::Initial | Condition::MaxSag => Ok(FinalState::default()),
    }
}

/// Adds the creep at elevated temperature to `state`, the state after
/// creep of `section` (its sag-tension section `ns`, as strung): the
/// line's time at high temperature, stage by stage in the order of rising
/// temperature, by the law of `cable`
/// ([`ohl_cables::hot_creep::HotCreep`]).
///
/// Per stage: the section's mean tension in still air, bare, at the
/// stage's temperature, in the state reached so far (one section solve) —
/// **held over the stage** (conservative: the tension falls as the wire
/// creeps) — gives the law's stress; the strain so far continues by strain
/// hardening ([`ohl_cables::hot_creep::HotCreep::after_stage`]). The sum is
/// **added** to the state after creep (conservative: not merged with the
/// creep curve's own time) as a lengthening of the whole conductor.
///
/// The state's notes say the law's source — the coefficients are the
/// user's — and the stages the law does not cover; a wire type without a
/// law: "NOT ON THE SAFE SIDE", its hot creep is not counted.
///
/// Exit clause: at most [`crate::condition::HOT_STAGES`] stages (checked
/// when the line is read), each one section solve under `budget`; no
/// iteration.
pub fn hot_creep_of(
    line: &Line,
    section: &Section,
    cable: &Cable,
    ns: &ohl_sagtension::nonlinear::NonlinearSection,
    state: &mut FinalState,
    budget: &Budget,
) -> Result<(), LineError> {
    use ohl_core::units::Temperature;
    use ohl_sagtension::nonlinear::NonlinearSection;
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let history = &line.creep_stretch;
    let Some(law) = &cable.hot_creep else {
        state.notes.push(format!(
            "NOT ON THE SAFE SIDE: wire type {} gives no law of creep at elevated temperature — \
             its creep during {} is not counted",
            cable.id,
            history.hot_text()
        ));
        return Ok(());
    };
    let (area, rts) = (cable.area().si(), cable.rated_tensile_strength.si());
    let mut strain = 0.0;
    let mut below = Vec::new();
    for stage in history.hot_in_order() {
        let at = Temperature::from_celsius(stage.temperature);
        if !law.applies_at(at) {
            below.push(format!("{} h at {} °C", stage.hours, stage.temperature));
            continue;
        }
        // The section in the state reached so far, at the stage's
        // temperature: still air, bare.
        let mut reached = ns.clone();
        reached.lengthen(state.lengthening() + strain);
        let still = WeatherCase::still("hot stage", at);
        let solved = reached
            .solve(load_state(&still, cable), &state.material, budget)
            .map_err(|e| {
                invalid(format!(
                    "time at high temperature, {} °C: {e}",
                    stage.temperature
                ))
            })?;
        let tension = NonlinearSection::section_mean_tension(&solved);
        strain = law.after_stage(strain, law.stress_of(tension, area, rts), at, stage.hours);
    }
    state.hot_creep = strain;
    state.notes.push(format!(
        "creep at elevated temperature: +{:.1} µε on the state after creep ({}), by the law of \
         wire type {} — the user's coefficients, source: {}",
        strain * 1e6,
        history.hot_text(),
        cable.id,
        law.source.trim()
    ));
    if !below.is_empty() {
        state.notes.push(format!(
            "creep at elevated temperature: {} not counted — the law of wire type {} applies \
             from {} °C",
            below.join(", "),
            cable.id,
            law.above.celsius()
        ));
    }
    Ok(())
}

/// Ruling-span solution of a section in `case`: one horizontal tension for
/// all spans (the sag-tension section, non-linear cable model, with the
/// permanent state `final_state`), each span a catenary between its
/// clamps held where they hang in still air at stringing, in the plane of
/// its load (blown out by the wind). Point loads are left out, and said
/// ([`point_loads_left_out`]).
///
/// It keeps the equal horizontal tension whatever the line's in-the-blocks
/// model ([`crate::InTheBlocks`], owner decision 2026-10-25): the ruling
/// span is itself that simplification (one tension, one equivalent span);
/// the sheaves are the finite-element calculation's.
pub(super) fn solve_rs(
    line: &Line,
    section: &Section,
    cable: &Cable,
    case: &WeatherCase,
    final_state: Option<&FinalState>,
    budget: &Budget,
) -> Result<SectionShape, LineError> {
    use ohl_cables::mechanics::MaterialState;
    use ohl_core::units::{Force, Length};
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let st = |e: ohl_sagtension::SagTensionError| invalid(e.to_string());
    let (mut ns, clamps, _, _) = strung_section(line, section, cable, budget)?;
    let spans = ns.spans.clone();
    let material = match final_state {
        Some(f) => {
            // The linear models' stretch and the creep at elevated
            // temperature lengthen the cable (adding no weight).
            if f.lengthening() != 0.0 {
                ns.lengthen(f.lengthening());
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
    // Bimetallic cables: the spans past the knee point (roadmap O22
    // item 1), at each span's mean tension. (A gap type is past it above
    // its stringing temperature by its nature: said once per line.)
    let knee = (!cable.is_gap()).then_some(&solved.spans);
    for (k, sol) in knee.into_iter().flatten().enumerate() {
        let mean = Force::from_si(sol.tension_integral() / sol.length().si());
        let state = ns
            .mechanics
            .outer_state(mean, case.temperature, &material)
            .map_err(|e| invalid(e.to_string()))?;
        flags.extend(state.and_then(|s| crate::bimetallic::past_knee_flag(k, s)));
    }
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
        // The end pulls by this model: the end tension along the curve's
        // end tangent (the curve is a + d·t + along_load·below(t), below =
        // d.z·t − y(x)), into the span.
        let tangent = |x: f64| {
            let t = d + along_load * (d.z - horizontal * sol.slope(Length::from_si(x)));
            if t.norm() > 0.0 { t / t.norm() } else { t }
        };
        let bundle = f64::from(section.bundle.max(1));
        let end_pulls = [
            p(tangent(0.0) * (sol.tension_left().si() * bundle)),
            p(-tangent(horizontal) * (sol.tension_right().si() * bundle)),
        ];
        out.push(SpanShape {
            end_pulls,
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
    // The point loads this method leaves out (roadmap O22 item 7).
    flags.extend(point_loads_left_out(section));
    // What the permanent state says of itself (the creep at elevated
    // temperature: the user's law, or none).
    flags.extend(final_state.iter().flat_map(|f| f.notes.iter().cloned()));
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
        broken_spans: Vec::new(),
    })
}

/// The flag of a section solved by the ruling-span method whose span
/// points carry loads — objects with a weight or a constant force
/// ([`crate::LineObject::force_across`]) — which that method leaves out:
/// "NOT ON THE SAFE SIDE: point loads (M1: 55 N down; P1: across 400 N)
/// left out by the ruling-span method — use FE". Up to six are named.
/// Nothing for a section without such loads.
pub fn point_loads_left_out(section: &Section) -> Option<String> {
    let loads: Vec<String> = section
        .objects()
        .filter(|(_, o)| o.weight > 0.0 || o.has_force())
        .map(|(p, o)| {
            let mut parts = Vec::new();
            if o.weight > 0.0 {
                parts.push(format!("{} N down", o.bundle_weight(section.bundle)));
            }
            if o.has_force() {
                parts.push(o.force_text());
            }
            format!("{}: {}", p.label, parts.join(", "))
        })
        .collect();
    if loads.is_empty() {
        return None;
    }
    let more = loads.len().saturating_sub(6);
    Some(format!(
        "NOT ON THE SAFE SIDE: point loads ({}{}) left out by the ruling-span method — use FE",
        loads[..loads.len() - more].join("; "),
        if more > 0 {
            format!("; and {more} more")
        } else {
            String::new()
        }
    ))
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
    // A gap-type conductor follows its components whatever its model:
    // their loads are not the composite's (the outer strands' offset).
    if cable.is_gap() {
        return ohl_cables::mechanics::Mechanics::from_cable(cable).ok();
    }
    if cable.model != ohl_cables::CableModel::Nonlinear {
        return None;
    }
    ohl_cables::mechanics::Mechanics::from_cable(cable)
        .ok()
        .filter(|m| m.is_nonlinear())
}

/// [`span_tensions`] per wire: in a span with subconductors broken the
/// element carries the share of the bundle left, and each remaining wire
/// the element's tension over that share ([`Cases::remaining`]).
pub(super) fn wire_tensions(
    built: &Built,
    solution: &ohl_wirefe::Solution,
    cases: Cases<'_>,
) -> Vec<f64> {
    let mut tensions = span_tensions(built, solution);
    for (s, t) in tensions.iter_mut().enumerate() {
        *t /= cases.remaining(s);
    }
    tensions
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
