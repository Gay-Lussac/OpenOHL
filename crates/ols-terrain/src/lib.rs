// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Terrain: survey points, a triangulated ground surface (TIN), ground
//! heights and profiles.
//!
//! - [`read_points`]: survey points from CSV / XYZ text (comma, semicolon,
//!   tab or space separated; optional header naming the columns), with an
//!   optional feature code and breakline id per point.
//! - [`Terrain`]: constrained Delaunay triangulation of the points
//!   (breaklines are edges the triangulation must keep: ridges, ditches,
//!   road edges), [`Terrain::height`] by linear interpolation on the
//!   triangle under a point, and [`Terrain::profile`] along a segment with a
//!   point at **every** crossing of a triangle edge, so the profile is the
//!   exact TIN surface (no sampling step to choose, no missed ridges).
//!
//! Coordinates are metres in the line's coordinate system (x east, y north,
//! z up). Cost: triangulation O(n log n) (about 1 s for 10⁶ points),
//! height queries O(√n) worst case by walking, profiles proportional to the
//! number of triangles crossed. Large point clouds (LiDAR) will get a tiled,
//! streamed index instead (DESIGN_GOALS §5).

use std::fmt;

use spade::{
    ConstrainedDelaunayTriangulation, HasPosition, Point2, PositionInTriangulation, Triangulation,
};

/// One survey point.
#[derive(Debug, Clone, PartialEq)]
pub struct SurveyPoint {
    /// Easting, m.
    pub x: f64,
    /// Northing, m.
    pub y: f64,
    /// Elevation, m.
    pub z: f64,
    /// Feature code (e.g. `"GND"`, `"ROAD"`, `"TREE"`); may be empty.
    pub code: String,
    /// Breakline id: consecutive points (in file order) with the same
    /// non-empty id are joined by breakline edges.
    pub line: String,
}

/// Errors reading or building terrain.
#[derive(Debug, Clone, PartialEq)]
pub enum TerrainError {
    /// A line of the input could not be read.
    Parse {
        /// 1-based line number.
        line: usize,
        /// What is wrong.
        message: String,
    },
    /// Fewer than three points, or all points on a line.
    Degenerate(String),
}

impl fmt::Display for TerrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { line, message } => write!(f, "survey line {line}: {message}"),
            Self::Degenerate(m) => write!(f, "terrain: {m}"),
        }
    }
}

impl std::error::Error for TerrainError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    X,
    Y,
    Z,
    Code,
    Line,
    Ignored,
}

fn column_of(name: &str) -> Column {
    match name.trim().to_ascii_lowercase().as_str() {
        "x" | "e" | "east" | "easting" => Column::X,
        "y" | "n" | "north" | "northing" => Column::Y,
        "z" | "h" | "elev" | "elevation" | "height" => Column::Z,
        "code" | "d" | "desc" | "description" | "feature" => Column::Code,
        "line" | "breakline" | "string" => Column::Line,
        _ => Column::Ignored,
    }
}

fn split_fields(line: &str) -> Vec<&str> {
    if line.contains([',', ';', '\t']) {
        line.split([',', ';', '\t']).map(str::trim).collect()
    } else {
        line.split_whitespace().collect()
    }
}

