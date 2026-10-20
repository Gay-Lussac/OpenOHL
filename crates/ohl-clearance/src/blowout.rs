// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Blowout: how far wires swung by the wind reach sideways. The swung
//! shapes are the solved shapes of a windy weather case (wind square to
//! each span, suspension strings swinging with it); this module measures
//! them against the edges of the right of way — a half width either side
//! of the route centreline — and reports where they go beyond.
//!
//! Clearances of blown-out wires to points (buildings, trees: LiDAR
//! classes) and to other wires are the radial clearances of the rules
//! ([`crate::rules`]) checked in the windy case.

use ohl_line::Line;
use ohl_line::route::project;

use crate::checks::Wire;
use crate::geometry::P3;

/// How far one wire reaches sideways from the centreline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowResult {
    /// Index of the wire.
    pub wire: usize,
    /// Largest offset to the right of the centreline (the outer
    /// subconductor's surface), m (negative: the wire stays left of it),
    /// and where (the centre line's point).
    pub right: (f64, P3),
    /// Largest offset to the left, m, and where.
    pub left: (f64, P3),
    /// Beyond the right-of-way edge, m: the larger of the two sides'
    /// overshoot (≤ 0: inside, by that margin).
    pub encroachment: f64,
}

/// A route's centreline and the structures on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Centreline {
    /// Structures of the route.
    pub structures: Vec<String>,
    /// The centreline (alignment, else through the structures).
    pub points: Vec<P3>,
}

/// The centrelines of a line's routes.
pub fn centrelines(line: &Line) -> Vec<Centreline> {
    line.alignment_views()
        .iter()
        .map(|r| Centreline {
            structures: r.structures.clone(),
            points: r.centreline(line),
        })
        .filter(|c| c.points.len() >= 2)
        .collect()
}

/// The structure of an attachment key `structure:set:phase`.
fn structure_of(key: &str) -> &str {
    key.rsplitn(3, ':').last().unwrap_or(key)
}

/// The sideways reach of every wire against a right of way `left` and
/// `right` m either side of its route's centreline: the routes holding
/// both structures the wire hangs between (all routes for a wire on none,
/// each point then measured to the nearest). Bundles reach out by their
/// radius, and every wire by its surface ([`Wire::outer_radius`]: the
/// outer envelope of the subconductors' surfaces). Wires without points
/// have no result.
///
/// Cost: wire points × centreline segments.
pub fn row_check(
    wires: &[Wire],
    centrelines: &[Centreline],
    left: f64,
    right: f64,
) -> Vec<RowResult> {
    let mut out = Vec::new();
    for (k, w) in wires.iter().enumerate() {
        let r = w.outer_radius();
        let on = |c: &&Centreline| {
            w.ends.len() == 2
                && w.ends
                    .iter()
                    .all(|e| c.structures.iter().any(|s| s == structure_of(e)))
        };
        let mut routes: Vec<&Centreline> = centrelines.iter().filter(on).collect();
        if routes.is_empty() {
            routes = centrelines.iter().collect();
        }
        let mut best: Option<RowResult> = None;
        for p in &w.points {
            let Some(offset) = routes
                .iter()
                .map(|c| project(&c.points, *p).1)
                .min_by(|a, b| a.abs().total_cmp(&b.abs()))
            else {
                continue;
            };
            let b = best.get_or_insert(RowResult {
                wire: k,
                right: (f64::NEG_INFINITY, *p),
                left: (f64::NEG_INFINITY, *p),
                encroachment: f64::NEG_INFINITY,
            });
            if offset + r > b.right.0 {
                b.right = (offset + r, *p);
            }
            if -offset + r > b.left.0 {
                b.left = (-offset + r, *p);
            }
        }
        if let Some(mut b) = best {
            b.encroachment = (b.right.0 - right).max(b.left.0 - left);
            out.push(b);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(y: f64, bundle: Vec<[f64; 2]>) -> Wire {
        Wire {
            section: "S".into(),
            span: 0,
            circuit: "c".into(),
            phase: "L1".into(),
            potential: None,
            bundle,
            // Swung out to the right (negative y) by up to 3 m mid-span.
            points: (0..=10)
                .map(|k| {
                    let t = k as f64 / 10.0;
                    [t * 300.0, y - 12.0 * t * (1.0 - t), 20.0]
                })
                .collect(),
            ends: vec![],
            surface_radius: 0.0,
            polyline_error: 0.0,
        }
    }

    #[test]
    fn swung_wire_beyond_the_edge() {
        let centre = vec![Centreline {
            structures: vec!["T1".into(), "T2".into()],
            points: vec![[0.0, 0.0, 0.0], [300.0, 0.0, 0.0]],
        }];
        // Outer phase 6 m right of the centreline (y = −6), swung 3 m more:
        // 9 m right; a ROW of 8 m each side: 1 m beyond.
        let r = row_check(&[wire(-6.0, vec![])], &centre, 8.0, 8.0);
        assert!((r[0].right.0 - 9.0).abs() < 1e-9, "{r:?}");
        assert!((r[0].left.0 + 6.0).abs() < 1e-9);
        assert!((r[0].encroachment - 1.0).abs() < 1e-9);
        assert!((r[0].right.1[0] - 150.0).abs() < 1e-9);
        // A twin bundle 0.2 m either side adds its radius; a wider ROW on
        // the right holds it.
        let r = row_check(
            &[wire(-6.0, vec![[-0.2, 0.0], [0.2, 0.0]])],
            &centre,
            8.0,
            10.0,
        );
        assert!((r[0].encroachment + 0.8).abs() < 1e-9, "{r:?}");
        // A branch route crossing near the wire is not its route.
        let mut on_route = wire(-6.0, vec![]);
        on_route.ends = vec!["T1:1:1".into(), "T2:1:1".into()];
        let mut both = centre.clone();
        both.push(Centreline {
            structures: vec!["T2".into(), "B1".into()],
            points: vec![[150.0, -20.0, 0.0], [150.0, 100.0, 0.0]],
        });
        let r = row_check(&[on_route], &both, 8.0, 8.0);
        assert!((r[0].right.0 - 9.0).abs() < 1e-9 && (r[0].left.0 + 6.0).abs() < 1e-9);
    }
}
