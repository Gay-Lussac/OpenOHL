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
    ConstrainedDelaunayTriangulation, HasPosition, Intersection, LineIntersectionIterator, Point2,
    PositionInTriangulation, Triangulation,
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

/// A triangulated ground surface.
pub struct Terrain {
    tin: ConstrainedDelaunayTriangulation<Vertex>,
    skipped_breaklines: usize,
}

impl fmt::Debug for Terrain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Terrain")
            .field("points", &self.tin.num_vertices())
            .field("triangles", &self.tin.num_inner_faces())
            .field("breaklines", &self.tin.num_constraints())
            .finish()
    }
}

impl Terrain {
    /// Triangulates survey points. Points at the same x, y are merged (the
    /// first one is kept). Breakline edges that would cross an earlier
    /// breakline are skipped and counted ([`Terrain::skipped_breaklines`]).
    pub fn from_points(points: &[SurveyPoint]) -> Result<Self, TerrainError> {
        let mut tin = ConstrainedDelaunayTriangulation::<Vertex>::new();
        let mut handles = Vec::with_capacity(points.len());
        for p in points {
            let vertex = Vertex {
                position: Point2::new(p.x, p.y),
                z: p.z,
            };
            let handle = match tin.locate(vertex.position) {
                PositionInTriangulation::OnVertex(existing) => existing,
                _ => tin
                    .insert(vertex)
                    .map_err(|e| TerrainError::Degenerate(format!("point {p:?}: {e:?}")))?,
            };
            handles.push(handle);
        }
        if tin.num_inner_faces() == 0 {
            return Err(TerrainError::Degenerate(format!(
                "{} distinct points do not span a surface (need three not on a line)",
                tin.num_vertices()
            )));
        }
        let mut skipped_breaklines = 0;
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
        Ok(Self {
            tin,
            skipped_breaklines,
        })
    }

    /// Reads and triangulates a survey file.
    pub fn load(path: &std::path::Path) -> Result<Self, TerrainError> {
        let text = std::fs::read_to_string(path).map_err(|e| TerrainError::Parse {
            line: 0,
            message: format!("{}: {e}", path.display()),
        })?;
        Self::from_points(&read_points(&text)?)
    }

    /// Number of distinct points.
    pub fn num_points(&self) -> usize {
        self.tin.num_vertices()
    }

    /// Number of triangles.
    pub fn num_triangles(&self) -> usize {
        self.tin.num_inner_faces()
    }

    /// Breakline edges skipped because they crossed earlier breaklines.
    pub fn skipped_breaklines(&self) -> usize {
        self.skipped_breaklines
    }

    /// Ground height at (x, y), m; `None` outside the surveyed area (the
    /// convex hull of the points).
    pub fn height(&self, x: f64, y: f64) -> Option<f64> {
        let q = Point2::new(x, y);
        match self.tin.locate(q) {
            PositionInTriangulation::OnVertex(v) => Some(self.tin.vertex(v).data().z),
            PositionInTriangulation::OnEdge(e) => {
                let edge = self.tin.directed_edge(e);
                let (a, b) = (*edge.from().data(), *edge.to().data());
                let (dx, dy) = (b.position.x - a.position.x, b.position.y - a.position.y);
                let t = ((x - a.position.x) * dx + (y - a.position.y) * dy) / (dx * dx + dy * dy);
                Some(a.z + (b.z - a.z) * t)
            }
            PositionInTriangulation::OnFace(f) => {
                let face = self.tin.face(f);
                let w = face.barycentric_interpolation(q);
                let v = face.vertices();
                Some(w[0] * v[0].data().z + w[1] * v[1].data().z + w[2] * v[2].data().z)
            }
            PositionInTriangulation::OutsideOfConvexHull(_)
            | PositionInTriangulation::NoTriangulation => None,
        }
    }

