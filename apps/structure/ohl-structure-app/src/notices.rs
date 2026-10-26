// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Notices (owner: UI review step 1, 2026-10-26): what the last action did
//! is shown at the bottom of the window (the status line, as before) and
//! as a short notice at the bottom right. A success or information fades
//! after a few seconds; an error stays until it is closed (its ×). A
//! long error scrolls in a box of fixed height, so a notice never grows
//! taller than the window and stays closable.

use eframe::egui;

/// How long a success or information notice shows, s (it fades in the
/// last second).
pub const SHOWN: f64 = 4.0;

/// The errors kept at once (the oldest closes when another comes).
const ERRORS_KEPT: usize = 3;

/// A notice.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    /// What it says.
    pub text: String,
    /// An error: stays until closed.
    pub error: bool,
    /// When it came, s (the window's clock).
    pub at: f64,
    id: u64,
}

/// The notices shown.
#[derive(Debug, Clone, Default)]
pub struct Notices {
    /// Shown now, the oldest first.
    pub items: Vec<Notice>,
    /// The message last seen (a notice when it changes).
    seen: Option<(String, bool)>,
    next: u64,
}

impl Notices {
    /// A notice for `message` when it is not the one seen last (`now`:
    /// the window's clock, s). An information replaces the one before it;
    /// errors stay (the last few).
    pub fn observe(&mut self, message: &Option<(String, bool)>, now: f64) {
        if *message == self.seen {
            return;
        }
        self.seen.clone_from(message);
        let Some((text, error)) = message else {
            return;
        };
        if !error {
            self.items.retain(|n| n.error);
        } else if self.items.iter().filter(|n| n.error).count() >= ERRORS_KEPT
            && let Some(k) = self.items.iter().position(|n| n.error)
        {
            self.items.remove(k);
        }
        self.next += 1;
        self.items.push(Notice {
            text: text.clone(),
            error: *error,
            at: now,
            id: self.next,
        });
    }

    /// The notices at the bottom right of the window, above the status
    /// line; an information gone after [`SHOWN`] s.
    pub fn ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.items.retain(|n| n.error || now - n.at < SHOWN);
        if self.items.is_empty() {
            return;
        }
        // Drawn again when an information starts to fade, then while it
        // fades (in the tests' steps of 0.25 s: not at once).
        let tick = if cfg!(test) { 0.3 } else { 0.05 };
        if let Some(left) = self
            .items
            .iter()
            .filter(|n| !n.error)
            .map(|n| SHOWN - (now - n.at))
            .min_by(f64::total_cmp)
        {
            let wait = if left > 1.0 { left - 1.0 } else { tick };
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait.max(tick)));
        }
        let mut close: Option<u64> = None;
        egui::Area::new(egui::Id::new("notices"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -40.0))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                for n in &self.items {
                    let fade = if n.error {
                        1.0
                    } else {
                        ((SHOWN - (now - n.at)) as f32).clamp(0.0, 1.0)
                    };
                    let stroke = if n.error {
                        ui.visuals().error_fg_color
                    } else {
                        crate::app::ok_color(ui)
                    };
                    ui.scope(|ui| {
                        ui.set_opacity(fade);
                        egui::Frame::popup(ui.style())
                            .stroke(egui::Stroke::new(1.5, stroke))
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    ui.vertical(|ui| {
                                        ui.set_max_width(370.0);
                                        if n.error {
                                            crate::app::error_label(ui, n.text.as_str());
                                        } else {
                                            ui.label(&n.text);
                                        }
                                    });
                                    if ui
                                        .small_button("\u{00d7}")
                                        .on_hover_text("Close this notice")
                                        .clicked()
                                    {
                                        close = Some(n.id);
                                    }
                                });
                            });
                    });
                    ui.add_space(4.0);
                }
            });
        if let Some(id) = close {
            self.items.retain(|n| n.id != id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An information replaces the one before; errors stay, the last
    /// three; the same message twice is one notice.
    #[test]
    fn errors_stay_information_replaces() {
        let mut n = Notices::default();
        n.observe(&Some(("saved".into(), false)), 0.0);
        n.observe(&Some(("saved".into(), false)), 0.1);
        assert_eq!(n.items.len(), 1);
        n.observe(&Some(("moved".into(), false)), 0.2);
        assert_eq!(n.items.len(), 1);
        for k in 0..4 {
            n.observe(&Some((format!("error {k}"), true)), 1.0);
        }
        let errors: Vec<&str> = n.items.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(errors, ["moved", "error 1", "error 2", "error 3"]);
    }
}
