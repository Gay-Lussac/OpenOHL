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
    use ohl_sagtension::nonlinear::{FinalCause, NonlinearSection, final_condition};

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
    let budget = default_solver_budget();
    let section = NonlinearSection::from_known_state(
        spans.to_vec(),
        mechanics,
        state(reference_case, cable),
        stringing,
        &MaterialState::Initial,
    )
    .map_err(|e| e.to_string())?;
    let (final_state, cause) = final_condition(
        &section,
        state(creep_case, cable),
        state(load_case, cable),
        &budget,
    )
    .map_err(|e| e.to_string())?;
    let _ = writeln!(
        out,
        "\nNon-linear stress-strain and creep (strung on the initial curve at {} / \"{}\")",
        show(stringing, units, 2),
        reference_case.name
    );
    let _ = writeln!(
        out,
        "  Final condition governed by {} (creep at \"{}\", heaviest load \"{}\")",
        match cause {
            FinalCause::Creep => "creep",
            FinalCause::Load => "load",
        },
        creep_case.name,
        load_case.name
    );
    let _ = writeln!(
        out,
        "  {:<24} {:>12} {:>12} {:>12} {:>12}",
        "case", "H initial", "H final", "sag initial", "sag final"
    );
    for case in &job.weather {
        let s = state(case, cable);
        let initial = section
            .solve(s, &MaterialState::Initial, &budget)
            .map_err(|e| format!("case \"{}\": {e}", case.name))?;
        let final_ = section
            .solve(s, &final_state, &budget)
            .map_err(|e| format!("case \"{}\": {e}", case.name))?;
        let sag = |sol: &SectionSolution| {
            sol.spans
                .iter()
                .map(|sp| sp.sag())
                .fold(Length::ZERO, Length::max)
        };
        let _ = writeln!(
            out,
            "  {:<24} {:>12} {:>12} {:>12} {:>12}",
            case.name,
            show(initial.horizontal_tension, units, 2),
            show(final_.horizontal_tension, units, 2),
            show(sag(&initial), units, 2),
            show(sag(&final_), units, 2)
        );
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
        (None, false) => {
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

    let mechanics =
        ohl_cables::mechanics::Mechanics::from_cable(cable).map_err(|e| e.to_string())?;
    if mechanics.is_nonlinear() {
        // The additive form's weight at stringing.
        let factor = mechanics
            .weight_factor(datasheet.mass_reference_temperature(), strung_at)
            .map_err(|e| e.to_string())?;
        nonlinear_report(
            &mut out,
            &job,
            &datasheet.with_mass_at(strung_at, factor),
            mechanics,
            &spans,
            stringing,
            reference_case,
            units,
        )?;
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
        "\nModel: exact catenary, equal horizontal tension in the section; weather table and criteria use the\n\
         linear-elastic wire (non-linear curves, when the wire type has them, are reported separately above).\n\
         Wind and ice use the resultant load in the span plane."
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
        assert!(
            report.contains("Final condition governed by load"),
            "{report}"
        );
        // At the governing heaviest case, final equals initial.
        let line = report
            .lines()
            .rfind(|l| l.trim_start().starts_with("cold + ice"))
            .unwrap();
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(cols[3], cols[5], "{line}");
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
