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
    Line(Box<LineArgs>),
    /// Structure and insulator libraries.
    Structures {
        #[command(subcommand)]
        command: StructuresCommand,
    },
    /// Inspect plugins.
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
}

/// Arguments of `ols line`.
#[derive(clap::Args)]
struct LineArgs {
    /// Line file (.ols.json).
    file: PathBuf,
    /// Wire library: .olsw file or directory.
    #[arg(long, default_value = "libraries/cables")]
    library: PathBuf,
    /// Structure type library directory (.olss), for typed structures.
    #[arg(long, default_value = "libraries/structures")]
    structures: PathBuf,
    /// Insulator assembly library directory (.olsi).
    #[arg(long, default_value = "libraries/insulators")]
    insulators: PathBuf,
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
    /// Print stationing, offsets and line angles of every route.
    #[arg(long)]
    stations: bool,
    /// Survey points (CSV / XYZ) for the ground surface: enables the
    /// ground clearance check and draws the ground in the viewer.
    #[arg(long)]
    terrain: Option<PathBuf>,
    /// Required vertical ground clearance, m: spans below it are listed.
    #[arg(long)]
    clearance: Option<f64>,
    /// Write the lowest ground clearance of every span as CSV.
    #[arg(long)]
    clearance_csv: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Units {
    Metric,
    Us,
}

#[derive(Subcommand)]
enum StructuresCommand {
    /// Derive draft structure types (.olss) and insulator assemblies (.olsi)
    /// from a line's attachment geometry, plus a copy of the line that
    /// references them; reports how closely the types reproduce the line.
    Derive {
        /// Line file (.ols.json).
        file: PathBuf,
        /// Output directory (created): structures/, insulators/, typed line.
        #[arg(long)]
        out: PathBuf,
    },
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
        Some(Command::Line(args)) => line(&args),
        Some(Command::Structures {
            command: StructuresCommand::Derive { file, out },
        }) => structures_derive(&file, &out),
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

fn line(args: &LineArgs) -> ExitCode {
    let LineArgs {
        file,
        library,
        structures: structure_dir,
        insulators: insulator_dir,
        temperature,
        wind_pressure,
        ice_thickness,
        html,
        csv,
        spans_csv,
        stations,
        terrain,
        clearance,
        clearance_csv,
    } = args;
    let (temperature, wind_pressure, ice_thickness) =
        (*temperature, *wind_pressure, *ice_thickness);
    use ols_core::units::{Length, Stress, Temperature};
    use ols_weather::WeatherCase;

    let run = || -> Result<(), String> {
        let mut line = ols_line::Line::load(file).map_err(|e| e.to_string())?;
        if line.structures.iter().any(|s| s.structure_type.is_some()) {
            let structures = ols_structures::StructureLibrary::load_dir(structure_dir)
                .map_err(|e| e.to_string())?;
            let insulators = ols_structures::InsulatorLibrary::load_dir(insulator_dir)
                .map_err(|e| e.to_string())?;
            line = line
                .resolve(&structures, &insulators)
                .map_err(|e| e.to_string())?;
        }
        let library = ols_cables::Library::load(library).map_err(|e| e.to_string())?;
        let mut case = WeatherCase::still("state", Temperature::from_celsius(temperature));
        case.wind_pressure = Stress::from_pascals(wind_pressure);
        case.ice_thickness = Length::from_millimeters(ice_thickness);
        if *stations {
            for route in line.effective_routes() {
                println!("Route {} {}", route.id, route.name);
                println!(
                    "  {:<12} {:>11} {:>9} {:>10} {:>10}  also on routes",
                    "structure", "station m", "offset m", "angle °", "ahead m"
                );
                // Print tiny rounding values as 0.00, not -0.00.
                let clean = |x: f64| if x.abs() < 0.005 { 0.0 } else { x };
                for st in route.stations(&line).map_err(|e| e.to_string())? {
                    let others: Vec<String> = line
                        .routes_of(&st.structure)
                        .into_iter()
                        .filter(|r| *r != route.id)
                        .collect();
                    println!(
                        "  {:<12} {:>11.2} {:>9.2} {:>10.2} {:>10.2}  {}",
                        st.structure,
                        clean(st.station),
                        clean(st.offset),
                        clean(st.line_angle_deg),
                        clean(st.ahead_span),
                        others.join(", ")
                    );
                }
            }
        }
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
                    // Slack V-string legs: the clamp hangs on one leg.
                    for (point, forces) in section.points.iter().zip(&shape.insulators) {
                        if forces.len() == 2 && forces.contains(&0.0) {
                            println!(
                                "  {:<12}   slack V-string leg at {} set {} phase {}",
                                "", point.structure, point.set, point.phase
                            );
                        }
                    }
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
        let mut extras = ols_line::export::ViewerExtras::default();
        if !line.jumpers.is_empty() {
            println!(
                "  {:<16} {:>10} {:>9} {:>11}",
                "jumper", "length m", "sag m", "max T (N)"
            );
            for (jumper, result) in line.jumpers.iter().zip(ols_line::jumper::solve_jumpers(
                &line, &library, &case, &shapes,
            )) {
                match result {
                    Ok(shape) => {
                        println!(
                            "  {:<16} {:>10.3} {:>9.3} {:>11.0}{}",
                            jumper.id,
                            shape.unstressed_length,
                            shape.sag,
                            shape.max_tension,
                            match shape.support {
                                Some((_, _, f)) => format!("   support string {f:.0} N"),
                                None => String::new(),
                            }
                        );
                        extras.jumpers.push(shape);
                    }
                    Err(error) => {
                        failed += 1;
                        println!("  {:<16} error: {error}", jumper.id);
                    }
                }
            }
        }
        if let Some(path) = terrain {
            let started = std::time::Instant::now();
            let ground = ols_terrain::Terrain::load(path).map_err(|e| e.to_string())?;
            let clearances: Vec<_> = shapes
                .iter()
                .flat_map(|shape| ols_clearance::ground_clearances(shape, &ground))
                .chain(
                    extras
                        .jumpers
                        .iter()
                        .map(|j| ols_clearance::jumper_clearance(j, &ground)),
                )
                .collect();
            println!(
                "terrain: {} points, {} triangles{} — loaded and {} spans checked in {:.1?}",
                ground.num_points(),
                ground.num_triangles(),
                match ground.skipped_breaklines() {
                    0 => String::new(),
                    n => format!(", {n} crossing breakline edges skipped"),
                },
                clearances.len(),
                started.elapsed()
            );
            let lowest = clearances
                .iter()
                .filter_map(|c| c.lowest.map(|l| (c, l)))
                .min_by(|a, b| a.1.clearance.total_cmp(&b.1.clearance));
            if let Some((c, l)) = lowest {
                println!(
                    "  lowest vertical ground clearance {:.2} m: section {} span {} at ({:.1}, {:.1})",
                    l.clearance, c.section, c.span, l.wire[0], l.wire[1]
                );
            }
            let partly = clearances.iter().filter(|c| c.covered < 0.999).count();
            if partly > 0 {
                println!("  {partly} spans are not fully over the surveyed area");
            }
            if let Some(required) = clearance {
                let found = ols_clearance::violations(&clearances, *required);
                println!(
                    "  spans below the required {required:.2} m: {}",
                    found.len()
                );
                for c in &found {
                    let l = c.lowest.expect("violations have a lowest point");
                    println!(
                        "    section {:<10} span {:>3}: {:>6.2} m at ({:.1}, {:.1}, wire {:.2} m)",
                        c.section, c.span, l.clearance, l.wire[0], l.wire[1], l.wire[2]
                    );
                    extras.markers.push((
                        l.wire,
                        format!("{} span {}: {:.2} m", c.section, c.span, l.clearance),
                    ));
                }
            }
            if let Some(path) = clearance_csv {
                let mut out = String::from(
                    "section,span,clearance_m,wire_x,wire_y,wire_z,ground_z,covered\n",
                );
                for c in &clearances {
                    match c.lowest {
                        Some(l) => out.push_str(&format!(
                            "{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}\n",
                            c.section,
                            c.span,
                            l.clearance,
                            l.wire[0],
                            l.wire[1],
                            l.wire[2],
                            l.ground[2],
                            c.covered
                        )),
                        None => out.push_str(&format!("{},{},,,,,,0\n", c.section, c.span)),
                    }
                }
                std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            extras.ground = Some(ground.mesh());
        } else if clearance.is_some() || clearance_csv.is_some() {
            return Err("--clearance and --clearance-csv need --terrain".into());
        }
        if let Some(path) = html {
            std::fs::write(
                path,
                ols_line::export::viewer_html_with(&line, &shapes, &line.name, &extras),
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
            return Err(format!("{failed} sections or jumpers failed"));
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

fn structures_derive(file: &std::path::Path, out: &std::path::Path) -> ExitCode {
    use ols_structures::{
        InsulatorAssembly, InsulatorLibrary, LibraryItem, StructureLibrary, StructureType,
    };

    let run = || -> Result<(), String> {
        let line = ols_line::Line::load(file).map_err(|e| e.to_string())?;
        let (derived, typed) = line.derive_libraries();
        let (sdir, idir) = (out.join("structures"), out.join("insulators"));
        for dir in [&sdir, &idir] {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut structures = StructureLibrary::default();
        let mut insulators = InsulatorLibrary::default();
        for (id, text) in &derived.structure_types {
            let path = sdir.join(format!("{id}.olss"));
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            structures
                .insert(
                    &path,
                    StructureType::parse(&path, text).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
        }
        for (id, text) in &derived.insulators {
            let path = idir.join(format!("{id}.olsi"));
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            insulators
                .insert(
                    &path,
                    InsulatorAssembly::parse(&path, text).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
        }
        let stem = file
            .file_name()
            .and_then(|n| n.to_str())
            .map_or("line", |n| n.trim_end_matches(".ols.json"));
        let typed_path = out.join(format!("{stem}-typed.ols.json"));
        std::fs::write(&typed_path, typed.to_json())
            .map_err(|e| format!("{}: {e}", typed_path.display()))?;
        // Self-check: resolve the typed line and compare with the original.
        let resolved = typed
            .resolve(&structures, &insulators)
            .map_err(|e| e.to_string())?;
        let check = ols_line::library::compare_attachments(&line, &resolved);
        let worst = (check.worst, check.at);
        println!(
            "{} structures -> {} structure types, {} insulator assemblies",
            line.structures.len(),
            derived.structure_types.len(),
            derived.insulators.len()
        );
        println!(
            "written to {} (typed line: {})",
            out.display(),
            typed_path.display()
        );
        println!(
            "self-check: attachment points and insulator lengths reproduced within {:.3} m (largest at {})",
            worst.0, worst.1
        );
        println!(
            "Note: insulator weights come from the line when given; check them, and fill in stiffness and wind areas."
        );
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
