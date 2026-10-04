// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The File menu's work: a new project, opening a file (also from the
//! files opened recently), closing it; the start page when no file is
//! open.

use std::path::{Path, PathBuf};

use eframe::egui;

/// What the File menu asks for (done by the app after the frame).
#[derive(Debug, Clone, PartialEq)]
pub enum FileAction {
    /// The New project window.
    New,
    /// Open a file: this one, or one chosen in a file dialog.
    Open(Option<PathBuf>),
    /// Close the file open.
    Close,
    /// Quit the application.
    Quit,
}

/// Files opened, most recent first (remembered).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recent(pub Vec<PathBuf>);

impl Recent {
    /// How many are kept.
    pub const MAX: usize = 10;

    /// `path` opened: first in the list.
    pub fn add(&mut self, path: &Path) {
        let p = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.0.retain(|q| *q != p);
        self.0.insert(0, p);
        self.0.truncate(Self::MAX);
    }

    /// Saved one per line.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        let text: Vec<String> = self.0.iter().map(|p| p.display().to_string()).collect();
        storage.set_string("recent", text.join("\n"));
    }

    /// As saved (files no longer there are left out).
    pub fn load(storage: &dyn eframe::Storage) -> Self {
        Self(
            storage
                .get_string("recent")
                .unwrap_or_default()
                .lines()
                .map(PathBuf::from)
                .filter(|p| p.is_file())
                .collect(),
        )
    }
}

/// The file dialog for opening: projects and line files.
pub fn pick_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("OLS CAD project or line", &["ols", "json"])
        .add_filter("OLS CAD project", &["ols"])
        .add_filter("Line file", &["json"])
        .pick_file()
}

/// The File menu: New project, Open, Open recent, (the open file's
/// Save and Export, from `extra`), Close, Quit.
pub fn file_menu(
    ui: &mut egui::Ui,
    recent: &Recent,
    open: Option<&Path>,
    action: &mut Option<FileAction>,
    extra: impl FnOnce(&mut egui::Ui),
) {
    let cmd = |k: &str| {
        if cfg!(target_os = "macos") {
            format!("⌘{k}")
        } else {
            format!("Ctrl+{k}")
        }
    };
    ui.menu_button("File", |ui| {
        if let Some(p) = open {
            ui.label(egui::RichText::new(p.display().to_string()).small());
            ui.separator();
        }
        if ui
            .add(egui::Button::new("New project…").shortcut_text(cmd("N")))
            .on_hover_text("Create a project file (.ols) with an empty line and open it")
            .clicked()
        {
            *action = Some(FileAction::New);
            ui.close();
        }
        if ui
            .add(egui::Button::new("Open…").shortcut_text(cmd("O")))
            .on_hover_text("Open a project (.ols) or a line file (.ols.json)")
            .clicked()
        {
            *action = Some(FileAction::Open(None));
            ui.close();
        }
        ui.add_enabled_ui(!recent.0.is_empty(), |ui| {
            ui.menu_button("Open recent", |ui| {
                for p in &recent.0 {
                    let name = p
                        .file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                    if ui
                        .button(name)
                        .on_hover_text(p.display().to_string())
                        .clicked()
                    {
                        *action = Some(FileAction::Open(Some(p.clone())));
                        ui.close();
                    }
                }
            });
        });
        if open.is_some() {
            ui.separator();
            extra(ui);
            ui.separator();
            if ui
                .button("Close")
                .on_hover_text("Close the file (the project is written through first)")
                .clicked()
            {
                *action = Some(FileAction::Close);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Quit").clicked() {
            *action = Some(FileAction::Quit);
            ui.close();
        }
    });
}

/// ⌘N / Ctrl+N and ⌘O / Ctrl+O.
pub fn file_keys(ctx: &egui::Context, action: &mut Option<FileAction>) {
    use egui::{Key, KeyboardShortcut, Modifiers};
    ctx.input_mut(|i| {
        if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::N)) {
            *action = Some(FileAction::New);
        }
        if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::O)) {
            *action = Some(FileAction::Open(None));
        }
    });
}

