// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The subconductors of a solved line, joined up at the clamps.
//!
//! At every clamp of a bundled section the subconductors are held by a
//! yoke: a bar for a twin bundle, a triangle for a triple one, a polygon
//! for more ([`BundleLines::yokes`]). Its corners are the subconductors'
//! positions across the bisector of the spans meeting there, so the back
//! and ahead spans end on the same corners. Jumpers end on the corners of
//! the yokes at their two ends: each subconductor on the nearest free
//! corner, shifted smoothly along its length to reach it.

use std::collections::HashMap;

use crate::jumper::JumperShape;
use crate::solve::SectionShape;
use crate::{Line, Point, SectionPoint, bundle_point};

fn key(p: &SectionPoint) -> (String, u32) {
    (p.structure.clone(), p.set)
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n > 1e-12).then(|| v.map(|x| x / n))
}

fn sub(a: Point, b: Point) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Subconductor polylines of spans and jumpers, and the clamp yokes.
#[derive(Debug, Clone, Default)]
pub struct BundleLines {
    /// Per (section id, span index): one polyline per subconductor (one,
    /// the wire itself, for a single wire).
    pub spans: HashMap<(String, usize), Vec<Vec<Point>>>,
    /// Per jumper id: one polyline per subconductor.
    pub jumpers: HashMap<String, Vec<Vec<Point>>>,
    /// Clamp yokes of bundles: the corners in order (a bar for two).
    pub yokes: Vec<Vec<Point>>,
    /// The section of each yoke (same order as `yokes`).
    pub yoke_sections: Vec<String>,
}

impl BundleLines {
    /// The subconductors of the solved `shapes` and `jumpers` of `line`.
    pub fn new(line: &Line, shapes: &[SectionShape], jumpers: &[JumperShape]) -> Self {
        let mut out = BundleLines::default();
        // Corners at each clamp, by attachment.
        let mut clamps: HashMap<(String, u32), Vec<Point>> = HashMap::new();
        for shape in shapes {
            let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
                continue;
            };
            let bundle = section.bundle_geometry();
            let offsets = bundle.offsets();
            let n_spans = shape.spans.len();
            // Corners at clamp k (between span k−1 and span k).
            let corners: Vec<Option<Vec<Point>>> = (0..=n_spans)
                .map(|k| {
                    if offsets.len() <= 1 {
                        return None;
                    }
                    let incoming = k
                        .checked_sub(1)
                        .and_then(|i| shape.spans.get(i))
                        .and_then(|s| {
                            let n = s.points.len();
                            (n >= 2).then(|| sub(s.points[n - 1], s.points[n - 2]))
                        })
                        .and_then(unit);
                    let outgoing = shape
                        .spans
                        .get(k)
                        .and_then(|s| (s.points.len() >= 2).then(|| sub(s.points[1], s.points[0])))
                        .and_then(unit);
                    let q = match (
                        k.checked_sub(1).and_then(|i| shape.spans.get(i)),
                        shape.spans.get(k),
                    ) {
                        (_, Some(s)) => s.points[0],
                        (Some(s), None) => *s.points.last()?,
                        (None, None) => return None,
                    };
                    let d = match (incoming, outgoing) {
                        (Some(a), Some(b)) => {
                            unit([a[0] + b[0], a[1] + b[1], a[2] + b[2]]).unwrap_or(b)
                        }
                        (Some(a), None) => a,
                        (None, Some(b)) => b,
                        (None, None) => return None,
                    };
                    let (a, b) = (sub(q, d), [q[0] + d[0], q[1] + d[1], q[2] + d[2]]);
                    Some(offsets.iter().map(|o| bundle_point(q, a, b, *o)).collect())
                })
                .collect();
            for (k, span) in shape.spans.iter().enumerate() {
                let mut lines = bundle.polylines(&span.points);
                if let (Some(start), Some(end)) = (&corners[k], &corners[k + 1]) {
                    for (i, l) in lines.iter_mut().enumerate() {
                        if let Some(first) = l.first_mut() {
                            *first = start[i];
                        }
                        if let Some(last) = l.last_mut() {
                            *last = end[i];
                        }
                    }
                }
                out.spans.insert((shape.section.clone(), k), lines);
            }
            for (k, c) in corners.into_iter().enumerate() {
                if let (Some(c), Some(p)) = (c, section.points.get(k)) {
                    clamps.entry(key(p)).or_insert_with(|| c.clone());
                    out.yokes.push(c);
                    out.yoke_sections.push(shape.section.clone());
                }
            }
        }
        for j in jumpers {
            let Some(def) = line.jumpers.iter().find(|d| d.id == j.jumper) else {
                continue;
            };
            let mut lines = line.jumper_bundle(def).polylines(&j.points);
            for (end, at_start) in [(&def.from, true), (&def.to, false)] {
                if let Some(p) = end.point()
                    && let Some(corners) = clamps.get(&key(p))
                    && corners.len() == lines.len()
                {
                    snap_ends(&mut lines, corners, at_start);
                }
            }
            out.jumpers.insert(j.jumper.clone(), lines);
        }
        out
    }
}

