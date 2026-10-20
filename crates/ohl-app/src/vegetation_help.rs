// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The explanations of the vegetation panel's settings: a line or two
//! beside each value (a longer text on hover), and small line drawings
//! (about 120 × 80 px, drawn with the painter in the theme's colours)
//! that follow the values typed: the falling tree with its root-ball
//! hinge and allowance, grow-in around a wire, the wind positions (swing
//! arc, steps), a tree base and its nearest ground point, unbalanced ice
//! on one span.

use eframe::egui;

/// Size of a drawing, px.
pub const SIZE: egui::Vec2 = egui::vec2(120.0, 80.0);

/// The explanation of a setting: a short weak line, the longer text on
/// hover.
pub fn explain(ui: &mut egui::Ui, short: &str, long: &str) {
    ui.label(egui::RichText::new(short).small().weak())
        .on_hover_text(long);
}

/// The colours of a drawing in the theme: lines, faint lines, the thing
/// explained, the wire.
struct Ink {
    line: egui::Color32,
    faint: egui::Color32,
    key: egui::Color32,
    wire: egui::Color32,
}

/// A drawing's canvas, named `name` for screen readers (and tests).
fn canvas(ui: &mut egui::Ui, name: &str, tip: &str) -> (egui::Painter, egui::Rect, Ink) {
    let (response, painter) = ui.allocate_painter(SIZE, egui::Sense::hover());
    let name = name.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &name));
    response.on_hover_text(tip);
    let v = ui.visuals();
    let rect = painter.clip_rect();
    painter.rect_filled(rect, 3.0, v.extreme_bg_color);
    let ink = Ink {
        line: v.text_color(),
        faint: v.weak_text_color(),
        key: v.warn_fg_color,
        wire: v.hyperlink_color,
    };
    (painter, rect, ink)
}

fn dashed(painter: &egui::Painter, a: egui::Pos2, b: egui::Pos2, color: egui::Color32) {
    painter.extend(egui::Shape::dashed_line(
        &[a, b],
        egui::Stroke::new(1.0, color),
        3.0,
        2.0,
    ));
}

fn text(painter: &egui::Painter, at: egui::Pos2, s: &str, color: egui::Color32) {
    painter.text(
        at,
        egui::Align2::LEFT_CENTER,
        s,
        egui::FontId::proportional(9.0),
        color,
    );
}

/// The falling tree: its trunk of height h, the root-ball hinge k·h from
/// it, the sector it sweeps (radius L = √(h² + (k·h)²)) down to the
/// ground, and the allowance A it must keep from the wire.
pub fn falling_tree(ui: &mut egui::Ui, k: f64, allowance: f64) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: falling tree",
        "A tree of height h turns about the edge of its root ball, k·h from its trunk; it sweeps \
         the sector of radius L = √(h² + (k·h)²) down to the ground. Where the sector comes \
         within A of a wire, the tree is a danger tree.",
    );
    let ground = r.bottom() - 10.0;
    let base = egui::pos2(r.left() + 22.0, ground);
    let h = 52.0_f32;
    let kh = (k.clamp(0.0, 0.5) as f32) * h;
    let hinge = egui::pos2(base.x + kh, ground);
    let len = h.hypot(kh);
    p.line_segment(
        [
            egui::pos2(r.left() + 4.0, ground),
            egui::pos2(r.right() - 4.0, ground),
        ],
        egui::Stroke::new(1.0, ink.faint),
    );
    // The trunk, the hinge.
    p.line_segment(
        [base, egui::pos2(base.x, ground - h)],
        egui::Stroke::new(2.0, ink.line),
    );
    p.circle_filled(hinge, 2.5, ink.key);
    if kh > 0.5 {
        p.line_segment([base, hinge], egui::Stroke::new(1.5, ink.key));
    }
    text(&p, egui::pos2(base.x + 2.0, ground + 5.0), "k·h", ink.key);
    // The sector swept from upright to the ground.
    let top = egui::pos2(base.x, ground - h);
    let a0 = (top.y - hinge.y).atan2(top.x - hinge.x);
    let pts: Vec<egui::Pos2> = (0..=24)
        .map(|i| {
            let a = a0 + (0.0 - a0) * i as f32 / 24.0;
            hinge + egui::vec2(a.cos(), a.sin()) * len
        })
        .collect();
    p.add(egui::Shape::line(pts, egui::Stroke::new(1.0, ink.line)));
    // The wire, A beyond the sector.
    let a = (allowance.clamp(0.0, 20.0) as f32) * 2.5;
    let dir = egui::vec2(0.7, -0.7);
    let edge = hinge + dir * len;
    let wire = edge + dir * a.max(3.0);
    p.circle_filled(wire, 3.0, ink.wire);
    dashed(&p, edge, wire, ink.key);
    text(&p, wire + egui::vec2(5.0, 0.0), "A", ink.key);
    text(
        &p,
        egui::pos2(base.x - 12.0, ground - h * 0.5),
        "h",
        ink.line,
    );
}

