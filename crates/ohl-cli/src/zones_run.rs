// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl zones`: the check of the desktop application's **Vertical
//! clearances…** and **Radial clearances…** windows without the window
//! ([`ohl_clearance::zone::run_zone_check`]): the zone, the rules, the
//! wind and what is solved come from a rule file (`.ohlr`, its `[check]`
//! and `[wind]`), the targets from the project — its point cloud and its
//! terrain surface — or from files given.
//!
//! Exit status: 0 with no violation, 1 with violations, 2 on an error.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use ohl_clearance::zone::{self, Measure, Zone, ZoneCheckResult, ZoneFinding};
use ohl_clearance::{RuleSet, Target, WindSteps};
use ohl_core::budget::Budget;
use ohl_core::text::csv_field;
use ohl_terrain::Terrain;
use ohl_weather::WeatherCases;

/// The project setting naming the weather case file used last (as the
/// desktop application keeps it: a path, or the name of one of the
/// project's own weather case files).
const WEATHER_SETTING: &str = "app.weather";
/// The project setting naming the surface the checks use.
const CHECK_SURFACE_SETTING: &str = "app.check_surface";
/// The project setting of earlier builds with the active surface (JSON).
const SURFACES_SETTING: &str = "app.surfaces";

/// Arguments of `ohl zones`.
#[derive(clap::Args, Clone)]
pub struct ZonesArgs {
    /// Line file (.ohl.json) or project file (.ohl).
    pub file: PathBuf,
    /// Clearance rules (.ohlr; for a project, also the name of one of its
    /// own): the rules of one measure (vertical or radial) with ground or
    /// LiDAR class targets; its `[check]` (circuits, method, search
    /// distance) and `[wind]` are used.
    #[arg(long)]
    pub rules: PathBuf,
    /// The measure checked, when the rule file has both: vertical or
    /// radial.
    #[arg(long)]
    pub measure: Option<String>,
    /// Weather case file (.ohlwc; for a project, also the name of one of
    /// its own) (default: the file of the same name as the rule file, next
    /// to it or in a `weather` folder beside it — a design code's preset;
    /// else the project's weather case file used last, else its first).
    #[arg(long)]
    pub weather: Option<PathBuf>,
    /// Weather case to check, by name (`*` and `?` wildcards; repeatable;
    /// default: every case of the rules).
    #[arg(long)]
    pub case: Vec<String>,
    /// Point cloud: a project file (`FILE.ohl` or `FILE.ohl#name`) or a
    /// COPC file (`FILE.copc.laz`) (default: the project's own cloud, if
    /// it holds one).
    #[arg(long)]
    pub cloud: Option<String>,
    /// Read no point cloud (the ground only).
    #[arg(long, conflicts_with = "cloud")]
    pub no_cloud: bool,
    /// Survey points (CSV / XYZ) for the terrain surface (default: the
    /// project's surface of the checks, else its active surface, else the
    /// ground classes of the point cloud).
    #[arg(long)]
    pub terrain: Option<PathBuf>,
    /// A terrain surface stored in the project, by name, as the ground.
    #[arg(long, conflicts_with = "terrain")]
    pub surface: Option<String>,
    /// Class scheme (.ohlc) of the LiDAR (default: the point cloud's own,
    /// else ASPRS LAS 1.4): the classes of the rules' roles.
    #[arg(long)]
    pub lidar_classes: Option<PathBuf>,
    /// Wire library: .ohlw file or directory (default: a project's own,
    /// else the shipped libraries/cables).
    #[arg(long)]
    pub library: Option<PathBuf>,
    /// Structure type directory (.ohls), for typed structures (default: a
    /// project's own, else the `Structures` folder next to the line or
    /// project file).
    #[arg(long)]
    pub structures: Option<PathBuf>,
    /// Insulator assembly library directory (.ohli) (default: a project's
    /// own, else the shipped libraries/insulators).
    #[arg(long)]
    pub insulators: Option<PathBuf>,
    /// LiDAR points farther from the wires are ignored, m (overrides the
    /// rule file's `search_distance`).
    #[arg(long)]
    pub search_distance: Option<f64>,
    /// Check only these circuits, by id or name (repeatable; `""`:
    /// sections in none; overrides the rule file's `circuits`; an unknown
    /// circuit is refused).
    #[arg(long)]
    pub circuits: Vec<String>,
    /// Solve every section by this method: fe (finite elements) or rs
    /// (ruling span) (overrides the rule file's `method`).
    #[arg(long)]
    pub method: Option<String>,
    /// Also check each span anywhere on its swing arc between still air
    /// and the full blowouts (the rule file's `[wind] arc = true`): three
    /// solves per windy case, a closed-form distance per point.
    #[arg(long)]
    pub swing_arc: bool,
    /// The swing band's margin, m (the rule file's `[wind] band_margin`;
    /// default 0.15 m, the measured worst excess of the intermediate
    /// pressures plus an allowance, conservative): every distance to a
    /// band is taken this much smaller; a smaller value is on the user's
    /// responsibility.
    #[arg(long)]
    pub band_margin: Option<f64>,
    /// The jumpers' swing band margin, m (the rule file's `[wind]
    /// jumper_band_margin`; default 0 m, no widening — the jumper's band
    /// then does not hold it between its blowouts: on the user's
    /// responsibility, a value to enter for their jumpers; exact: the
    /// wind steps).
    #[arg(long)]
    pub jumper_band_margin: Option<f64>,
    /// Step the wind by this many Pa from 0 up to each case's pressure
    /// (the rule file's `[wind] steps` and `step`).
    #[arg(long)]
    pub wind_step: Option<f64>,
    /// The sides the wind blows from: both, left or right (the rule
    /// file's `[wind] sides`).
    #[arg(long)]
    pub wind_sides: Option<String>,
    /// Write every finding (the violations, then each rule's lowest
    /// clearance) as CSV.
    #[arg(long)]
    pub csv: Option<PathBuf>,
    /// Stop after this many seconds (solving and reading the points).
    #[arg(long, default_value_t = 600.0)]
    pub time_limit: f64,
}