    /// Ground profile under the segment from `a` to `b` (x, y): the
    /// segment's ends and every crossing with a triangle edge or vertex, in
    /// order, as runs of points with ground between them (a run ends where
    /// the segment leaves the surveyed area). Between consecutive points of
    /// a run the ground is exactly linear.
    pub fn profile(&self, a: [f64; 2], b: [f64; 2]) -> Vec<Vec<GroundPoint>> {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let at = |t: f64| [a[0] + dx * t, a[1] + dy * t];
        let param = |p: Point2<f64>| {
            if len2 > 0.0 {
                (((p.x - a[0]) * dx + (p.y - a[1]) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let mut found: Vec<(f64, f64)> = Vec::new();
        for end in [0.0, 1.0] {
            let p = at(end);
            if let Some(z) = self.height(p[0], p[1]) {
                found.push((end, z));
            }
        }
        if len2 > 0.0 {
            let iter = LineIntersectionIterator::new(
                &self.tin,
                Point2::new(a[0], a[1]),
                Point2::new(b[0], b[1]),
            );
            for intersection in iter {
                match intersection {
                    Intersection::VertexIntersection(v) => {
                        found.push((param(v.position()), v.data().z));
                    }
                    Intersection::EdgeOverlap(e) => {
                        for v in [e.from(), e.to()] {
                            found.push((param(v.position()), v.data().z));
                        }
                    }
                    Intersection::EdgeIntersection(e) => {
                        let (p, q) = (*e.from().data(), *e.to().data());
                        // Segment a + t·d crosses edge p + s·(q − p).
                        let (ex, ey) = (q.position.x - p.position.x, q.position.y - p.position.y);
                        let det = dx * ey - dy * ex;
                        if det.abs() < f64::EPSILON * len2.sqrt() * ex.hypot(ey) {
                            continue;
                        }
                        let (rx, ry) = (p.position.x - a[0], p.position.y - a[1]);
                        let t = (rx * ey - ry * ex) / det;
                        let s = (rx * dy - ry * dx) / det;
                        if (0.0..=1.0).contains(&t) {
                            found.push((t, p.z + (q.z - p.z) * s.clamp(0.0, 1.0)));
                        }
                    }
                }
            }
        }
        found.sort_by(|x, y| x.0.total_cmp(&y.0));
        found.dedup_by(|x, y| (x.0 - y.0).abs() < 1e-12);
        let mut runs: Vec<Vec<GroundPoint>> = Vec::new();
        let mut previous: Option<f64> = None;
        for (t, z) in found {
            let p = at(t);
            let point = GroundPoint {
                t,
                point: [p[0], p[1], z],
            };
            // Same run if the ground continues between the two points.
            let joined = previous.is_some_and(|t0| {
                let m = at((t0 + t) / 2.0);
                self.height(m[0], m[1]).is_some()
            });
            match runs.last_mut() {
                Some(run) if joined => run.push(point),
                _ => runs.push(vec![point]),
            }
            previous = Some(t);
        }
        runs
    }

    /// The surface as a mesh: points and triangles (indices into the
    /// points, counter-clockwise seen from above).
    pub fn mesh(&self) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        let points = self
            .tin
            .vertices()
            .map(|v| [v.position().x, v.position().y, v.data().z])
            .collect();
        let triangles = self
            .tin
            .inner_faces()
            .map(|f| f.vertices().map(|v| v.fix().index()))
            .collect();
        (points, triangles)
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
        let terrain = Terrain::from_points(&grid(plane, 6, 20.0)).unwrap();
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
        let terrain = Terrain::from_points(&grid(f, 5, 25.0)).unwrap();
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
        let plain = Terrain::from_points(&corners("")).unwrap();
        assert!(plain.height(50.0, 0.0).unwrap().abs() < 1e-9);
        let ridged = Terrain::from_points(&corners("ridge")).unwrap();
        assert_eq!(ridged.skipped_breaklines(), 0);
        for x in [25.0, 50.0, 75.0] {
            assert!((ridged.height(x, 0.0).unwrap() - 10.0).abs() < 1e-9);
        }
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
            Terrain::from_points(&line),
            Err(TerrainError::Degenerate(_))
        ));
    }
}
