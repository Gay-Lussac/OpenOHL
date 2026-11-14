// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the windows the tools open share, in both applications (owner,
//! 2026-11-14: "user friendlier for humans that don't know the program"):
//! at the top one plain sentence of what the window does, its longer text
//! behind a **(?)** and a **User guide ↗** button opening the tool's
//! section of the manual ([`intro`]); labels whose help was only on hover
//! with a visible (?) ([`label`], [`heading`]); one button row at the
//! bottom, the action first, Cancel last ([`buttons`]). The rules:
//! docs/development/tool-pages.md. Each application opens its manual
//! itself (the closure given to [`intro`]).

use eframe::egui;

/// A heading's anchor as the book makes it: lower case, spaces as `-`,
/// other signs left out.
pub fn anchor(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// A small **(?)** whose hover holds `long`.
pub fn question(ui: &mut egui::Ui, long: &str) -> egui::Response {
    ui.add(
        egui::Label::new(egui::RichText::new("(?)").small().strong()).sense(egui::Sense::hover()),
    )
    .on_hover_text(long)
}

/// Whether `text` (a page of the book, as markdown) has a heading whose
/// anchor is `anchor` — for the tests that every User guide button
/// leads somewhere.
pub fn has_heading(text: &str, anchor_wanted: &str) -> bool {
    text.lines()
        .any(|l| l.starts_with('#') && anchor(l.trim_start_matches('#').trim()) == anchor_wanted)
}

/// The window's top: `what` it does in one sentence, `more` behind a
/// **(?)**, a **User guide ↗** button calling `open` (the application
/// opens its manual at the tool's section; why it did not is shown below
/// the row). `key` tells this window's error apart from another's.
pub fn intro(
    ui: &mut egui::Ui,
    what: &str,
    more: &str,
    key: &str,
    open: impl FnOnce() -> Result<(), String>,
) {
    let id = ui.id().with(("tool-guide-error", key));
    ui.add(egui::Label::new(what).wrap());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if !more.is_empty() {
            question(ui, more);
            ui.label(egui::RichText::new("more").small().weak())
                .on_hover_text(more);
        }
        if guide_clicked(ui) {
            let error = open().err();
            ui.data_mut(|d| d.insert_temp(id, error));
        }
    });
    if let Some(Some(e)) = ui.data(|d| d.get_temp::<Option<String>>(id)) {
        crate::messages::error_label(ui, egui::RichText::new(e).small());
    }
    ui.add_space(4.0);
}

/// The small **User guide ↗** button; true when clicked.
fn guide_clicked(ui: &mut egui::Ui) -> bool {
    ui.small_button("User guide ↗")
        .on_hover_text("This tool's section of the manual: what it does, step by step")
        .clicked()
}

/// A **User guide ↗** button on its own (a tool strip's): `open` called
/// on a click; why it did not open shown beside it. `key` tells this
/// button's error apart from another's.
pub fn guide_button(ui: &mut egui::Ui, key: &str, open: impl FnOnce() -> Result<(), String>) {
    let id = ui.id().with(("tool-guide-error", key));
    if guide_clicked(ui) {
        let error = open().err();
        ui.data_mut(|d| d.insert_temp(id, error));
    }
    if let Some(Some(e)) = ui.data(|d| d.get_temp::<Option<String>>(id)) {
        crate::messages::error_label(ui, egui::RichText::new(e).small());
    }
}

/// A field's name with its help behind a visible **(?)** (the help on
/// hover of the name too).
pub fn label(ui: &mut egui::Ui, text: &str, help: &str) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let r = ui.label(text).on_hover_text(help);
        question(ui, help);
        r
    })
    .inner
}

/// A section's title with its explanation behind a **(?)** (the small
/// grey lines that were under it).
pub fn heading(ui: &mut egui::Ui, title: &str, help: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.strong(title).on_hover_text(help);
        question(ui, help);
    });
}

/// What the button row asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// The window's action (Apply, Add, Keep…).
    Action,
    /// Cancel.
    Cancel,
}

/// The keys a window takes for its buttons (said on their hover).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keys {
    /// Enter for the action, Escape for Cancel.
    Both,
    /// Escape for Cancel only (a deletion's confirmation: never Enter).
    Escape,
}

/// The window's bottom: a line, then `action` (enabled when `enabled`;
/// `why_not` on hover when it is not) and **Cancel**, the `keys` the
/// window takes said on their hover.
pub fn buttons(
    ui: &mut egui::Ui,
    action: &str,
    enabled: bool,
    why_not: Option<&str>,
    keys: Keys,
) -> Option<Pressed> {
    let mut out = None;
    ui.separator();
    ui.horizontal(|ui| {
        let b = ui
            .add_enabled(
                enabled,
                egui::Button::new(egui::RichText::new(action).strong()),
            )
            .on_hover_text(if keys == Keys::Both {
                "Enter"
            } else {
                "Click to confirm"
            });
        let b = match why_not {
            Some(w) => b.on_disabled_hover_text(w),
            None => b,
        };
        if b.clicked() {
            out = Some(Pressed::Action);
        }
        if ui
            .button("Cancel")
            .on_hover_text("Escape: nothing changes")
            .clicked()
        {
            out = Some(Pressed::Cancel);
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_are_the_books() {
        assert_eq!(anchor("Move structure"), "move-structure");
        assert_eq!(anchor("Follow and snap"), "follow-and-snap");
        assert_eq!(
            anchor("Edit on a kept measurement, or a note"),
            "edit-on-a-kept-measurement-or-a-note"
        );
        assert!(has_heading("text\n## Add a joint\n", "add-a-joint"));
        assert!(!has_heading("Add a joint\n", "add-a-joint"));
    }
}
