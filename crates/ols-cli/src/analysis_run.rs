// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols clearance galloping|row` and `ols lidar classify|vegetation|
//! fit-wires`: analyses of a solved line against its galloping ellipses,
//! its right of way and LiDAR points ([`ols_clearance::galloping`],
//! [`ols_clearance::blowout`], [`ols_clearance::vegetation`],
//! [`ols_clearance::wirefit`], [`ols_lidar::classify`]).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use ols_clearance::galloping::{self, GallopingParams};
use ols_clearance::vegetation::{VegetationCheck, VegetationParams};
use ols_clearance::wirefit::{FitParams, WireCollector};
use ols_clearance::{Wire, wires_of};
use ols_core::budget::Budget;
use ols_core::text::csv_field;
use ols_line::Line;
use ols_line::solve::SectionShape;
use ols_weather::{WeatherCase, WeatherCases};

/// The line and where its libraries are.
#[derive(Args)]
pub struct ModelArgs {
    /// Line file (.ols.json) or project file (.ols).
    pub file: PathBuf,
    /// Wire library: .olsw file or directory (default: the shipped
    /// libraries/cables).
    #[arg(long)]
    pub library: Option<PathBuf>,
    /// Structure type directory (.olss) for typed structures (default: the
    /// `Structures` folder next to the line).
    #[arg(long)]
    pub structures: Option<PathBuf>,
    /// Insulator assembly library directory (.olsi).
    #[arg(long)]
    pub insulators: Option<PathBuf>,
}

/// Weather cases from a file.
#[derive(Args)]
pub struct CaseArgs {
    /// Weather case file (.olswc).
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
    /// Point cloud in a project file (`FILE.ols` or `FILE.ols#name`).
    #[arg(long, conflicts_with = "lidar")]
    pub pointcloud: Option<String>,
    /// LAS/LAZ file or directory of tiles.
    #[arg(long)]
    pub lidar: Option<PathBuf>,
    /// Class scheme (.olsc); default: the cloud's own, else ASPRS LAS 1.4.
    #[arg(long)]
    pub lidar_classes: Option<PathBuf>,
}

