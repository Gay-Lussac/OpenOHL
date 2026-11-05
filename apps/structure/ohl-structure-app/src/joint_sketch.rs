// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the values of the connections table mean, drawn (owner
//! 2026-11-04: "with gauge, pitch, End, EDGE, e, stops, it's hard to
//! understand exactly what does what on a joint can you make it clearer ?
//! like make an explanatory illustration"; built, in active development,
//! not approved yet).
//!
//! The bolted leg of an angle seen flat — a strip from its heel to its
//! edge (the toe), its end where the steel stops — with the member's axis
//! (its centroid line, dashed) running past that end to beside the work
//! point, the gauge line with three bolts, a second gauge line faint, and
//! a dimension per value of the table, named as its column is (`TERMS`:
//! the table's headers and hover texts come from the same list).
//!
//! One drawing, two outputs: the folding section "What the values mean"
//! above the table (egui's painter, the theme's colours, scaled to the
//! width, hover on a label: its column's hover text), and the user
//! guide's SVG (`docs/images/joint-details/joint-values.svg`, written by
//! `svg`; a test keeps it in step).
//!
//! A second drawing (2026-11-05, [`show_plies`]): how member ends meet,
//! each in section — A nested (an angle in an angle, one shear plane), B
//! a double cover splice (the leg between two covers, two shear planes),
//! C continuous (one part through the joint, no bolt); its SVG
//! `docs/images/joint-details/joint-plies.svg`.

use eframe::egui;

/// A value of the connections table: its column's header, its name in
/// the drawing and its hover text (the column's and the drawing's).
#[derive(Debug, Clone, Copy)]
pub struct Term {
    /// The column's header.
    pub header: &'static str,
    /// Its name in the drawing.
    pub label: &'static str,
    /// What it is.
    pub tip: &'static str,
}

/// Gauge lines.
pub const LINES: Term = Term {
    header: "Lines",
    label: "Lines",
    tip: "Gauge lines: rows of bolts along the leg bolted, each at its own gauge from the heel \
          (1 to 3; every second line staggered by half a pitch)",
};
/// The first gauge.
pub const GAUGE: Term = Term {
    header: "Gauge mm",
    label: "Gauge",
    tip: "Across the leg bolted: from its heel (the angle's corner) to the gauge line (the first \
          line)",
};
/// The pitch.
pub const PITCH: Term = Term {
    header: "Pitch mm",
    label: "Pitch",
    tip: "Along a gauge line: from one bolt to the next",
};
/// The end distance.
pub const END: Term = Term {
    header: "End dist. mm",
    label: "End dist.",
    tip: "Along the member: from the member's end (where its steel stops) to the first bolt",
};
/// The edge distance.
pub const EDGE: Term = Term {
    header: "Edge dist. mm",
    label: "Edge dist.",
    tip: "Across the leg: from the gauge line (the outer one) to the leg's edge (its toe)",
};
/// The eccentricity.
pub const ECC: Term = Term {
    header: "Ecc. e mm",
    label: "Ecc. e",
    tip: "The eccentricity: across the leg, from the member's axis (its centroid line) to the \
          gauge line",
};
/// Where the steel stops.
pub const STOPS: Term = Term {
    header: "Steel stops mm",
    label: "Steel stops",
    tip: "Along the axis: from the work point to the member's end, where its steel stops (worked \
          out so it fits); its axis still runs to the work point",
};
/// The sideways shift.
pub const SHIFT: Term = Term {
    header: "Shift mm",
    label: "Shift",
    tip: "The member's end moved sideways to fit (+: towards what it bolts onto): its axis, and \
          its force, pass beside the work point — an eccentricity the analysis takes",
};

/// Every value drawn, in the table's order.
#[cfg(test)]
pub const TERMS: [Term; 8] = [LINES, GAUGE, PITCH, END, EDGE, ECC, STOPS, SHIFT];

/// The values of a member end written beside their names (m; none: the
/// name only).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shown {
    /// The member end: "D1 at N2".
    pub end: Option<String>,
    /// Gauge lines.
    pub lines: Option<u32>,
    /// The first gauge.
    pub gauge: Option<f64>,
    /// Pitch.
    pub pitch: Option<f64>,
    /// End distance.
    pub end_distance: Option<f64>,
    /// Edge distance.
    pub edge: Option<f64>,
    /// Eccentricity.
    pub ecc: Option<f64>,
    /// Where the steel stops.
    pub stops: Option<f64>,
    /// Shift (none: 0).
    pub shift: Option<f64>,
}

impl Shown {
    /// The values of member end `a` at joint `joint`, as the table shows
    /// them.
    pub fn of(a: &ohl_structure_link::Arrival, joint: &str) -> Self {
        Self {
            end: Some(format!("{} at {joint}", a.member)),
            lines: a.gauge_lines.value,
            gauge: a.gauges.value.as_ref().and_then(|g| g.first().copied()),
            pitch: a.pitch.value,
            end_distance: a.end_distance.value,
            edge: a.edge_distance.value,
            ecc: a.eccentricity.value,
            stops: a.cut.value,
            shift: Some(a.shift.0),
        }
    }
}

