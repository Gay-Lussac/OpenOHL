// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl electrical …`: electrical calculations of a line (roadmap O18) —
//! `parameters`: its line parameters per circuit, stretch and span;
//! `magnetic`: the magnetic flux density of its currents in profiles across
//! it, grids, cuts, volumes and given points; `electric`: the electric field
//! strength of its voltages likewise; `fields`: both in one run, one set of
//! results ([`ohl_electrical`]).

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
    /// the line (every span's lowest point by default, ±100 m, 1 m above
    /// the ground), grids, given points; 3D from the wires as solved
    /// (Biot–Savart, finite segments) or 2D; an optional limit with the
    /// margin and the limit line's width.
    Magnetic(Box<MagneticArgs>),
    /// Electric field strength (kV/m) of the line's voltages: profiles
    /// across the line (every span's lowest point by default, ±50 m, 1 m
    /// above the ground), given points, cuts, volumes; 3D from the wires as
    /// solved with their images in the ground under each point, or 2D; an
    /// optional limit with the margin.
    Electric(Box<ElectricArgs>),
    /// Both fields in one run (2026-10-31): the magnetic field of the
    /// currents and the electric field of the voltages at the same
    /// profiles, cuts, volumes and points, over the same ground; one JSON
    /// set of results and one report of a --preset-plugin.
    Fields(Box<FieldsArgs>),
}

/// What both fields take (`ohl electrical magnetic`, `electric`,
/// `fields`): the line and its case, the ground, the places.
#[derive(Args)]
pub struct FieldCommonArgs {
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
    /// The 2D model (infinite straight parallel conductors) instead of 3D.
    #[arg(long)]
    pub two_d: bool,
    /// A profile: FROM:TO (a span's structures) at its lowest point, or
    /// FROM:TO@FRACTION (0–1 of the span from the first along the line).
    /// Repeatable; default every span at its lowest point.
    #[arg(long, value_name = "FROM:TO[@FRACTION]")]
    pub profile: Vec<String>,
    /// The profiles (and cuts, volumes, grids) reach this far to either
    /// side, m (default: the magnetic field's 100 m, the electric field's
    /// 50 m; `fields` each its own).
    #[arg(long)]
    pub width: Option<f64>,
    /// Height above the ground of the profiles, the grids and the points
    /// without one, m.
    #[arg(long, default_value_t = 1.0)]
    pub height: f64,
    /// Between profile and grid points, m.
    #[arg(long, default_value_t = 1.0)]
    pub step: f64,
    /// The longest straight piece of a wire, m (3D; default: the solver's
    /// pieces, 64 per span or part of a span). Shorter pieces lie closer
    /// to the wire and take longer.
    #[arg(long)]
    pub piece_length: Option<f64>,
    /// Leave the jumpers out (by default they carry their phase's current
    /// and charge).
    #[arg(long)]
    pub no_jumpers: bool,
    /// A vertical cut across the line at a station of an alignment (3D;
    /// 2026-10-31: cuts are made on the alignment): its plane
    /// perpendicular to the alignment, at an angle point along the
    /// bisector, as OHL-Design's cross-section panels; --width to either
    /// side, from the ground to --above over the highest wire; an
    /// optional label after # (carried to the results). The older
    /// FROM:TO[@FRACTION] (a span's place; its lowest point without @) is
    /// converted to the alignment station of that place, said.
    /// Repeatable.
    #[arg(long, value_name = "ALIGNMENT@STATION[#LABEL]")]
    pub cut: Vec<String>,
    /// Between a cut's points, m (1 m: the low levels' isolines within
    /// 0.03 % of their level; near the wires a finer step for the high
    /// levels — see the theory's measurements).
    #[arg(long, default_value_t = 1.0)]
    pub cut_step: f64,
    /// A volume over the spans from FROM to TO along the line (3D): the
    /// isosurfaces of the levels, --width to either side, from the ground
    /// to --above over the highest wire. Repeatable.
    #[arg(long, value_name = "FROM:TO")]
    pub volume: Vec<String>,
    /// The volumes' fine lattice step, m.
    #[arg(long, default_value_t = 1.0)]
    pub volume_step: f64,
    /// Cuts and volumes reach this far over the highest wire, m.
    #[arg(long, default_value_t = 10.0)]
    pub above: f64,
    /// Points from a CSV file: a header with x, y and z (elevation) or
    /// height (above the ground), optionally name.
    #[arg(long)]
    pub points: Option<PathBuf>,
    /// Print every profile point (default: a summary per profile).
    #[arg(long)]
    pub all_points: bool,
    #[command(flatten)]
    pub plugin: FieldPluginArgs,
}

/// The magnetic field's own options (`magnetic`, `fields`).
#[derive(Args)]
pub struct MagneticOptions {
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
    /// A grid on a horizontal plane over span FROM:TO. Repeatable.
    #[arg(long, value_name = "FROM:TO")]
    pub grid: Vec<String>,
    /// Between grid points, m (default: --step).
    #[arg(long)]
    pub grid_step: Option<f64>,
    /// Cuts and volumes take the wires within this plan distance of them
    /// only, m (default: the whole line); the difference to the whole
    /// line's field is computed at some of their points and said.
    #[arg(long)]
    pub modelled_within: Option<f64>,
    /// With --two-d: the wires at the span's mean height (the line
    /// parameters' rule) instead of where they cross the station.
    #[arg(long, requires = "two_d")]
    pub mean_heights: bool,
}

