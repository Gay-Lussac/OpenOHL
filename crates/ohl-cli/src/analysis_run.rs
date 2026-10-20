// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl clearance galloping|row` and `ohl lidar classify|vegetation|
//! fit-wires`: analyses of a solved line against its galloping ellipses,
//! its right of way and LiDAR points ([`ohl_clearance::galloping`],
//! [`ohl_clearance::blowout`], [`ohl_clearance::vegetation`],
//! [`ohl_clearance::wirefit`], [`ohl_lidar::classify`]).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use ohl_clearance::galloping::{self, GallopingParams};
use ohl_clearance::positions::PositionKind;
use ohl_clearance::vegetation::{VegetationCheck, VegetationParams};
use ohl_clearance::wirefit::{FitParams, WireCollector};
use ohl_clearance::{Wire, wires_of};
use ohl_core::budget::Budget;
use ohl_core::text::csv_field;
use ohl_line::Line;
use ohl_line::solve::SectionShape;
use ohl_weather::{WeatherCase, WeatherCases};

/// The line and where its libraries are.
#[derive(Args)]
pub struct ModelArgs {
    /// Line file (.ohl.json) or project file (.ohl).
    pub file: PathBuf,
    /// Wire library: .ohlw file or directory (default: the shipped
    /// libraries/cables).
    #[arg(long)]
    pub library: Option<PathBuf>,
    /// Structure type directory (.ohls) for typed structures (default: the
    /// `Structures` folder next to the line).
    #[arg(long)]
    pub structures: Option<PathBuf>,
    /// Insulator assembly library directory (.ohli).
    #[arg(long)]
    pub insulators: Option<PathBuf>,
}

/// Weather cases from a file.
#[derive(Args)]
pub struct CaseArgs {
    /// Weather case file (.ohlwc).
    #[arg(long)]
    pub weather: PathBuf,
    /// Weather case to run, by name (`*` and `?` wildcards; repeatable).
    #[arg(long)]
    pub case: Vec<String>,
    /// Weather cases to run, by tag (repeatable).
    #[arg(long)]
    pub tag: Vec<String>,
}

/// Points: a project's point cloud or LAS/LAZ tiles.
#[derive(Args)]
pub struct PointArgs {
    /// Point cloud in a project file (`FILE.ohl` or `FILE.ohl#name`).
    #[arg(long, conflicts_with = "lidar")]
    pub pointcloud: Option<String>,
    /// LAS/LAZ file or directory of tiles.
    #[arg(long)]
    pub lidar: Option<PathBuf>,
    /// Class scheme (.ohlc); default: the cloud's own, else ASPRS LAS 1.4.
    #[arg(long)]
    pub lidar_classes: Option<PathBuf>,
}

/// `ohl clearance …`.
#[derive(Subcommand)]
pub enum ClearanceCommand {
    /// Galloping ellipses of the wires of each span (parameters: .ohlg
    /// file) and their clearances to each other.
    Galloping {
        #[command(flatten)]
        model: ModelArgs,
        #[command(flatten)]
        cases: CaseArgs,
        /// Galloping parameters (.ohlg).
        #[arg(long)]
        params: PathBuf,
        /// Write every pair and method as CSV.
        #[arg(long)]
        csv: Option<PathBuf>,
    },
    /// Blowout: how far the wires swing sideways in each (windy) case,
    /// against the right of way either side of the route centrelines.
    Row {
        #[command(flatten)]
        model: ModelArgs,
        #[command(flatten)]
        cases: CaseArgs,
        /// Half width of the right of way, m (both sides).
        #[arg(long)]
        half_width: f64,
        /// Width on the left of the centreline instead, m.
        #[arg(long)]
        left: Option<f64>,
        /// Width on the right of the centreline instead, m.
        #[arg(long)]
        right: Option<f64>,
        /// Write one row per wire as CSV.
        #[arg(long)]
        csv: Option<PathBuf>,
    },
}

/// `ohl lidar classify`.
#[derive(Args)]
pub struct ClassifyArgs {
    /// Points: LAS/LAZ file or directory, or `FILE.ohl#cloud`.
    pub input: String,
    /// Output: a .las/.laz file, or `FILE.ohl#cloud` (a new cloud).
    #[arg(long)]
    pub out: String,
    /// Only points within --corridor of this line's routes.
    #[arg(long)]
    pub line: Option<PathBuf>,
    /// Half width of the corridor, m.
    #[arg(long, default_value_t = 100.0)]
    pub corridor: f64,
    /// Grid cell of the ground filter, m.
    #[arg(long, default_value_t = 1.0)]
    pub cell: f64,
    /// Largest window of the ground filter, m (objects up to this size
    /// are removed).
    #[arg(long, default_value_t = 20.0)]
    pub max_window: f64,
    /// Terrain slope tolerated (rise over run).
    #[arg(long, default_value_t = 0.3)]
    pub slope: f64,
    /// Points within this height of the ground are ground, m.
    #[arg(long, default_value_t = 0.3)]
    pub ground_tolerance: f64,
    /// Classify points of these classes (repeatable; default 0 and 1).
    #[arg(long)]
    pub reclassify: Vec<u8>,
    /// Replace an existing cloud of the output's name.
    #[arg(long)]
    pub replace: bool,
    /// Most points read (held in memory, 32 bytes each).
    #[arg(long, default_value_t = 200_000_000)]
    pub max_points: usize,
}

