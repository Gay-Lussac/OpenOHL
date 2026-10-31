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
    /// Electric field strength (kV/m) of the line's voltages: profiles
    /// across the line (every span's lowest point by default, ±50 m, 1 m
    /// above the ground), given points, cuts, volumes; 3D from the wires as
    /// solved with their images in the ground under each point, or 2D; an
    /// optional limit with the margin.
    Electric(Box<ElectricArgs>),
}

/// `ohl electrical electric`.
#[derive(Args)]
pub struct ElectricArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    /// Weather case file (.ohlwc) holding the case the wires hang in.
    #[arg(long, requires = "case")]
    pub weather: Option<PathBuf>,
    /// The weather case the wires hang in (default: still air at
    /// --temperature; an ordinance may ask the largest sag).
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
    /// A circuit's voltage between phases: CIRCUIT=KV (CIRCUIT: its name or
    /// its system's id, `*` every circuit). Repeatable; default: the
    /// system's nominal voltage, said.
    #[arg(long, value_name = "CIRCUIT=KV")]
    pub voltage: Vec<String>,
    /// 2D: the cross-section's wires as infinite line charges.
    #[arg(long)]
    pub two_d: bool,
    /// A profile: FROM:TO or FROM:TO@FRACTION. Repeatable; default every
    /// span at its lowest point.
    #[arg(long, value_name = "FROM:TO[@FRACTION]")]
    pub profile: Vec<String>,
    /// The profiles reach this far to either side, m.
    #[arg(long, default_value_t = 50.0)]
    pub width: f64,
    /// Height above the ground of the profiles and points, m.
    #[arg(long, default_value_t = 1.0)]
    pub height: f64,
    /// Between profile points, m.
    #[arg(long, default_value_t = 1.0)]
    pub step: f64,
    /// The ground plane under a point is fitted over this distance either
    /// side, m.
    #[arg(long, default_value_t = 10.0)]
    pub ground_radius: f64,
    /// The ground a plane under each point (quick; not conservative on
    /// bumps) instead of the tiled conducting floor around each profile,
    /// group of points and cut (the default: some seconds each).
    #[arg(long)]
    pub plane: bool,
    /// The tiled floor's fine tiles, m (near the profile).
    #[arg(long, default_value_t = 2.0)]
    pub tile: f64,
    /// The tiled floor's coarse tiles, m (a whole multiple of --tile).
    #[arg(long, default_value_t = 8.0)]
    pub coarse_tile: f64,
    /// The tiled floor reaches this far every way from the station, m.
    #[arg(long, default_value_t = 200.0)]
    pub floor_reach: f64,
    /// The tiles under the cuts across the line, m (coarser than the
    /// profiles' by default).
    #[arg(long, default_value_t = 8.0)]
    pub cut_tile: f64,
    /// The tiles under the volumes, m (8 m: some 3 % of the largest value
    /// on even ground; 4 m: 0.2 %, some 40 times as long).
    #[arg(long, default_value_t = 8.0)]
    pub volume_tile: f64,
    /// The ground from the project's point cloud even when the project has
    /// a surface of the checks: its ground classes within the floor's
    /// reach of the spans computed, the lowest point per metre
    /// triangulated. (Without --terrain or --surface the ground is by
    /// default the project's surface of the checks, else its point
    /// cloud's, as in OHL-Design.)
    #[arg(long, conflicts_with_all = ["terrain", "surface"])]
    pub lidar_ground: bool,
    /// Leave the jumpers out (by default they carry their phase's charge).
    #[arg(long)]
    pub no_jumpers: bool,
    /// The longest straight piece of a wire, m (default: the solver's).
    #[arg(long)]
    pub piece_length: Option<f64>,
    /// A vertical cut across span FROM:TO at its lowest point or a fraction.
    /// Repeatable.
    #[arg(long, value_name = "FROM:TO[@FRACTION]")]
    pub cut: Vec<String>,
    /// Between a cut's points, m.
    #[arg(long, default_value_t = 0.5)]
    pub cut_step: f64,
    /// A volume over the spans from FROM to TO along the line. Repeatable.
    #[arg(long, value_name = "FROM:TO")]
    pub volume: Vec<String>,
    /// The volumes' fine lattice step, m.
    #[arg(long, default_value_t = 1.0)]
    pub volume_step: f64,
    /// Cuts and volumes reach this far over the highest wire, m.
    #[arg(long, default_value_t = 10.0)]
    pub above: f64,
    /// A level for the isolines and isosurfaces, kV/m. Repeatable.
    #[arg(long)]
    pub level: Vec<f64>,
    /// Points to evaluate: a CSV with x, y and z or height.
    #[arg(long)]
    pub points: Option<PathBuf>,
    /// A limit, kV/m: the margin per point (none is built in).
    #[arg(long)]
    pub limit: Option<f64>,
    /// Print every profile point.
    #[arg(long)]
    pub all_points: bool,
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
    /// A current at a circuit's end point: CIRCUIT@STRUCTURE=A (into the
    /// line; negative: out of it) or CIRCUIT@STRUCTURE=rest (the others'
    /// sum, out; one end at most). Every section's current then follows
    /// by Kirchhoff's current law. Repeatable; --ends lists the end points.
    #[arg(long, value_name = "CIRCUIT@STRUCTURE=A")]
    pub end: Vec<String>,
    /// List every circuit's end points and stop.
    #[arg(long)]
    pub ends: bool,
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
    /// With --earth-wires: the earth wires as a network over the line,
    /// each structure leading current into the ground through its
    /// earthing resistance (its own, the line's, else 10 Ω assumed), the
    /// structures' currents in the field (3D).
    #[arg(long, requires = "earth_wires")]
    pub towers: bool,
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
    /// Between grid points, m (default: --step).
    #[arg(long)]
    pub grid_step: Option<f64>,
    /// The longest straight piece of a wire, m (3D; default: the solver's
    /// pieces, 64 per span or part of a span). Shorter pieces lie closer
    /// to the wire and take longer.
    #[arg(long)]
    pub piece_length: Option<f64>,
    /// Leave the jumpers out (by default they carry their phase's
    /// current).
    #[arg(long)]
    pub no_jumpers: bool,
    /// Cuts and volumes take the wires within this plan distance of them
    /// only, m (default: the whole line); the difference to the whole
    /// line's field is computed at some of their points and said.
    #[arg(long)]
    pub modelled_within: Option<f64>,
    /// A vertical cut across span FROM:TO at its lowest point or at a
    /// fraction of it (3D): --width to either side, from the ground to
    /// --above over the highest wire. Repeatable.
    #[arg(long, value_name = "FROM:TO[@FRACTION]")]
    pub cut: Vec<String>,
    /// Between a cut's points, m.
    #[arg(long, default_value_t = 0.5)]
    pub cut_step: f64,
    /// A volume over span FROM:TO (3D): the isosurfaces of the --level
    /// values, --width to either side, from the ground to --above over
    /// the highest wire. Repeatable.
    #[arg(long, value_name = "FROM:TO")]
    pub volume: Vec<String>,
    /// The volumes' fine lattice step, m.
    #[arg(long, default_value_t = 1.0)]
    pub volume_step: f64,
    /// Cuts and volumes reach this far over the highest wire, m.
    #[arg(long, default_value_t = 10.0)]
    pub above: f64,
    /// A level for the isolines and isosurfaces, µT. Repeatable.
    #[arg(long)]
    pub level: Vec<f64>,
    /// Write the volumes' isosurfaces to this Wavefront OBJ file.
    #[arg(long)]
    pub obj: Option<PathBuf>,
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
        ElectricalCommand::Electric(a) => electric(&a),
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
    let ground: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> = terrain
        .as_ref()
        .map(|_| &height as &(dyn Fn(f64, f64) -> Option<f64> + Sync));
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
/// The ground of the fields when no --terrain or --surface is given
/// (owner, 2026-10-31: the same as OHL-Design's; across a valley the
/// straight ground between the structures' feet passed close under the
/// wires): a project's surface of the checks (its setting
/// `app.check_surface`), else its point cloud's ground classes within
/// `reach` of every span (the whole line, as OHL-Design: the spans not
/// computed carry charges too, from their own cross-sections; some
/// seconds), the lowest point per metre triangulated; else none — straight between the structures' feet and
/// level across, said as a warning. `lidar`: the point cloud even with a
/// surface of the checks.
fn project_ground(
    file: &std::path::Path,
    line: &ohl_line::Line,
    reach: f64,
    lidar: bool,
) -> Result<Option<ohl_terrain::Terrain>, String> {
    let project = file.extension().is_some_and(|e| e == "ohl");
    if project && !lidar {
        let session = ohl_session::Session::open(file).map_err(|e| e.to_string())?;
        if let Some(name) = session.setting("app.check_surface").ok().flatten() {
            let (points, triangles) = session.surface(&name).map_err(|e| e.to_string())?;
            println!("  ground: the project's surface of the checks {name}");
            return Ok(Some(
                ohl_terrain::Terrain::from_mesh(points, triangles).map_err(|e| e.to_string())?,
            ));
        }
    }
    let straight = || {
        println!(
            "  WARNING ground: none given and none in the file — straight between the structures' \
             feet and level across; across a valley it passes close under the wires (far too \
             high values): give --terrain or --surface"
        );
        Ok(None)
    };
    if !project {
        return straight();
    }
    let Ok(source) = ohl_pointcloud::open_source(&file.display().to_string()) else {
        return straight();
    };
    let all: Vec<(String, String)> = ohl_electrical::cross_section::line_spans(line)
        .into_iter()
        .map(|s| (s.from, s.to))
        .collect();
    let at = |id: &str| {
        line.structure(id)
            .map(|s| [s.position[0], s.position[1]])
            .ok_or_else(|| format!("no structure {id}"))
    };
    let mut lines = Vec::new();
    for (from, to) in &all {
        lines.push(vec![at(from)?, at(to)?]);
    }
    if lines.is_empty() {
        return straight();
    }
    let corridor = ohl_lidar::Corridor::new(&lines, reach);
    let scheme = source
        .class_scheme()
        .and_then(|t| ohl_lidar::ClassScheme::parse(&t).ok())
        .unwrap_or_else(ohl_lidar::ClassScheme::asprs);
    let classes = scheme.codes_with_role("ground");
    let mut cells = ohl_lidar::LowestPerCell::new(Some(1.0));
    // Exit clause: decoding is about 1 s per 10⁷ points.
    let budget = ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
    source.visit(
        Some(&corridor),
        Some(&classes),
        &budget,
        &mut |x, y, z, _| cells.add(x, y, z),
    )?;
    if cells.points().len() < 3 {
        return straight();
    }
    println!(
        "  ground: {} LiDAR ground cells of 1 m within {reach:.0} m of {} span(s)",
        cells.points().len(),
        lines.len()
    );
    Ok(Some(cells.terrain(Some(50.0)).map_err(|e| e.to_string())?))
}

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
    if a.ends {
        for (key, ends) in magnetic::circuit_ends(&line) {
            println!("circuit {}: end points {}", key.label(), ends.join(", "));
        }
        return Ok(());
    }
    let mut ends = Vec::new();
    for e in &a.end {
        let (left, value) = e
            .split_once('=')
            .ok_or_else(|| format!("--end {e}: CIRCUIT@STRUCTURE=A"))?;
        let (circuit, structure) = left
            .split_once('@')
            .ok_or_else(|| format!("--end {e}: CIRCUIT@STRUCTURE=A"))?;
        ends.push(magnetic::EndCurrent {
            circuit: circuit.trim().to_owned(),
            structure: structure.trim().to_owned(),
            amps: match value.trim() {
                "rest" => None,
                v => Some(
                    v.parse()
                        .map_err(|_| format!("--end {e}: {v} is not a number of amperes"))?,
                ),
            },
        });
    }
    if currents.is_empty() && ends.is_empty() {
        return Err(
            "give the currents: --current CIRCUIT=A, --end CIRCUIT@STRUCTURE=A (or --thermal)"
                .into(),
        );
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
    let mut cuts = Vec::new();
    for c in &a.cut {
        let (span, fraction) = match c.split_once('@') {
            Some((s, f)) => (
                s,
                Some(
                    f.trim()
                        .parse::<f64>()
                        .map_err(|_| format!("--cut {c}: {f} is not a fraction 0–1"))?,
                ),
            ),
            None => (c.as_str(), None),
        };
        let (from, to) = span_arg(span, "--cut")?;
        cuts.push(magnetic::CutSpec {
            from,
            to,
            fraction,
            width: a.width,
            above: a.above,
            step: a.cut_step,
        });
    }
    let mut volumes = Vec::new();
    for v in &a.volume {
        let (from, to) = span_arg(v, "--volume")?;
        volumes.push(magnetic::VolumeSpec {
            from,
            to,
            width: a.width,
            above: a.above,
            step: a.volume_step,
        });
    }
    if !volumes.is_empty() && a.level.is_empty() {
        return Err("give the volumes' levels: --level µT".into());
    }
    let mut levels = a.level.clone();
    levels.sort_by(f64::total_cmp);
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
        ends,
        earth_wires: a.earth_wires,
        towers: a.towers,
        height: a.height,
        width: a.width,
        step: a.step,
        stations,
        grids,
        grid_step: a.grid_step,
        points,
        limit_ut: a.limit,
        piece_length: a.piece_length,
        modelled_within: a.modelled_within,
        jumpers: !a.no_jumpers,
        cuts,
        volumes,
        levels,
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
        (None, None) => project_ground(&a.model.file, &line, a.width + 20.0, false)?,
    };
    let height = |x: f64, y: f64| terrain.as_ref().and_then(|t| t.height(x, y));
    let ground: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> = terrain
        .as_ref()
        .map(|_| &height as &(dyn Fn(f64, f64) -> Option<f64> + Sync));
    let jumpers: Vec<_> = ohl_line::jumper::solve_jumpers(&line, &library, &case, &shapes)
        .into_iter()
        .filter_map(Result::ok)
        .collect();
    let src = ohl_electrical::Sources {
        line: &line,
        library: &library,
        shapes: &shapes,
        temperature: case.temperature,
        ground,
        jumpers: &jumpers,
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
    for t in &m.towers {
        println!(
            "  structure {}: {:.1} A at {:.1}° into the ground through {} Ω{} ({} Hz)",
            t.structure,
            t.amps,
            t.angle_deg,
            t.resistance_ohm,
            if t.assumed { " (assumed)" } else { "" },
            t.frequency_hz
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
    let off = |d: &Option<ohl_electrical::field_views::Deviation>| match d {
        Some(d) => format!(
            "off the level: half within {:.2} %, nine in ten within {:.2} %, at most {:.1} % ({} points checked)",
            d.median * 100.0,
            d.p90 * 100.0,
            d.max * 100.0,
            d.points
        ),
        None => "no surface (the level lies outside the box)".into(),
    };
    let left = |d: &Option<ohl_electrical::field_views::Deviation>| match d {
        Some(d) => format!(
            "; the wires left out change the field by half within {:.3} %, nine in ten within {:.3} %, at most {:.3} % ({} points checked)",
            d.median * 100.0,
            d.p90 * 100.0,
            d.max * 100.0,
            d.points
        ),
        None => String::new(),
    };
    for c in &m.cuts {
        let v = &c.values;
        let max = v.values.iter().copied().fold(0.0, f64::max);
        println!(
            "cut {}–{} at {:.1} m: {} × {} points ({} m apart), max {max:.3} µT; {} pieces{}",
            c.from,
            c.to,
            c.at,
            v.offsets.len(),
            v.elevations.len(),
            v.cut.step,
            c.pieces,
            left(&c.left_out)
        );
    }
    for vol in &m.volumes {
        let v = &vol.values;
        println!(
            "volume {}–{}: {} of {} lattice points computed ({} m apart); {} pieces{}",
            vol.from,
            vol.to,
            v.computed,
            v.full,
            v.bounds.step,
            vol.pieces,
            left(&vol.left_out)
        );
        for (k, ((l, mesh), d)) in v.surfaces.iter().zip(&v.deviation).enumerate() {
            println!(
                "  {l} µT: {} triangles; {}{}",
                mesh.triangles.len(),
                off(d),
                ohl_electrical::field_views::left_out_text(v.left_out.get(k).copied().unwrap_or(0))
            );
        }
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
    if let Some(path) = &a.obj {
        std::fs::write(path, magnetic::obj(&m)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    Ok(())
}

/// A span FROM:TO with an optional @FRACTION.
fn span_at(text: &str, flag: &str) -> Result<(String, String, Option<f64>), String> {
    let (span, fraction) = match text.split_once('@') {
        Some((s, f)) => (
            s,
            Some(
                f.trim()
                    .parse::<f64>()
                    .map_err(|_| format!("{flag} {text}: {f} is not a fraction 0–1"))?,
            ),
        ),
        None => (text, None),
    };
    let (from, to) = span_arg(span, flag)?;
    Ok((from, to, fraction))
}

fn electric(a: &ElectricArgs) -> Result<(), String> {
    use ohl_electrical::electric::{self, CircuitVoltage, Settings};
    use ohl_electrical::magnetic::{self, Model, StationSpec};
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
    let mut voltages = Vec::new();
    for v in &a.voltage {
        let (c, kv) = v
            .split_once('=')
            .ok_or_else(|| format!("--voltage {v}: CIRCUIT=KV"))?;
        voltages.push(CircuitVoltage {
            circuit: c.trim().to_owned(),
            kv: kv
                .trim()
                .parse()
                .map_err(|_| format!("--voltage {v}: {kv} is not a number of kV"))?,
        });
    }
    let mut stations = Vec::new();
    for p in &a.profile {
        let (from, to, fraction) = span_at(p, "--profile")?;
        stations.push(StationSpec { from, to, fraction });
    }
    let mut cuts = Vec::new();
    for c in &a.cut {
        let (from, to, fraction) = span_at(c, "--cut")?;
        cuts.push(magnetic::CutSpec {
            from,
            to,
            fraction,
            width: a.width,
            above: a.above,
            step: a.cut_step,
        });
    }
    let mut volumes = Vec::new();
    for v in &a.volume {
        let (from, to) = span_arg(v, "--volume")?;
        volumes.push(magnetic::VolumeSpec {
            from,
            to,
            width: a.width,
            above: a.above,
            step: a.volume_step,
        });
    }
    if !volumes.is_empty() && a.level.is_empty() {
        return Err("give the volumes' levels: --level kV/m".into());
    }
    let mut levels = a.level.clone();
    levels.sort_by(f64::total_cmp);
    let points = match &a.points {
        Some(p) => magnetic::parse_points(
            &std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        )
        .map_err(|e| format!("{}: {e}", p.display()))?,
        None => Vec::new(),
    };
    let settings = Settings {
        model: if a.two_d { Model::TwoD } else { Model::ThreeD },
        voltages,
        height: a.height,
        width: a.width,
        step: a.step,
        stations,
        points,
        limit_kv_m: a.limit,
        ground_radius: a.ground_radius,
        ground_model: if !a.plane {
            electric::GroundModel::Tiled(electric::TiledFloor {
                fine: a.tile,
                coarse: a.coarse_tile,
                reach: a.floor_reach,
                ..Default::default()
            })
        } else {
            electric::GroundModel::Plane
        },
        cut_tile: a.cut_tile,
        volume_tile: a.volume_tile,
        jumpers: !a.no_jumpers,
        piece_length: a.piece_length,
        cuts,
        volumes,
        levels,
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
    let jumpers: Vec<_> = ohl_line::jumper::solve_jumpers(&line, &library, &case, &shapes)
        .into_iter()
        .filter_map(Result::ok)
        .collect();
    let terrain = match (&a.terrain, &a.surface) {
        (Some(path), _) => Some(ohl_terrain::Terrain::load(path).map_err(|e| e.to_string())?),
        (None, Some(name)) => {
            let session = ohl_session::Session::open(&a.model.file).map_err(|e| e.to_string())?;
            let (points, triangles) = session.surface(name).map_err(|e| e.to_string())?;
            Some(ohl_terrain::Terrain::from_mesh(points, triangles).map_err(|e| e.to_string())?)
        }
        (None, None) => {
            let reach = match settings.ground_model {
                ohl_electrical::electric::GroundModel::Tiled(_) => a.floor_reach + a.width + 20.0,
                ohl_electrical::electric::GroundModel::Plane => a.ground_radius + a.width + 20.0,
            };
            project_ground(&a.model.file, &line, reach, a.lidar_ground)?
        }
    };
    let height = |x: f64, y: f64| terrain.as_ref().and_then(|t| t.height(x, y));
    let ground: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> = terrain
        .as_ref()
        .map(|_| &height as &(dyn Fn(f64, f64) -> Option<f64> + Sync));
    let src = ohl_electrical::Sources {
        line: &line,
        library: &library,
        shapes: &shapes,
        temperature: case.temperature,
        ground,
        jumpers: &jumpers,
    };
    let m = electric::run(&src, &settings, None)?;
    println!(
        "Electric field strength of {} in \"{}\" ({:.1} °C): {}, {} m above the ground{}",
        line.name,
        case.name,
        case.temperature.celsius(),
        m.model.describe(),
        m.height,
        match m.limit_kv_m {
            Some(l) => format!(", limit {l} kV/m"),
            None => String::new(),
        }
    );
    for n in &m.notes {
        println!("  note: {n}");
    }
    println!(
        "{:<16} {:>9} {:>10} {:>9} {:>12} {:>7}",
        "profile", "station m", "max kV/m", "at m", "ground off m", "tiles"
    );
    for p in &m.profiles {
        println!(
            "{:<16} {:>9.1} {:>10.4} {:>9.1} {:>12.2} {:>7}",
            format!("{}–{}", p.from, p.to),
            p.at,
            p.max_kv_m,
            p.max_offset,
            p.ground_departure,
            p.tiles.map_or(String::new(), |n| n.to_string())
        );
        if a.all_points {
            for q in &p.points {
                println!(
                    "    {:>8.1} m  {:>9.4} kV/m{}",
                    q.offset,
                    q.e_kv_m,
                    q.margin_kv_m
                        .map_or(String::new(), |x| format!("  margin {x:.4}"))
                );
            }
        }
    }
    for c in &m.cuts {
        let v = &c.values;
        let max = v.values.iter().copied().fold(0.0, f64::max);
        println!(
            "cut {}–{} at {:.1} m: {} × {} points ({} m apart), max {max:.3} kV/m",
            c.from,
            c.to,
            c.at,
            v.offsets.len(),
            v.elevations.len(),
            v.cut.step
        );
    }
    for vol in &m.volumes {
        let v = &vol.values;
        println!(
            "volume {}–{}: {} of {} lattice points computed ({} m apart)",
            vol.from, vol.to, v.computed, v.full, v.bounds.step
        );
        for (k, ((l, mesh), d)) in v.surfaces.iter().zip(&v.deviation).enumerate() {
            println!(
                "  {l} kV/m: {} triangles{}{}",
                mesh.triangles.len(),
                d.map_or(String::new(), |d| format!(
                    "; off the level: half within {:.2} %, nine in ten within {:.2} %, at most {:.1} %",
                    d.median * 100.0,
                    d.p90 * 100.0,
                    d.max * 100.0
                )),
                ohl_electrical::field_views::left_out_text(v.left_out.get(k).copied().unwrap_or(0))
            );
        }
    }
    if !m.points.is_empty() {
        println!(
            "{:<16} {:>12} {:>12} {:>8} {:>8} {:>9} {:>9}",
            "point", "x", "y", "z", "h m", "E kV/m", "margin"
        );
        for p in &m.points {
            println!(
                "{:<16} {:>12.2} {:>12.2} {:>8.2} {:>8.2} {:>9.4} {:>9}",
                p.name,
                p.x,
                p.y,
                p.z,
                p.height,
                p.e_kv_m,
                p.margin_kv_m.map_or("—".into(), |x| format!("{x:.4}"))
            );
        }
    }
    Ok(())
}
