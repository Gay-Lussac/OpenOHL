// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The windows the tools open, OHL-Design's side: the manual's section
//! of each tool ([`guide`]) and the window's top opening it
//! ([`intro_of`]); the parts both applications share are
//! [`ohl_ui::tool_window`] (owner, 2026-11-14; the rules:
//! docs/development/tool-pages.md).

use eframe::egui;

pub use ohl_ui::tool_window::{Keys, Pressed, anchor, buttons, heading, label};

use crate::tools::Tool;

/// The manual's page of `tool` (by its theme, e.g.
/// `user-guide/tools-structures`) and its section there (its name as
/// the book makes a heading's anchor, e.g. `move-structure`).
pub fn guide(tool: Tool) -> (String, String) {
    let page = format!("user-guide/tools-{}", tool.theme().to_lowercase());
    (page, anchor(tool.name()))
}

/// The window's top ([`ohl_ui::tool_window::intro`]): `what` it does,
/// `more` behind a (?), User guide ↗ opening the manual at `guide`
/// (page, anchor).
pub fn intro(ui: &mut egui::Ui, what: &str, more: &str, guide: (&str, &str)) {
    let key = format!("{}#{}", guide.0, guide.1);
    ohl_ui::tool_window::intro(ui, what, more, &key, || {
        crate::help::open_section(guide.0, guide.1).map(|_| ())
    });
}

/// [`intro`] with the manual's section of `tool`.
pub fn intro_of(ui: &mut egui::Ui, what: &str, more: &str, tool: Tool) {
    let (page, anchor) = guide(tool);
    intro(ui, what, more, (&page, &anchor));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The anchors are the book's for the tools' headings, and every tool
    /// has a page of its theme.
    #[test]
    fn every_tool_has_its_section() {
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
                ohl_ui::tool_window::has_heading(&text, a),
                "{page}: no heading for {a}"
            );
        }
    }
}
