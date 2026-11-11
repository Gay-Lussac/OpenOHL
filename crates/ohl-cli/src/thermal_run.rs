// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl thermal`: thermal ratings of a conductor (ampacity, temperature,
//! transient) and the clearance-limited temperature of a line.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use ohl_core::files::shipped_library;
use ohl_core::text::csv_field;
use ohl_thermal::limit::{Limit, LimitOptions, Targets};
use ohl_thermal::{Ambient, AmbientCases, Conductor, HeatBalance, Method};

/// Thermal rating commands.
#[derive(Subcommand)]
pub enum ThermalCommand {
    /// Steady-state ampacity for a maximum temperature (--max-temp), the
    /// temperature for a current (--current), or the temperature after a
    /// step of current (--current with --from-current).
    Ampacity(Box<AmpacityArgs>),
    /// Per section, the highest conductor temperature at which the ground
    /// and point clearances are met, and the ampacity at it per ambient
    /// case.
    ClearanceLimit(Box<LimitArgs>),
    /// Per section, the conductor temperature at a current (given, or the
    /// systems' operating currents of the line file) in the hottest of the
    /// ambient cases, the section solved at it and checked by the ground
    /// and point clearance rules: pass or fail, the least clearance.
    ClearanceCheck(Box<CheckArgs>),
}

/// Arguments of `ohl thermal ampacity`.
#[derive(Args)]
pub struct AmpacityArgs {
    /// Wire library: .ohlw file or directory (default: the shipped
    /// libraries/cables).
    #[arg(long)]
    library: Option<PathBuf>,
    /// Wire type id (`--cable` is still read).
    #[arg(long = "wire-type", alias = "cable")]
    cable: String,
    /// Ambient cases (.ohlta).
    #[arg(long)]
    ambient: PathBuf,
    /// Ambient case to run, by name (`*` `?` wildcards; repeatable;
    /// default: all).
    #[arg(long)]
    case: Vec<String>,
    /// Method: ieee738, cigre601 or cigre207.
    #[arg(long, default_value = "ieee738")]
    method: String,
    /// Maximum conductor temperature, °C: the ampacity.
    #[arg(long, conflicts_with = "current", required_unless_present = "current")]
    max_temp: Option<f64>,
    /// Current, A: the steady-state conductor temperature.
    #[arg(long)]
    current: Option<f64>,
    /// Current before the step, A: the temperature after switching from it
    /// (steady state) to --current.
    #[arg(long, requires = "current")]
    from_current: Option<f64>,
    /// Duration of the transient, s.
    #[arg(long, default_value_t = 3600.0)]
    duration: f64,
    /// Time step of the transient, s (default: the default values', else
    /// 10 s).
    #[arg(long)]
    time_step: Option<f64>,
    /// Default values (.ohldef, Criteria › Default values exported):
    /// emissivity, absorptivity, roughness, albedo and time step where the
    /// wire type or the case gives none (default: the program's).
    #[arg(long)]
    defaults: Option<PathBuf>,
    /// Print the transient every this many seconds.
    #[arg(long, default_value_t = 300.0)]
    print_every: f64,
    /// Heat capacity of the conductor, J/(m·K) (default: estimated from
    /// its materials).
    #[arg(long)]
    heat_capacity: Option<f64>,
    /// Write the results (per case; the transient: every step) as CSV.
    #[arg(long)]
    csv: Option<PathBuf>,
}