/// `ohl lidar vegetation`.
#[derive(Args)]
pub struct VegetationArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    #[command(flatten)]
    pub cases: CaseArgs,
    #[command(flatten)]
    pub points: PointArgs,
    /// Role of the vegetation classes (repeatable; default vegetation).
    #[arg(long)]
    pub role: Vec<String>,
    /// Role of the ground classes (heights above the ground).
    #[arg(long, default_value = "ground")]
    pub ground_role: String,
    /// Required clearance from the wires, m.
    #[arg(long, default_value_t = 5.0)]
    pub clearance: f64,
    /// Growth per year, m.
    #[arg(long, default_value_t = 0.0)]
    pub growth: f64,
    /// Years until the next cut.
    #[arg(long, default_value_t = 0.0)]
    pub years: f64,
    /// Also look for trees that could fall onto the wires.
    #[arg(long)]
    pub fall_in: bool,
    /// Tallest tree looked for, m.
    #[arg(long, default_value_t = 40.0)]
    pub max_tree_height: f64,
    /// Largest horizontal distance from a tree to the nearest ground
    /// point for its base to be taken from the ground, m (default: no
    /// limit); beyond it no fall-in check.
    #[arg(long)]
    pub max_base_ground_distance: Option<f64>,
    /// Buffer added to the clearance for grow-in (e.g. the survey's
    /// accuracy), m.
    #[arg(long, default_value_t = 0.0)]
    pub buffer: f64,
    /// Root-ball radius as a fraction of the grown tree's height: the
    /// tree turns about a hinge this far from its trunk (0: about its
    /// base; the desktop application proposes 0.1).
    #[arg(long, default_value_t = 0.0)]
    pub root_ball: f64,
    /// Clearance allowance of a falling tree (flashover clearance,
    /// buffers), m.
    #[arg(long, default_value_t = 0.0)]
    pub fall_allowance: f64,
    /// Also check each span anywhere along its swing arc between its left
    /// and right blowout (windy cases).
    #[arg(long)]
    pub swing_arc: bool,
    /// Unbalanced ice on each span with vegetation within reach, solved
    /// with its section on its own: `case` (the case's ice on that span
    /// only), `extra=N/m` (an extra load on the span, the whole bundle's)
    /// or `load=N` (a concentrated load at mid-span).
    #[arg(long, value_parser = unbalanced_ice)]
    pub unbalanced_ice: Option<ohl_clearance::positions::UnbalancedIce>,
    /// Half width of the corridor around the spans' chords, m (the points
    /// read, and the isoclearance lines).
    #[arg(long, default_value_t = 60.0)]
    pub corridor: f64,
    /// Write per span the counts and the nearest, worst falling and worst
    /// points, with the case and position of each, as CSV.
    #[arg(long)]
    pub csv: Option<PathBuf>,
    /// Write isoclearance lines over the corridor: .geojson, .dxf or .csv
    /// by the extension (repeatable).
    #[arg(long)]
    pub isoclearance: Vec<PathBuf>,
    /// Interval of the isoclearance lines, m.
    #[arg(long, default_value_t = 5.0)]
    pub iso_interval: f64,
    /// Isoclearance lines above this clearance are not drawn, m.
    #[arg(long, default_value_t = 40.0)]
    pub iso_max: f64,
    /// Grid spacing of the isoclearance lines, m.
    #[arg(long, default_value_t = 1.0)]
    pub iso_spacing: f64,
    /// Spacing of the coarse grid first computed, m.
    #[arg(long, default_value_t = 8.0)]
    pub iso_coarse: f64,
    /// Exit clause: stop after this many seconds (an error, never a
    /// partial result).
    #[arg(long, default_value_t = 3600)]
    pub time_limit: u64,
}

/// `case`, `extra=N/m` or `load=N`.
fn unbalanced_ice(text: &str) -> Result<ohl_clearance::positions::UnbalancedIce, String> {
    use ohl_clearance::positions::UnbalancedIce;
    let number = |v: &str| {
        v.trim()
            .parse::<f64>()
            .ok()
            .filter(|x| x.is_finite() && *x > 0.0)
            .ok_or_else(|| format!("{text}: a positive number after ="))
    };
    match text.trim().split_once('=') {
        None if text.trim() == "case" => Ok(UnbalancedIce::CaseIce),
        Some(("extra", v)) => Ok(UnbalancedIce::Extra(number(v)?)),
        Some(("load", v)) => Ok(UnbalancedIce::Concentrated(number(v)?)),
        _ => Err(format!("{text}: write case, extra=N/m or load=N")),
    }
}

