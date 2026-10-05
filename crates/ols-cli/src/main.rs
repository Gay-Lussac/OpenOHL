// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols` — OLS CAD command-line interface.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use ols_core::units::UnitSystem;

mod clearance_run;
mod io_run;
mod project;
mod sagtension_job;
mod surface;

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
    /// Coordinate systems: describe, transform points or a whole line.
    Crs {
        #[command(subcommand)]
        command: CrsCommand,
    },
    /// Point clouds stored in a project file.
    Pointcloud {
        #[command(subcommand)]
        command: PointcloudCommand,
    },
    /// File formats for import and export (built-in and from plugins).
    Formats {
        /// File-format plugin directory (repeatable).
        #[arg(long)]
        plugin: Vec<PathBuf>,
    },
    /// Read a file into a line file (.ols.json).
    Import {
        /// File to read.
        file: PathBuf,
        /// Format id (default: from the file extension; see ols formats).
        #[arg(long)]
        format: Option<String>,
        /// Line to update (e.g. structure positions into an existing line).
        #[arg(long)]
        into: Option<PathBuf>,
        /// Output line file.
        #[arg(long)]
        out: PathBuf,
        /// Format option KEY=VALUE (repeatable).
        #[arg(long = "option")]
        options: Vec<String>,
        /// File-format plugin directory (repeatable).
        #[arg(long)]
        plugin: Vec<PathBuf>,
    },
    /// Terrain surfaces (TINs) kept in a project file.
    Surface {
        #[command(subcommand)]
        command: surface::SurfaceCommand,
    },
    /// Inspect plugins.
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
    /// Host a project for several users (server mode, alpha): prints the
    /// invitation to give them; runs until stopped (Ctrl+C: every edit is
    /// already in the project file).
    Serve {
        /// Project file (.ols).
        project: PathBuf,
        /// Address and port to listen on (UDP: QUIC).
        #[arg(long, default_value = "0.0.0.0:4433")]
        bind: std::net::SocketAddr,
        /// Host name or address in the invitation (default: this
        /// computer's address on the local network).
        #[arg(long)]
        advertise: Option<String>,
        /// Access code (default: a random one).
        #[arg(long)]
        code: Option<String>,
        /// Role of users the project does not know: viewer, designer, or
        /// none (refused).
        #[arg(long, default_value = "viewer")]
        newcomers: String,
        /// Stop after this many seconds (default: run until stopped).
        #[arg(long)]
        seconds: Option<u64>,
        /// Print the personal invitation of this user of the project
        /// (repeatable): users the project knows join only with theirs.
        #[arg(long)]
        invite: Vec<String>,
    },
}