/// What the clearance commands check against, and how (shared by
/// `clearance-limit` and `clearance-check`).
#[derive(Args)]
pub struct TargetArgs {
    /// Line file (.ohl.json) or project file (.ohl).
    file: PathBuf,
    /// Wire library (default: the shipped libraries/cables).
    #[arg(long)]
    library: Option<PathBuf>,
    /// Structure type directory (default: the `Structures` folder next to
    /// the line).
    #[arg(long)]
    structures: Option<PathBuf>,
    /// Insulator library directory (default: libraries/insulators).
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// Weather case file (.ohlwc): the base case (--case) whose temperature
    /// is varied; default still air without ice.
    #[arg(long)]
    weather: Option<PathBuf>,
    /// The base weather case, by name.
    #[arg(long, requires = "weather")]
    case: Option<String>,
    /// Ambient cases (.ohlta): the ampacities at the limits
    /// (clearance-limit); the conductor temperature at the current
    /// (clearance-check, required there).
    #[arg(long)]
    ambient: Option<PathBuf>,
    /// Ambient case, by name (repeatable; default: all).
    #[arg(long)]
    ambient_case: Vec<String>,
    /// Method: ieee738, cigre601 or cigre207.
    #[arg(long, default_value = "ieee738")]
    method: String,
    /// Clearance rules (.ohlr): their ground and point rules are checked.
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Required vertical ground clearance, m (adds a ground rule).
    #[arg(long)]
    clearance: Option<f64>,
    /// Survey points (CSV / XYZ) for the terrain surface.
    #[arg(long)]
    terrain: Option<PathBuf>,
    /// LiDAR tiles (LAS/LAZ file or directory).
    #[arg(long, conflicts_with = "terrain")]
    lidar: Option<PathBuf>,
    /// Point cloud stored in a project file (`FILE.ohl` or `FILE.ohl#name`).
    #[arg(long, conflicts_with_all = ["terrain", "lidar"])]
    pointcloud: Option<String>,
    /// Class scheme (.ohlc) of the LiDAR; default ASPRS LAS 1.4.
    #[arg(long)]
    lidar_classes: Option<PathBuf>,
    /// Role of the classes that make the terrain surface.
    #[arg(long, default_value = "ground")]
    ground_role: String,
    /// Half width of the LiDAR corridor around the spans, m.
    #[arg(long, default_value_t = 60.0)]
    corridor: f64,
    /// Longest triangle side of the LiDAR terrain surface, m.
    #[arg(long, default_value_t = 50.0)]
    max_edge: f64,
    /// Thin the LiDAR ground points: lowest point per cell of this size, m.
    #[arg(long)]
    thin: Option<f64>,
    /// Write one row per section as CSV.
    #[arg(long)]
    csv: Option<PathBuf>,
}

/// Arguments of `ohl thermal clearance-limit`.
#[derive(Args)]
pub struct LimitArgs {
    #[command(flatten)]
    targets: TargetArgs,
    /// Lowest conductor temperature tried, °C.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    min_temp: f64,
    /// Highest conductor temperature tried, °C.
    #[arg(long, default_value_t = 150.0)]
    max_temp: f64,
    /// Step of the upward scan, °C.
    #[arg(long, default_value_t = 10.0)]
    step: f64,
    /// Resolution of the limit, °C.
    #[arg(long, default_value_t = 0.5)]
    tolerance: f64,
}

/// Arguments of `ohl thermal clearance-check`.
#[derive(Args)]
pub struct CheckArgs {
    #[command(flatten)]
    targets: TargetArgs,
    /// Current, A rms per phase, in every phase section (shared by a
    /// bundle's subconductors); default: each section's system's operating
    /// current from the line file (`current_a`).
    #[arg(long)]
    current: Option<f64>,
}

/// Runs a thermal command.
pub fn run(command: ThermalCommand) -> Result<(), String> {
    match command {
        ThermalCommand::Ampacity(a) => ampacity(&a),
        ThermalCommand::ClearanceLimit(a) => clearance_limit(&a),
        ThermalCommand::ClearanceCheck(a) => clearance_check(&a),
    }
}

fn write_csv(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("written to {}", path.display());
    Ok(())
}

/// One line about a heat balance.
fn terms(b: &HeatBalance) -> String {
    let sun = match (b.solar.sun, b.solar.incidence) {
        (Some(s), Some(theta)) => format!(
            " (sun altitude {:.1}°, azimuth {:.1}°, {theta:.1}° to the line)",
            s.altitude, s.azimuth
        ),
        _ => String::new(),
    };
    format!(
        "Joule {:.2}, solar {:.2}{sun}, convection {:.2} ({}), radiation {:.2} W/m",
        b.joule,
        b.solar.heat,
        b.convection,
        b.convection_mode.as_str(),
        b.radiation
    )
}

