// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The point and ground checks as they were before the speed work of
//! 2026-10-20 (one point at a time, every wire piece and sample looked
//! at), kept for the tests only: the faster checks must give the same
//! results, bit for bit.

#![allow(dead_code, clippy::all)]

use super::*;

/// Clearances of every wire to points of chosen classes, for one rule;
/// feed the points with [`OldPointCheck::visit`] (e.g. from a LiDAR reader).
#[derive(Debug, Clone)]
pub struct OldPointCheck {
    rule: Rule,
    classes: [bool; 256],
    /// One result per wire of the set.
    pub results: Vec<WireResult>,
    seen: Vec<usize>,
    groups: Option<Vec<usize>>,
    /// With groups ([`OldPointCheck::grouped`]): the points below the
    /// vertical and the radial requirement of any wire of each group (a
    /// point counted once per group).
    pub group_violations: Vec<[u64; 2]>,
}

impl OldPointCheck {
    /// A check of `rule` on the wires of `wires` against points of
    /// `classes`.
    pub fn new(rule: &Rule, classes: &[u8], wires: &WireSet) -> Self {
        let mut mask = [false; 256];
        for &c in classes {
            mask[c as usize] = true;
        }
        Self {
            rule: rule.clone(),
            classes: mask,
            results: wires
                .wires
                .iter()
                .map(|w| WireResult::with(rule.required(w.kv())))
                .collect(),
            seen: Vec::new(),
            groups: None,
            group_violations: Vec::new(),
        }
    }

    /// Also counts the points too close per group of wires — `group[w]`
    /// for wire `w`, e.g. the positions of one span in several wind
    /// states: a point too close to several of them counts once.
    pub fn grouped(mut self, group: Vec<usize>) -> Self {
        let n = group.iter().max().map_or(0, |g| g + 1);
        self.group_violations = vec![[0, 0]; n];
        self.groups = Some(group);
        self
    }

    /// The rule checked.
    pub fn rule(&self) -> &Rule {
        &self.rule
    }

    /// Takes one point into account.
    pub fn visit(&mut self, wires: &WireSet, x: f64, y: f64, z: f64, class: u8) {
        if !self.classes[class as usize] {
            return;
        }
        let p = [x, y, z];
        // Wires this point violates (each counted once per point).
        let mut vertical_bad: Vec<usize> = Vec::new();
        let mut radial_bad: Vec<usize> = Vec::new();
        self.seen.clear();
        for (w, k) in wires.near(x, y) {
            let (a, b) = wires.segment(w, k);
            let wire = &wires.wires[w];
            let result = &mut self.results[w];
            let reach = (result.required.largest() + self.rule.search_margin).min(wires.reach);
            // To the surface.
            let off = wires.allowances[w];
            // The centre line first: subconductors only when within reach.
            let (q, _) = closest_on_segment(p, a, b);
            let near = distance(p, q) - wires.radii[w] - off <= reach;
            let (d, sub) = if near && wires.radii[w] > 0.0 {
                wire.nearest(p, a, b)
            } else {
                (distance(p, q), q)
            };
            let d = d - off;
            if near && d <= reach {
                let hit = Hit {
                    distance: d,
                    wire: sub,
                    target: p,
                    class: Some(class),
                };
                if result.radial(hit) && !radial_bad.contains(&w) {
                    radial_bad.push(w);
                }
            }
            if let Some((q, _)) = over_in_plan(x, y, a, b, self.rule.vertical_band) {
                // Vertical separation, above or below the wire (a tree
                // overhanging a low wire, a line crossing over); with a
                // depth, to the wire's lowest (highest) in the slice.
                // Measured vertically on the sloping piece (2026-10-26).
                let off = wire.vertical_allowance(a, b);
                let gap = wire.vertical_gap(q, z, a, b)
                    - slice_rise(a, b, self.rule.vertical_depth)
                    - off;
                let hit = Hit {
                    distance: gap.max(0.0),
                    wire: q,
                    target: p,
                    class: Some(class),
                };
                if hit.distance <= reach && result.vertical(hit) && !vertical_bad.contains(&w) {
                    vertical_bad.push(w);
                }
            }
        }
        if let Some(groups) = &self.groups {
            for (k, bad) in [&vertical_bad, &radial_bad].into_iter().enumerate() {
                let mut gs: Vec<usize> = bad.iter().map(|w| groups[*w]).collect();
                gs.sort_unstable();
                gs.dedup();
                for g in gs {
                    self.group_violations[g][k] += 1;
                }
            }
        }
        for w in vertical_bad {
            self.results[w].vertical_violations += 1;
        }
        for w in radial_bad {
            self.results[w].radial_violations += 1;
        }
    }
}