/// How a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ink {
    /// The steel's edges.
    Steel,
    /// Extension lines, the gauge line.
    Thin,
    /// The member's axis.
    Dashed,
    /// The second gauge line.
    Faint,
    /// What it bolts onto.
    Onto,
}

/// Where a label lies from its point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    /// Left of it, ending there.
    Left,
    /// Right of it, starting there.
    Right,
    /// Above it, centred.
    Above,
    /// Below it, centred.
    Below,
    /// Right of it and below, its top left there.
    RightBelow,
}

/// A piece of the drawing, in mm (y up).
#[derive(Debug, Clone)]
enum Prim {
    /// The steel's face.
    Fill(Vec<[f32; 2]>),
    /// A line through the points.
    Line(Vec<[f32; 2]>, Ink),
    /// A bolt (faint: of the second gauge line).
    Bolt([f32; 2], bool),
    /// The work point.
    Point([f32; 2]),
    /// A dimension line, an arrow at each end.
    Dim([f32; 2], [f32; 2]),
    /// A label.
    Label {
        at: [f32; 2],
        text: String,
        anchor: Anchor,
        tip: Option<&'static str>,
        faint: bool,
    },
}

/// A bolt's radius in the drawing, mm.
const BOLT_R: f32 = 8.0;

/// The drawing (mm, y up): the strip from x = −250 to the member's end at
/// 0, the heel at y = 0, the leg's edge at 120; the axis at 33, the gauge
/// line at 62, a second at 95; the work point at (110, 12), the axis
/// 21 mm beside it.
fn prims(v: &Shown) -> Vec<Prim> {
    let (x0, b, yc, g, g2) = (-250.0, 120.0, 33.0, 62.0, 95.0);
    let (end, pitch) = (30.0, 55.0);
    let w = [110.0, 12.0];
    let bolts: Vec<f32> = (0..3).map(|k| -end - pitch * k as f32).collect();
    let mm = |x: Option<f64>| x.map(|m| format!(" {:.0}", m * 1000.0)).unwrap_or_default();
    let name = |t: Term, x: Option<f64>| match x {
        Some(_) => format!("{}{} mm", t.label, mm(x)),
        None => t.label.to_owned(),
    };
    let label =
        |at: [f32; 2], text: String, anchor: Anchor, tip: Option<&'static str>| Prim::Label {
            at,
            text,
            anchor,
            tip,
            faint: false,
        };
    let faint = |at: [f32; 2], text: &str, anchor: Anchor| Prim::Label {
        at,
        text: text.to_owned(),
        anchor,
        tip: None,
        faint: true,
    };
    let line = |a: [f32; 2], b: [f32; 2], ink: Ink| Prim::Line(vec![a, b], ink);
    let mut out = vec![
        Prim::Fill(vec![[x0, 0.0], [0.0, 0.0], [0.0, b], [x0, b]]),
        // The heel, the leg's edge, the member's end; broken off at the
        // left.
        line([x0, 0.0], [0.0, 0.0], Ink::Steel),
        line([x0, b], [0.0, b], Ink::Steel),
        line([0.0, 0.0], [0.0, b], Ink::Steel),
        Prim::Line(
            vec![
                [x0, 0.0],
                [x0 + 5.0, 30.0],
                [x0 - 5.0, 60.0],
                [x0 + 5.0, 90.0],
                [x0, b],
            ],
            Ink::Thin,
        ),
        // The axis, past the member's end to beside the work point.
        line([x0, yc], [w[0], yc], Ink::Dashed),
        // The gauge lines.
        line([x0, g], [0.0, g], Ink::Thin),
        line([x0, g2], [0.0, g2], Ink::Faint),
        // What it bolts onto, through the work point.
        line(
            [w[0] - 17.0, w[1] - 36.0],
            [w[0] + 30.0, w[1] + 64.0],
            Ink::Onto,
        ),
        Prim::Point(w),
    ];
    for x in &bolts {
        out.push(Prim::Bolt([*x, g], false));
        out.push(Prim::Bolt([*x - pitch / 2.0, g2], true));
    }
    // Across the leg, at the left: the gauge and the edge distance.
    let xd = x0 - 22.0;
    for y in [0.0, g, b] {
        out.push(line([x0 - 4.0, y], [xd - 4.0, y], Ink::Thin));
    }
    out.push(Prim::Dim([xd, 0.0], [xd, g]));
    out.push(Prim::Dim([xd, g], [xd, b]));
    out.push(label(
        [xd - 4.0, g / 2.0],
        name(GAUGE, v.gauge),
        Anchor::Left,
        Some(GAUGE.tip),
    ));
    out.push(label(
        [xd - 4.0, (g + b) / 2.0],
        name(EDGE, v.edge),
        Anchor::Left,
        Some(EDGE.tip),
    ));
    // Along the leg, above it: the pitch and the end distance.
    let yd = b + 30.0;
    for x in [bolts[2], bolts[1], bolts[0]] {
        out.push(line([x, g + BOLT_R + 2.0], [x, yd + 4.0], Ink::Thin));
    }
    out.push(line([0.0, b + 2.0], [0.0, yd + 4.0], Ink::Thin));
    out.push(Prim::Dim([bolts[2], yd], [bolts[1], yd]));
    out.push(Prim::Dim([bolts[0], yd], [0.0, yd]));
    out.push(label(
        [(bolts[2] + bolts[1]) / 2.0, yd + 2.0],
        name(PITCH, v.pitch),
        Anchor::Above,
        Some(PITCH.tip),
    ));
    out.push(label(
        [2.0, yd],
        name(END, v.end_distance),
        Anchor::Right,
        Some(END.tip),
    ));
    // The eccentricity, right of the member's end: the axis to the gauge
    // line.
    let xe = 16.0;
    out.push(line([2.0, g], [xe + 4.0, g], Ink::Thin));
    out.push(Prim::Dim([xe, yc], [xe, g]));
    out.push(label(
        [xe + 3.0, (yc + g) / 2.0],
        name(ECC, v.ecc),
        Anchor::Right,
        Some(ECC.tip),
    ));
    // Where the steel stops: the work point to the member's end.
    let ys = -24.0;
    out.push(line([0.0, -2.0], [0.0, ys - 4.0], Ink::Thin));
    out.push(line([w[0], w[1] - 5.0], [w[0], ys - 4.0], Ink::Thin));
    out.push(Prim::Dim([0.0, ys], [w[0], ys]));
    out.push(label(
        [w[0] / 2.0, ys - 2.0],
        name(STOPS, v.stops),
        Anchor::Below,
        Some(STOPS.tip),
    ));
    // The shift: the axis beside the work point, towards what it bolts
    // onto.
    let xs = w[0] + 26.0;
    out.push(line([w[0] + 5.0, w[1]], [xs + 4.0, w[1]], Ink::Thin));
    out.push(line([w[0] + 2.0, yc], [xs + 4.0, yc], Ink::Thin));
    out.push(Prim::Dim([xs, w[1]], [xs, yc]));
    let shift = v.shift.filter(|s| *s != 0.0);
    out.push(label(
        [xs + 3.0, (w[1] + yc) / 2.0],
        match shift {
            Some(s) => format!("Shift {:+.0} mm", s * 1000.0),
            None if v.end.is_some() => "Shift 0 mm".into(),
            None => "Shift +".into(),
        },
        Anchor::Right,
        Some(SHIFT.tip),
    ));
    // The names of the parts.
    out.push(faint([x0 + 30.0, -2.0], "heel", Anchor::Below));
    out.push(faint(
        [x0 + 40.0, b + 2.0],
        "leg's edge (toe)",
        Anchor::Above,
    ));
    out.push(faint(
        [x0 + 4.0, yc - 2.0],
        "axis (centroid line)",
        Anchor::RightBelow,
    ));
    out.push(faint([x0 + 4.0, g - 2.0], "gauge line", Anchor::RightBelow));
    out.push(faint([w[0] - 7.0, w[1]], "work point", Anchor::Left));
    out.push(faint(
        [w[0] + 30.0, w[1] + 66.0],
        "what it bolts onto",
        Anchor::Above,
    ));
    out.push(faint([-3.0, b + 12.0], "member's end", Anchor::Left));
    // The second gauge line, named with its column.
    let xl = bolts[0] - pitch / 2.0;
    out.push(line([xl, g2 + BOLT_R + 2.0], [xl, yd + 32.0], Ink::Thin));
    out.push(label(
        [xl, yd + 34.0],
        match v.lines {
            Some(n) => format!("a 2nd gauge line (Lines = 2; here {n})"),
            None => "a 2nd gauge line (Lines = 2)".to_owned(),
        },
        Anchor::Above,
        Some(LINES.tip),
    ));
    out
}