/// `ols clearance …`.
#[derive(Subcommand)]
pub enum ClearanceCommand {
    /// Galloping ellipses of the wires of each span (parameters: .olsg
    /// file) and their clearances to each other.
    Galloping {
        #[command(flatten)]
        model: ModelArgs,
        #[command(flatten)]
        cases: CaseArgs,
        /// Galloping parameters (.olsg).
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

/// `ols lidar classify`.
#[derive(Args)]
pub struct ClassifyArgs {
    /// Points: LAS/LAZ file or directory, or `FILE.ols#cloud`.
    pub input: String,
    /// Output: a .las/.laz file, or `FILE.ols#cloud` (a new cloud).
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

/// `ols lidar vegetation`.
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
    /// Half width of the corridor around the spans' chords, m.
    #[arg(long, default_value_t = 60.0)]
    pub corridor: f64,
    /// Write the nearest and worst points of every span as CSV.
    #[arg(long)]
    pub csv: Option<PathBuf>,
}

/// `ols lidar fit-wires`.
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
fn load(m: &ModelArgs) -> Result<(Line, ols_cables::Library), String> {
    use ols_core::files::shipped_library;
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
            ols_structures::StructureLibrary::load_dir(&dir).map_err(|e| e.to_string())?;
        let insulators =
            ols_structures::InsulatorLibrary::load_dir(&insulators).map_err(|e| e.to_string())?;
        line = line
            .resolve(&structures, &insulators)
            .map_err(|e| e.to_string())?;
    }
    let library = m
        .library
        .clone()
        .unwrap_or_else(|| shipped_library("cables"));
    let library = ols_cables::Library::load(&library).map_err(|e| e.to_string())?;
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
fn solve(line: &Line, library: &ols_cables::Library, case: &WeatherCase) -> Vec<SectionShape> {
    let mut shapes = Vec::new();
    for (section, r) in line
        .sections
        .iter()
        .zip(ols_line::solve::solve_line(line, library, case))
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

/// Runs `ols clearance …`.
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
            let centres = ols_clearance::blowout::centrelines(&line);
            let mut rows = String::from("case,section,span,right_m,left_m,encroachment_m,x,y,z\n");
            for case in &chosen {
                let shapes = solve(&line, &library, case);
                let wires = wires_of(&line, &shapes, &[]);
                let mut results = ols_clearance::blowout::row_check(&wires, &centres, left, right);
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
) -> Result<(Box<dyn ols_pointcloud::PointSource>, ols_lidar::ClassScheme), String> {
    let source: Box<dyn ols_pointcloud::PointSource> = match (&p.pointcloud, &p.lidar) {
        (Some(spec), _) => {
            let (file, name) = ols_lidar::cloud_spec(spec);
            Box::new(ols_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?)
        }
        (None, Some(dir)) => Box::new(ols_pointcloud::TileSource {
            tiles: ols_lidar::tiles_in(dir).map_err(|e| e.to_string())?,
            scheme: None,
        }),
        (None, None) => return Err("give --pointcloud or --lidar".into()),
    };
    let scheme = match (&p.lidar_classes, source.class_scheme()) {
        (Some(path), _) => ols_lidar::ClassScheme::load(path)?,
        (None, Some(text)) => ols_lidar::ClassScheme::parse(&text)?,
        (None, None) => ols_lidar::ClassScheme::asprs(),
    };
    Ok((source, scheme))
}

/// Class numbers of `roles` (default `fallback`), an error when none.
fn classes_of(
    scheme: &ols_lidar::ClassScheme,
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

/// Runs `ols lidar classify`.
pub fn classify(a: ClassifyArgs) -> Result<(), String> {
    use ols_lidar::LidarPoint;
    let started = Instant::now();
    let corridor = match &a.line {
        Some(path) => {
            let line = crate::project::load_line(path)?;
            let routes: Vec<Vec<[f64; 2]>> = ols_clearance::blowout::centrelines(&line)
                .iter()
                .map(|c| c.points.iter().map(|p| [p[0], p[1]]).collect())
                .collect();
            Some(ols_lidar::Corridor::new(&routes, a.corridor))
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
    let is_cloud = a.input.contains('#') || a.input.ends_with(".ols");
    let (crs, scheme_text) = if is_cloud {
        let (file, name) = ols_lidar::cloud_spec(&a.input);
        let src = ols_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
        ols_pointcloud::PointSource::visit(
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
        let tiles = ols_lidar::tiles_in(Path::new(&a.input)).map_err(|e| e.to_string())?;
        ols_lidar::for_each_record(&tiles, corridor.as_ref(), &reading_budget(), |p| push(*p))
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
    let params = ols_lidar::classify::ClassifyParams {
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
        ols_lidar::classify::classify(&mut points, &params, &budget).map_err(|e| e.to_string())?;
    println!(
        "classified {} points, kept {} ({} of {} cells ground, {:.1?})",
        stats.classified,
        stats.kept,
        stats.ground_cells,
        stats.cells,
        t.elapsed()
    );
    let names = ols_lidar::ClassScheme::asprs();
    for (class, n) in stats.counts.iter().enumerate().filter(|(_, n)| **n > 0) {
        let name = names
            .class(class as u8)
            .map_or(String::new(), |c| c.name.clone());
        println!("  {class:>3} {n:>12}  {name}");
    }
    let lower = a.out.to_lowercase();
    if lower.ends_with(".las") || lower.ends_with(".laz") {
        ols_lidar::classify::write_las(Path::new(&a.out), &points, &budget)
            .map_err(|e| e.to_string())?;
        println!("written to {}", a.out);
    } else {
        let (file, name) = ols_lidar::cloud_spec(&a.out);
        let name = name.ok_or("write the output as FILE.ols#cloud-name (or a .las/.laz file)")?;
        let mut store = ols_pointcloud::Store::open(file).map_err(|e| e.to_string())?;
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
                ols_pointcloud::DEFAULT_CHUNK,
                ols_pointcloud::DEFAULT_SCALE,
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

/// Runs `ols lidar vegetation`.
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
    let mut ground = ols_lidar::LowestPerCell::new(Some(1.0));
    let started = Instant::now();
    let corridor = ols_lidar::Corridor::along_spans(&line, a.corridor);
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
        fall_in: a.fall_in,
        max_tree_height: a.max_tree_height,
        ..VegetationParams::default()
    };
    let mut rows = String::from(ols_clearance::vegetation::CSV_HEADER);
    for case in &chosen {
        let t = Instant::now();
        let shapes = solve(&line, &library, case);
        let wires = wires_of(&line, &shapes, &[]);
        let mut check = VegetationCheck::new(wires.clone(), &classes, params.clone());
        let at = |v: i32, k: usize| v as f64 / 1000.0 + origin[k];
        for (p, c) in &veg {
            check.visit(terrain.as_ref(), at(p[0], 0), at(p[1], 1), at(p[2], 2), *c);
        }
        let results = check.finish();
        let (enc, fall): (u64, u64) = results
            .iter()
            .fold((0, 0), |(e, f), r| (e + r.encroaching, f + r.fall_in));
        println!(
            "case \"{}\": {enc} points within {:.2} m (clearance {:.2} + growth {:.2}), {fall} points of trees that could fall in ({:.1?})",
            case.name,
            params.clearance + params.growth(),
            params.clearance,
            params.growth(),
            t.elapsed()
        );
        println!(
            "  {:<18} {:>11} {:>8} {:>11} {:>14}",
            "span", "encroaching", "fall-in", "nearest m", "fall margin m"
        );
        for (w, r) in wires.iter().zip(&results) {
            if r.encroaching == 0 && r.fall_in == 0 {
                continue;
            }
            println!(
                "  {:<18} {:>11} {:>8} {:>11} {:>14}",
                wire_name(w),
                r.encroaching,
                r.fall_in,
                r.nearest
                    .map_or("-".into(), |v| format!("{:.2}", v.distance)),
                r.worst_fall
                    .and_then(|v| v.fall_margin)
                    .map_or("-".into(), |f| format!("{f:.2}")),
            );
        }
        let csv = ols_clearance::vegetation::to_csv(&case.name, &wires, &results, &params);
        rows.push_str(csv.split_once('\n').map_or("", |x| x.1));
    }
    if let Some(path) = &a.csv {
        write(path, &rows)?;
    }
    Ok(())
}

/// Runs `ols lidar fit-wires`.
pub fn fit_wires(a: FitArgs) -> Result<(), String> {
    let (mut line, library) = load(&a.model)?;
    if let Some(w) = &a.weather {
        line.set_weather_cases(&WeatherCases::load(w)?.cases);
    }
    let (source, scheme) = source(&a.points)?;
    let classes = classes_of(&scheme, &a.role, &["conductor", "shield_wire"])?;
    let case = WeatherCase::still(
        "model",
        ols_core::units::Temperature::from_celsius(a.temperature),
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
    let corridor = ols_lidar::Corridor::along_spans(&line, a.lateral + 2.0);
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
            ols_clearance::wirefit::estimate_temperature(
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