/// Grow-in: the wire, the required clearance c around it, then the
/// growth g until the next cut and the buffer b; a vegetation point
/// inside the outer ring grows in.
pub fn grow_in(ui: &mut egui::Ui, clearance: f64, growth: f64, buffer: f64) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: grow-in",
        "Around each wire position: the required clearance c, the growth g until the next cut, \
         the buffer b. A vegetation point closer than c + g + b grows in.",
    );
    let c = r.center() + egui::vec2(-18.0, -4.0);
    let total = (clearance + growth + buffer).max(1e-6);
    let scale = 34.0 / total as f32;
    let r1 = clearance.max(0.0) as f32 * scale;
    let r2 = r1 + growth.max(0.0) as f32 * scale;
    let r3 = r2 + buffer.max(0.0) as f32 * scale;
    p.circle_stroke(c, r1, egui::Stroke::new(1.5, ink.key));
    if r2 > r1 + 0.5 {
        p.circle_stroke(c, r2, egui::Stroke::new(1.0, ink.line));
    }
    if r3 > r2 + 0.5 {
        p.circle_stroke(c, r3, egui::Stroke::new(1.0, ink.faint));
    }
    p.circle_filled(c, 3.0, ink.wire);
    // A tree point inside the outer ring.
    let q = c + egui::vec2(r3 * 0.55, r3 * 0.6);
    p.circle_filled(q, 2.5, ink.line);
    let x = r.right() - 34.0;
    text(&p, egui::pos2(x, r.top() + 12.0), "c", ink.key);
    text(&p, egui::pos2(x, r.top() + 26.0), "+ g", ink.line);
    text(&p, egui::pos2(x, r.top() + 40.0), "+ b", ink.faint);
}

/// The wind positions of a span seen along it: still, blown left and
/// right; the swing arc between them; the steps of a stepped wind.
pub fn wind_positions(ui: &mut egui::Ui, arc: bool, steps: usize) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: wind positions",
        "Seen along the span: the wire still, and blown to the left and to the right by the \
         case's wind; the swing arc is every position between; wind steps add the positions at \
         every pressure from 0 to the case's, from each side.",
    );
    let top = egui::pos2(r.center().x, r.top() + 10.0);
    let len = 54.0;
    let phi = 0.75_f32;
    let at = |a: f32| top + egui::vec2(a.sin(), a.cos()) * len;
    if arc {
        let pts: Vec<egui::Pos2> = (0..=24)
            .map(|i| at(-phi + 2.0 * phi * i as f32 / 24.0))
            .collect();
        p.add(egui::Shape::line(pts, egui::Stroke::new(2.0, ink.key)));
    }
    for a in [-phi, 0.0, phi] {
        p.line_segment([top, at(a)], egui::Stroke::new(1.0, ink.faint));
        p.circle_filled(at(a), 3.0, ink.wire);
    }
    let n = steps.min(12);
    for i in 1..n {
        let t = i as f32 / n as f32;
        for s in [-1.0, 1.0] {
            p.circle_stroke(at(s * phi * t), 2.5, egui::Stroke::new(1.0, ink.key));
        }
    }
    text(
        &p,
        egui::pos2(r.left() + 4.0, r.top() + 8.0),
        "left",
        ink.line,
    );
    text(
        &p,
        egui::pos2(r.right() - 26.0, r.top() + 8.0),
        "right",
        ink.line,
    );
}