/// A label laid out: its rectangle on the screen, its text and tip.
#[derive(Debug, Clone)]
#[cfg(test)]
pub struct Placed {
    /// Where.
    pub rect: egui::Rect,
    /// Its text.
    pub text: String,
    /// Its hover text.
    pub tip: Option<&'static str>,
}

/// The gap between a label and its point, points.
const GAP: f32 = 3.0;

/// The label's rectangle of size `size` at screen point `p`.
fn label_rect(p: egui::Pos2, size: egui::Vec2, anchor: Anchor) -> egui::Rect {
    let min = match anchor {
        Anchor::Left => egui::pos2(p.x - GAP - size.x, p.y - size.y / 2.0),
        Anchor::Right => egui::pos2(p.x + GAP, p.y - size.y / 2.0),
        Anchor::Above => egui::pos2(p.x - size.x / 2.0, p.y - GAP - size.y),
        Anchor::Below => egui::pos2(p.x - size.x / 2.0, p.y + GAP),
        Anchor::RightBelow => egui::pos2(p.x + GAP, p.y + GAP),
    };
    egui::Rect::from_min_size(min, size)
}

/// The drawing laid out at `scale` points per mm: the screen point of a
/// drawing point (from the drawing's top left, labels included), the
/// labels' rectangles and its size.
struct Laid {
    scale: f32,
    origin: egui::Vec2,
    labels: Vec<(egui::Rect, Anchor, bool)>,
    size: egui::Vec2,
}

