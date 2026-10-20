// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Box and lasso selection of LiDAR points in the 3D view and the
//! profile: **Alt + drag** with the button that selects draws a box,
//! **Alt + Shift + drag** a free lasso; on release, every LiDAR point drawn
//! inside it is selected (the rule of a click: drawn, of a class shown
//! with a colour) and the Edit window of LiDAR points opens on them —
//! their count, a class for all of them ([`crate::lidar_edit`]).
//!
//! The selecting button is the left one, or the right one with the option
//! "tools act on a left-click" ([`crate::controls::click_buttons`]). While
//! Alt is held the keys do not move the 3D view's camera (Shift would move
//! it down), and the drag does not turn or pan the view.

use eframe::egui;

/// A shape on a view's screen (pixels from the view's top left).
#[derive(Debug, Clone, PartialEq)]
pub enum Area {
    /// A rectangle between two corners.
    Box([f64; 2], [f64; 2]),
    /// A closed polygon (its last point joined to the first).
    Lasso(Vec<[f64; 2]>),
}

impl Area {
    /// Its lowest and highest corner on the screen.
    pub fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        match self {
            Area::Box(a, b) => (
                [a[0].min(b[0]), a[1].min(b[1])],
                [a[0].max(b[0]), a[1].max(b[1])],
            ),
            Area::Lasso(p) => {
                let mut lo = [f64::INFINITY; 2];
                let mut hi = [f64::NEG_INFINITY; 2];
                for q in p {
                    for k in 0..2 {
                        lo[k] = lo[k].min(q[k]);
                        hi[k] = hi[k].max(q[k]);
                    }
                }
                (lo, hi)
            }
        }
    }

    /// Whether screen point `p` is inside (a lasso: by the even-odd rule).
    pub fn contains(&self, p: [f64; 2]) -> bool {
        let (lo, hi) = self.bounds();
        if p[0] < lo[0] || p[0] > hi[0] || p[1] < lo[1] || p[1] > hi[1] {
            return false;
        }
        match self {
            Area::Box(..) => true,
            Area::Lasso(v) => {
                let mut inside = false;
                let mut j = v.len().wrapping_sub(1);
                for i in 0..v.len() {
                    let (a, b) = (v[i], v[j]);
                    if (a[1] > p[1]) != (b[1] > p[1])
                        && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
                    {
                        inside = !inside;
                    }
                    j = i;
                }
                inside
            }
        }
    }

    /// Whether the rectangle `lo`–`hi` on the screen may hold points
    /// inside (it meets the area's bounds).
    pub fn meets(&self, lo: [f64; 2], hi: [f64; 2]) -> bool {
        let (a, b) = self.bounds();
        lo[0] <= b[0] && hi[0] >= a[0] && lo[1] <= b[1] && hi[1] >= a[1]
    }
}

/// A box or lasso being drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaDrag {
    /// A lasso (else a box).
    pub lasso: bool,
    /// The points drawn (a box: where it started and where it is),
    /// screen positions.
    pub points: Vec<egui::Pos2>,
}

impl AreaDrag {
    /// The area drawn, in pixels from `origin` (the view's top left).
    pub fn area(&self, origin: egui::Pos2) -> Option<Area> {
        let q = |p: &egui::Pos2| [f64::from(p.x - origin.x), f64::from(p.y - origin.y)];
        if self.lasso {
            (self.points.len() >= 3).then(|| Area::Lasso(self.points.iter().map(q).collect()))
        } else {
            match (self.points.first(), self.points.last()) {
                (Some(a), Some(b)) if a != b => Some(Area::Box(q(a), q(b))),
                _ => None,
            }
        }
    }

    /// Draws it: a dashed outline (a lasso closed back to its start).
    pub fn paint(&self, painter: &egui::Painter) {
        let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 230, 90));
        let fill = egui::Color32::from_rgba_unmultiplied(255, 230, 90, 24);
        if self.lasso {
            let mut closed = self.points.clone();
            if let Some(first) = self.points.first() {
                closed.push(*first);
            }
            painter.extend(egui::Shape::dashed_line(&closed, stroke, 6.0, 4.0));
        } else if let (Some(a), Some(b)) = (self.points.first(), self.points.last()) {
            let r = egui::Rect::from_two_pos(*a, *b);
            painter.rect_filled(r, 0.0, fill);
            let c = [
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
            ];
            let closed = [c[0], c[1], c[2], c[3], c[0]];
            painter.extend(egui::Shape::dashed_line(&closed, stroke, 6.0, 4.0));
        }
    }
}

/// Whether a drag starting now draws an area (Alt held), and a lasso
/// (Shift too).
pub fn wanted(i: &egui::InputState) -> Option<bool> {
    i.modifiers.alt.then_some(i.modifiers.shift)
}

/// One frame of a view's area selection with `button` (the selecting
/// one): a drag of it begun with Alt held starts one, its points follow
/// the pointer, its release gives the area (pixels from the view's top
/// left). True in the second value while an area is drawn (the drag then
/// neither turns nor pans the view).
pub fn frame(
    drag: &mut Option<AreaDrag>,
    ui: &egui::Ui,
    response: &egui::Response,
    button: egui::PointerButton,
) -> (Option<Area>, bool) {
    let origin = response.rect.min;
    if drag.is_none()
        && response.drag_started_by(button)
        && let Some(lasso) = ui.input(wanted)
    {
        let start = ui
            .input(|i| i.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos());
        *drag = start.map(|p| AreaDrag {
            lasso,
            points: vec![p],
        });
    }
    let Some(d) = drag.as_mut() else {
        return (None, false);
    };
    if let Some(p) = ui.input(|i| i.pointer.latest_pos()) {
        if d.lasso {
            if d.points.last().is_none_or(|q| q.distance(p) >= 2.0) {
                d.points.push(p);
            }
        } else {
            d.points.truncate(1);
            d.points.push(p);
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        *drag = None;
        return (None, true);
    }
    let released = ui.input(|i| !i.pointer.button_down(button));
    if released {
        let area = d.area(origin);
        *drag = None;
        return (area, true);
    }
    ui.ctx().request_repaint();
    (None, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_a_box_and_a_lasso() {
        let b = Area::Box([10.0, 50.0], [0.0, 0.0]);
        assert!(b.contains([5.0, 25.0]));
        assert!(!b.contains([11.0, 25.0]));
        // An L-shaped lasso: the notch is outside.
        let l = Area::Lasso(vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 4.0],
            [4.0, 4.0],
            [4.0, 10.0],
            [0.0, 10.0],
        ]);
        assert!(l.contains([2.0, 8.0]));
        assert!(l.contains([8.0, 2.0]));
        assert!(!l.contains([8.0, 8.0]), "the notch");
        assert!(!l.contains([-1.0, 5.0]));
        assert!(l.meets([9.0, 9.0], [20.0, 20.0]));
        assert!(!l.meets([11.0, 0.0], [20.0, 20.0]));
    }
}
