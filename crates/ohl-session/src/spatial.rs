// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The spatial index of the project file (ADR-0008).
//!
//! Every model object with a place in the plan has its **plan bounds** and
//! its **home tile** of the project's chunk grid ([`ohl_core::grid`]) in
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

use ohl_core::grid::{Bounds, Grid, Tile};
use ohl_line::{Line, Point, SectionPoint};

use crate::Kind;

/// One indexed object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// Plan bounds, m.
    pub bounds: Bounds,
    /// Home tile.
    pub tile: Tile,
}

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
                None => ohl_line::jumper::approximate_end(line, e, 0),
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
                ohl_line::jumper::approximate_end(
                    line,
                    &ohl_line::JumperEnd::tap(ohl_line::Tap::Span(e.clone())),
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