impl Laid {
    fn at(&self, p: [f32; 2]) -> egui::Pos2 {
        egui::pos2(p[0] * self.scale, -p[1] * self.scale) + self.origin
    }
}

/// `prims` laid out at `scale`, the labels' sizes by `measure`.
fn lay(prims: &[Prim], scale: f32, measure: &dyn Fn(&str) -> egui::Vec2) -> Laid {
    let raw = |p: [f32; 2]| egui::pos2(p[0] * scale, -p[1] * scale);
    let mut all = egui::Rect::NOTHING;
    let mut labels = Vec::new();
    for p in prims {
        match p {
            Prim::Fill(pts) | Prim::Line(pts, _) => {
                for q in pts {
                    all.extend_with(raw(*q));
                }
            }
            Prim::Bolt(c, _) | Prim::Point(c) => {
                all = all.union(egui::Rect::from_center_size(
                    raw(*c),
                    egui::Vec2::splat(2.0 * BOLT_R * scale),
                ));
            }
            Prim::Dim(a, b) => {
                all.extend_with(raw(*a));
                all.extend_with(raw(*b));
            }
            Prim::Label {
                at,
                text,
                anchor,
                faint,
                ..
            } => {
                let r = label_rect(raw(*at), measure(text), *anchor);
                all = all.union(r);
                labels.push((r, *anchor, *faint));
            }
        }
    }
    let margin = egui::vec2(4.0, 4.0);
    let origin = margin - all.min.to_vec2();
    for l in &mut labels {
        l.0 = l.0.translate(origin);
    }
    Laid {
        scale,
        origin,
        labels,
        size: all.size() + 2.0 * margin,
    }
}

/// The smallest width the drawing is laid out at, points (a narrower
/// panel scrolls it).
pub const MIN_WIDTH: f32 = 480.0;

/// The most points per mm (a wide panel: no larger).
const MAX_SCALE: f32 = 1.6;

/// The drawing laid out for `width` points: the scale that fills it
/// (labels keep their size), at most [`MAX_SCALE`].
fn fit(prims: &[Prim], width: f32, measure: &dyn Fn(&str) -> egui::Vec2) -> Laid {
    let width = width.max(MIN_WIDTH);
    let mut scale = 1.0;
    for _ in 0..4 {
        let at1 = lay(prims, scale, measure).size.x;
        let at2 = lay(prims, scale * 1.1, measure).size.x;
        let per = (at2 - at1) / (0.1 * scale);
        let fixed = at1 - per * scale;
        scale = ((width - fixed) / per).clamp(0.2, MAX_SCALE);
    }
    lay(prims, scale, measure)
}

/// The labels of the drawing laid out for `width` (the tests: none
/// overlaps another or a bolt).
#[cfg(test)]
pub fn placed(
    v: &Shown,
    width: f32,
    measure: &dyn Fn(&str) -> egui::Vec2,
) -> (Vec<Placed>, Vec<egui::Rect>, egui::Vec2) {
    let ps = prims(v);
    let laid = fit(&ps, width, measure);
    let mut labels = Vec::new();
    let mut bolts = Vec::new();
    let mut k = 0;
    for p in &ps {
        match p {
            Prim::Label { text, tip, .. } => {
                labels.push(Placed {
                    rect: laid.labels[k].0,
                    text: text.clone(),
                    tip: *tip,
                });
                k += 1;
            }
            Prim::Bolt(c, _) => bolts.push(egui::Rect::from_center_size(
                laid.at(*c),
                egui::Vec2::splat(2.0 * BOLT_R * laid.scale),
            )),
            _ => {}
        }
    }
    (labels, bolts, laid.size)
}

/// The colours of the drawing from the theme.
struct Colours {
    ink: egui::Color32,
    weak: egui::Color32,
    steel: egui::Color32,
    dim: egui::Color32,
    bg: egui::Color32,
}

impl Colours {
    fn of(v: &egui::Visuals) -> Self {
        Self {
            ink: v.text_color(),
            weak: v.weak_text_color(),
            steel: v.widgets.inactive.bg_fill,
            dim: v.hyperlink_color,
            bg: v.extreme_bg_color,
        }
    }
}

/// The drawing in `ui`, as wide as the panel (at least [`MIN_WIDTH`]),
/// with `v`'s values beside the names; a label's hover: its column's.
pub fn show(ui: &mut egui::Ui, v: &Shown, width: f32) {
    show_prims(ui, &prims(v), width);
}

/// How member ends meet, in section (A nested, B double cover, C
/// continuous), as wide as the panel (at least [`MIN_WIDTH`]).
pub fn show_plies(ui: &mut egui::Ui, width: f32) {
    show_prims(ui, &plies(), width);
}

/// What the plies drawing's names mean.
const NESTED_TIP: &str = "Face = nested: an angle heel into heel inside the other (its heel \
     ground to clear the other's root fillet), bolted through both legs: one shear plane";
