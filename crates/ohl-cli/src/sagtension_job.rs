// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl sagtension`: sag-tension job for one tension section.
//!
//! Reads a TOML job (cable from a library, spans, weather cases, either a
//! fixed stringing tension or tension criteria, chart temperatures) and
//! reports the ruling span, the stringing tension, every weather case and a
//! stringing chart. See `examples/sagtension/drake-section.toml`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nalgebra::Vector3;
use ohl_cables::{Cable, Library};
use ohl_core::units::{Force, Length, Quantity, Temperature, UnitSystem};
use ohl_sagtension::catenary::default_solver_budget;
use ohl_sagtension::criteria::default_criteria_budget;
use ohl_sagtension::{
    ElasticCable, LoadState, SectionSolution, SpanGeometry, TensionCriterion, TensionLimit,
    TensionSection, governing_tension, ruling_span,
};
use ohl_weather::WeatherCase;
use ohl_wirefe::{Assembly, CableSpec, FeSection, SectionSpec, State, Support, default_fe_budget};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    /// Wire type id in the library.
    cable: String,
    /// Wire library (`.ohlw` file or directory), relative to the job file (default: the shipped
    /// `libraries/cables`).
    library: Option<PathBuf>,
    spans: Vec<SpanInput>,
    reference: Reference,
    #[serde(default)]
    stringing: Option<Stringing>,
    weather: Vec<WeatherCase>,
    #[serde(default)]
    criterion: Vec<CriterionInput>,
    #[serde(default)]
    chart: Option<Chart>,
    #[serde(default)]
    fe: Option<FeInput>,
    #[serde(default, rename = "final")]
    final_conditions: Option<FinalInput>,
    #[serde(default)]
    sagging: Option<SaggingInput>,
}

/// Sagging outputs ([`ohl_sagtension::sagging`]): the section in the
/// stringing state at the sagging temperature.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaggingInput {
    /// Conductor temperature at sagging (default: the reference case's).
    temperature: Option<Temperature>,
    /// Numbers of waves timed by the stopwatch (default 1 and 3).
    #[serde(default)]
    waves: Vec<u32>,
    /// Temperature at clipping in, for the clipping offsets (default: the
    /// sagging temperature, where the offsets are zero).
    clip_temperature: Option<Temperature>,
    /// Transits: one per span to sight.
    #[serde(default)]
    transit: Vec<TransitInput>,
}

/// A transit sighting a span ([`ohl_sagtension::sagging::angle_of_sight`]).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitInput {
    /// Span number, 1-based as in the report.
    span: usize,
    /// The instrument's distance below the back support (positive down).
    below_back: Length,
    /// Horizontal distance of the instrument from the back support
    /// (default 0: at it).
    #[serde(default)]
    from_back: Length,
}

/// Cases defining the final condition of a non-linear wire.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinalInput {
    /// Creep condition (default: the reference weather case).
    creep_weather: Option<String>,
    /// Heaviest load case (default: the weather case with the largest load).
    load_weather: Option<String>,
}