/// Arguments of `ols line`.
#[derive(clap::Args)]
struct LineArgs {
    /// Line file (.ols.json) or project file (.ols).
    file: PathBuf,
    /// Wire library: .olsw file or directory (default: the shipped
    /// libraries/cables, in the current folder or next to the program).
    #[arg(long)]
    library: Option<PathBuf>,
    /// Structure type directory (.olss pylon files), for typed structures
    /// (default: the `Structures` folder next to the line or project file,
    /// else libraries/structures).
    #[arg(long)]
    structures: Option<PathBuf>,
    /// Insulator assembly library directory (.olsi) (default: the shipped
    /// libraries/insulators).
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// Cable temperature, °C.
    #[arg(long, default_value_t = 15.0)]
    temperature: f64,
    /// Wind pressure on the wires, Pa (perpendicular to each span).
    #[arg(long, default_value_t = 0.0)]
    wind_pressure: f64,
    /// Side the wind comes from, looking ahead along the line: left or
    /// right.
    #[arg(long, default_value = "right")]
    wind_from: String,
    /// Radial ice thickness, mm.
    #[arg(long, default_value_t = 0.0)]
    ice_thickness: f64,
    /// Or: ice load, N/m (instead of a thickness).
    #[arg(long)]
    ice_load: Option<f64>,
    /// Ice density, kg/m³ (with a thickness: default 913; with a load: the
    /// iced diameter for the wind, none = bare diameter).
    #[arg(long)]
    ice_density: Option<f64>,
    /// Weather case file (.olswc): solve and check its cases (all, or those
    /// chosen with --case / --tag) instead of the single case of
    /// --temperature, --wind-pressure and the ice options.
    #[arg(long)]
    weather: Option<PathBuf>,
    /// Weather case to run, by name (`*` and `?` wildcards; repeatable).
    #[arg(long)]
    case: Vec<String>,
    /// Weather cases to run, by tag (repeatable).
    #[arg(long)]
    tag: Vec<String>,
    /// Write a standalone 3D viewer (HTML).
    #[arg(long)]
    html: Option<PathBuf>,
    /// Write all wire points as CSV.
    #[arg(long)]
    csv: Option<PathBuf>,
    /// Write one row per span (tensions, sag, mid and low points) as CSV.
    #[arg(long)]
    spans_csv: Option<PathBuf>,
    /// Load cases (.olslc): the loads on the structures in each (their
    /// weather cases from --weather).
    #[arg(long, requires = "weather")]
    load_cases: Option<PathBuf>,
    /// Write the structure loads (every attachment, every load case) as CSV.
    #[arg(long, requires = "load_cases")]
    loads_csv: Option<PathBuf>,
    /// Write how much of its capacity each structure uses (its type's
    /// allowable spans and loads; every load case) as CSV.
    #[arg(long, requires = "load_cases")]
    usage_csv: Option<PathBuf>,
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
    /// Export the solved line (every weather case): FILE (format from the
    /// extension) or FORMAT=FILE, e.g. kmz=line.kmz (repeatable; see ols
    /// formats).
    #[arg(long)]
    export: Vec<String>,
    /// Export format option KEY=VALUE (repeatable), e.g. crs=EPSG:2056.
    #[arg(long = "option")]
    export_options: Vec<String>,
    /// File-format plugin directory (repeatable).
    #[arg(long)]
    plugin: Vec<PathBuf>,
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
        /// Output directory (created): structures/, insulators/, typed line;
        /// with --inline-strings: the project folder, pylon files go to its
        /// Structures/ folder.
        #[arg(long)]
        out: PathBuf,
        /// Write simple pylon files: the strings in each structure type
        /// (length, weight, top and bottom labels), no insulator library.
        #[arg(long)]
        inline_strings: bool,
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
enum CrsCommand {
    /// What a coordinate system is (EPSG:<code> or a proj4 string).
    Info {
        /// Coordinate system.
        spec: String,
    },
    /// Transform points: X Y [Z] … (geographic: longitude latitude, degrees).
    Transform {
        /// From (e.g. EPSG:2056).
        #[arg(long)]
        from: String,
        /// To (e.g. EPSG:4326).
        #[arg(long)]
        to: String,
        /// Heights are ellipsoidal and go through the datum shift (default:
        /// kept as they are).
        #[arg(long)]
        ellipsoidal: bool,
        /// Coordinates: X Y (Z defaults to 0), repeated for more points
        /// with --points 3.
        #[arg(allow_hyphen_values = true, num_args = 2..)]
        values: Vec<f64>,
        /// Values per point: 2 (X Y) or 3 (X Y Z).
        #[arg(long, default_value_t = 2)]
        points: usize,
    },
    /// Move a whole line into another projected coordinate system.
    Line {
        /// Line file (.ols.json).
        file: PathBuf,
        /// Its coordinate system (default: the line's `crs`).
        #[arg(long)]
        from: Option<String>,
        /// Target coordinate system (projected, metres).
        #[arg(long)]
        to: String,
        /// Output line file.
        #[arg(long)]
        out: PathBuf,
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
    /// Write a point cloud of a project file to LAS / LAZ (by extension):
    /// LAS 1.4, point format 6, with its class scheme next to it.
    Export {
        /// Project file (.ols).
        file: PathBuf,
        /// LAS / LAZ file to write.
        #[arg(long)]
        out: PathBuf,
        /// Point cloud (default: the only one).
        #[arg(long)]
        name: Option<String>,
        /// Only these classes, comma separated (e.g. 2,5,14).
        #[arg(long, value_delimiter = ',')]
        classes: Vec<u8>,
        /// Only this rectangle: xmin,ymin,xmax,ymax.
        #[arg(long, value_delimiter = ',', num_args = 4)]
        area: Vec<f64>,
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
            command:
                StructuresCommand::Derive {
                    file,
                    out,
                    inline_strings,
                },
        }) => structures_derive(&file, &out, inline_strings),
        Some(Command::Plugin {
            command: PluginCommand::Info { dir },
        }) => plugin_info(&dir),
        Some(Command::Formats { plugin }) => report(io_run::formats(&plugin)),
        Some(Command::Surface { command }) => report(surface::run(command)),
        Some(Command::Serve {
            project,
            bind,
            advertise,
            code,
            newcomers,
            seconds,
            invite,
        }) => report(serve(
            &project, bind, advertise, code, &newcomers, seconds, &invite,
        )),
        Some(Command::Import {
            file,
            format,
            into,
            out,
            options,
            plugin,
        }) => report(io_run::options(&options).and_then(|o| {
            io_run::import(&file, format.as_deref(), into.as_deref(), &out, &o, &plugin)
        })),
        Some(Command::Pointcloud { command }) => pointcloud(command),
        Some(Command::Crs { command }) => match crs(command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
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

/// `ols serve`: hosts the project, prints who joins and leaves.
fn serve(
    project: &std::path::Path,
    bind: std::net::SocketAddr,
    advertise: Option<String>,
    code: Option<String>,
    newcomers: &str,
    seconds: Option<u64>,
    invite: &[String],
) -> Result<(), String> {
    let newcomers = match newcomers {
        "none" => None,
        r => Some(ols_session::Role::parse(r).map_err(|e| e.to_string())?),
    };
    let server = ols_server::server::Server::start(
        project,
        ols_server::server::Options {
            bind,
            advertise,
            code,
            newcomers,
        },
    )?;
    println!("hosting {} on {}", project.display(), server.local_addr);
    println!("invitation (newcomers): {}", server.invitation);
    for user in invite {
        println!("invitation for {user}: {}", server.invitation_for(user));
    }
    let started = std::time::Instant::now();
    let mut shown: Vec<String> = Vec::new();
    loop {
        if seconds.is_some_and(|s| started.elapsed().as_secs() >= s) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        let now: Vec<String> = server
            .peers()
            .iter()
            .map(|p| format!("{} ({})", p.user, p.role.as_str()))
            .collect();
        if now != shown {
            println!(
                "connected: {}",
                if now.is_empty() {
                    "nobody".into()
                } else {
                    now.join(", ")
                }
            );
            shown = now;
        }
    }
    server.stop();
    Ok(())
}

fn report(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
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
                // Tiles far from the line: said so, read anyway.
                if let Some(s) = ols_session::Session::open(&into)
                    .ok()
                    .filter(|s| s.has_line())
                {
                    for w in ols_lidar::far_tile_warnings(&tiles, s.line()) {
                        eprintln!("warning: {w}");
                    }
                }
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
            PointcloudCommand::Export {
                file,
                out,
                name,
                classes,
                area,
            } => {
                let started = std::time::Instant::now();
                let store = ols_pointcloud::Store::open(&file).map_err(|e| e.to_string())?;
                let cloud = store.cloud(name.as_deref()).map_err(|e| e.to_string())?;
                let area = (area.len() == 4).then(|| ([area[0], area[1]], [area[2], area[3]]));
                // Exit clause: ~1 µs per point for LAZ; 2 h is far beyond.
                let budget = ols_core::budget::Budget::new()
                    .time_limit(std::time::Duration::from_secs(7200));
                let stats = store
                    .export_las(
                        &cloud,
                        &out,
                        (!classes.is_empty()).then_some(classes.as_slice()),
                        area,
                        &budget,
                        |_, _| {},
                    )
                    .map_err(|e| e.to_string())?;
                if let Some(text) = &cloud.class_scheme {
                    let olsc = out.with_extension("olsc");
                    std::fs::write(&olsc, text).map_err(|e| format!("{}: {e}", olsc.display()))?;
                }
                println!(
                    "{} points of \"{}\" written to {} in {:.1?}",
                    stats.points_visited,
                    cloud.name,
                    out.display(),
                    started.elapsed()
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

fn crs(command: CrsCommand) -> Result<(), String> {
    use ols_crs::{Crs, Heights, Transformation};
    match command {
        CrsCommand::Info { spec } => {
            let c = Crs::parse(&spec)?;
            println!("{}", c.name());
            println!("  {}", c.definition());
            println!(
                "  {}",
                if c.is_geographic() {
                    "geographic: longitude, latitude in degrees"
                } else {
                    "projected"
                }
            );
        }
        CrsCommand::Transform {
            from,
            to,
            ellipsoidal,
            values,
            points,
        } => {
            if !(points == 2 || points == 3) || values.len() % points != 0 {
                return Err(format!("give the coordinates as groups of {points} values"));
            }
            let t = Transformation::new(
                &Crs::parse(&from)?,
                &Crs::parse(&to)?,
                if ellipsoidal {
                    Heights::Ellipsoidal
                } else {
                    Heights::Keep
                },
            );
            let mut pts: Vec<[f64; 3]> = values
                .chunks(points)
                .map(|c| [c[0], c[1], c.get(2).copied().unwrap_or(0.0)])
                .collect();
            t.points(&mut pts)?;
            for p in pts {
                if points == 3 {
                    println!("{:.9} {:.9} {:.4}", p[0], p[1], p[2]);
                } else {
                    println!("{:.9} {:.9}", p[0], p[1]);
                }
            }
        }
        CrsCommand::Line {
            file,
            from,
            to,
            out,
        } => {
            let mut line = ols_line::Line::load(&file).map_err(|e| e.to_string())?;
            let from = Crs::parse(from.as_deref().unwrap_or(&line.crs))
                .map_err(|e| format!("{e} (the line's crs is \"{}\"; give --from)", line.crs))?;
            let to = Crs::parse(&to)?;
            if from.is_geographic() || to.is_geographic() {
                return Err(
                    "lines are designed in a projected coordinate system in metres; choose projected systems"
                        .into(),
                );
            }
            let t = Transformation::new(&from, &to, Heights::Keep);
            line.transform_coordinates(|pts| t.points(pts))?;
            line.crs = to.name().to_owned();
            line.validate().map_err(|e| e.to_string())?;
            std::fs::write(&out, line.to_json()).map_err(|e| format!("{}: {e}", out.display()))?;
            println!(
                "{} → {}: {} structures moved; wrote {} (grid distances change with the projection's scale: check spans and stringing)",
                from.name(),
                to.name(),
                line.structures.len(),
                out.display()
            );
        }
    }
    Ok(())
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
        structures: structure_dir_arg,
        insulators: insulator_dir,
        temperature,
        wind_pressure,
        wind_from,
        ice_thickness,
        ice_load,
        ice_density,
        weather,
        case,
        tag,
        html,
        csv,
        spans_csv,
        load_cases,
        loads_csv,
        usage_csv,
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
        export,
        export_options,
        plugin,
    } = args;
    use ols_core::files::shipped_library;
    use ols_core::units::{Density, ForcePerLength, Length, Stress, Temperature};
    let library = &library.clone().unwrap_or_else(|| shipped_library("cables"));
    let insulator_dir = &insulator_dir
        .clone()
        .unwrap_or_else(|| shipped_library("insulators"));
    use ols_weather::{WeatherCase, WeatherCases};
    // Pylon files: given, else the project's Structures/ folder, else the
    // shared library.
    let structure_dir: &std::path::Path = &match structure_dir_arg {
        Some(d) => d.clone(),
        None => {
            let local = file
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("Structures");
            if local.is_dir() {
                local
            } else {
                shipped_library("structures")
            }
        }
    };

    let run = || -> Result<(), String> {
        let mut line = project::load_line(file)?;
        let mut types = ols_structures::StructureLibrary::default();
        if line.structures.iter().any(|s| s.structure_type.is_some()) {
            let structures = ols_structures::StructureLibrary::load_dir(structure_dir)
                .map_err(|e| e.to_string())?;
            let insulators = ols_structures::InsulatorLibrary::load_dir(insulator_dir)
                .map_err(|e| e.to_string())?;
            line = line
                .resolve(&structures, &insulators)
                .map_err(|e| e.to_string())?;
            types = structures;
        }
        let library = ols_cables::Library::load(library).map_err(|e| e.to_string())?;
        // Formats first: a wrong format or plugin fails before solving.
        let export_options = io_run::options(export_options)?;
        let mut formats = if export.is_empty() {
            None
        } else {
            Some(io_run::registry(plugin)?)
        };
        let cases: Vec<WeatherCase> = match weather {
            Some(path) => {
                let set = WeatherCases::load(path)?;
                // The creep-stretch cases from the whole file.
                line.set_weather_cases(&set.cases);
                let chosen = set.select(case, tag)?;
                println!(
                    "weather: {} — {} of {} cases",
                    if set.name.is_empty() {
                        path.display().to_string()
                    } else {
                        set.name.clone()
                    },
                    chosen.len(),
                    set.cases.len()
                );
                chosen
            }
            None => {
                if !case.is_empty() || !tag.is_empty() {
                    return Err("--case and --tag choose cases of a --weather file".into());
                }
                let c = WeatherCase {
                    wind_pressure: Stress::from_pascals(*wind_pressure),
                    wind_from: match wind_from.as_str() {
                        "right" => ols_weather::WindSide::Right,
                        "left" => ols_weather::WindSide::Left,
                        other => {
                            return Err(format!("--wind-from {other}: write left or right"));
                        }
                    },
                    ice_thickness: Length::from_millimeters(*ice_thickness),
                    ice_load: ice_load.map(ForcePerLength::from_newtons_per_meter),
                    ice_density: ice_density.map(Density::from_kilograms_per_cubic_meter),
                    ..WeatherCase::still("command line", Temperature::from_celsius(*temperature))
                };
                c.validate()?;
                vec![c]
            }
        };
        if cases.is_empty() {
            return Err("no weather case to run".into());
        }
        if let (Some(lc), Some(w)) = (load_cases, weather) {
            let set = ols_loads::LoadCases::load(lc)?;
            let all = WeatherCases::load(w)?.cases;
            let started = std::time::Instant::now();
            let results = ols_loads::structure_loads(
                &line,
                &library,
                &all,
                &set.cases,
                &ols_wirefe::default_fe_budget(),
            )?;
            println!(
                "structure loads: {} load cases in {:.1?}",
                results.len(),
                started.elapsed()
            );
            for r in &results {
                let max = |k: usize| {
                    r.loads
                        .iter()
                        .map(|l| l.factored[k].abs())
                        .fold(0.0, f64::max)
                        / 1000.0
                };
                println!(
                    "  {:<24} {:>4} supports  largest factored kN: transverse {:>8.2}  \
                     longitudinal {:>8.2}  vertical {:>8.2}{}",
                    r.case.name,
                    r.loads.len(),
                    max(0),
                    max(1),
                    max(2),
                    if r.failed.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "  ({} sections failed: {})",
                            r.failed.len(),
                            r.failed.join("; ")
                        )
                    }
                );
            }
            if let Some(out) = loads_csv {
                std::fs::write(out, ols_loads::to_csv(&results))
                    .map_err(|e| format!("{}: {e}", out.display()))?;
                println!("  written to {}", out.display());
            }
            // Structure checks: the types' allowable spans and loads.
            let usage = ols_loads::capacity::usage(&line, &types, &library, &all, &results);
            let governing = ols_loads::capacity::governing(&usage);
            if !governing.is_empty() {
                let over = governing
                    .iter()
                    .filter(|(_, s)| s.governing().is_some_and(|g| g.usage > 1.0))
                    .count();
                println!(
                    "structure usage: {} structures checked, {over} over 100 %",
                    governing.len()
                );
                for (case, s) in governing.iter().take(10) {
                    let g = s.governing().expect("checked");
                    println!(
                        "  {:<12} {:>12}  {:<20} {}",
                        s.structure,
                        if g.usage.is_finite() {
                            format!("{:.1} %", g.usage * 100.0)
                        } else {
                            "not allowed".into()
                        },
                        case,
                        g.governs
                    );
                }
            }
            if let Some(out) = usage_csv {
                std::fs::write(out, ols_loads::capacity::usage_csv(&usage))
                    .map_err(|e| format!("{}: {e}", out.display()))?;
                println!("  written to {}", out.display());
            }
        }
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
        let prepared = clearance_run::prepare(
            &line,
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
                other_lines: other_line,
                structures: structure_dir,
                insulators: insulator_dir,
            },
        )?;
        let mut clearance_rows = String::from(clearance_run::CSV_HEADER);
        // Outputs (viewer, CSV, drawings) show the first case.
        let mut first: Option<(WeatherCase, Vec<ols_line::solve::SectionShape>)> = None;
        // Every case's solution, for the drawings (one trace per case).
        let mut solved_cases: Vec<(
            WeatherCase,
            Vec<ols_line::solve::SectionShape>,
            Vec<ols_line::jumper::JumperShape>,
        )> = Vec::new();
        let mut case_wires = Vec::new();
        let mut envelope = ols_clearance::Envelope::default();
        let mut extras = ols_line::export::ViewerExtras::default();
        let mut failed = 0;
        for case in &cases {
            let started = std::time::Instant::now();
            let results = ols_line::solve::solve_line(&line, &library, case);
            let elapsed = started.elapsed();
            println!(
                "{}: {} structures, {} sections — case \"{}\": {}",
                line.name,
                line.structures.len(),
                line.sections.len(),
                case.name,
                case.summary()
            );
            println!(
                "  {:<12} {:<22} {:>5} {:>12} {:>10} {:>6}",
                "section", "circuit / phase", "spans", "max T (kN)", "max sag", "iter"
            );
            let mut shapes = Vec::new();
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
            let mut jumpers = Vec::new();
            if !line.jumpers.is_empty() {
                println!(
                    "  {:<16} {:>10} {:>9} {:>11}",
                    "jumper", "length m", "sag m", "max T (N)"
                );
                for (jumper, result) in line.jumpers.iter().zip(ols_line::jumper::solve_jumpers(
                    &line, &library, case, &shapes,
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
                            jumpers.push(shape);
                        }
                        Err(error) => {
                            failed += 1;
                            println!("  {:<16} error: {error}", jumper.id);
                        }
                    }
                }
            }
            case_wires.push(clearance_run::check(
                &line,
                &library,
                case,
                &shapes,
                &jumpers,
                &prepared,
                &mut clearance_rows,
                &mut extras,
                &mut envelope,
            )?);
            solved_cases.push((case.clone(), shapes.clone(), jumpers.clone()));
            if first.is_none() {
                extras.jumpers = jumpers;
                first = Some((case.clone(), shapes));
            }
        }
        clearance_run::cross_cases(
            &prepared,
            &case_wires,
            &mut clearance_rows,
            &mut extras,
            &mut envelope,
        );
        clearance_run::envelope_report(&prepared, &envelope, &mut clearance_rows);
        let (first_case, shapes) = first.expect("at least one case");
        if cases.len() > 1 && (html.is_some() || csv.is_some() || spans_csv.is_some()) {
            println!(
                "viewer and wire CSVs show the first case, \"{}\" (violation markers of all cases); drawings show every case",
                first_case.name
            );
        }
        if let Some(path) = clearance_csv {
            std::fs::write(path, &clearance_rows)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if let Some(g) = prepared
            .ground
            .as_ref()
            .filter(|g| g.num_triangles() <= ols_line::export::MAX_VIEWER_TRIANGLES)
        {
            extras.ground = Some(g.mesh());
        }
        let ground = prepared.ground.as_ref();
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
                title: {
                    let mut t = vec![line.name.clone()];
                    let names: Vec<String> = solved_cases
                        .iter()
                        .enumerate()
                        .map(|(k, (c, _, _))| {
                            format!(
                                "{}{}",
                                c.name,
                                if k == 0 {
                                    " (solid)".to_owned()
                                } else {
                                    format!(" ({})", ols_drawing::case_dash_name(k))
                                }
                            )
                        })
                        .collect();
                    t.push(if names.len() > 1 {
                        format!("Weather cases: {}", names.join(", "))
                    } else {
                        format!("{}: {}", first_case.name, first_case.summary())
                    });
                    t
                },
                plan_offset_scale: *plan_scale,
                logos,
            };
            for route in line.effective_routes() {
                let mut d = ols_drawing::build(
                    &line,
                    &route,
                    &shapes,
                    &extras.jumpers,
                    ground,
                    &extras.markers,
                    &options,
                )?;
                // The other cases' wires, dashed (the envelope of positions).
                for (k, (case, case_shapes, case_jumpers)) in
                    solved_cases.iter().enumerate().skip(1)
                {
                    let other = ols_drawing::build(
                        &line,
                        &route,
                        case_shapes,
                        case_jumpers,
                        None,
                        &[],
                        &options,
                    )?;
                    for mut w in other.wires {
                        w.label = format!("{} — {}", w.label, case.name);
                        w.dash = Some(ols_drawing::case_dash(k));
                        d.wires.push(w);
                    }
                }
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
        if let Some(registry) = &mut formats {
            let solved: Vec<_> = solved_cases
                .iter()
                .map(|(c, s, j)| (c.name.clone(), c.summary(), s.clone(), j.clone()))
                .collect();
            let model = ols_io::ExportModel::new(&line, &solved);
            io_run::export(&model, export, &export_options, registry)?;
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

fn structures_derive(file: &std::path::Path, out: &std::path::Path, inline: bool) -> ExitCode {
    use ols_structures::{
        InsulatorAssembly, InsulatorLibrary, LibraryItem, StructureLibrary, StructureType,
    };

    let run = || -> Result<(), String> {
        let line = ols_line::Line::load(file).map_err(|e| e.to_string())?;
        let (derived, typed) = line.derive_libraries(inline);
        // Simple pylon files: one per type in the project's Structures/.
        let (sdir, idir) = if inline {
            (out.join("Structures"), out.join("insulators"))
        } else {
            (out.join("structures"), out.join("insulators"))
        };
        std::fs::create_dir_all(&sdir).map_err(|e| format!("{}: {e}", sdir.display()))?;
        if !inline {
            std::fs::create_dir_all(&idir).map_err(|e| format!("{}: {e}", idir.display()))?;
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
        if inline {
            println!(
                "Note: one simple pylon file per type in {}; string weights come from the line when given; labels A<set>-<phase> (top) and W<set>-<phase> (bottom) can be renamed.",
                sdir.display()
            );
        } else {
            println!(
                "Note: insulator weights come from the line when given; check them, and fill in stiffness and wind areas."
            );
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