/// What a double cover means.
const COVERS_TIP: &str = "A splice: a cover on the outer and the inner face of the leg (the \
     inner clear of the root fillet), the bolts through the cover, the leg and the cover: two \
     shear planes";
/// What continuous means.
const CONTINUOUS_TIP: &str = "Held = continuous: the member runs on through the joint into the \
     other as one part (in line, the same section): no cut, no bolts, one piece";

/// The plies drawing (mm, y up): A an angle nested in another, B a leg
/// between two covers, C one bar through a joint.
fn plies() -> Vec<Prim> {
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let outline = |x0: f32, y0: f32, x1: f32, y1: f32| {
        Prim::Line(
            vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]],
            Ink::Steel,
        )
    };
    let label = |at: [f32; 2], text: &str, anchor: Anchor, tip: Option<&'static str>| Prim::Label {
        at,
        text: text.to_owned(),
        anchor,
        tip,
        faint: false,
    };
    let faint = |at: [f32; 2], text: &str, anchor: Anchor| Prim::Label {
        at,
        text: text.to_owned(),
        anchor,
        tip: None,
        faint: true,
    };
    // A bolt in section at `x`: its shank from `lo` to `hi`, its head above
    // `hi`, its nut below `lo`.
    let bolt = |x: f32, lo: f32, hi: f32| {
        vec![
            outline(x - 4.0, lo - 6.0, x + 4.0, hi + 6.0),
            outline(x - 10.0, hi, x + 10.0, hi + 6.0),
            outline(x - 10.0, lo - 6.0, x + 10.0, lo),
        ]
    };
    let mut out = Vec::new();
    // A: the larger angle (legs 140, 10 thick), the smaller inside it (its
    // heel on the larger's inner faces, legs 110, 8 thick).
    for r in [
        rect(0.0, 0.0, 140.0, 10.0),
        rect(0.0, 0.0, 10.0, 140.0),
        rect(10.0, 10.0, 120.0, 18.0),
        rect(10.0, 10.0, 18.0, 120.0),
    ] {
        out.push(Prim::Fill(r));
    }
    out.push(outline(0.0, 0.0, 140.0, 10.0));
    out.push(outline(0.0, 0.0, 10.0, 140.0));
    out.push(outline(10.0, 10.0, 120.0, 18.0));
    out.push(outline(10.0, 10.0, 18.0, 120.0));
    out.extend(bolt(75.0, 0.0, 18.0));
    out.push(Prim::Line(vec![[55.0, 10.0], [150.0, 10.0]], Ink::Dashed));
    out.push(faint([152.0, 10.0], "shear plane", Anchor::Right));
    out.push(label(
        [70.0, -16.0],
        "A — nested",
        Anchor::Below,
        Some(NESTED_TIP),
    ));
    // B: an angle's leg (140, 10 thick) between an outer cover (below) and
    // an inner one (above, clear of the root).
    let bx = 250.0;
    for r in [
        rect(bx, 0.0, bx + 140.0, 10.0),
        rect(bx, 0.0, bx + 10.0, 140.0),
        rect(bx, -10.0, bx + 140.0, 0.0),
        rect(bx + 25.0, 10.0, bx + 140.0, 20.0),
    ] {
        out.push(Prim::Fill(r));
    }
    out.push(outline(bx, 0.0, bx + 140.0, 10.0));
    out.push(outline(bx, 0.0, bx + 10.0, 140.0));
    out.push(outline(bx, -10.0, bx + 140.0, 0.0));
    out.push(outline(bx + 25.0, 10.0, bx + 140.0, 20.0));
    out.extend(bolt(bx + 85.0, -10.0, 20.0));
    for y in [0.0, 10.0] {
        out.push(Prim::Line(
            vec![[bx + 65.0, y], [bx + 105.0, y]],
            Ink::Dashed,
        ));
    }
    out.push(faint([bx + 142.0, 30.0], "inner cover", Anchor::Right));
    out.push(faint([bx + 142.0, -8.0], "outer cover", Anchor::Right));
    out.push(label(
        [bx + 70.0, -24.0],
        "B — double cover",
        Anchor::Below,
        Some(COVERS_TIP),
    ));
    // C: one bar through the joint (seen from the side), the work point on
    // its axis, no bolt.
    let cx = 540.0;
    out.push(Prim::Fill(rect(cx, 50.0, cx + 200.0, 74.0)));
    out.push(outline(cx, 50.0, cx + 200.0, 74.0));
    out.push(Prim::Line(
        vec![[cx - 10.0, 62.0], [cx + 210.0, 62.0]],
        Ink::Dashed,
    ));
    out.push(Prim::Point([cx + 100.0, 62.0]));
    out.push(faint(
        [cx + 100.0, 78.0],
        "the joint: no cut, no bolt",
        Anchor::Above,
    ));
    out.push(faint([cx + 30.0, 46.0], "L1", Anchor::Below));
    out.push(faint([cx + 170.0, 46.0], "L2", Anchor::Below));
    out.push(label(
        [cx + 100.0, -16.0],
        "C — continuous",
        Anchor::Below,
        Some(CONTINUOUS_TIP),
    ));
    out
}

