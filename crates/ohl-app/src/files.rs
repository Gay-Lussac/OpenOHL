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
    /// The New project window, the starter template chosen.
    NewStarter,
    /// A copy of the example line (`examples/synthetic-line`) in a folder
    /// chosen, opened.
    OpenExample,
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
            "OpenOHL project, line or structure type",
            &[
                ohl_core::files::PROJECT_EXTENSION,
                ohl_core::files::LEGACY_PROJECT_EXTENSION,
                "json",
                ohl_structures::structures::STRUCTURE_FILE_EXTENSION,
                "olss",
            ],
        )
        .add_filter(
            "OpenOHL project",
            &ohl_core::files::open_extensions(ohl_core::files::PROJECT_EXTENSION),
        )
        .add_filter("Line file", &["json"])
        .add_filter(
            "Structure type (opened on its own)",
            &ohl_core::files::open_extensions(ohl_structures::structures::STRUCTURE_FILE_EXTENSION),
        )
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
            .on_hover_text("Create a project file (.ohl) with an empty line and open it")
            .clicked()
        {
            *action = Some(FileAction::New);
            ui.close();
        }
        if ui
            .button("New project from the starter template…")
            .on_hover_text(Template::Starter.about())
            .clicked()
        {
            *action = Some(FileAction::NewStarter);
            ui.close();
        }
        if ui
            .add(egui::Button::new("Open…").shortcut_text(cmd("O")))
            .on_hover_text("Open a project (.ohl) or a line file (.ohl.json)")
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
            .button("Open the example (copy)…")
            .on_hover_text(EXAMPLE_TIP)
            .clicked()
        {
            *action = Some(FileAction::OpenExample);
            ui.close();
        }
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
                    let project = ohl_core::files::is_project_file(path);
                    if ui
                        .add_enabled(project, egui::Button::new("Host this project…"))
                        .on_hover_text(
                            "Let others work on this project with you: a server on this computer",
                        )
                        .on_disabled_hover_text("A project file (.ohl) can be hosted")
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
        // The OpenOHL mark above the name.
        ui.add(crate::icons::mark(ui.ctx(), 64.0));
        ui.heading(crate::APP_NAME);
        ui.label(egui::RichText::new(format!("version {}", env!("CARGO_PKG_VERSION"))).weak());
        ui.add_space(20.0);
        ui.label(egui::RichText::new("New to OpenOHL?").strong());
        if ui
            .button("New project from the starter template…")
            .on_hover_text(Template::Starter.about())
            .clicked()
        {
            *action = Some(FileAction::NewStarter);
        }
        if ui.button("Open the example (copy)…").on_hover_text(EXAMPLE_TIP).clicked() {
            *action = Some(FileAction::OpenExample);
        }
        ui.label(
            egui::RichText::new(
                "A tutorial from the starter template to a strung line (Help › Start from \
                 scratch) is planned, not written yet",
            )
            .small()
            .weak(),
        );
        ui.add_space(12.0);
        if ui
            .button("New project…")
            .on_hover_text("An empty project, or from the starter template")
            .clicked()
        {
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
                        ui.visuals().error_fg_color,
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
                    && let Some(options) = match ohl_io::parse_options(&self.options) {
                        Ok(o) => Some(o),
                        Err(e) => {
                            self.error = Some(e);
                            None
                        }
                    }
                    && let Some(into) = rfd::FileDialog::new()
                        .add_filter("OpenOHL project", &[ohl_core::files::PROJECT_EXTENSION])
                        .set_file_name(format!(
                            "{}.ohl",
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

/// What a new project starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Template {
    /// Nothing: an empty line and empty libraries.
    #[default]
    Empty,
    /// The starter template: example libraries for learning, no line yet
    /// ([`crate::starter`]).
    Starter,
}

impl Template {
    /// Name in the New project window.
    pub fn name(self) -> &'static str {
        match self {
            Template::Empty => "Empty project",
            Template::Starter => "Starter template (for learning)",
        }
    }

    /// What it holds, under its name.
    pub fn about(self) -> &'static str {
        match self {
            Template::Empty => {
                "An empty line and empty libraries: add cables, structure types and weather \
                 cases from the library pages (e.g. Sections › Wires library › Browse…)."
            }
            Template::Starter => {
                "Example libraries with invented values, clearly marked: cables, two \
                 structure types, pylon templates, insulators, weather and load cases, \
                 clearance rules, and survey points of invented ground beside the project. \
                 No route, alignment, structure or wire: you add them (a tutorial is planned)."
            }
        }
    }
}

