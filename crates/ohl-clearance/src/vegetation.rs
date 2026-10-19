// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Vegetation encroachment: trees near the wires of a weather case (a hot
//! case for sag, a windy one for blowout), now and after they have grown,
//! and trees that could reach a wire when they fall.
//!
//! Distances are to the conductor's surface (iced in a case with ice),
//! as every clearance ([`crate::checks`]).
//!
//! - **Encroaching**: a vegetation point closer to the wire (radially, to
//!   the nearest subconductor) than the required clearance plus the growth
//!   until the next cut (`growth_per_year × years`, grown in every
//!   direction).
//! - **Fall-in**: a vegetation point of height h above the ground (the
//!   TIN below it) is taken as a tree top on a trunk straight below it; a
//!   falling tree pivots at its base and its top sweeps a sphere of radius
//!   h (+ growth) about the base. The tree can reach the wire when the
//!   wire comes within that sphere: margin = distance(base, wire) − h −
//!   growth < 0. Leaning trees, and the crown's width, are not modelled.
//!
//! Results are per wire (a span of a section): counts, the nearest point,
//! the worst fall-in, and the worst points by margin.

use ohl_terrain::Terrain;

use crate::checks::{Wire, WireSet};
use crate::geometry::{P3, closest_on_segment, distance};

/// Search margin beyond the clearance, m: the nearest point is reported
/// when within clearance + growth + this.
pub const SEARCH_MARGIN: f64 = 5.0;

/// What the vegetation check looks for.
#[derive(Debug, Clone, PartialEq)]
pub struct VegetationParams {
    /// Required radial clearance from the wire, m.
    pub clearance: f64,
    /// Growth per year, m.
    pub growth_per_year: f64,
    /// Years until the next cut.
    pub years: f64,
    /// Look for trees that could fall onto the wires (needs the ground).
    pub fall_in: bool,
    /// Points lower than this above the ground are not trees, m.
    pub min_tree_height: f64,
    /// The tallest tree looked for, m (how far from the wires bases are
    /// searched).
    pub max_tree_height: f64,
    /// Worst points kept per wire.
    pub keep: usize,
}

impl Default for VegetationParams {
    fn default() -> Self {
        Self {
            clearance: 5.0,
            growth_per_year: 0.0,
            years: 0.0,
            fall_in: false,
            min_tree_height: 2.0,
            max_tree_height: 40.0,
            keep: 10,
        }
    }
}

impl VegetationParams {
    /// Growth until the next cut, m.
    pub fn growth(&self) -> f64 {
        (self.growth_per_year * self.years).max(0.0)
    }
}

/// One vegetation point near a wire.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VegPoint {
    /// The point.
    pub point: P3,
    /// Its class.
    pub class: u8,
    /// Radial distance to the wire's surface, m (`INFINITY`: beyond the
    /// search).
    pub distance: f64,
    /// Nearest point of the wire (its axis).
    pub wire_point: P3,
    /// Height above the ground, m (none: no ground below it).
    pub height: Option<f64>,
    /// Fall-in margin: distance from the tree's base to the wire's surface
    /// minus the grown tree's height, m (negative: it can reach the wire).
    pub fall_margin: Option<f64>,
}

impl VegPoint {
    /// The smaller of the clearance margin (distance − growth − required)
    /// and the fall-in margin, m.
    pub fn margin(&self, p: &VegetationParams) -> f64 {
        let near = self.distance - p.growth() - p.clearance;
        self.fall_margin.map_or(near, |f| near.min(f))
    }
}

/// The vegetation found near one wire.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WireVegetation {
    /// Points closer than the clearance plus growth.
    pub encroaching: u64,
    /// Points of trees that could fall onto the wire.
    pub fall_in: u64,
    /// The nearest point within the search.
    pub nearest: Option<VegPoint>,
    /// The tree with the smallest fall-in margin.
    pub worst_fall: Option<VegPoint>,
    /// The worst points by margin (encroaching or falling in), worst first.
    pub worst: Vec<VegPoint>,
}

/// The vegetation check of a set of wires; feed points with
/// [`VegetationCheck::visit`], then take the results with
/// [`VegetationCheck::finish`].
///
/// Cost: per point, the wire segments indexed in its 10 m cell — with a
/// 40 m tree search a few hundred: about 1 µs a point, 10 s for 10⁷
/// vegetation points.
#[derive(Debug, Clone)]
pub struct VegetationCheck {
    params: VegetationParams,
    classes: [bool; 256],
    set: WireSet,
    results: Vec<WireVegetation>,
    /// Points looked at.
    pub points: u64,
    /// Points with no ground below them (no fall-in check).
    pub without_ground: u64,
    touched: Vec<(usize, f64, P3, f64)>,
}