/// `ps` drawn in `ui`, as wide as the panel (at least [`MIN_WIDTH`]); a
/// label's hover: its tip.
fn show_prims(ui: &mut egui::Ui, ps: &[Prim], width: f32) {
    let ps = ps.to_vec();
    let font = egui::FontId::proportional(12.0);

    let painter = ui.painter().clone();
    let measure = |t: &str| {
        painter
            .layout_no_wrap(t.to_owned(), font.clone(), egui::Color32::WHITE)
            .size()
    };
    let laid = fit(&ps, width, &measure);
    let (rect, _) = ui.allocate_exact_size(laid.size, egui::Sense::hover());
    let c = Colours::of(ui.visuals());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, c.bg);
    let shift = rect.min.to_vec2();
    let at = |p: [f32; 2]| laid.at(p) + shift;
    let mut k = 0;
    for p in &ps {
        match p {
            Prim::Fill(pts) => {
                painter.add(egui::Shape::convex_polygon(
                    pts.iter().map(|q| at(*q)).collect(),
                    c.steel,
                    egui::Stroke::NONE,
                ));
            }
            Prim::Line(pts, ink) => {
                let pts: Vec<egui::Pos2> = pts.iter().map(|q| at(*q)).collect();
                match ink {
                    Ink::Steel => {
                        painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0, c.ink)));
                    }
                    Ink::Thin => {
                        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0, c.weak)));
                    }
                    Ink::Dashed => {
                        painter.extend(egui::Shape::dashed_line(
                            &pts,
                            egui::Stroke::new(1.2, c.ink),
                            8.0,
                            4.0,
                        ));
                    }
                    Ink::Faint => {
                        painter.extend(egui::Shape::dashed_line(
                            &pts,
                            egui::Stroke::new(1.0, c.weak),
                            3.0,
                            3.0,
                        ));
                    }
                    Ink::Onto => {
                        painter.add(egui::Shape::line(
                            pts,
                            egui::Stroke::new(6.0, c.weak.gamma_multiply(0.35)),
                        ));
                    }
                }
            }
            Prim::Bolt(q, faint) => {
                let r = BOLT_R * laid.scale;
                let colour = if *faint { c.weak } else { c.ink };
                painter.circle(at(*q), r, c.bg, egui::Stroke::new(1.5, colour));
                let o = at(*q);
                let d = r * 0.6;
                for (a, b) in [
                    (o + egui::vec2(-d, 0.0), o + egui::vec2(d, 0.0)),
                    (o + egui::vec2(0.0, -d), o + egui::vec2(0.0, d)),
                ] {
                    painter.line_segment([a, b], egui::Stroke::new(1.0, colour));
                }
            }
            Prim::Point(q) => {
                painter.circle_filled(at(*q), 4.0, c.ink);
            }
            Prim::Dim(a, b) => arrow2(&painter, at(*a), at(*b), c.dim),
            Prim::Label {
                text, tip, faint, ..
            } => {
                let r = laid.labels[k].0.translate(shift);
                k += 1;
                painter.rect_filled(r.expand(1.0), 2.0, c.bg.gamma_multiply(0.85));
                painter.text(
                    r.left_top(),
                    egui::Align2::LEFT_TOP,
                    text,
                    font.clone(),
                    if *faint {
                        c.weak
                    } else if tip.is_some() {
                        c.dim
                    } else {
                        c.ink
                    },
                );
                if let Some(tip) = tip {
                    ui.interact(r, ui.id().with(("joint-sketch", k)), egui::Sense::hover())
                        .on_hover_text(*tip);
                }
            }
        }
    }
}

/// A dimension line from `a` to `b` with an arrow at each end.
fn arrow2(painter: &egui::Painter, a: egui::Pos2, b: egui::Pos2, colour: egui::Color32) {
    let stroke = egui::Stroke::new(1.2, colour);
    painter.line_segment([a, b], stroke);
    let d = (b - a).normalized();
    let n = egui::vec2(-d.y, d.x);
    for (tip, dir) in [(a, d), (b, -d)] {
        painter.add(egui::Shape::convex_polygon(
            vec![tip, tip + dir * 7.0 + n * 3.0, tip + dir * 7.0 - n * 3.0],
            colour,
            egui::Stroke::NONE,
        ));
    }
}

/// The drawing as an SVG for the user guide: theme-neutral (its colours
/// follow the reader's light or dark scheme, as the other images of the
/// guide), the names only (no values).
#[cfg(test)]
pub fn svg() -> String {
    svg_of(
        &prims(&Shown::default()),
        "What the values of a member end mean",
        "The bolted leg of an angle seen flat, from its \
         heel to its edge, its end where the steel stops; the member's axis runs past that end \
         to beside the work point. Dimensioned: the gauge from the heel to the gauge line, the \
         edge distance from it to the leg's edge, the pitch between bolts, the end distance \
         from the last bolt to the member's end, the eccentricity e from the axis to the gauge \
         line, where the steel stops from the work point, and the shift of the axis beside the \
         work point towards what the member bolts onto; a second gauge line faint.",
        "What the values of a member end mean (the leg bolted, seen flat)",
    )
}