/// `ohl lidar fit-wires`.
#[derive(Args)]
pub struct FitArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    #[command(flatten)]
    pub points: PointArgs,
    /// Role of the wire classes (repeatable; default conductor and
    /// shield_wire).
    #[arg(long)]
    pub role: Vec<String>,
    /// Weather case file, for the creep and load conditions of the
    /// sections (`Criteria › Creep-Stretch`).
    #[arg(long)]
    pub weather: Option<PathBuf>,
    /// Temperature of the model compared with, °C (still air).
    #[arg(long, default_value_t = 15.0)]
    pub temperature: f64,
    /// Largest plan distance of a point from the span's plane, m.
    #[arg(long, default_value_t = 1.0)]
    pub lateral: f64,
    /// Largest height difference of a point from the modelled wire, m.
    #[arg(long, default_value_t = 3.0)]
    pub vertical: f64,
    /// Lowest temperature searched, °C.
    #[arg(long, default_value_t = -30.0, allow_hyphen_values = true)]
    pub min_temperature: f64,
    /// Highest temperature searched, °C.
    #[arg(long, default_value_t = 120.0)]
    pub max_temperature: f64,
    /// Do not estimate the temperature at the survey.
    #[arg(long)]
    pub no_temperature: bool,
    /// Write one row per span as CSV.
    #[arg(long)]
    pub csv: Option<PathBuf>,
}

/// The line with its structure types resolved, and the wire library.
fn load(m: &ModelArgs) -> Result<(Line, ohl_cables::Library), String> {
    use ohl_core::files::shipped_library;
    let mut line = crate::project::load_line(&m.file)?;
    if line.structures.iter().any(|s| s.structure_type.is_some()) {
        let dir = match &m.structures {
            Some(d) => d.clone(),
            None => m.file.parent().unwrap_or(Path::new(".")).join("Structures"),
        };
        let insulators = m
            .insulators
            .clone()
            .unwrap_or_else(|| shipped_library("insulators"));
        let structures =
            ohl_structures::StructureLibrary::load_dir(&dir).map_err(|e| e.to_string())?;
        let insulators =
            ohl_structures::InsulatorLibrary::load_dir(&insulators).map_err(|e| e.to_string())?;
        line = line
            .resolve(&structures, &insulators)
            .map_err(|e| e.to_string())?;
    }
    let library = m
        .library
        .clone()
        .unwrap_or_else(|| shipped_library("cables"));
    let library = ohl_cables::Library::load(&library).map_err(|e| e.to_string())?;
    Ok((line, library))
}

/// The chosen weather cases (the line gets the file's condition cases).
fn cases(c: &CaseArgs, line: &mut Line) -> Result<Vec<WeatherCase>, String> {
    let set = WeatherCases::load(&c.weather)?;
    line.set_weather_cases(&set.cases);
    let chosen = set.select(&c.case, &c.tag)?;
    if chosen.is_empty() {
        return Err("no weather case to run".into());
    }
    Ok(chosen)
}

/// Solves every section; failures are printed and left out.
fn solve(line: &Line, library: &ohl_cables::Library, case: &WeatherCase) -> Vec<SectionShape> {
    let mut shapes = Vec::new();
    for (section, r) in line
        .sections
        .iter()
        .zip(ohl_line::solve::solve_line(line, library, case))
    {
        match r {
            Ok(s) => shapes.push(s),
            Err(e) => println!("  section {} not solved: {e}", section.id),
        }
    }
    shapes
}

fn wire_name(w: &Wire) -> String {
    format!("{} span {}", w.section, w.span)
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("written to {}", path.display());
    Ok(())
}