/// Reads survey points.
///
/// Without a header the columns are `x y z [code] [line]`. A first line
/// whose first field is not a number is a header naming the columns:
/// `x|e|easting`, `y|n|northing`, `z|h|elevation`, `code|d|description`,
/// `line|breakline`; other columns (e.g. a point number `p`) are ignored —
/// so PNEZD files work with a `p,n,e,z,d` header. Empty lines and lines
/// starting with `#` are skipped.
pub fn read_points(text: &str) -> Result<Vec<SurveyPoint>, TerrainError> {
    let default = [Column::X, Column::Y, Column::Z, Column::Code, Column::Line];
    let mut columns: Option<Vec<Column>> = None;
    let mut points = Vec::new();
    for (k, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = split_fields(line);
        if columns.is_none() && points.is_empty() && fields[0].parse::<f64>().is_err() {
            let named: Vec<Column> = fields.iter().map(|f| column_of(f)).collect();
            for needed in [Column::X, Column::Y, Column::Z] {
                if !named.contains(&needed) {
                    return Err(TerrainError::Parse {
                        line: k + 1,
                        message: format!("header has no {needed:?} column"),
                    });
                }
            }
            columns = Some(named);
            continue;
        }
        let cols = columns.as_deref().unwrap_or(&default);
        let mut point = SurveyPoint {
            x: f64::NAN,
            y: f64::NAN,
            z: f64::NAN,
            code: String::new(),
            line: String::new(),
        };
        for (field, column) in fields.iter().zip(cols) {
            let number = || {
                field.parse::<f64>().map_err(|_| TerrainError::Parse {
                    line: k + 1,
                    message: format!("\"{field}\" is not a number"),
                })
            };
            match column {
                Column::X => point.x = number()?,
                Column::Y => point.y = number()?,
                Column::Z => point.z = number()?,
                Column::Code => point.code = (*field).to_owned(),
                Column::Line => point.line = (*field).to_owned(),
                Column::Ignored => {}
            }
        }
        if ![point.x, point.y, point.z].iter().all(|v| v.is_finite()) {
            return Err(TerrainError::Parse {
                line: k + 1,
                message: "needs finite x, y and z".into(),
            });
        }
        points.push(point);
    }
    Ok(points)
}

#[derive(Debug, Clone, Copy)]
struct Vertex {
    position: Point2<f64>,
    z: f64,
}

impl HasPosition for Vertex {
    type Scalar = f64;

    fn position(&self) -> Point2<f64> {
        self.position
    }
}

/// A point on the ground along a profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundPoint {
    /// Fraction of the profile segment, 0–1.
    pub t: f64,
    /// Position on the ground, m.
    pub point: [f64; 3],
}

/// Most grid cells of the triangle index (64 Mi, 256 MB of offsets);
/// beyond this the cells are made larger.
const MAX_CELLS: f64 = 67_108_864.0;

/// Uniform grid over the mesh: for every cell, the triangles whose bounding
/// box overlaps it (compressed rows: `start[c]..start[c + 1]` in `items`).
#[derive(Debug, Clone, Default)]
struct Grid {
    origin: [f64; 2],
    cell: f64,
    nx: usize,
    ny: usize,
    start: Vec<u32>,
    items: Vec<u32>,
}

