// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The project line worked on and its uncommitted changes, as shown at
//! the top of the Versions menu and in the status bar.
//!
//! Counted with [`ohl_session::Session::status`] (the working state
//! compared with the line's last revision: its objects hashed, about 2 µs
//! each, those that differ read) — not every frame: only when the line
//! or the project changed ([`Model::line_version`],
//! [`Model::project_version`], the library files), and never while an
//! object is dragged. The texts are made then and kept.

use eframe::egui;

use crate::model::Model;

/// The line worked on and what is not committed on it, as last counted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineStatus {
    /// What it was counted for: the line's, the project's and the
    /// libraries' counters.
    key: Option<(u64, u64, u64)>,
    /// The project line worked on (empty: a line file, no project).
    pub line: String,
    /// Objects changed, added or removed since the line's last revision.
    pub objects: usize,
    /// Library files changed, added or removed since then.
    pub libraries: usize,
    /// The line has no revision yet (nothing was ever committed on it).
    pub no_revision: bool,
    /// The changes in words ("objects: 2 changed; library files: 1 added").
    summary: String,
    /// Why they could not be counted.
    error: Option<String>,
}

impl LineStatus {
    /// Counts again when the line or the project changed since the last
    /// count (else nothing is read).
    pub fn refresh(&mut self, model: &Model) {
        // A drag previews the line every frame without changing the
        // project: counted when it ends.
        if model.previewing && self.key.is_some() {
            return;
        }
        let key = (
            model.line_version,
            model.project_version,
            model.revisions.libraries,
        );
        if self.key == Some(key) {
            return;
        }
        *self = Self::count(model);
        self.key = Some(key);
    }

    fn count(model: &Model) -> Self {
        let Some(s) = model.session() else {
            return Self::default();
        };
        let mut out = Self {
            line: s.current_line().to_owned(),
            ..Self::default()
        };
        match s.status() {
            Ok(st) => {
                out.objects = st.changes.objects.len();
                out.libraries = st.changes.libraries.len();
                out.no_revision = st.revision.is_none();
                out.summary = st.changes.summary();
            }
            Err(e) => out.error = Some(e.to_string()),
        }
        out
    }

    /// Whether a project is open (a line is worked on).
    pub fn in_project(&self) -> bool {
        !self.line.is_empty()
    }

    /// Changes not committed: objects and library files.
    pub fn uncommitted(&self) -> usize {
        self.objects + self.libraries
    }

    /// "3 uncommitted changes", "no uncommitted changes".
    fn count_text(&self) -> String {
        match self.uncommitted() {
            0 => "no uncommitted changes".to_owned(),
            1 => "1 uncommitted change".to_owned(),
            n => format!("{n} uncommitted changes"),
        }
    }

    /// The first line of the Versions menu: "Working on: main · 3
    /// uncommitted changes".
    pub fn menu_text(&self) -> String {
        format!("Working on: {} · {}", self.line, self.count_text())
    }

    /// The status bar's text: the line, and a dot when it has uncommitted
    /// changes.
    pub fn chip_text(&self) -> String {
        if self.uncommitted() > 0 {
            format!("line: {} •", self.line)
        } else {
            format!("line: {}", self.line)
        }
    }

    /// The counts in words, for the hover texts.
    pub fn details(&self) -> String {
        if let Some(e) = &self.error {
            return format!(
                "Project line {}: its changes could not be counted ({e})",
                self.line
            );
        }
        let mut out = format!("Project line {}: {}", self.line, self.count_text());
        if self.uncommitted() > 0 {
            out.push_str(&format!(" since its last commit ({})", self.summary));
        }
        if self.no_revision {
            out.push_str("; nothing was committed on this line yet");
        }
        out
    }
}

/// The status bar's chip: the project line worked on, a dot when it has
/// uncommitted changes; the counts on hover. Returns whether it was
/// clicked (Commit is opened).
pub fn chip(ui: &mut egui::Ui, status: &LineStatus) -> bool {
    if !status.in_project() {
        return false;
    }
    ui.add(
        egui::Label::new(egui::RichText::new(status.chip_text()).small())
            .sense(egui::Sense::click()),
    )
    .on_hover_text(format!(
        "{}.\nYour edits go to this project line. Click: commit (Versions › Commit…).",
        status.details()
    ))
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .clicked()
}
