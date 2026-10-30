// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! How the electrical checks work, drawn (owner, 2026-10-30: "some
//! illustration on how it works"): small line drawings in the theme's
//! colours, as in the other check windows ([`crate::check_help`]), that
//! follow the values typed. The profile across the line and the field
//! over it; the sources (a straight piece's current and the point,
//! Biot–Savart; a charge and its image in a plane or the tiled floor);
//! the wire's straight pieces and their sagitta; the wires modelled
//! within a distance of a cut; a vertical cut beside a building with its
//! isolines; the earth return at the complex depth (line parameters).

use eframe::egui::{self, Pos2, Stroke, pos2, vec2};

use crate::check_help::{canvas_of, dashed, text};

/// Size of these drawings, px.
const SIZE: egui::Vec2 = vec2(170.0, 100.0);

/// Which field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Field {
    Magnetic,
    Electric,
}

/// A tower seen along the line, `ground` its foot: its top, the three
/// conductors (left, middle, right) and the earth wire.
fn tower(
    p: &egui::Painter,
    x: f32,
    ground: f32,
    top: f32,
    ink: &crate::check_help::Ink,
) -> [Pos2; 4] {
    let s = Stroke::new(1.0, ink.faint);
    p.line_segment([pos2(x - 6.0, ground), pos2(x - 2.0, top)], s);
    p.line_segment([pos2(x + 6.0, ground), pos2(x + 2.0, top)], s);
    let arm = top + 14.0;
    p.line_segment([pos2(x - 26.0, arm), pos2(x + 26.0, arm)], s);
    let at = [
        pos2(x - 22.0, arm + 8.0),
        pos2(x, arm + 8.0),
        pos2(x + 22.0, arm + 8.0),
        pos2(x, top),
    ];
    for c in &at[..3] {
        p.line_segment([pos2(c.x, arm), *c], s);
        p.circle_filled(*c, 2.5, ink.wire);
    }
    p.circle_filled(at[3], 2.0, ink.line);
    at
}

/// The profile across the line: the points ± `width` every `step` at
/// `height` above the ground, and the field over them (largest near the
/// axis, falling away).
pub fn profile(ui: &mut egui::Ui, field: Field, width: f64, step: f64, height: f64) {
    let (p, r, ink) = canvas_of(
        ui,
        SIZE,
        "drawing: profile across the line",
        "Seen along the line: the points of a profile, from −w to +w across the axis every step, \
         at h above the ground under each; the field computed at each (the curve). A profile is \
         taken at the lowest point of each span ticked, or where you give it in one span.",
    );
    let ground = r.bottom() - 14.0;
    let cx = r.center().x;
    p.line_segment(
        [pos2(r.left() + 4.0, ground), pos2(r.right() - 4.0, ground)],
        Stroke::new(1.0, ink.faint),
    );
    tower(&p, cx, ground, r.top() + 8.0, &ink);
    let y = ground - (height.clamp(0.0, 10.0) as f32 * 3.0).max(3.0);
    let (left, right) = (r.left() + 12.0, r.right() - 12.0);
    let n = ((2.0 * width / step.max(0.01)).round() as usize).clamp(2, 20);
    for k in 0..=n {
        let x = left + (right - left) * k as f32 / n as f32;
        p.circle_filled(pos2(x, y), 1.5, ink.key);
    }
    dashed(&p, pos2(right + 4.0, ground), pos2(right + 4.0, y), ink.key);
    text(
        &p,
        pos2(right - 4.0, (ground + y) / 2.0 + 6.0),
        "h",
        ink.key,
    );
    text(&p, pos2(left - 8.0, ground + 7.0), "−w", ink.key);
    text(&p, pos2(right - 14.0, ground + 7.0), "+w", ink.key);
    // The field over the points.
    let base = y - 3.0;
    let curve: Vec<Pos2> = (0..=40)
        .map(|i| {
            let x = left + (right - left) * i as f32 / 40.0;
            let u = (x - cx) / ((right - left) * 0.22);
            pos2(x, base - 26.0 / (1.0 + u * u))
        })
        .collect();
    p.add(egui::Shape::line(curve, Stroke::new(1.5, ink.key)));
    text(
        &p,
        pos2(r.left() + 4.0, r.top() + 8.0),
        match field {
            Field::Magnetic => "B µT",
            Field::Electric => "E kV/m",
        },
        ink.key,
    );
}

