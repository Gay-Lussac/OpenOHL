// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The menu bar and the quick bar under it. Menus hold everything that
//! needs no view (pages, checks, libraries, tables, plugins, the snap
//! choice); the quick bar holds the commands the user puts there
//! (customizable, remembered).

use eframe::egui;

use crate::library::LibraryKind;
use crate::tools::Tool;

/// A menu (or a submenu) of options to tick, e.g. display options: ticking
/// a box never closes it — several are often ticked in a row — only a
/// click outside it or a command (which closes it itself) does. Every menu
/// of display options is made with it.
pub fn options_menu<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) {
    use egui::containers::menu::{MenuButton, MenuConfig, SubMenuButton, is_in_menu};
    let config = MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    if is_in_menu(ui) {
        SubMenuButton::new(title).config(config).ui(ui, add);
    } else {
        MenuButton::new(title).config(config).ui(ui, add);
    }
}

/// Longest text shown whole in a window (characters, lines); longer text
/// scrolls in a box of [`TEXT_BOX_HEIGHT`].
const LONG_TEXT: (usize, usize) = (300, 4);

/// Height of the box long messages scroll in, points.
pub const TEXT_BOX_HEIGHT: f32 = 160.0;

/// A message in a window (`color`, or the usual text colour): long ones
/// (lists of errors, reports) scroll in a box of fixed height, so a window
/// never grows beyond the screen and its buttons stay in reach. Every
/// error and report in a window is shown with it ([`error_label`]).
pub fn message(
    ui: &mut egui::Ui,
    text: impl Into<egui::RichText>,
    color: Option<egui::Color32>,
) -> egui::Response {
    let text: egui::RichText = text.into();
    let show = |ui: &mut egui::Ui, text: egui::RichText| match color {
        Some(c) => ui.colored_label(c, text),
        None => ui.label(text),
    };
    let raw = text.text();
    if raw.len() <= LONG_TEXT.0 && raw.lines().count() <= LONG_TEXT.1 {
        return show(ui, text);
    }
    let id = egui::Id::new(("message", raw.len(), raw.lines().next().unwrap_or("")));
    egui::Frame::group(ui.style())
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(TEXT_BOX_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| show(ui, text))
                .inner
        })
        .inner
}

/// An error in a window, in the theme's error colour; long ones scroll
/// ([`message`]).
pub fn error_label(ui: &mut egui::Ui, text: impl Into<egui::RichText>) -> egui::Response {
    let color = ui.visuals().error_fg_color;
    message(ui, text, Some(color))
}

/// The colour of a good result (nothing found, a check passed): a green
/// readable on the dark and on the light theme (errors and warnings take
/// the theme's own colours, `error_fg_color` and `warn_fg_color`).
pub fn ok_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::LIGHT_GREEN
    } else {
        egui::Color32::from_rgb(0, 120, 40)
    }
}

/// Asks for page `tab` to be shown (from a window or a page that has no
/// hold of the dock): the application opens it after the frame.
pub fn request_page(ctx: &egui::Context, tab: crate::app::Tab) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("request-page"), Some(tab)));
}

/// The page asked for with [`request_page`], if any (taken).
pub fn take_page_request(ctx: &egui::Context) -> Option<crate::app::Tab> {
    ctx.data_mut(|d| {
        d.remove_temp::<Option<crate::app::Tab>>(egui::Id::new("request-page"))
            .flatten()
    })
}

/// A file as the user knows it: a library file of the project by its
/// name, "(in the project)" — not the working copy's temporary path —,
/// any other by its path.
pub fn file_label(model: &crate::model::Model, path: &std::path::Path) -> String {
    let name = path
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    match &model.library_dir {
        Some(w) if path.starts_with(w) => format!("{name} (in the project)"),
        _ => path.display().to_string(),
    }
}

/// A command of the menus and the quick bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    /// Save: the project file, library pages with changes.
    Save,
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
    /// Open New pylon from a template (the pylon builder): Tools menu,
    /// Structures group. A window building a structure type, not a tool
    /// held (no click in a view).
    NewPylon,
}