/// Runs `ohl clearance …`.
pub fn clearance(command: ClearanceCommand) -> Result<(), String> {
    match command {
        ClearanceCommand::Galloping {
            model,
            cases: case_args,
            params,
            csv,
        } => {
            let (mut line, library) = load(&model)?;
            let chosen = cases(&case_args, &mut line)?;
            let params = GallopingParams::load(&params)?;
            println!(
                "galloping: {} — {} methods",
                if params.name.is_empty() {
                    "parameters"
                } else {
                    &params.name
                },
                params.methods.len()
            );
            let mut rows = String::from(
                "case,method,section,span,other_section,other_span,distance_m,required_m,overlap,at_fraction,major_m,minor_m,other_major_m,other_minor_m,x,y,z\n",
            );
            for case in chosen.iter().filter(|c| params.applies(&c.name)) {
                let started = Instant::now();
                let shapes = solve(&line, &library, case);
                let wires = galloping::galloping_wires(&line, &shapes, &library);
                // Exit clause: well under a second for any line; 5 min is
                // far beyond.
                let budget = Budget::new().time_limit(Duration::from_secs(300));
                let mut pairs = galloping::check(&params, &wires, &budget)?;
                pairs.sort_by(|a, b| {
                    let short =
                        |p: &galloping::GallopingPair| p.distance - p.required.unwrap_or(0.0);
                    short(a).total_cmp(&short(b))
                });
                let bad = pairs.iter().filter(|p| p.violates()).count();
                let overlaps = pairs.iter().filter(|p| p.overlap).count();
                println!(
                    "case \"{}\": {} wire pairs × methods checked, {overlaps} overlapping, {bad} below the requirement ({:.1?})",
                    case.name,
                    pairs.len(),
                    started.elapsed()
                );
                for p in pairs.iter().take(20) {
                    let (a, b) = (&wires[p.a].wire, &wires[p.b].wire);
                    println!(
                        "  {:<22} {:<16} {:<16} {:>8.2} m{} at {:.0} % (ellipses {:.2} × {:.2}, {:.2} × {:.2} m)",
                        params.methods[p.method].name,
                        wire_name(a),
                        wire_name(b),
                        p.distance,
                        p.required
                            .map_or(String::new(), |r| format!(" (required {r:.2} m)")),
                        p.at * 100.0,
                        p.axes[0][0],
                        p.axes[0][1],
                        p.axes[1][0],
                        p.axes[1][1],
                    );
                }
                for p in &pairs {
                    let (a, b) = (&wires[p.a].wire, &wires[p.b].wire);
                    let _ = writeln!(
                        rows,
                        "{},{},{},{},{},{},{:.3},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
                        csv_field(&case.name),
                        csv_field(&params.methods[p.method].name),
                        csv_field(&a.section),
                        a.span,
                        csv_field(&b.section),
                        b.span,
                        p.distance,
                        p.required.map_or(String::new(), |r| format!("{r:.3}")),
                        p.overlap,
                        p.at,
                        p.axes[0][0],
                        p.axes[0][1],
                        p.axes[1][0],
                        p.axes[1][1],
                        p.rest[0][0],
                        p.rest[0][1],
                        p.rest[0][2],
                    );
                }
            }
            if let Some(path) = csv {
                write(&path, &rows)?;
            }
            Ok(())
        }
        ClearanceCommand::Row {
            model,
            cases: case_args,
            half_width,
            left,
            right,
            csv,
        } => {
            let (mut line, library) = load(&model)?;
            let chosen = cases(&case_args, &mut line)?;
            let (left, right) = (left.unwrap_or(half_width), right.unwrap_or(half_width));
            let centres = ohl_clearance::blowout::centrelines(&line);
            let mut rows = String::from("case,section,span,right_m,left_m,encroachment_m,x,y,z\n");
            for case in &chosen {
                let shapes = solve(&line, &library, case);
                let wires = wires_of(&line, &shapes, &[]);
                let mut results = ohl_clearance::blowout::row_check(&wires, &centres, left, right);
                results.sort_by(|a, b| b.encroachment.total_cmp(&a.encroachment));
                let beyond = results.iter().filter(|r| r.encroachment > 0.0).count();
                println!(
                    "case \"{}\": {} wires, {beyond} beyond the right of way (left {left} m, right {right} m)",
                    case.name,
                    results.len()
                );
                for r in results.iter().take(10) {
                    println!(
                        "  {:<18} right {:>7.2} m  left {:>7.2} m  {}",
                        wire_name(&wires[r.wire]),
                        r.right.0,
                        r.left.0,
                        if r.encroachment > 0.0 {
                            format!("{:.2} m beyond", r.encroachment)
                        } else {
                            format!("{:.2} m inside", -r.encroachment)
                        }
                    );
                }
                for r in &results {
                    let w = &wires[r.wire];
                    let at = if r.right.0 - right >= r.left.0 - left {
                        r.right.1
                    } else {
                        r.left.1
                    };
                    let _ = writeln!(
                        rows,
                        "{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
                        csv_field(&case.name),
                        csv_field(&w.section),
                        w.span,
                        r.right.0,
                        r.left.0,
                        r.encroachment,
                        at[0],
                        at[1],
                        at[2]
                    );
                }
            }
            if let Some(path) = csv {
                write(&path, &rows)?;
            }
            Ok(())
        }
    }
}

/// A point source and its class scheme.
fn source(
    p: &PointArgs,
) -> Result<(Box<dyn ohl_pointcloud::PointSource>, ohl_lidar::ClassScheme), String> {
    let source: Box<dyn ohl_pointcloud::PointSource> = match (&p.pointcloud, &p.lidar) {
        (Some(spec), _) => {
            let (file, name) = ohl_lidar::cloud_spec(spec);
            Box::new(ohl_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?)
        }
        (None, Some(dir)) => Box::new(ohl_pointcloud::TileSource {
            tiles: ohl_lidar::tiles_in(dir).map_err(|e| e.to_string())?,
            scheme: None,
        }),
        (None, None) => return Err("give --pointcloud or --lidar".into()),
    };
    let scheme = match (&p.lidar_classes, source.class_scheme()) {
        (Some(path), _) => ohl_lidar::ClassScheme::load(path)?,
        (None, Some(text)) => ohl_lidar::ClassScheme::parse(&text)?,
        (None, None) => ohl_lidar::ClassScheme::asprs(),
    };
    Ok((source, scheme))
}

