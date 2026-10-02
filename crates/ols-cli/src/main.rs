// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols` — OLS CAD command-line interface.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use ols_core::units::UnitSystem;

mod sagtension_job;

#[derive(Parser)]
#[command(
    name = "ols",
    version,
    about = "OLS CAD — Open Line System Computer Assisted Design"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print version and build information.
    Info,
    /// Run validation cases and report against their reference results.
    Validate {
        /// Case file or directory of case files.
        #[arg(default_value = "benchmarks")]
        path: PathBuf,
        /// Also write the report as Markdown to this file.
        #[arg(long)]
        report: Option<PathBuf>,
        /// List the calculations available to validation cases and exit.
        #[arg(long)]
        list: bool,
    },
    /// Sag-tension of a tension section from a TOML job file.
    Sagtension {
        /// Job file (see examples/sagtension/).
        job: PathBuf,
        /// Units used in the report.
        #[arg(long, value_enum, default_value = "metric")]
        units: Units,
        /// Write the stringing chart as CSV (SI units) to this file.
        #[arg(long)]
        csv: Option<PathBuf>,
    },
    /// Solve a line model (.ols.json) in 3D for one weather state.
    Line {
        /// Line file (.ols.json).
        file: PathBuf,
        /// Wire library: .olsw file or directory.
        #[arg(long, default_value = "libraries/cables")]
        library: PathBuf,
        /// Cable temperature, °C.
        #[arg(long, default_value_t = 15.0)]
        temperature: f64,
        /// Wind pressure on the wires, Pa (perpendicular to each span).
        #[arg(long, default_value_t = 0.0)]
        wind_pressure: f64,
        /// Radial ice thickness, mm.
        #[arg(long, default_value_t = 0.0)]
        ice_thickness: f64,
        /// Write a standalone 3D viewer (HTML).
        #[arg(long)]
        html: Option<PathBuf>,
        /// Write all wire points as CSV.
        #[arg(long)]
        csv: Option<PathBuf>,
        /// Write one row per span (tensions, sag, mid and low points) as CSV.
        #[arg(long)]
        spans_csv: Option<PathBuf>,
    },
    /// Inspect plugins.
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Units {
    Metric,
    Us,
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Load a WebAssembly plugin from its directory and print what it reports.
    Info {
        /// Directory containing `ols-plugin.toml`.
        dir: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Info) | None => {
            print_info();
            ExitCode::SUCCESS
        }
        Some(Command::Validate { path, report, list }) => validate(&path, report.as_deref(), list),
        Some(Command::Sagtension { job, units, csv }) => sagtension(&job, units, csv.as_deref()),
        Some(Command::Line {
            file,
            library,
            temperature,
            wind_pressure,
            ice_thickness,
            html,
            csv,
            spans_csv,
        }) => line(
            &file,
            &library,
            (temperature, wind_pressure, ice_thickness),
            [html.as_deref(), csv.as_deref(), spans_csv.as_deref()],
        ),
        Some(Command::Plugin {
            command: PluginCommand::Info { dir },
        }) => plugin_info(&dir),
    }
}

fn print_info() {
    println!("OLS CAD {}", env!("CARGO_PKG_VERSION"));
    println!("core    {}", ols_core::VERSION);
    println!("license MPL-2.0");
}