impl Command {
    /// Every command that can go on the quick bar.
    pub fn catalog() -> Vec<Command> {
        let mut out = vec![
            Command::Save,
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
        out.push(Command::NewPylon);
        out
    }

    /// The quick bar to start with.
    pub fn default_bar() -> Vec<Command> {
        // Undo, Redo and Save first, as in OHL-Structure.
        vec![
            Command::Undo,
            Command::Redo,
            Command::Save,
            Command::FitViews,
            Command::RunChecks,
        ]
    }

    /// Name on a button and in the menus.
    pub fn name(self) -> String {
        match self {
            Command::Save => "Save all".into(),
            Command::Undo => "Undo".into(),
            Command::Redo => "Redo".into(),
            Command::FitViews => "Fit views".into(),
            Command::ClearSelection => "Clear selection".into(),
            Command::DeleteSelected => "Delete selected".into(),
            Command::GoTo => "Go to structure…".into(),
            Command::Inventory => "Tool inventory".into(),
            Command::RunChecks => "Check wire clearances".into(),
            Command::DisplayCases => "Display weather cases…".into(),
            Command::Library(k) => k.library_name(),
            Command::Table(k) => format!("{} table", k.title()),
            Command::Hold(t) => t.name().into(),
            Command::NewPylon => "New pylon from a template…".into(),
        }
    }

    /// What it does, in the quick bar's hover text (none: its name says).
    pub fn tip(self) -> Option<&'static str> {
        match self {
            Command::RunChecks => Some(
                "The rules of the clearance rule file (Checks › Wire clearances) in the \
                 weather case shown; results on the Clearances page. The ground and LiDAR \
                 points: Checks › Ground clearances",
            ),
            Command::FitViews => Some("Show the whole line in the 3D view and the plan"),
            Command::NewPylon => Some(
                "A structure type of level 1 from a pylon template: the face's dimensions, \
                 suspension or strain",
            ),
            Command::Save => {
                Some("The project written through, the library pages with changes saved")
            }
            _ => None,
        }
    }

    /// Its shortcut, if any.
    pub fn shortcut(self) -> Option<String> {
        // ⌘ on a Mac, Ctrl elsewhere (egui's command modifier).
        let cmd = |k: &str| {
            if cfg!(target_os = "macos") {
                format!("⌘{k}")
            } else {
                format!("Ctrl+{k}")
            }
        };
        match self {
            Command::Save => Some(cmd("S")),
            Command::Undo => Some(cmd("Z")),
            Command::Redo => Some(format!("Shift+{}", cmd("Z"))),
            Command::DeleteSelected => Some("Delete".into()),
            Command::GoTo => Some("G".into()),
            Command::Inventory => Some("E".into()),
            Command::Hold(t) => t.key().map(str::to_owned),
            _ => None,
        }
    }

    /// Key for saving.
    pub fn key(self) -> String {
        match self {
            Command::Save => "save".into(),
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
            Command::NewPylon => "new-pylon".into(),
        }
    }

    fn from_key(key: &str) -> Option<Command> {
        Command::catalog().into_iter().find(|c| c.key() == key)
    }

    /// Its button: a tool's icon (in the text colour) before its name.
    pub fn button(self, ui: &egui::Ui) -> egui::Button<'static> {
        match self {
            Command::Hold(t) => egui::Button::new((t.image(ui.ctx(), 16.0), self.name()))
                .image_tint_follows_text_color(true),
            _ => egui::Button::new(self.name()),
        }
    }

    /// A menu entry (with its shortcut); true when chosen.
    pub fn menu_item(self, ui: &mut egui::Ui) -> bool {
        let mut b = self.button(ui);
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
            let r = ui.add(c.button(ui));
            let r = match (c.tip(), c.shortcut()) {
                (Some(t), Some(s)) => r.on_hover_text(format!("{t} ({s})")),
                (Some(t), None) => r.on_hover_text(t),
                (None, Some(s)) => r.on_hover_text(s),
                (None, None) => r,
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
                    ui.horizontal(|ui| {
                        if let Command::Hold(t) = c {
                            ui.add(
                                t.image(ui.ctx(), 16.0)
                                    .tint(crate::icons::text_tint(ui.visuals())),
                            );
                        }
                        ui.label(c.name());
                    });
                    if ui
                        .add_enabled(k > 0, egui::Button::new("⏶").small())
                        .clicked()
                    {
                        swap = Some((k, k - 1));
                    }
                    if ui
                        .add_enabled(k + 1 < n, egui::Button::new("⏷").small())
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
                if !self.0.contains(&c) && ui.add(c.button(ui)).clicked() {
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