impl VegetationCheck {
    /// A check of `wires` against points of `classes`.
    pub fn new(wires: Vec<Wire>, classes: &[u8], params: VegetationParams) -> Self {
        let mut mask = [false; 256];
        for &c in classes {
            mask[c as usize] = true;
        }
        let growth = params.growth();
        let mut reach = params.clearance + growth + SEARCH_MARGIN;
        if params.fall_in {
            reach = reach.max(params.max_tree_height + growth);
        }
        let n = wires.len();
        Self {
            params,
            classes: mask,
            set: WireSet::new(wires, reach),
            results: vec![WireVegetation::default(); n],
            points: 0,
            without_ground: 0,
            touched: Vec::new(),
        }
    }

    /// The wires checked.
    pub fn wires(&self) -> &[Wire] {
        &self.set.wires
    }

    /// Takes one point into account, with the ground below it for the
    /// fall-in check.
    pub fn visit(&mut self, ground: Option<&Terrain>, x: f64, y: f64, z: f64, class: u8) {
        if !self.classes[class as usize] {
            return;
        }
        self.points += 1;
        let p = [x, y, z];
        let params = &self.params;
        let growth = params.growth();
        let near_reach = params.clearance + growth + SEARCH_MARGIN;
        let g = ground.and_then(|t| t.height(x, y));
        if params.fall_in && g.is_none() {
            self.without_ground += 1;
        }
        let height = g.map(|g| z - g);
        let tree = params.fall_in
            && height.is_some_and(|h| h >= params.min_tree_height && h <= params.max_tree_height);
        let base = [x, y, g.unwrap_or(z)];
        // Per wire: (wire, distance, wire point, base distance).
        self.touched.clear();
        for (w, k) in self.set.near(x, y) {
            let (a, b) = self.set.segment(w, k);
            let wire = &self.set.wires[w];
            let radius = self.set.radii[w];
            // To the surface.
            let off = self.set.allowances[w];
            let (q, _) = closest_on_segment(p, a, b);
            let mut d = (f64::INFINITY, q);
            if distance(p, q) - radius - off <= near_reach {
                d = if radius > 0.0 {
                    wire.nearest(p, a, b)
                } else {
                    (distance(p, q), q)
                };
                d.0 -= off;
            }
            let mut db = f64::INFINITY;
            if tree {
                let (qb, _) = closest_on_segment(base, a, b);
                db = distance(base, qb) - radius - off;
            }
            match self.touched.iter_mut().find(|t| t.0 == w) {
                Some(t) => {
                    if d.0 < t.1 {
                        (t.1, t.2) = d;
                    }
                    t.3 = t.3.min(db);
                }
                None => self.touched.push((w, d.0, d.1, db)),
            }
        }
        for &(w, d, q, db) in &self.touched {
            let fall_margin = (tree && db.is_finite())
                .then(|| db - height.expect("a tree has a height") - growth);
            let vp = VegPoint {
                point: p,
                class,
                distance: d,
                wire_point: q,
                height,
                fall_margin,
            };
            let r = &mut self.results[w];
            let encroaching = d - growth < params.clearance;
            let falls = fall_margin.is_some_and(|f| f < 0.0);
            r.encroaching += u64::from(encroaching);
            r.fall_in += u64::from(falls);
            if d.is_finite() && r.nearest.is_none_or(|n| d < n.distance) {
                r.nearest = Some(vp);
            }
            if let Some(f) = fall_margin
                && r.worst_fall
                    .is_none_or(|n| n.fall_margin.is_none_or(|m| f < m))
            {
                r.worst_fall = Some(vp);
            }
            if (encroaching || falls) && params.keep > 0 {
                r.worst.push(vp);
                if r.worst.len() >= 4 * params.keep {
                    r.worst
                        .sort_by(|a, b| a.margin(params).total_cmp(&b.margin(params)));
                    r.worst.truncate(params.keep);
                }
            }
        }
    }

    /// The results, one per wire (in the order given).
    pub fn finish(mut self) -> Vec<WireVegetation> {
        let p = &self.params;
        for r in &mut self.results {
            r.worst.sort_by(|a, b| a.margin(p).total_cmp(&b.margin(p)));
            r.worst.truncate(p.keep);
        }
        self.results
    }
}

