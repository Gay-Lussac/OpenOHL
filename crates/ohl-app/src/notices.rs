// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Notices (owner: OHL-Design UI review, 2026-10-26, as OHL-Structure's):
//! what the last action did is shown at the bottom of the window (the
//! status line, as before) and as a short notice at the bottom right. A
//! success or information fades after a few seconds; an error stays until
//! it is closed (its ×). A long error scrolls in a box of fixed height, so
//! a notice never grows taller than the window and stays closable.
//!
//! An edit's message says in a short sentence what happened ("Section S1
//! changed and recalculated."); the detailed text the edit gave (its log
//! entry, the sections and weather cases solved, the time) shows on hover
//! ([`plain`]).

use eframe::egui;

/// How long a success or information notice shows, s (it fades in the
/// last second).
pub const SHOWN: f64 = 4.0;

/// The errors kept at once (the oldest closes when another comes).
const ERRORS_KEPT: usize = 3;

/// A message in a short human sentence, when it is one of the detailed
/// kinds (none: it is shown as it is):
///
/// - an edit: "changed section S1 (log entry 13): 1 sections touched, 1
///   re-solved over 1 weather cases (4 others …) in 954ms" → "Section S1
///   changed and recalculated." (nothing solved again: "… changed.");
/// - Save all: "saved: project file, …" → "Saved.";
/// - solved in the background: "solved: a, b" → "Solved a, b.".
pub fn plain(text: &str) -> Option<String> {
    if let Some((what, rest)) = text.split_once(" (log entry ")
        && let Some((_, detail)) = rest.split_once("): ")
    {
        let recalculated = detail
            .split_once(" re-solved")
            .and_then(|(head, _)| head.rsplit(' ').next())
            .and_then(|n| n.parse::<usize>().ok())
            .is_some_and(|n| n > 0);
        let done = done_sentence(what);
        return Some(if recalculated {
            format!("{done} and recalculated.")
        } else {
            format!("{done}.")
        });
    }
    if text.starts_with("saved: ") && !text.contains(" — ") {
        return Some("Saved.".into());
    }
    if let Some(cases) = text.strip_prefix("solved: ") {
        return Some(format!("Solved {cases}."));
    }
    None
}

/// What an edit did, as a sentence without its full stop: "changed
/// section S1" → "Section S1 changed", "undo" → "Undone".
fn done_sentence(what: &str) -> String {
    match what {
        "undo" => return "Undone".into(),
        "redo" => return "Redone".into(),
        _ => {}
    }
    // A verb in the past first ("changed", "moved", "deleted", "added",
    // "imported", "renamed"): the object first, the verb after.
    if let Some((verb, object)) = what.split_once(' ')
        && verb.ends_with("ed")
        && !object.is_empty()
        && !object.contains(" (")
        && !object.contains(" into ")
    {
        return format!("{} {verb}", capital(object));
    }
    capital(what)
}

/// `text` with a capital first letter.
fn capital(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// A message as shown (the status line, a notice): its short sentence,
/// and the full text for the hover when they differ.
pub fn shown(text: &str) -> (String, Option<&str>) {
    match plain(text) {
        Some(p) => (p, Some(text)),
        None => (text.to_owned(), None),
    }
}

/// A notice.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    /// What it says (the short sentence).
    pub text: String,
    /// The detailed text, on hover (none: the text says it all).
    pub detail: Option<String>,
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
    /// Not drawn (the tests, but those of the notices: the status line
    /// says the same, read there).
    pub hidden: bool,
}

impl Notices {
    /// Notices not drawn ([`Notices::hidden`]; the tests).
    #[cfg(test)]
    pub fn hidden() -> Self {
        Self {
            hidden: true,
            ..Self::default()
        }
    }

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
        let (short, detail) = shown(text);
        self.items.push(Notice {
            text: short,
            detail: detail.map(str::to_owned),
            error: *error,
            at: now,
            id: self.next,
        });
    }

    /// The notices at the bottom right of the window, above the status
    /// line; an information gone after [`SHOWN`] s. Not drawn while
    /// [`Notices::hidden`] (the tests that read the status line).
    pub fn ui(&mut self, ctx: &egui::Context) {
        if self.hidden {
            return;
        }
        let now = ctx.input(|i| i.time);
        self.items.retain(|n| n.error || now - n.at < SHOWN);
        if self.items.is_empty() {
            return;
        }
        // Drawn again when an information starts to fade, then while it
        // fades (not in the tests: their frames come as they step).
        let tick = 0.05;
        if !cfg!(test)
            && let Some(left) = self
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
                        crate::menu::ok_color(ui)
                    };
                    ui.scope(|ui| {
                        ui.set_opacity(fade);
                        egui::Frame::popup(ui.style())
                            .stroke(egui::Stroke::new(1.5, stroke))
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    ui.vertical(|ui| {
                                        ui.set_max_width(370.0);
                                        let r = if n.error {
                                            crate::menu::error_label(ui, n.text.as_str())
                                        } else {
                                            ui.label(&n.text)
                                        };
                                        if let Some(d) = &n.detail {
                                            r.on_hover_text(d);
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

    /// An edit's message in a short sentence, the detail kept.
    #[test]
    fn an_edit_is_said_in_a_short_sentence() {
        let long = "changed section S1 (log entry 13): 1 sections touched, 1 re-solved over 1 \
                    weather cases (4 others to solve again when wanted) in 954ms";
        assert_eq!(
            plain(long).as_deref(),
            Some("Section S1 changed and recalculated.")
        );
        let (short, detail) = shown(long);
        assert_eq!(short, "Section S1 changed and recalculated.");
        assert_eq!(detail, Some(long));
        let nothing = "moved structure T3 (log entry 2): 0 sections touched, 0 re-solved over 5 \
                       weather cases (0 others to solve again when wanted) in 3ms";
        assert_eq!(plain(nothing).as_deref(), Some("Structure T3 moved."));
        assert_eq!(
            plain("undo (log entry 4): 2 sections touched, 2 re-solved over 1 weather cases (0 others to solve again when wanted) in 5ms")
                .as_deref(),
            Some("Undone and recalculated.")
        );
        assert_eq!(plain("saved: project file").as_deref(), Some("Saved."));
        assert_eq!(plain("saved: nothing — project file: busy"), None);
        assert_eq!(plain("solved: wind").as_deref(), Some("Solved wind."));
        assert_eq!(plain("display weather cases set"), None);
    }
}
