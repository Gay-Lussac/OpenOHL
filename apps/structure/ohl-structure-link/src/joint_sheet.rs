// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A joint seen square to a face, as a shop drawing (stage 2 of the
//! joint details): the members' outlines cut a little beyond the joint,
//! the gauge lines, the bolts, the plates, the end distance and pitch
//! dimensioned along each gauge line, a label per member. Plain marks in
//! metres in the view's plane for the application to paint, and the same
//! as an SVG or PDF sheet (`ohl-drawing`).

use ohl_drawing::model::{Anchor, Color, Drawing, Item, Layer, Pen, Text};

use crate::joints::{Arrival, JointDetail, Source};

/// What a line of the face view is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A member's outline.
    Member,
    /// An edge along a member (an angle's heel).
    Edge,
    /// A gauge line.
    Gauge,
    /// A plate's outline.
    Plate,
    /// A dimension line and its ticks.
    Dimension,
    /// A member's axis, where its force runs (to the work point, or beside
    /// it by its end's shift).
    Axis,
    /// A weld seam where two plates of a welded plate meet (2026-11-04).
    Weld,
}

/// One mark of the face view, metres in the view's plane (x right, y up).
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// A polyline.
    Line {
        /// Points.
        points: Vec<[f64; 2]>,
        /// Closed.
        closed: bool,
        /// What it is.
        kind: LineKind,
    },
    /// A bolt as seen: the outlines of its nut, shank and head (seen along
    /// its axis: the head's hexagon over the shank's circle; from its
    /// side: three rectangles), its hole's centre.
    Bolt {
        /// Its centre (where the axis meets the face bolted).
        at: [f64; 2],
        /// The outlines, the far part first (nut, shank, head, or the
        /// reverse when the nut is nearer).
        parts: Vec<Vec<[f64; 2]>>,
        /// Seen along its axis.
        end_on: bool,
        /// Its values assumed (drawn paler).
        assumed: bool,
    },
    /// A text.
    Text {
        /// Where (its middle, on the baseline).
        at: [f64; 2],
        /// The text.
        text: String,
        /// A label (larger) or a dimension's value.
        label: bool,
    },
}

/// A joint seen square to one of its faces.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceView {
    /// The joint.
    pub joint: String,
    /// The face (`x+`, `x-`, `y+`, `y-`, `plan`).
    pub face: String,
    /// The marks.
    pub marks: Vec<Mark>,
    /// Left, bottom, right, top of the marks, m.
    pub bounds: [f64; 4],
    /// What the view says of itself (the title).
    pub title: String,
}

/// The faces a joint can be seen from: those its members lie in, the
/// others after (`x+`, `x-`, `y+`, `y-`, `plan`).
pub fn faces_of(d: &JointDetail) -> Vec<String> {
    let all = ["x+", "x-", "y+", "y-", "plan"];
    let mut count: Vec<(usize, &str)> = all
        .iter()
        .map(|f| (d.arrivals.iter().filter(|a| a.face == *f).count(), *f))
        .collect();
    count.sort_by_key(|a| std::cmp::Reverse(a.0));
    count.into_iter().map(|(_, f)| f.to_owned()).collect()
}

