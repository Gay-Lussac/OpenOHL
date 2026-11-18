// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The synthetic country written as a project file, its LiDAR sample
//! tiles, and today's session measured on it (phase 1's baseline).

use std::path::Path;
use std::time::{Duration, Instant};

use ohl_core::budget::Budget;
use ohl_session::{Command, Session};
use serde::Serialize;

use crate::{Country, lidar_tile};

/// Seconds since `t`, rounded to the millisecond.
fn secs(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 1000.0).round() / 1000.0
}

/// Writes `country` into a new project file at `path` (refused when it
/// exists) as one import by `user`, as `ohl project import` does; returns
/// the seconds it took.
pub fn write_project(country: &Country, path: &Path, user: &str) -> Result<f64, String> {
    if path.exists() {
        return Err(format!("{}: exists already", path.display()));
    }
    let t = Instant::now();
    let mut session = Session::open(path).map_err(|e| e.to_string())?;
    session
        .import(&country.line, user)
        .map_err(|e| format!("import: {e}"))?;
    session.checkpoint().map_err(|e| e.to_string())?;
    Ok(secs(t))
}

/// One LiDAR sample tile as stored.
#[derive(Debug, Clone, Serialize)]
pub struct TileStat {
    /// Lower-left corner, m.
    pub corner: [f64; 2],
    /// Points.
    pub points: usize,
    /// Seconds to make and import it (levels of detail included).
    pub seconds: f64,
    /// Bytes of its blocks and levels of detail in the file.
    pub bytes: u64,
}

/// What the LiDAR of the whole country would be, extrapolated from the
/// sample tiles (said so wherever it is shown).
#[derive(Debug, Clone, Serialize)]
pub struct LidarEstimate {
    /// Corridor width, m (centred on the routes).
    pub corridor_m: f64,
    /// Density, points per m².
    pub density: f64,
    /// Points in the country's corridors.
    pub points: f64,
    /// Bytes per point measured on the sample tiles.
    pub bytes_per_point: f64,
    /// Bytes for the whole country.
    pub bytes: f64,
    /// Import time per point measured, s.
    pub seconds_per_point: f64,
}

/// Imports `country.sample_tiles` as point clouds `sample-1`, … into the
/// project file at `path`, `density` points per m², each tile timed and
/// sized. Exit clause: `limit` for all tiles together.
pub fn add_lidar(
    country: &Country,
    path: &Path,
    density: f64,
    limit: Duration,
) -> Result<Vec<TileStat>, String> {
    let mut store = ohl_pointcloud::Store::open(path).map_err(|e| e.to_string())?;
    let budget = Budget::new().time_limit(limit);
    let started = Instant::now();
    let mut out = Vec::new();
    for (k, corner) in country.sample_tiles.iter().enumerate() {
        if started.elapsed() > limit {
            return Err(format!(
                "LiDAR samples stopped after {} of {} tiles: over {} s",
                k,
                country.sample_tiles.len(),
                limit.as_secs()
            ));
        }
        let t = Instant::now();
        let points = lidar_tile(country.summary.seed, *corner, density);
        let n = points.len();
        let name = format!("sample-{}", k + 1);
        store
            .import_points(&name, crate::CRS, None, 64.0, 0.001, points, &budget)
            .map_err(|e| format!("LiDAR sample {name}: {e}"))?;
        let seconds = secs(t);
        out.push(TileStat {
            corner: *corner,
            points: n,
            seconds,
            bytes: 0,
        });
    }
    drop(store);
    // Sizes: the blocks' and levels' bytes per cloud.
    let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
    for (k, stat) in out.iter_mut().enumerate() {
        let bytes: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(length(b.xyz) + COALESCE(length(b.intensity), 0)
                         + COALESCE(length(b.returns), 0)), 0)
                   + COALESCE((SELECT SUM(length(l.xyz)) FROM pointcloud_lod l
                       JOIN pointcloud_block b2 ON l.block = b2.id
                       JOIN pointcloud c2 ON b2.cloud = c2.id WHERE c2.name = ?1), 0)
                 FROM pointcloud_block b JOIN pointcloud c ON b.cloud = c.id WHERE c.name = ?1",
                [format!("sample-{}", k + 1)],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        stat.bytes = bytes.max(0) as u64;
    }
    Ok(out)
}

