// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Classification of unclassified points (a base to build on): ground,
//! then classes by height above the ground.
//!
//! **Ground**: a progressive morphological filter (K. Zhang, S.-C. Chen,
//! D. Whitman, M.-L. Shyu, J. Yan, C. Zhang, "A progressive morphological
//! filter for removing nonground measurements from airborne LIDAR data",
//! IEEE TGRS 41(4), 2003) on a grid of the lowest point per cell: the
//! surface is opened (erosion then dilation, square windows) with windows
//! growing 3, 5, 9, 17… cells up to `max_window`; a cell whose lowest point
//! stands above the opened surface by more than the threshold of that
//! window — `initial_threshold + slope · (window growth) · cell`, at most
//! `max_threshold` — holds no ground. The lowest points of the remaining
//! cells make a TIN.
//!
//! **Heights**: every point to classify is measured above that TIN: within
//! `ground_tolerance` it is ground; above, the first of `bands` (up to a
//! height, class) it falls in — by default the ASPRS LAS 1.4 classes 3, 4
//! and 5 (low, medium, high vegetation) — and above the last band
//! `above` (high noise, 18); below the ground by more than the tolerance,
//! `below` (low noise, 7). Buildings, wires and poles get vegetation
//! classes too: telling them apart is not part of this base.
//!
//! Only points of the classes in `reclassify` (default 0 and 1: never
//! classified, unclassified) are changed and used; other classes are kept.
//!
//! Cost: the grid filter about 140 operations per cell (1 m cells, 17 m
//! windows), the TIN of the ground cells, then a TIN lookup per point —
//! measured (release build, one core) 4 million points over 10⁶ cells in
//! 1.2 s, about 3 million points a second. Points are held in memory (32
//! bytes each). Exit clauses: `budget` checked per grid row
//! and per 2²⁰ points; grids above `max_cells` are refused.

use ohl_core::budget::Budget;
use ohl_terrain::Terrain;

use crate::{LidarError, LidarPoint};

/// What [`classify`] does.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifyParams {
    /// Grid cell, m.
    pub cell: f64,
    /// Largest window, m: objects up to about this size are removed.
    pub max_window: f64,
    /// Terrain slope tolerated (rise over run).
    pub slope: f64,
    /// Height threshold of the first window, m.
    pub initial_threshold: f64,
    /// Largest height threshold, m.
    pub max_threshold: f64,
    /// Points within this height of the ground TIN are ground, m.
    pub ground_tolerance: f64,
    /// Longest TIN triangle side, m (no ground across larger gaps).
    pub max_edge: f64,
    /// Ground class.
    pub ground_class: u8,
    /// Height bands above the ground: (up to m, class), heights increasing.
    pub bands: Vec<(f64, u8)>,
    /// Class above the last band.
    pub above: u8,
    /// Class below the ground.
    pub below: u8,
    /// Classes of the points classified (others are kept).
    pub reclassify: Vec<u8>,
    /// Largest grid, cells.
    pub max_cells: usize,
}

impl Default for ClassifyParams {
    fn default() -> Self {
        Self {
            cell: 1.0,
            max_window: 20.0,
            slope: 0.3,
            initial_threshold: 0.3,
            max_threshold: 3.0,
            ground_tolerance: 0.3,
            max_edge: 50.0,
            ground_class: 2,
            bands: vec![(0.5, 3), (2.0, 4), (60.0, 5)],
            above: 18,
            below: 7,
            reclassify: vec![0, 1],
            max_cells: 100_000_000,
        }
    }
}

/// What [`classify`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifyStats {
    /// Points classified.
    pub classified: usize,
    /// Points kept as they were (other classes, or no ground below).
    pub kept: usize,
    /// Points of each new class (index: class).
    pub counts: Vec<u64>,
    /// Grid cells with points, and how many of them hold ground.
    pub cells: usize,
    /// Cells holding ground.
    pub ground_cells: usize,
}

/// Square grid of the lowest point per cell.
struct Grid {
    nx: usize,
    ny: usize,
    x0: f64,
    y0: f64,
    cell: f64,
    /// Lowest point per cell (`INFINITY` z: empty).
    low: Vec<[f64; 3]>,
}

impl Grid {
    fn index(&self, x: f64, y: f64) -> usize {
        let i = (((x - self.x0) / self.cell) as usize).min(self.nx - 1);
        let j = (((y - self.y0) / self.cell) as usize).min(self.ny - 1);
        j * self.nx + i
    }
}