fn ampacity(a: &AmpacityArgs) -> Result<(), String> {
    let method = Method::parse(&a.method)?;
    let library_path = a
        .library
        .clone()
        .unwrap_or_else(|| shipped_library("cables"));
    let library = ohl_cables::Library::load(&library_path).map_err(|e| e.to_string())?;
    let cable = library
        .get(&a.cable)
        .ok_or_else(|| format!("wire type {} is not in {}", a.cable, library_path.display()))?;
    let defaults = match &a.defaults {
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| format!("{}: {e}", p.display()))
            .and_then(|t| {
                ohl_line::Defaults::parse(&t).map_err(|e| format!("{}: {e}", p.display()))
            })?,
        None => ohl_line::Defaults::default(),
    };
    let time_step = a.time_step.unwrap_or(defaults.thermal_time_step);
    let mut conductor = Conductor::from_cable(cable)
        .map_err(|e| e.to_string())?
        .with_defaults(&defaults);
    if let Some(mc) = a.heat_capacity {
        conductor.heat_capacity = vec![(mc, 0.0)];
    }
    let cases = AmbientCases::load(&a.ambient)?.select(&a.case)?;
    for n in ohl_thermal::sections::cable_notes(cable) {
        println!("{n}");
    }
    println!(
        "{} — {}, {} ambient cases",
        cable.name,
        method.title(),
        cases.len()
    );
    let mut csv = String::new();
    match (a.max_temp, a.current, a.from_current) {
        (Some(t), _, _) => {
            csv.push_str("case,max_temperature_c,ampacity_a,joule_w_per_m,solar_w_per_m,convection_w_per_m,radiation_w_per_m\n");
            for case in &cases {
                match ohl_thermal::ampacity(method, &conductor, case, t) {
                    Ok(b) => {
                        println!("  {:<24} {:>8.0} A at {t} °C", case.name, b.current);
                        println!("    {}", terms(&b));
                        let _ = writeln!(
                            csv,
                            "{},{t},{:.3},{:.4},{:.4},{:.4},{:.4}",
                            csv_field(&case.name),
                            b.current,
                            b.joule,
                            b.solar.heat,
                            b.convection,
                            b.radiation
                        );
                    }
                    Err(e) => println!("  {:<24} {e}", case.name),
                }
            }
        }
        (None, Some(i), None) => {
            csv.push_str("case,current_a,temperature_c,joule_w_per_m,solar_w_per_m,convection_w_per_m,radiation_w_per_m\n");
            for case in &cases {
                let b = ohl_thermal::temperature(
                    method,
                    &conductor,
                    case,
                    i,
                    &ohl_thermal::default_steady_budget(),
                )
                .map_err(|e| format!("{}: {e}", case.name))?;
                println!("  {:<24} {:>7.1} °C at {i} A", case.name, b.temperature);
                println!("    {}", terms(&b));
                let _ = writeln!(
                    csv,
                    "{},{i},{:.4},{:.4},{:.4},{:.4},{:.4}",
                    csv_field(&case.name),
                    b.temperature,
                    b.joule,
                    b.solar.heat,
                    b.convection,
                    b.radiation
                );
            }
        }
        (None, Some(i), Some(i0)) => {
            println!(
                "  heat capacity {:.0} J/(m·K) at 20 °C; time step {} s",
                conductor.heat_capacity_at(20.0),
                time_step
            );
            csv.push_str("case,time_s,temperature_c\n");
            for case in &cases {
                let budget = ohl_thermal::default_steady_budget();
                let start = ohl_thermal::temperature(method, &conductor, case, i0, &budget)
                    .map_err(|e| format!("{}: {e}", case.name))?;
                let end = ohl_thermal::temperature(method, &conductor, case, i, &budget)
                    .map_err(|e| format!("{}: {e}", case.name))?;
                let steps = ohl_thermal::transient(
                    method,
                    &conductor,
                    case,
                    start.temperature,
                    i,
                    a.duration,
                    time_step,
                )
                .map_err(|e| format!("{}: {e}", case.name))?;
                println!(
                    "  {}: {i0} A → {i} A, from {:.1} °C towards {:.1} °C",
                    case.name, start.temperature, end.temperature
                );
                let mut next = 0.0;
                for (k, (time, t)) in steps.iter().enumerate() {
                    if *time >= next - 1e-9 || k + 1 == steps.len() {
                        println!("    {:>8.0} s {:>7.1} °C", time, t);
                        next += a.print_every.max(time_step);
                    }
                    let _ = writeln!(csv, "{},{time},{t:.4}", csv_field(&case.name));
                }
            }
        }
        _ => return Err("give --max-temp or --current".into()),
    }
    if let Some(path) = &a.csv {
        write_csv(path, &csv)?;
    }
    Ok(())
}