impl Grid {
    fn build(points: &[[f64; 3]], triangles: &[[u32; 3]]) -> Self {
        if triangles.is_empty() {
            return Self::default();
        }
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        let mut edge_sum = 0.0;
        for t in triangles {
            for &k in t {
                let p = points[k as usize];
                for i in 0..2 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
            let (a, b) = (points[t[0] as usize], points[t[1] as usize]);
            edge_sum += (b[0] - a[0]).hypot(b[1] - a[1]);
        }
        // Cells about twice the mean edge: a few triangles per cell. Mean
        // edge, not area / count: corridor data fills a small part of its
        // bounding box.
        let mean_edge = (edge_sum / triangles.len() as f64).max(1e-6);
        let (w, h) = ((hi[0] - lo[0]).max(1e-9), (hi[1] - lo[1]).max(1e-9));
        let cell = (2.0 * mean_edge).max((w * h / MAX_CELLS).sqrt());
        let nx = (w / cell).floor() as usize + 1;
        let ny = (h / cell).floor() as usize + 1;
        let mut grid = Self {
            origin: lo,
            cell,
            nx,
            ny,
            start: vec![0; nx * ny + 1],
            items: Vec::new(),
        };
        // Two passes: count, then fill.
        for pass in 0..2 {
            let mut fill = if pass == 1 {
                let mut acc = 0u32;
                for c in grid.start.iter_mut() {
                    let n = *c;
                    *c = acc;
                    acc += n;
                }
                grid.items = vec![0; acc as usize];
                grid.start.clone()
            } else {
                Vec::new()
            };
            for (k, t) in triangles.iter().enumerate() {
                let (i0, i1, j0, j1) = grid.cell_range(t.map(|v| points[v as usize]));
                for j in j0..=j1 {
                    for i in i0..=i1 {
                        let c = j * nx + i;
                        if pass == 0 {
                            grid.start[c] += 1;
                        } else {
                            grid.items[fill[c] as usize] = k as u32;
                            fill[c] += 1;
                        }
                    }
                }
            }
        }
        grid
    }

    fn index(&self, v: f64, axis: usize, n: usize) -> usize {
        (((v - self.origin[axis]) / self.cell).floor().max(0.0) as usize).min(n - 1)
    }

    fn cell_range(&self, t: [[f64; 3]; 3]) -> (usize, usize, usize, usize) {
        let min = |a: usize| t[0][a].min(t[1][a]).min(t[2][a]);
        let max = |a: usize| t[0][a].max(t[1][a]).max(t[2][a]);
        (
            self.index(min(0), 0, self.nx),
            self.index(max(0), 0, self.nx),
            self.index(min(1), 1, self.ny),
            self.index(max(1), 1, self.ny),
        )
    }

    fn cell_items(&self, i: usize, j: usize) -> &[u32] {
        let c = j * self.nx + i;
        &self.items[self.start[c] as usize..self.start[c + 1] as usize]
    }

    /// Triangles near the segment a–b (sorted, unique): the cells met when
    /// stepping along it at a quarter cell, and their neighbours across
    /// each step.
    fn along(&self, a: [f64; 2], b: [f64; 2]) -> Vec<u32> {
        if self.nx == 0 {
            return Vec::new();
        }
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let steps = ((length / (0.25 * self.cell)).ceil() as usize).max(1);
        let mut cells = Vec::new();
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            let (x, y) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
            let (i, j) = (self.index(x, 0, self.nx), self.index(y, 1, self.ny));
            cells.push((i, j));
        }
        cells.dedup();
        let mut found: Vec<u32> = Vec::new();
        for w in cells
            .windows(2)
            .chain(std::iter::once(&cells[cells.len() - 1..]))
        {
            let (i, j) = w[0];
            found.extend_from_slice(self.cell_items(i, j));
            if let Some(&(i2, j2)) = w.get(1)
                && i2 != i
                && j2 != j
            {
                // Diagonal step: the two cells beside the corner too.
                found.extend_from_slice(self.cell_items(i2, j));
                found.extend_from_slice(self.cell_items(i, j2));
            }
        }
        found.sort_unstable();
        found.dedup();
        found
    }
}

/// A triangulated ground surface (a TIN): points and triangles, from a
/// Delaunay triangulation of survey points ([`Terrain::from_points`]) or
/// given as a mesh ([`Terrain::from_mesh`]: imported TINs, terrain-building
/// plugins). The surface may have holes (triangles removed, e.g. longer
/// than a maximum side): there is no ground there.
pub struct Terrain {
    points: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
    grid: Grid,
    skipped_breaklines: usize,
}

impl fmt::Debug for Terrain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Terrain")
            .field("points", &self.points.len())
            .field("triangles", &self.triangles.len())
            .finish()
    }
}