/// The hover text of "Open the example (copy)".
pub const EXAMPLE_TIP: &str = "A copy of the example line (examples/synthetic-line: invented \
     coordinates and values) in a folder you choose, opened with its ground and weather cases: \
     change it freely, the example itself stays as it is";

/// The example line's folder in the source tree.
pub const EXAMPLE: &str = "examples/synthetic-line";

/// Copies the example line into a new folder in `into` (named after it,
/// numbered when taken) and returns what to open: the line file with its
/// survey points and weather cases.
pub fn copy_example(into: &Path) -> Result<crate::model::Sources, String> {
    let from = crate::help::shipped(EXAMPLE)
        .ok_or_else(|| format!("the example ({EXAMPLE}) is not on this computer"))?;
    let mut to = into.join("synthetic-line-example");
    let mut k = 2;
    while to.exists() {
        to = into.join(format!("synthetic-line-example-{k}"));
        k += 1;
    }
    copy_dir(&from, &to)?;
    Ok(crate::model::Sources {
        file: to.join("synthetic-line.ohl.json"),
        terrain: Some(to.join("synthetic-survey.csv")),
        weather: Some(to.join("weather-cases.ohlwc")),
        ..Default::default()
    })
}

/// Copies the folder `from` (its files and folders) to the new folder
/// `to`.
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let entries = std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?;
    for e in entries {
        let e = e.map_err(|e| e.to_string())?;
        let p = e.path();
        let target = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &target)?;
        } else {
            std::fs::copy(&p, &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    Ok(())
}

/// The New project window's values.
#[derive(Debug, Clone)]
pub struct NewProject {
    /// Name of the line.
    pub name: String,
    /// What the project starts with.
    pub template: Template,
    /// Local coordinates (plane, metres, no geographic reference) instead
    /// of a coordinate system of the list.
    pub local: bool,
    /// The coordinate system chosen (searchable EPSG list, proj4, PRJ).
    pub picker: crate::crs::Picker,
    /// Why the last attempt failed.
    pub error: Option<String>,
}

impl Default for NewProject {
    fn default() -> Self {
        Self {
            name: "New line".into(),
            template: Template::Empty,
            local: true,
            picker: crate::crs::Picker::new(""),
            error: None,
        }
    }
}

/// The coordinate system of a project in local coordinates.
pub const LOCAL_CRS: &str = "local";

/// Width of the New project window's fields, points.
const FIELD_WIDTH: f32 = 260.0;

impl NewProject {
    /// The window for a project from the starter template.
    pub fn starter() -> Self {
        Self {
            name: "My first line".into(),
            template: Template::Starter,
            ..Self::default()
        }
    }

    /// The coordinate system chosen, as the line keeps it, or why none
    /// is.
    pub fn crs(&self) -> Result<String, String> {
        if self.local {
            return Ok(LOCAL_CRS.into());
        }
        self.picker.crs().map(|c| c.name().to_owned())
    }

    /// The window; `Some(path)` once the project file is created there.
    pub fn ui(&mut self, ctx: &egui::Context, open: &mut bool) -> Option<PathBuf> {
        let mut created = None;
        egui::Window::new("New project")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Start from").strong());
                for t in [Template::Empty, Template::Starter] {
                    ui.radio_value(&mut self.template, t, t.name());
                    ui.indent(t.name(), |ui| {
                        ui.label(egui::RichText::new(t.about()).small().weak());
                    });
                }
                if self.template == Template::Starter && crate::starter::dir().is_none() {
                    crate::menu::error_label(
                        ui,
                        "The starter template (examples/starter) is not on this computer.",
                    );
                }
                ui.separator();
                egui::Grid::new("new-project")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Line name");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.name).desired_width(FIELD_WIDTH),
                        );
                        ui.end_row();
                        ui.label("Coordinate system");
                        ui.vertical(|ui| {
                            ui.radio_value(
                                &mut self.local,
                                true,
                                "Local coordinates (metres, no geographic reference)",
                            )
                            .on_hover_text(
                                "Plane x, y, z in metres: for learning and for lines not tied to \
                                 a map (set it later: Terrain › Coordinate system › Define)",
                            );
                            ui.radio_value(&mut self.local, false, "A coordinate system…")
                                .on_hover_text(
                                    "From the EPSG list (search by code or name, e.g. LV95, \
                                     UTM 32N, 2056), a proj4 string or a .prj file",
                                );
                        });
                        ui.end_row();
                        ui.label("Units");
                        ui.label("SI (m, N, °C)");
                        ui.end_row();
                    });
                if !self.local {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        self.picker.ui(ui, "new-project-crs");
                    });
                }
                match self.crs() {
                    Ok(c) if c == LOCAL_CRS => {
                        ui.label(egui::RichText::new("Coordinates: local, in metres").small())
                    }
                    Ok(c) => ui.label(
                        egui::RichText::new(format!("Coordinates: {}", ohl_crs::describe(&c)))
                            .small(),
                    ),
                    Err(e) => crate::menu::error_label(ui, e),
                };
                ui.label(
                    egui::RichText::new(
                        "The project file (.ohl) keeps the line and its libraries; you are its \
                         admin.",
                    )
                    .small()
                    .weak(),
                );
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
                }
                let crs = self.crs();
                if ui
                    .add_enabled(
                        crs.is_ok(),
                        egui::Button::new("Choose where to save it and create…"),
                    )
                    .clicked()
                    && let Ok(crs) = crs
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("OpenOHL project", &[ohl_core::files::PROJECT_EXTENSION])
                        .set_file_name(format!("{}.ohl", self.name.trim()))
                        .save_file()
                {
                    match create_project_from(&path, &self.name, &crs, &user(), self.template) {
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

/// Creates the project file `path` (`.ohl`) holding an empty line `name`
/// in `crs`, `user` its admin; refuses to replace a file. Returns the
/// path with its extension. (The window makes it with
/// [`create_project_from`].)
#[cfg(test)]
pub fn create_project(path: &Path, name: &str, crs: &str, user: &str) -> Result<PathBuf, String> {
    create_project_from(path, name, crs, user, Template::Empty)
}

/// Creates the project file `path` (`.ohl`) holding an empty line `name`
/// in `crs`, `user` its admin, with what the project starts with: the
/// starter template's libraries kept in it (and its pylon templates and
/// survey points beside it), or nothing; refuses to replace a file.
/// Returns the path with its extension.
pub fn create_project_from(
    path: &Path,
    name: &str,
    crs: &str,
    user: &str,
    template: Template,
) -> Result<PathBuf, String> {
    let path = path.with_extension(ohl_core::files::PROJECT_EXTENSION);
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
    let starter =
        match template {
            Template::Empty => None,
            Template::Starter => Some(crate::starter::dir().ok_or(
                "the starter template (examples/starter) is not on this computer".to_owned(),
            )?),
        };
    let line: ohl_line::Line = serde_json::from_value(serde_json::json!({
        "name": name,
        "crs": crs.trim(),
        "structures": [],
        "sections": [],
    }))
    .map_err(|e| e.to_string())?;
    let mut session = ohl_session::Session::open(&path).map_err(|e| e.to_string())?;
    session.import(&line, user).map_err(|e| e.to_string())?;
    // Libraries of its own: none copied in from around it.
    session
        .set_setting(crate::model::OWN_LIBRARIES_SETTING, Some("1"))
        .map_err(|e| e.to_string())?;
    if let Some(dir) = starter {
        crate::starter::fill(&mut session, &path, user, &dir)?;
    }
    session.checkpoint().map_err(|e| e.to_string())?;
    Ok(path)
}