/// What a clearance command works on: the line, the wires, the base
/// weather case, the ambient cases and what the rules check against.
struct Inputs {
    line: ohl_line::Line,
    library: ohl_cables::Library,
    base: ohl_weather::WeatherCase,
    ambient: Vec<Ambient>,
    prepared: crate::clearance_run::Prepared,
}

/// The [`Inputs`] `a` asks for; the base case without `--weather` is
/// still air at `still` °C (its temperature is replaced).
fn prepare(a: &TargetArgs, still: f64) -> Result<Inputs, String> {
    let mut line = crate::project::load_line(&a.file)?;
    let insulators = a
        .insulators
        .clone()
        .unwrap_or_else(|| shipped_library("insulators"));
    let structures = a
        .structures
        .clone()
        .unwrap_or_else(|| ohl_core::files::dir_of(&a.file).join("Structures"));
    if line.structures.iter().any(|s| s.structure_type.is_some()) {
        let types =
            ohl_structures::StructureLibrary::load_dir(&structures).map_err(|e| e.to_string())?;
        let ins =
            ohl_structures::InsulatorLibrary::load_dir(&insulators).map_err(|e| e.to_string())?;
        line = line.resolve(&types, &ins).map_err(|e| e.to_string())?;
    }
    let library = ohl_cables::Library::load(
        &a.library
            .clone()
            .unwrap_or_else(|| shipped_library("cables")),
    )
    .map_err(|e| e.to_string())?;
    let base = match &a.weather {
        Some(path) => {
            let set = ohl_weather::WeatherCases::load(path)?;
            line.set_weather_cases(&set.cases);
            let name = a
                .case
                .clone()
                .ok_or("--weather needs --case: the base case whose temperature is varied")?;
            set.select(&[name], &[])?
                .into_iter()
                .next()
                .ok_or("no such weather case")?
        }
        None => ohl_weather::WeatherCase::still(
            "thermal limit",
            ohl_core::units::Temperature::from_celsius(still),
        ),
    };
    let ambient: Vec<Ambient> = match &a.ambient {
        Some(p) => AmbientCases::load(p)?.select(&a.ambient_case)?,
        None => Vec::new(),
    };
    let prepared = crate::clearance_run::prepare(
        &line,
        &crate::clearance_run::Options {
            terrain: a.terrain.as_deref(),
            lidar: a.lidar.as_deref(),
            pointcloud: a.pointcloud.as_deref(),
            lidar_classes: a.lidar_classes.as_deref(),
            ground_role: &a.ground_role,
            corridor: a.corridor,
            max_edge: a.max_edge,
            thin: a.thin,
            rules: a.rules.as_deref(),
            clearance: a.clearance,
            other_lines: &[],
            structures: &structures,
            insulators: &insulators,
            // Its terrain surfaces, for the ground rules naming one.
            project: crate::project::is_project(&a.file).then_some(a.file.as_path()),
        },
    )?;
    Ok(Inputs {
        line,
        library,
        base,
        ambient,
        prepared,
    })
}

/// "Rule span 2 vertical 4.99 m < 5.00 m".
fn governing_words(g: &ohl_thermal::limit::Governing) -> String {
    format!(
        "{} span {} {} {:.2} m {} {:.2} m",
        g.rule,
        g.span + 1,
        g.measure,
        g.clearance,
        if g.met() { "≥" } else { "<" },
        g.required
    )
}

