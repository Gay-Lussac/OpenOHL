// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl electrical …`: electrical calculations of a line (roadmap O18) —
//! `parameters`: its line parameters per circuit, stretch and span
//! ([`ohl_electrical`]).

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
    Parameters(ParametersArgs),
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