/// Where the field comes from. Magnetic: a straight piece of wire with
/// its current and a point (Biot–Savart, exact for the piece). Electric:
/// a charge on a wire and its image under the ground: in a plane under
/// the point, or (`tiled`) the charges on the floor's tiles following
/// the terrain.
pub fn sources(ui: &mut egui::Ui, field: Field, tiled: bool) {
    match field {
        Field::Magnetic => {
            let (p, r, ink) = canvas_of(
                ui,
                SIZE,
                "drawing: Biot–Savart for a straight piece",
                "Each wire is cut into straight pieces; the field of a piece carrying the \
                 current I at a point P is exact in closed form (Biot–Savart, from the angles \
                 under which P sees its two ends). The field is the sum over every piece of \
                 every phase, earth wire (with its induced current) and jumper.",
            );
            let a = pos2(r.left() + 14.0, r.top() + 22.0);
            let b = pos2(r.right() - 20.0, r.top() + 40.0);
            p.line_segment([a, b], Stroke::new(2.5, ink.wire));
            let m = a + (b - a) * 0.55;
            let d = (b - a).normalized();
            p.arrow(m - d * 14.0, d * 18.0, Stroke::new(1.5, ink.key));
            text(&p, m + vec2(-6.0, -10.0), "I", ink.key);
            let q = pos2(r.center().x - 4.0, r.bottom() - 16.0);
            p.circle_filled(q, 2.5, ink.line);
            text(&p, q + vec2(6.0, 2.0), "P", ink.line);
            dashed(&p, a, q, ink.faint);
            dashed(&p, b, q, ink.faint);
            // B around the wire.
            for rad in [8.0, 13.0] {
                p.circle_stroke(m, rad, Stroke::new(1.0, ink.faint));
            }
            text(
                &p,
                pos2(r.right() - 34.0, r.bottom() - 10.0),
                "B at P",
                ink.key,
            );
        }
        Field::Electric => {
            let (p, r, ink) = canvas_of(
                ui,
                SIZE,
                if tiled {
                    "drawing: charges and the tiled floor"
                } else {
                    "drawing: a charge and its image"
                },
                if tiled {
                    "The charges on the wires come from their voltages (potential coefficients). \
                     The ground is a floor of tiles following the terrain, each with a charge \
                     solved so that the floor stays at zero potential: it follows hollows and \
                     ridges."
                } else {
                    "The charges on the wires come from their voltages (potential coefficients). \
                     The ground under each point is a plane fitted to the terrain; each charge has \
                     its image, −q, mirrored under it, so the plane stays at zero potential: \
                     exact for level or evenly sloping ground."
                },
            );
            let q = pos2(r.center().x, r.top() + 16.0);
            p.circle_filled(q, 3.5, ink.wire);
            text(&p, q + vec2(6.0, 0.0), "+q", ink.wire);
            if tiled {
                // Terrain with a hollow, cut into tiles carrying charges.
                let ground: Vec<Pos2> = (0..=12)
                    .map(|i| {
                        let x = r.left() + 6.0 + (r.width() - 12.0) * i as f32 / 12.0;
                        let u = (i as f32 - 6.0) / 3.0;
                        pos2(x, r.bottom() - 34.0 + 16.0 / (1.0 + u * u))
                    })
                    .collect();
                p.add(egui::Shape::line(
                    ground.clone(),
                    Stroke::new(1.5, ink.line),
                ));
                for w in ground.windows(2) {
                    p.circle_filled(w[0], 1.5, ink.line);
                    let m = w[0] + (w[1] - w[0]) * 0.5;
                    text(&p, m + vec2(-2.0, 6.0), "−", ink.key);
                }
                text(
                    &p,
                    pos2(r.left() + 4.0, r.bottom() - 8.0),
                    "tiles: σ each",
                    ink.key,
                );
            } else {
                let g = r.center().y - 2.0;
                p.line_segment(
                    [pos2(r.left() + 6.0, g), pos2(r.right() - 6.0, g)],
                    Stroke::new(1.5, ink.line),
                );
                let image = pos2(q.x, 2.0 * g - q.y);
                p.circle_stroke(image, 3.5, Stroke::new(1.0, ink.key));
                text(&p, image + vec2(6.0, 0.0), "−q", ink.key);
                dashed(&p, q, image, ink.faint);
                text(&p, pos2(r.left() + 6.0, g - 6.0), "plane", ink.line);
            }
        }
    }
}