fn sagtension(job: &std::path::Path, units: Units, csv: Option<&std::path::Path>) -> ExitCode {
    let units = match units {
        Units::Metric => UnitSystem::Metric,
        Units::Us => UnitSystem::UsCustomary,
    };
    match sagtension_job::run(job, units) {
        Ok(output) => {
            print!("{}", output.report);
            if let Some(path) = csv {
                let Some(chart) = output.chart_csv else {
                    eprintln!("error: --csv given but the job has no [chart] section");
                    return ExitCode::FAILURE;
                };
                if let Err(error) = std::fs::write(path, chart) {
                    eprintln!("error: cannot write {}: {error}", path.display());
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn line(
    file: &std::path::Path,
    library: &std::path::Path,
    (temperature, wind_pressure, ice_thickness): (f64, f64, f64),
    [html, csv, spans_csv]: [Option<&std::path::Path>; 3],
) -> ExitCode {
    use ols_core::units::{Length, Stress, Temperature};
    use ols_weather::WeatherCase;

    let run = || -> Result<(), String> {
        let line = ols_line::Line::load(file).map_err(|e| e.to_string())?;
        let library = ols_cables::Library::load(library).map_err(|e| e.to_string())?;
        let mut case = WeatherCase::still("state", Temperature::from_celsius(temperature));
        case.wind_pressure = Stress::from_pascals(wind_pressure);
        case.ice_thickness = Length::from_millimeters(ice_thickness);
        let started = std::time::Instant::now();
        let results = ols_line::solve::solve_line(&line, &library, &case);
        let elapsed = started.elapsed();
        println!(
            "{}: {} structures, {} sections — {temperature} °C, wind {wind_pressure} Pa, ice {ice_thickness} mm",
            line.name,
            line.structures.len(),
            line.sections.len()
        );
        println!(
            "  {:<12} {:<22} {:>5} {:>12} {:>10} {:>6}",
            "section", "circuit / phase", "spans", "max T (kN)", "max sag", "iter"
        );
        let mut shapes = Vec::new();
        let mut failed = 0;
        for (section, result) in line.sections.iter().zip(results) {
            match result {
                Ok(shape) => {
                    let max_t = shape
                        .spans
                        .iter()
                        .map(|s| s.max_tension)
                        .fold(0.0, f64::max);
                    let max_sag = shape.spans.iter().map(|s| s.sag).fold(0.0, f64::max);
                    println!(
                        "  {:<12} {:<22} {:>5} {:>12.2} {:>8.2} m {:>6}",
                        section.id,
                        format!("{} {}", section.circuit, section.phase_label),
                        shape.spans.len(),
                        max_t / 1e3,
                        max_sag,
                        shape.iterations
                    );
                    shapes.push(shape);
                }
                Err(error) => {
                    failed += 1;
                    println!("  {:<12} error: {error}", section.id);
                }
            }
        }
        println!(
            "solved {} of {} sections in {elapsed:.1?}",
            shapes.len(),
            line.sections.len()
        );
        if let Some(path) = html {
            std::fs::write(
                path,
                ols_line::export::viewer_html(&line, &shapes, &line.name),
            )
            .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if let Some(path) = csv {
            std::fs::write(path, ols_line::export::wires_csv(&shapes))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if let Some(path) = spans_csv {
            std::fs::write(path, ols_line::export::spans_csv(&shapes))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if failed > 0 {
            return Err(format!("{failed} sections failed"));
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn plugin_info(dir: &std::path::Path) -> ExitCode {
    let result = ols_plugins::WasmHost::new().and_then(|host| {
        let mut plugin = host.load(dir)?;
        let info = plugin.describe()?;
        Ok((plugin.manifest().clone(), info))
    });
    match result {
        Ok((manifest, info)) => {
            println!("name         {}", info.name);
            println!("version      {}", info.version);
            println!("description  {}", info.description);
            println!("tier         {}", manifest.plugin.tier);
            println!("sdk          {}", manifest.plugin.sdk);
            println!("entry        {}", manifest.entry_path().display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn validate(path: &std::path::Path, report_path: Option<&std::path::Path>, list: bool) -> ExitCode {
    use ols_validation::{Outcome, Registry, Report, load_cases};

    let registry = Registry::with_builtin();
    if list {
        for calculation in registry.iter() {
            println!("{:<24} {}", calculation.id(), calculation.description());
        }
        return ExitCode::SUCCESS;
    }

    let cases = match load_cases(path) {
        Ok(cases) => cases,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let report = Report::run(&registry, cases);

    for result in &report.results {
        let case = &result.case.case;
        let mark = if result.passed() { "ok  " } else { "FAIL" };
        println!(
            "{mark} {:<16} {:>9.1?}  {}",
            case.id, result.elapsed, case.title
        );
        if !result.passed() {
            match &result.outcome {
                Outcome::Error(message) => println!("       error: {message}"),
                Outcome::TimedOut(limit) => println!("       exceeded time limit of {limit:?}"),
                Outcome::Ran(checks) => {
                    for check in checks.iter().filter(|c| !c.passed()) {
                        let actual = check.actual.map_or("missing".into(), |a| a.to_string());
                        println!(
                            "       {}: expected {}, got {actual}",
                            check.name, check.expected.value
                        );
                    }
                }
            }
        }
    }
    println!(
        "\n{} cases: {} passed, {} failed",
        report.results.len(),
        report.passed(),
        report.failed()
    );

    if let Some(report_path) = report_path
        && let Err(error) = std::fs::write(report_path, report.to_markdown())
    {
        eprintln!("error: cannot write {}: {error}", report_path.display());
        return ExitCode::from(2);
    }

    if report.all_passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
