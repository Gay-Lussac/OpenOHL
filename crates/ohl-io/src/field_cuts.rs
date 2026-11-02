// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What a field plan template asks the host to show along each cut
//! (owner 2026-11-02: "choose the point class and dxf or tin we want to
//! see in the cut"), gathered into the cuts' documents (`ohl-field-cut`,
//! `ohl_electrical::field_results::split_set`) before a plugin's report:
//!
//! - `points`: the point cloud's points of the plan's classes within its
//!   band of the cut's plane and within the cut's span, as `[offset,
//!   elevation, class]` (one pass over the cloud for every cut; at most
//!   [`MAX_CUT_POINTS`] a cut, the rest thinned evenly, said);
//! - `sections`: a TIN along the cut (`{"kind": "tin", "name", "lines":
//!   [[[offset, elevation], …], …]}`, broken where the surface has no
//!   height) or a drawing's 3D lines where they cross the cut's plane
//!   (`{"kind": "dxf", "name", "marks": [[offset, elevation, layer,
//!   "#rrggbb"], …]}`).

use ohl_core::budget::Budget;
use ohl_lidar::Corridor;
use ohl_pointcloud::PointSource;
use serde_json::{Value, json};

/// The most points of a cut handed to the plugin.
pub const MAX_CUT_POINTS: usize = 200_000;

/// A cut's vertical plane: its origin and unit direction to the right in
/// plan, and its span of offsets, m.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutPlane {
    /// Where offset 0 lies, plan.
    pub origin: [f64; 2],
    /// Unit vector of increasing offset, plan.
    pub right: [f64; 2],
    /// First and last offset, m.
    pub span: [f64; 2],
}

impl CutPlane {
    /// The plane of a cut's document (`plane`, and `span` or `offsets`
    /// of its first field); none without them.
    pub fn of(doc: &Value) -> Option<Self> {
        let pair = |v: &Value| Some([v.get(0)?.as_f64()?, v.get(1)?.as_f64()?]);
        let origin = pair(&doc["plane"]["origin"])?;
        let right = pair(&doc["plane"]["right"])?;
        let offsets = &doc["fields"][0]["offsets"];
        let span = match offsets.as_array() {
            Some(o) if !o.is_empty() => [o.first()?.as_f64()?, o.last()?.as_f64()?],
            _ => pair(&doc["span"])?,
        };
        Some(Self {
            origin,
            right,
            span,
        })
    }

    /// The offset along the cut and the distance off its plane of plan
    /// point `x`, `y`.
    pub fn locate(&self, x: f64, y: f64) -> (f64, f64) {
        let (dx, dy) = (x - self.origin[0], y - self.origin[1]);
        let s = dx * self.right[0] + dy * self.right[1];
        let d = -dx * self.right[1] + dy * self.right[0];
        (s, d)
    }

    /// The plan point at offset `s`.
    pub fn at(&self, s: f64) -> [f64; 2] {
        [
            self.origin[0] + s * self.right[0],
            self.origin[1] + s * self.right[1],
        ]
    }
}

/// Per cut, its points: `[offset, elevation, class]`.
pub type CutPoints = Vec<Vec<[f64; 3]>>;

/// Per cut, the points of `classes` within `band` of its plane and within
/// its span (`[offset, elevation, class]`), from one pass over `source`;
/// at most [`MAX_CUT_POINTS`] a cut, thinned evenly (said in the notes).
pub fn cut_points(
    source: &dyn PointSource,
    planes: &[CutPlane],
    classes: &[u8],
    band: f64,
    budget: &Budget,
) -> Result<(CutPoints, Vec<String>), String> {
    let mut out: Vec<Vec<[f64; 3]>> = vec![Vec::new(); planes.len()];
    let mut notes = Vec::new();
    if planes.is_empty() || classes.is_empty() {
        return Ok((out, notes));
    }
    let lines: Vec<Vec<[f64; 2]>> = planes
        .iter()
        .map(|p| vec![p.at(p.span[0]), p.at(p.span[1])])
        .collect();
    let corridor = Corridor::new(&lines, band);
    source.visit(Some(&corridor), Some(classes), budget, &mut |x, y, z, c| {
        for (k, p) in planes.iter().enumerate() {
            let (s, d) = p.locate(x, y);
            if d.abs() <= band && s >= p.span[0] && s <= p.span[1] {
                out[k].push([s, z, f64::from(c)]);
            }
        }
    })?;
    let mut thinned = 0;
    for pts in &mut out {
        if pts.len() > MAX_CUT_POINTS {
            let every = pts.len().div_ceil(MAX_CUT_POINTS);
            let mut k = 0;
            pts.retain(|_| {
                k += 1;
                (k - 1) % every == 0
            });
            thinned += 1;
        }
    }
    let total: usize = out.iter().map(Vec::len).sum();
    notes.push(format!(
        "{total} points of the classes {} within {band} m of {} cut(s) from {}",
        classes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", "),
        planes.len(),
        source.describe()
    ));
    if thinned > 0 {
        notes.push(format!(
            "{thinned} cut(s) held more than {MAX_CUT_POINTS} points: thinned evenly to about that"
        ));
    }
    Ok((out, notes))
}

