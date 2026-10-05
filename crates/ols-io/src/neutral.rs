// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structures and a route from neutral files (points and a polyline from
//! CSV, GeoJSON, Shapefile or LandXML): the common step of those
//! importers, and the import of any line format into a project as one
//! logged command ([`import_into_project`]).
//!
//! Points become structures (a default type from option `type`), ordered
//! by their station along the polyline when there is one, and the route
//! `route` (default `main`) runs through them along the polyline. Points
//! whose id is a structure of the line move it (with its attachments).
//! Linear in the number of points (the stationing: points × polyline
//! vertices).
//!
//! Options common to these importers:
//!
//! | option       | meaning |
//! |--------------|---------|
//! | `type`       | structure type of new structures that have none |
//! | `prefix`     | id prefix of points without an id (default `P`) |
//! | `route`      | route made or replaced (default `main`) |
//! | `order`      | `station` (default: along the polyline) or `file` |
//! | `crs`        | coordinate system of a new line (the line's own otherwise) |
//! | `source_crs` | coordinate system of the file, if it does not say |
//! | `z`          | height of points without one, m (default 0) |
//! | `name`       | name of a new line |

use std::path::Path;

use ols_crs::{Crs, Heights, Transformation};
use ols_line::{AlignmentPoint, Line, Point, Route, Structure};

use crate::{Options, Registry};

/// An empty line named `name`.
pub fn empty_line(name: &str) -> Line {
    Line {
        format: ols_line::LINE_FILE_FORMAT.into(),
        version: ols_line::LINE_FILE_VERSION,
        name: name.to_owned(),
        crs: String::new(),
        alignment: Vec::new(),
        routes: Vec::new(),
        structures: Vec::new(),
        sections: Vec::new(),
        jumpers: Vec::new(),
        display: Default::default(),
        systems: Vec::new(),
        creep_stretch: Default::default(),
        condition_cases: Default::default(),
    }
}

/// A point read from a neutral file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NeutralPoint {
    /// Structure id, if the file names it.
    pub id: Option<String>,
    /// Position; z is NaN when the file has none.
    pub position: Point,
    /// Structure type.
    pub structure_type: Option<String>,
    /// Height extension.
    pub extension: Option<String>,
    /// Orientation, degrees counter-clockwise from east.
    pub orientation_deg: Option<f64>,
}

/// What a neutral file holds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Neutral {
    /// Points (structures), in file order.
    pub points: Vec<NeutralPoint>,
    /// The route's polyline (z may be NaN), if the file has one.
    pub route: Vec<Point>,
    /// The file's coordinate system, if it says (e.g. `EPSG:4326`).
    pub crs: Option<String>,
    /// A name (layer, file).
    pub name: Option<String>,
}

fn same_crs(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Orientation of a structure's x axis (right of the line looking ahead)
/// at point `k` of a polyline: square to the bisector of the directions.
fn orientation(points: &[Point], k: usize) -> Option<f64> {
    let unit = |a: Point, b: Point| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx.hypot(dy);
        (l > 0.0).then(|| [dx / l, dy / l])
    };
    let back = (k > 0).then(|| unit(points[k - 1], points[k])).flatten();
    let ahead = points.get(k + 1).and_then(|&p| unit(points[k], p));
    let d = match (back, ahead) {
        (Some(a), Some(b)) => [a[0] + b[0], a[1] + b[1]],
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => return None,
    };
    if d[0].hypot(d[1]) < 1e-9 {
        return None;
    }
    let deg = d[1].atan2(d[0]).to_degrees() - 90.0;
    Some(if deg <= -180.0 { deg + 360.0 } else { deg })
}

