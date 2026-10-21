// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl safety-volume`: the safety volumes around the wires
//! ([`ohl_clearance::safety_volume`]) written as georeferenced DXF, IFC
//! and KML/KMZ ([`ohl_io::safety_volume`]).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Args;
use ohl_clearance::rules::{DEFAULT_WIND_STEP, Requirement, WindSides, WindSteps};
use ohl_clearance::safety_volume::{self, DEFAULT_TOLERANCE, Grouping, SafetyParams, WindMode};
use ohl_core::budget::Budget;
use ohl_io::safety_volume::{Description, Format};

use crate::analysis_run::{CaseArgs, ModelArgs, cases, load};

/// `ohl safety-volume`.
#[derive(Args)]
pub struct SafetyArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    #[command(flatten)]
    pub cases: CaseArgs,
    /// The wind of the windy cases: `none` (blown left and right at full
    /// pressure), `steps` (also still air and every pressure by
    /// --wind-step), `arc` (also the swing arc between the blowouts) or
    /// `both`.
    #[arg(long, default_value = "none", value_parser = WindMode::parse)]
    pub wind: WindMode,
    /// Step of the wind pressure, Pa (--wind steps or both).
    #[arg(long, default_value_t = DEFAULT_WIND_STEP)]
    pub wind_step: f64,
    /// The sides the stepped wind blows from: both, left or right.
    #[arg(long, default_value = "both", value_parser = WindSides::parse)]
    pub wind_sides: WindSides,
    /// The swing band's margin, m (--wind arc or both): every distance to
    /// a band is taken this much smaller. Default 0.15 m, the measured
    /// worst excess of the intermediate pressures plus an allowance
    /// (conservative); a smaller value is on the user's responsibility.
    #[arg(long, default_value_t = ohl_clearance::positions::BAND_MARGIN)]
    pub band_margin: f64,
    /// The distance from the conductors' surface (required; none is built
    /// in): a distance (`5`, `"5 m"`), or by the voltage of each wire's
    /// system as a rule file writes it (`'{ by_voltage = [[110, 3], [380,
    /// 5]] }'`: up to kV, m; `'{ base = 3, per_kv = 0.01, above_kv = 110
    /// }'`). Earthed wires at 0 kV.
    #[arg(long, value_parser = Requirement::parse)]
    pub distance: Requirement,
    /// An extra margin added to the distance, m.
    #[arg(long, default_value_t = 0.0)]
    pub margin: f64,
    /// How far the mesh may lie outside the exact envelope where it is
    /// smooth, m (never inside it).
    #[arg(long, default_value_t = DEFAULT_TOLERANCE)]
    pub tolerance: f64,
    /// One volume per `wire` (a circuit's phase), per `circuit` or per
    /// `system` (an electrical system's circuits together; the wires
    /// without a system — earthed wires — together in one volume named
    /// "(no system)").
    #[arg(long, default_value = "wire", value_parser = Grouping::parse)]
    pub per: Grouping,
    /// Only these circuits (repeatable; default all).
    #[arg(long)]
    pub circuit: Vec<String>,
    /// Only these electrical systems, by id or name (repeatable; default
    /// all). `none` (or `"(no system)"`) takes the wires without a system
    /// (earthed wires, sections without one), which are left out when
    /// systems are given without it (a system called `none` takes
    /// precedence). With --circuit too, a wire must be in both.
    #[arg(long)]
    pub system: Vec<String>,
    /// Only these spans, `SECTION:N` with N from 1 (repeatable; default
    /// all): every wire on them (within --circuit / --system), and the
    /// jumpers whose two ends are on spans taken.
    #[arg(long, value_parser = safety_volume::parse_span)]
    pub span: Vec<(String, usize)>,
    /// The line's coordinate system, when the line file has none (or an
    /// informative one): `EPSG:<code>`.
    #[arg(long)]
    pub crs: Option<String>,
    /// Folder the files are written to.
    #[arg(long)]
    pub out: PathBuf,
    /// Formats: dxf (with a .prj), ifc, kml, kmz (comma-separated).
    #[arg(long, default_value = "dxf,ifc,kmz")]
    pub format: String,
    /// File name stem.
    #[arg(long, default_value = "safety-volume")]
    pub name: String,
    /// Exit clause: stop after this many seconds (an error, never a
    /// partial result).
    #[arg(long, default_value_t = 3600)]
    pub time_limit: u64,
}

/// Runs `ohl safety-volume`.
pub fn run(a: SafetyArgs) -> Result<(), String> {
    let formats = Format::parse_list(&a.format)?;
    let (mut line, library) = load(&a.model)?;
    let chosen = cases(&a.cases, &mut line)?;
    let params = SafetyParams {
        distance: a.distance.clone(),
        margin: a.margin,
        tolerance: a.tolerance,
        grouping: a.per,
        circuits: a.circuit.clone(),
        systems: a.system.clone(),
        spans: a.span.clone(),
        ..SafetyParams::default()
    };
    if let Some(e) = ohl_clearance::rules::band_margin_problem(a.band_margin) {
        return Err(format!("--band-margin: {e}"));
    }
    let steps = WindSteps {
        stepped: a.wind.stepped(),
        arc: false,
        step: a.wind_step,
        sides: a.wind_sides,
        band_margin: a.band_margin,
    };
    let budget = Budget::new().time_limit(Duration::from_secs(a.time_limit));
    let t = Instant::now();
    let v = safety_volume::run(&line, &library, &chosen, a.wind, steps, &params, &budget)?;
    let wind = safety_volume::wind_text(a.wind, a.band_margin);
    println!(
        "safety volumes of {}: {} weather cases, {wind}, {} positions ({:.2?})",
        line.name,
        v.cases.len(),
        v.positions,
        t.elapsed()
    );
    for s in &v.volumes {
        println!(
            "  {}: {} — d {:.2} m{} — {} triangles, {:.0} m³, outside by at most {:.3} m ({:.2} s)",
            s.name,
            s.kv.map_or("earthed".into(), |k| format!("{k} kV")),
            s.distance,
            if s.margin > 0.0 {
                format!(" + {:.2} m", s.margin)
            } else {
                String::new()
            },
            s.mesh.triangles.len(),
            s.mesh.volume(),
            s.allowance,
            s.seconds
        );
    }
    for n in &v.notes {
        println!("  note: {n}");
    }
    let crs = a.crs.clone().unwrap_or_else(|| line.crs.clone());
    let (date, _) = ohl_io::landxml::utc_now();
    let d = Description {
        line: line.name.clone(),
        crs,
        distance: format!(
            "{}{}",
            a.distance,
            if a.margin > 0.0 {
                format!(" + {:.2} m", a.margin)
            } else {
                String::new()
            }
        ),
        wind,
        date,
    };
    let (files, notes) = ohl_io::safety_volume::write(&a.out, &a.name, &v, &d, &formats)?;
    for f in files {
        println!("written to {}", f.display());
    }
    for n in notes {
        println!("  note: {n}");
    }
    Ok(())
}