/// The plies drawing as an SVG for the user guide (as [`svg`]).
#[cfg(test)]
pub fn plies_svg() -> String {
    svg_of(
        &plies(),
        "How member ends meet",
        "Three cases in section. A: an angle nested heel into heel inside a larger one, a bolt \
         through both legs, one shear plane between them. B: an angle's leg between an outer \
         cover and an inner cover clear of the root fillet, a bolt through the three, two shear \
         planes. C: one bar running on through the joint, the work point on its axis, no cut \
         and no bolt.",
        "How member ends meet (in section)",
    )
}

/// `ps` as an SVG: its title, description and heading.
#[cfg(test)]
fn svg_of(ps: &[Prim], title: &str, desc: &str, heading: &str) -> String {
    let ps = ps.to_vec();
    // 13 px text, about 7 px a character.
    let measure = |t: &str| egui::vec2(t.chars().count() as f32 * 7.0 + 2.0, 16.0);
    let laid = lay(&ps, 1.6, &measure);
    let title_h = 30.0;
    let (w, h) = (laid.size.x.ceil(), (laid.size.y + title_h).ceil());
    let at = |p: [f32; 2]| {
        let q = laid.at(p);
        (q.x, q.y + title_h)
    };
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" \
         height=\"{h}\" role=\"img\" aria-labelledby=\"t d\"><title id=\"t\">{title}</title>\
         <desc id=\"d\">{desc}</desc>\
         <style>svg{{--ink:#1f2328;--muted:#59636e;--acc:#0969da;--bg:#ffffff;--fill:#eef1f4;}}\
         @media (prefers-color-scheme: dark){{svg{{--ink:#e6edf3;--muted:#9198a1;--acc:#58a6ff;\
         --bg:#0d1117;--fill:#1c2128;}}}}.bg{{fill:var(--bg)}}.steel{{fill:var(--fill)}}\
         .st{{stroke:var(--ink);stroke-width:2.5;fill:none;stroke-linecap:round;\
         stroke-linejoin:round}}.thin{{stroke:var(--muted);stroke-width:1.2;fill:none}}\
         .dash{{stroke:var(--ink);stroke-width:1.4;fill:none;stroke-dasharray:9 5}}\
         .faint{{stroke:var(--muted);stroke-width:1.2;fill:none;stroke-dasharray:3 3}}\
         .onto{{stroke:var(--muted);stroke-width:7;fill:none;opacity:0.35}}\
         .dim{{stroke:var(--acc);stroke-width:1.3;fill:none}}.arr{{fill:var(--acc)}}\
         .bolt{{fill:var(--bg);stroke:var(--ink);stroke-width:1.8}}\
         .bolt2{{fill:var(--bg);stroke:var(--muted);stroke-width:1.4}}\
         .wp{{fill:var(--ink)}}text{{font:13px system-ui,-apple-system,'Segoe UI',Roboto,\
         sans-serif;fill:var(--ink);paint-order:stroke;stroke:var(--bg);stroke-width:4px;\
         stroke-linejoin:round}}.lab{{fill:var(--muted);font-size:12px}}\
         .val{{fill:var(--acc)}}.t{{font-weight:600;font-size:14px}}</style>\
         <rect class=\"bg\" width=\"{w}\" height=\"{h}\"/><text x=\"8\" y=\"20\" class=\"t\">\
         {heading}</text>"
    );
    let pts = |v: &[[f32; 2]]| {
        v.iter()
            .map(|q| {
                let (x, y) = at(*q);
                format!("{x:.1},{y:.1}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut k = 0;
    for p in &ps {
        match p {
            Prim::Fill(v) => {
                s += &format!("<polygon class=\"steel\" points=\"{}\"/>", pts(v));
            }
            Prim::Line(v, ink) => {
                let class = match ink {
                    Ink::Steel => "st",
                    Ink::Thin => "thin",
                    Ink::Dashed => "dash",
                    Ink::Faint => "faint",
                    Ink::Onto => "onto",
                };
                s += &format!("<polyline class=\"{class}\" points=\"{}\"/>", pts(v));
            }
            Prim::Bolt(q, faint) => {
                let (x, y) = at(*q);
                let r = BOLT_R * laid.scale;
                let d = r * 0.6;
                let class = if *faint { "bolt2" } else { "bolt" };
                s += &format!(
                    "<circle class=\"{class}\" cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{r:.1}\"/>\
                     <path class=\"thin\" d=\"M{:.1},{y:.1} L{:.1},{y:.1} M{x:.1},{:.1} \
                     L{x:.1},{:.1}\"/>",
                    x - d,
                    x + d,
                    y - d,
                    y + d
                );
            }
            Prim::Point(q) => {
                let (x, y) = at(*q);
                s += &format!("<circle class=\"wp\" cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"4.5\"/>");
            }
            Prim::Dim(a, b) => {
                let (a, b) = (at(*a), at(*b));
                s += &format!(
                    "<line class=\"dim\" x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/>",
                    a.0, a.1, b.0, b.1
                );
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let l = (dx * dx + dy * dy).sqrt().max(1e-3);
                let (ux, uy) = (dx / l, dy / l);
                for (t, sx, sy) in [(a, ux, uy), (b, -ux, -uy)] {
                    let (nx, ny) = (-sy, sx);
                    s += &format!(
                        "<polygon class=\"arr\" points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\"/>",
                        t.0,
                        t.1,
                        t.0 + sx * 8.0 + nx * 3.5,
                        t.1 + sy * 8.0 + ny * 3.5,
                        t.0 + sx * 8.0 - nx * 3.5,
                        t.1 + sy * 8.0 - ny * 3.5
                    );
                }
            }
            Prim::Label {
                text, tip, faint, ..
            } => {
                let r = laid.labels[k].0;
                k += 1;
                let class = if *faint {
                    " class=\"lab\""
                } else if tip.is_some() {
                    " class=\"val\""
                } else {
                    ""
                };
                s += &format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\"{class}>{}</text>",
                    r.min.x + 1.0,
                    r.max.y - 4.0 + title_h,
                    text.replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;")
                );
            }
        }
    }
    s += "</svg>\n";
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The labels laid out at `width` with egui's own fonts.
    fn laid_at(v: &Shown, width: f32) -> (Vec<Placed>, Vec<egui::Rect>, egui::Vec2) {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        let font = egui::FontId::proportional(12.0);
        let measure = |t: &str| {
            ctx.fonts_mut(|f| {
                f.layout_no_wrap(t.to_owned(), font.clone(), egui::Color32::WHITE)
                    .size()
            })
        };
        placed(v, width, &measure)
    }

    /// No label overlaps another or a bolt, narrow or wide, with the
    /// values written or not; the drawing fits the width asked (at least
    /// [`MIN_WIDTH`]).
    #[test]
    fn the_labels_do_not_overlap() {
        let values = Shown {
            end: Some("D1 at N2".into()),
            lines: Some(1),
            gauge: Some(0.030),
            pitch: Some(0.055),
            end_distance: Some(0.030),
            edge: Some(0.0235),
            ecc: Some(0.0118),
            stops: Some(0.120),
            shift: Some(-0.015),
        };
        let mut bad = Vec::new();
        for v in [Shown::default(), values] {
            for width in [300.0, MIN_WIDTH, 600.0, 800.0, 1400.0] {
                let (labels, bolts, size) = laid_at(&v, width);
                if size.x > width.max(MIN_WIDTH) + 1.0 {
                    bad.push(format!("{width}: {size:?} wider"));
                }
                for (i, a) in labels.iter().enumerate() {
                    for b in &labels[i + 1..] {
                        if a.rect.intersects(b.rect) {
                            bad.push(format!("{width}: \"{}\" over \"{}\"", a.text, b.text));
                        }
                    }
                    if bolts.iter().any(|r| a.rect.intersects(*r)) {
                        bad.push(format!("{width}: \"{}\" over a bolt", a.text));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    /// Every value of the table is named in the drawing, with its
    /// column's hover text.
    #[test]
    fn every_column_is_drawn() {
        let (labels, _, _) = laid_at(&Shown::default(), 600.0);
        for t in TERMS {
            assert!(
                labels
                    .iter()
                    .any(|l| l.text.contains(t.label) && l.tip == Some(t.tip)),
                "{}",
                t.label
            );
        }
    }

    /// The user guide's SVG is this drawing (`OHL_WRITE_DOCS_SVG=1`
    /// writes it again).
    #[test]
    fn the_guides_svg_is_the_drawing() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/images/joint-details/joint-values.svg");
        let now = svg();
        if ohl_core::env::var_os("OHL_WRITE_DOCS_SVG").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &now).unwrap();
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(have, now, "OHL_WRITE_DOCS_SVG=1 writes it again");
    }

    /// The plies drawing (2026-11-05): its labels apart, at every width;
    /// the guide's SVG is it (`OHL_WRITE_DOCS_SVG=1` writes it again).
    #[test]
    fn the_plies_drawing_is_laid_out_and_in_the_guide() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        let font = egui::FontId::proportional(12.0);
        let measure = |t: &str| {
            ctx.fonts_mut(|f| {
                f.layout_no_wrap(t.to_owned(), font.clone(), egui::Color32::WHITE)
                    .size()
            })
        };
        let ps = plies();
        for width in [300.0, MIN_WIDTH, 800.0, 1400.0] {
            let laid = fit(&ps, width, &measure);
            assert!(laid.size.x <= width.max(MIN_WIDTH) + 1.0, "{width}");
            for (i, a) in laid.labels.iter().enumerate() {
                for b in &laid.labels[i + 1..] {
                    assert!(!a.0.intersects(b.0), "{width}: labels {i} overlap");
                }
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/images/joint-details/joint-plies.svg");
        let now = plies_svg();
        if ohl_core::env::var_os("OHL_WRITE_DOCS_SVG").is_some() {
            std::fs::write(&path, &now).unwrap();
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(have, now, "OHL_WRITE_DOCS_SVG=1 writes it again");
    }
}