/// The line made from (or updated by, with `base`) a neutral file's
/// points and polyline; see the module documentation for the options.
pub fn to_line(data: Neutral, base: Option<&Line>, options: &Options) -> Result<Line, String> {
    let Neutral {
        mut points,
        route: mut polyline,
        crs,
        name,
    } = data;
    if points.is_empty() {
        return Err("no points in the file".into());
    }
    let source = options
        .get("source_crs")
        .cloned()
        .or(crs)
        .filter(|c| !c.trim().is_empty());
    let target = base
        .map(|b| b.crs.clone())
        .filter(|c| !c.trim().is_empty())
        .or_else(|| options.get("crs").cloned())
        .filter(|c| !c.trim().is_empty());
    match (&source, &target) {
        (Some(s), Some(t)) if !same_crs(s, t) => {
            let from = Crs::parse(s)?;
            let to = Crs::parse(t).map_err(|e| {
                format!("{e} (the line's coordinate system, to convert the file's {s} to)")
            })?;
            let tr = Transformation::new(&from, &to, Heights::Keep);
            let mut all: Vec<Point> = points.iter().map(|p| p.position).collect();
            tr.points(&mut all)?;
            for (p, q) in points.iter_mut().zip(all) {
                p.position = q;
            }
            tr.points(&mut polyline)?;
        }
        (Some(s), None) if Crs::parse(s).is_ok_and(|c| c.is_geographic()) => {
            return Err(format!(
                "the file's coordinates are longitude and latitude ({s}): give the line's \
                 coordinate system with option crs=EPSG:<code>"
            ));
        }
        _ => {}
    }
    let z = match options.get("z") {
        Some(v) => v
            .parse::<f64>()
            .ok()
            .filter(|z| z.is_finite())
            .ok_or_else(|| format!("option z: \"{v}\" is not a height"))?,
        None => 0.0,
    };
    for p in &mut points {
        if !p.position[2].is_finite() {
            p.position[2] = z;
        }
    }
    for p in &mut polyline {
        if !p[2].is_finite() {
            p[2] = z;
        }
    }
    let by_station = match options.get("order").map(String::as_str) {
        None | Some("station") => polyline.len() >= 2,
        Some("file") => false,
        Some(o) => return Err(format!("option order: \"{o}\" (station or file)")),
    };
    if by_station {
        let mut keyed: Vec<(f64, NeutralPoint)> = points
            .into_iter()
            .map(|p| (ols_line::route::project(&polyline, p.position).0, p))
            .collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
        points = keyed.into_iter().map(|(_, p)| p).collect();
    }

    let mut line = match base {
        Some(b) => b.clone(),
        None => {
            let mut l = empty_line(
                options
                    .get("name")
                    .cloned()
                    .or_else(|| name.clone())
                    .as_deref()
                    .unwrap_or("imported"),
            );
            l.crs = target.clone().or(source.clone()).unwrap_or_default();
            l
        }
    };
    let prefix = options.get("prefix").map_or("P", String::as_str);
    let default_type = options.get("type").filter(|t| !t.trim().is_empty());
    let positions: Vec<Point> = points.iter().map(|p| p.position).collect();
    let mut ids = Vec::with_capacity(points.len());
    let mut next = 1;
    for (k, p) in points.into_iter().enumerate() {
        let id = match p.id.filter(|i| !i.trim().is_empty()) {
            Some(i) => i.trim().to_owned(),
            None => loop {
                let candidate = format!("{prefix}{next}");
                next += 1;
                if line.structure(&candidate).is_none() && !ids.contains(&candidate) {
                    break candidate;
                }
            },
        };
        if ids.contains(&id) {
            return Err(format!("structure {id} is in the file twice"));
        }
        match line.structures.iter_mut().find(|s| s.id == id) {
            Some(s) => {
                let o = p.orientation_deg.unwrap_or(s.orientation_deg);
                crate::formats::place(s, p.position, o);
                if p.structure_type.is_some() {
                    s.structure_type = p.structure_type;
                }
                if p.extension.is_some() {
                    s.extension = p.extension;
                }
            }
            None => line.structures.push(Structure {
                id: id.clone(),
                position: p.position,
                orientation_deg: p
                    .orientation_deg
                    .or_else(|| orientation(&positions, k))
                    .unwrap_or(0.0),
                structure_type: p.structure_type.or_else(|| default_type.cloned()),
                extension: p.extension,
                attachments: Vec::new(),
                omit: Vec::new(),
                members: Vec::new(),
            }),
        }
        ids.push(id);
    }
    // The route: always for a new line, else when the file has a polyline
    // or a route is named.
    let route_id = options.get("route").map_or("main", String::as_str);
    if (base.is_none() || polyline.len() >= 2 || options.contains_key("route")) && ids.len() >= 2 {
        let alignment: Vec<AlignmentPoint> = if polyline.len() >= 2 {
            polyline.into_iter().map(AlignmentPoint::free).collect()
        } else {
            Vec::new()
        };
        if line.routes.is_empty() && base.is_some_and(|b| !b.structures.is_empty()) {
            // Keep the implicit route of the line as it was.
            line.routes = base.expect("checked").effective_routes();
            line.routes[0].alignment = base.expect("checked").alignment.clone();
            line.alignment.clear();
        }
        match line.routes.iter_mut().find(|r| r.id == route_id) {
            Some(r) => {
                r.structures = ids;
                if !alignment.is_empty() {
                    r.alignment = alignment;
                }
            }
            None => line.routes.push(Route {
                id: route_id.to_owned(),
                name: name.unwrap_or_default(),
                alignment,
                structures: ids,
            }),
        }
    }
    Ok(line)
}

