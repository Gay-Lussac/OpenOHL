// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols sagtension`: sag-tension job for one tension section.
//!
//! Reads a TOML job (cable from a library, spans, weather cases, either a
//! fixed stringing tension or tension criteria, chart temperatures) and
//! reports the ruling span, the stringing tension, every weather case and a
//! stringing chart. See `examples/sagtension/drake-section.toml`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ols_cables::{Cable, Library};
use ols_core::units::{Force, Length, Quantity, Temperature, UnitSystem};
use ols_sagtension::catenary::default_solver_budget;
use ols_sagtension::criteria::default_criteria_budget;
use ols_sagtension::{
    ElasticCable, LoadState, SectionSolution, SpanGeometry, TensionCriterion, TensionLimit,
    TensionSection, governing_tension, ruling_span,
};
use ols_weather::WeatherCase;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    /// Cable id in the library.
    cable: String,
    /// Library directory, relative to the job file (default: `libraries/cables`
    /// in the current directory).
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
    LoadState {
        load: case.wire_load(cable.diameter, cable.weight()).resultant(),
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

/// Runs a job file.
pub fn run(path: &Path, units: UnitSystem) -> Result<JobOutput, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let job: Job = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let library_dir = match &job.library {
        Some(dir) => path.parent().unwrap_or(Path::new(".")).join(dir),
        None => PathBuf::from("libraries/cables"),
    };
    let library = Library::load_dir(&library_dir).map_err(|e| e.to_string())?;
    let cable = library.get(&job.cable).ok_or_else(|| {
        format!(
            "cable \"{}\" not found in {}",
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
    let reference = state(reference_case, cable);

    let mut out = String::new();
    let _ = writeln!(out, "OLS CAD sag-tension — {}", path.display());
    let _ = writeln!(out, "Cable: {} ({})", cable.name, cable.id);
    let _ = writeln!(
        out,
        "  EA {:.2} MN, α {:.2e} /K, RTS {}, weight {}",
        elastic.axial_stiffness.si() / 1e6,
        elastic.thermal_expansion,
        show(rts, units, 1),
        show(cable.weight(), units, 3)
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
        "\nModel: linear elastic + thermal, exact catenary, equal horizontal tension in the section.\n\
         Wind and ice use the resultant load in the span plane. Creep and non-linear stress-strain are not yet included."
    );
    Ok(JobOutput {
        report: out,
        chart_csv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let csv = output.chart_csv.unwrap();
        assert_eq!(csv.lines().count(), 1 + 9, "header + 9 temperatures");
        assert!(csv.starts_with("temperature_c,horizontal_tension_n,sag_span1_m"));
    }

    #[test]
    fn unknown_weather_case_is_reported() {
        let text = std::fs::read_to_string(example())
            .unwrap()
            .replace("weather = \"everyday\"\n", "weather = \"nope\"\n");
        let path = std::env::temp_dir().join("ols-cli-bad-job.toml");
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
