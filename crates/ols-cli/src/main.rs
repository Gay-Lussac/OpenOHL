// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols` — OLS CAD command-line interface.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use ols_core::units::UnitSystem;

mod clearance_run;
mod project;
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
    /// LiDAR point clouds (LAS / LAZ).
    Lidar {
        #[command(subcommand)]
        command: LidarCommand,
    },
    /// Project file (.ols): import/export, commands, undo, log, users.
    Project {
        #[command(subcommand)]
        command: project::ProjectCommand,
    },
    /// Point clouds stored in a project file.
    Pointcloud {
        #[command(subcommand)]
        command: PointcloudCommand,
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
    /// Line file (.ols.json) or project file (.ols).
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
    /// LiDAR tiles (LAS/LAZ file or directory): ground surface built from
    /// the classes with the ground role, inside a corridor along the spans.
    #[arg(long, conflicts_with = "terrain")]
    lidar: Option<PathBuf>,
    /// Point cloud stored in a project file (`FILE.ols`, or `FILE.ols#name`
    /// when it holds several): much faster than reading tiles.
    #[arg(long, conflicts_with_all = ["terrain", "lidar"])]
    pointcloud: Option<String>,
    /// Class scheme (.olsc) of the LiDAR; default ASPRS LAS 1.4.
    #[arg(long)]
    lidar_classes: Option<PathBuf>,
    /// Role of the classes that make the ground surface.
    #[arg(long, default_value = "ground")]
    ground_role: String,
    /// Half width of the LiDAR corridor around the spans, m.
    #[arg(long, default_value_t = 60.0)]
    corridor: f64,
    /// Longest triangle side of the LiDAR ground surface, m (no ground is
    /// invented across larger gaps).
    #[arg(long, default_value_t = 50.0)]
    max_edge: f64,
    /// Thin the LiDAR ground points: lowest point per square cell of this
    /// size, m.
    #[arg(long)]
    thin: Option<f64>,
    /// Required vertical ground clearance, m (a ground rule; adds to
    /// --rules).
    #[arg(long)]
    clearance: Option<f64>,
    /// Clearance rules (.olsr): vertical and radial clearances to the
    /// ground, LiDAR classes, other lines and wires of this line.
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Another line (.ols.json) for wire-to-wire clearances (repeatable);
    /// solved in the same weather with the same libraries.
    #[arg(long)]
    other_line: Vec<PathBuf>,
    /// Write every clearance found (per rule, wire and measure) as CSV.
    #[arg(long)]
    clearance_csv: Option<PathBuf>,
    /// Write a plan and profile sheet (SVG) and profile CSV per route into
    /// this directory.
    #[arg(long)]
    drawings: Option<PathBuf>,
    /// Horizontal scale of the drawings (2000 = 1:2000).
    #[arg(long, default_value_t = 2000.0)]
    hscale: f64,
    /// Vertical scale of the profile (200 = 1:200).
    #[arg(long, default_value_t = 200.0)]
    vscale: f64,
    /// Side ground profile at this offset, m (right positive; repeatable).
    #[arg(long, allow_hyphen_values = true)]
    side_offset: Vec<f64>,
    /// Half width of the plan view, m.
    #[arg(long, default_value_t = 40.0)]
    plan_width: f64,
    /// Scale of the plan across the line (default: the horizontal scale).
    #[arg(long)]
    plan_scale: Option<f64>,
    /// Split the drawings over sheets of this format, at structures: A0, A1,
    /// A2, A3, A4 (landscape) or WIDTHxHEIGHT in mm.
    #[arg(long)]
    page: Option<String>,
    /// Logo for the sheets' title block: FILE (SVG, PNG, JPEG) or
    /// FILE:WIDTH_MM (repeatable).
    #[arg(long)]
    logo: Vec<String>,
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
enum LidarCommand {
    /// Tiles, point counts and bounds; with --classes, point counts per
    /// class (reads every point).
    Info {
        /// LAS / LAZ file or directory of tiles.
        path: PathBuf,
        /// Count points per class.
        #[arg(long)]
        classes: bool,
        /// Class scheme (.olsc) naming the classes; default ASPRS LAS 1.4.
        #[arg(long)]
        scheme: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum PointcloudCommand {
    /// Import LAS/LAZ tiles into a project file (created if missing).
    Import {
        /// LAS / LAZ file or directory of tiles.
        tiles: PathBuf,
        /// Project file (.ols).
        #[arg(long)]
        into: PathBuf,
        /// Name of the point cloud in the project.
        #[arg(long, default_value = "lidar")]
        name: String,
        /// Class scheme (.olsc) stored with the cloud.
        #[arg(long)]
        classes: Option<PathBuf>,
        /// Coordinate system (informative), e.g. EPSG:2056.
        #[arg(long, default_value = "")]
        crs: String,
        /// Chunk size, m.
        #[arg(long, default_value_t = ols_pointcloud::DEFAULT_CHUNK)]
        chunk: f64,
    },
    /// Point clouds of a project file and their points per class.
    Info {
        /// Project file (.ols).
        file: PathBuf,
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
        Some(Command::Pointcloud { command }) => pointcloud(command),
        Some(Command::Project { command }) => match project::run(command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
        Some(Command::Lidar {
            command:
                LidarCommand::Info {
                    path,
                    classes,
                    scheme,
                },
        }) => lidar_info(&path, classes, scheme.as_deref()),
    }
}

fn class_scheme(path: Option<&std::path::Path>) -> Result<ols_lidar::ClassScheme, String> {
    match path {
        Some(p) => ols_lidar::ClassScheme::load(p),
        None => Ok(ols_lidar::ClassScheme::asprs()),
    }
}

fn lidar_info(path: &std::path::Path, classes: bool, scheme: Option<&std::path::Path>) -> ExitCode {
    let run = || -> Result<(), String> {
        let scheme = class_scheme(scheme)?;
        let tiles = ols_lidar::tiles_in(path).map_err(|e| e.to_string())?;
        let (mut total, mut lo, mut hi) = (0u64, [f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for tile in &tiles {
            let info = ols_lidar::tile_info(tile).map_err(|e| e.to_string())?;
            total += info.points;
            for k in 0..3 {
                lo[k] = lo[k].min(info.bounds[0][k]);
                hi[k] = hi[k].max(info.bounds[1][k]);
            }
        }
        println!("{} tiles, {total} points", tiles.len());
        println!(
            "bounds x {:.2} – {:.2}, y {:.2} – {:.2}, z {:.2} – {:.2}",
            lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
        );
        if classes {
            let started = std::time::Instant::now();
            let filter = ols_lidar::GroundFilter {
                classes: vec![],
                ..Default::default()
            };
            let budget =
                ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(1800));
            let (_, stats) =
                ols_lidar::read_ground(&tiles, &filter, &budget).map_err(|e| e.to_string())?;
            println!(
                "read {} points in {:.1?}",
                stats.points_read,
                started.elapsed()
            );
            println!("classes ({}):", scheme.name);
            for (class, n) in stats.classes.iter().enumerate().filter(|(_, n)| **n > 0) {
                let (name, roles) = scheme
                    .class(class as u8)
                    .map_or(("(not in the scheme)".to_owned(), String::new()), |c| {
                        (c.name.clone(), c.roles.join(", "))
                    });
                println!("  {class:>3} {n:>12}  {name:<48} {roles}");
            }
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

fn pointcloud(command: PointcloudCommand) -> ExitCode {
    let run = || -> Result<(), String> {
        match command {
            PointcloudCommand::Import {
                tiles,
                into,
                name,
                classes,
                crs,
                chunk,
            } => {
                let scheme = match &classes {
                    Some(p) => {
                        let text = std::fs::read_to_string(p)
                            .map_err(|e| format!("{}: {e}", p.display()))?;
                        ols_lidar::ClassScheme::parse(&text)?;
                        Some(text)
                    }
                    None => None,
                };
                let tiles = ols_lidar::tiles_in(&tiles).map_err(|e| e.to_string())?;
                let mut store = ols_pointcloud::Store::open(&into).map_err(|e| e.to_string())?;
                let started = std::time::Instant::now();
                // Exit clause: ~1 min per 10⁸ points; 2 h is far beyond.
                let budget = ols_core::budget::Budget::new()
                    .time_limit(std::time::Duration::from_secs(7200));
                let total = tiles.len();
                let (cloud, stats) = store
                    .import_tiles(
                        &name,
                        &tiles,
                        &crs,
                        scheme.as_deref(),
                        chunk,
                        ols_pointcloud::DEFAULT_SCALE,
                        &budget,
                        |k, tile, n| {
                            eprintln!(
                                "  [{}/{total}] {} ({n} points, {:.0?})",
                                k + 1,
                                tile.file_name()
                                    .map_or(String::new(), |f| f.to_string_lossy().into_owned()),
                                started.elapsed()
                            );
                        },
                    )
                    .map_err(|e| e.to_string())?;
                let size = std::fs::metadata(&into).map_or(0, |m| m.len());
                println!(
                    "imported {} points from {} tiles into \"{}\" in {} ({:.1?}); cloud now {} points, file {:.0} MB",
                    stats.points_read,
                    stats.read,
                    cloud.name,
                    into.display(),
                    started.elapsed(),
                    cloud.points,
                    size as f64 / 1e6
                );
            }
            PointcloudCommand::Info { file } => {
                let store = ols_pointcloud::Store::open(&file).map_err(|e| e.to_string())?;
                for cloud in store.clouds().map_err(|e| e.to_string())? {
                    let scheme = match &cloud.class_scheme {
                        Some(text) => ols_lidar::ClassScheme::parse(text)?,
                        None => ols_lidar::ClassScheme::asprs(),
                    };
                    println!(
                        "\"{}\": {} points, {} m chunks, scale {} m, {} — classes: {}",
                        cloud.name,
                        cloud.points,
                        cloud.chunk_size,
                        cloud.scale,
                        cloud.crs,
                        scheme.name
                    );
                    for (class, n) in store.class_counts(&cloud).map_err(|e| e.to_string())? {
                        let (cname, roles) = scheme
                            .class(class)
                            .map_or(("(not in the scheme)".to_owned(), String::new()), |c| {
                                (c.name.clone(), c.roles.join(", "))
                            });
                        println!("  {class:>3} {n:>12}  {cname:<48} {roles}");
                    }
                }
            }
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
        lidar,
        pointcloud,
        lidar_classes,
        ground_role,
        corridor,
        max_edge,
        thin,
        clearance,
        rules,
        other_line,
        clearance_csv,
        drawings,
        hscale,
        vscale,
        side_offset,
        plan_width,
        plan_scale,
        page,
        logo,
    } = args;
    let (temperature, wind_pressure, ice_thickness) =
        (*temperature, *wind_pressure, *ice_thickness);
    use ols_core::units::{Length, Stress, Temperature};
    use ols_weather::WeatherCase;

    let run = || -> Result<(), String> {
        let mut line = project::load_line(file)?;
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
        let ground = clearance_run::run(
            &line,
            &library,
            &case,
            &shapes,
            &extras.jumpers.clone(),
            &clearance_run::Options {
                terrain: terrain.as_deref(),
                lidar: lidar.as_deref(),
                pointcloud: pointcloud.as_deref(),
                lidar_classes: lidar_classes.as_deref(),
                ground_role,
                corridor: *corridor,
                max_edge: *max_edge,
                thin: *thin,
                rules: rules.as_deref(),
                clearance: *clearance,
                clearance_csv: clearance_csv.as_deref(),
                other_lines: other_line,
                structures: structure_dir,
                insulators: insulator_dir,
            },
            &mut extras,
        )?;
        if let Some(dir) = drawings {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let logos = logo
                .iter()
                .map(|spec| {
                    let (file, width) = match spec.rsplit_once(':') {
                        Some((f, w)) if w.parse::<f64>().is_ok() => (f, w.parse().unwrap_or(30.0)),
                        _ => (spec.as_str(), 30.0),
                    };
                    ols_drawing::Logo::from_file(std::path::Path::new(file), width)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let page = match page {
                Some(name) => Some(ols_drawing::page_format(name).ok_or_else(|| {
                    format!("unknown page format \"{name}\" (A0–A4 or WIDTHxHEIGHT in mm)")
                })?),
                None => None,
            };
            let options = ols_drawing::Options {
                horizontal_scale: *hscale,
                vertical_scale: *vscale,
                side_offsets: side_offset.clone(),
                clearance: *clearance,
                plan_half_width: *plan_width,
                title: vec![
                    line.name.clone(),
                    format!("{temperature} °C, wind {wind_pressure} Pa, ice {ice_thickness} mm"),
                ],
                plan_offset_scale: *plan_scale,
                logos,
            };
            for route in line.effective_routes() {
                let d = ols_drawing::build(
                    &line,
                    &route,
                    &shapes,
                    &extras.jumpers,
                    ground.as_ref(),
                    &extras.markers,
                    &options,
                )?;
                let svg_path = dir.join(format!("{}.svg", route.id));
                let csv_path = dir.join(format!("{}-profile.csv", route.id));
                match &page {
                    None => std::fs::write(&svg_path, ols_drawing::svg(&d, &options))
                        .map_err(|e| format!("{}: {e}", svg_path.display()))?,
                    Some(page) => {
                        let sheets = ols_drawing::sheets(&d, &options, page);
                        for sheet in &sheets {
                            let path =
                                dir.join(format!("{}-sheet-{:02}.svg", route.id, sheet.number));
                            std::fs::write(&path, &sheet.svg)
                                .map_err(|e| format!("{}: {e}", path.display()))?;
                            for w in &sheet.warnings {
                                println!("  warning: sheet {}: {w}", sheet.number);
                            }
                        }
                        println!(
                            "  route {}: {} sheet(s) {} at 1:{} / 1:{}",
                            route.id,
                            sheets.len(),
                            page.name,
                            options.horizontal_scale,
                            options.vertical_scale
                        );
                    }
                }
                std::fs::write(&csv_path, ols_drawing::profile_csv(&d))
                    .map_err(|e| format!("{}: {e}", csv_path.display()))?;
                println!(
                    "drawing: route {} ({:.0} m, {} structures, {} wires) → {}",
                    route.id,
                    d.length,
                    d.structures.len(),
                    d.wires.len(),
                    match &page {
                        None => svg_path.display().to_string(),
                        Some(_) => dir
                            .join(format!("{}-sheet-NN.svg", route.id))
                            .display()
                            .to_string(),
                    }
                );
            }
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