/// Moves the start (or end) of each subconductor polyline onto the
/// nearest free corner, shifting the polyline linearly along its points
/// (fully at that end, not at all at the other).
fn snap_ends(lines: &mut [Vec<Point>], corners: &[Point], at_start: bool) {
    let mut free: Vec<bool> = vec![true; corners.len()];
    for l in lines.iter_mut() {
        let n = l.len();
        if n == 0 {
            continue;
        }
        let end = if at_start { l[0] } else { l[n - 1] };
        let Some((c, _)) = corners
            .iter()
            .enumerate()
            .filter(|(i, _)| free[*i])
            .map(|(i, p)| (i, sub(*p, end)))
            .min_by(|a, b| {
                let d = |v: [f64; 3]| v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
                d(a.1).total_cmp(&d(b.1))
            })
        else {
            continue;
        };
        free[c] = false;
        let shift = sub(corners[c], end);
        for (i, p) in l.iter_mut().enumerate() {
            let t = if n > 1 {
                i as f64 / (n - 1) as f64
            } else {
                0.0
            };
            let w = if at_start { 1.0 - t } else { t };
            for k in 0..3 {
                p[k] += w * shift[k];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jumper::solve_jumpers;
    use crate::solve::solve_line;
    use ols_core::units::Temperature;
    use ols_weather::WeatherCase;

    fn dist(a: Point, b: Point) -> f64 {
        let d = sub(a, b);
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    #[test]
    fn spans_and_jumpers_end_on_the_yokes() {
        let mut line = crate::tests::example();
        for s in &mut line.sections {
            s.bundle = 3;
            s.bundle_spacing = Some(0.4);
        }
        line.validate().unwrap();
        let lib = ols_cables::Library::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let shapes: Vec<SectionShape> = solve_line(&line, &lib, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let jumpers: Vec<JumperShape> = solve_jumpers(&line, &lib, &case, &shapes)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let b = BundleLines::new(&line, &shapes, &jumpers);
        // Every clamp has a triangle with sides of the spacing.
        let clamps: usize = line.sections.iter().map(|s| s.points.len()).sum();
        assert_eq!(b.yokes.len(), clamps);
        for y in &b.yokes {
            assert_eq!(y.len(), 3);
            for k in 0..3 {
                assert!((dist(y[k], y[(k + 1) % 3]) - 0.4).abs() < 1e-9);
            }
        }
        // Back and ahead spans of S1 at T2 end on the same corners.
        let back = &b.spans[&("S1".to_owned(), 0)];
        let ahead = &b.spans[&("S1".to_owned(), 1)];
        for (x, y) in back.iter().zip(ahead) {
            assert!(dist(*x.last().unwrap(), y[0]) < 1e-12);
        }
        // The jumper's three subconductors end on corners of the yokes at
        // its two ends (S3 at T3 set 2, S1 at T3 set 1).
        let j = &b.jumpers["J-T3-branch"];
        assert_eq!(j.len(), 3);
        let all: Vec<Point> = b.yokes.iter().flatten().copied().collect();
        for l in j {
            for end in [l[0], *l.last().unwrap()] {
                assert!(all.iter().any(|c| dist(*c, end) < 1e-12), "{end:?}");
            }
        }
        // Single wires: no yokes, the wire itself.
        let plain = crate::tests::example();
        let shapes1: Vec<SectionShape> = solve_line(&plain, &lib, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let b1 = BundleLines::new(&plain, &shapes1, &[]);
        assert!(b1.yokes.is_empty());
        assert_eq!(
            b1.spans[&("S1".to_owned(), 0)],
            vec![shapes1[0].spans[0].points.clone()]
        );
    }
}
