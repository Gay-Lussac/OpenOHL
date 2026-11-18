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
use ohl_line::index::{LineIndex, Of};
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

fn clamp_point(line: &Line, index: &LineIndex, p: &SectionPoint) -> Option<Point> {
    match index.wire_point(line, p) {
        Some(w) => Some(w.position),
        None => index.structure(line, &p.structure).map(|s| s.position),
    }
}

/// The plan bounds of object `kind`/`id` of `line` (indexed by `index`),
/// if it has a place.
pub fn bounds_of(line: &Line, index: &LineIndex, kind: Kind, id: &str) -> Option<Bounds> {
    let of = of_kind(kind)?;
    bounds_at(line, index, of, index.position(of, id)?)
}

/// The kind of line object of kind `kind`.
fn of_kind(kind: Kind) -> Option<Of> {
    Some(match kind {
        Kind::Route => Of::Route,
        Kind::Alignment => Of::Alignment,
        Kind::AlignmentPoint => Of::AlignmentPoint,
        Kind::Structure => Of::Structure,
        Kind::Section => Of::Section,
        Kind::Jumper => Of::Jumper,
        Kind::Spacer => Of::Spacer,
        Kind::Meta | Kind::Report | Kind::Library | Kind::PointCloud | Kind::Annotation => {
            return None;
        }
    })
}

/// The plan bounds of the object of kind `of` at place `k` of `line` (the
/// objects it refers to found by `index`).
fn bounds_at(line: &Line, index: &LineIndex, of: Of, k: usize) -> Option<Bounds> {
    let points: Vec<Point> = match of {
        Of::Structure => {
            let s = &line.structures[k];
            let mut pts = vec![s.position];
            pts.extend(s.points.iter().map(|p| p.position));
            pts.extend(s.wires.iter().map(|w| w.position));
            pts.extend(s.members.iter().flatten());
            pts
        }
        Of::Section => line.sections[k]
            .points
            .iter()
            .filter_map(|p| clamp_point(line, index, p))
            .collect(),
        Of::Jumper => {
            let j = &line.jumpers[k];
            // Its ends: wire points, and taps where they are.
            let mut pts: Vec<Point> = [&j.from, &j.to]
                .into_iter()
                .filter_map(|e| match e.point() {
                    Some(p) => clamp_point(line, index, p),
                    None => ohl_line::jumper::approximate_end_in(line, index, e, 0),
                })
                .collect();
            pts.extend(j.support().iter().map(|s| s.point));
            // Its support clamps, where their wire points hang.
            pts.extend(j.supports().iter().filter_map(|c| {
                index
                    .structure(line, c.structure_id(j))
                    .and_then(|s| s.wire(&c.wire))
                    .map(|w| w.position)
            }));
            pts
        }
        // Spacers: where their ends are on the wires.
        Of::Spacer => {
            let s = &line.spacers[k];
            [&s.a, &s.b]
                .into_iter()
                .filter_map(|e| {
                    ohl_line::jumper::approximate_end_in(
                        line,
                        index,
                        &ohl_line::JumperEnd::tap(ohl_line::Tap::Span(e.clone())),
                        0,
                    )
                })
                .collect()
        }
        Of::AlignmentPoint => vec![line.alignment_points[k].position],
        Of::Alignment => points_of(line, index, &line.alignments[k]),
        // A route: where its alignments are (in the line's order).
        Of::Route => {
            let id = &line.routes[k].id;
            let mut on: Vec<usize> = index
                .referrers_of(Of::Route, id, Of::Alignment)
                .filter_map(|a| index.position(Of::Alignment, a))
                .filter(|&a| line.alignments[a].route == *id)
                .collect();
            on.sort_unstable();
            on.into_iter()
                .flat_map(|a| points_of(line, index, &line.alignments[a]))
                .collect()
        }
    };
    Bounds::of_points(&points)
}

/// The positions of alignment `a`'s points (as [`Line::points_of`]).
fn points_of(line: &Line, index: &LineIndex, a: &ohl_line::Alignment) -> Vec<Point> {
    a.points
        .iter()
        .filter_map(|p| index.alignment_point(line, p).map(|p| p.position))
        .collect()
}

/// The plan bounds of every object of `line` that has a place.
pub fn object_bounds(line: &Line) -> BTreeMap<(Kind, String), Bounds> {
    let index = LineIndex::of(line);
    let mut out = BTreeMap::new();
    for (kind, of) in [
        (Kind::Structure, Of::Structure),
        (Kind::Section, Of::Section),
        (Kind::Jumper, Of::Jumper),
        (Kind::Spacer, Of::Spacer),
        (Kind::AlignmentPoint, Of::AlignmentPoint),
        (Kind::Alignment, Of::Alignment),
        (Kind::Route, Of::Route),
    ] {
        for k in 0..ohl_line::index::count(line, of) {
            // (Of an id given twice, the last: as written before.)
            if let Some(b) = bounds_at(line, &index, of, k) {
                out.insert((kind, id_at(line, of, k).to_owned()), b);
            }
        }
    }
    out
}

fn id_at(line: &Line, of: Of, k: usize) -> &str {
    match of {
        Of::Route => &line.routes[k].id,
        Of::Alignment => &line.alignments[k].id,
        Of::AlignmentPoint => &line.alignment_points[k].id,
        Of::Structure => &line.structures[k].id,
        Of::Section => &line.sections[k].id,
        Of::Jumper => &line.jumpers[k].id,
        Of::Spacer => &line.spacers[k].id,
    }
}

/// The index entry of object `kind`/`id` of `line` on `grid`.
pub fn entry_of(
    line: &Line,
    index: &LineIndex,
    grid: &Grid,
    kind: Kind,
    id: &str,
) -> Option<Entry> {
    bounds_of(line, index, kind, id).map(|bounds| Entry {
        bounds,
        tile: grid.home(&bounds),
    })
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
