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
    /// The Join a server window.
    Join,
    /// The Host this project window.
    Host,
    /// Leave the server (stop hosting).
    Leave,
    /// A new project from another program's file.
    ImportProject,
    /// The plugin manager (also before a project is open).
    Plugins,
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
        .add_filter(
            "OLS CAD project, line or structure type",
            &["ols", "json", "olss"],
        )
        .add_filter("OLS CAD project", &["ols"])
        .add_filter("Line file", &["json"])
        .add_filter("Structure type (opened on its own)", &["olss"])
        .pick_file()
}

/// How the file open is shared (server mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shared {
    /// Not: a file here (a project can be hosted).
    No,
    /// Joined on a server.
    Joined,
    /// Hosted by this application.
    Hosting,
}

/// The File menu: New project, Open, Open recent, Join a server, (the
/// open file's Save and Export, from `extra`), Host this project / Leave,
/// Close, Quit.
pub fn file_menu(
    ui: &mut egui::Ui,
    recent: &Recent,
    open: Option<&Path>,
    shared: Shared,
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
        if ui
            .button("New project from another program…")
            .on_hover_text(
                "Read another program's file (a project plugin's format) into a new project: \
                 the line, its wires, weather cases, load cases",
            )
            .clicked()
        {
            *action = Some(FileAction::ImportProject);
            ui.close();
        }
        if ui
            .button("Join a server…")
            .on_hover_text("Work on a project someone hosts, with their invitation")
            .clicked()
        {
            *action = Some(FileAction::Join);
            ui.close();
        }
        if let Some(path) = open {
            ui.separator();
            extra(ui);
            ui.separator();
            match shared {
                Shared::No => {
                    let project = path.extension().is_some_and(|e| e == "ols");
                    if ui
                        .add_enabled(project, egui::Button::new("Host this project…"))
                        .on_hover_text(
                            "Let others work on this project with you: a server on this computer",
                        )
                        .on_disabled_hover_text("A project file (.ols) can be hosted")
                        .clicked()
                    {
                        *action = Some(FileAction::Host);
                        ui.close();
                    }
                }
                Shared::Joined | Shared::Hosting => {
                    let label = if shared == Shared::Hosting {
                        "Stop hosting"
                    } else {
                        "Leave the server"
                    };
                    if ui.button(label).clicked() {
                        *action = Some(FileAction::Leave);
                        ui.close();
                    }
                }
            }
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
        if ui.button("New project from another program…").clicked() {
            *action = Some(FileAction::ImportProject);
        }
        if ui
            .button("Plugins…")
            .on_hover_text("The plugin manager: install, update, turn on or off the plugins (project formats among them)")
            .clicked()
        {
            *action = Some(FileAction::Plugins);
        }
        if ui.button("Join a server…").clicked() {
            *action = Some(FileAction::Join);
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

/// File › New project from another program.
#[derive(Debug, Clone, Default)]
pub struct ImportProjectWindow {
    /// The file to read.
    pub file: Option<PathBuf>,
    /// Format id (empty: from the extension).
    pub format: String,
    /// Options, `KEY=VALUE` separated by commas.
    pub options: String,
    /// Clip the sections in at their stringing condition.
    pub clip_in: bool,
    /// Why the last attempt failed.
    pub error: Option<String>,
}

impl ImportProjectWindow {
    /// The window; a job to run once the file and the new project are
    /// chosen.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        open: &mut bool,
        formats: &[crate::plugins::FormatInfo],
        busy: bool,
    ) -> Option<crate::plugins::ProjectImport> {
        let mut job = None;
        egui::Window::new("New project from another program")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                if formats.is_empty() {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        "No project format: install a project plugin (Plugins › Plugin manager).",
                    );
                }
                for f in formats {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} (.{}) — {} — {}",
                            f.id, f.extension, f.description, f.origin
                        ))
                        .small(),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Choose the file…").clicked()
                        && let Some(p) = rfd::FileDialog::new().pick_file()
                    {
                        self.file = Some(p);
                    }
                    ui.label(
                        self.file
                            .as_ref()
                            .map_or("no file".into(), |p| p.display().to_string()),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("format");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.format)
                            .hint_text("from the extension")
                            .desired_width(140.0),
                    );
                    ui.label("options");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.options)
                            .hint_text("KEY=VALUE, …")
                            .desired_width(200.0),
                    );
                });
                ui.checkbox(
                    &mut self.clip_in,
                    "Clip the sections in at their stringing condition",
                )
                .on_hover_text(
                    "Each span keeps the length of its condition (sections the reader left in \
                         the blocks)",
                );
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
                }
                if busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("reading…");
                    });
                } else if ui
                    .add_enabled(
                        self.file.is_some(),
                        egui::Button::new("Create the project…"),
                    )
                    .clicked()
                    && let Some(file) = &self.file
                    && let Some(options) = match ols_io::parse_options(&self.options) {
                        Ok(o) => Some(o),
                        Err(e) => {
                            self.error = Some(e);
                            None
                        }
                    }
                    && let Some(into) = rfd::FileDialog::new()
                        .add_filter("OLS CAD project", &["ols"])
                        .set_file_name(format!(
                            "{}.ols",
                            file.file_stem()
                                .map_or("project".into(), |s| s.to_string_lossy())
                        ))
                        .save_file()
                {
                    job = Some(crate::plugins::ProjectImport {
                        path: file.clone(),
                        format: (!self.format.trim().is_empty()).then(|| self.format.trim().into()),
                        options,
                        into,
                        user: user(),
                        clip_in: self.clip_in,
                    });
                }
            });
        job
    }
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
                    crate::menu::error_label(ui, e);
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