fn clearance_limit(l: &LimitArgs) -> Result<(), String> {
    let a = &l.targets;
    let method = Method::parse(&a.method)?;
    let options = LimitOptions {
        min_temperature: l.min_temp,
        max_temperature: l.max_temp,
        scan_step: l.step,
        tolerance: l.tolerance,
    };
    options.validate()?;
    let Inputs {
        line,
        library,
        base,
        ambient,
        prepared,
    } = prepare(a, l.min_temp)?;
    let surfaces: Vec<(String, &ohl_terrain::Terrain)> = prepared
        .surfaces
        .iter()
        .map(|(n, t)| (n.clone(), t))
        .collect();
    let mut feed = |_: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        prepared.feed_points(visit);
        Ok(())
    };
    let mut targets = Targets {
        ground: prepared.ground.as_ref(),
        surfaces: &surfaces,
        point_classes: prepared.point_classes(),
        points: Some(&mut feed),
    };
    let started = std::time::Instant::now();
    let budget = ohl_thermal::limit::default_budget(&options, line.sections.len());
    let limits = ohl_thermal::limit::clearance_limits(
        &line,
        &library,
        &base,
        prepared.rules(),
        &mut targets,
        &options,
        &budget,
        &mut |_, _| {},
    )?;
    println!(
        "clearance-limited temperature of {} sections ({}–{} °C, ±{} °C; base case \"{}\"; {:.1?}):",
        limits.len(),
        options.min_temperature,
        options.max_temperature,
        options.tolerance,
        base.name,
        started.elapsed()
    );
    for n in thermal_notes(&line, &library) {
        println!("  {n}");
    }
    // What the limits leave out, not on the safe side: each note once.
    let mut said: Vec<&str> = Vec::new();
    for n in limits.iter().flat_map(|l| &l.notes) {
        if !said.contains(&n.as_str()) {
            said.push(n);
            println!("  {n}");
        }
    }
    let mut header =
        "section,cable,limit_c,status,rule,span,measure,clearance_m,required_m".to_owned();
    for c in &ambient {
        let _ = write!(header, ",{}", csv_field(&format!("ampacity_a {}", c.name)));
    }
    let mut csv = header + "\n";
    for s in &limits {
        let (limit, status) = match &s.result {
            Ok(Limit::At(t)) => (Some(*t), format!("{t:.1} °C")),
            Ok(Limit::AboveRange) => (
                Some(options.max_temperature),
                format!("above {} °C", options.max_temperature),
            ),
            Ok(Limit::BelowRange) => (None, format!("below {} °C", options.min_temperature)),
            Err(e) => (None, format!("error: {e}")),
        };
        let governing = s.governing.as_ref().map_or(String::new(), |g| {
            format!("{} at {:.1} °C", governing_words(g), g.temperature)
        });
        let head = format!(
            "  {:<12} {:<20} {:<18} {governing}",
            s.section, s.cable, status
        );
        println!("{}", head.trim_end());
        let mut row = format!(
            "{},{},{},{},",
            csv_field(&s.section),
            csv_field(&s.cable),
            limit.map_or(String::new(), |t| format!("{t:.2}")),
            match &s.result {
                Ok(Limit::At(_)) => "limited",
                Ok(Limit::AboveRange) => "above_range",
                Ok(Limit::BelowRange) => "below_range",
                Err(_) => "error",
            }
        );
        match &s.governing {
            Some(g) => {
                let _ = write!(
                    row,
                    "{},{},{},{:.3},{:.3}",
                    csv_field(&g.rule),
                    g.span + 1,
                    g.measure,
                    g.clearance,
                    g.required
                );
            }
            None => row.push_str(",,,,"),
        }
        let conductor = library
            .get(&s.cable)
            .map(|c| Conductor::from_cable(c).map(|c| c.with_defaults(&line.defaults)));
        for c in &ambient {
            let rating = match (limit, &conductor) {
                (Some(t), Some(Ok(cond))) => ohl_thermal::ampacity(method, cond, c, t)
                    .map(|b| b.current)
                    .map_err(|e| e.to_string()),
                (_, Some(Err(e))) => Err(e.to_string()),
                _ => Err("no limit".into()),
            };
            match &rating {
                Ok(i) => {
                    // Above the range: the rating at its top, a lower bound.
                    let at_least = if s.result == Ok(Limit::AboveRange) {
                        "at least "
                    } else {
                        ""
                    };
                    println!(
                        "      {:<24} {at_least}{i:.0} A ({})",
                        c.name,
                        method.title()
                    );
                    let _ = write!(row, ",{i:.1}");
                }
                Err(e) => {
                    println!("      {:<24} {e}", c.name);
                    row.push(',');
                }
            }
        }
        csv.push_str(&row);
        csv.push('\n');
    }
    if let Some(path) = &a.csv {
        write_csv(path, &csv)?;
    }
    Ok(())
}

