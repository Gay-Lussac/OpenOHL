// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl synth …`: the synthetic country (ADR-0013, phase 1) — generated,
//! written as a project file, and today's session measured on it.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Subcommand;
use ohl_synth::{Settings, project};

#[derive(Subcommand)]
pub enum SynthCommand {
    /// Generate a synthetic country (invented lines; the owner's size by
    /// default: 12 000 pylons, 2.5 systems each on average) into DIR:
    /// country.ohl (one project holding every route, as today's code
    /// would), summary.json (counts, the line's SHA-256, LiDAR samples
    /// and their extrapolation), and with --json the line file.
    Country {
        /// Pylons.
        #[arg(long, default_value_t = ohl_synth::DEFAULT_PYLONS)]
        pylons: usize,
        /// Electrical systems per pylon, on average.
        #[arg(long, default_value_t = ohl_synth::DEFAULT_SYSTEMS)]
        systems: f64,
        /// Seed (the same seed, the same country).
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Spans per tension section, on average.
        #[arg(long, default_value_t = 2.0)]
        section_spans: f64,
        /// Share of routes branching off another and sharing its towers.
        #[arg(long, default_value_t = 0.7)]
        branching: f64,
        /// Output folder (made if missing; country.ohl must not exist).
        #[arg(long)]
        out: PathBuf,
        /// LiDAR sample tiles: points per m² (0: none).
        #[arg(long, default_value_t = 16.0)]
        lidar_density: f64,
        /// Corridor width for the LiDAR extrapolation, m.
        #[arg(long, default_value_t = 200.0)]
        corridor: f64,
        /// Also write the line file (country.ohl.json).
        #[arg(long)]
        json: bool,
        /// User importing (the project's admin).
        #[arg(long, default_value = "synth")]
        user: String,
        /// Time limit for the LiDAR samples, s.
        #[arg(long, default_value_t = 600)]
        seconds: u64,
        /// Also write each route as a project file of its own,
        /// DIR/lines/CODE.ohl (a design line per project, for `ohl network
        /// import`; towers shared by two routes are in both files).
        #[arg(long)]
        per_line: bool,
    },
    /// Measure the session on a project file: open time, memory, whole
    /// validation, edit, undo and redo latency (structures moved by 1 m,
    /// back and again); prints JSON.
    Measure {
        /// Project file (.ohl; changed by the edits, then undone).
        project: PathBuf,
        /// Structures to move (spread over the project).
        #[arg(long, default_value_t = 5)]
        edits: usize,
        /// User (must be allowed to edit).
        #[arg(long, default_value = "synth")]
        user: String,
        /// Time limit for the edits, undos and redos, s.
        #[arg(long, default_value_t = 600)]
        seconds: u64,
        /// Every command the whole way (the line made again from every
        /// object and validated whole, as before the commands were applied
        /// in place): to compare.
        #[arg(long)]
        whole: bool,
        /// With a working set (towers shared by several routes): the user
        /// works on every other route, the moves are of towers shared with
        /// routes outside it (each marks those stale); both taken off at
        /// the end.
        #[arg(long)]
        working_set: bool,
        /// The same towers moved without a working set (to compare).
        #[arg(long, conflicts_with = "working_set")]
        shared_towers: bool,
    },
}

pub fn run(command: SynthCommand) -> Result<(), String> {
    match command {
        SynthCommand::Country {
            pylons,
            systems,
            seed,
            section_spans,
            branching,
            out,
            lidar_density,
            corridor,
            json,
            user,
            seconds,
            per_line,
        } => {
            let settings = Settings {
                pylons,
                systems,
                seed,
                section_spans,
                branching,
            };
            std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            let t = Instant::now();
            let country = ohl_synth::country(&settings)?;
            let generate_s = t.elapsed().as_secs_f64();
            let s = &country.summary;
            println!(
                "generated in {generate_s:.2} s: {} pylons ({} shared by two or more routes, {:.1} %), \
                 {} routes, {} km, {:.2} systems per pylon, {} sections, {} wire spans, {} jumpers, \
                 line file {:.1} MB",
                s.pylons,
                s.shared_pylons,
                100.0 * s.shared_share,
                s.routes,
                s.km,
                s.systems_per_pylon,
                s.sections,
                s.wire_spans,
                s.jumpers,
                s.json_bytes as f64 / 1e6
            );
            println!("line SHA-256 {}", s.hash);
            if json {
                let path = out.join("country.ohl.json");
                let text =
                    serde_json::to_string_pretty(&country.line).map_err(|e| e.to_string())?;
                std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            let path = out.join("country.ohl");
            let import_s = project::write_project(&country, &path, &user)?;
            println!("project written in {import_s:.2} s: {}", path.display());
            if per_line {
                let dir = out.join("lines");
                let t = Instant::now();
                let written = project::write_line_projects(&country, &dir, &user)?;
                println!(
                    "{} line projects written in {:.2} s: {}",
                    written.len(),
                    t.elapsed().as_secs_f64(),
                    dir.display()
                );
            }
            let (tiles, estimate) = if lidar_density > 0.0 {
                let tiles = project::add_lidar(
                    &country,
                    &path,
                    lidar_density,
                    Duration::from_secs(seconds),
                )?;
                for t in &tiles {
                    println!(
                        "LiDAR sample tile {:.0}/{:.0}: {} points in {:.2} s, {:.1} MB ({:.2} bytes a point)",
                        t.corner[0],
                        t.corner[1],
                        t.points,
                        t.seconds,
                        t.bytes as f64 / 1e6,
                        t.bytes as f64 / t.points.max(1) as f64
                    );
                }
                let e = project::lidar_estimate(&country, &tiles, lidar_density, corridor);
                println!(
                    "LiDAR of the whole country EXTRAPOLATED from {} sample tiles (not generated): \
                     {:.2e} points in a {} m corridor at {} points/m² ≈ {:.0} GB, import ≈ {:.1} h",
                    tiles.len(),
                    e.points,
                    e.corridor_m,
                    e.density,
                    e.bytes / 1e9,
                    e.points * e.seconds_per_point / 3600.0
                );
                (tiles, Some(e))
            } else {
                (Vec::new(), None)
            };
            let file_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let summary = serde_json::json!({
                "summary": country.summary,
                "generate_s": (generate_s * 1000.0).round() / 1000.0,
                "import_s": import_s,
                "file_bytes": file_bytes,
                "lidar_tiles": tiles,
                "lidar_estimate": estimate,
            });
            let path = out.join("summary.json");
            std::fs::write(
                &path,
                serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(())
        }
        SynthCommand::Measure {
            project,
            edits,
            user,
            seconds,
            whole,
            working_set,
            shared_towers,
        } => {
            let shared = match (working_set, shared_towers) {
                (true, _) => project::Shared::WorkingSet,
                (false, true) => project::Shared::TowersOnly,
                _ => project::Shared::No,
            };
            let b = project::measure_with(
                &project,
                &user,
                edits,
                Duration::from_secs(seconds),
                whole,
                shared,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&b).map_err(|e| e.to_string())?
            );
            Ok(())
        }
    }
}
