// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the plan windows share (owner, 2026-11-07: "make sure that the
//! menu for the plans look similar … but each keep their own parameters
//! and functionality"): Plans & sheets › Profile sheets…, Route plan…,
//! Situation plans…, Phase plan… and Plan set… are laid out alike — a line saying what the
//! window makes, one grid of settings (Template, Route, Paper, Scale first;
//! then each window's own; Formats and Folder last), the window's own
//! sections, and the same footer (what went wrong, **Export**).

use std::path::PathBuf;

use eframe::egui;
use ohl_drawing::export::Format;

/// The windows' frame: titled, not collapsible, all as wide and first
/// opened at the same place (top right, owner 2026-11-08: "have the
/// window be similar in size and content"), its content scrolled when
/// taller than 85 % of the window.
pub fn window<R>(
    ctx: &egui::Context,
    title: &str,
    open: &mut bool,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    let screen = ctx.content_rect();
    let max_height = (screen.height() * 0.85).max(300.0);
    egui::Window::new(title)
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(WIDTH)
        .pivot(egui::Align2::RIGHT_TOP)
        .default_pos(screen.right_top() + egui::vec2(-8.0, 16.0))
        .show(ctx, |ui| {
            ui.set_min_width(WIDTH);
            ui.set_max_width(WIDTH);
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
pub const WIDTH: f32 = 540.0;

/// What the window makes, a small line at its top, and **How it works:
/// user guide ↗** opening its page of the manual (`guide`, e.g.
/// `user-guide/route-plan`, at `anchor`; owner, 2026-11-13: an
/// explanation page as for the checks). Why the page did not open, when
/// clicked and it did not.
pub fn intro(ui: &mut egui::Ui, text: &str, guide: (&str, &str)) -> Option<String> {
    ui.label(egui::RichText::new(text).small().weak());
    let mut error = None;
    if ui
        .small_button("How it works: user guide ↗")
        .on_hover_text("What each setting does and how the drawing is made")
        .clicked()
        && let Err(e) = crate::help::open_section(guide.0, guide.1)
    {
        error = Some(e);
    }
    ui.add_space(4.0);
    error
}

/// The settings grid, its rows two columns: a name, its value.
pub fn grid<R>(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, rows)
        .inner
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

/// The Name row (owner, 2026-11-11: "the possibility to give the plan a
/// name, not just the export name given by the app"): the name the files
/// take, `own` what they are called without one.
pub fn name_row(ui: &mut egui::Ui, id: &str, name: &mut String, own: &str) {
    let label = ui.label("Name");
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(name)
                .id_salt((id, "name"))
                .hint_text(own)
                .desired_width(220.0),
        )
        .labelled_by(label.id)
        .on_hover_text(format!(
            "The name of the files written, in place of the application's ({own}). With \
             several routes or plans written at once, each one's own name follows yours, so \
             no file overwrites another; sheets, formats and a plan set's drawings keep \
             their endings. Empty: the application's names"
        ));
        if !name.trim().is_empty() && ui.small_button("×").on_hover_text("No name").clicked() {
            name.clear();
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