/// `ohl thermal clearance-check`: per section the steady temperature at
/// the current (the hottest of the ambient cases), the section solved at
/// it in the base case, checked by the ground and point rules — the solve
/// and check of `clearance-limit`, at one temperature.
fn clearance_check(c: &CheckArgs) -> Result<(), String> {
    let a = &c.targets;
    let method = Method::parse(&a.method)?;
    if a.ambient.is_none() {
        return Err(
            "--ambient: the ambient cases (.ohlta) the temperature at the current is found in"
                .into(),
        );
    }
    let current = match c.current {
        Some(i) if !(i.is_finite() && i >= 0.0) => {
            return Err(format!("--current {i}: a current ≥ 0 A"));
        }
        Some(i) => ohl_thermal::sections::Current::Given(i),
        None => ohl_thermal::sections::Current::Line,
    };
    let Inputs {
        line,
        library,
        base,
        ambient,
        prepared,
    } = prepare(a, 15.0)?;
    let heats = ohl_thermal::sections::section_temperatures(
        &line, &library, method, &ambient, current, None,
    );
    // The sections with a temperature, at the hottest case's.
    let temps: Vec<(String, f64)> = heats
        .iter()
        .filter_map(|h| h.hottest().map(|(_, t)| (h.section.clone(), t)))
        .collect();
    if temps.is_empty() {
        let why = heats
            .iter()
            .find_map(|h| h.current.as_ref().err().cloned())
            .unwrap_or_else(|| "no section".into());
        return Err(format!(
            "no section has a temperature at the current ({why}): give --current A or the systems' operating currents in the line file"
        ));
    }
    let surfaces: Vec<(String, &ohl_terrain::Terrain)> = prepared
        .surfaces
        .iter()
        .map(|(n, t)| (n.clone(), t))
        .collect();
    let mut feed = |_: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        prepared.feed_points(visit);
        Ok(())
    };
    let mut targets = Targets {
        ground: prepared.ground.as_ref(),
        surfaces: &surfaces,
        point_classes: prepared.point_classes(),
        points: Some(&mut feed),
    };
    let started = std::time::Instant::now();
    let checks = ohl_thermal::limit::clearance_at(
        &line,
        &library,
        &base,
        prepared.rules(),
        &mut targets,
        &temps,
        &ohl_thermal::limit::check_budget(temps.len()),
        &mut |_, _| {},
    )?;
    println!(
        "clearance at the current of {} sections ({}; the hottest of {} ambient cases; base case \"{}\"; {:.1?}):",
        heats.len(),
        method.title(),
        ambient.len(),
        base.name,
        started.elapsed()
    );
    for n in thermal_notes(&line, &library) {
        println!("  {n}");
    }
    let mut csv = "section,cable,current_a,bundle,temperature_c,ambient_case,status,rule,span,measure,clearance_m,required_m,margin_m\n".to_owned();
    let (mut passed, mut failed) = (0, 0);
    for h in &heats {
        let check = checks.iter().find(|k| k.section == h.section);
        let (case, temp) = match h.hottest() {
            Some((k, t)) => (ambient[k].name.clone(), Some(t)),
            None => (String::new(), None),
        };
        let (status, words, g) = match (check, &h.current) {
            (_, Err(e)) => ("no_current", e.clone(), None),
            (None, Ok(_)) => (
                "error",
                h.cases
                    .iter()
                    .find_map(|r| r.as_ref().err().cloned())
                    .unwrap_or_default(),
                None,
            ),
            (Some(k), Ok(_)) => match &k.result {
                Ok(Some(g)) if g.met() => ("pass", governing_words(g), Some(g)),
                Ok(Some(g)) => ("fail", governing_words(g), Some(g)),
                Ok(None) => ("not_checked", "nothing of it checked".into(), None),
                Err(e) => ("error", e.clone(), None),
            },
        };
        match status {
            "pass" => passed += 1,
            "fail" => failed += 1,
            _ => {}
        }
        let head = format!(
            "  {:<12} {:<20} {:>8} {:>9} {:<5} {}",
            h.section,
            h.cable,
            h.current
                .as_ref()
                .map_or("—".into(), |i| format!("{i:.0} A")),
            temp.map_or("—".into(), |t| format!("{t:.1} °C")),
            match status {
                "pass" => "pass",
                "fail" => "FAIL",
                _ => "—",
            },
            words
        );
        println!("{}", head.trim_end());
        let _ = write!(
            csv,
            "{},{},{},{},{},{},{status},",
            csv_field(&h.section),
            csv_field(&h.cable),
            h.current.as_ref().map_or(String::new(), |i| format!("{i}")),
            h.bundle,
            temp.map_or(String::new(), |t| format!("{t:.2}")),
            csv_field(&case),
        );
        match g {
            Some(g) => {
                let _ = writeln!(
                    csv,
                    "{},{},{},{:.3},{:.3},{:.3}",
                    csv_field(&g.rule),
                    g.span + 1,
                    g.measure,
                    g.clearance,
                    g.required,
                    g.margin()
                );
            }
            None => csv.push_str(",,,,,\n"),
        }
    }
    println!("{passed} pass, {failed} fail");
    if let Some(path) = &a.csv {
        write_csv(path, &csv)?;
    }
    Ok(())
}

