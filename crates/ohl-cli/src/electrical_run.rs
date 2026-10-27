// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl electrical …`: electrical calculations of a line (roadmap O18) —
//! `parameters`: its line parameters per circuit, stretch and span;
//! `magnetic`: the magnetic flux density of its currents in profiles across
//! it, grids and given points ([`ohl_electrical`]).

use std::path::PathBuf;

use clap::{Args, Subcommand};
use ohl_core::units::Temperature;
use ohl_line::electrical::{EarthReturn, MeanHeight};

use crate::analysis_run::{ModelArgs, load};

/// `ohl electrical …`.
#[derive(Subcommand)]
pub enum ElectricalCommand {
    /// Line parameters: series impedance with earth return (earth wires
    /// reduced out), capacitance and susceptance, per circuit and its
    /// stretches, sequence values, coupling between circuits; and the
    /// cross-sections they come from.
    Parameters(Box<ParametersArgs>),
    /// Magnetic flux density (µT) of the line's currents: profiles across
    /// the line (every span's lowest point by default, ±50 m, 1 m above the
    /// ground), grids, given points; 3D from the wires as solved
    /// (Biot–Savart, finite segments) or 2D; an optional limit with the
    /// margin and the limit line's width.
    Magnetic(Box<MagneticArgs>),
}

/// `ohl electrical magnetic`.
#[derive(Args)]
pub struct MagneticArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    /// Weather case file (.ohlwc) holding the case the wires hang in.
    #[arg(long, requires = "case")]
    pub weather: Option<PathBuf>,
    /// The weather case the wires hang in (default: still air at
    /// --temperature).
    #[arg(long, requires = "weather")]
    pub case: Option<String>,
    /// Conductor temperature without --case, °C.
    #[arg(long, default_value_t = 40.0)]
    pub temperature: f64,
    /// Terrain file for the ground (default: straight between the
    /// structures' feet, level across).
    #[arg(long, conflicts_with = "surface")]
    pub terrain: Option<PathBuf>,
    /// A terrain surface of the project file for the ground.
    #[arg(long)]
    pub surface: Option<String>,
    /// A circuit's current: CIRCUIT=AMPS, or CIRCUIT=AMPS:reverse for a
    /// load flow against the line's direction (CIRCUIT: its name or its
    /// system's id, `*` every circuit). Repeatable.
    #[arg(long, value_name = "CIRCUIT=A[:reverse]")]
    pub current: Vec<String>,
    /// The circuits' thermal rating as their currents: ambient case file
    /// (.ohlta) with --ambient-case and --max-temp; the weakest section of
    /// a circuit governs. --current overrides it per circuit.
    #[arg(long, requires_all = ["ambient_case", "max_temp"])]
    pub thermal: Option<PathBuf>,
    /// The ambient case of --thermal, by name.
    #[arg(long)]
    pub ambient_case: Option<String>,
    /// The conductor temperature of the thermal rating, °C.
    #[arg(long)]
    pub max_temp: Option<f64>,
    /// The thermal rating's method: ieee738, cigre601 or cigre207.
    #[arg(long, default_value = "ieee738")]
    pub method: String,
    /// With --thermal: the load flows against the line's direction in this
    /// circuit. Repeatable.
    #[arg(long)]
    pub reverse: Vec<String>,
    /// Earth wire currents induced by the phases (an infinitely long earth
    /// wire earthed everywhere).
    #[arg(long)]
    pub earth_wires: bool,
    /// A profile: FROM:TO (a span's structures) at its lowest point, or
    /// FROM:TO@FRACTION (0–1 of the span from the first along the line).
    /// Repeatable; default every span at its lowest point.
    #[arg(long, value_name = "FROM:TO[@FRACTION]")]
    pub profile: Vec<String>,
    /// The profiles reach this far to either side, m.
    #[arg(long, default_value_t = 50.0)]
    pub width: f64,
    /// Height above the ground of the profiles, the grids and the points
    /// without one, m.
    #[arg(long, default_value_t = 1.0)]
    pub height: f64,
    /// Between profile and grid points, m.
    #[arg(long, default_value_t = 1.0)]
    pub step: f64,
    /// A grid on a horizontal plane over span FROM:TO. Repeatable.
    #[arg(long, value_name = "FROM:TO")]
    pub grid: Vec<String>,
    /// Points from a CSV file: a header with x, y and z (elevation) or
    /// height (above the ground), optionally name.
    #[arg(long)]
    pub points: Option<PathBuf>,
    /// A limit, µT: the margin per point and the limit line's width per
    /// profile (no limit is built in).
    #[arg(long)]
    pub limit: Option<f64>,
    /// The 2D model (infinite straight parallel conductors) instead of 3D.
    #[arg(long)]
    pub two_d: bool,
    /// With --two-d: the wires at the span's mean height (the line
    /// parameters' rule) instead of where they cross the station.
    #[arg(long, requires = "two_d")]
    pub mean_heights: bool,
    /// Print every profile point (default: a summary per profile).
    #[arg(long)]
    pub all_points: bool,
    /// Write every point as CSV here.
    #[arg(long)]
    pub csv: Option<PathBuf>,
}