/// Signed doubled area of a triangle in plan.
fn area2(p: [f64; 3], q: [f64; 3], r: [f64; 3]) -> f64 {
    (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
}

impl Terrain {
    /// Triangulates survey points (constrained Delaunay). Points at the same
    /// x, y are merged (the first one is kept). Breakline edges that would
    /// cross an earlier breakline are skipped and counted
    /// ([`Terrain::skipped_breaklines`]). With `max_edge`, triangles with a
    /// side longer than it (in plan) are left out — no ground across gaps
    /// in the data or outside a concave survey area.
    ///
    /// Cost: O(n log n) — about 1 s per million points.
    pub fn from_points(
        points: &[SurveyPoint],
        max_edge: Option<f64>,
    ) -> Result<Self, TerrainError> {
        let has_breaklines = points
            .windows(2)
            .any(|w| !w[0].line.is_empty() && w[0].line == w[1].line);
        let vertex = |p: &SurveyPoint| Vertex {
            position: Point2::new(p.x, p.y),
            z: p.z,
        };
        let insert_error = |e: spade::InsertionError| TerrainError::Degenerate(format!("{e:?}"));
        let mut skipped_breaklines = 0;
        let tin = if has_breaklines {
            let mut tin = ConstrainedDelaunayTriangulation::<Vertex>::new();
            let mut handles = Vec::with_capacity(points.len());
            for p in points {
                let v = vertex(p);
                let handle = match tin.locate(v.position) {
                    PositionInTriangulation::OnVertex(existing) => existing,
                    _ => tin.insert(v).map_err(insert_error)?,
                };
                handles.push(handle);
            }
            for k in 1..points.len() {
                let (a, b) = (&points[k - 1], &points[k]);
                if a.line.is_empty() || a.line != b.line || handles[k - 1] == handles[k] {
                    continue;
                }
                if tin
                    .try_add_constraint(handles[k - 1], handles[k])
                    .is_empty()
                    && !tin.exists_constraint(handles[k - 1], handles[k])
                {
                    skipped_breaklines += 1;
                }
            }
            tin
        } else {
            // Bulk loading is several times faster for large point sets.
            ConstrainedDelaunayTriangulation::<Vertex>::bulk_load(
                points.iter().map(vertex).collect(),
            )
            .map_err(insert_error)?
        };
        let mut terrain = Self::from_triangulation(&tin, max_edge)?;
        terrain.skipped_breaklines = skipped_breaklines;
        Ok(terrain)
    }

    /// Triangulates plain points (x, y, z) without codes or breaklines —
    /// e.g. LiDAR ground points — like [`Terrain::from_points`].
    pub fn from_xyz(points: &[[f64; 3]], max_edge: Option<f64>) -> Result<Self, TerrainError> {
        let vertices = points
            .iter()
            .map(|p| Vertex {
                position: Point2::new(p[0], p[1]),
                z: p[2],
            })
            .collect();
        let tin = ConstrainedDelaunayTriangulation::<Vertex>::bulk_load(vertices)
            .map_err(|e| TerrainError::Degenerate(format!("{e:?}")))?;
        Self::from_triangulation(&tin, max_edge)
    }

    fn from_triangulation(
        tin: &ConstrainedDelaunayTriangulation<Vertex>,
        max_edge: Option<f64>,
    ) -> Result<Self, TerrainError> {
        if tin.num_inner_faces() == 0 {
            return Err(TerrainError::Degenerate(format!(
                "{} distinct points do not span a surface (need three not on a line)",
                tin.num_vertices()
            )));
        }
        let mesh_points: Vec<[f64; 3]> = tin
            .vertices()
            .map(|v| [v.position().x, v.position().y, v.data().z])
            .collect();
        let limit2 = max_edge.map_or(f64::INFINITY, |m| m * m);
        let triangles: Vec<[u32; 3]> = tin
            .inner_faces()
            .map(|f| f.vertices().map(|v| v.fix().index() as u32))
            .filter(|t| {
                (0..3).all(|k| {
                    let (a, b) = (
                        mesh_points[t[k] as usize],
                        mesh_points[t[(k + 1) % 3] as usize],
                    );
                    (b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) <= limit2
                })
            })
            .collect();
        Self::from_mesh(mesh_points, triangles)
    }

    /// A surface from a given mesh: points (x, y, z) and triangles (indices
    /// into the points, either orientation). Triangles with no area in plan
    /// are ignored. Overlapping triangles are not detected: the first one
    /// found wins.
    pub fn from_mesh(
        points: Vec<[f64; 3]>,
        triangles: Vec<[u32; 3]>,
    ) -> Result<Self, TerrainError> {
        if let Some(p) = points.iter().find(|p| !p.iter().all(|v| v.is_finite())) {
            return Err(TerrainError::Degenerate(format!(
                "point {p:?} is not finite"
            )));
        }
        if let Some(t) = triangles
            .iter()
            .find(|t| t.iter().any(|&k| k as usize >= points.len()))
        {
            return Err(TerrainError::Degenerate(format!(
                "triangle {t:?} refers to a point beyond the {} given",
                points.len()
            )));
        }
        let triangles: Vec<[u32; 3]> = triangles
            .into_iter()
            .filter(|t| {
                let [a, b, c] = t.map(|k| points[k as usize]);
                area2(a, b, c).abs() > 1e-12
            })
            .collect();
        if triangles.is_empty() {
            return Err(TerrainError::Degenerate("no triangle with an area".into()));
        }
        let grid = Grid::build(&points, &triangles);
        Ok(Self {
            points,
            triangles,
            grid,
            skipped_breaklines: 0,
        })
    }

    /// Reads and triangulates a survey file (no maximum side).
    pub fn load(path: &std::path::Path) -> Result<Self, TerrainError> {
        let text = std::fs::read_to_string(path).map_err(|e| TerrainError::Parse {
            line: 0,
            message: format!("{}: {e}", path.display()),
        })?;
        Self::from_points(&read_points(&text)?, None)
    }

    /// Number of points.
    pub fn num_points(&self) -> usize {
        self.points.len()
    }

    /// Number of triangles.
    pub fn num_triangles(&self) -> usize {
        self.triangles.len()
    }

    /// Breakline edges skipped because they crossed earlier breaklines.
    pub fn skipped_breaklines(&self) -> usize {
        self.skipped_breaklines
    }

    fn corners(&self, k: u32) -> [[f64; 3]; 3] {
        self.triangles[k as usize].map(|v| self.points[v as usize])
    }

    /// Height of triangle `k`'s plane at (x, y), if (x, y) is inside it
    /// (with a tolerance of 1e-9 of the triangle's size).
    fn height_in(&self, k: u32, x: f64, y: f64) -> Option<f64> {
        let [a, b, c] = self.corners(k);
        let d = area2(a, b, c);
        let q = [x, y, 0.0];
        let (wa, wb, wc) = (area2(q, b, c) / d, area2(a, q, c) / d, area2(a, b, q) / d);
        let eps = -1e-9;
        (wa >= eps && wb >= eps && wc >= eps).then(|| wa * a[2] + wb * b[2] + wc * c[2])
    }

    /// Ground height at (x, y), m; `None` where there is no ground (outside
    /// the triangles).
    pub fn height(&self, x: f64, y: f64) -> Option<f64> {
        if self.grid.nx == 0 {
            return None;
        }
        let (i, j) = (
            self.grid.index(x, 0, self.grid.nx),
            self.grid.index(y, 1, self.grid.ny),
        );
        self.grid
            .cell_items(i, j)
            .iter()
            .find_map(|&k| self.height_in(k, x, y))
    }

    /// Part of the segment a + t·(b − a), t ∈ [0, 1], inside triangle `k`
    /// in plan (Cyrus–Beck clipping against its three edges).
    fn clip(&self, k: u32, a: [f64; 2], d: [f64; 2]) -> Option<(f64, f64)> {
        let corners = self.corners(k);
        let s = area2(corners[0], corners[1], corners[2]).signum();
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for e in 0..3 {
            let (p, q) = (corners[e], corners[(e + 1) % 3]);
            let edge = [q[0] - p[0], q[1] - p[1]];
            // Inside: s · cross(edge, x − p) ≥ 0, linear in t.
            let f0 = s * (edge[0] * (a[1] - p[1]) - edge[1] * (a[0] - p[0]));
            let f1 = s * (edge[0] * d[1] - edge[1] * d[0]);
            // Exact bounds (neighbours then meet at the same t); a
            // tolerance only for a segment running along this edge.
            let scale = edge[0].hypot(edge[1]) * d[0].hypot(d[1]);
            if f1.abs() <= 1e-12 * scale {
                if f0 < -1e-9 * edge[0].hypot(edge[1]) {
                    return None;
                }
            } else if f1 > 0.0 {
                lo = lo.max(-f0 / f1);
            } else {
                hi = hi.min(-f0 / f1);
            }
            if lo > hi {
                return None;
            }
        }
        Some((lo.clamp(0.0, 1.0), hi.clamp(0.0, 1.0)))
    }

    /// Ground profile under the segment from `a` to `b` (x, y): runs of
    /// points where the segment is over ground, ending where it leaves the
    /// surface (an edge of the TIN or a hole). Points are where the segment
    /// enters and leaves each triangle — every edge crossing — so between
    /// consecutive points of a run the ground is exactly linear.
    pub fn profile(&self, a: [f64; 2], b: [f64; 2]) -> Vec<Vec<GroundPoint>> {
        let d = [b[0] - a[0], b[1] - a[1]];
        if d[0].hypot(d[1]) == 0.0 {
            return self
                .height(a[0], a[1])
                .map(|z| {
                    vec![vec![GroundPoint {
                        t: 0.0,
                        point: [a[0], a[1], z],
                    }]]
                })
                .unwrap_or_default();
        }
        let mut pieces: Vec<(f64, f64, u32)> = self
            .grid
            .along(a, b)
            .into_iter()
            .filter_map(|k| self.clip(k, a, d).map(|(t0, t1)| (t0, t1, k)))
            .collect();
        pieces.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
        let at = |t: f64, k: u32| -> GroundPoint {
            let (x, y) = (a[0] + d[0] * t, a[1] + d[1] * t);
            // On the triangle's plane (no inside test: t is on its boundary
            // or inside by construction).
            let [p, q, r] = self.corners(k);
            let den = area2(p, q, r);
            let s = [x, y, 0.0];
            let z = (area2(s, q, r) * p[2] + area2(p, s, r) * q[2] + area2(p, q, s) * r[2]) / den;
            GroundPoint {
                t,
                point: [x, y, z],
            }
        };
        const GAP: f64 = 1e-9;
        let mut runs: Vec<Vec<GroundPoint>> = Vec::new();
        let mut reach = f64::NEG_INFINITY;
        for (t0, t1, k) in pieces {
            if t0 > reach + GAP || runs.is_empty() {
                runs.push(Vec::new());
            }
            let run = runs.last_mut().expect("pushed");
            run.push(at(t0, k));
            run.push(at(t1, k));
            reach = reach.max(t1);
        }
        for run in &mut runs {
            run.sort_by(|x, y| x.t.total_cmp(&y.t));
            run.dedup_by(|x, y| (x.t - y.t).abs() < 1e-10);
        }
        runs
    }

    /// The surface as a mesh: points and triangles (indices into the
    /// points).
    pub fn mesh(&self) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        (
            self.points.clone(),
            self.triangles
                .iter()
                .map(|t| t.map(|k| k as usize))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(x: f64, y: f64) -> f64 {
        400.0 + 0.05 * x - 0.02 * y
    }

    fn grid(f: impl Fn(f64, f64) -> f64, n: usize, step: f64) -> Vec<SurveyPoint> {
        let mut points = Vec::new();
        for i in 0..n {
            for j in 0..n {
                let (x, y) = (i as f64 * step, j as f64 * step);
                points.push(SurveyPoint {
                    x,
                    y,
                    z: f(x, y),
                    code: String::new(),
                    line: String::new(),
                });
            }
        }
        points
    }

    #[test]
    fn reads_plain_and_headed_files() {
        let plain = read_points("# survey\n1 2 3\n4,5,6,ROAD,r1\n\n7;8;9\n").unwrap();
        assert_eq!(plain.len(), 3);
        assert_eq!(
            (plain[1].code.as_str(), plain[1].line.as_str()),
            ("ROAD", "r1")
        );
        let pnezd = read_points("P,N,E,Z,D\n10,200.5,100.25,431.0,GND\n").unwrap();
        assert_eq!((pnezd[0].x, pnezd[0].y, pnezd[0].z), (100.25, 200.5, 431.0));
        assert_eq!(pnezd[0].code, "GND");
        let err = read_points("1 2 3\n4 five 6\n").unwrap_err();
        assert_eq!(
            err,
            TerrainError::Parse {
                line: 2,
                message: "\"five\" is not a number".into()
            }
        );
        assert!(read_points("a,b,c\n").is_err());
        assert!(read_points("1 2\n").is_err());
    }

    #[test]
    fn plane_is_reproduced_exactly() {
        let terrain = Terrain::from_points(&grid(plane, 6, 20.0), None).unwrap();
        for (x, y) in [(0.0, 0.0), (13.3, 71.9), (40.0, 40.0), (99.99, 0.01)] {
            let z = terrain.height(x, y).unwrap();
            assert!((z - plane(x, y)).abs() < 1e-9, "{x} {y}: {z}");
        }
        assert_eq!(terrain.height(-1.0, 5.0), None);
        assert_eq!(terrain.height(50.0, 100.5), None);
    }

    #[test]
    fn profile_crosses_every_edge_and_stops_outside() {
        // A ridge along x = 50 between two valleys: z = 400 + |x − 50|·0.2,
        // sampled every 25 m — the ridge line is an edge of the TIN.
        let f = |x: f64, _y: f64| 400.0 + (x - 50.0).abs() * 0.2;
        let terrain = Terrain::from_points(&grid(f, 5, 25.0), None).unwrap();
        let runs = terrain.profile([-10.0, 37.0], [90.0, 41.0]);
        assert_eq!(runs.len(), 1);
        let run = &runs[0];
        // Starts where it enters the TIN (x = 0), ends at the far end.
        assert!(run[0].point[0].abs() < 1e-9 && (run.last().unwrap().t - 1.0).abs() < 1e-12);
        // Every profile point lies on the surface; the ridge top is a point.
        for p in run {
            assert!(
                (p.point[2] - f(p.point[0], p.point[1])).abs() < 1e-9,
                "{p:?}"
            );
        }
        assert!(run.iter().any(|p| (p.point[0] - 50.0).abs() < 1e-9));
        // Between consecutive points the ground is linear: mid-points agree.
        for w in run.windows(2) {
            let m = [
                (w[0].point[0] + w[1].point[0]) / 2.0,
                (w[0].point[1] + w[1].point[1]) / 2.0,
            ];
            let z = terrain.height(m[0], m[1]).unwrap();
            assert!((z - (w[0].point[2] + w[1].point[2]) / 2.0).abs() < 1e-9);
        }
        // Fully outside: nothing.
        assert!(terrain.profile([200.0, 0.0], [300.0, 0.0]).is_empty());
    }

    #[test]
    fn breaklines_are_kept_as_edges() {
        // A flat rhombus: Delaunay joins the near corners B–D (0 m high),
        // the ridge breakline joins the far corners A–C (10 m high).
        let p = |x: f64, y: f64, z: f64, line: &str| SurveyPoint {
            x,
            y,
            z,
            code: String::new(),
            line: line.into(),
        };
        let corners = |line: &str| {
            vec![
                p(0.0, 0.0, 10.0, line),
                p(100.0, 0.0, 10.0, line),
                p(50.0, -20.0, 0.0, ""),
                p(50.0, 20.0, 0.0, ""),
            ]
        };
        let plain = Terrain::from_points(&corners(""), None).unwrap();
        assert!(plain.height(50.0, 0.0).unwrap().abs() < 1e-9);
        let ridged = Terrain::from_points(&corners("ridge"), None).unwrap();
        assert_eq!(ridged.skipped_breaklines(), 0);
        for x in [25.0, 50.0, 75.0] {
            assert!((ridged.height(x, 0.0).unwrap() - 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn long_triangles_are_left_out_as_holes() {
        // Two survey patches 100 m apart: without a limit the gap is bridged
        // by long triangles; with a 30 m limit it is a hole.
        let mut points = grid(plane, 3, 10.0);
        points.extend(grid(plane, 3, 10.0).into_iter().map(|mut p| {
            p.x += 120.0;
            p.z = plane(p.x, p.y);
            p
        }));
        let bridged = Terrain::from_points(&points, None).unwrap();
        let xyz: Vec<[f64; 3]> = points.iter().map(|p| [p.x, p.y, p.z]).collect();
        assert_eq!(
            Terrain::from_xyz(&xyz, Some(30.0)).unwrap().num_triangles(),
            Terrain::from_points(&points, Some(30.0))
                .unwrap()
                .num_triangles()
        );
        let holed = Terrain::from_points(&points, Some(30.0)).unwrap();
        assert!(bridged.height(70.0, 10.0).is_some());
        assert_eq!(holed.height(70.0, 10.0), None);
        assert!((holed.height(130.0, 10.0).unwrap() - plane(130.0, 10.0)).abs() < 1e-9);
        // A profile across the gap: two runs, ending and starting at the
        // patch edges (x = 20 and x = 120).
        let runs = holed.profile([-5.0, 10.0], [150.0, 10.0]);
        assert_eq!(runs.len(), 2, "{runs:?}");
        assert!((runs[0].last().unwrap().point[0] - 20.0).abs() < 1e-6);
        assert!((runs[1][0].point[0] - 120.0).abs() < 1e-6);
        assert_eq!(bridged.profile([-5.0, 10.0], [150.0, 10.0]).len(), 1);
    }

    #[test]
    fn given_mesh_is_used_as_is() {
        // Two triangles of a 10 m square, split along the other diagonal
        // than Delaunay would choose for these heights (both are Delaunay
        // in plan; the mesh decides).
        let points = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 10.0],
        ];
        // Diagonal 1–3: the centre is halfway along it, at 5 m.
        let terrain = Terrain::from_mesh(points.clone(), vec![[0, 1, 3], [1, 2, 3]]).unwrap();
        assert!((terrain.height(5.0, 5.0).unwrap() - 5.0).abs() < 1e-12);
        // Diagonal 0–2: the centre is on it, at 0 m.
        let other = Terrain::from_mesh(points.clone(), vec![[0, 1, 2], [0, 2, 3]]).unwrap();
        assert!(other.height(5.0, 5.0).unwrap().abs() < 1e-12);
        // Profile across: crossings of the diagonal included, exact.
        let run = &terrain.profile([-1.0, 2.0], [11.0, 2.0])[0];
        assert_eq!(run.len(), 3, "{run:?}");
        assert!((run[1].point[0] - 8.0).abs() < 1e-9 && (run[1].point[2] - 2.0).abs() < 1e-9);
        assert!(Terrain::from_mesh(points, vec![[0, 1, 9]]).is_err());
    }

    #[test]
    fn degenerate_input_is_an_error() {
        let line: Vec<_> = (0..5)
            .map(|k| SurveyPoint {
                x: k as f64,
                y: 0.0,
                z: 0.0,
                code: String::new(),
                line: String::new(),
            })
            .collect();
        assert!(matches!(
            Terrain::from_points(&line, None),
            Err(TerrainError::Degenerate(_))
        ));
    }
}