/// Minimum (`max` false) or maximum filter over a square window of
/// radius `r` cells, separable: rows, then columns; `INFINITY` values
/// (empty cells) are skipped.
fn filter(
    values: &[f64],
    nx: usize,
    ny: usize,
    r: usize,
    max: bool,
    run: &ohl_core::budget::Run,
) -> Result<Vec<f64>, LidarError> {
    let stop = |e: ohl_core::budget::Stop| LidarError::Stopped(e.to_string());
    let pick = |a: f64, b: f64| {
        if b.is_infinite() {
            a
        } else if a.is_infinite() {
            b
        } else if max {
            a.max(b)
        } else {
            a.min(b)
        }
    };
    let mut rows = vec![f64::INFINITY; values.len()];
    for j in 0..ny {
        run.check().map_err(stop)?;
        let row = &values[j * nx..(j + 1) * nx];
        for i in 0..nx {
            let (lo, hi) = (i.saturating_sub(r), (i + r).min(nx - 1));
            rows[j * nx + i] = row[lo..=hi].iter().fold(f64::INFINITY, |a, &b| pick(a, b));
        }
    }
    let mut out = vec![f64::INFINITY; values.len()];
    for j in 0..ny {
        run.check().map_err(stop)?;
        let (lo, hi) = (j.saturating_sub(r), (j + r).min(ny - 1));
        for i in 0..nx {
            out[j * nx + i] = (lo..=hi).fold(f64::INFINITY, |a, k| pick(a, rows[k * nx + i]));
        }
    }
    Ok(out)
}