/// The view's axes for `face`: right, up, towards the viewer.
pub fn face_axes(face: &str) -> [[f64; 3]; 3] {
    match face {
        // Seen from outside the face, z up.
        "x+" => [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
        "x-" => [[0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [-1.0, 0.0, 0.0]],
        "y+" => [[-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
        "y-" => [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]],
        // From above, ahead (y) up.
        _ => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    }
}

/// How far from the joint the members are drawn, m: past the farthest
/// bolt, at least 0.3 m.
pub fn reach(d: &JointDetail) -> f64 {
    let far = d
        .arrivals
        .iter()
        .flat_map(|a| a.bolt_points.iter())
        .map(|p| dist(*p, d.position))
        .fold(0.0, f64::max);
    (far + 0.12).max(0.3)
}

/// The joint `d` seen square to `face`.
pub fn face_view(d: &JointDetail, face: &str) -> FaceView {
    let [r, u, _] = face_axes(face);
    let o = d.position;
    let to = |p: [f64; 3]| {
        let q = sub(p, o);
        [dot(q, r), dot(q, u)]
    };
    let cut = reach(d);
    let mut marks = Vec::new();
    // The plates first (under the members), each with its id and
    // thickness at its middle.
    for p in &d.plates {
        let pts: Vec<[f64; 2]> = p.outline.iter().map(|q| to(*q)).collect();
        if pts.is_empty() {
            continue;
        }
        let n = pts.len() as f64;
        let mid = [
            pts.iter().map(|q| q[0]).sum::<f64>() / n,
            pts.iter().map(|q| q[1]).sum::<f64>() / n,
        ];
        marks.push(Mark::Line {
            points: pts,
            closed: true,
            kind: LineKind::Plate,
        });
        marks.push(Mark::Text {
            at: mid,
            text: if p.assembly.len() > 1 {
                // A plate of a welded plate said so on the drawing.
                format!(
                    "{} · {:.0} mm · welded to {}",
                    p.id,
                    p.thickness * 1000.0,
                    p.assembly
                        .iter()
                        .filter(|x| **x != p.id)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                format!("{} · {:.0} mm", p.id, p.thickness * 1000.0)
            },
            label: false,
        });
    }
    // The welds where the plates of a welded plate meet.
    for p in &d.plates {
        for w in &p.welds {
            marks.push(Mark::Line {
                points: w.ends.iter().map(|q| to(*q)).collect(),
                closed: false,
                kind: LineKind::Weld,
            });
        }
    }
    for a in &d.arrivals {
        let length = a.length.min(cut);
        let along = |s: f64| add(o, scale(a.direction, s));
        match &a.profile {
            Some(p) => {
                // The prism's corners seen from the face; its outline the
                // hull of them.
                let corner = |s: f64, q: [f64; 2]| {
                    add(
                        add(along(s), a.offset),
                        add(scale(p.axes[1], q[0]), scale(p.axes[2], q[1])),
                    )
                };
                // The steel from where it stops (its cut) to the drawing's
                // reach.
                let start = a.cut.value.unwrap_or(0.0).min(length);
                let pts: Vec<[f64; 2]> = [start, length]
                    .iter()
                    .flat_map(|s| p.outline.iter().map(move |q| to(corner(*s, *q))))
                    .collect();
                marks.push(Mark::Line {
                    points: hull(pts),
                    closed: true,
                    kind: LineKind::Member,
                });
                if let Some(g) = p.angle {
                    marks.push(Mark::Line {
                        points: vec![to(corner(start, g.heel)), to(corner(length, g.heel))],
                        closed: false,
                        kind: LineKind::Edge,
                    });
                }
            }
            None => marks.push(Mark::Line {
                points: vec![
                    to(add(along(a.cut.value.unwrap_or(0.0).min(length)), a.offset)),
                    to(add(along(length), a.offset)),
                ],
                closed: false,
                kind: LineKind::Member,
            }),
        }
        // Its axis (where its force runs): to the work point, beside it by
        // its end's shift (an eccentricity).
        let shift = scale(a.shift.1.unwrap_or([0.0; 3]), a.shift.0);
        marks.push(Mark::Line {
            points: vec![to(add(o, shift)), to(add(along(length), shift))],
            closed: false,
            kind: LineKind::Axis,
        });
        bolts_and_dimensions(a, &to, face_axes(face)[2], o, length, &mut marks);
        // The label beyond the member's cut end.
        let at = to(along(length + 0.04));
        marks.push(Mark::Text {
            at,
            text: label(a),
            label: true,
        });
    }
    let title = format!(
        "Joint {} — face {} ({}) — {}",
        d.id,
        face,
        if face == "plan" {
            "seen from above"
        } else {
            "seen from outside"
        },
        d.completeness.words()
    );
    let bounds = bounds(&marks);
    FaceView {
        joint: d.id.clone(),
        face: face.to_owned(),
        marks,
        bounds,
        title,
    }
}

/// A member's label: its id and section, its bolts.
fn label(a: &Arrival) -> String {
    let mut s = a.member.clone();
    if let Some(p) = &a.profile {
        s += &format!(" {}", p.section_id);
    }
    if let Some(b) = &a.bolts.value {
        s += &format!(" · {} × M{:.0}", b.count, b.diameter * 1000.0);
        if let Some(g) = &b.grade {
            s += &format!(" {g}");
        }
        if a.bolts.source == Source::Member {
            s += " (the member's)";
        }
    }
    s
}

/// The bolts of `a` and its gauge line, the end distance and the pitch
/// dimensioned beside it.
fn bolts_and_dimensions(
    a: &Arrival,
    to: &dyn Fn([f64; 3]) -> [f64; 2],
    towards: [f64; 3],
    o: [f64; 3],
    length: f64,
    marks: &mut Vec<Mark>,
) {
    let Some(b) = &a.bolts.value else {
        return;
    };
    if a.bolt_points.is_empty() {
        return;
    }
    let assumed = a.gauges.source == Source::Assumed
        || a.pitch.source == Source::Assumed
        || a.end_distance.source == Source::Assumed;
    let end_on = dot(a.bolt_axis, towards).abs() > 0.5;
    // The head towards the viewer is drawn last (over the shank).
    let head_near = dot(a.bolt_axis, towards) >= 0.0;
    for p in &a.bolt_points {
        let faces = crate::joints::bolt_faces(*p, a.bolt_axis, b.diameter, a.grip, b.drawn);
        let outline = |part: crate::joints::BoltPart| {
            hull(
                faces
                    .iter()
                    .filter(|f| f.1 == part)
                    .flat_map(|f| f.0.iter().map(|q| to(*q)))
                    .collect(),
            )
        };
        use crate::joints::BoltPart::{Head, Nut, Shank};
        let order = if head_near {
            [Nut, Shank, Head]
        } else {
            [Head, Shank, Nut]
        };
        marks.push(Mark::Bolt {
            at: to(*p),
            parts: order.iter().map(|x| outline(*x)).collect(),
            end_on,
            assumed,
        });
    }
    // The gauge line through the first bolt, from the joint's end to the cut.
    let first = a.bolt_points[0];
    let s0 = dot(sub(first, o), a.direction);
    let off = sub(first, add(o, scale(a.direction, s0)));
    let on = |s: f64| add(add(o, off), scale(a.direction, s));
    marks.push(Mark::Line {
        points: vec![to(on(0.0)), to(on(length))],
        closed: false,
        kind: LineKind::Gauge,
    });
    // The chain: end distance, then the pitches, 25 mm beside the line.
    let mut stations: Vec<f64> = a
        .bolt_points
        .iter()
        .map(|p| dot(sub(*p, o), a.direction))
        .collect();
    stations.sort_by(f64::total_cmp);
    stations.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
    stations.insert(0, 0.0);
    let p0 = to(on(0.0));
    let p1 = to(on(1.0));
    let dir = [p1[0] - p0[0], p1[1] - p0[1]];
    let n = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    if n < 1e-9 {
        return;
    }
    let side = [-dir[1] / n * 0.025, dir[0] / n * 0.025];
    let at = |s: f64| {
        let q = to(on(s));
        [q[0] + side[0], q[1] + side[1]]
    };
    marks.push(Mark::Line {
        points: stations.iter().map(|s| at(*s)).collect(),
        closed: false,
        kind: LineKind::Dimension,
    });
    for s in &stations {
        let q = at(*s);
        let t = [side[0] * 0.25, side[1] * 0.25];
        marks.push(Mark::Line {
            points: vec![[q[0] - t[0], q[1] - t[1]], [q[0] + t[0], q[1] + t[1]]],
            closed: false,
            kind: LineKind::Dimension,
        });
    }
    for w in stations.windows(2) {
        let q = at((w[0] + w[1]) / 2.0);
        marks.push(Mark::Text {
            at: [q[0] + side[0] * 0.6, q[1] + side[1] * 0.6],
            text: format!("{:.0}", (w[1] - w[0]) * 1000.0),
            label: false,
        });
    }
    if let Some(g) = a.gauges.value.as_ref().and_then(|g| g.first()) {
        let q = at(stations[stations.len() - 1] + 0.03);
        marks.push(Mark::Text {
            at: q,
            text: format!(
                "g {:.0}{}",
                g * 1000.0,
                if a.gauges.source == Source::Assumed {
                    " (assumed)"
                } else {
                    ""
                }
            ),
            label: false,
        });
    }
}

fn bounds(marks: &[Mark]) -> [f64; 4] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut take = |p: [f64; 2], r: f64| {
        b[0] = b[0].min(p[0] - r);
        b[1] = b[1].min(p[1] - r);
        b[2] = b[2].max(p[0] + r);
        b[3] = b[3].max(p[1] + r);
    };
    for m in marks {
        match m {
            Mark::Line { points, .. } => points.iter().for_each(|p| take(*p, 0.0)),
            Mark::Bolt { parts, .. } => parts.iter().flatten().for_each(|p| take(*p, 0.0)),
            // Texts sit beside what they name: not counted.
            Mark::Text { .. } => {}
        }
    }
    if !b[0].is_finite() {
        return [-0.1, -0.1, 0.1, 0.1];
    }
    // Room for the labels beyond the members' ends.
    [b[0] - 0.06, b[1] - 0.06, b[2] + 0.06, b[3] + 0.06]
}

/// The face view on an A4 sheet (landscape), at the largest of 1:2, 1:5,
/// 1:10, 1:20 that fits; the title and scale below.
pub fn face_sheet(v: &FaceView) -> Drawing {
    let (w, h, margin, band) = (297.0, 210.0, 12.0, 14.0);
    let (vw, vh) = (v.bounds[2] - v.bounds[0], v.bounds[3] - v.bounds[1]);
    let denominator = [2.0, 5.0, 10.0, 20.0, 50.0]
        .into_iter()
        .find(|k| vw * 1000.0 / k <= w - 2.0 * margin && vh * 1000.0 / k <= h - 2.0 * margin - band)
        .unwrap_or(50.0);
    let k = 1000.0 / denominator;
    let cx = margin + (w - 2.0 * margin - vw * k) / 2.0;
    let cy = margin + (h - 2.0 * margin - band - vh * k) / 2.0;
    let at = |p: [f64; 2]| [cx + (p[0] - v.bounds[0]) * k, cy + (v.bounds[3] - p[1]) * k];
    let mut d = Drawing::sheet(w, h);
    let black = Color(0, 0, 0);
    let grey = Color(110, 110, 110);
    for m in &v.marks {
        match m {
            Mark::Line {
                points,
                closed,
                kind,
            } => {
                let pen = match kind {
                    LineKind::Member => Pen::new(black, 0.35),
                    LineKind::Edge => Pen::new(black, 0.18),
                    LineKind::Gauge => Pen::new(grey, 0.18).dashed("4 1 1 1"),
                    LineKind::Plate => Pen::new(Color(0, 90, 160), 0.3),
                    LineKind::Weld => Pen::new(Color(200, 50, 30), 0.6),
                    LineKind::Dimension => Pen::new(grey, 0.13),
                    LineKind::Axis => Pen::new(Color(60, 120, 200), 0.18).dashed("6 1.5 1 1.5"),
                };
                let layer = match kind {
                    LineKind::Dimension => Layer::Labels,
                    _ => Layer::Members,
                };
                let path = d.path(layer, points.iter().map(|p| at(*p)).collect(), pen);
                path.closed = *closed;
            }
            Mark::Bolt {
                at: c,
                parts,
                end_on,
                assumed,
            } => {
                let color = if *assumed { grey } else { Color(180, 30, 30) };
                for part in parts {
                    let path = d.path(
                        Layer::Markers,
                        part.iter().map(|p| at(*p)).collect(),
                        Pen::new(color, 0.2),
                    );
                    path.closed = true;
                    path.title = Some("bolt".into());
                }
                if *end_on {
                    // The axis' cross.
                    let q = at(*c);
                    for (dx, dy) in [(1.2, 0.0), (0.0, 1.2)] {
                        d.path(
                            Layer::Markers,
                            vec![[q[0] - dx, q[1] - dy], [q[0] + dx, q[1] + dy]],
                            Pen::new(color, 0.1),
                        );
                    }
                }
            }
            Mark::Text { at: p, text, label } => d.items.push(Item::Text(Text {
                layer: Layer::Labels,
                at: at(*p),
                text: text.clone(),
                size: if *label { 2.5 } else { 1.8 },
                anchor: Anchor::Middle,
                bold: false,
                color: black,
            })),
        }
    }
    d.items.push(Item::Text(Text {
        layer: Layer::Title,
        at: [margin, h - margin],
        text: format!("{} — scale 1:{denominator:.0} on A4", v.title),
        size: 3.0,
        anchor: Anchor::Start,
        bold: true,
        color: black,
    }));
    d.items.push(Item::Text(Text {
        layer: Layer::Title,
        at: [margin, h - margin + 5.0],
        text: "Dimensions in mm. Grey bolts: their layout partly assumed (for the drawing). \
               Not checked by any calculation yet."
            .into(),
        size: 2.0,
        anchor: Anchor::Start,
        bold: false,
        color: grey,
    }));
    d
}

/// The face view as an SVG sheet.
pub fn face_svg(v: &FaceView) -> String {
    face_sheet(v).svg()
}

/// The face view as a one-page PDF.
pub fn face_pdf(v: &FaceView) -> Vec<u8> {
    ohl_drawing::pdf::write(&[&face_sheet(v)], &v.title)
}

pub(crate) fn hull(mut p: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let turn = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower: Vec<[f64; 2]> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && turn(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<[f64; 2]> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && turn(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}