/// The start page (no file open): new, open, the recent files.
pub fn start_page(ui: &mut egui::Ui, recent: &Recent, action: &mut Option<FileAction>) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.heading("OLS CAD");
        ui.label(egui::RichText::new(format!("version {}", env!("CARGO_PKG_VERSION"))).weak());
        ui.add_space(20.0);
        if ui.button("New project…").clicked() {
            *action = Some(FileAction::New);
        }
        if ui.button("Open…").clicked() {
            *action = Some(FileAction::Open(None));
        }
        if !recent.0.is_empty() {
            ui.add_space(20.0);
            ui.label(egui::RichText::new("Recent").strong());
            for p in &recent.0 {
                let name = p
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                if ui
                    .link(name)
                    .on_hover_text(p.display().to_string())
                    .clicked()
                {
                    *action = Some(FileAction::Open(Some(p.clone())));
                }
                ui.label(egui::RichText::new(p.display().to_string()).small().weak());
            }
        }
    });
}

/// The New project window's values.
#[derive(Debug, Clone, PartialEq)]
pub struct NewProject {
    /// Name of the line.
    pub name: String,
    /// Coordinate system, e.g. `EPSG:2056`.
    pub crs: String,
    /// Why the last attempt failed.
    pub error: Option<String>,
}

impl Default for NewProject {
    fn default() -> Self {
        Self {
            name: "New line".into(),
            crs: "EPSG:2056".into(),
            error: None,
        }
    }
}

impl NewProject {
    /// The window; `Some(path)` once the project file is created there.
    pub fn ui(&mut self, ctx: &egui::Context, open: &mut bool) -> Option<PathBuf> {
        let mut created = None;
        egui::Window::new("New project")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("new-project").show(ui, |ui| {
                    ui.label("Line name");
                    ui.text_edit_singleline(&mut self.name);
                    ui.end_row();
                    ui.label("Coordinate system");
                    ui.text_edit_singleline(&mut self.crs)
                        .on_hover_text("e.g. EPSG:2056 (Swiss LV95), EPSG:25832 (UTM 32N)");
                    ui.end_row();
                });
                ui.label(
                    egui::RichText::new(
                        "The project starts with an empty line; you are its admin. Its \
                         libraries are taken from the default folders and kept in it.",
                    )
                    .small()
                    .weak(),
                );
                if let Some(e) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
                if ui.button("Choose where to save it and create…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("OLS CAD project", &["ols"])
                        .set_file_name(format!("{}.ols", self.name.trim()))
                        .save_file()
                {
                    match create_project(&path, &self.name, &self.crs, &user()) {
                        Ok(p) => created = Some(p),
                        Err(e) => self.error = Some(e),
                    }
                }
            });
        created
    }
}

/// The login name (the project's first user, its admin).
pub fn user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".into())
}

/// Creates the project file `path` (`.ols`) holding an empty line `name`
/// in `crs`, `user` its admin; refuses to replace a file. Returns the
/// path with its extension.
pub fn create_project(path: &Path, name: &str, crs: &str, user: &str) -> Result<PathBuf, String> {
    let path = path.with_extension("ols");
    let name = name.trim();
    if name.is_empty() {
        return Err("give the line a name".into());
    }
    if path.exists() {
        return Err(format!(
            "{} exists already: choose another name (a project is never replaced)",
            path.display()
        ));
    }
    let line: ols_line::Line = serde_json::from_value(serde_json::json!({
        "name": name,
        "crs": crs.trim(),
        "structures": [],
        "sections": [],
    }))
    .map_err(|e| e.to_string())?;
    let mut session = ols_session::Session::open(&path).map_err(|e| e.to_string())?;
    session.import(&line, user).map_err(|e| e.to_string())?;
    session.checkpoint().map_err(|e| e.to_string())?;
    Ok(path)
}
