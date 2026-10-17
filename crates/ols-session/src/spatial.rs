// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The spatial index of the project file (ADR-0008).
//!
//! Every model object with a place in the plan has its **plan bounds** and
//! its **home tile** of the project's chunk grid ([`ols_core::grid`]) in
//! the table
//!
//! ```sql
//! spatial_index(line, kind, id, level, tx, ty, xmin, ymin, xmax, ymax,
//!               PRIMARY KEY(line, kind, id))   -- index on (line, level, tx, ty)
//! ```
//!
//! kept up to date in the same transaction as every command. The grid's
//! tile size is stored in `ols_meta` (`grid_size`) and fixed for the
//! project.
//!
//! Bounds are those of the model, not of a solved state:
//!
//! - structures: position, attachment and V-string leg points, stick
//!   drawing;
//! - sections: the wire points of their attachments (or the structure
//!   positions for typed structures not resolved in the project);
//! - jumpers: their two ends and the support string point;
//! - routes and the line's own alignment (`meta`): alignment points and
//!   route structures.
//!
//! Wires swing out under wind: an area query for wires in a weather case
//! grows the area by the largest swing expected.

use std::collections::BTreeMap;

use ols_core::grid::{Bounds, Grid, Tile};
use ols_line::{Line, Point, SectionPoint};
use rusqlite::{Connection, params};

use crate::{Kind, SessionError};

/// One indexed object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// Plan bounds, m.
    pub bounds: Bounds,
    /// Home tile.
    pub tile: Tile,
}

pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS spatial_index (
        line TEXT NOT NULL DEFAULT 'main',
        kind TEXT NOT NULL, id TEXT NOT NULL, level INTEGER NOT NULL, tx INTEGER NOT NULL,
        ty INTEGER NOT NULL, xmin REAL NOT NULL, ymin REAL NOT NULL, xmax REAL NOT NULL,
        ymax REAL NOT NULL, PRIMARY KEY (line, kind, id));
     CREATE INDEX IF NOT EXISTS spatial_tile ON spatial_index (line, level, tx, ty);";

fn clamp_point(line: &Line, p: &SectionPoint) -> Option<Point> {
    match line.wire_point(p) {
        Some(w) => Some(w.position),
        None => line.structure(&p.structure).map(|s| s.position),
    }
}

/// The plan bounds of every object of `line` that has a place.
pub fn object_bounds(line: &Line) -> BTreeMap<(Kind, String), Bounds> {
    let mut out = BTreeMap::new();
    let mut put = |kind: Kind, id: &str, points: Vec<Point>| {
        if let Some(b) = Bounds::of_points(&points) {
            out.insert((kind, id.to_owned()), b);
        }
    };
    for s in &line.structures {
        let mut pts = vec![s.position];
        pts.extend(s.points.iter().map(|p| p.position));
        pts.extend(s.wires.iter().map(|w| w.position));
        pts.extend(s.members.iter().flatten());
        put(Kind::Structure, &s.id, pts);
    }
    for s in &line.sections {
        let pts = s
            .points
            .iter()
            .filter_map(|p| clamp_point(line, p))
            .collect();
        put(Kind::Section, &s.id, pts);
    }
    for j in &line.jumpers {
        // Its ends: wire points, and taps where they are.
        let mut pts: Vec<Point> = [&j.from, &j.to]
            .into_iter()
            .filter_map(|e| match e.point() {
                Some(p) => clamp_point(line, p),
                None => ols_line::jumper::approximate_end(line, e, 0),
            })
            .collect();
        pts.extend(j.support().iter().map(|s| s.point));
        // Its support clamps, where their wire points hang.
        pts.extend(j.supports().iter().filter_map(|c| {
            line.structure(c.structure_id(j))
                .and_then(|s| s.wire(&c.wire))
                .map(|w| w.position)
        }));
        put(Kind::Jumper, &j.id, pts);
    }
    // Spacers: where their ends are on the wires.
    for s in &line.spacers {
        let pts = [&s.a, &s.b]
            .into_iter()
            .filter_map(|e| {
                ols_line::jumper::approximate_end(
                    line,
                    &ols_line::JumperEnd::tap(ols_line::Tap::Span(e.clone())),
                    0,
                )
            })
            .collect();
        put(Kind::Spacer, &s.id, pts);
    }
    for p in &line.alignment_points {
        put(Kind::AlignmentPoint, &p.id, vec![p.position]);
    }
    for a in &line.alignments {
        put(
            Kind::Alignment,
            &a.id,
            line.points_of(a).iter().map(|p| p.position).collect(),
        );
    }
    // A route: where its alignments are.
    for r in &line.routes {
        put(
            Kind::Route,
            &r.id,
            line.alignments
                .iter()
                .filter(|a| a.route == r.id)
                .flat_map(|a| line.points_of(a).into_iter().map(|p| p.position))
                .collect(),
        );
    }
    out
}