/// The notes of a thermal run on a line: what is not on the safe side
/// (`ohl_thermal::sections::not_safe_notes`, the bimetallic models other
/// than the default among them), then the default bimetallic model of
/// the wire types that have it (hot states are where it acts).
fn thermal_notes(line: &ohl_line::Line, library: &ohl_cables::Library) -> Vec<String> {
    let mut notes = ohl_thermal::sections::not_safe_notes(line, library);
    for n in ohl_line::bimetallic_notes(line, library) {
        if !notes.contains(&n) {
            notes.push(n);
        }
    }
    notes
}

#[cfg(test)]
mod tests {

    /// The wire type is given with `--wire-type`; `--cable`, its name
    /// before the glossary (2026-10-21), is still read.
    #[test]
    fn wire_type_flag_and_its_older_name() {
        #[derive(clap::Parser)]
        struct Cli {
            #[command(subcommand)]
            command: ThermalCommand,
        }
        for flag in ["--wire-type", "--cable"] {
            let cli = <Cli as clap::Parser>::try_parse_from([
                "ohl",
                "ampacity",
                flag,
                "acsr-x",
                "--ambient",
                "a.ohlta",
                "--max-temp",
                "80",
            ])
            .unwrap();
            let ThermalCommand::Ampacity(args) = cli.command else {
                panic!("not ampacity");
            };
            assert_eq!(args.cable, "acsr-x", "{flag}");
        }
    }
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// The example line on its survey with a 5 m ground clearance, the
    /// base case "max temperature", every ambient case, CIGRE TB 601.
    fn targets(csv: Option<PathBuf>) -> TargetArgs {
        let ex = root().join("examples/synthetic-line");
        TargetArgs {
            file: ex.join("synthetic-line.ohl.json"),
            library: Some(root().join("libraries/cables")),
            structures: None,
            insulators: Some(root().join("libraries/insulators")),
            weather: Some(ex.join("weather-cases.ohlwc")),
            case: Some("max temperature".into()),
            ambient: Some(ex.join("thermal-ambient.ohlta")),
            ambient_case: Vec::new(),
            method: "cigre601".into(),
            rules: None,
            clearance: Some(5.0),
            terrain: Some(ex.join("synthetic-survey.csv")),
            lidar: None,
            pointcloud: None,
            lidar_classes: None,
            ground_role: "ground".into(),
            corridor: 60.0,
            max_edge: 50.0,
            thin: None,
            csv,
        }
    }

    fn limit_args(targets: TargetArgs) -> LimitArgs {
        LimitArgs {
            targets,
            min_temp: 0.0,
            max_temp: 120.0,
            step: 10.0,
            tolerance: 0.5,
        }
    }