/// The surface `height` along `plane` every `step` m, broken where it has
/// none: lines of `[offset, elevation]`.
pub fn tin_section(
    height: &dyn Fn(f64, f64) -> Option<f64>,
    plane: &CutPlane,
    step: f64,
) -> Vec<Vec<[f64; 2]>> {
    let mut lines: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut cur: Vec<[f64; 2]> = Vec::new();
    let n = ((plane.span[1] - plane.span[0]) / step.max(1e-3)).ceil() as usize;
    for k in 0..=n {
        let s = (plane.span[0] + k as f64 * step).min(plane.span[1]);
        let [x, y] = plane.at(s);
        match height(x, y) {
            Some(z) => cur.push([s, z]),
            None => {
                if cur.len() > 1 {
                    lines.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() > 1 {
        lines.push(cur);
    }
    lines
}

/// A section of a surface drawn along a cut (`kind` `tin`).
pub fn tin_json(name: &str, lines: &[Vec<[f64; 2]>]) -> Value {
    json!({"kind": "tin", "name": name, "lines": lines})
}

/// The crossings of a drawing's lines with a cut (`kind` `dxf`): each
/// offset, elevation, layer and colour.
pub fn dxf_json(name: &str, marks: &[(f64, f64, String, [u8; 3])]) -> Value {
    json!({
        "kind": "dxf",
        "name": name,
        "marks": marks
            .iter()
            .map(|(s, z, layer, c)| json!([s, z, layer, format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])]))
            .collect::<Vec<_>>(),
    })
}

/// Adds a cut's `points` and `sections` to its document.
pub fn fill(doc: &mut Value, points: &[[f64; 3]], sections: Vec<Value>) {
    doc["points"] = json!(
        points
            .iter()
            .map(|p| json!([p[0], p[1], p[2] as u8]))
            .collect::<Vec<_>>()
    );
    doc["sections"] = Value::Array(sections);
}

#[cfg(test)]
mod tests {
    use ohl_pointcloud::Stats;

    use super::*;

    struct Cloud(Vec<([f64; 3], u8)>);

    impl PointSource for Cloud {
        fn describe(&self) -> String {
            "test cloud".into()
        }
        fn class_scheme(&self) -> Option<String> {
            None
        }
        fn visit(
            &self,
            area: Option<&Corridor>,
            classes: Option<&[u8]>,
            _budget: &Budget,
            visit: &mut dyn FnMut(f64, f64, f64, u8),
        ) -> Result<Stats, String> {
            for (p, c) in &self.0 {
                if area.is_none_or(|a| a.contains(p[0], p[1]))
                    && classes.is_none_or(|k| k.contains(c))
                {
                    visit(p[0], p[1], p[2], *c);
                }
            }
            Ok(Stats::default())
        }
    }

    /// A cut along x through (10, 0), offsets −5 … 5 to +y: the points of
    /// class 6 within 1 m of it and within its span, at their offset.
    #[test]
    fn points_near_a_cut() {
        let plane = CutPlane {
            origin: [10.0, 0.0],
            right: [0.0, 1.0],
            span: [-5.0, 5.0],
        };
        let cloud = Cloud(vec![
            ([10.5, 2.0, 100.0], 6), // in: offset 2
            ([10.5, 2.0, 100.0], 2), // another class
            ([12.0, 2.0, 100.0], 6), // 2 m off the plane
            ([10.0, 7.0, 100.0], 6), // beyond the span
            ([9.2, -4.0, 101.0], 6), // in: offset −4
        ]);
        let (pts, notes) = cut_points(&cloud, &[plane], &[6], 1.0, &Budget::new()).unwrap();
        assert_eq!(pts[0], vec![[2.0, 100.0, 6.0], [-4.0, 101.0, 6.0]]);
        assert!(
            notes[0].starts_with("2 points of the classes 6"),
            "{notes:?}"
        );
        let (s, d) = plane.locate(9.2, -4.0);
        assert!((s + 4.0).abs() < 1e-12 && (d - 0.8).abs() < 1e-12);
    }

    /// A surface sampled along a cut, broken where it has no height.
    #[test]
    fn a_tin_along_a_cut() {
        let plane = CutPlane {
            origin: [0.0, 0.0],
            right: [1.0, 0.0],
            span: [-2.0, 2.0],
        };
        let h = |x: f64, _y: f64| (x.abs() > 0.5).then_some(x);
        let lines = tin_section(&h, &plane, 1.0);
        assert_eq!(
            lines,
            vec![
                vec![[-2.0, -2.0], [-1.0, -1.0]],
                vec![[1.0, 1.0], [2.0, 2.0]]
            ]
        );
        let v = tin_json("dtm", &lines);
        assert_eq!(v["kind"], "tin");
        let d = dxf_json("plan", &[(1.0, 2.0, "WALLS".into(), [255, 0, 16])]);
        assert_eq!(d["marks"][0][3], "#ff0010");
    }
}