/// The country's LiDAR extrapolated from the sample tiles: the routes'
/// length times `corridor_m` at `density`.
pub fn lidar_estimate(
    country: &Country,
    tiles: &[TileStat],
    density: f64,
    corridor_m: f64,
) -> LidarEstimate {
    let points: usize = tiles.iter().map(|t| t.points).sum();
    let bytes: u64 = tiles.iter().map(|t| t.bytes).sum();
    let seconds: f64 = tiles.iter().map(|t| t.seconds).sum();
    let per_point = if points > 0 {
        bytes as f64 / points as f64
    } else {
        0.0
    };
    let country_points = country.summary.km * 1000.0 * corridor_m * density;
    LidarEstimate {
        corridor_m,
        density,
        points: country_points,
        bytes_per_point: (per_point * 100.0).round() / 100.0,
        bytes: country_points * per_point,
        seconds_per_point: if points > 0 {
            seconds / points as f64
        } else {
            0.0
        },
    }
}

/// Today's session measured on a project file (phase 1's baseline).
#[derive(Debug, Clone, Serialize)]
pub struct Baseline {
    /// Project file bytes (after a checkpoint).
    pub file_bytes: u64,
    /// Seconds to open it (`Session::open`: every object read, the line
    /// built and validated, the spatial index read).
    pub open_s: f64,
    /// Resident memory of this process after opening, MB (`ps`).
    pub rss_after_open_mb: f64,
    /// Seconds to validate the whole line once (`Line::validate`).
    pub validate_s: f64,
    /// Seconds per edit: a structure moved by 1 m (each a logged command).
    pub move_s: Vec<f64>,
    /// Seconds per undo of those moves.
    pub undo_s: Vec<f64>,
    /// Resident memory after the edits, MB.
    pub rss_after_edits_mb: f64,
    /// Edits stopped by the time limit (said in the report).
    pub stopped: Option<String>,
}

/// The resident memory of this process, MB (`ps -o rss=`; 0 when `ps` is
/// missing).
pub fn rss_mb() -> f64 {
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map_or(0.0, |kb| (kb / 1024.0 * 10.0).round() / 10.0)
}

/// Opens the project at `path` with today's session and times it, then
/// moves `edits` structures (spread over the country) by 1 m each and
/// undoes them, as `user`. Exit clause: `limit` for the edits and undos
/// together — stopped there, the report says so (opening itself is
/// timed whole: it cannot be stopped from outside the session).
pub fn measure(path: &Path, user: &str, edits: usize, limit: Duration) -> Result<Baseline, String> {
    let file_bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let t = Instant::now();
    let mut session = Session::open(path).map_err(|e| e.to_string())?;
    let open_s = secs(t);
    let rss_after_open_mb = rss_mb();
    let t = Instant::now();
    session.line().validate().map_err(|e| e.to_string())?;
    let validate_s = secs(t);
    let picks: Vec<(String, [f64; 3])> = {
        let s = &session.line().structures;
        let n = edits.min(s.len());
        (0..n)
            .map(|k| {
                let x = &s[(k * s.len() / n + 1).min(s.len() - 1)];
                (x.id.clone(), x.position)
            })
            .collect()
    };
    let started = Instant::now();
    let mut move_s = Vec::new();
    let mut undo_s = Vec::new();
    let mut stopped = None;
    for (id, p) in &picks {
        if started.elapsed() > limit {
            stopped = Some(format!(
                "edits stopped after {} of {}: over {} s",
                move_s.len(),
                picks.len(),
                limit.as_secs()
            ));
            break;
        }
        let t = Instant::now();
        session
            .apply(
                user,
                Command::MoveStructure {
                    id: id.clone(),
                    position: [p[0] + 1.0, p[1], p[2]],
                },
            )
            .map_err(|e| format!("move {id}: {e}"))?;
        move_s.push(secs(t));
    }
    for _ in 0..move_s.len() {
        if started.elapsed() > limit {
            stopped.get_or_insert_with(|| {
                format!(
                    "undos stopped after {}: over {} s",
                    undo_s.len(),
                    limit.as_secs()
                )
            });
            break;
        }
        let t = Instant::now();
        session.undo(user).map_err(|e| format!("undo: {e}"))?;
        undo_s.push(secs(t));
    }
    let rss_after_edits_mb = rss_mb();
    Ok(Baseline {
        file_bytes,
        open_s,
        rss_after_open_mb,
        validate_s,
        move_s,
        undo_s,
        rss_after_edits_mb,
        stopped,
    })
}
