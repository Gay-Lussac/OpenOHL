// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Put the project's strings in a library…** (Line › Structures; owner,
//! 2026-11-18: "yes do the one-off command for existing projects"): the
//! preview of [`crate::string_entries::plan`] — the entries to make (their
//! labels may be renamed) and how many strings each serves, the entries
//! of the library reused, the strings left inline with their reason — and
//! **Apply**: one command (one undo). A tool window (Escape cancels).

use eframe::egui;

use crate::string_entries::{Plan, Values};
use crate::tool_ui::{Keys, Pressed};

/// The window's title.
pub const TITLE: &str = "Put the project's strings in a library";

/// Its menu item (Line › Structures).
pub const MENU: &str = "Put the project's strings in a library…";

/// The window's state: its plan (labels as renamed), why Apply was
/// refused.
#[derive(Debug, Clone, Default)]
pub struct Window {
    /// What Apply does.
    pub plan: Plan,
    /// Why it was not applied (kept open).
    pub error: Option<String>,
}

/// What the window asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// Apply the plan.
    Apply,
    /// Close, nothing changed.
    Cancel,
}

/// A string's values in words.
fn values_words(v: &Values) -> String {
    let mut w = format!(
        "{}{}, {:.3} m, {:.1} N, {:.4} m², EA {:.3e} N",
        if v.strain { "strain" } else { "suspension" },
        if v.legs >= 2 { " (leg of a V)" } else { "" },
        v.length,
        v.weight,
        v.wind_area,
        v.stiffness
    );
    if let Some(r) = v.max_load {
        w.push_str(&format!(", rated {r:.0} N"));
    }
    for (n, s) in ["swing", "across", "along"].iter().zip(v.swings) {
        if let Some(s) = s {
            w.push_str(&format!(", {n} {s:.1}°"));
        }
    }
    w
}

/// Draws the window; what it asked for.
pub fn show(ctx: &egui::Context, w: &mut Window) -> Option<Asked> {
    let mut asked = None;
    let mut open = true;
    egui::Window::new(TITLE)
        .open(&mut open)
        .resizable(true)
        .default_width(720.0)
        .show(ctx, |ui| {
            crate::tool_ui::intro(
                ui,
                "Every string written inline in the project (on its structures and in its \
                 structure type files) gets an entry of the project's insulator library: \
                 strings of exactly the same values share one.",
                "The values are compared as stored, to the last digit: strings never merge \
                 by approximation. An entry of the library with exactly those values is \
                 reused. Each string keeps every value it has and names its entry, so sags, \
                 tensions, loads and swings stay the same. Apply is one command: one undo \
                 takes it all back. The labels of the new entries may be renamed here.",
                (
                    "user-guide/structure-library",
                    "put-the-projects-strings-in-a-library",
                ),
            );
            let made: Vec<usize> = (0..w.plan.groups.len())
                .filter(|&k| !w.plan.groups[k].reused)
                .collect();
            let reused: Vec<usize> = (0..w.plan.groups.len())
                .filter(|&k| w.plan.groups[k].reused)
                .collect();
            egui::ScrollArea::vertical()
                .max_height(460.0)
                .show(ui, |ui| {
                    crate::tool_ui::heading(
                        ui,
                        &format!("Entries to make ({})", made.len()),
                        "New .ohli entries of the project's insulator library: a label (rename \
                         it here: letters, digits, - _ .), its values, the strings it serves",
                    );
                    egui::Grid::new("string-library-new")
                        .striped(true)
                        .show(ui, |ui| {
                            for &k in &made {
                                let g = &mut w.plan.groups[k];
                                ui.add(egui::TextEdit::singleline(&mut g.id).desired_width(220.0));
                                ui.label(values_words(&g.values));
                                let names: Vec<String> = g
                                    .strings
                                    .iter()
                                    .map(|(p, l)| format!("{l} ({})", p.describe()))
                                    .collect();
                                ui.label(format!("{} string(s)", g.strings.len()))
                                    .on_hover_text(names.join("\n"));
                                ui.end_row();
                            }
                        });
                    if made.is_empty() {
                        ui.label(egui::RichText::new("None.").weak());
                    }
                    ui.add_space(6.0);
                    crate::tool_ui::heading(
                        ui,
                        &format!("Entries of the library reused ({})", reused.len()),
                        "Entries already in the library with exactly the same values",
                    );
                    egui::Grid::new("string-library-reused")
                        .striped(true)
                        .show(ui, |ui| {
                            for &k in &reused {
                                let g = &w.plan.groups[k];
                                ui.label(&g.id);
                                ui.label(values_words(&g.values));
                                ui.label(format!("{} string(s)", g.strings.len()));
                                ui.end_row();
                            }
                        });
                    if reused.is_empty() {
                        ui.label(egui::RichText::new("None.").weak());
                    }
                    ui.add_space(6.0);
                    crate::tool_ui::heading(
                        ui,
                        &format!("Strings left inline ({})", w.plan.left.len()),
                        "Strings that keep their values written on them, each with its reason",
                    );
                    egui::Grid::new("string-library-left")
                        .striped(true)
                        .show(ui, |ui| {
                            for (p, l, why) in &w.plan.left {
                                ui.label(format!("{l} ({})", p.describe()));
                                ui.label(why);
                                ui.end_row();
                            }
                        });
                    if w.plan.left.is_empty() {
                        ui.label(egui::RichText::new("None.").weak());
                    }
                });
            if let Some(e) = &w.error {
                crate::menu::error_label(ui, egui::RichText::new(e));
            }
            let empty = w.plan.groups.is_empty();
            match crate::tool_ui::buttons(
                ui,
                "Apply",
                !empty,
                Some("No string written inline: nothing to put in the library"),
                Keys::Both,
            ) {
                Some(Pressed::Action) => asked = Some(Asked::Apply),
                Some(Pressed::Cancel) => asked = Some(Asked::Cancel),
                None => {}
            }
        });
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) || !open {
        asked = Some(Asked::Cancel);
    }
    asked
}