/// Class numbers of `roles` (default `fallback`), an error when none.
fn classes_of(
    scheme: &ohl_lidar::ClassScheme,
    roles: &[String],
    fallback: &[&str],
) -> Result<Vec<u8>, String> {
    let roles: Vec<String> = if roles.is_empty() {
        fallback.iter().map(|r| r.to_string()).collect()
    } else {
        roles.to_vec()
    };
    let mut codes: Vec<u8> = roles
        .iter()
        .flat_map(|r| scheme.codes_with_role(r))
        .collect();
    codes.sort_unstable();
    codes.dedup();
    if codes.is_empty() {
        return Err(format!(
            "no class of \"{}\" has the role {}",
            scheme.name,
            roles.join(" or ")
        ));
    }
    Ok(codes)
}

/// Exit clause of point reading: ~1 min per 10⁸ points from LAZ.
fn reading_budget() -> Budget {
    Budget::new().time_limit(Duration::from_secs(1800))
}

/// Runs `ohl lidar classify`.
pub fn classify(a: ClassifyArgs) -> Result<(), String> {
    use ohl_lidar::LidarPoint;
    let started = Instant::now();
    let corridor = match &a.line {
        Some(path) => {
            let line = crate::project::load_line(path)?;
            let routes: Vec<Vec<[f64; 2]>> = ohl_clearance::blowout::centrelines(&line)
                .iter()
                .map(|c| c.points.iter().map(|p| [p[0], p[1]]).collect())
                .collect();
            Some(ohl_lidar::Corridor::new(&routes, a.corridor))
        }
        None => None,
    };
    let mut points: Vec<LidarPoint> = Vec::new();
    let mut too_many = false;
    let mut push = |p: LidarPoint| {
        if points.len() < a.max_points {
            points.push(p);
        } else {
            too_many = true;
        }
    };
    let is_cloud = a.input.contains('#') || ohl_core::files::is_project_file(Path::new(&a.input));
    let (crs, scheme_text) = if is_cloud {
        let (file, name) = ohl_lidar::cloud_spec(&a.input);
        let src = ohl_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
        ohl_pointcloud::PointSource::visit(
            &src,
            corridor.as_ref(),
            None,
            &reading_budget(),
            &mut |x, y, z, class| {
                push(LidarPoint {
                    x,
                    y,
                    z,
                    class,
                    intensity: 0,
                    return_number: 1,
                    number_of_returns: 1,
                })
            },
        )?;
        (src.cloud().crs.clone(), src.cloud().class_scheme.clone())
    } else {
        let tiles = ohl_lidar::tiles_in(Path::new(&a.input)).map_err(|e| e.to_string())?;
        ohl_lidar::for_each_record(&tiles, corridor.as_ref(), &reading_budget(), |p| push(*p))
            .map_err(|e| e.to_string())?;
        (String::new(), None)
    };
    if too_many {
        return Err(format!(
            "more than {} points: classify a corridor (--line) or raise --max-points",
            a.max_points
        ));
    }
    println!("read {} points ({:.1?})", points.len(), started.elapsed());
    let params = ohl_lidar::classify::ClassifyParams {
        cell: a.cell,
        max_window: a.max_window,
        slope: a.slope,
        ground_tolerance: a.ground_tolerance,
        reclassify: if a.reclassify.is_empty() {
            vec![0, 1]
        } else {
            a.reclassify.clone()
        },
        ..Default::default()
    };
    // Exit clause: ~3 million points a second; 30 min is far beyond.
    let budget = Budget::new().time_limit(Duration::from_secs(1800));
    let t = Instant::now();
    let stats =
        ohl_lidar::classify::classify(&mut points, &params, &budget).map_err(|e| e.to_string())?;
    println!(
        "classified {} points, kept {} ({} of {} cells ground, {:.1?})",
        stats.classified,
        stats.kept,
        stats.ground_cells,
        stats.cells,
        t.elapsed()
    );
    let names = ohl_lidar::ClassScheme::asprs();
    for (class, n) in stats.counts.iter().enumerate().filter(|(_, n)| **n > 0) {
        let name = names
            .class(class as u8)
            .map_or(String::new(), |c| c.name.clone());
        println!("  {class:>3} {n:>12}  {name}");
    }
    let lower = a.out.to_lowercase();
    if lower.ends_with(".las") || lower.ends_with(".laz") {
        ohl_lidar::classify::write_las(Path::new(&a.out), &points, &budget)
            .map_err(|e| e.to_string())?;
        println!("written to {}", a.out);
    } else {
        let (file, name) = ohl_lidar::cloud_spec(&a.out);
        let name = name.ok_or("write the output as FILE.ohl#cloud-name (or a .las/.laz file)")?;
        let mut store = ohl_pointcloud::Store::open(file).map_err(|e| e.to_string())?;
        if store
            .clouds()
            .map_err(|e| e.to_string())?
            .iter()
            .any(|c| c.name == name)
        {
            if !a.replace {
                return Err(format!(
                    "{} already holds a cloud \"{name}\" (--replace to replace it)",
                    file.display()
                ));
            }
            store.delete_cloud(name).map_err(|e| e.to_string())?;
        }
        let (cloud, _) = store
            .import_points(
                name,
                &crs,
                scheme_text.as_deref(),
                ohl_pointcloud::DEFAULT_CHUNK,
                ohl_pointcloud::DEFAULT_SCALE,
                points.iter().cloned(),
                &budget,
            )
            .map_err(|e| e.to_string())?;
        println!(
            "stored as \"{}\" in {} ({} points)",
            cloud.name,
            file.display(),
            cloud.points
        );
    }
    Ok(())
}