/// A tree base: the ground under a vegetation point is taken from the
/// ground surface only with a ground point within the distance in plan.
pub fn tree_base(ui: &mut egui::Ui, limit: Option<f64>) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: tree base",
        "The tree's base is the ground below its top, taken only when a ground point (a TIN \
         vertex: LiDAR ground or survey) lies within this distance in plan; else the point gets \
         no fall-in check.",
    );
    let ground = r.bottom() - 16.0;
    let tree = egui::pos2(r.center().x, r.top() + 12.0);
    p.line_segment(
        [
            egui::pos2(r.left() + 4.0, ground),
            egui::pos2(r.right() - 4.0, ground),
        ],
        egui::Stroke::new(1.0, ink.faint),
    );
    let base = egui::pos2(tree.x, ground);
    dashed(&p, tree, base, ink.line);
    p.circle_filled(tree, 3.0, ink.line);
    let rad = limit.map_or(48.0, |d| (d.clamp(0.0, 20.0) as f32) * 5.0 + 4.0);
    p.line_segment(
        [base - egui::vec2(rad, 0.0), base + egui::vec2(rad, 0.0)],
        egui::Stroke::new(2.0, ink.key),
    );
    for x in [-50.0_f32, -14.0, 30.0] {
        let g = egui::pos2(base.x + x, ground);
        let near = x.abs() <= rad;
        p.circle_filled(g, 2.5, if near { ink.key } else { ink.faint });
    }
    text(
        &p,
        egui::pos2(r.left() + 4.0, r.bottom() - 6.0),
        if limit.is_some() {
            "nearest ground point"
        } else {
            "no limit"
        },
        ink.key,
    );
}

/// Unbalanced ice: one span iced more than its neighbours sags further.
pub fn unbalanced_ice(ui: &mut egui::Ui, on: bool) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: unbalanced ice",
        "Unbalanced ice: the span checked carries more ice than its neighbours (the case's \
         ice on it alone, more radial ice, more ice load per wire, or a load at mid-span); it \
         sags further, its strings swing towards it.",
    );
    let y = r.top() + 16.0;
    let xs = [
        r.left() + 8.0,
        r.left() + 42.0,
        r.right() - 42.0,
        r.right() - 8.0,
    ];
    for x in xs {
        p.line_segment(
            [egui::pos2(x, y - 6.0), egui::pos2(x, r.bottom() - 6.0)],
            egui::Stroke::new(1.5, ink.faint),
        );
    }
    for (k, w) in xs.windows(2).enumerate() {
        let iced = on && k == 1;
        let sag = if iced { 34.0 } else { 16.0 };
        let pts: Vec<egui::Pos2> = (0..=16)
            .map(|i| {
                let t = i as f32 / 16.0;
                egui::pos2(w[0] + (w[1] - w[0]) * t, y + 4.0 * sag * t * (1.0 - t))
            })
            .collect();
        p.add(egui::Shape::line(
            pts,
            egui::Stroke::new(
                if iced { 3.0 } else { 1.5 },
                if iced { ink.key } else { ink.wire },
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// Every drawing renders, in the dark and the light theme, at the
    /// values' ends (k = 0 and 0.5, no allowance, no limit, steps off).
    #[test]
    fn the_drawings_render_in_both_themes() {
        for dark in [true, false] {
            for (k, a, steps, limit) in [(0.0, 0.0, 0, None), (0.5, 20.0, 40, Some(5.0))] {
                let mut h = Harness::new_ui(move |ui| {
                    ui.ctx().set_visuals(if dark {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    });
                    falling_tree(ui, k, a);
                    grow_in(ui, 5.0, 1.0, 0.5);
                    wind_positions(ui, steps > 0, steps);
                    tree_base(ui, limit);
                    unbalanced_ice(ui, steps > 0);
                });
                h.run();
                for name in [
                    "drawing: falling tree",
                    "drawing: grow-in",
                    "drawing: wind positions",
                    "drawing: tree base",
                    "drawing: unbalanced ice",
                ] {
                    h.get_by_label(name);
                }
            }
        }
    }
}