/// The electric field's own options (`electric`, `fields`).
#[derive(Args)]
pub struct ElectricOptions {
    /// A circuit's voltage between phases: CIRCUIT=KV (CIRCUIT: its name or
    /// its system's id, `*` every circuit). Repeatable; default: the
    /// system's nominal voltage, said.
    #[arg(long, value_name = "CIRCUIT=KV")]
    pub voltage: Vec<String>,
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
    /// How far the floor under a volume reaches beyond it every way, m
    /// (a volume too large for one floor is floored in parts as long as
    /// this, each over the ground within this of its part).
    #[arg(long, default_value_t = ohl_electrical::electric::VOLUME_MARGIN)]
    pub volume_margin: f64,
    /// 3D: the stretches each span is cut into, each at the charge of its
    /// own cross-section (24 by default; 16: 0.03 % off 64 near a
    /// structure, 24: 0.005 %).
    #[arg(long, default_value_t = ohl_electrical::electric::STRETCHES)]
    pub stretches: usize,
    /// The ground from the project's point cloud even when the project has
    /// a surface of the checks: its ground classes within the floor's
    /// reach of the spans computed, the lowest point per metre
    /// triangulated. (Without --terrain or --surface the ground is by
    /// default the project's surface of the checks, else its point
    /// cloud's, as in OHL-Design.)
    #[arg(long, conflicts_with_all = ["terrain", "surface"])]
    pub lidar_ground: bool,
    /// The structures' steel earthed (3D on the tiled floor): their types'
    /// members and poles at detail levels 2 and 3, else their own stick
    /// drawings; the others left out, said.
    #[arg(long)]
    pub steel: bool,
}

/// `ohl electrical electric`.
#[derive(Args)]
pub struct ElectricArgs {
    #[command(flatten)]
    pub common: FieldCommonArgs,
    #[command(flatten)]
    pub electric: ElectricOptions,
    /// A level for the isolines and isosurfaces, kV/m. Repeatable.
    #[arg(long)]
    pub level: Vec<f64>,
    /// A limit, kV/m: the margin per point (none is built in).
    #[arg(long)]
    pub limit: Option<f64>,
    /// Write every point as CSV here.
    #[arg(long)]
    pub csv: Option<PathBuf>,
}

/// `ohl electrical magnetic`.
#[derive(Args)]
pub struct MagneticArgs {
    #[command(flatten)]
    pub common: FieldCommonArgs,
    #[command(flatten)]
    pub magnetic: MagneticOptions,
    /// A level for the isolines and isosurfaces, µT. Repeatable.
    #[arg(long)]
    pub level: Vec<f64>,
    /// A limit, µT: the margin per point and the limit line's width per
    /// profile (no limit is built in).
    #[arg(long)]
    pub limit: Option<f64>,
    /// Write the volumes' isosurfaces to this Wavefront OBJ file.
    #[arg(long)]
    pub obj: Option<PathBuf>,
    /// Write every point as CSV here.
    #[arg(long)]
    pub csv: Option<PathBuf>,
}

/// `ohl electrical fields`: both fields in one run (owner, 2026-10-31:
/// "Ok for 1 but keep it clean").
#[derive(Args)]
pub struct FieldsArgs {
    #[command(flatten)]
    pub common: FieldCommonArgs,
    #[command(flatten)]
    pub magnetic: MagneticOptions,
    #[command(flatten)]
    pub electric: ElectricOptions,
    /// A level of the magnetic field's isolines and isosurfaces, µT.
    /// Repeatable.
    #[arg(long, value_name = "µT")]
    pub level_ut: Vec<f64>,
    /// A level of the electric field's isolines and isosurfaces, kV/m.
    /// Repeatable.
    #[arg(long, value_name = "KV/M")]
    pub level_kv_m: Vec<f64>,
    /// A limit of the magnetic field, µT (none is built in).
    #[arg(long, value_name = "µT")]
    pub limit_ut: Option<f64>,
    /// A limit of the electric field, kV/m (none is built in).
    #[arg(long, value_name = "KV/M")]
    pub limit_kv_m: Option<f64>,
    /// Write the magnetic field's points as CSV here.
    #[arg(long)]
    pub csv_magnetic: Option<PathBuf>,
    /// Write the electric field's points as CSV here.
    #[arg(long)]
    pub csv_electric: Option<PathBuf>,
}