/// Reads a file with a line format (format: `format`, else the extension)
/// into a project file: a new project (created, `user` its admin) gets
/// the line; one holding a line gets it updated (the file's line built on
/// it) — either way one logged command, undone at once. Returns the log
/// entry and the line.
pub fn import_into_project(
    registry: &mut Registry,
    path: &Path,
    format: Option<&str>,
    options: &Options,
    project: &Path,
    user: &str,
) -> Result<(ols_session::Applied, Line), String> {
    let id = crate::format_of(path, format);
    let importer = registry
        .importer(&id)
        .ok_or_else(|| format!("no importer for \"{id}\" (see ols formats)"))?;
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut s = ols_session::Session::open(project).map_err(|e| e.to_string())?;
    let base = s.has_line().then(|| s.export());
    let line = importer
        .import(&data, base.as_ref(), options)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    line.validate().map_err(|e| e.to_string())?;
    let applied = if base.is_some() {
        s.apply(
            user,
            ols_session::Command::ReplaceLine {
                line: Box::new(line.clone()),
            },
        )
    } else {
        s.import(&line, user)
    }
    .map_err(|e| e.to_string())?;
    s.checkpoint().map_err(|e| e.to_string())?;
    Ok((applied, line))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: Option<&str>, x: f64, y: f64) -> NeutralPoint {
        NeutralPoint {
            id: id.map(str::to_owned),
            position: [x, y, f64::NAN],
            ..Default::default()
        }
    }

    #[test]
    fn points_are_ordered_along_the_polyline() {
        let data = Neutral {
            points: vec![
                p(None, 200.0, 1.0),
                p(Some("A"), 0.0, 0.0),
                p(None, 100.0, -1.0),
            ],
            route: vec![[0.0, 0.0, f64::NAN], [300.0, 0.0, f64::NAN]],
            crs: None,
            name: Some("pts".into()),
        };
        let options = Options::from([
            ("type".to_owned(), "tangent".to_owned()),
            ("z".to_owned(), "400".to_owned()),
        ]);
        let line = to_line(data, None, &options).unwrap();
        let ids: Vec<&str> = line.structures.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["A", "P1", "P2"]);
        assert_eq!(line.structures[1].position, [100.0, -1.0, 400.0]);
        assert_eq!(
            line.structures[0].structure_type.as_deref(),
            Some("tangent")
        );
        // Heading east (towards P1, 1 m right): the x axis points south.
        let o = (-1.0f64).atan2(100.0).to_degrees() - 90.0;
        assert!((line.structures[0].orientation_deg - o).abs() < 1e-9);
        assert_eq!(line.routes[0].structures, ["A", "P1", "P2"]);
        assert_eq!(line.routes[0].alignment.len(), 2);
        line.validate().unwrap();
        // Into the line: A moves, a new point joins; the route is kept.
        let again = Neutral {
            points: vec![p(Some("A"), 5.0, 0.0)],
            ..Default::default()
        };
        let moved = to_line(again, Some(&line), &Options::new()).unwrap();
        assert_eq!(moved.structures[0].position, [5.0, 0.0, 0.0]);
        assert_eq!(moved.routes, line.routes);
        assert!(to_line(Neutral::default(), None, &Options::new()).is_err());
    }

    #[test]
    fn geographic_points_need_the_line_crs() {
        let data = Neutral {
            points: vec![p(None, 7.438632420871814, 46.9510827728495)],
            crs: Some("EPSG:4326".into()),
            ..Default::default()
        };
        assert!(
            to_line(data.clone(), None, &Options::new())
                .unwrap_err()
                .contains("crs=")
        );
        let o = Options::from([("crs".to_owned(), "EPSG:2056".to_owned())]);
        let line = to_line(data, None, &o).unwrap();
        let q = line.structures[0].position;
        assert!((q[0] - 2_600_000.0).abs() < 0.01 && (q[1] - 1_200_000.0).abs() < 0.01);
        assert_eq!(line.crs, "EPSG:2056");
    }
}
