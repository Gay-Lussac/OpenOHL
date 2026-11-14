// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the windows the tools open share (owner, 2026-11-14: "user
//! friendlier for humans that don't know the program … functionality
//! stays the same"): at the top one plain sentence of what the window
//! does, its longer text behind a **(?)** and a **User guide ↗** button
//! opening the tool's page of the manual ([`intro`]); labels whose help
//! was only on hover with a visible (?) ([`label`]); one button row at the
//! bottom, the action first, Cancel last ([`buttons`]). The rule is
//! written in docs/development/ui.md, "Tool windows".

use eframe::egui;

use crate::tools::Tool;

/// The manual's page of `tool` (by its theme, e.g.
/// `user-guide/tools-structures`) and its section there (its name as
/// the book makes a heading's anchor, e.g. `move-structure`).
pub fn guide(tool: Tool) -> (String, String) {
    let page = format!("user-guide/tools-{}", tool.theme().to_lowercase());
    (page, anchor(tool.name()))
}

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

/// The window's top: `what` it does in one sentence, `more` behind a
/// **(?)**, a **User guide ↗** button opening the manual at `guide`
/// (page, anchor). Why the page did not open is shown below it.
pub fn intro(ui: &mut egui::Ui, what: &str, more: &str, guide: (&str, &str)) {
    let id = ui.id().with(("tool-guide-error", guide.0, guide.1));
    ui.add(egui::Label::new(what).wrap());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if !more.is_empty() {
            crate::check_help::question(ui, more);
            ui.label(egui::RichText::new("more").small().weak())
                .on_hover_text(more);
        }
        if ui
            .small_button("User guide ↗")
            .on_hover_text("This tool's page of the manual: what it does, step by step")
            .clicked()
        {
            let error = crate::help::open_section(guide.0, guide.1).err();
            ui.data_mut(|d| d.insert_temp(id, error));
        }
    });
    if let Some(Some(e)) = ui.data(|d| d.get_temp::<Option<String>>(id)) {
        crate::menu::error_label(ui, egui::RichText::new(e).small());
    }
    ui.add_space(4.0);
}

/// [`intro`] with the manual's section of `tool`.
pub fn intro_of(ui: &mut egui::Ui, what: &str, more: &str, tool: Tool) {
    let (page, anchor) = guide(tool);
    intro(ui, what, more, (&page, &anchor));
}

/// A field's name with its help behind a visible **(?)** (the help on
/// hover of the name too).
pub fn label(ui: &mut egui::Ui, text: &str, help: &str) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let r = ui.label(text).on_hover_text(help);
        crate::check_help::question(ui, help);
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
        crate::check_help::question(ui, help);
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
    /// Neither: the buttons are clicked.
    None,
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
            .on_hover_text(if keys == Keys::None {
                "Nothing changes"
            } else {
                "Escape: nothing changes"
            })
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

    /// The anchors are the book's for the tools' headings, and every tool
    /// has a page of its theme.
    #[test]
    fn every_tool_has_its_section() {
        assert_eq!(anchor("Move structure"), "move-structure");
        assert_eq!(anchor("Follow and snap"), "follow-and-snap");
        let (page, a) = guide(Tool::MeasureHeight);
        assert_eq!(
            (page.as_str(), a.as_str()),
            ("user-guide/tools-measure", "height-above-ground")
        );
        // The sections the other windows open (their buttons name them).
        let fixed = [
            ("user-guide/tools", "the-inventory"),
            ("user-guide/tools-measure", "the-measurements-panel"),
            (
                "user-guide/tools-general",
                "edit-on-a-kept-measurement-or-a-note",
            ),
            ("user-guide/desktop-app", "cross-section-view"),
        ];
        let tools = Tool::ALL.map(guide);
        let all = tools
            .iter()
            .map(|(p, a)| (p.as_str(), a.as_str()))
            .chain(fixed);
        for (page, a) in all {
            let file = crate::tests::root().join(format!("docs/{page}.md"));
            let text = std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            assert!(
                text.lines()
                    .any(|l| l.starts_with('#') && anchor(l.trim_start_matches('#').trim()) == a),
                "{page}: no heading for {a}"
            );
        }
    }
}