/// `ohl electrical parameters`.
#[derive(Args)]
pub struct ParametersArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    /// Weather case file (.ohlwc) holding the case the wires hang in.
    #[arg(long, requires = "case")]
    pub weather: Option<PathBuf>,
    /// The weather case the wires hang in (default: still air at
    /// --temperature).
    #[arg(long, requires = "weather")]
    pub case: Option<String>,
    /// Conductor temperature without --case, °C (also the resistances').
    #[arg(long, default_value_t = 20.0)]
    pub temperature: f64,
    /// The wires' height in a span: solved (the solved wire's average,
    /// default) or mean (attachment height minus 2/3 of the sag); default
    /// the line's setting.
    #[arg(long)]
    pub height: Option<String>,
    /// The line's soil resistivity, Ω·m (default: the line's, else 100 Ω·m
    /// assumed); spans at structures with a measured one take theirs.
    #[arg(long)]
    pub soil: Option<f64>,
    /// Earth return: carson_series, complex_depth or carson_first_terms
    /// (default: the line's setting).
    #[arg(long)]
    pub earth: Option<String>,
    /// Terrain file for the ground under the spans (default: straight
    /// between the structures' feet).
    #[arg(long, conflicts_with = "surface")]
    pub terrain: Option<PathBuf>,
    /// A terrain surface of the project file (`ohl surface list`) for the
    /// ground under the spans.
    #[arg(long)]
    pub surface: Option<String>,
    /// Only the stretches holding this section (repeatable).
    #[arg(long)]
    pub section: Vec<String>,
    /// Also print each circuit's phase matrices per km.
    #[arg(long)]
    pub matrices: bool,
    /// Also print every span's cross-section.
    #[arg(long)]
    pub cross_sections: bool,
    /// Write the table as CSV here.
    #[arg(long)]
    pub csv: Option<PathBuf>,
}

/// Runs `ohl electrical …`.
pub fn run(command: ElectricalCommand) -> Result<(), String> {
    match command {
        ElectricalCommand::Parameters(a) => parameters(&a),
        ElectricalCommand::Magnetic(a) => magnetic(&a),
    }
}

