// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the plan windows share (owner, 2026-11-07: "make sure that the
//! menu for the plans look similar … but each keep their own parameters
//! and functionality"): Plans & sheets › Plan and profile sheets… and
//! Plans & sheets › Plans › … are laid out alike — a line saying what the
//! window makes, one grid of settings (Template, Route, Paper, Scale first;
//! then each window's own; Formats and Folder last), the window's own
//! sections, and the same footer (what went wrong, **Export**).

use std::path::PathBuf;

use eframe::egui;
use ohl_drawing::export::Format;

/// The windows' frame: titled, not collapsible, sized alike, its content
/// scrolled when taller than 85 % of the window.
pub fn window<R>(
    ctx: &egui::Context,
    title: &str,
    open: &mut bool,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    let max_height = (ctx.content_rect().height() * 0.85).max(300.0);
    egui::Window::new(title)
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(WIDTH)
        .show(ctx, |ui| {
            ui.set_min_width(WIDTH);
            egui::ScrollArea::vertical()
                .id_salt((title, "scroll"))
                .max_height(max_height)
                // Taller content: as tall as allowed, not the default
                // minimum.
                .min_scrolled_height(max_height)
                .show(ui, content)
                .inner
        })
        .and_then(|r| r.inner)
}

/// The windows' width, points.
pub const WIDTH: f32 = 480.0;

/// What the window makes, a small line at its top.
pub fn intro(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().weak());
    ui.add_space(4.0);
}

/// The settings grid, its rows two columns: a name, its value.
pub fn grid<R>(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, rows)
        .inner
}

/// A row that only says something (a value taken from elsewhere): its
/// name, the text and, when given, a button that leads to where it is set.
pub fn shown_row(ui: &mut egui::Ui, name: &str, text: &str, go: Option<&str>) -> bool {
    ui.label(name);
    let mut clicked = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(text);
        if let Some(g) = go {
            clicked = ui
                .small_button(g)
                .on_hover_text("Set there; this window takes it")
                .clicked();
        }
    });
    ui.end_row();
    clicked
}

/// The Formats row: a tick for each of `offered`, in a row, each with what
/// it writes on hover.
pub fn formats_row(ui: &mut egui::Ui, offered: &[Format], chosen: &mut Vec<Format>) {
    ui.label("Formats");
    ui.horizontal_wrapped(|ui| {
        for f in offered {
            let mut on = chosen.contains(f);
            if ui
                .checkbox(&mut on, f.name().to_uppercase())
                .on_hover_text(f.description())
                .changed()
            {
                chosen.retain(|x| x != f);
                if on {
                    chosen.push(*f);
                }
            }
        }
    });
    ui.end_row();
}

/// The Folder row: **Choose…** and the folder written into (none chosen:
/// `fallback`, said so, else "no folder chosen").
pub fn folder_row(
    ui: &mut egui::Ui,
    out: &mut Option<PathBuf>,
    fallback: Option<(&PathBuf, &str)>,
) {
    ui.label("Folder");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Choose…").clicked()
            && let Some(p) = rfd::FileDialog::new().pick_folder()
        {
            *out = Some(p);
        }
        let shown = match (out.as_ref(), fallback) {
            (Some(p), _) => p.display().to_string(),
            (None, Some((p, whose))) => format!("{} ({whose})", p.display()),
            (None, None) => "no folder chosen".into(),
        };
        ui.label(egui::RichText::new(shown).small());
    });
    ui.end_row();
}

/// The footer: what went wrong, then **Export** (greyed while `busy`).
/// Whether Export was clicked.
pub fn footer(ui: &mut egui::Ui, error: Option<&str>, busy: bool) -> bool {
    ui.separator();
    if let Some(e) = error {
        crate::menu::error_label(ui, e);
    }
    ui.add_enabled(!busy, egui::Button::new("Export"))
        .on_disabled_hover_text("An export is under way")
        .clicked()
}
