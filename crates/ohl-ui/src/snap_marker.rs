// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The snap marker (owner, 2026-11-12): while the pointer moves over a
//! view, a semi-transparent red dot with a ring at the point a click there
//! would pick — the point itself, or the nearest point of a line as drawn.
//! Each view finds that point with the same search its click runs; this
//! module only holds the settings (shown or not, the snap distance) and
//! the painter, so that both applications draw the same thing.
//!
//! The settings live in the [`egui::Context`]'s memory (as the UI scale
//! lives in its zoom), read by every view of the application each frame;
//! each application keeps them between runs under its own storage keys.

use eframe::egui;

/// Radius of the dot, points (not scaled by the zoom).
pub const DOT_RADIUS: f32 = 5.0;
/// Radius of the ring around it, points.
pub const RING_RADIUS: f32 = 8.0;
/// The dot's fill: red, alpha about 0.4 (seen on light and dark themes).
pub const FILL: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(230, 30, 30, 105);
/// The ring: the same red, stronger.
pub const RING: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(230, 30, 30, 170);
/// The snap distance by default, pixels: the 3D view's pick radius before
/// it was a setting (the plan and profile take 2 px less, as before).
pub const DEFAULT_DISTANCE: f32 = 14.0;
/// The range the snap distance may take, pixels.
pub const DISTANCE_RANGE: std::ops::RangeInclusive<f32> = 4.0..=40.0;

/// Whether the marker is shown and how far from the pointer a click picks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// Draw the marker ("Show the snap point").
    pub show: bool,
    /// How far from the pointer a click picks, pixels ("Snap distance").
    pub distance: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show: true,
            distance: DEFAULT_DISTANCE,
        }
    }
}

fn id() -> egui::Id {
    egui::Id::new("ohl-ui-snap-marker-settings")
}

impl Settings {
    /// The settings in force (the default before any were set).
    pub fn get(ctx: &egui::Context) -> Settings {
        ctx.data(|d| d.get_temp::<Settings>(id()))
            .unwrap_or_default()
    }

    /// Puts `self` in force for every view.
    pub fn set(self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(id(), self));
    }

    /// The settings kept in `storage` under `prefix` (`prefix.show`,
    /// `prefix.distance`); missing or out of range: the defaults.
    pub fn load(storage: Option<&dyn eframe::Storage>, prefix: &str) -> Settings {
        let mut s = Settings::default();
        if let Some(st) = storage {
            if let Some(v) = st.get_string(&format!("{prefix}.show")) {
                s.show = v != "false";
            }
            if let Some(v) = st
                .get_string(&format!("{prefix}.distance"))
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| DISTANCE_RANGE.contains(v))
            {
                s.distance = v;
            }
        }
        s
    }

    /// Keeps the settings in `storage` under `prefix`.
    pub fn save(&self, storage: &mut dyn eframe::Storage, prefix: &str) {
        storage.set_string(&format!("{prefix}.show"), self.show.to_string());
        storage.set_string(&format!("{prefix}.distance"), self.distance.to_string());
    }

    /// A pick radius of a view, pixels: the snap distance less `less`
    /// (the views that picked within a smaller radius before keep the
    /// difference), at least 2.
    pub fn radius(&self, less: f32) -> f64 {
        f64::from((self.distance - less).max(2.0))
    }

    /// The two settings for a View menu: the tick and the distance, each
    /// explaining its effect. Changed: put in force (and returned true).
    pub fn menu(ui: &mut egui::Ui) -> bool {
        let mut s = Settings::get(ui.ctx());
        let mut changed = ui
            .checkbox(&mut s.show, "Show the snap point")
            .on_hover_text(
                "A red dot where a click would pick, while the pointer moves over a view \
                 (it follows the snap settings and the snap distance; no dot: nothing to \
                 pick there). Costs one pick each time the pointer moves.",
            )
            .changed();
        ui.horizontal(|ui| {
            ui.label("Snap distance (px)");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut s.distance)
                        .range(DISTANCE_RANGE)
                        .speed(0.2)
                        .max_decimals(0),
                )
                .on_hover_text(
                    "How far from the pointer a click still picks an object, in screen \
                     pixels (the 2D views 2 px less). Larger: easier to hit, but a near \
                     object may be taken instead of the one meant. No effect on results.",
                )
                .changed();
        });
        if changed {
            s.set(ui.ctx());
        }
        changed
    }
}

/// Paints the marker at `at` (screen points).
pub fn paint(painter: &egui::Painter, at: egui::Pos2) {
    painter.circle_filled(at, DOT_RADIUS, FILL);
    painter.circle_stroke(at, RING_RADIUS, egui::Stroke::new(1.5, RING));
}

/// The marker's place, kept by a view between frames: found again only
/// when the pointer moved or what it is computed from changed (`key`: a
/// hash of the view, the model state and the snap settings).
#[derive(Debug, Clone, Default)]
pub struct Hover {
    /// Where the pointer was, and the key, when last found.
    last: Option<(egui::Pos2, u64)>,
    /// Where a click would pick (screen points), none: nothing there.
    pub at: Option<egui::Pos2>,
}

impl Hover {
    /// Whether the point must be found again for the pointer at `pointer`
    /// and `key` (else the one found stands).
    pub fn stale(&self, pointer: egui::Pos2, key: u64) -> bool {
        self.last != Some((pointer, key))
    }

    /// The point found for `pointer` and `key`.
    pub fn found(&mut self, pointer: egui::Pos2, key: u64, at: Option<egui::Pos2>) {
        self.last = Some((pointer, key));
        self.at = at;
    }

    /// The pointer left the view (or the marker is off): nothing shown.
    pub fn clear(&mut self) {
        self.last = None;
        self.at = None;
    }

    /// Paints the marker if there is one and it is shown.
    pub fn paint(&self, painter: &egui::Painter) {
        if let Some(at) = self.at
            && Settings::get(painter.ctx()).show
        {
            paint(painter, at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_hover_is_found_again_only_when_moved() {
        let ctx = egui::Context::default();
        assert_eq!(Settings::get(&ctx), Settings::default());
        let s = Settings {
            show: false,
            distance: 20.0,
        };
        s.set(&ctx);
        assert_eq!(Settings::get(&ctx), s);
        assert_eq!(s.radius(2.0), 18.0);
        let mut h = Hover::default();
        let p = egui::pos2(1.0, 2.0);
        assert!(h.stale(p, 7));
        h.found(p, 7, Some(p));
        assert!(!h.stale(p, 7));
        assert!(h.stale(p, 8));
        assert!(h.stale(egui::pos2(1.0, 3.0), 7));
        // Kept between runs; nothing kept, or out of range: the default.
        #[derive(Default)]
        struct Memory(std::collections::HashMap<String, String>);
        impl eframe::Storage for Memory {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.to_owned(), value);
            }
            fn remove_string(&mut self, key: &str) {
                self.0.remove(key);
            }
            fn flush(&mut self) {}
        }
        use eframe::Storage as _;
        let mut m = Memory::default();
        s.save(&mut m, "x");
        assert_eq!(Settings::load(Some(&m), "x"), s);
        assert_eq!(Settings::load(Some(&m), "y"), Settings::default());
        assert_eq!(Settings::load(None, "x"), Settings::default());
        m.set_string("x.distance", "999".into());
        assert_eq!(Settings::load(Some(&m), "x").distance, DEFAULT_DISTANCE);
    }
}
