// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols thermal`: thermal ratings of a conductor (ampacity, temperature,
//! transient) and the clearance-limited temperature of a line.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use ols_core::files::shipped_library;
use ols_core::text::csv_field;
use ols_thermal::limit::{Limit, LimitOptions, Targets};
use ols_thermal::{Ambient, AmbientCases, Conductor, HeatBalance, Method};

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
}

/// Arguments of `ols thermal ampacity`.
#[derive(Args)]
pub struct AmpacityArgs {
    /// Wire library: .olsw file or directory (default: the shipped
    /// libraries/cables).
    #[arg(long)]
    library: Option<PathBuf>,
    /// Cable id.
    #[arg(long)]
    cable: String,
    /// Ambient cases (.olsta).
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
    /// Default values (.olsdef, Criteria › Default values exported):
    /// emissivity, absorptivity, roughness, albedo and time step where the
    /// cable or the case gives none (default: the program's).
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

/// Arguments of `ols thermal clearance-limit`.
#[derive(Args)]
pub struct LimitArgs {
    /// Line file (.ols.json) or project file (.ols).
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
    /// Weather case file (.olswc): the base case (--case) whose temperature
    /// is varied; default still air without ice.
    #[arg(long)]
    weather: Option<PathBuf>,
    /// The base weather case, by name.
    #[arg(long, requires = "weather")]
    case: Option<String>,
    /// Ambient cases (.olsta) for the ampacities at the limits.
    #[arg(long)]
    ambient: Option<PathBuf>,
    /// Ambient case, by name (repeatable; default: all).
    #[arg(long)]
    ambient_case: Vec<String>,
    /// Method: ieee738, cigre601 or cigre207.
    #[arg(long, default_value = "ieee738")]
    method: String,
    /// Clearance rules (.olsr): their ground and point rules are checked.
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Required vertical ground clearance, m (adds a ground rule).
    #[arg(long)]
    clearance: Option<f64>,
    /// Survey points (CSV / XYZ) for the ground surface.
    #[arg(long)]
    terrain: Option<PathBuf>,
    /// LiDAR tiles (LAS/LAZ file or directory).
    #[arg(long, conflicts_with = "terrain")]
    lidar: Option<PathBuf>,
    /// Point cloud stored in a project file (`FILE.ols` or `FILE.ols#name`).
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
    /// Longest triangle side of the LiDAR ground surface, m.
    #[arg(long, default_value_t = 50.0)]
    max_edge: f64,
    /// Thin the LiDAR ground points: lowest point per cell of this size, m.
    #[arg(long)]
    thin: Option<f64>,
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
    /// Write one row per section as CSV.
    #[arg(long)]
    csv: Option<PathBuf>,
}

/// Runs a thermal command.
pub fn run(command: ThermalCommand) -> Result<(), String> {
    match command {
        ThermalCommand::Ampacity(a) => ampacity(&a),
        ThermalCommand::ClearanceLimit(a) => clearance_limit(&a),
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
    let library = ols_cables::Library::load(&library_path).map_err(|e| e.to_string())?;
    let cable = library
        .get(&a.cable)
        .ok_or_else(|| format!("cable {} is not in {}", a.cable, library_path.display()))?;
    let defaults = match &a.defaults {
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| format!("{}: {e}", p.display()))
            .and_then(|t| {
                ols_line::Defaults::parse(&t).map_err(|e| format!("{}: {e}", p.display()))
            })?,
        None => ols_line::Defaults::default(),
    };
    let time_step = a.time_step.unwrap_or(defaults.thermal_time_step);
    let mut conductor = Conductor::from_cable(cable)
        .map_err(|e| e.to_string())?
        .with_defaults(&defaults);
    if let Some(mc) = a.heat_capacity {
        conductor.heat_capacity = vec![(mc, 0.0)];
    }
    let cases = AmbientCases::load(&a.ambient)?.select(&a.case)?;
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
                match ols_thermal::ampacity(method, &conductor, case, t) {
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
                let b = ols_thermal::temperature(
                    method,
                    &conductor,
                    case,
                    i,
                    &ols_thermal::default_steady_budget(),
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
                let budget = ols_thermal::default_steady_budget();
                let start = ols_thermal::temperature(method, &conductor, case, i0, &budget)
                    .map_err(|e| format!("{}: {e}", case.name))?;
                let end = ols_thermal::temperature(method, &conductor, case, i, &budget)
                    .map_err(|e| format!("{}: {e}", case.name))?;
                let steps = ols_thermal::transient(
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

fn clearance_limit(a: &LimitArgs) -> Result<(), String> {
    let method = Method::parse(&a.method)?;
    let options = LimitOptions {
        min_temperature: a.min_temp,
        max_temperature: a.max_temp,
        scan_step: a.step,
        tolerance: a.tolerance,
    };
    options.validate()?;
    let mut line = crate::project::load_line(&a.file)?;
    let insulators = a
        .insulators
        .clone()
        .unwrap_or_else(|| shipped_library("insulators"));
    let structures = a
        .structures
        .clone()
        .unwrap_or_else(|| ols_core::files::dir_of(&a.file).join("Structures"));
    if line.structures.iter().any(|s| s.structure_type.is_some()) {
        let types =
            ols_structures::StructureLibrary::load_dir(&structures).map_err(|e| e.to_string())?;
        let ins =
            ols_structures::InsulatorLibrary::load_dir(&insulators).map_err(|e| e.to_string())?;
        line = line.resolve(&types, &ins).map_err(|e| e.to_string())?;
    }
    let library = ols_cables::Library::load(
        &a.library
            .clone()
            .unwrap_or_else(|| shipped_library("cables")),
    )
    .map_err(|e| e.to_string())?;
    let base = match &a.weather {
        Some(path) => {
            let set = ols_weather::WeatherCases::load(path)?;
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
        None => ols_weather::WeatherCase::still(
            "thermal limit",
            ols_core::units::Temperature::from_celsius(a.min_temp),
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
        },
    )?;
    let mut feed = |_: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        prepared.feed_points(visit);
        Ok(())
    };
    let mut targets = Targets {
        ground: prepared.ground.as_ref(),
        point_classes: prepared.point_classes(),
        points: Some(&mut feed),
    };
    let started = std::time::Instant::now();
    let budget = ols_thermal::limit::default_budget(&options, line.sections.len());
    let limits = ols_thermal::limit::clearance_limits(
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
            format!(
                "{} span {} {} {:.2} m < {:.2} m at {:.1} °C",
                g.rule,
                g.span + 1,
                g.measure,
                g.clearance,
                g.required,
                g.temperature
            )
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
                (Some(t), Some(Ok(cond))) => ols_thermal::ampacity(method, cond, c, t)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn clearance_limit_of_the_example_line() {
        let ex = root().join("examples/synthetic-line");
        let out =
            std::env::temp_dir().join(format!("ols-thermal-limit-{}.csv", std::process::id()));
        clearance_limit(&LimitArgs {
            file: ex.join("synthetic-line.ols.json"),
            library: Some(root().join("libraries/cables")),
            structures: None,
            insulators: Some(root().join("libraries/insulators")),
            weather: Some(ex.join("weather-cases.olswc")),
            case: Some("max temperature".into()),
            ambient: Some(ex.join("thermal-ambient.olsta")),
            ambient_case: vec!["winter night".into()],
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
            min_temp: 0.0,
            max_temp: 120.0,
            step: 10.0,
            tolerance: 0.5,
            csv: Some(out.clone()),
        })
        .unwrap();
        let csv = std::fs::read_to_string(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        let s2: Vec<&str> = csv
            .lines()
            .find(|l| l.starts_with("S2,"))
            .unwrap()
            .split(',')
            .collect();
        assert_eq!(s2[3], "limited");
        let t: f64 = s2[2].parse().unwrap();
        assert!((15.0..80.0).contains(&t), "{t}");
        assert_eq!((s2[5], s2[6]), ("2", "vertical"));
        let amps: f64 = s2[9].parse().unwrap();
        assert!((500.0..2000.0).contains(&amps), "{amps}");
    }

    #[test]
    fn ampacity_temperature_and_transient() {
        let ex = root().join("examples/synthetic-line");
        let out = std::env::temp_dir().join(format!("ols-thermal-amp-{}.csv", std::process::id()));
        let args = |max_temp, current, from_current| AmpacityArgs {
            library: Some(root().join("libraries/cables")),
            cable: "acsr-drake-26-7".into(),
            ambient: ex.join("thermal-ambient.olsta"),
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