/// The wire's straight pieces: the wire's curve, its chords (`per_span`
/// in a span, a few drawn) and the sagitta (how far the wire lies off a piece, falling with the
/// square of its length).
pub fn pieces(ui: &mut egui::Ui, per_span: usize) {
    let (p, r, ink) = canvas_of(
        ui,
        SIZE,
        "drawing: straight pieces",
        "The wire (faint) and the straight pieces the field is computed for. The only error of \
         the pieces is the sagitta s, how far the wire curves away from a piece; it falls with \
         the square of the piece's length (half the length: a quarter).",
    );
    let (x0, x1) = (r.left() + 10.0, r.right() - 10.0);
    let top = r.top() + 14.0;
    let sag = 48.0;
    let at = |t: f32| pos2(x0 + (x1 - x0) * t, top + sag * 4.0 * t * (1.0 - t));
    let curve: Vec<Pos2> = (0..=48).map(|i| at(i as f32 / 48.0)).collect();
    p.add(egui::Shape::line(curve, Stroke::new(1.0, ink.faint)));
    // Drawn fewer (the sagitta seen), the count said.
    let n = per_span.clamp(2, 4);
    for k in 0..n {
        let (a, b) = (at(k as f32 / n as f32), at((k + 1) as f32 / n as f32));
        p.line_segment([a, b], Stroke::new(1.5, ink.wire));
        p.circle_filled(a, 1.5, ink.wire);
    }
    p.circle_filled(at(1.0), 1.5, ink.wire);
    // The sagitta of the first piece.
    let (a, b) = (at(0.0), at(1.0 / n as f32));
    let mid = at(0.5 / n as f32);
    let chord = a + (b - a) * 0.5;
    p.line_segment([chord, mid], Stroke::new(1.5, ink.key));
    text(&p, mid + vec2(4.0, 4.0), "s", ink.key);
    text(
        &p,
        pos2(r.left() + 4.0, r.bottom() - 8.0),
        &format!("{} pieces a span (fewer drawn)", per_span.max(1)),
        ink.line,
    );
}

/// The wires a cut or volume takes: the whole line, or those within `d`
/// of it (plan view).
pub fn modelled(ui: &mut egui::Ui, within: Option<f64>) {
    let (p, r, ink) = canvas_of(
        ui,
        SIZE,
        "drawing: wires modelled",
        "Seen from above: the line, a cut across it (or a volume), and the wires its field is \
         computed from — every wire of the line, or only those within d of it (far wires add \
         little: quicker, the difference said with each run).",
    );
    let y = r.center().y;
    let (x0, x1) = (r.left() + 6.0, r.right() - 6.0);
    p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.0, ink.faint));
    for k in 0..6 {
        let x = x0 + (x1 - x0) * (k as f32 + 0.5) / 6.0;
        p.circle_filled(pos2(x, y), 2.0, ink.line);
    }
    let c = r.center().x + 8.0;
    p.line_segment(
        [pos2(c, y - 24.0), pos2(c, y + 24.0)],
        Stroke::new(2.0, ink.key),
    );
    text(&p, pos2(c + 4.0, y - 26.0), "cut", ink.key);
    let (a, b) = match within {
        Some(_) => (c - 40.0, c + 40.0),
        None => (x0, x1),
    };
    p.line_segment([pos2(a, y), pos2(b, y)], Stroke::new(2.5, ink.wire));
    if within.is_some() {
        dashed(&p, pos2(c, y + 14.0), pos2(c + 40.0, y + 14.0), ink.key);
        text(&p, pos2(c + 16.0, y + 22.0), "d", ink.key);
    }
    text(
        &p,
        pos2(r.left() + 4.0, r.bottom() - 8.0),
        if within.is_some() {
            "wires within d"
        } else {
            "the whole line"
        },
        ink.line,
    );
}