/// `ohl zones`: 0 with no violation, 1 with violations, 2 on an error.
pub fn command(args: &ZonesArgs) -> ExitCode {
    match run(args) {
        Ok(r) if r.violations() == 0 => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

/// The measure of a rule set's ground and point rules: the only one, or
/// the one asked for.
fn measure_of(set: &RuleSet, asked: Option<&str>) -> Result<Measure, String> {
    let ours = set
        .rules
        .iter()
        .filter(|r| matches!(r.target, Target::Ground | Target::Points { .. }));
    let (mut vertical, mut radial) = (false, false);
    for r in ours {
        vertical |= r.vertical.is_some();
        radial |= r.radial.is_some();
    }
    match asked {
        Some("vertical") => Ok(Measure::Vertical),
        Some("radial") => Ok(Measure::Radial),
        Some(other) => Err(format!("--measure {other}: write vertical or radial")),
        None => match (vertical, radial) {
            (true, false) => Ok(Measure::Vertical),
            (false, true) => Ok(Measure::Radial),
            (true, true) => Err(
                "the rule file has vertical and radial clearances: choose one with --measure \
                 vertical or --measure radial"
                    .into(),
            ),
            (false, false) => Err(
                "the rule file has no vertical or radial clearance to the ground or to LiDAR \
                 classes"
                    .into(),
            ),
        },
    }
}

/// Opens a project file (`None`: a line file).
fn session_of(file: &Path) -> Result<Option<ohl_session::Session>, String> {
    if !crate::project::is_project(file) {
        return Ok(None);
    }
    crate::project::open_existing(file).map(Some)
}

/// The weather cases: given, the preset's (same name as the rule file),
/// else the project's used last, else the project's first weather case
/// file; with where they come from.
fn weather_cases(
    args: &ZonesArgs,
    session: Option<&ohl_session::Session>,
    libs: &crate::project::Libraries,
) -> Result<(WeatherCases, String), String> {
    if let Some(p) = &args.weather {
        return Ok((WeatherCases::load(p)?, libs.describe(p)));
    }
    let ext = ohl_weather::WEATHER_FILE_EXTENSION;
    if let (Some(dir), Some(stem)) = (args.rules.parent(), args.rules.file_stem()) {
        let name = format!("{}.{ext}", stem.to_string_lossy());
        for p in [dir.join(&name), dir.join("../weather").join(&name)] {
            if p.is_file() {
                return Ok((WeatherCases::load(&p)?, libs.describe(&p)));
            }
        }
    }
    if let Some(s) = session
        && let Ok(Some(used)) = s.setting(WEATHER_SETTING)
    {
        let p = PathBuf::from(&used);
        if p.is_absolute() && p.is_file() {
            return Ok((WeatherCases::load(&p)?, p.display().to_string()));
        }
        // One of the project's own weather case files.
        let own = s.library_files("weather").map_err(|e| e.to_string())?;
        if let Some((name, text)) = own.iter().find(|(n, _)| *n == used) {
            let set = WeatherCases::parse(text).map_err(|e| format!("{name}: {e}"))?;
            return Ok((set, format!("{name} (the project's)")));
        }
    }
    if let Some(p) = libs.first("weather") {
        return Ok((WeatherCases::load(&p)?, libs.describe(&p)));
    }
    Err("no weather case file: give --weather".into())
}

/// The point cloud: given, else the project's own (if it holds one).
fn cloud_of(args: &ZonesArgs) -> Option<String> {
    if args.no_cloud {
        return None;
    }
    args.cloud.clone().or_else(|| {
        let is_project = ohl_core::files::is_project_file(&args.file);
        let has = is_project
            && ohl_pointcloud::Store::open(&args.file)
                .and_then(|s| s.clouds())
                .is_ok_and(|c| !c.is_empty());
        has.then(|| args.file.display().to_string())
    })
}

/// A surface stored in the project.
fn stored_surface(session: &ohl_session::Session, name: &str) -> Result<Terrain, String> {
    let (points, triangles) = session.surface(name).map_err(|e| format!("{name}: {e}"))?;
    Terrain::from_mesh(points, triangles).map_err(|e| format!("{name}: {e}"))
}

/// The ground: survey points given, a surface named, the project's surface
/// of the checks, its active surface, else built from the ground classes
/// of the point cloud along the spans (as the desktop application does);
/// with where it comes from.
fn ground_of(
    args: &ZonesArgs,
    session: Option<&ohl_session::Session>,
    line: &ohl_line::Line,
    cloud: Option<&str>,
    scheme: &ohl_lidar::ClassScheme,
    budget: &Budget,
) -> Result<Option<(Terrain, String)>, String> {
    if let Some(p) = &args.terrain {
        let t = Terrain::load(p).map_err(|e| format!("{}: {e}", p.display()))?;
        return Ok(Some((t, format!("survey file {}", p.display()))));
    }
    if let Some(name) = &args.surface {
        let s = session.ok_or("--surface: a project file holds the surfaces")?;
        return Ok(Some((stored_surface(s, name)?, format!("surface {name}"))));
    }
    if let Some(s) = session {
        let check = s.setting(CHECK_SURFACE_SETTING).ok().flatten();
        let active = || {
            s.setting(SURFACES_SETTING)
                .ok()
                .flatten()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v.get("active")?.as_str().map(str::to_owned))
        };
        if let Some(name) = check.or_else(active) {
            return Ok(Some((stored_surface(s, &name)?, format!("surface {name}"))));
        }
    }
    let Some(spec) = cloud else {
        return Ok(None);
    };
    // The ground classes in a corridor along the spans: the lowest point
    // per square metre, triangles of up to 50 m (the desktop
    // application's LiDAR ground).
    let classes = scheme.codes_with_role("ground");
    if classes.is_empty() {
        return Ok(None);
    }
    let source = ohl_pointcloud::open_source(spec)?;
    let corridor = ohl_lidar::Corridor::along_spans(line, 60.0);
    let mut lowest = ohl_lidar::LowestPerCell::new(Some(1.0));
    source.visit(
        Some(&corridor),
        Some(&classes),
        budget,
        &mut |x, y, z, _| lowest.add(x, y, z),
    )?;
    if lowest.points().len() < 3 {
        return Ok(None);
    }
    let t = lowest.terrain(Some(50.0)).map_err(|e| e.to_string())?;
    Ok(Some((t, format!("ground classes of {spec}"))))
}

/// Runs the check and prints its report (and writes the CSV).
pub fn run(args: &ZonesArgs) -> Result<ZoneCheckResult, String> {
    // A project's own libraries (unless given), and its own rule and
    // weather case files by name.
    let libs = crate::project::Libraries::resolve(
        &args.file,
        crate::project::GivenLibraries {
            cables: args.library.as_deref(),
            structures: args.structures.as_deref(),
            insulators: args.insulators.as_deref(),
            sections: None,
        },
    )?;
    let mut own = args.clone();
    own.rules = libs.named("rules", &args.rules).0;
    own.weather = args.weather.as_deref().map(|w| libs.named("weather", w).0);
    let args = &own;
    if !(args.time_limit.is_finite() && args.time_limit > 0.0) {
        return Err("--time-limit must be > 0 s".into());
    }
    let limit = Duration::from_secs_f64(args.time_limit);
    let budget = Budget::new().time_limit(limit);
    let started = Instant::now();
    let stopped = |e: String| {
        if started.elapsed() >= limit {
            format!(
                "the check stopped at the time limit of {} s (--time-limit): {e}",
                args.time_limit
            )
        } else {
            e
        }
    };
    let (library, insulators, structures) = (&libs.cables, &libs.insulators, &libs.structures);
    let (mut line, _) = crate::project::load_resolved(&args.file, structures, insulators)?;
    let library = ohl_cables::Library::load(library).map_err(|e| e.to_string())?;
    let session = session_of(&args.file)?;

    // The rules, their measure, zone, wind and check.
    let set = RuleSet::load_with(&args.rules, &(&line.defaults).into())?;
    let measure = measure_of(&set, args.measure.as_deref())?;
    let cloud = cloud_of(args);
    let scheme = match (
        &args.lidar_classes,
        cloud
            .as_deref()
            .and_then(|spec| ohl_pointcloud::open_source(spec).ok()?.class_scheme()),
    ) {
        (Some(p), _) => ohl_lidar::ClassScheme::load(p)?,
        (None, Some(text)) => ohl_lidar::ClassScheme::parse(&text)?,
        (None, None) => ohl_lidar::ClassScheme::asprs(),
    };
    let imported = zone::from_rule_set(
        &set,
        measure,
        |role| scheme.codes_with_role(role),
        &zone::line_voltages(&line),
    );
    if imported.rules.is_empty() {
        return Err(format!(
            "no {} rule to the ground or to LiDAR classes in {}{}",
            measure.as_str(),
            libs.describe(&args.rules),
            if imported.notes.is_empty() {
                String::new()
            } else {
                format!(" ({})", imported.notes.join("; "))
            }
        ));
    }
    let mut z = Zone::with_defaults(measure, &line.defaults);
    if let Some(w) = imported.width {
        z.width = w;
    }
    if let Some(d) = imported.depth {
        z.depth = d;
    }
    let mut wind = imported.wind.unwrap_or_default();
    if args.swing_arc {
        wind.arc = true;
    }
    if let Some(m) = args.band_margin {
        if let Some(e) = ohl_clearance::rules::band_margin_problem(m) {
            return Err(format!("--band-margin: {e}"));
        }
        wind.band_margin = m;
    }
    if let Some(m) = args.jumper_band_margin {
        if let Some(e) = ohl_clearance::rules::band_margin_problem(m) {
            return Err(format!("--jumper-band-margin: {e}"));
        }
        wind.jumper_band_margin = m;
    }
    if let Some(step) = args.wind_step {
        if !(step.is_finite() && step > 0.0) {
            return Err("--wind-step must be > 0 Pa".into());
        }
        wind.stepped = true;
        wind.step = step;
    }
    if let Some(sides) = &args.wind_sides {
        wind.sides =
            ohl_clearance::WindSides::parse(sides).map_err(|e| format!("--wind-sides: {e}"))?;
    }
    let mut check = imported.check.clone().unwrap_or_default();
    if let Some(d) = args.search_distance {
        if !(d.is_finite() && d > 0.0) {
            return Err("--search-distance must be > 0 m".into());
        }
        check.search_distance = Some(d);
    }
    if !args.circuits.is_empty() {
        // By id or name: the circuits' names (an unknown one refused).
        check.circuits = line
            .circuit_names_for(&args.circuits)
            .map_err(|e| format!("--circuits: {e}"))?;
    }
    if let Some(m) = &args.method {
        check.method =
            Some(ohl_clearance::rules::parse_method(m).map_err(|e| format!("--method {e}"))?);
    }
    z.search = check.search_distance;
    let rules = imported.rules;

    // The weather cases and the wind positions.
    let (weather, weather_from) = weather_cases(args, session.as_ref(), &libs)?;
    line.set_weather_cases(&weather.cases);
    let cases = weather.select(&args.case, &[])?;
    let states = zone::wind_states(&cases, |n| rules.iter().any(|r| r.applies(n)), &wind);
    if states.is_empty() {
        return Err(format!(
            "no weather case of the rules in {weather_from}{}",
            if args.case.is_empty() {
                ""
            } else {
                " (of those chosen with --case)"
            }
        ));
    }

    // The targets: the ground of the rules naming no surface, and the
    // surfaces the others name (read from the project).
    let mut named: Vec<(String, Terrain)> = Vec::new();
    for r in rules.iter().filter(|r| r.ground) {
        if let Some(n) = &r.surface
            && !named.iter().any(|(m, _)| m == n)
        {
            match session.as_ref() {
                Some(s) => match stored_surface(s, n) {
                    Ok(t) => named.push((n.clone(), t)),
                    Err(e) => println!("  note: \"{}\": {e}", r.name),
                },
                None => println!(
                    "  note: \"{}\": surface {n} — a project file holds the surfaces",
                    r.name
                ),
            }
        }
    }
    let wants_ground = rules.iter().any(|r| r.ground && r.surface.is_none());
    let wants_points = rules.iter().any(|r| !r.classes.is_empty());
    let ground = if wants_ground {
        ground_of(
            args,
            session.as_ref(),
            &line,
            cloud.as_deref(),
            &scheme,
            &budget,
        )
        .map_err(stopped)?
    } else {
        None
    };
    let cloud = cloud.filter(|_| wants_points);

    println!(
        "{} clearances: {} — {} rule row(s){}",
        measure.as_str(),
        if set.name.is_empty() {
            libs.describe(&args.rules)
        } else {
            format!("{} ({})", set.name, libs.describe(&args.rules))
        },
        rules.len(),
        describe_check(&z, &wind, &check)
    );
    for n in condense_notes(&imported.notes) {
        println!("  note: {n}");
    }
    libs.print(&["cables", "structures", "insulators"]);
    println!(
        "weather: {weather_from} — {} case(s), {} wind position(s)",
        cases.len(),
        states.len()
    );
    println!(
        "ground: {}",
        match (&ground, wants_ground) {
            (Some((_, from)), _) => from.clone(),
            (None, true) => "none (the ground rules find nothing)".into(),
            (None, false) => "not needed".into(),
        }
    );
    if !named.is_empty() {
        println!(
            "surfaces named by rules: {}",
            named
                .iter()
                .map(|(n, t)| format!("{n} ({} triangles)", t.num_triangles()))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let surfaces: Vec<(String, &Terrain)> = named.iter().map(|(n, t)| (n.clone(), t)).collect();
    println!(
        "points: {}",
        match (&cloud, wants_points) {
            (Some(spec), _) => spec.clone(),
            (None, true) => "none (the class rules find nothing; give --cloud)".into(),
            (None, false) => "not needed".into(),
        }
    );

    let read = |polylines: &[Vec<[f64; 2]>],
                reach: f64,
                classes: &[u8],
                budget: &Budget,
                visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        let spec = cloud.as_deref().unwrap_or_default();
        ohl_pointcloud::visit_around(spec, polylines, reach, classes, budget, visit).map(|_| ())
    };
    let done = AtomicUsize::new(0);
    let result = zone::run_zone_check(
        &line,
        &library,
        z,
        &check,
        rules,
        states,
        wind.arc.then_some(wind.margins()),
        cloud.is_some().then_some(&read as _),
        ground.as_ref().map(|(t, _)| t),
        &surfaces,
        None,
        &budget,
        &done,
    )
    .map_err(stopped)?;
    print_report(&result, &scheme);
    if let Some(out) = &args.csv {
        std::fs::write(out, csv(&result, &scheme))
            .map_err(|e| format!("{}: {e}", out.display()))?;
        println!("findings written to {}", out.display());
    }
    Ok(result)
}

/// ", zone ±1.5 m in slices of 0.2 m, wind stepped by 50 Pa from both
/// sides, circuits North, method fe, points within 30 m".
fn describe_check(z: &Zone, wind: &WindSteps, check: &ohl_clearance::CheckScope) -> String {
    let mut parts = Vec::new();
    if z.measure == Measure::Vertical {
        parts.push(format!("zone ±{} m in slices of {} m", z.width, z.depth));
    }
    let sides = match wind.sides {
        ohl_clearance::WindSides::Both => "the left and the right",
        ohl_clearance::WindSides::Left => "the left",
        ohl_clearance::WindSides::Right => "the right",
    };
    let margin = format!(
        "band margin {} m, jumpers {} m",
        wind.band_margin, wind.jumper_band_margin
    );
    parts.push(if wind.stepped && wind.arc {
        format!(
            "wind stepped by {} Pa from {sides}, and the swing arc ({margin})",
            wind.step
        )
    } else if wind.arc {
        format!("the cases' wind from {sides} and the swing arc ({margin})")
    } else if wind.stepped {
        format!("wind stepped by {} Pa from {sides}", wind.step)
    } else {
        format!("the cases' wind from {sides}")
    });
    if !check.circuits.is_empty() {
        parts.push(format!("circuits {}", check.circuits.join(", ")));
    }
    if let Some(m) = check.method {
        parts.push(format!("method {}", ohl_clearance::rules::method_name(m)));
    }
    if let Some(d) = check.search_distance {
        parts.push(format!("points within {d} m"));
    }
    format!("; {}", parts.join(", "))
}

/// "S1 span 2", "jumper T3".
fn span_name(f: &ZoneFinding) -> String {
    match f.section.strip_prefix("jumper:") {
        Some(j) => format!("jumper {j}"),
        None => format!("{} span {}", f.section, f.span + 1),
    }
}

/// "ground", "class 5 High Vegetation".
fn target_name(class: Option<u8>, scheme: &ohl_lidar::ClassScheme) -> String {
    match class {
        None => "ground".into(),
        Some(c) => match scheme.class(c) {
            Some(d) => format!("class {c} {}", d.name),
            None => format!("class {c}"),
        },
    }
}

/// The findings: the violations (worst first), then each rule's lowest
/// clearance that keeps its required clearance — `(status, finding)`.
fn rows(r: &ZoneCheckResult) -> Vec<(&'static str, &ZoneFinding)> {
    let mut out: Vec<(&'static str, &ZoneFinding)> =
        r.zone.findings.iter().map(|f| ("violation", f)).collect();
    out.extend(
        r.zone
            .lowest
            .iter()
            .flatten()
            .filter(|f| f.distance >= f.required)
            .map(|f| ("ok", f)),
    );
    out
}

/// The summary and the findings table.
fn print_report(r: &ZoneCheckResult, scheme: &ohl_lidar::ClassScheme) {
    println!(
        "solved {} section(s) in {} wind position(s){} in {:.1?}; checked in {:.1?}; {} LiDAR \
         points read",
        r.sections.len(),
        r.states.len(),
        if r.arcs.is_empty() {
            String::new()
        } else {
            format!(", {} swing arcs", r.arc_spans)
        },
        r.solving,
        r.checking,
        r.zone.points
    );
    for f in &r.failed {
        println!("  not solved: {f}");
    }
    for n in condense_notes(&r.zone.notes) {
        println!("  note: {n}");
    }
    let n = r.violations();
    println!("{n} span{} below a rule", if n == 1 { "" } else { "s" });
    let rows = rows(r);
    if rows.is_empty() {
        println!("nothing within reach of the rules");
        return;
    }
    println!(
        "  {:<9} {:<28} {:<16} {:<44} {:<26} {:>9} {:>10} {:>8}",
        "status", "rule", "span", "case & wind", "target", "surface m", "required m", "count"
    );
    for (status, f) in rows {
        let unit = match (f.class, f.count) {
            (None, 1) => "piece",
            (None, _) => "pieces",
            (Some(_), 1) => "point",
            (Some(_), _) => "points",
        };
        println!(
            "  {:<9} {:<28} {:<16} {:<44} {:<26} {:>9.2} {:>10.2} {:>8}",
            status,
            r.rules[f.rule].name,
            span_name(f),
            f.position(&r.states),
            target_name(f.class, scheme),
            f.distance,
            f.required,
            if status == "ok" {
                String::new()
            } else {
                format!("{} {unit}", f.count)
            }
        );
    }
    let empty: Vec<(&str, &[f64])> = r
        .zone
        .lowest
        .iter()
        .enumerate()
        .filter(|(_, low)| low.is_none())
        .map(|(k, _)| (r.rules[k].name.as_str(), r.rules[k].voltages.as_slice()))
        .collect();
    for line in nothing_within_reach(&empty, &r.zone.notes) {
        println!("  {line}");
    }
}

/// A note "\"NAME\": no wire of 150 kV" (a rule row that checks no wire,
/// section or jumper of its voltage): (name, wire kind, voltage text).
fn unused_row(note: &str) -> Option<(&str, &str, &str)> {
    let rest = note.strip_prefix('"')?;
    let (name, rest) = rest.split_once("\": no ")?;
    let (kind, rest) = rest.split_once(" of ")?;
    let kv = rest.strip_suffix(" kV")?;
    let mut voltages = kv.split(" or ").flat_map(|p| p.split(", "));
    (matches!(kind, "wire" | "section" | "jumper") && voltages.all(|v| v.parse::<f64>().is_ok()))
        .then_some((name, kind, kv))
}

/// The notes of a check for the summary: the rule rows of a voltage the
/// line does not have (one per rule and voltage) in one note per wire
/// kind — "no wire of 150, 125, 110 kV: 12 rule rows not used" — and a
/// note repeated once with its count. The CSV keeps every finding.
pub fn condense_notes(notes: &[String]) -> Vec<String> {
    // Kind -> (voltages in order, rows); other notes -> count, in order.
    let mut unused: Vec<(&str, Vec<&str>, usize)> = Vec::new();
    let mut others: Vec<(&str, usize)> = Vec::new();
    // Where each group goes among the notes (the first of it).
    let mut order: Vec<(bool, usize)> = Vec::new();
    for n in notes {
        if let Some((_, kind, kv)) = unused_row(n) {
            match unused.iter_mut().position(|(k, _, _)| *k == kind) {
                Some(i) => {
                    let (_, kvs, rows) = &mut unused[i];
                    if !kvs.contains(&kv) {
                        kvs.push(kv);
                    }
                    *rows += 1;
                }
                None => {
                    order.push((true, unused.len()));
                    unused.push((kind, vec![kv], 1));
                }
            }
        } else {
            match others.iter_mut().position(|(t, _)| *t == n.as_str()) {
                Some(i) => others[i].1 += 1,
                None => {
                    order.push((false, others.len()));
                    others.push((n, 1));
                }
            }
        }
    }
    order
        .into_iter()
        .map(|(is_unused, i)| {
            if is_unused {
                let (kind, kvs, rows) = &unused[i];
                format!(
                    "no {kind} of {} kV: {rows} rule row{} not used",
                    kvs.join(", "),
                    if *rows == 1 { "" } else { "s" }
                )
            } else {
                match others[i] {
                    (text, 1) => text.to_owned(),
                    (text, n) => format!("{text} ({n} times)"),
                }
            }
        })
        .collect()
}

/// The summary's "nothing within reach" lines: one per rule name (in
/// order), with the voltages of its rows when it has several — the rows
/// that check no wire of their voltage left out (a note says so); `rows`:
/// the name and voltage of each rule row without a clearance found.
pub fn nothing_within_reach(rows: &[(&str, &[f64])], notes: &[String]) -> Vec<String> {
    let unused: Vec<(&str, &str)> = notes
        .iter()
        .filter_map(|n| unused_row(n))
        .map(|(name, _, kv)| (name, kv))
        .collect();
    let mut by_name: Vec<(&str, Vec<String>, usize)> = Vec::new();
    for (name, kv) in rows {
        // A row's voltages as its note writes them ("220", "110 or 220").
        let kv = (!kv.is_empty()).then(|| {
            let t = ohl_clearance::zone::voltages_text(kv);
            t.strip_suffix(" kV").unwrap_or(&t).to_owned()
        });
        if let Some(v) = &kv
            && unused.contains(&(*name, v.as_str()))
        {
            continue;
        }
        let i = match by_name.iter().position(|(n, _, _)| n == name) {
            Some(i) => i,
            None => {
                by_name.push((name, Vec::new(), 0));
                by_name.len() - 1
            }
        };
        let (_, kvs, count) = &mut by_name[i];
        *count += 1;
        if let Some(v) = kv
            && !kvs.contains(&v)
        {
            kvs.push(v);
        }
    }
    by_name
        .into_iter()
        .map(|(name, kvs, count)| {
            if count > 1 && !kvs.is_empty() {
                format!("{name}: nothing within reach ({} kV)", kvs.join(", "))
            } else {
                format!("{name}: nothing within reach")
            }
        })
        .collect()
}

/// The CSV header of [`csv`].
pub const CSV_HEADER: &str = "status,rule,section,span,case,wind_pa,wind_from,target,class,clearance_m,required_m,count,wire_x,wire_y,wire_z,target_x,target_y,target_z,arc_deg\n";

/// Every finding as CSV: the violations, then each rule's lowest clearance
/// that keeps its required clearance (`status` ok).
pub fn csv(r: &ZoneCheckResult, scheme: &ohl_lidar::ClassScheme) -> String {
    let mut out = String::from(CSV_HEADER);
    for (status, f) in rows(r) {
        let s = &r.states[f.state];
        let (section, span) = match f.section.strip_prefix("jumper:") {
            Some(j) => (format!("jumper {j}"), String::new()),
            None => (f.section.clone(), (f.span + 1).to_string()),
        };
        let _ = writeln!(
            out,
            "{status},{},{},{span},{},{:.0},{},{},{},{:.3},{:.3},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{}",
            csv_field(&r.rules[f.rule].name),
            csv_field(&section),
            csv_field(&s.name),
            if f.arc.is_some() { s.full } else { s.pressure },
            match f.arc {
                Some(_) => "arc",
                None => s.side.map_or("", |d| d.as_str()),
            },
            csv_field(&target_name(f.class, scheme)),
            f.class.map_or(String::new(), |c| c.to_string()),
            f.distance,
            f.required,
            f.count,
            f.wire[0],
            f.wire[1],
            f.wire[2],
            f.target[0],
            f.target[1],
            f.target[2],
            f.arc.map_or(String::new(), |a| format!("{a:.1}")),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rule of many voltages: one note for the rows of voltages the line
    /// lacks, one "nothing within reach" line per rule name.
    #[test]
    fn summary_groups_rule_rows_by_name() {
        let mut notes: Vec<String> = [150.0, 125.0, 110.0, 150.0]
            .iter()
            .map(|v| format!("\"Trees\": no wire of {v} kV"))
            .collect();
        notes.push("\"Roads\": no wire of 110 kV".into());
        notes.push("\"Ground\": none of its weather cases is in the model".into());
        notes.push("\"Ground\": none of its weather cases is in the model".into());
        notes.push("\"Earth\": no jumper of 16.7 kV".into());
        assert_eq!(
            condense_notes(&notes),
            [
                "no wire of 150, 125, 110 kV: 5 rule rows not used",
                "\"Ground\": none of its weather cases is in the model (2 times)",
                "no jumper of 16.7 kV: 1 rule row not used",
            ]
        );
        // Of the rows without a clearance: those of a voltage not on the
        // line are in the note; the others grouped by name.
        let rows: [(&str, &[f64]); 6] = [
            ("Trees", &[150.0]),
            ("Trees", &[220.0]),
            ("Trees", &[380.0]),
            ("Trees", &[220.0]),
            ("Roads", &[110.0]),
            ("Ground", &[]),
        ];
        assert_eq!(
            nothing_within_reach(&rows, &notes),
            [
                "Trees: nothing within reach (220, 380 kV)",
                "Ground: nothing within reach",
            ]
        );
        // A row of several voltages (one rule, 2026-10-21): its note
        // "110 or 220 kV" is understood.
        let several = vec!["\"Fences\": no wire of 110 or 220 kV".to_owned()];
        assert_eq!(
            nothing_within_reach(&[("Fences", &[110.0, 220.0])], &several),
            Vec::<String>::new()
        );
        // "any voltage" is not a voltage: kept as it is.
        let any = vec!["\"X\": no wire of any voltage".to_owned()];
        assert_eq!(condense_notes(&any), any);
    }
}