/// Finite-element section analysis (insulator swing, unequal spans, broken
/// conductor). Spans are measured between conductor clamps.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FeInput {
    #[serde(default)]
    suspension: Option<SuspensionInput>,
    /// Two strings to one clamp (`[fe.two_strings]`; `[fe.v_string]`,
    /// its name before the glossary of 2026-10-21, still read).
    #[serde(default, alias = "v_string")]
    two_strings: Option<VStringInput>,
    #[serde(default)]
    broken: Vec<BrokenInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SuspensionInput {
    length: Length,
    weight: Force,
    axial_stiffness: Force,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VStringInput {
    half_width: Length,
    leg_length: Length,
    leg_weight: Force,
    axial_stiffness: Force,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrokenInput {
    weather: String,
    /// Span number, 1-based as in the report.
    span: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpanInput {
    span: Length,
    #[serde(default)]
    rise: Length,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    /// Weather case at which the stringing tension is defined.
    weather: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stringing {
    horizontal_tension: Force,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CriterionInput {
    weather: String,
    limit: String,
    value: toml::Value,
    /// Non-linear wires: `initial` (default), `creep` (after creep) or
    /// `load` (after load).
    #[serde(default)]
    condition: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Chart {
    temperatures: Vec<Temperature>,
}

/// Result of a job: the text report and the stringing chart as CSV.
pub struct JobOutput {
    pub report: String,
    pub chart_csv: Option<String>,
}

fn state(case: &WeatherCase, cable: &Cable) -> LoadState {
    let load = case.wire_load(cable.diameter, cable.weight());
    LoadState {
        load: load.resultant(),
        weight: cable.weight(),
        wind: load.horizontal,
        temperature: case.temperature,
    }
}

fn criterion(
    input: &CriterionInput,
    cases: &[WeatherCase],
    cable: &Cable,
) -> Result<TensionCriterion, String> {
    let case = find_case(cases, &input.weather)?;
    let ratio = || {
        input.value.as_float().ok_or_else(|| {
            format!(
                "criterion \"{}\": value must be a number (fraction of RTS)",
                input.weather
            )
        })
    };
    let text = || {
        input.value.as_str().ok_or_else(|| {
            format!(
                "criterion \"{}\": value must be text with a unit",
                input.weather
            )
        })
    };
    let limit = match input.limit.as_str() {
        "support_ratio" => TensionLimit::SupportTensionRatio(ratio()?),
        "horizontal_ratio" => TensionLimit::HorizontalTensionRatio(ratio()?),
        "horizontal" => {
            TensionLimit::HorizontalTension(text()?.parse().map_err(|e| format!("{e}"))?)
        }
        "catenary_constant" => {
            TensionLimit::CatenaryConstant(text()?.parse().map_err(|e| format!("{e}"))?)
        }
        other => return Err(format!("unknown limit \"{other}\"")),
    };
    Ok(TensionCriterion {
        label: case.name.clone(),
        state: state(case, cable),
        limit,
    })
}

fn find_case<'a>(cases: &'a [WeatherCase], name: &str) -> Result<&'a WeatherCase, String> {
    cases
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| format!("unknown weather case \"{name}\""))
}

fn show<Q: Quantity>(q: Q, units: UnitSystem, precision: usize) -> String {
    format!("{:.*}", precision, q.display_in(units))
}

fn max_support(solution: &SectionSolution) -> Force {
    solution
        .spans
        .iter()
        .map(|s| s.tension_left().max(s.tension_right()))
        .fold(Force::ZERO, Force::max)
}

/// The creep case and the heaviest load case of the final states: from
/// `[final]`, else the reference case and the weather case with the largest
/// load.
fn final_cases<'a>(
    job: &'a Job,
    cable: &Cable,
    reference_case: &'a WeatherCase,
) -> Result<(&'a WeatherCase, &'a WeatherCase), String> {
    let creep_case = match job
        .final_conditions
        .as_ref()
        .and_then(|f| f.creep_weather.as_deref())
    {
        Some(name) => find_case(&job.weather, name)?,
        None => reference_case,
    };
    let load_case = match job
        .final_conditions
        .as_ref()
        .and_then(|f| f.load_weather.as_deref())
    {
        Some(name) => find_case(&job.weather, name)?,
        None => job
            .weather
            .iter()
            .max_by(|a, b| {
                state(a, cable)
                    .load
                    .si()
                    .total_cmp(&state(b, cable).load.si())
            })
            .ok_or("no weather cases")?,
    };
    Ok((creep_case, load_case))
}

#[allow(clippy::too_many_arguments)]
fn nonlinear_report(
    out: &mut String,
    job: &Job,
    cable: &Cable,
    mechanics: ohl_cables::mechanics::Mechanics,
    spans: &[SpanGeometry],
    stringing: Force,
    reference_case: &WeatherCase,
    units: UnitSystem,
) -> Result<(), String> {
    use ohl_cables::mechanics::MaterialState;
    use ohl_sagtension::nonlinear::{FinalCause, NonlinearSection, final_states, governing_final};

    let (creep_case, load_case) = final_cases(job, cable, reference_case)?;
    let budget = default_solver_budget();
    let section = NonlinearSection::from_known_state(
        spans.to_vec(),
        mechanics,
        state(reference_case, cable),
        stringing,
        &MaterialState::Initial,
    )
    .map_err(|e| e.to_string())?;
    let states = final_states(
        &section,
        state(creep_case, cable),
        state(load_case, cable),
        &budget,
    )
    .map_err(|e| e.to_string())?;
    let name = |c: FinalCause| match c {
        FinalCause::Creep => "creep",
        FinalCause::Load => "load",
    };
    let _ = writeln!(
        out,
        "\nNon-linear stress-strain and creep (strung on the initial curve at {} / \"{}\")",
        show(stringing, units, 2),
        reference_case.name
    );
    let _ = writeln!(
        out,
        "  Final states: after creep at \"{}\", after load at \"{}\"; per case the one leaving the \
         wire longer governs (lower H, larger sag); at the creep tension: {}",
        creep_case.name,
        load_case.name,
        name(states.longer)
    );
    let _ = writeln!(
        out,
        "  {:<24} {:>12} {:>12} {:>12} {:>8} {:>12} {:>12}",
        "case", "H initial", "H creep", "H load", "governs", "sag initial", "sag final"
    );
    let sag = |sol: &SectionSolution| {
        sol.spans
            .iter()
            .map(|sp| sp.sag())
            .fold(Length::ZERO, Length::max)
    };
    let mut warnings = Vec::new();
    for case in &job.weather {
        let s = state(case, cable);
        let initial = section
            .solve(s, &MaterialState::Initial, &budget)
            .map_err(|e| format!("case \"{}\": {e}", case.name))?;
        let fin = governing_final(&section, &states, s, &budget)
            .map_err(|e| format!("case \"{}\": {e}", case.name))?;
        for (label, solution, material) in [
            ("initial", &initial, &MaterialState::Initial),
            ("after creep", &fin.after_creep, &states.after_creep),
            ("after load", &fin.after_load, &states.after_load),
        ] {
            for w in section
                .range_warnings(solution, &s, material)
                .map_err(|e| e.to_string())?
            {
                warnings.push(format!("\"{}\" {label}: {w}", case.name));
            }
        }
        let _ = writeln!(
            out,
            "  {:<24} {:>12} {:>12} {:>12} {:>8} {:>12} {:>12}",
            case.name,
            show(initial.horizontal_tension, units, 2),
            show(fin.after_creep.horizontal_tension, units, 2),
            show(fin.after_load.horizontal_tension, units, 2),
            name(fin.governing),
            show(sag(&initial), units, 2),
            show(sag(fin.solution()), units, 2)
        );
    }
    for w in &warnings {
        let _ = writeln!(out, "  Warning: {w}");
    }
    Ok(())
}

/// The stringing tension from the criteria on the non-linear model; the
/// report's table.
#[allow(clippy::too_many_arguments)]
fn nonlinear_criteria(
    out: &mut String,
    job: &Job,
    cable: &Cable,
    mechanics: &ohl_cables::mechanics::Mechanics,
    spans: &[SpanGeometry],
    reference_case: &WeatherCase,
    units: UnitSystem,
) -> Result<Force, String> {
    use ohl_sagtension::criteria::{
        Condition, NonlinearCases, NonlinearCriterion, governing_tension_nonlinear,
    };
    let (creep_case, load_case) = final_cases(job, cable, reference_case)?;
    let criteria = job
        .criterion
        .iter()
        .map(|c| {
            let condition = match c.condition.as_deref().unwrap_or("initial") {
                "initial" => Condition::Initial,
                "creep" => Condition::AfterCreep,
                "load" => Condition::AfterLoad,
                other => {
                    return Err(format!(
                        "criterion \"{}\": unknown condition \"{other}\" (initial, creep or load)",
                        c.weather
                    ));
                }
            };
            Ok(NonlinearCriterion {
                criterion: criterion(c, &job.weather, cable)?,
                condition,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let governing = governing_tension_nonlinear(
        spans,
        mechanics,
        cable.rated_tensile_strength,
        NonlinearCases {
            reference: state(reference_case, cable),
            creep: state(creep_case, cable),
            max_load: state(load_case, cable),
        },
        &criteria,
        &default_criteria_budget(),
    )
    .map_err(|e| e.to_string())?;
    let _ = writeln!(
        out,
        "\nStringing tension (governing, non-linear model): {} at \"{}\" (creep at \"{}\", \
         heaviest load \"{}\")",
        show(governing.reference_tension, units, 2),
        reference_case.name,
        creep_case.name,
        load_case.name
    );
    let _ = writeln!(
        out,
        "  {:<28} {:<12} {:<34} {:>11}",
        "criterion", "condition", "limit", "utilisation"
    );
    for (i, (c, r)) in criteria.iter().zip(&governing.criteria).enumerate() {
        let mark = if i == governing.governing {
            "  ← governs"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "  {:<28} {:<12} {:<34} {:>10.1} %{mark}",
            c.criterion.label,
            c.condition.to_string(),
            c.criterion.limit.to_string(),
            r.utilisation * 100.0
        );
    }
    Ok(governing.reference_tension)
}

/// Each span's unstressed length in a solution at a temperature.
type SpanLengths<'a> = dyn Fn(&SectionSolution, Temperature) -> Result<Vec<f64>, String> + 'a;

/// The sagging outputs of the section in the stringing state
/// ([`ohl_sagtension::sagging`]). `solve` gives the section at a
/// temperature (bare, still air, initial), `unstressed` each span's
/// unstressed length in such a solution.
fn sagging_report(
    out: &mut String,
    input: &SaggingInput,
    reference_case: &WeatherCase,
    solve: &dyn Fn(Temperature) -> Result<SectionSolution, String>,
    unstressed: &SpanLengths<'_>,
    units: UnitSystem,
) -> Result<(), String> {
    use ohl_sagtension::sagging;
    let t = input.temperature.unwrap_or(reference_case.temperature);
    let waves = if input.waves.is_empty() {
        vec![1, 3]
    } else {
        input.waves.clone()
    };
    if waves.contains(&0) {
        return Err("[sagging] waves must be 1 or more".into());
    }
    let solved = solve(t)?;
    let dt = 0.5;
    let per_k = sagging::temperature_correction(
        &solve(Temperature::from_celsius(t.celsius() - dt))?,
        &solve(Temperature::from_celsius(t.celsius() + dt))?,
        dt,
    );
    let _ = writeln!(
        out,
        "\nSagging (in the blocks, initial state, bare, still air, at {}; H {})",
        show(t, units, 1),
        show(solved.horizontal_tension, units, 2)
    );
    let mut header = format!(
        "  {:>4} {:>10} {:>10} {:>12} {:>14} {:>12}",
        "span", "sag", "level sag", "incl. corr.", "sag per K", "dyno back"
    );
    let _ = write!(header, " {:>12}", "dyno ahead");
    for n in &waves {
        let _ = write!(
            header,
            " {:>10}",
            format!("{n} wave{}", if *n == 1 { "" } else { "s" })
        );
    }
    let _ = writeln!(out, "{header}");
    for (i, sp) in solved.spans.iter().enumerate() {
        let per_k_shown = Length::from_si(per_k[i]);
        let mut line = format!(
            "  {:>4} {:>10} {:>10} {:>12} {:>14} {:>12} {:>12}",
            i + 1,
            show(sp.sag(), units, 3),
            show(sagging::level_sag(sp), units, 3),
            show(sagging::inclined_correction(sp), units, 3),
            format!("{}/K", show(per_k_shown, units, 4)),
            show(sagging::dynamometer(sp, true), units, 2),
            show(sagging::dynamometer(sp, false), units, 2)
        );
        for n in &waves {
            let _ = write!(line, " {:>9.2}s", sagging::stopwatch_time(sp, *n));
        }
        let _ = writeln!(out, "{line}");
    }
    let _ = writeln!(
        out,
        "  (sag: vertical, where the wire is parallel to the chord; level sag: a level span's of the \
         same span, H and load; incl. corr. = sag - level sag; stopwatch: the return time of n waves)"
    );
    for transit in &input.transit {
        let sp = solved
            .spans
            .get(transit.span.wrapping_sub(1))
            .ok_or_else(|| format!("[[sagging.transit]]: no span {}", transit.span))?;
        let sight =
            sagging::angle_of_sight(sp, transit.from_back, -transit.below_back, sp.geometry.span)
                .map_err(|e| format!("transit in span {}: {e}", transit.span))?;
        let below_ahead = sp.geometry.rise - sight.target_elevation;
        let _ = writeln!(
            out,
            "  Transit, span {}: {} below the back support, {} from it: angle of sight {:+.4}°, \
             tangent {} from the back support, target {} below the ahead support",
            transit.span,
            show(transit.below_back, units, 2),
            show(transit.from_back, units, 2),
            sight.angle.to_degrees(),
            show(sight.tangent_x, units, 2),
            show(below_ahead, units, 3)
        );
    }
    let clip_t = input.clip_temperature.unwrap_or(t);
    let clipped = solve(clip_t)?;
    let offsets =
        sagging::clipping_offsets(&unstressed(&solved, t)?, &unstressed(&clipped, clip_t)?);
    let shown: Vec<String> = offsets[..offsets.len().saturating_sub(1)]
        .iter()
        .enumerate()
        .map(|(i, o)| format!("{}: {}", i + 2, show(Length::from_si(*o), units, 3)))
        .collect();
    if !shown.is_empty() {
        let _ = writeln!(
            out,
            "  Clipping offsets, clipped in at {} (structure from the first dead-end = 1; positive: \
             the clamp point lies that far into the span ahead, along the unstressed wire): {}",
            show(clip_t, units, 1),
            shown.join(", ")
        );
        if clip_t == t {
            let _ = writeln!(
                out,
                "  (clipped at the sagging temperature: zero in this model, one horizontal tension \
                 in the blocks; sheaves and their friction are not modelled)"
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn fe_report(
    out: &mut String,
    fe: &FeInput,
    job: &Job,
    cable: &Cable,
    spans: &[SpanGeometry],
    elastic: ElasticCable,
    stringing: Force,
    reference_case: &WeatherCase,
    units: UnitSystem,
) -> Result<(), String> {
    let middle = match (&fe.suspension, &fe.two_strings) {
        (Some(s), None) => Assembly::Suspension {
            length: s.length.si(),
            weight: s.weight.si(),
            axial_stiffness: s.axial_stiffness.si(),
        },
        (None, Some(v)) => Assembly::VString {
            half_width: v.half_width.si(),
            leg_length: v.leg_length.si(),
            leg_weight: v.leg_weight.si(),
            axial_stiffness: v.axial_stiffness.si(),
        },
        _ => return Err("[fe] needs exactly one of suspension or two_strings".into()),
    };
    let string_depth = match middle {
        Assembly::Suspension { length, .. } => length,
        Assembly::VString {
            half_width,
            leg_length,
            ..
        } => (leg_length * leg_length - half_width * half_width).sqrt(),
        Assembly::DeadEnd => 0.0,
    };
    // Supports from the clamp geometry: x along the line, z from the rises.
    let mut clamp = Vector3::zeros();
    let mut supports = vec![Support {
        attachment: clamp,
        assembly: Assembly::DeadEnd,
    }];
    for (k, span) in spans.iter().enumerate() {
        clamp += Vector3::new(span.span.si(), 0.0, span.rise.si());
        let last = k == spans.len() - 1;
        supports.push(Support {
            attachment: if last {
                clamp
            } else {
                clamp + Vector3::new(0.0, 0.0, string_depth)
            },
            assembly: if last { Assembly::DeadEnd } else { middle },
        });
    }
    let spec = SectionSpec {
        supports,
        cable: CableSpec {
            axial_stiffness: elastic.axial_stiffness.si(),
            thermal_expansion: elastic.thermal_expansion,
            weight: cable.weight().si(),
        },
        stringing_tension: stringing.si(),
        stringing_temperature: reference_case.temperature.celsius(),
        load_sites: vec![],
    };
    let section = FeSection::build(spec, &default_criteria_budget()).map_err(|e| e.to_string())?;
    let state_for = |case: &WeatherCase| {
        let load = case.wire_load(cable.diameter, cable.weight());
        let mut state = State::bare(section.spec(), case.temperature.celsius());
        state.cable_load = Vector3::new(0.0, load.horizontal.si(), -load.vertical.si());
        state
    };
    let _ = writeln!(
        out,
        "\nFinite-element section (insulators modelled, sagged plumb at {} / \"{}\")",
        show(stringing, units, 2),
        reference_case.name
    );
    let _ = writeln!(
        out,
        "  {:<24} {:>12} {:>12} {:>16} {:>10} {:>10} {:>14}",
        "case", "min H", "max H", "max support T", "max sag", "blowout", "max swing"
    );
    let row = |out: &mut String, label: &str, r: &ohl_wirefe::SectionResult| {
        let spans: Vec<_> = r.spans.iter().flatten().collect();
        let h =
            |f: fn(f64, f64) -> f64, init| spans.iter().map(|s| s.horizontal_tension).fold(init, f);
        let max_t = spans
            .iter()
            .map(|s| s.tension_back.max(s.tension_ahead))
            .fold(0.0, f64::max);
        let sag = spans.iter().map(|s| s.sag).fold(0.0, f64::max);
        let blowout = spans.iter().map(|s| s.blowout).fold(0.0, f64::max);
        let (swing_at, swing) = r
            .supports
            .iter()
            .enumerate()
            .map(|(k, s)| (k + 1, s.swing_deg))
            .fold((0, 0.0), |best, x| if x.1 > best.1 { x } else { best });
        let _ = writeln!(
            out,
            "  {:<24} {:>12} {:>12} {:>16} {:>10} {:>10} {:>7.1}° {:<5}",
            label,
            show(Force::from_si(h(f64::min, f64::INFINITY)), units, 2),
            show(Force::from_si(h(f64::max, 0.0)), units, 2),
            show(Force::from_si(max_t), units, 2),
            show(Length::from_si(sag), units, 2),
            show(Length::from_si(blowout), units, 2),
            swing,
            if swing_at == 0 {
                "—".to_owned()
            } else {
                format!("at {swing_at}")
            }
        );
    };
    let _ = writeln!(
        out,
        "  (swing: suspension string angle from vertical, support numbers from the first dead-end = 1)"
    );
    for case in &job.weather {
        let r = section
            .solve(&state_for(case), &default_fe_budget())
            .map_err(|e| format!("FE case \"{}\": {e}", case.name))?;
        row(out, &case.name, &r);
    }
    for broken in &fe.broken {
        if broken.span == 0 || broken.span > spans.len() {
            return Err(format!(
                "broken span {} does not exist (1 to {})",
                broken.span,
                spans.len()
            ));
        }
        let case = find_case(&job.weather, &broken.weather)?;
        let mut state = state_for(case);
        state.broken_spans = vec![broken.span - 1];
        let r = section
            .solve(&state, &default_fe_budget())
            .map_err(|e| format!("FE broken span {} ({}): {e}", broken.span, case.name))?;
        row(
            out,
            &format!("{} – span {} broken", case.name, broken.span),
            &r,
        );
        let longitudinal = r
            .supports
            .iter()
            .map(|s| s.load.x.abs())
            .fold(0.0, f64::max);
        let _ = writeln!(
            out,
            "  {:<24} largest longitudinal support load {}",
            "",
            show(Force::from_si(longitudinal), units, 2)
        );
    }
    Ok(())
}

/// Runs a job file.
pub fn run(path: &Path, units: UnitSystem) -> Result<JobOutput, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let job: Job = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let library_dir = match &job.library {
        Some(dir) => path.parent().unwrap_or(Path::new(".")).join(dir),
        None => ohl_core::files::shipped_library("cables"),
    };
    let library = Library::load(&library_dir).map_err(|e| e.to_string())?;
    let cable = library.get(&job.cable).ok_or_else(|| {
        format!(
            "wire type \"{}\" not found in {}",
            job.cable,
            library_dir.display()
        )
    })?;
    let elastic = ElasticCable {
        axial_stiffness: cable.axial_stiffness(),
        thermal_expansion: cable.thermal_expansion(),
    };
    let rts = cable.rated_tensile_strength;
    let spans: Vec<SpanGeometry> = job
        .spans
        .iter()
        .map(|s| SpanGeometry::inclined(s.span, s.rise))
        .collect();
    let reference_case = find_case(&job.weather, &job.reference.weather)?;
    // The data sheet's weight is at its reference temperature; strung at
    // the reference case's temperature, a metre of the cable holds
    // w e^(-alpha (t_s - t_ref)) (owner decision 2026-10-21), conserved
    // from there. The non-linear model takes the additive form's.
    let datasheet = cable;
    let strung_at = reference_case.temperature;
    let cable = &datasheet.with_mass_at(strung_at, datasheet.weight_factor_at(strung_at));
    let reference = state(reference_case, cable);

    let mechanics =
        ohl_cables::mechanics::Mechanics::from_cable(cable).map_err(|e| e.to_string())?;
    // The additive form's weight at stringing (the non-linear model).
    let nl_cable_owned = if mechanics.is_nonlinear() {
        let factor = mechanics
            .weight_factor(datasheet.mass_reference_temperature(), strung_at)
            .map_err(|e| e.to_string())?;
        Some(datasheet.with_mass_at(strung_at, factor))
    } else {
        None
    };
    let nl_cable = nl_cable_owned.as_ref().unwrap_or(cable);

    let mut out = String::new();
    let _ = writeln!(out, "OpenOHL sag-tension — {}", path.display());
    let _ = writeln!(out, "Wire type: {} ({})", cable.name, cable.id);
    let _ = writeln!(
        out,
        "  EA {:.2} MN, α {:.2e} /K, RTS {}, weight {} at {} (data sheet), {} at stringing ({})",
        elastic.axial_stiffness.si() / 1e6,
        elastic.thermal_expansion,
        show(rts, units, 1),
        show(datasheet.weight(), units, 3),
        show(datasheet.mass_reference_temperature(), units, 0),
        show(cable.weight(), units, 4),
        show(strung_at, units, 0)
    );
    let ruling = ruling_span(&spans).map_err(|e| e.to_string())?;
    let _ = writeln!(
        out,
        "Section: {} spans, total {}, ruling span {}",
        spans.len(),
        show(spans.iter().map(|s| s.span).sum::<Length>(), units, 1),
        show(ruling, units, 1)
    );

    // Stringing tension: fixed, or governed by the criteria.
    let stringing = match (&job.stringing, job.criterion.is_empty()) {
        (Some(s), _) => {
            let _ = writeln!(
                out,
                "\nStringing tension (given): {} at \"{}\"",
                show(s.horizontal_tension, units, 2),
                reference_case.name
            );
            s.horizontal_tension
        }
        (None, false) if mechanics.is_nonlinear() => nonlinear_criteria(
            &mut out,
            &job,
            nl_cable,
            &mechanics,
            &spans,
            reference_case,
            units,
        )?,
        (None, false) => {
            if let Some(c) = job
                .criterion
                .iter()
                .find(|c| c.condition.as_deref().is_some_and(|x| x != "initial"))
            {
                return Err(format!(
                    "criterion \"{}\": a condition after creep or load needs a wire type with \
                     stress-strain curves (the linear wire has no permanent set)",
                    c.weather
                ));
            }
            let criteria = job
                .criterion
                .iter()
                .map(|c| criterion(c, &job.weather, cable))
                .collect::<Result<Vec<_>, _>>()?;
            let governing = governing_tension(
                &spans,
                elastic,
                rts,
                reference,
                &criteria,
                &default_criteria_budget(),
            )
            .map_err(|e| e.to_string())?;
            let _ = writeln!(
                out,
                "\nStringing tension (governing): {} at \"{}\"",
                show(governing.reference_tension, units, 2),
                reference_case.name
            );
            let _ = writeln!(
                out,
                "  {:<28} {:<34} {:>11}",
                "criterion", "limit", "utilisation"
            );
            for (i, (c, r)) in criteria.iter().zip(&governing.criteria).enumerate() {
                let mark = if i == governing.governing {
                    "  ← governs"
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "  {:<28} {:<34} {:>10.1} %{mark}",
                    c.label,
                    c.limit.to_string(),
                    r.utilisation * 100.0
                );
            }
            governing.reference_tension
        }
        (None, true) => {
            return Err(
                "give either [stringing] horizontal_tension or [[criterion]] entries".into(),
            );
        }
    };

    let section = TensionSection::from_known_state(spans.clone(), elastic, reference, stringing)
        .map_err(|e| e.to_string())?;
    let budget = default_solver_budget();

    let _ = writeln!(out, "\nWeather cases");
    let _ = writeln!(
        out,
        "  {:<24} {:>9} {:>12} {:>14} {:>16} {:>7} {:>10}",
        "case", "temp", "load", "H", "max support T", "% RTS", "max sag"
    );
    for case in &job.weather {
        let s = state(case, cable);
        let solution = section
            .solve(s, &budget)
            .map_err(|e| format!("case \"{}\": {e}", case.name))?;
        let support = max_support(&solution);
        let sag = solution
            .spans
            .iter()
            .map(|sp| sp.sag())
            .fold(Length::ZERO, Length::max);
        let _ = writeln!(
            out,
            "  {:<24} {:>9} {:>12} {:>14} {:>16} {:>6.1}% {:>10}",
            case.name,
            show(case.temperature, units, 0),
            show(s.load, units, 2),
            show(solution.horizontal_tension, units, 2),
            show(support, units, 2),
            support / rts * 100.0,
            show(sag, units, 2)
        );
    }

    if mechanics.is_nonlinear() {
        nonlinear_report(
            &mut out,
            &job,
            nl_cable,
            mechanics.clone(),
            &spans,
            stringing,
            reference_case,
            units,
        )?;
    }

    if let Some(input) = &job.sagging {
        if mechanics.is_nonlinear() {
            use ohl_cables::mechanics::MaterialState;
            use ohl_sagtension::nonlinear::NonlinearSection;
            let ns = NonlinearSection::from_known_state(
                spans.clone(),
                mechanics.clone(),
                state(reference_case, nl_cable),
                stringing,
                &MaterialState::Initial,
            )
            .map_err(|e| e.to_string())?;
            let bare = |t: Temperature| state(&WeatherCase::still("sagging", t), nl_cable);
            sagging_report(
                &mut out,
                input,
                reference_case,
                &|t| {
                    ns.solve(bare(t), &MaterialState::Initial, &budget)
                        .map_err(|e| e.to_string())
                },
                &|sol, t| {
                    ns.span_unstressed_lengths(sol, &bare(t), &MaterialState::Initial)
                        .map_err(|e| e.to_string())
                },
                units,
            )?;
        } else {
            let bare = |t: Temperature| state(&WeatherCase::still("sagging", t), cable);
            sagging_report(
                &mut out,
                input,
                reference_case,
                &|t| section.solve(bare(t), &budget).map_err(|e| e.to_string()),
                &|sol, t| Ok(section.span_unstressed_lengths(sol, &bare(t))),
                units,
            )?;
        }
    }

    if let Some(fe) = &job.fe {
        fe_report(
            &mut out,
            fe,
            &job,
            cable,
            &spans,
            elastic,
            stringing,
            reference_case,
            units,
        )?;
    }

    let chart_csv = match &job.chart {
        None => None,
        Some(chart) => {
            let bare = WeatherCase::still("bare", Temperature::from_celsius(15.0));
            let _ = writeln!(
                out,
                "\nStringing chart (bare conductor, still air): horizontal tension and sag per span"
            );
            let mut header = format!("  {:>9} {:>12}", "temp", "H");
            let mut csv = String::from("temperature_c,horizontal_tension_n");
            for i in 1..=spans.len() {
                let _ = write!(header, " {:>14}", format!("sag {i}"));
                let _ = write!(csv, ",sag_span{i}_m");
            }
            let _ = writeln!(out, "{header}");
            csv.push('\n');
            for &t in &chart.temperatures {
                let case = WeatherCase {
                    temperature: t,
                    ..bare.clone()
                };
                let solution = section
                    .solve(state(&case, cable), &budget)
                    .map_err(|e| format!("chart at {:.1} °C: {e}", t.celsius()))?;
                let mut line = format!(
                    "  {:>9} {:>12}",
                    show(t, units, 0),
                    show(solution.horizontal_tension, units, 2)
                );
                let _ = write!(
                    csv,
                    "{},{}",
                    t.celsius(),
                    solution.horizontal_tension.newtons()
                );
                for span in &solution.spans {
                    let _ = write!(line, " {:>14}", show(span.sag(), units, 3));
                    let _ = write!(csv, ",{}", span.sag().meters());
                }
                let _ = writeln!(out, "{line}");
                csv.push('\n');
            }
            Some(csv)
        }
    };

    let _ = writeln!(
        out,
        "\nModel: exact catenary, equal horizontal tension in the section; the weather table uses the\n\
         linear-elastic wire (non-linear curves, when the wire type has them, are reported separately above,\n\
         and the criteria then use them). Wind and ice use the resultant load in the span plane."
    );
    Ok(JobOutput {
        report: out,
        chart_csv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two strings to one clamp: `[fe.two_strings]`, and the older
    /// `[fe.v_string]` still read the same.
    #[test]
    fn two_strings_and_the_older_v_string_table() {
        let body = "half_width = \"2 m\"\nleg_length = \"4 m\"\nleg_weight = \"500 N\"\naxial_stiffness = \"50000 kN\"\n";
        for table in ["two_strings", "v_string"] {
            let fe: FeInput = toml::from_str(&format!("[{table}]\n{body}")).unwrap();
            let v = fe.two_strings.expect(table);
            assert_eq!(v.half_width.si(), 2.0, "{table}");
            assert_eq!(v.leg_length.si(), 4.0, "{table}");
        }
    }

    fn example() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sagtension/drake-section.toml")
    }

    #[test]
    fn example_job_runs_and_everyday_limit_governs() {
        let output = run(&example(), UnitSystem::Metric).unwrap();
        // 18 % of Drake's 138.8 kN RTS = 24.98 kN.
        assert!(
            output
                .report
                .contains("Stringing tension (governing): 24.98 kN"),
            "{}",
            output.report
        );
        assert!(output.report.contains("← governs"));
        // FE section: valley structure (support 3) swings most in high wind.
        assert!(
            output.report.contains("Finite-element section"),
            "{}",
            output.report
        );
        let high_wind = output
            .report
            .lines()
            .rfind(|l| l.trim_start().starts_with("high wind"))
            .unwrap();
        assert!(high_wind.trim_end().ends_with("at 3"), "{high_wind}");
        let csv = output.chart_csv.unwrap();
        assert_eq!(csv.lines().count(), 1 + 9, "header + 9 temperatures");
        assert!(csv.starts_with("temperature_c,horizontal_tension_n,sag_span1_m"));
    }

    #[test]
    fn nonlinear_example_reports_initial_and_final() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/sagtension/synthetic-nonlinear.toml");
        let report = run(&path, UnitSystem::Metric).unwrap().report;
        assert!(report.contains("at the creep tension: load"), "{report}");
        // At the heaviest case (the load-stretch case), after load equals
        // initial: H initial, H creep, H load, governs, sags.
        let line = report
            .lines()
            .rfind(|l| l.trim_start().starts_with("cold + ice"))
            .unwrap();
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(cols[3], cols[7], "{line}");
        assert_eq!(cols[10], cols[12], "{line}");
        // Hot: both final states reported, the governing one's sag.
        let hot = report
            .lines()
            .rfind(|l| l.trim_start().starts_with("hot 75"))
            .unwrap();
        assert!(hot.contains("load") || hot.contains("creep"), "{hot}");
        // The sagging outputs.
        assert!(report.contains("Sagging (in the blocks"), "{report}");
        assert!(report.contains("Transit, span 2"), "{report}");
    }

    /// A job file next to the examples, from `text` (the library path made
    /// absolute).
    fn job(name: &str, text: &str) -> PathBuf {
        let library = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/sagtension/synthetic-library");
        let path = std::env::temp_dir().join(name);
        std::fs::write(
            &path,
            text.replace(
                "library = \"synthetic-library\"",
                &format!("library = '{}'", library.display()),
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn criteria_on_the_nonlinear_model_with_conditions() {
        let synthetic = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/sagtension/synthetic-nonlinear.toml");
        // No line endings in the patterns: Windows checkouts may use CRLF.
        let text = std::fs::read_to_string(synthetic).unwrap().replace("[stringing]", "").replace(
            "horizontal_tension = \"25 kN\"",
            "[[criterion]]\nweather = \"everyday\"\nlimit = \"horizontal\"\nvalue = \"25 kN\"\n\n\
             [[criterion]]\nweather = \"everyday\"\nlimit = \"horizontal\"\nvalue = \"25 kN\"\n\
             condition = \"creep\"",
        );
        let report = run(&job("ohl-cli-nl-criteria.toml", &text), UnitSystem::Metric)
            .unwrap()
            .report;
        // The initial one at its own case governs: the stringing tension is
        // its limit; after creep the same limit is met with room.
        assert!(
            report.contains("Stringing tension (governing, non-linear model): 25.00 kN"),
            "{report}"
        );
        assert!(report.contains("after creep"), "{report}");
        // A condition on a linear wire is refused.
        let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables");
        let linear = std::fs::read_to_string(example())
            .unwrap()
            .replace("value = 0.18", "value = 0.18\ncondition = \"creep\"")
            .replace(
                "\"../../libraries/cables\"",
                &format!("'{}'", library.display()),
            );
        let path = std::env::temp_dir().join("ohl-cli-linear-condition.toml");
        std::fs::write(&path, linear).unwrap();
        let error = run(&path, UnitSystem::Metric).err().unwrap();
        assert!(
            error.contains("needs a wire type with stress-strain curves"),
            "{error}"
        );
    }

    #[test]
    fn sagging_outputs_of_the_example() {
        let report = run(&example(), UnitSystem::Metric).unwrap().report;
        assert!(
            report.contains("Sagging (in the blocks, initial state"),
            "{report}"
        );
        assert!(
            report.contains("Clipping offsets, clipped in at 10.0 °C"),
            "{report}"
        );
        assert!(report.contains("Transit, span 3"), "{report}");
    }

    #[test]
    fn unknown_weather_case_is_reported() {
        let text = std::fs::read_to_string(example())
            .unwrap()
            // No line ending in the pattern: Windows checkouts may use CRLF.
            .replace("weather = \"everyday\"", "weather = \"nope\"");
        let path = std::env::temp_dir().join("ohl-cli-bad-job.toml");
        // Absolute library path as a TOML literal string ('…'): Windows paths
        // contain backslashes, which are escape characters in "…" strings
        // (this broke the test on Windows CI).
        let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables");
        std::fs::write(
            &path,
            text.replace(
                "\"../../libraries/cables\"",
                &format!("'{}'", library.display()),
            ),
        )
        .unwrap();
        let error = run(&path, UnitSystem::Metric).err().unwrap();
        assert!(error.contains("unknown weather case \"nope\""), "{error}");
    }
}