/// A vertical cut across the line beside a building: the lattice every
/// `step`, the field's isolines (levels) around the conductors.
pub fn cut(ui: &mut egui::Ui, step: f64) {
    let (p, r, ink) = canvas_of(
        ui,
        SIZE,
        "drawing: a vertical cut",
        "A vertical cut across the line, e.g. through a building beside it: the field on a \
         lattice every step from the ground up to above the wires, its isolines at the levels \
         and the bands between them drawn in the cross-section panel.",
    );
    let ground = r.bottom() - 10.0;
    let cx = r.left() + 62.0;
    let n = ((10.0 / step.max(0.1)).round() as usize).clamp(3, 14);
    let gap = (r.width() - 12.0) / n as f32;
    let mut x = r.left() + 6.0;
    while x < r.right() - 4.0 {
        let mut y = ground;
        while y > r.top() + 4.0 {
            p.circle_filled(pos2(x, y), 0.8, ink.faint);
            y -= gap;
        }
        x += gap;
    }
    p.line_segment(
        [pos2(r.left() + 4.0, ground), pos2(r.right() - 4.0, ground)],
        Stroke::new(1.0, ink.line),
    );
    let wires = tower(&p, cx, ground, r.top() + 8.0, &ink);
    // Isolines: ovals around the conductors, the outer one reaching the
    // building.
    let centre = pos2(cx, wires[1].y);
    for (k, (rx, ry)) in [(36.0, 18.0), (62.0, 34.0)].into_iter().enumerate() {
        let pts: Vec<Pos2> = (0..48)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / 48.0;
                centre + vec2(rx * a.cos(), ry * a.sin())
            })
            .collect();
        let c = if k == 0 { ink.key } else { ink.wire };
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.0, c)));
    }
    let house = egui::Rect::from_min_max(
        pos2(r.right() - 40.0, ground - 30.0),
        pos2(r.right() - 10.0, ground),
    );
    p.rect_stroke(
        house,
        0.0,
        Stroke::new(1.5, ink.line),
        egui::StrokeKind::Inside,
    );
    p.line_segment(
        [house.left_top(), pos2(house.center().x, house.top() - 10.0)],
        Stroke::new(1.5, ink.line),
    );
    p.line_segment(
        [
            pos2(house.center().x, house.top() - 10.0),
            house.right_top(),
        ],
        Stroke::new(1.5, ink.line),
    );
}

/// The earth return of a wire (line parameters): the wire at h, its
/// return current pictured as an image at the complex depth h + 2p under
/// the ground (Dubanton, Deri et al.: close to Carson's integral, which
/// the default series computes), p from the soil's resistivity and the
/// frequency.
pub fn earth_return(ui: &mut egui::Ui, rho: f64) {
    let (p, r, ink) = canvas_of(
        ui,
        SIZE,
        "drawing: earth return",
        "A wire's current returns through the earth, spread deep under the line. Pictured as an \
         image wire at the complex depth h + 2p, p = √(ρ / (jωµ₀)) (the Complex depth method; \
         Carson's series, the default, computes the earth's integral itself): the higher the \
         soil's resistivity ρ, the deeper (hundreds of metres at power frequency). The \
         impedances per km come from the wires and the earth; the earth wires reduced out.",
    );
    let g = r.top() + 40.0;
    p.line_segment(
        [pos2(r.left() + 6.0, g), pos2(r.right() - 6.0, g)],
        Stroke::new(1.5, ink.line),
    );
    let w = pos2(r.left() + 40.0, r.top() + 14.0);
    p.circle_filled(w, 3.0, ink.wire);
    text(&p, w + vec2(6.0, 0.0), "I", ink.wire);
    dashed(&p, pos2(w.x - 12.0, g), pos2(w.x - 12.0, w.y), ink.faint);
    text(&p, pos2(w.x - 22.0, (g + w.y) / 2.0), "h", ink.line);
    // Deeper for a higher resistivity (drawn on a log scale).
    let depth = (12.0 + 8.0 * (rho.max(1.0).log10() as f32)).clamp(14.0, 48.0);
    let image = pos2(w.x, (g + depth).min(r.bottom() - 6.0));
    p.circle_stroke(image, 3.0, Stroke::new(1.0, ink.key));
    text(&p, image + vec2(6.0, 0.0), "−I at h + 2p", ink.key);
    dashed(&p, w, image, ink.faint);
    // The return current spread in the soil.
    for k in 1..4 {
        let y = g + 4.0 * k as f32;
        dashed(
            &p,
            pos2(r.right() - 70.0, y),
            pos2(r.right() - 10.0, y),
            ink.faint,
        );
    }
    text(
        &p,
        pos2(r.right() - 64.0, r.top() + 8.0),
        &format!("ρ {rho:.0} Ω·m"),
        ink.line,
    );
}