    /// The CSV row of section `id`.
    fn row(csv: &str, id: &str) -> Vec<String> {
        csv.lines()
            .find(|l| l.starts_with(&format!("{id},")))
            .unwrap()
            .split(',')
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn clearance_limit_of_the_example_line() {
        let out =
            std::env::temp_dir().join(format!("ohl-thermal-limit-{}.csv", std::process::id()));
        let mut t = targets(Some(out.clone()));
        t.ambient_case = vec!["winter night".into()];
        clearance_limit(&limit_args(t)).unwrap();
        let csv = std::fs::read_to_string(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        let s2 = row(&csv, "S2");
        assert_eq!(s2[3], "limited");
        let t: f64 = s2[2].parse().unwrap();
        assert!((15.0..80.0).contains(&t), "{t}");
        assert_eq!((s2[5].as_str(), s2[6].as_str()), ("2", "vertical"));
        let amps: f64 = s2[9].parse().unwrap();
        assert!((500.0..2000.0).contains(&amps), "{amps}");
    }

    /// `clearance-check`: a small current keeps S2 cool and passing, a
    /// large one heats it past its clearance-limited temperature and fails
    /// it; the systems' operating current of the line file is the default;
    /// without any current, refused.
    #[test]
    fn clearance_check_at_a_current() {
        let dir = std::env::temp_dir().join(format!("ohl-thermal-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("check.csv");
        let check = |current: Option<f64>, file: Option<PathBuf>| {
            let mut t = targets(Some(out.clone()));
            if let Some(f) = file {
                t.file = f;
            }
            clearance_check(&CheckArgs {
                targets: t,
                current,
            })
            .map(|()| std::fs::read_to_string(&out).unwrap())
        };
        // The limit search's S2 limit, for comparison.
        let limits = dir.join("limit.csv");
        clearance_limit(&limit_args(targets(Some(limits.clone())))).unwrap();
        let limit: f64 = row(&std::fs::read_to_string(&limits).unwrap(), "S2")[2]
            .parse()
            .unwrap();
        let cool = check(Some(100.0), None).unwrap();
        let s2 = row(&cool, "S2");
        let t: f64 = s2[4].parse().unwrap();
        assert!(t < limit, "{t} {limit}");
        assert_eq!(s2[6], "pass");
        assert_eq!((s2[8].as_str(), s2[9].as_str()), ("2", "vertical"));
        let hot = check(Some(1500.0), None).unwrap();
        let s2 = row(&hot, "S2");
        let t: f64 = s2[4].parse().unwrap();
        assert!(t > limit, "{t} {limit}");
        assert_eq!(s2[6], "fail");
        let margin: f64 = s2[12].parse().unwrap();
        assert!(margin < 0.0, "{margin}");
        // No current anywhere: refused, saying how to give one.
        let e = check(None, None).unwrap_err();
        assert!(e.contains("--current"), "{e}");
        // The line's operating current is the default: the same as given.
        let mut line =
            ohl_line::Line::load(&root().join("examples/synthetic-line/synthetic-line.ohl.json"))
                .unwrap();
        line.systems[0].current_a = Some(1500.0);
        let file = dir.join("with-current.ohl.json");
        std::fs::write(&file, line.to_json()).unwrap();
        let from_line = check(None, Some(file)).unwrap();
        assert_eq!(row(&from_line, "S2"), row(&hot, "S2"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_surface_a_line_file_cannot_hold_is_reported() {
        // A ground rule naming a terrain surface, the line read from a line
        // file (no project, so no surfaces): refused, not passed.
        let rules =
            std::env::temp_dir().join(format!("ohl-thermal-surface-{}.ohlr", std::process::id()));
        std::fs::write(
            &rules,
            "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"t\"\n\n\
             [[rule]]\nname = \"Ground\"\ntarget = \"ground\"\nvertical = \"5 m\"\n\
             surface = \"design TIN\"\n",
        )
        .unwrap();
        let mut t = targets(None);
        t.weather = None;
        t.case = None;
        t.ambient = None;
        t.rules = Some(rules.clone());
        t.clearance = None;
        let r = clearance_limit(&limit_args(t));
        let _ = std::fs::remove_file(&rules);
        let e = r.unwrap_err();
        assert!(e.contains("design TIN") && e.contains("Ground"), "{e}");
    }

    #[test]
    fn ampacity_temperature_and_transient() {
        let ex = root().join("examples/synthetic-line");
        let out = std::env::temp_dir().join(format!("ohl-thermal-amp-{}.csv", std::process::id()));
        let args = |max_temp, current, from_current| AmpacityArgs {
            library: Some(root().join("libraries/cables")),
            cable: "acsr-drake-26-7".into(),
            ambient: ex.join("thermal-ambient.ohlta"),
            case: Vec::new(),
            method: "ieee738".into(),
            max_temp,
            current,
            from_current,
            duration: 600.0,
            time_step: None,
            defaults: None,
            print_every: 300.0,
            heat_capacity: None,
            csv: Some(out.clone()),
        };
        ampacity(&args(Some(80.0), None, None)).unwrap();
        let csv = std::fs::read_to_string(&out).unwrap();
        assert_eq!(csv.lines().count(), 4);
        ampacity(&args(None, Some(900.0), None)).unwrap();
        ampacity(&args(None, Some(900.0), Some(400.0))).unwrap();
        let csv = std::fs::read_to_string(&out).unwrap();
        // Three cases, 61 times each.
        assert_eq!(csv.lines().count(), 1 + 3 * 61);
        let _ = std::fs::remove_file(&out);
    }
}