/// One row per wire with vegetation within the search, and its worst
/// points, as CSV.
pub fn to_csv(
    case: &str,
    wires: &[Wire],
    results: &[WireVegetation],
    p: &VegetationParams,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::from(CSV_HEADER);
    let f = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.3}"));
    for (w, r) in wires.iter().zip(results) {
        let rows: Vec<(&str, &VegPoint)> = r
            .nearest
            .iter()
            .map(|v| ("nearest", v))
            .chain(r.worst_fall.iter().map(|v| ("worst_fall", v)))
            .chain(r.worst.iter().map(|v| ("worst", v)))
            .collect();
        for (kind, v) in rows {
            let _ = writeln!(
                out,
                "{},{},{},{kind},{},{},{},{:.3},{:.3},{:.3},{},{},{},{},{:.3}",
                ohl_core::text::csv_field(case),
                ohl_core::text::csv_field(&w.section),
                w.span,
                r.encroaching,
                r.fall_in,
                v.class,
                v.point[0],
                v.point[1],
                v.point[2],
                f(v.distance.is_finite().then_some(v.distance)),
                f(v.height),
                f(v.fall_margin),
                f(Some(p.growth())),
                v.margin(p),
            );
        }
    }
    out
}

/// Header of [`to_csv`].
pub const CSV_HEADER: &str = "case,section,span,kind,encroaching,fall_in,class,x,y,z,distance_m,height_m,fall_margin_m,growth_m,margin_m\n";

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_terrain::SurveyPoint;

    fn level_wire() -> Wire {
        Wire {
            section: "S".into(),
            span: 0,
            circuit: "c".into(),
            phase: "L1".into(),
            potential: None,
            bundle: vec![],
            points: (0..=50).map(|k| [k as f64 * 6.0, 0.0, 20.0]).collect(),
            ends: vec![],
            surface_radius: 0.0,
            polyline_error: 0.0,
        }
    }

    fn flat(z: f64) -> Terrain {
        let p = |x: f64, y: f64| SurveyPoint {
            x,
            y,
            z,
            code: String::new(),
            line: String::new(),
        };
        Terrain::from_points(
            &[
                p(-100.0, -100.0),
                p(400.0, -100.0),
                p(400.0, 100.0),
                p(-100.0, 100.0),
            ],
            None,
        )
        .unwrap()
    }

    #[test]
    fn encroaching_growing_and_falling_trees() {
        let ground = flat(0.0);
        let params = VegetationParams {
            clearance: 4.0,
            growth_per_year: 0.5,
            years: 2.0,
            fall_in: true,
            ..VegetationParams::default()
        };
        let mut check = VegetationCheck::new(vec![level_wire()], &[5], params.clone());
        // A tree top 3 m below the wire: encroaching (3 < 4 + 1).
        check.visit(Some(&ground), 50.0, 0.0, 17.0, 5);
        // 4.5 m to the side at wire height: encroaching only after growth;
        // a 20 m tree whose base is √(4.5² + 20²) = 20.5 m from the wire
        // falls in once grown (margin −0.5).
        check.visit(Some(&ground), 100.0, 4.5, 20.0, 5);
        // 7 m to the side: clear (7 − 1 > 4); 7 m from the wire's foot
        // line but a 15 m tree whose base is √(7² + 20²) = 21.2 m from
        // the wire: no fall-in.
        check.visit(Some(&ground), 150.0, 7.0, 15.0, 5);
        // A 25 m tree 12 m to the side: base √(12² + 20²) = 23.3 m from
        // the wire, < 25 + 1: falls in, margin −2.68.
        check.visit(Some(&ground), 200.0, 12.0, 25.0, 5);
        // Another class: ignored.
        check.visit(Some(&ground), 50.0, 0.0, 19.0, 2);
        assert_eq!(check.points, 4);
        let r = &check.finish()[0];
        assert_eq!((r.encroaching, r.fall_in), (2, 2));
        assert!((r.nearest.unwrap().distance - 3.0).abs() < 1e-9);
        let f = r.worst_fall.unwrap();
        assert!((f.fall_margin.unwrap() - (12f64.hypot(20.0) - 26.0)).abs() < 1e-9);
        assert_eq!(f.height, Some(25.0));
        // Worst first: the falling tree (−2.68), then the tree below the
        // wire (3 − 1 − 4 = −2), then the 20 m tree (−0.5).
        let margins: Vec<f64> = r.worst.iter().map(|v| v.margin(&params)).collect();
        assert_eq!(margins.len(), 3);
        assert!(margins.windows(2).all(|w| w[0] <= w[1]), "{margins:?}");
        assert!((margins[0] - (12f64.hypot(20.0) - 26.0)).abs() < 1e-9);
        let csv = to_csv("hot", &[level_wire()], std::slice::from_ref(r), &params);
        assert_eq!(csv.lines().count(), 1 + 2 + 3);
    }
}