fn parameters(a: &ParametersArgs) -> Result<(), String> {
    let (mut line, library) = load(&a.model)?;
    if let Some(h) = &a.height {
        line.electrical.mean_height = MeanHeight::parse(h)
            .ok_or_else(|| format!("--height {h}: solved or mean (two_thirds_sag)"))?;
    }
    if let Some(e) = &a.earth {
        line.electrical.earth_return = EarthReturn::parse(e).ok_or_else(|| {
            format!("--earth {e}: carson_series, complex_depth or carson_first_terms")
        })?;
    }
    if let Some(r) = a.soil {
        line.electrical.soil_resistivity_ohm_m = Some(r);
        line.electrical.validate()?;
    }
    for s in &a.section {
        if !line.sections.iter().any(|x| &x.id == s) {
            return Err(format!("--section {s}: no such section"));
        }
    }
    let case = match (&a.weather, &a.case) {
        (Some(path), Some(name)) => {
            let set = ohl_weather::WeatherCases::load(path)?;
            line.set_weather_cases(&set.cases);
            set.select(std::slice::from_ref(name), &[])?
                .into_iter()
                .next()
                .ok_or_else(|| format!("no weather case {name}"))?
        }
        _ => ohl_weather::WeatherCase::still("still air", Temperature::from_celsius(a.temperature)),
    };
    let mut shapes = Vec::new();
    for (section, r) in line
        .sections
        .iter()
        .zip(ohl_line::solve::solve_line(&line, &library, &case))
    {
        match r {
            Ok(s) => shapes.push(s),
            Err(e) => println!("  section {} not solved: {e}", section.id),
        }
    }
    let terrain = match (&a.terrain, &a.surface) {
        (Some(path), _) => Some(ohl_terrain::Terrain::load(path).map_err(|e| e.to_string())?),
        (None, Some(name)) => {
            let session = ohl_session::Session::open(&a.model.file).map_err(|e| e.to_string())?;
            let (points, triangles) = session.surface(name).map_err(|e| e.to_string())?;
            Some(ohl_terrain::Terrain::from_mesh(points, triangles).map_err(|e| e.to_string())?)
        }
        (None, None) => None,
    };
    let height = |x: f64, y: f64| terrain.as_ref().and_then(|t| t.height(x, y));
    let ground: Option<&dyn Fn(f64, f64) -> Option<f64>> = terrain
        .as_ref()
        .map(|_| &height as &dyn Fn(f64, f64) -> Option<f64>);
    let p = ohl_electrical::run(&line, &library, &shapes, case.temperature, ground);
    let (rho, assumed) = line.electrical.soil_resistivity();
    println!(
        "Line parameters of {} in \"{}\" ({:.1} °C)",
        line.name,
        case.name,
        case.temperature.celsius()
    );
    println!(
        "earth return: {}; soil (the line's) {rho} Ω·m{}; height: {}",
        p.method.describe(),
        if assumed { " (assumed)" } else { "" },
        line.electrical.mean_height.describe()
    );
    for n in &p.notes {
        println!("  note: {n}");
    }
    let rows = ohl_electrical::report::rows(&p, &a.section);
    println!(
        "{:<9} {:<18} {:<14} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "",
        "circuit",
        "from–to",
        "km",
        "R1 Ω/km",
        "X1 Ω/km",
        "B1 µS/km",
        "R0 Ω/km",
        "X0 Ω/km",
        "B0 µS/km"
    );
    let num = |x: Option<f64>, d: usize| match x {
        Some(v) => format!("{v:.d$}"),
        None => "—".into(),
    };
    for r in &rows {
        let span = if r.from.is_empty() {
            String::new()
        } else {
            format!("{}–{}", r.from, r.to)
        };
        println!(
            "{:<9} {:<18} {:<14} {:>8.3} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
            r.kind,
            r.circuit,
            span,
            r.length_km,
            num(r.positive.map(|x| x[0]), 4),
            num(r.positive.map(|x| x[1]), 4),
            num(r.positive.map(|x| x[2]), 3),
            num(r.zero.map(|x| x[0]), 4),
            num(r.zero.map(|x| x[1]), 4),
            num(r.zero.map(|x| x[2]), 3),
        );
        if r.kind == "stretch" {
            println!("{:<9}   sections {}", "", r.sections.join(", "));
        }
        if r.kind == "circuit"
            && let Some(((z, b), u)) = r.totals().zip(r.unbalance)
        {
            println!(
                "{:<9}   total: Z1 = {:.3} + j{:.3} Ω, Z0 = {:.3} + j{:.3} Ω, B1 = {:.2} µS, B0 = {:.2} µS; as built the sequences couple by {:.1} % of Z1{}",
                "",
                z[0],
                z[1],
                z[2],
                z[3],
                b[0],
                b[1],
                100.0 * u,
                if u > 0.01 {
                    " (not transposed: Z1, Z0 are those of the ideally transposed line)"
                } else {
                    ""
                }
            );
        }
    }
    if a.matrices {
        for c in &p.circuits {
            let (z, cap) = c.total.per_km();
            println!(
                "{} phase matrices per km ({}), Z Ω/km:",
                c.key.label(),
                c.phases.join(", ")
            );
            for i in 0..z.nrows() {
                let row: Vec<String> = (0..z.ncols())
                    .map(|j| format!("{:>9.5}{:+.5}j", z[(i, j)].re, z[(i, j)].im))
                    .collect();
                println!("  {}", row.join("  "));
            }
            println!("  C nF/km:");
            for i in 0..cap.nrows() {
                let row: Vec<String> = (0..cap.ncols())
                    .map(|j| format!("{:>9.4}", cap[(i, j)] * 1e9))
                    .collect();
                println!("  {}", row.join("  "));
            }
        }
    }
    if a.cross_sections {
        for (span, cs, _) in &p.spans {
            println!(
                "cross-section {}–{} ({:.1} m; ground {:.2} m):",
                span.from, span.to, cs.station.span_length, cs.ground.elevation
            );
            for w in &cs.wires {
                let what = match &w.phase {
                    Some(ph) => format!("{} {} {}", ph.circuit, ph.system, ph.label),
                    None => "earthed".into(),
                };
                println!(
                    "  {:<10} {:<24} offset {:>8.3} m  height {:>7.3} m  bundle {}  {}",
                    w.section, what, w.offset, w.height, w.bundle.count, w.wire.cable
                );
            }
        }
    }
    if let Some(path) = &a.csv {
        std::fs::write(path, ohl_electrical::report::csv(&p, &a.section))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    Ok(())
}

/// `FROM:TO`.
fn span_arg(text: &str, what: &str) -> Result<(String, String), String> {
    let (a, b) = text
        .split_once(':')
        .ok_or_else(|| format!("{what} {text}: FROM:TO"))?;
    Ok((a.trim().to_owned(), b.trim().to_owned()))
}

/// `--current CIRCUIT=A[:reverse]`.
fn current_arg(text: &str) -> Result<ohl_electrical::magnetic::CircuitCurrent, String> {
    let (name, rest) = text
        .split_once('=')
        .ok_or_else(|| format!("--current {text}: CIRCUIT=AMPS[:reverse]"))?;
    let (amps, dir) = match rest.split_once(':') {
        Some((a, d)) => (a, Some(d.trim())),
        None => (rest, None),
    };
    let reverse = match dir {
        None | Some("forward") | Some("+") => false,
        Some("reverse") | Some("-") => true,
        Some(d) => {
            return Err(format!(
                "--current {text}: direction {d}: reverse or forward"
            ));
        }
    };
    Ok(ohl_electrical::magnetic::CircuitCurrent {
        circuit: name.trim().to_owned(),
        amps: amps
            .trim()
            .parse()
            .map_err(|_| format!("--current {text}: {amps} is not a number of amperes"))?,
        reverse,
    })
}

fn magnetic(a: &MagneticArgs) -> Result<(), String> {
    use ohl_electrical::magnetic::{self, Heights2d, Model, Settings, StationSpec};
    let (mut line, library) = load(&a.model)?;
    let case = match (&a.weather, &a.case) {
        (Some(path), Some(name)) => {
            let set = ohl_weather::WeatherCases::load(path)?;
            line.set_weather_cases(&set.cases);
            set.select(std::slice::from_ref(name), &[])?
                .into_iter()
                .next()
                .ok_or_else(|| format!("no weather case {name}"))?
        }
        _ => ohl_weather::WeatherCase::still("still air", Temperature::from_celsius(a.temperature)),
    };
    let mut currents = Vec::new();
    let mut sources: Vec<(String, String)> = Vec::new();
    if let Some(path) = &a.thermal {
        let method = ohl_thermal::Method::parse(&a.method)?;
        let name = a.ambient_case.clone().unwrap_or_default();
        let ambient = ohl_thermal::ambient::AmbientCases::load(path)?
            .select(std::slice::from_ref(&name))?
            .into_iter()
            .next()
            .ok_or_else(|| format!("no ambient case {name}"))?;
        let t = a.max_temp.unwrap_or_default();
        for mut c in magnetic::thermal_currents(
            &line,
            &library,
            method,
            &ambient,
            t,
            &ohl_line::Defaults::default(),
        )? {
            c.reverse = a.reverse.contains(&c.circuit);
            sources.push((
                c.circuit.clone(),
                format!(
                    "thermal rating, {} in \"{}\" at {t} °C",
                    method.title(),
                    ambient.name
                ),
            ));
            currents.push(c);
        }
    }
    for c in &a.current {
        let c = current_arg(c)?;
        sources.push((c.circuit.clone(), "given".into()));
        currents.push(c);
    }
    if currents.is_empty() {
        return Err("give the currents: --current CIRCUIT=A (or --thermal)".into());
    }
    let mut stations = Vec::new();
    for p in &a.profile {
        let (span, fraction) = match p.split_once('@') {
            Some((s, f)) => (
                s,
                Some(
                    f.trim()
                        .parse::<f64>()
                        .map_err(|_| format!("--profile {p}: {f} is not a fraction 0–1"))?,
                ),
            ),
            None => (p.as_str(), None),
        };
        let (from, to) = span_arg(span, "--profile")?;
        stations.push(StationSpec { from, to, fraction });
    }
    let grids = a
        .grid
        .iter()
        .map(|g| span_arg(g, "--grid"))
        .collect::<Result<Vec<_>, _>>()?;
    let points = match &a.points {
        Some(p) => magnetic::parse_points(
            &std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        )
        .map_err(|e| format!("{}: {e}", p.display()))?,
        None => Vec::new(),
    };
    let settings = Settings {
        model: if a.two_d { Model::TwoD } else { Model::ThreeD },
        heights_2d: if a.mean_heights {
            Heights2d::Mean
        } else {
            Heights2d::Station
        },
        currents,
        earth_wires: a.earth_wires,
        height: a.height,
        width: a.width,
        step: a.step,
        stations,
        grids,
        points,
        limit_ut: a.limit,
    };
    let mut shapes = Vec::new();
    for (section, r) in line
        .sections
        .iter()
        .zip(ohl_line::solve::solve_line(&line, &library, &case))
    {
        match r {
            Ok(s) => shapes.push(s),
            Err(e) => println!("  section {} not solved: {e}", section.id),
        }
    }
    let terrain = match (&a.terrain, &a.surface) {
        (Some(path), _) => Some(ohl_terrain::Terrain::load(path).map_err(|e| e.to_string())?),
        (None, Some(name)) => {
            let session = ohl_session::Session::open(&a.model.file).map_err(|e| e.to_string())?;
            let (points, triangles) = session.surface(name).map_err(|e| e.to_string())?;
            Some(ohl_terrain::Terrain::from_mesh(points, triangles).map_err(|e| e.to_string())?)
        }
        (None, None) => None,
    };
    let height = |x: f64, y: f64| terrain.as_ref().and_then(|t| t.height(x, y));
    let ground: Option<&dyn Fn(f64, f64) -> Option<f64>> = terrain
        .as_ref()
        .map(|_| &height as &dyn Fn(f64, f64) -> Option<f64>);
    let src = ohl_electrical::Sources {
        line: &line,
        library: &library,
        shapes: &shapes,
        temperature: case.temperature,
        ground,
    };
    let m = magnetic::run(&src, &settings, None)?;
    println!(
        "Magnetic flux density of {} in \"{}\" ({:.1} °C): {}, {} m above the ground{}",
        line.name,
        case.name,
        case.temperature.celsius(),
        m.model.describe(),
        m.height,
        match m.limit_ut {
            Some(l) => format!(", limit {l} µT"),
            None => String::new(),
        }
    );
    for c in &m.currents {
        let source = sources
            .iter()
            .rev()
            .find(|(n, _)| n == &c.circuit || n == &c.system || n == "*")
            .map_or("none given", |s| s.1.as_str());
        println!(
            "  circuit {:<16} {:>8.1} A  {} Hz  {}  ({source})",
            c.circuit,
            c.amps,
            c.frequency_hz,
            if c.reverse {
                "against the line"
            } else {
                "along the line"
            }
        );
    }
    for e in &m.earth_wires {
        println!(
            "  earth wire {} {}–{}: {:.1} A at {:.1}° ({} Hz)",
            e.section, e.from, e.to, e.amps, e.angle_deg, e.frequency_hz
        );
    }
    for n in &m.notes {
        println!("  note: {n}");
    }
    println!(
        "{:<16} {:>9} {:>9} {:>9} {:>9} {:>20}",
        "profile", "station m", "along m", "max µT", "at m", "limit line m"
    );
    for p in &m.profiles {
        let limit = match p.limit {
            Some(l) => format!(
                "{:.1} … {:.1}{}",
                l.left,
                l.right,
                if l.beyond { " (beyond)" } else { "" }
            ),
            None if m.limit_ut.is_some() => "below the limit".into(),
            None => String::new(),
        };
        println!(
            "{:<16} {:>9.1} {:>9.1} {:>9.3} {:>9.1} {:>20}",
            format!("{}–{}", p.from, p.to),
            p.at,
            p.along_line,
            p.max_ut,
            p.max_offset,
            limit
        );
        for w in &p.warnings {
            println!("    warning: {w}");
        }
        if a.all_points {
            for q in &p.points {
                println!(
                    "    {:>8.1} m  {:>9.4} µT  (largest over the cycle {:.4}){}",
                    q.offset,
                    q.b_ut,
                    q.major_ut,
                    q.margin_ut
                        .map_or(String::new(), |x| format!("  margin {x:.4}"))
                );
            }
        }
    }
    for g in &m.grids {
        let max = g.points.iter().map(|p| p.b_ut).fold(0.0, f64::max);
        println!(
            "grid {}–{}: {} × {} points, max {max:.3} µT",
            g.from, g.to, g.rows, g.columns
        );
    }
    if !m.points.is_empty() {
        println!(
            "{:<16} {:>12} {:>12} {:>8} {:>8} {:>9} {:>9}",
            "point", "x", "y", "z", "h m", "B µT", "margin"
        );
        for p in &m.points {
            println!(
                "{:<16} {:>12.2} {:>12.2} {:>8.2} {:>8.2} {:>9.4} {:>9}",
                p.name,
                p.x,
                p.y,
                p.z,
                p.height,
                p.b_ut,
                p.margin_ut.map_or("—".into(), |x| format!("{x:.4}"))
            );
        }
    }
    if let Some(path) = &a.csv {
        std::fs::write(path, magnetic::csv(&m)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    Ok(())
}