/// Runs `ohl lidar vegetation`.
pub fn vegetation(a: VegetationArgs) -> Result<(), String> {
    let (mut line, library) = load(&a.model)?;
    let chosen = cases(&a.cases, &mut line)?;
    let (source, scheme) = source(&a.points)?;
    let classes = classes_of(&scheme, &a.role, &["vegetation"])?;
    let ground_classes = scheme.codes_with_role(&a.ground_role);
    let mut wanted = classes.clone();
    wanted.extend(&ground_classes);
    wanted.sort_unstable();
    wanted.dedup();
    let (mut is_veg, mut is_ground) = ([false; 256], [false; 256]);
    classes.iter().for_each(|&c| is_veg[c as usize] = true);
    ground_classes
        .iter()
        .for_each(|&c| is_ground[c as usize] = true);
    // Points kept in memory as whole millimetres from the first structure.
    let origin = line.structures.first().map_or([0.0; 3], |s| s.position);
    let mm = |v: f64, k: usize| ((v - origin[k]) * 1000.0).round() as i32;
    let mut veg: Vec<([i32; 3], u8)> = Vec::new();
    let mut ground = ohl_lidar::LowestPerCell::new(Some(1.0));
    let started = Instant::now();
    let corridor = ohl_lidar::Corridor::along_spans(&line, a.corridor);
    source.visit(
        Some(&corridor),
        Some(&wanted),
        &reading_budget(),
        &mut |x, y, z, c| {
            if is_ground[c as usize] {
                ground.add(x, y, z);
            }
            if is_veg[c as usize] {
                veg.push(([mm(x, 0), mm(y, 1), mm(z, 2)], c));
            }
        },
    )?;
    let terrain = if ground.points().len() >= 3 {
        Some(ground.terrain(Some(50.0)).map_err(|e| e.to_string())?)
    } else {
        None
    };
    println!(
        "points: {} — {} vegetation points of classes {classes:?}, ground {} points ({:.1?})",
        source.describe(),
        veg.len(),
        ground.points().len(),
        started.elapsed()
    );
    if a.fall_in && terrain.is_none() {
        println!("  no ground points: the fall-in check is left out");
    }
    let params = VegetationParams {
        clearance: a.clearance,
        growth_per_year: a.growth,
        years: a.years,
        buffer: a.buffer,
        fall_in: a.fall_in,
        root_ball: a.root_ball,
        fall_allowance: a.fall_allowance,
        max_tree_height: a.max_tree_height,
        max_base_ground_distance: a.max_base_ground_distance,
        ..VegetationParams::default()
    };
    // Exit clause of the whole run (solves, checks, lines).
    let budget = Budget::new().time_limit(Duration::from_secs(a.time_limit));
    let t = Instant::now();
    let mut positions = ohl_clearance::positions::solve_positions(
        &line,
        &library,
        &chosen,
        ohl_clearance::positions::PositionOptions {
            swing_arc: a.swing_arc,
        },
        &budget,
    )?;
    let at = |v: i32, k: usize| v as f64 / 1000.0 + origin[k];
    if let Some(mode) = a.unbalanced_ice {
        let reach = if params.fall_in {
            params.near_reach().max(params.fall_reach())
        } else {
            params.near_reach()
        };
        let spans = ohl_clearance::vegetation::spans_near(
            &positions,
            reach,
            veg.iter().map(|(p, _)| [at(p[0], 0), at(p[1], 1)]),
        );
        let solves = ohl_clearance::positions::add_unbalanced_ice(
            &mut positions,
            &line,
            &library,
            &chosen,
            mode,
            &spans,
            &budget,
        )?;
        println!(
            "unbalanced ice: {} of {} spans with vegetation within {reach:.1} m, {solves} solves",
            spans.len(),
            positions.spans.len()
        );
    }
    for n in &positions.notes {
        println!("  {n}");
    }
    let count = |k: PositionKind| positions.positions.iter().filter(|p| p.kind == k).count();
    println!(
        "positions: {} spans, {} cases — {} as the case, {} left, {} right, {} swing arcs, {} with unbalanced ice ({:.1?})",
        positions.spans.len(),
        positions.cases.len(),
        count(PositionKind::Case),
        count(PositionKind::Left),
        count(PositionKind::Right),
        count(PositionKind::Arc),
        count(PositionKind::Ice) + count(PositionKind::IceLeft) + count(PositionKind::IceRight),
        t.elapsed()
    );
    let positions = Arc::new(positions);
    let t = Instant::now();
    let mut check =
        VegetationCheck::with_positions(Arc::clone(&positions), &classes, params.clone());
    check.visit_parallel(
        terrain.as_ref(),
        &veg,
        |(p, c)| (at(p[0], 0), at(p[1], 1), at(p[2], 2), *c),
        &budget,
    )?;
    let (points, work) = (check.points, check.work);
    if params.fall_in && check.without_ground > 0 {
        println!(
            "  {} points without a tree base (no fall-in check): {} with no ground below them, {} with no ground point within {} m",
            check.without_ground,
            check.without_ground - check.ground_too_far,
            check.ground_too_far,
            params.max_base_ground_distance.unwrap_or(f64::INFINITY)
        );
    }
    let results = check.finish();
    let (enc, fall): (u64, u64) = results
        .iter()
        .fold((0, 0), |(e, f), r| (e + r.encroaching, f + r.fall_in));
    let elapsed = t.elapsed();
    println!(
        "{enc} points within {:.2} m (clearance {:.2} + growth {:.2} + buffer {:.2}), {fall} points of trees that could fall in (root ball {}, allowance {:.2} m) — {points} points in {elapsed:.1?}, {:.2} µs a point",
        params.clearance + params.growth() + params.buffer,
        params.clearance,
        params.growth(),
        params.buffer,
        params.root_ball,
        params.fall_allowance,
        elapsed.as_secs_f64() * 1e6 / points.max(1) as f64
    );
    println!(
        "  work: {} point–span pairs, {} distances to positions, {} fall sectors, {} sheltered by the ground",
        work.pairs, work.distances, work.sectors, work.sheltered
    );
    println!(
        "  {:<18} {:>8} {:>8} {:>10} {:<26} {:>13} {:<26}",
        "span",
        "grow-in",
        "fall-in",
        "nearest m",
        "case, position",
        "fall margin m",
        "case, position"
    );
    let place = |k: Option<usize>| k.map_or("-".to_owned(), |k| positions.describe(k));
    for (s, r) in positions.spans.iter().zip(&results) {
        if r.encroaching == 0 && r.fall_in == 0 {
            continue;
        }
        println!(
            "  {:<18} {:>8} {:>8} {:>10} {:<26} {:>13} {:<26}",
            wire_name(&s.wire),
            r.encroaching,
            r.fall_in,
            r.nearest
                .map_or("-".into(), |v| format!("{:.2}", v.distance)),
            place(r.nearest.and_then(|v| v.position)),
            r.worst_fall
                .and_then(|v| v.fall_margin)
                .map_or("-".into(), |f| format!("{f:.2}")),
            place(r.worst_fall.and_then(|v| v.fall_position)),
        );
    }
    if let Some(path) = &a.csv {
        write(
            path,
            &ohl_clearance::vegetation::danger_csv(&positions, &results, &params),
        )?;
    }
    if !a.isoclearance.is_empty() {
        let Some(ground) = terrain.as_ref() else {
            return Err("isoclearance lines need the ground: no ground points".into());
        };
        let iso = ohl_clearance::isoclearance::IsoParams {
            interval: a.iso_interval,
            max_level: a.iso_max,
            spacing: a.iso_spacing,
            coarse: a.iso_coarse,
        };
        let bounds = corridor
            .bounds()
            .ok_or("isoclearance lines: no span to draw them along")?;
        let t = Instant::now();
        let lines = ohl_clearance::isoclearance::isoclearance(
            &positions,
            ground,
            bounds,
            &|x, y| corridor.contains(x, y),
            &iso,
            &budget,
        )?;
        println!(
            "isoclearance: {} lines every {} m up to {} m, {} of {} coarse cells refined to {} m, {} clearances ({:.1?}){}",
            lines.contours.len(),
            iso.interval,
            iso.max_level,
            lines.refined_cells,
            lines.coarse_cells,
            iso.spacing,
            lines.nodes,
            t.elapsed(),
            lines.lowest.map_or(String::new(), |(c, _)| format!(
                "; lowest clearance {c:.2} m"
            ))
        );
        for path in &a.isoclearance {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let text = match ext.as_str() {
                "geojson" | "json" => {
                    ohl_clearance::isoclearance::to_geojson(&lines.contours, &line.name, &line.crs)
                }
                "csv" => ohl_clearance::isoclearance::to_csv(&lines.contours),
                "dxf" => {
                    let l: Vec<(f64, Vec<[f64; 3]>)> = lines
                        .contours
                        .iter()
                        .map(|c| (c.level, c.points.clone()))
                        .collect();
                    ohl_drawing::dxf::write(&ohl_drawing::isoclearance_model(
                        &l,
                        iso.max_level,
                        2000.0,
                    ))
                }
                _ => {
                    return Err(format!(
                        "{}: write the isoclearance lines as .geojson, .dxf or .csv",
                        path.display()
                    ));
                }
            };
            write(path, &text)?;
        }
    }
    Ok(())
}