/// Index entries of `line` on `grid`.
pub fn entries(line: &Line, grid: &Grid) -> BTreeMap<(Kind, String), Entry> {
    object_bounds(line)
        .into_iter()
        .map(|(k, bounds)| {
            (
                k,
                Entry {
                    bounds,
                    tile: grid.home(&bounds),
                },
            )
        })
        .collect()
}

/// Writes the differences between `old` and `new` entries of project
/// line `line`; returns how many rows changed.
pub(crate) fn write_diff(
    conn: &Connection,
    line: &str,
    old: &BTreeMap<(Kind, String), Entry>,
    new: &BTreeMap<(Kind, String), Entry>,
) -> Result<usize, SessionError> {
    let mut changed = 0;
    for ((kind, id), _) in old.iter().filter(|(k, _)| !new.contains_key(*k)) {
        conn.execute(
            "DELETE FROM spatial_index WHERE line = ?1 AND kind = ?2 AND id = ?3",
            params![line, kind.as_str(), id],
        )?;
        changed += 1;
    }
    let mut stmt = conn.prepare_cached(
        "INSERT INTO spatial_index (kind, id, level, tx, ty, xmin, ymin, xmax, ymax, line)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(line, kind, id) DO UPDATE SET level = excluded.level, tx = excluded.tx,
             ty = excluded.ty, xmin = excluded.xmin, ymin = excluded.ymin,
             xmax = excluded.xmax, ymax = excluded.ymax",
    )?;
    for ((kind, id), e) in new {
        if old.get(&(*kind, id.clone())) == Some(e) {
            continue;
        }
        let b = e.bounds;
        stmt.execute(params![
            kind.as_str(),
            id,
            e.tile.level,
            e.tile.x,
            e.tile.y,
            b.min[0],
            b.min[1],
            b.max[0],
            b.max[1],
            line
        ])?;
        changed += 1;
    }
    Ok(changed)
}

/// Reads the stored index of project line `line`.
pub(crate) fn load(
    conn: &Connection,
    line: &str,
) -> Result<BTreeMap<(Kind, String), Entry>, SessionError> {
    let mut stmt = conn.prepare(
        "SELECT kind, id, level, tx, ty, xmin, ymin, xmax, ymax FROM spatial_index WHERE line = ?1",
    )?;
    let rows = stmt.query_map(params![line], row)?;
    let mut out = BTreeMap::new();
    for r in rows {
        let (kind, id, e) = r?;
        out.insert((Kind::parse(&kind)?, id), e);
    }
    Ok(out)
}

type Row = (String, String, Entry);

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        Entry {
            tile: Tile {
                level: r.get(2)?,
                x: r.get(3)?,
                y: r.get(4)?,
            },
            bounds: Bounds {
                min: [r.get(5)?, r.get(6)?],
                max: [r.get(7)?, r.get(8)?],
            },
        },
    ))
}

/// Objects of project line `line` whose bounds touch `area`, read from
/// the index in the file (one range query per level in use).
pub(crate) fn query(
    conn: &Connection,
    line: &str,
    grid: &Grid,
    area: &Bounds,
) -> Result<Vec<(Kind, String, Entry)>, SessionError> {
    let levels: Vec<i8> = conn
        .prepare("SELECT DISTINCT level FROM spatial_index WHERE line = ?1 ORDER BY level")?
        .query_map(params![line], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut stmt = conn.prepare_cached(
        "SELECT kind, id, level, tx, ty, xmin, ymin, xmax, ymax FROM spatial_index
         WHERE level = ?1 AND tx BETWEEN ?2 AND ?3 AND ty BETWEEN ?4 AND ?5
           AND xmax >= ?6 AND xmin <= ?7 AND ymax >= ?8 AND ymin <= ?9 AND line = ?10",
    )?;
    let mut out = Vec::new();
    for level in levels {
        let r = grid.query(area, level);
        let rows = stmt.query_map(
            params![
                level,
                r.x[0],
                r.x[1],
                r.y[0],
                r.y[1],
                area.min[0],
                area.max[0],
                area.min[1],
                area.max[1],
                line
            ],
            row,
        )?;
        for x in rows {
            let (kind, id, e) = x?;
            out.push((Kind::parse(&kind)?, id, e));
        }
    }
    out.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(out)
}