/// Vertical and radial clearances of every wire to the ground surface.
/// Vertical: under every wire piece at every TIN edge crossing (exact for
/// the polyline) — with the rule's `vertical_band`, the highest ground
/// within that horizontal distance of the wire; with its `vertical_depth`,
/// the wire's lowest in a slice of that depth. Radial: wire samples every [`GROUND_STEP`] to the
/// triangles within the rule's reach (narrowed to the vertical clearance
/// where that is smaller: the radial distance is never larger).
pub fn old_ground_check(rule: &Rule, wires: &[Wire], terrain: &Terrain) -> Vec<WireResult> {
    wires
        .iter()
        .map(|wire| {
            let mut result = WireResult::with(rule.required(wire.kv()));
            let reach = result.required.largest() + rule.search_margin;
            // To the surface.
            let off = wire.allowance();
            for s in wire.points.windows(2) {
                let (a, b) = (s[0], s[1]);
                let (mut vertical_bad, mut radial_bad) = (false, false);
                let rise = slice_rise(a, b, rule.vertical_depth);
                // Bundles: the lowest subconductor for the vertical
                // clearance; it and the radius measured vertically on the
                // sloping piece (2026-10-26).
                let (lowest, _) = wire.vertical_extent_on(a, b);
                let off_vertical = wire.vertical_allowance(a, b);
                for (t, g) in ground_under(terrain, a, b, rule.vertical_band) {
                    let z = a[2] + (b[2] - a[2]) * t + lowest;
                    let hit = Hit {
                        distance: z - rise - g[2] - off_vertical,
                        wire: [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, z],
                        target: g,
                        class: None,
                    };
                    vertical_bad |= result.vertical(hit);
                }
                result.vertical_violations += u64::from(vertical_bad);
                if result.required.radial.is_none() {
                    continue;
                }
                let n = ((distance(a, b) / GROUND_STEP).ceil() as usize).max(1);
                // The samples' spacing, at most GROUND_STEP.
                let spacing = distance(a, b) / n as f64;
                for i in 0..=n {
                    let t = i as f64 / n as f64;
                    let centre = [
                        a[0] + (b[0] - a[0]) * t,
                        a[1] + (b[1] - a[1]) * t,
                        a[2] + (b[2] - a[2]) * t,
                    ];
                    // Each subconductor of a bundle (one point for a wire).
                    // The ground straight below bounds the search (radial is
                    // never larger than vertical); where it is far below or
                    // missing (a hole), the ground beside may still be near:
                    // searched over the whole reach.
                    for p in wire.subconductors(centre, a, b) {
                        let below = terrain.height(p[0], p[1]);
                        let radius = below.map_or(reach, |g| (p[2] - g).max(0.0).min(reach));
                        let mut best: Option<(f64, P3)> = None;
                        terrain.triangles_near(p[0], p[1], radius, |[ta, tb, tc]| {
                            let q = closest_on_triangle(p, ta, tb, tc);
                            let d = distance(p, q);
                            if best.is_none_or(|(bd, _)| d < bd) {
                                best = Some((d, q));
                            }
                        });
                        if let Some((d, q)) = best {
                            // Between samples the wire may come nearer:
                            // the lower bound, then to the surface.
                            let d = if below.is_some_and(|g| p[2] < g) {
                                -d
                            } else {
                                sampled_floor(d, spacing)
                            } - off;
                            let hit = Hit {
                                distance: d,
                                wire: p,
                                target: q,
                                class: None,
                            };
                            radial_bad |= result.radial(hit);
                        }
                    }
                }
                result.radial_violations += u64::from(radial_bad);
            }
            result
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSet;
    use ohl_core::units::{Stress, Temperature};
    use ohl_weather::{WeatherCase, WindSide};
    use std::path::Path;

    /// The example line's wires in still air, at 300 and 600 Pa from each
    /// side (the positions of a stepped wind), some made two-wire bundles.
    fn wires() -> Vec<Wire> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let mut out = Vec::new();
        for (pa, side) in [
            (0.0, WindSide::Left),
            (300.0, WindSide::Left),
            (600.0, WindSide::Left),
            (300.0, WindSide::Right),
            (600.0, WindSide::Right),
        ] {
            let mut case = WeatherCase::still("c", Temperature::from_celsius(40.0));
            case.wind_pressure = Stress::from_si(pa);
            case.wind_from = side;
            let shapes: Vec<SectionShape> = ohl_line::solve::solve_line(&line, &library, &case)
                .into_iter()
                .map(Result::unwrap)
                .collect();
            out.extend(wires_of(&line, &shapes, &[]));
        }
        for (k, w) in out.iter_mut().enumerate() {
            if k % 3 == 0 {
                w.bundle = vec![[-0.2, 0.0], [0.2, 0.0]];
            }
        }
        out
    }

    /// `n` invented points around the wires (a fixed sequence).
    fn points(wires: &[Wire], n: usize) -> Vec<([f64; 3], u8)> {
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        (0..n)
            .map(|k| {
                let w = &wires[k % wires.len()];
                let p = w.points[(next() * w.points.len() as f64) as usize % w.points.len()];
                let q = [
                    p[0] + (next() - 0.5) * 30.0,
                    p[1] + (next() - 0.5) * 30.0,
                    p[2] - next() * 25.0 + 3.0,
                ];
                // Some points on whole metres (equal distances, ties).
                let q = if k % 7 == 0 {
                    [q[0].round(), q[1].round(), q[2].round()]
                } else {
                    q
                };
                (q, 3 + (k % 4) as u8)
            })
            .collect()
    }

    fn rule(target: &str, band: f64, depth: f64) -> Rule {
        RuleSet::parse(&format!(
            "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"x\"\n[[rule]]\nname = \"r\"\n{target}\n\
             vertical = \"21 m\"\nradial = \"20 m\"\nvertical_band = \"{band} m\"\nvertical_depth = \"{depth} m\"\n"
        ))
        .unwrap()
        .rules
        .remove(0)
    }

    /// The point check — one point at a time, in stretches worked out from
    /// a snapshot of the results and applied in order (as on several
    /// cores), and with one measure only — gives the results of before,
    /// bit for bit: every wire's clearances (value, wire point, target),
    /// its counts and the counts per span.
    #[test]
    fn the_point_check_gives_the_results_of_before() {
        let wires = wires();
        let cloud = points(&wires, 30_000);
        let groups: Vec<usize> = wires.iter().map(|w| w.span).collect();
        for (band, depth) in [(1.0, 0.2), (0.0, 0.0), (3.0, 1.0)] {
            let r = rule("target = \"points\"\nclasses = [3, 4, 5]", band, depth);
            let set = WireSet::new(wires.clone(), 25.0);
            let mut old = OldPointCheck::new(&r, &[3, 4, 5], &set).grouped(groups.clone());
            let mut new = PointCheck::new(&r, &[3, 4, 5], &set).grouped(groups.clone());
            for (p, c) in &cloud {
                old.visit(&set, p[0], p[1], p[2], *c);
                new.visit(&set, p[0], p[1], p[2], *c);
            }
            assert_eq!(new.results, old.results);
            assert_eq!(new.group_violations, old.group_violations);
            assert!(old.group_violations.iter().any(|g| g[0] > 0 && g[1] > 0));
            // The segments in blocks by group (passed over block by block,
            // 2026-10-26): the same.
            let by_group = WireSet::grouped(wires.clone(), 25.0, &groups);
            let mut grouped = PointCheck::new(&r, &[3, 4, 5], &by_group).grouped(groups.clone());
            for (p, c) in &cloud {
                grouped.visit(&by_group, p[0], p[1], p[2], *c);
            }
            assert_eq!(grouped.results, old.results);
            assert_eq!(grouped.group_violations, old.group_violations);
            // In stretches from a snapshot, applied in order.
            let mut par = PointCheck::new(&r, &[3, 4, 5], &set).grouped(groups.clone());
            let mut stamp = 0;
            for block in cloud.chunks(4096) {
                let mut all = Vec::new();
                for stretch in block.chunks(1000) {
                    let mut best = par.best.clone();
                    for (p, c) in stretch {
                        let mut ev = Vec::new();
                        stamp += 1;
                        let taken =
                            par.eval(&set, &mut best, None, stamp, p[0], p[1], p[2], *c, &mut ev);
                        all.push((taken, ev));
                    }
                }
                for (taken, ev) in all {
                    if taken {
                        par.apply(&ev);
                    }
                }
            }
            assert_eq!(par.results, old.results);
            assert_eq!(par.group_violations, old.group_violations);
            // The counts tallied per stretch, the hits applied in order.
            let mut tal = PointCheck::new(&r, &[3, 4, 5], &set).grouped(groups.clone());
            for block in cloud.chunks(4096) {
                let mut all = Vec::new();
                let mut tallies = Vec::new();
                for stretch in block.chunks(1000) {
                    let mut best = tal.best.clone();
                    let mut t = Tally::new(set.wires.len(), tal.groups());
                    for (p, c) in stretch {
                        let mut ev = Vec::new();
                        stamp += 1;
                        let taken = tal.eval(
                            &set,
                            &mut best,
                            Some(&mut t),
                            stamp,
                            p[0],
                            p[1],
                            p[2],
                            *c,
                            &mut ev,
                        );
                        assert!(ev.iter().all(|e| !e.bad));
                        all.push((taken, ev));
                    }
                    tallies.push(t);
                }
                for (taken, ev) in all {
                    if taken {
                        tal.apply(&ev);
                    }
                }
                for t in &tallies {
                    tal.take_tally(t);
                }
            }
            assert_eq!(tal.results, old.results);
            assert_eq!(tal.group_violations, old.group_violations);
            // One measure only: that measure's results.
            for (v, rad) in [(true, false), (false, true)] {
                let mut one = PointCheck::new(&r, &[3, 4, 5], &set)
                    .only(v, rad)
                    .grouped(groups.clone());
                for (p, c) in &cloud {
                    one.visit(&set, p[0], p[1], p[2], *c);
                }
                for (a, b) in one.results.iter().zip(&old.results) {
                    if v {
                        assert_eq!(
                            (a.vertical, a.vertical_violations),
                            (b.vertical, b.vertical_violations)
                        );
                    } else {
                        assert_eq!(
                            (a.radial, a.radial_violations),
                            (b.radial, b.radial_violations)
                        );
                    }
                }
                let k = usize::from(rad);
                for (a, b) in one.group_violations.iter().zip(&old.group_violations) {
                    assert_eq!(a[k], b[k]);
                }
            }
        }
    }

    /// The ground check (bounds from the highest ground, every core) gives
    /// the results of before, bit for bit — with and without a band and a
    /// depth, with clearances below the requirement and above it.
    #[test]
    fn the_ground_check_gives_the_results_of_before() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let terrain =
            Terrain::load(&root.join("examples/synthetic-line/synthetic-survey.csv")).unwrap();
        let wires = wires();
        for (band, depth) in [(0.0, 0.0), (1.5, 0.2)] {
            let r = rule("target = \"ground\"", band, depth);
            let old = old_ground_check(&r, &wires, &terrain);
            let new = ground_check(&r, &wires, &terrain);
            assert_eq!(new, old);
            // In groups with a shared bound (2026-10-26): each group's
            // lowest (the first of equal ones) and each wire's counts.
            let groups: Vec<usize> = wires.iter().map(|w| w.span).collect();
            let top = crate::checks::GroundMax::new(&terrain);
            let grouped = crate::checks::ground_check_grouped(&r, &wires, &groups, &terrain, &top);
            let lowest = |results: &[WireResult], g: usize, vertical: bool| {
                let mut best: Option<Hit> = None;
                for (k, x) in results.iter().enumerate() {
                    let hit = if vertical { x.vertical } else { x.radial };
                    if groups[k] == g
                        && let Some(h) = hit
                        && best.is_none_or(|b| h.distance < b.distance)
                    {
                        best = Some(h);
                    }
                }
                best
            };
            for g in 0..=groups.iter().copied().max().unwrap_or(0) {
                for vertical in [true, false] {
                    assert_eq!(
                        lowest(&grouped, g, vertical),
                        lowest(&old, g, vertical),
                        "group {g}"
                    );
                }
            }
            for (a, b) in grouped.iter().zip(&old) {
                assert_eq!(a.vertical_violations, b.vertical_violations);
                assert_eq!(a.radial_violations, b.radial_violations);
            }
            assert!(
                old.iter()
                    .any(|x| x.vertical_violations > 0 && x.radial_violations > 0)
            );
            assert!(old.iter().any(|x| x.vertical_violations == 0));
        }
    }
}