/// Runs `ohl lidar fit-wires`.
pub fn fit_wires(a: FitArgs) -> Result<(), String> {
    let (mut line, library) = load(&a.model)?;
    if let Some(w) = &a.weather {
        line.set_weather_cases(&WeatherCases::load(w)?.cases);
    }
    let (source, scheme) = source(&a.points)?;
    let classes = classes_of(&scheme, &a.role, &["conductor", "shield_wire"])?;
    let case = WeatherCase::still(
        "model",
        ohl_core::units::Temperature::from_celsius(a.temperature),
    );
    let shapes = solve(&line, &library, &case);
    let wires = wires_of(&line, &shapes, &[]);
    let params = FitParams {
        lateral: a.lateral,
        vertical: a.vertical,
        ..FitParams::default()
    };
    let mut collector = WireCollector::new(wires, &classes, params.clone());
    let started = Instant::now();
    let corridor = ohl_lidar::Corridor::along_spans(&line, a.lateral + 2.0);
    let stats = source.visit(
        Some(&corridor),
        Some(&classes),
        &reading_budget(),
        &mut |x, y, z, c| collector.visit(x, y, z, c),
    )?;
    println!(
        "points: {} — {} of classes {classes:?} near the spans ({:.1?})",
        source.describe(),
        stats.points_visited,
        started.elapsed()
    );
    // Exit clause per span: ~10 iterations usual.
    let fits = collector.fit(&Budget::new().max_iterations(500));
    let mut rows = String::from(
        "section,span,points,dropped,rms_m,catenary_m,sag_m,model_sag_m,sag_difference_m,mid_difference_m,low_x,low_y,low_z,low_difference_m,temperature_c\n",
    );
    println!(
        "  {:<18} {:>6} {:>7} {:>9} {:>9} {:>9} {:>9} {:>8}",
        "span", "points", "rms m", "sag m", "model m", "Δsag m", "Δmid m", "°C"
    );
    let (mut fitted, mut temps) = (0, Vec::new());
    for (w, f) in collector.wires().iter().zip(&fits) {
        let Some(f) = f else { continue };
        let f = match f {
            Ok(f) => f,
            Err(e) => {
                println!("  {:<18} {e}", wire_name(w));
                continue;
            }
        };
        fitted += 1;
        let temperature = if a.no_temperature {
            None
        } else {
            let section = line
                .sections
                .iter()
                .find(|s| s.id == w.section)
                .ok_or("section not found")?;
            let cable = library
                .get(&section.cable)
                .ok_or_else(|| format!("cable {} not in the library", section.cable))?;
            // Exit clause: about 6 section solves are usual.
            let budget = Budget::new().max_iterations(60);
            ohl_clearance::wirefit::estimate_temperature(
                &line,
                section,
                cable,
                w.span,
                f.sag,
                (a.min_temperature, a.max_temperature),
                &budget,
            )?
        };
        temps.extend(temperature);
        println!(
            "  {:<18} {:>6} {:>7.3} {:>9.3} {:>9.3} {:>9.3} {:>9.3} {:>8}",
            wire_name(w),
            f.fit.points,
            f.fit.rms,
            f.sag,
            f.model_sag,
            f.sag - f.model_sag,
            f.mid_difference,
            temperature.map_or("-".into(), |t| format!("{t:.1}"))
        );
        let _ = writeln!(
            rows,
            "{},{},{},{},{:.4},{:.2},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{}",
            csv_field(&w.section),
            w.span,
            f.fit.points,
            f.fit.dropped,
            f.fit.rms,
            f.fit.curve.c,
            f.sag,
            f.model_sag,
            f.sag - f.model_sag,
            f.mid_difference,
            f.low_point[0],
            f.low_point[1],
            f.low_point[2],
            f.low_difference,
            temperature.map_or(String::new(), |t| format!("{t:.2}")),
        );
    }
    println!(
        "fitted {fitted} of {} spans{}",
        fits.len(),
        if temps.is_empty() {
            String::new()
        } else {
            format!(
                "; temperature at the survey: mean {:.1} °C over {} spans",
                temps.iter().sum::<f64>() / temps.len() as f64,
                temps.len()
            )
        }
    );
    if let Some(path) = &a.csv {
        write(path, &rows)?;
    }
    Ok(())
}