/// The results as JSON and the field presets of a plugin, for `ohl
/// electrical magnetic`, `electric` and `fields` (2026-10-31: the core
/// extension points for a building plugin).
#[derive(Args)]
pub struct FieldPluginArgs {
    /// Write the results as JSON here (format `ohl-field-results-set` 1:
    /// one `ohl-field-results` 2 document per field computed — the cuts
    /// with their isolines, the given points, the settings).
    #[arg(long)]
    pub json: Option<PathBuf>,
    /// A plugin with the `field-presets` export: the points its manifest
    /// asks for are gathered (the point cloud's, --terrain's survey
    /// points), it gives the cuts and points to compute (added to those
    /// given here), and makes its report files from the results of the
    /// run (one report, also of both fields with `fields`).
    #[arg(long, value_name = "DIR")]
    pub preset_plugin: Option<PathBuf>,
    /// The plugin's parameters, a TOML file (default: its template).
    #[arg(long, requires = "preset_plugin")]
    pub params: Option<PathBuf>,
    /// The plugin's preset to run, by name (default: its first).
    #[arg(long, requires = "preset_plugin")]
    pub preset: Option<String>,
    /// The folder the plugin's report files are written to (default: the
    /// current folder).
    #[arg(long, requires = "preset_plugin")]
    pub out: Option<PathBuf>,
    /// The point cloud the plugin's points come from: a COPC file or a
    /// project's cloud (FILE.ohl or FILE.ohl#name); default: the
    /// project's, if any.
    #[arg(long, requires = "preset_plugin")]
    pub cloud: Option<String>,
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
        ElectricalCommand::Fields(a) => fields(&a),
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
pub(crate) fn project_ground(
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

/// The structures' steel (`--steel`), as OHL-Design takes it: a type's
/// members and pole at levels 2 and 3 as placed (each member's radius from
/// its section's solid, else 5 cm), else the structure's own stick drawing
/// (5 cm); the structures with neither (level 1) left out, by id.
fn steel(
    model: &crate::analysis_run::ModelArgs,
    line: &ohl_line::Line,
) -> Result<(Vec<ohl_electrical::electric::SteelMember>, Vec<String>), String> {
    const DEFAULT_RADIUS: f64 = 0.05;
    let libs = crate::project::Libraries::resolve(
        &model.file,
        crate::project::GivenLibraries {
            cables: model.library.as_deref(),
            structures: model.structures.as_deref(),
            insulators: model.insulators.as_deref(),
            sections: None,
        },
    )?;
    let bodies = if line.structures.iter().any(|s| s.structure_type.is_some()) {
        let structures = ohl_structures::StructureLibrary::load_dir(&libs.structures)
            .map_err(|e| e.to_string())?;
        let sections =
            match libs.sections.clone().or_else(|| {
                Some(ohl_core::files::shipped_library("sections")).filter(|p| p.is_dir())
            }) {
                Some(p) => ohl_sections::SectionLibrary::load(&p).unwrap_or_default(),
                None => Default::default(),
            };
        ohl_structures_fe::drawn::Bodies::load(&structures, &sections)
    } else {
        Default::default()
    };
    let mut steel = Vec::new();
    let mut left_out = Vec::new();
    for st in &line.structures {
        let member = |points: Vec<[f64; 3]>, radius: f64| ohl_electrical::electric::SteelMember {
            structure: st.id.clone(),
            points,
            radius,
        };
        let body = st
            .structure_type
            .as_deref()
            .and_then(|t| bodies.get(t, st.extension.as_deref()));
        if let Some(b) = body {
            let at = ohl_line::links::feet(st);
            for (points, radius) in b.placed_steel(at, st.orientation_deg, DEFAULT_RADIUS) {
                steel.push(member(points, radius));
            }
        } else if !st.members.is_empty() {
            for m in &st.members {
                steel.push(member(m.clone(), DEFAULT_RADIUS));
            }
        } else {
            left_out.push(st.id.clone());
        }
    }
    Ok((steel, left_out))
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

/// The line, its wire library and its case solved: what every field run
/// starts from.
struct Solved {
    line: ohl_line::Line,
    library: ohl_cables::Library,
    case: ohl_weather::WeatherCase,
    shapes: Vec<ohl_line::solve::SectionShape>,
    jumpers: Vec<ohl_line::jumper::JumperShape>,
}

/// The line of `c` loaded, its weather case chosen.
fn load_case(
    c: &FieldCommonArgs,
) -> Result<
    (
        ohl_line::Line,
        ohl_cables::Library,
        ohl_weather::WeatherCase,
    ),
    String,
> {
    let (mut line, library) = load(&c.model)?;
    let case = match (&c.weather, &c.case) {
        (Some(path), Some(name)) => {
            let set = ohl_weather::WeatherCases::load(path)?;
            line.set_weather_cases(&set.cases);
            set.select(std::slice::from_ref(name), &[])?
                .into_iter()
                .next()
                .ok_or_else(|| format!("no weather case {name}"))?
        }
        _ => ohl_weather::WeatherCase::still("still air", Temperature::from_celsius(c.temperature)),
    };
    Ok((line, library, case))
}

/// The sections and jumpers of `line` solved in `case` (a section not
/// solved said and left out).
fn solve(
    line: ohl_line::Line,
    library: ohl_cables::Library,
    case: ohl_weather::WeatherCase,
) -> Solved {
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
    Solved {
        line,
        library,
        case,
        shapes,
        jumpers,
    }
}

/// The ground: `--terrain`, `--surface`, else the project's
/// ([`project_ground`], within `reach` of the spans; `lidar`: its point
/// cloud's).
fn terrain(
    c: &FieldCommonArgs,
    line: &ohl_line::Line,
    reach: f64,
    lidar: bool,
) -> Result<Option<ohl_terrain::Terrain>, String> {
    match (&c.terrain, &c.surface) {
        (Some(path), _) => Ok(Some(
            ohl_terrain::Terrain::load(path).map_err(|e| e.to_string())?,
        )),
        (None, Some(name)) => {
            let session = ohl_session::Session::open(&c.model.file).map_err(|e| e.to_string())?;
            let (points, triangles) = session.surface(name).map_err(|e| e.to_string())?;
            Ok(Some(
                ohl_terrain::Terrain::from_mesh(points, triangles).map_err(|e| e.to_string())?,
            ))
        }
        (None, None) => project_ground(&c.model.file, line, reach, lidar),
    }
}

/// The places both fields take from `c`: the profiles, the cuts (with
/// whether each was given as a span's place, converted), the volumes, the
/// points.
struct Places {
    stations: Vec<ohl_electrical::magnetic::StationSpec>,
    cuts: Vec<ohl_electrical::magnetic::CutSpec>,
    volumes: Vec<ohl_electrical::magnetic::VolumeSpec>,
    points: Vec<ohl_electrical::magnetic::GivenPoint>,
    /// To either side, m.
    width: f64,
}

/// The places asked, `width` to either side unless --width says.
fn places(c: &FieldCommonArgs, width: f64) -> Result<Places, String> {
    let width = c.width.unwrap_or(width);
    use ohl_electrical::magnetic::{self, StationSpec};
    let mut stations = Vec::new();
    for p in &c.profile {
        let (from, to, fraction) = span_at(p, "--profile")?;
        stations.push(StationSpec { from, to, fraction });
    }
    let mut cuts = Vec::new();
    for text in &c.cut {
        let (at, label) = cut_arg(text)?;
        cuts.push(magnetic::CutSpec {
            at,
            width,
            above: c.above,
            step: c.cut_step,
            label,
        });
    }
    let mut volumes = Vec::new();
    for v in &c.volume {
        let (from, to) = span_arg(v, "--volume")?;
        volumes.push(magnetic::VolumeSpec {
            from,
            to,
            width,
            above: c.above,
            step: c.volume_step,
        });
    }
    let points = match &c.points {
        Some(p) => magnetic::parse_points(
            &std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        )
        .map_err(|e| format!("{}: {e}", p.display()))?,
        None => Vec::new(),
    };
    Ok(Places {
        stations,
        cuts,
        volumes,
        points,
        width,
    })
}

/// The levels given, ascending; an error when volumes are asked without.
fn levels(given: &[f64], volumes: bool, flag: &str) -> Result<Vec<f64>, String> {
    if volumes && given.is_empty() {
        return Err(format!("give the volumes' levels: {flag}"));
    }
    let mut levels = given.to_vec();
    levels.sort_by(f64::total_cmp);
    Ok(levels)
}

/// The circuits' currents of `o` (with where each comes from), and the
/// currents at the end points. With `--ends`: the end points listed,
/// `None`.
#[allow(clippy::type_complexity)]
fn currents(
    o: &MagneticOptions,
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
) -> Result<
    Option<(
        Vec<ohl_electrical::magnetic::CircuitCurrent>,
        Vec<(String, String)>,
        Vec<ohl_electrical::magnetic::EndCurrent>,
    )>,
    String,
> {
    use ohl_electrical::magnetic;
    let mut currents = Vec::new();
    let mut sources: Vec<(String, String)> = Vec::new();
    if let Some(path) = &o.thermal {
        let method = ohl_thermal::Method::parse(&o.method)?;
        let name = o.ambient_case.clone().unwrap_or_default();
        let ambient = ohl_thermal::ambient::AmbientCases::load(path)?
            .select(std::slice::from_ref(&name))?
            .into_iter()
            .next()
            .ok_or_else(|| format!("no ambient case {name}"))?;
        let t = o.max_temp.unwrap_or_default();
        for mut c in magnetic::thermal_currents(
            line,
            library,
            method,
            &ambient,
            t,
            &ohl_line::Defaults::default(),
        )? {
            c.reverse = o.reverse.contains(&c.circuit);
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
    for c in &o.current {
        let c = current_arg(c)?;
        sources.push((c.circuit.clone(), "given".into()));
        currents.push(c);
    }
    if o.ends {
        for (key, ends) in magnetic::circuit_ends(line) {
            println!("circuit {}: end points {}", key.label(), ends.join(", "));
        }
        return Ok(None);
    }
    let mut ends = Vec::new();
    for e in &o.end {
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
    Ok(Some((currents, sources, ends)))
}

/// The magnetic field's settings.
fn magnetic_settings(
    c: &FieldCommonArgs,
    o: &MagneticOptions,
    p: &Places,
    currents: Vec<ohl_electrical::magnetic::CircuitCurrent>,
    ends: Vec<ohl_electrical::magnetic::EndCurrent>,
    levels: Vec<f64>,
    limit: Option<f64>,
) -> Result<ohl_electrical::magnetic::Settings, String> {
    use ohl_electrical::magnetic::{Heights2d, Model, Settings};
    let grids = o
        .grid
        .iter()
        .map(|g| span_arg(g, "--grid"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Settings {
        model: if c.two_d { Model::TwoD } else { Model::ThreeD },
        heights_2d: if o.mean_heights {
            Heights2d::Mean
        } else {
            Heights2d::Station
        },
        currents,
        ends,
        earth_wires: o.earth_wires,
        towers: o.towers,
        height: c.height,
        width: p.width,
        step: c.step,
        profiles: true,
        stations: p.stations.clone(),
        grids,
        grid_step: o.grid_step,
        points: p.points.clone(),
        limit_ut: limit,
        piece_length: c.piece_length,
        modelled_within: o.modelled_within,
        jumpers: !c.no_jumpers,
        cuts: p.cuts.clone(),
        volumes: p.volumes.clone(),
        levels,
    })
}

/// The electric field's settings (the structures' steel with `--steel`).
fn electric_settings(
    c: &FieldCommonArgs,
    o: &ElectricOptions,
    p: &Places,
    line: &ohl_line::Line,
    levels: Vec<f64>,
    limit: Option<f64>,
) -> Result<ohl_electrical::electric::Settings, String> {
    use ohl_electrical::electric::{self, CircuitVoltage, Settings};
    use ohl_electrical::magnetic::Model;
    let mut voltages = Vec::new();
    for v in &o.voltage {
        let (circuit, kv) = v
            .split_once('=')
            .ok_or_else(|| format!("--voltage {v}: CIRCUIT=KV"))?;
        voltages.push(CircuitVoltage {
            circuit: circuit.trim().to_owned(),
            kv: kv
                .trim()
                .parse()
                .map_err(|_| format!("--voltage {v}: {kv} is not a number of kV"))?,
        });
    }
    let mut settings = Settings {
        model: if c.two_d { Model::TwoD } else { Model::ThreeD },
        voltages,
        height: c.height,
        width: p.width,
        step: c.step,
        stations: p.stations.clone(),
        points: p.points.clone(),
        limit_kv_m: limit,
        ground_radius: o.ground_radius,
        ground_model: if !o.plane {
            electric::GroundModel::Tiled(electric::TiledFloor {
                fine: o.tile,
                coarse: o.coarse_tile,
                reach: o.floor_reach,
                ..Default::default()
            })
        } else {
            electric::GroundModel::Plane
        },
        cut_tile: o.cut_tile,
        volume_tile: o.volume_tile,
        volume_margin: o.volume_margin,
        stretches: o.stretches,
        jumpers: !c.no_jumpers,
        piece_length: c.piece_length,
        cuts: p.cuts.clone(),
        volumes: p.volumes.clone(),
        levels,
        ..Default::default()
    };
    if o.steel {
        (settings.steel, settings.steel_left_out) = steel(&c.model, line)?;
    }
    Ok(settings)
}

/// How far around the spans the electric field needs the ground, m.
fn electric_reach(width: f64, o: &ElectricOptions) -> f64 {
    if o.plane {
        o.ground_radius + width + 20.0
    } else {
        o.floor_reach + width + 20.0
    }
}

/// The ground closure of `terrain` as the fields take it.
fn ground_of(
    terrain: &Option<ohl_terrain::Terrain>,
) -> impl Fn(f64, f64) -> Option<f64> + Sync + '_ {
    move |x: f64, y: f64| terrain.as_ref().and_then(|t| t.height(x, y))
}

fn magnetic(a: &MagneticArgs) -> Result<(), String> {
    use ohl_electrical::magnetic;
    let (line, library, case) = load_case(&a.common)?;
    let Some((currents, sources, ends)) = currents(&a.magnetic, &line, &library)? else {
        return Ok(());
    };
    let p = places(&a.common, ohl_electrical::magnetic::WIDTH)?;
    let levels = levels(&a.level, !p.volumes.is_empty(), "--level µT")?;
    let mut settings =
        magnetic_settings(&a.common, &a.magnetic, &p, currents, ends, levels, a.limit)?;
    let s = solve(line, library, case);
    let terrain = terrain(&a.common, &s.line, p.width + 20.0, false)?;
    let height = ground_of(&terrain);
    let ground: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> = terrain
        .as_ref()
        .map(|_| &height as &(dyn Fn(f64, f64) -> Option<f64> + Sync));
    let preset = preset_from_plugin(&a.common, &s)?;
    if let Some(p) = &preset {
        settings.cuts.extend(preset_cuts(&p.preset));
        settings.points.extend(preset_points(&p.preset));
        add_levels(&mut settings.levels, &p.preset.levels_ut);
    }
    let src = ohl_electrical::Sources {
        line: &s.line,
        library: &s.library,
        shapes: &s.shapes,
        temperature: s.case.temperature,
        ground,
        jumpers: &s.jumpers,
    };
    let m = magnetic::run(&src, &settings, None)?;
    print_magnetic(&s, &m, &settings, &sources, a.common.all_points);
    if let Some(path) = &a.csv {
        std::fs::write(path, magnetic::csv(&m)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    if let Some(path) = &a.obj {
        std::fs::write(path, magnetic::obj(&m)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    let (name, t) = (&s.case.name, s.case.temperature.celsius());
    let json = ohl_electrical::field_results::set_json(
        vec![ohl_electrical::field_results::magnetic_value(
            &m, &settings, name, t,
        )],
        name,
        t,
    );
    write_results(&a.common.plugin, &json, preset)
}

fn electric(a: &ElectricArgs) -> Result<(), String> {
    use ohl_electrical::electric;
    let (line, library, case) = load_case(&a.common)?;
    let p = places(&a.common, ohl_electrical::electric::WIDTH)?;
    let levels = levels(&a.level, !p.volumes.is_empty(), "--level kV/m")?;
    let mut settings = electric_settings(&a.common, &a.electric, &p, &line, levels, a.limit)?;
    let s = solve(line, library, case);
    let terrain = terrain(
        &a.common,
        &s.line,
        electric_reach(p.width, &a.electric),
        a.electric.lidar_ground,
    )?;
    let height = ground_of(&terrain);
    let ground: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> = terrain
        .as_ref()
        .map(|_| &height as &(dyn Fn(f64, f64) -> Option<f64> + Sync));
    let preset = preset_from_plugin(&a.common, &s)?;
    if let Some(p) = &preset {
        settings.cuts.extend(preset_cuts(&p.preset));
        settings.points.extend(preset_points(&p.preset));
        add_levels(&mut settings.levels, &p.preset.levels_kv_m);
    }
    let src = ohl_electrical::Sources {
        line: &s.line,
        library: &s.library,
        shapes: &s.shapes,
        temperature: s.case.temperature,
        ground,
        jumpers: &s.jumpers,
    };
    let e = electric::run(&src, &settings, None)?;
    print_electric(&s, &e, &settings, a.common.all_points);
    if let Some(path) = &a.csv {
        std::fs::write(path, electric::csv(&e)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    let (name, t) = (&s.case.name, s.case.temperature.celsius());
    let json = ohl_electrical::field_results::set_json(
        vec![ohl_electrical::field_results::electric_value(
            &e, &settings, name, t,
        )],
        name,
        t,
    );
    write_results(&a.common.plugin, &json, preset)
}

/// `ohl electrical fields`: both fields on the same places and ground,
/// one set of results, one report.
fn fields(a: &FieldsArgs) -> Result<(), String> {
    use ohl_electrical::{electric, field_results, magnetic};
    let (line, library, case) = load_case(&a.common)?;
    let Some((currents, sources, ends)) = currents(&a.magnetic, &line, &library)? else {
        return Ok(());
    };
    // Each field its own reach to either side (the magnetic field's
    // wider), unless --width says.
    let p = places(&a.common, magnetic::WIDTH)?;
    let pe = places(&a.common, electric::WIDTH)?;
    let volumes = !p.volumes.is_empty();
    let mut ms = magnetic_settings(
        &a.common,
        &a.magnetic,
        &p,
        currents,
        ends,
        levels(&a.level_ut, volumes, "--level-ut µT")?,
        a.limit_ut,
    )?;
    let mut es = electric_settings(
        &a.common,
        &a.electric,
        &pe,
        &line,
        levels(&a.level_kv_m, volumes, "--level-kv-m kV/m")?,
        a.limit_kv_m,
    )?;
    let s = solve(line, library, case);
    // The ground reaches as far as either field needs.
    let terrain = terrain(
        &a.common,
        &s.line,
        electric_reach(pe.width, &a.electric).max(p.width + 20.0),
        a.electric.lidar_ground,
    )?;
    let height = ground_of(&terrain);
    let ground: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> = terrain
        .as_ref()
        .map(|_| &height as &(dyn Fn(f64, f64) -> Option<f64> + Sync));
    let preset = preset_from_plugin(&a.common, &s)?;
    if let Some(p) = &preset {
        for (cuts, points) in [
            (&mut ms.cuts, &mut ms.points),
            (&mut es.cuts, &mut es.points),
        ] {
            cuts.extend(preset_cuts(&p.preset));
            points.extend(preset_points(&p.preset));
        }
        add_levels(&mut ms.levels, &p.preset.levels_ut);
        add_levels(&mut es.levels, &p.preset.levels_kv_m);
    }
    let src = ohl_electrical::Sources {
        line: &s.line,
        library: &s.library,
        shapes: &s.shapes,
        temperature: s.case.temperature,
        ground,
        jumpers: &s.jumpers,
    };
    let m = magnetic::run(&src, &ms, None)?;
    print_magnetic(&s, &m, &ms, &sources, a.common.all_points);
    let e = electric::run(&src, &es, None)?;
    print_electric(&s, &e, &es, a.common.all_points);
    if let Some(path) = &a.csv_magnetic {
        std::fs::write(path, magnetic::csv(&m)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    if let Some(path) = &a.csv_electric {
        std::fs::write(path, electric::csv(&e)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    let (name, t) = (&s.case.name, s.case.temperature.celsius());
    let json = field_results::set_json(
        vec![
            field_results::magnetic_value(&m, &ms, name, t),
            field_results::electric_value(&e, &es, name, t),
        ],
        name,
        t,
    );
    write_results(&a.common.plugin, &json, preset)
}

/// A cut's line of the summary: its label, alignment station (said when
/// it was given as a span's place), span, grid and largest value.
fn cut_line(
    spec: Option<&ohl_electrical::magnetic::CutSpec>,
    c: &ohl_electrical::magnetic::FieldCut,
    unit: &str,
) -> String {
    let v = &c.values;
    let max = v.values.iter().copied().fold(0.0, f64::max);
    let converted = match spec.map(|s| &s.at) {
        Some(ohl_electrical::magnetic::CutAt::InSpan { from, to, fraction }) => format!(
            " (given as {from}:{to}{}: converted to {}@{:.2}, cuts are placed on the alignment)",
            fraction.map_or(String::new(), |f| format!("@{f}")),
            c.alignment,
            c.station
        ),
        _ => String::new(),
    };
    format!(
        "cut {}{}@{:.1}{}{converted}: span {}–{} at {:.1} m, {} × {} points ({} m apart), max {max:.3} {unit}",
        c.label.as_ref().map_or(String::new(), |l| format!("{l} ")),
        c.alignment,
        c.station,
        if c.angle_point {
            " (angle point: along the bisector)"
        } else {
            ""
        },
        c.from,
        c.to,
        c.at,
        v.offsets.len(),
        v.elevations.len(),
        v.cut.step,
    )
}

/// The magnetic field's summary on the terminal.
fn print_magnetic(
    s: &Solved,
    m: &ohl_electrical::magnetic::MagneticField,
    settings: &ohl_electrical::magnetic::Settings,
    sources: &[(String, String)],
    all_points: bool,
) {
    println!(
        "Magnetic flux density of {} in \"{}\" ({:.1} °C): {}, {} m above the ground{}",
        s.line.name,
        s.case.name,
        s.case.temperature.celsius(),
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
        if all_points {
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
    for (k, c) in m.cuts.iter().enumerate() {
        println!(
            "{}; {} pieces{}",
            cut_line(settings.cuts.get(k), c, "µT"),
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
}

/// The electric field's summary on the terminal.
fn print_electric(
    s: &Solved,
    m: &ohl_electrical::electric::ElectricField,
    settings: &ohl_electrical::electric::Settings,
    all_points: bool,
) {
    println!(
        "Electric field strength of {} in \"{}\" ({:.1} °C): {}, {} m above the ground{}",
        s.line.name,
        s.case.name,
        s.case.temperature.celsius(),
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
        if all_points {
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
    for (k, c) in m.cuts.iter().enumerate() {
        println!("{}", cut_line(settings.cuts.get(k), c, "kV/m"));
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

/// `--cut ALIGNMENT@STATION[#LABEL]` (a station in m, `m` after it
/// allowed); the older `FROM:TO[@FRACTION][#LABEL]` (a `:` before the
/// `@`) as a span's place, converted by the run to its alignment station.
fn cut_arg(text: &str) -> Result<(ohl_electrical::magnetic::CutAt, Option<String>), String> {
    use ohl_electrical::magnetic::CutAt;
    let (spec, label) = match text.split_once('#') {
        Some((s, l)) if !l.trim().is_empty() => (s, Some(l.trim().to_owned())),
        Some((s, _)) => (s, None),
        None => (text, None),
    };
    let place = spec.rsplit_once('@').map_or(spec, |(p, _)| p);
    if place.contains(':') {
        let (from, to, fraction) = span_at(spec, "--cut")?;
        return Ok((CutAt::InSpan { from, to, fraction }, label));
    }
    let (alignment, station) = spec
        .rsplit_once('@')
        .ok_or_else(|| format!("--cut {text}: ALIGNMENT@STATION (m along the alignment)"))?;
    let s = station.trim();
    let s = s.strip_suffix('m').unwrap_or(s).trim();
    let station = s
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("--cut {text}: {station} is not a station in m"))?;
    Ok((
        CutAt::Station {
            alignment: alignment.trim().to_owned(),
            station,
        },
        label,
    ))
}

/// A plugin's preset, chosen and checked, with what its report needs.
struct PresetRun {
    host: ohl_plugins::WasmHost,
    dir: PathBuf,
    parameters: String,
    preset: ohl_plugins::FieldPreset,
}

/// The preset of `--preset-plugin` (none without it): the points its
/// manifest's `[field-input]` asks for gathered (the point cloud of
/// `--cloud`, else the project's; the survey points of `--terrain`), the
/// plugin given them, its parameters and the solved model, its presets
/// checked by the host against the line's alignments, the one named by
/// `--preset` (else the first) taken.
fn preset_from_plugin(c: &FieldCommonArgs, s: &Solved) -> Result<Option<PresetRun>, String> {
    let a = &c.plugin;
    let (file, terrain, line) = (&c.model.file, c.terrain.as_deref(), &s.line);
    let Some(dir) = &a.preset_plugin else {
        return Ok(None);
    };
    let host = ohl_plugins::WasmHost::new().map_err(|e| e.to_string())?;
    let manifest = ohl_plugins::Manifest::load(dir).map_err(|e| e.to_string())?;
    if !host.has_field_presets(dir).map_err(|e| e.to_string())? {
        return Err(format!(
            "{}: plugin {} has no field-presets export",
            dir.display(),
            manifest.plugin.name
        ));
    }
    let parameters = match &a.params {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        None => host
            .field_parameters_template(dir)
            .map_err(|e| e.to_string())?,
    };
    let spans: Vec<(String, String)> = ohl_electrical::cross_section::line_spans(line)
        .into_iter()
        .map(|s| (s.from, s.to))
        .collect();
    let alignments: Vec<(String, f64)> = line
        .alignment_views()
        .iter()
        .map(|v| {
            (
                v.id.clone(),
                ohl_line::route::plan_length(&v.centreline(line)),
            )
        })
        .collect();
    let at = |id: &str| {
        line.structure(id)
            .map(|s| [s.position[0], s.position[1]])
            .ok_or_else(|| format!("no structure {id}"))
    };
    let mut polylines = Vec::new();
    for (from, to) in &spans {
        polylines.push(vec![at(from)?, at(to)?]);
    }
    let table = manifest.field_input.clone().unwrap_or_default();
    let wants_cloud = !table.roles.is_empty() || !table.ground_roles.is_empty();
    let cloud = match (&a.cloud, wants_cloud) {
        (_, false) => None,
        (Some(spec), true) => Some(ohl_pointcloud::open_source(spec)?),
        (None, true) if file.extension().is_some_and(|e| e == "ohl") => {
            ohl_pointcloud::open_source(&file.display().to_string()).ok()
        }
        (None, true) => None,
    };
    let survey = match terrain {
        Some(path) if !table.survey_codes.is_empty() => ohl_terrain::read_points(
            &std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?,
        )
        .map_err(|e| format!("{}: {e}", path.display()))?,
        _ => Vec::new(),
    };
    // Exit clause: decoding is about 1 s per 10⁷ points.
    let budget = ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
    let gathered = ohl_io::field_input::gather(
        &table,
        cloud
            .as_deref()
            .map(|c| c as &dyn ohl_pointcloud::PointSource),
        &survey,
        &polylines,
        &budget,
    )?;
    let model = ohl_io::ExportModel::new(
        line,
        &[(
            s.case.name.clone(),
            s.case.summary(),
            s.shapes.clone(),
            s.jumpers.clone(),
        )],
    )
    .to_json();
    let input = ohl_plugins::FieldInput {
        parameters: parameters.clone(),
        model,
        points: gathered.points,
        ground: gathered.ground,
    };
    let presets = host
        .field_presets(dir, &input, &alignments)
        .map_err(|e| e.to_string())?;
    let names: Vec<&str> = presets.iter().map(|p| p.name.as_str()).collect();
    let preset = match &a.preset {
        Some(n) => presets.iter().find(|p| &p.name == n).ok_or_else(|| {
            format!(
                "plugin {}: no preset \"{n}\" (it gives: {})",
                manifest.plugin.name,
                names.join(", ")
            )
        })?,
        None => presets
            .first()
            .ok_or_else(|| format!("plugin {}: no preset for this line", manifest.plugin.name))?,
    }
    .clone();
    for n in &gathered.notes {
        println!("  plugin input: {n}");
    }
    println!(
        "  preset {} of plugin {} (of {}): {} cuts, {} points, {} outlines",
        preset.name,
        manifest.plugin.name,
        names.join(", "),
        preset.cuts.len(),
        preset.points.len(),
        preset.outlines.len()
    );
    for n in &preset.notes {
        println!("  plugin note: {n}");
    }
    Ok(Some(PresetRun {
        host,
        dir: dir.clone(),
        parameters,
        preset,
    }))
}

/// A preset's cuts as the fields take them: at their alignment stations.
fn preset_cuts(p: &ohl_plugins::FieldPreset) -> Vec<ohl_electrical::magnetic::CutSpec> {
    p.cuts
        .iter()
        .map(|c| ohl_electrical::magnetic::CutSpec {
            at: ohl_electrical::magnetic::CutAt::Station {
                alignment: c.alignment.clone(),
                station: c.station,
            },
            width: c.width,
            above: c.above,
            step: c.step,
            label: (!c.label.is_empty()).then(|| c.label.clone()),
        })
        .collect()
}

/// A preset's points as the fields take them.
fn preset_points(p: &ohl_plugins::FieldPreset) -> Vec<ohl_electrical::magnetic::GivenPoint> {
    p.points
        .iter()
        .map(|q| ohl_electrical::magnetic::GivenPoint {
            name: q.name.clone(),
            x: q.x,
            y: q.y,
            z: q.z,
            height: q.height,
        })
        .collect()
}

/// `levels` with `extra` added, ascending, each once.
fn add_levels(levels: &mut Vec<f64>, extra: &[f64]) {
    levels.extend_from_slice(extra);
    levels.sort_by(f64::total_cmp);
    levels.dedup();
}

/// Writes `--json` (the run's `ohl-field-results-set`, `json`) and, with
/// a preset, the plugin's report files of it into `--out`.
fn write_results(a: &FieldPluginArgs, json: &str, preset: Option<PresetRun>) -> Result<(), String> {
    if let Some(path) = &a.json {
        std::fs::write(path, json).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    let Some(p) = preset else {
        return Ok(());
    };
    let files = p
        .host
        .field_report(&p.dir, &p.parameters, &p.preset, json)
        .map_err(|e| e.to_string())?;
    let out = a.out.clone().unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    for f in &files {
        let path = out.join(&f.name);
        std::fs::write(&path, &f.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {} ({})", path.display(), f.media_type);
    }
    Ok(())
}