/// Classifies `points` in place (see the module). Exit clause: `budget`.
pub fn classify(
    points: &mut [LidarPoint],
    params: &ClassifyParams,
    budget: &Budget,
) -> Result<ClassifyStats, LidarError> {
    let run = budget.start();
    let stop = |e: ohl_core::budget::Stop| LidarError::Stopped(e.to_string());
    let bad = |m: &str| LidarError::Invalid(format!("classification: {m}"));
    if !(params.cell > 0.0 && params.max_window >= params.cell) {
        return Err(bad(
            "the cell must be positive and the window at least a cell",
        ));
    }
    if params.bands.windows(2).any(|w| w[1].0 <= w[0].0) {
        return Err(bad("height bands must increase"));
    }
    let mut wanted = [false; 256];
    for &c in &params.reclassify {
        wanted[c as usize] = true;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in points.iter().filter(|p| wanted[p.class as usize]) {
        lo = [lo[0].min(p.x), lo[1].min(p.y)];
        hi = [hi[0].max(p.x), hi[1].max(p.y)];
    }
    let mut stats = ClassifyStats {
        classified: 0,
        kept: points.len(),
        counts: vec![0; 256],
        cells: 0,
        ground_cells: 0,
    };
    if lo[0] > hi[0] {
        return Ok(stats);
    }
    let nx = ((hi[0] - lo[0]) / params.cell) as usize + 1;
    let ny = ((hi[1] - lo[1]) / params.cell) as usize + 1;
    if nx.saturating_mul(ny) > params.max_cells {
        return Err(bad(&format!(
            "{nx} × {ny} cells is more than {}: classify a smaller area or use larger cells",
            params.max_cells
        )));
    }
    let mut grid = Grid {
        nx,
        ny,
        x0: lo[0],
        y0: lo[1],
        cell: params.cell,
        low: vec![[0.0, 0.0, f64::INFINITY]; nx * ny],
    };
    for chunk in points.chunks(1 << 20) {
        run.check().map_err(stop)?;
        for p in chunk.iter().filter(|p| wanted[p.class as usize]) {
            let k = grid.index(p.x, p.y);
            if p.z < grid.low[k][2] {
                grid.low[k] = [p.x, p.y, p.z];
            }
        }
    }
    // Progressive opening: windows 2r + 1 for r = 1, 2, 4, 8…
    let lowest: Vec<f64> = grid.low.iter().map(|p| p[2]).collect();
    let mut surface = lowest.clone();
    let mut ground = vec![true; nx * ny];
    let (mut r, mut last_window) = (1usize, 1.0);
    while (2 * r + 1) as f64 * params.cell <= params.max_window + 1e-9 {
        let window = (2 * r + 1) as f64;
        let threshold = if r == 1 {
            params.initial_threshold
        } else {
            (params.slope * (window - last_window) * params.cell + params.initial_threshold)
                .min(params.max_threshold)
        };
        let eroded = filter(&surface, nx, ny, r, false, &run)?;
        let opened = filter(&eroded, nx, ny, r, true, &run)?;
        for k in 0..nx * ny {
            if lowest[k].is_finite() && opened[k].is_finite() && lowest[k] - opened[k] > threshold {
                ground[k] = false;
            }
        }
        surface = opened;
        last_window = window;
        r *= 2;
    }
    let ground_points: Vec<[f64; 3]> = grid
        .low
        .iter()
        .zip(&ground)
        .filter(|(p, g)| **g && p[2].is_finite())
        .map(|(p, _)| *p)
        .collect();
    stats.cells = lowest.iter().filter(|z| z.is_finite()).count();
    stats.ground_cells = ground_points.len();
    run.check().map_err(stop)?;
    let tin =
        Terrain::from_xyz(&ground_points, Some(params.max_edge)).map_err(LidarError::Terrain)?;
    for chunk in points.chunks_mut(1 << 20) {
        run.check().map_err(stop)?;
        for p in chunk.iter_mut().filter(|p| wanted[p.class as usize]) {
            let Some(g) = tin.height(p.x, p.y) else {
                continue;
            };
            let h = p.z - g;
            p.class = if h.abs() <= params.ground_tolerance {
                params.ground_class
            } else if h < 0.0 {
                params.below
            } else {
                params
                    .bands
                    .iter()
                    .find(|(up_to, _)| h <= *up_to)
                    .map_or(params.above, |b| b.1)
            };
            stats.counts[p.class as usize] += 1;
            stats.classified += 1;
        }
    }
    stats.kept = points.len() - stats.classified;
    Ok(stats)
}

/// Writes points to a LAS file, or LAZ when `path` ends in `.laz`: LAS
/// 1.4, point format 6 (classes 0–255), 1 mm coordinates. Exit clause:
/// `budget`, checked every 2²⁰ points; an incomplete file is removed.
pub fn write_las(
    path: &std::path::Path,
    points: &[LidarPoint],
    budget: &Budget,
) -> Result<(), LidarError> {
    let file_error = |e: String| LidarError::File(path.to_path_buf(), e);
    let run = budget.start();
    let scale = 0.001;
    let low = |f: fn(&LidarPoint) -> f64| {
        let v = points.iter().map(f).fold(f64::INFINITY, f64::min);
        if v.is_finite() {
            (v / scale).floor() * scale
        } else {
            0.0
        }
    };
    let mut builder = las::Builder::from((1, 4));
    builder.point_format = las::point::Format::new(6).map_err(|e| file_error(e.to_string()))?;
    builder.point_format.is_compressed = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("laz"));
    builder.generating_software = format!("OpenOHL {}", env!("CARGO_PKG_VERSION"));
    let offsets = [low(|p| p.x), low(|p| p.y), low(|p| p.z)];
    for (t, o) in [
        &mut builder.transforms.x,
        &mut builder.transforms.y,
        &mut builder.transforms.z,
    ]
    .into_iter()
    .zip(offsets)
    {
        t.scale = scale;
        t.offset = o;
    }
    let header = builder
        .into_header()
        .map_err(|e| file_error(e.to_string()))?;
    let mut writer = las::Writer::from_path(path, header).map_err(|e| file_error(e.to_string()))?;
    let result = (|| -> Result<(), LidarError> {
        for chunk in points.chunks(1 << 20) {
            run.check()
                .map_err(|e| LidarError::Stopped(e.to_string()))?;
            for p in chunk {
                let classification = las::point::Classification::new(p.class)
                    .unwrap_or(las::point::Classification::Reserved(p.class));
                writer
                    .write_point(las::Point {
                        x: p.x,
                        y: p.y,
                        z: p.z,
                        intensity: p.intensity,
                        return_number: p.return_number.max(1),
                        number_of_returns: p.number_of_returns.max(1),
                        classification,
                        gps_time: Some(0.0),
                        ..Default::default()
                    })
                    .map_err(|e| file_error(e.to_string()))?;
            }
        }
        writer.close().map_err(|e| file_error(e.to_string()))
    })();
    if result.is_err() {
        drop(writer);
        let _ = std::fs::remove_file(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64, z: f64) -> LidarPoint {
        LidarPoint {
            x,
            y,
            z,
            class: 1,
            intensity: 0,
            return_number: 1,
            number_of_returns: 1,
        }
    }

    /// Rolling ground: a slope of 5 % and 2 %, with a 1 m wave.
    fn ground_z(x: f64, y: f64) -> f64 {
        100.0 + 0.05 * x + 0.02 * y + (x / 15.0).sin() * (y / 20.0).cos()
    }

    /// A synthetic scene: ground every 0.5 m over 120 × 120 m; low
    /// vegetation (0.3 m), shrubs (1.5 m) and trees (8–20 m, crowns of
    /// points from 4 m up, no ground under the crown's centre); a 15 m
    /// square building with an 8 m roof and no points under it. All
    /// unclassified. Returns the points and their true classes.
    fn scene() -> (Vec<LidarPoint>, Vec<u8>) {
        let (mut pts, mut truth) = (Vec::new(), Vec::new());
        let building = |x: f64, y: f64| (60.0..75.0).contains(&x) && (20.0..35.0).contains(&y);
        let trees: Vec<(f64, f64, f64)> = (0..12)
            .map(|k| {
                let k = k as f64;
                (10.0 + 8.0 * k, 80.0 + 3.0 * (k % 3.0), 8.0 + k)
            })
            .collect();
        let under_crown = |x: f64, y: f64| trees.iter().any(|t| (x - t.0).hypot(y - t.1) < 1.0);
        for i in 0..240 {
            for j in 0..240 {
                let (x, y) = (i as f64 * 0.5, j as f64 * 0.5);
                if building(x, y) {
                    pts.push(point(x, y, ground_z(70.0, 30.0) + 8.0));
                    truth.push(6);
                    continue;
                }
                if under_crown(x, y) {
                    continue;
                }
                pts.push(point(x, y, ground_z(x, y)));
                truth.push(2);
                if (i + j) % 7 == 0 && y < 60.0 {
                    pts.push(point(x + 0.1, y, ground_z(x, y) + 0.4));
                    truth.push(3);
                }
                if (i * 3 + j) % 11 == 0 && y < 60.0 && x > 80.0 {
                    pts.push(point(x, y + 0.1, ground_z(x, y) + 1.5));
                    truth.push(4);
                }
            }
        }
        for (cx, cy, h) in &trees {
            let g = ground_z(*cx, *cy);
            for a in 0..36 {
                for r in 0..4 {
                    let (s, c) = (a as f64 * 10.0f64).to_radians().sin_cos();
                    let rad = 0.5 + r as f64;
                    let z = g + h - rad * 0.8;
                    pts.push(point(cx + rad * c, cy + rad * s, z.max(g + 4.0)));
                    truth.push(5);
                }
            }
        }
        (pts, truth)
    }

    #[test]
    fn ground_and_vegetation_of_a_synthetic_scene() {
        let (mut pts, truth) = scene();
        let stats = classify(&mut pts, &ClassifyParams::default(), &Budget::new()).unwrap();
        assert!(
            stats.ground_cells > 0 && stats.classified > pts.len() * 99 / 100,
            "{stats:?}"
        );
        let agree = |class: u8, got: &[u8]| {
            let of: Vec<usize> = (0..truth.len()).filter(|&k| truth[k] == class).collect();
            of.iter().filter(|&&k| got.contains(&pts[k].class)).count() as f64 / of.len() as f64
        };
        let ground = agree(2, &[2]);
        let low = agree(3, &[3]);
        let medium = agree(4, &[4]);
        let high = agree(5, &[5]);
        // The roof is not ground (it gets a vegetation class: see the
        // module).
        let roof = agree(6, &[3, 4, 5]);
        assert!(ground > 0.99, "ground {ground}");
        assert!(
            low > 0.95 && medium > 0.95 && high > 0.99,
            "{low} {medium} {high}"
        );
        assert!(roof > 0.99, "roof {roof}");
        // Points of other classes are kept.
        let mut wire = vec![LidarPoint {
            class: 14,
            ..point(50.0, 50.0, 130.0)
        }];
        wire.extend(scene().0);
        classify(&mut wire, &ClassifyParams::default(), &Budget::new()).unwrap();
        assert_eq!(wire[0].class, 14);
        // Exit clauses: time, and too many cells.
        let budget = Budget::new().time_limit(std::time::Duration::ZERO);
        assert!(classify(&mut scene().0, &ClassifyParams::default(), &budget).is_err());
        let tiny = ClassifyParams {
            max_cells: 100,
            ..ClassifyParams::default()
        };
        assert!(classify(&mut scene().0, &tiny, &Budget::new()).is_err());
    }

    #[test]
    fn classified_points_are_written_and_read_back() {
        let dir = std::env::temp_dir().join(format!("ohl-classify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.laz");
        let pts = vec![
            LidarPoint {
                class: 2,
                intensity: 7,
                ..point(1.0, 2.0, 3.0)
            },
            LidarPoint {
                class: 42,
                ..point(1.5, 2.5, 13.25)
            },
        ];
        write_las(&path, &pts, &Budget::new()).unwrap();
        let mut back = Vec::new();
        crate::for_each_record(std::slice::from_ref(&path), None, &Budget::new(), |p| {
            back.push(*p)
        })
        .unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(
            (back[0].class, back[0].intensity, back[1].class),
            (2, 7, 42)
        );
        assert!((back[1].z - 13.25).abs() < 1e-9);
        std::fs::remove_dir_all(dir).ok();
    }
}
