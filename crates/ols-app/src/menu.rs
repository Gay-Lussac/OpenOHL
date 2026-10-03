// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The menu bar and the quick bar under it. Menus hold everything that
//! needs no view (pages, checks, libraries, tables, plugins, the snap
//! choice); the quick bar holds the commands the user puts there
//! (customizable, remembered).

use eframe::egui;

use crate::library::LibraryKind;
use crate::tools::Tool;

/// A command of the menus and the quick bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    /// Undo the user's last edit.
    Undo,
    /// Redo.
    Redo,
    /// Fit the views to the line.
    FitViews,
    /// Clear the selection.
    ClearSelection,
    /// Delete the selected object.
    DeleteSelected,
    /// Open the Go to structure window.
    GoTo,
    /// Open the tool inventory.
    Inventory,
    /// Run the checks (clearances) now.
    RunChecks,
    /// Choose the weather cases shown.
    DisplayCases,
    /// Open a library editor.
    Library(LibraryKind),
    /// Open a table.
    Table(crate::tables::TableKind),
    /// Hold a tool.
    Hold(Tool),
}

impl Command {
    /// Every command that can go on the quick bar.
    pub fn catalog() -> Vec<Command> {
        let mut out = vec![
            Command::Undo,
            Command::Redo,
            Command::FitViews,
            Command::ClearSelection,
            Command::DeleteSelected,
            Command::GoTo,
            Command::Inventory,
            Command::RunChecks,
            Command::DisplayCases,
        ];
        out.extend(LibraryKind::ALL.map(Command::Library));
        out.extend(crate::tables::TableKind::ALL.map(Command::Table));
        out.extend(Tool::ALL.map(Command::Hold));
        out
    }

    /// The quick bar to start with.
    pub fn default_bar() -> Vec<Command> {
        vec![
            Command::Undo,
            Command::Redo,
            Command::FitViews,
            Command::RunChecks,
        ]
    }

    /// Name on a button and in the menus.
    pub fn name(self) -> String {
        match self {
            Command::Undo => "Undo".into(),
            Command::Redo => "Redo".into(),
            Command::FitViews => "Fit views".into(),
            Command::ClearSelection => "Clear selection".into(),
            Command::DeleteSelected => "Delete selected".into(),
            Command::GoTo => "Go to structure…".into(),
            Command::Inventory => "Tool inventory".into(),
            Command::RunChecks => "Run checks".into(),
            Command::DisplayCases => "Display weather cases…".into(),
            Command::Library(k) => format!("{} library", k.title()),
            Command::Table(k) => format!("{} table", k.title()),
            Command::Hold(t) => format!("{} {}", t.icon(), t.name()),
        }
    }

    /// Its shortcut, if any.
    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            Command::Undo => Some("⌘Z"),
            Command::Redo => Some("Shift+⌘Z"),
            Command::DeleteSelected => Some("Delete"),
            Command::GoTo => Some("G"),
            Command::Inventory => Some("E"),
            Command::Hold(t) => t.key(),
            _ => None,
        }
    }

    /// Key for saving.
    pub fn key(self) -> String {
        match self {
            Command::Undo => "undo".into(),
            Command::Redo => "redo".into(),
            Command::FitViews => "fit".into(),
            Command::ClearSelection => "clear".into(),
            Command::DeleteSelected => "delete".into(),
            Command::GoTo => "goto".into(),
            Command::Inventory => "inventory".into(),
            Command::RunChecks => "checks".into(),
            Command::DisplayCases => "display-cases".into(),
            Command::Library(k) => format!("library-{}", k.extension()),
            Command::Table(k) => format!("table-{}", k.key()),
            Command::Hold(t) => format!("tool-{}", t.key_name()),
        }
    }

    fn from_key(key: &str) -> Option<Command> {
        Command::catalog().into_iter().find(|c| c.key() == key)
    }

    /// A menu entry (with its shortcut); true when chosen.
    pub fn menu_item(self, ui: &mut egui::Ui) -> bool {
        let mut b = egui::Button::new(self.name());
        if let Some(s) = self.shortcut() {
            b = b.shortcut_text(s);
        }
        ui.add(b).clicked()
    }
}

/// The quick bar's commands, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickBar(pub Vec<Command>);

impl Default for QuickBar {
    fn default() -> Self {
        Self(Command::default_bar())
    }
}

impl QuickBar {
    /// Saved as a comma list of keys.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        let keys: Vec<String> = self.0.iter().map(|c| c.key()).collect();
        storage.set_string("quickbar", keys.join(","));
    }

    /// As saved, if saved.
    pub fn load(storage: &dyn eframe::Storage) -> Option<QuickBar> {
        let text = storage.get_string("quickbar")?;
        Some(QuickBar(
            text.split(',').filter_map(Command::from_key).collect(),
        ))
    }

    /// The bar's buttons; the command clicked.
    pub fn ui(&self, ui: &mut egui::Ui) -> Option<Command> {
        let mut out = None;
        for c in &self.0 {
            let r = ui.button(c.name());
            let r = match c.shortcut() {
                Some(s) => r.on_hover_text(s),
                None => r,
            };
            if r.clicked() {
                out = Some(*c);
            }
        }
        out
    }

    /// The "Customize the quick bar" window's content: tick a command to
    /// put it on the bar, arrows to order it.
    pub fn customize_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("On the quick bar, in this order:");
        let mut swap: Option<(usize, usize)> = None;
        let mut remove: Option<usize> = None;
        let n = self.0.len();
        egui::Grid::new("quickbar-order")
            .striped(true)
            .show(ui, |ui| {
                for (k, c) in self.0.iter().enumerate() {
                    ui.label(c.name());
                    if ui
                        .add_enabled(k > 0, egui::Button::new("▲").small())
                        .clicked()
                    {
                        swap = Some((k, k - 1));
                    }
                    if ui
                        .add_enabled(k + 1 < n, egui::Button::new("▼").small())
                        .clicked()
                    {
                        swap = Some((k, k + 1));
                    }
                    if ui.small_button("🗑").clicked() {
                        remove = Some(k);
                    }
                    ui.end_row();
                }
            });
        if let Some((a, b)) = swap {
            self.0.swap(a, b);
        }
        if let Some(k) = remove {
            self.0.remove(k);
        }
        ui.separator();
        ui.label("Add:");
        ui.horizontal_wrapped(|ui| {
            for c in Command::catalog() {
                if !self.0.contains(&c) && ui.button(c.name()).clicked() {
                    self.0.push(c);
                }
            }
        });
        ui.separator();
        if ui.button("Default").clicked() {
            *self = QuickBar::default();
        }
    }
}
