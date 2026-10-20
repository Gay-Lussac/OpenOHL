// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plugin manager (Plugins › Plugin manager): the plugins installed
//! for the user and those of the folders added, each with what it
//! provides, why it cannot be used, and a tick to turn it off; install
//! from a folder, remove. File-format plugins give File › Import into the
//! line and File › Export their formats, next to the built-in ones; the
//! formats run on a worker thread (a plugin may take a while; its own
//! exit clauses, ADR-0003, stop it).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui;
use ohl_plugins::{Found, Tier};

/// A plugin listed: where it was found and what can be done with it.
#[derive(Debug, Clone)]
pub struct Listed {
    /// The plugin directory and its manifest.
    pub found: Found,
    /// In the user's plugin directory (it can be removed).
    pub installed: bool,
    /// Another plugin of this name was found first (this one is not used).
    pub duplicate: bool,
}

impl Listed {
    /// Why it cannot be used, if it cannot.
    pub fn problem(&self) -> Option<String> {
        if self.duplicate {
            return Some(format!(
                "another plugin named {} is used (found first)",
                self.found.name()
            ));
        }
        self.found.problem()
    }

    /// A format plugin (WebAssembly, extension point `io` or `terrain`).
    pub fn is_io(&self) -> bool {
        self.found.manifest.as_ref().is_ok_and(|m| {
            m.plugin.tier == Tier::Wasm
                && m.plugin
                    .extension_points
                    .iter()
                    .any(|p| p == "io" || p == "terrain" || p == "project")
        })
    }
}

/// A format of the registry, as listed in the windows.
#[derive(Debug, Clone, PartialEq)]
pub struct FormatInfo {
    /// Id.
    pub id: String,
    /// File extension, without the dot.
    pub extension: String,
    /// One line.
    pub description: String,
    /// "built-in" or "plugin NAME".
    pub origin: String,
}

/// The formats available (built-in and from the plugins turned on).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Formats {
    /// Exporters.
    pub exporters: Vec<FormatInfo>,
    /// Importers.
    pub importers: Vec<FormatInfo>,
    /// Terrain surface readers.
    pub surfaces: Vec<FormatInfo>,
    /// Project readers.
    pub projects: Vec<FormatInfo>,
    /// Plugins whose formats could not be loaded, with why.
    pub errors: Vec<String>,
}

enum Job {
    /// Load the built-in formats and those of these plugin directories.
    Reload(Vec<PathBuf>),
    Export {
        format: String,
        model: Box<ohl_io::ExportModel>,
        options: ohl_io::Options,
        path: PathBuf,
    },
    Import {
        format: String,
        path: PathBuf,
        base: Box<ohl_line::Line>,
        options: ohl_io::Options,
    },
    StoreSurfaces(SurfaceImport),
    CreateProject(ProjectImport),
}

/// Another program's file made a new project.
#[derive(Debug, Clone)]
pub struct ProjectImport {
    /// The file.
    pub path: PathBuf,
    /// Format id (`None`: from the file extension).
    pub format: Option<String>,
    /// Format options.
    pub options: ohl_io::Options,
    /// The new project file (must not exist).
    pub into: PathBuf,
    /// The user (its admin).
    pub user: String,
    /// Clip the sections in at their stringing condition.
    pub clip_in: bool,
}

/// A terrain surface file to read and keep in a project.
#[derive(Debug, Clone)]
pub struct SurfaceImport {
    /// The file.
    pub path: PathBuf,
    /// Format id (`None`: from the file extension).
    pub format: Option<String>,
    /// Format options.
    pub options: ohl_io::Options,
    /// The project file.
    pub project: PathBuf,
    /// The user storing it.
    pub user: String,
    /// Name (`None`: the surface's own, else the file's).
    pub name: Option<String>,
}

enum Reply {
    Formats(Formats),
    Exported(Result<String, String>),
    Imported(Result<Box<ohl_line::Line>, String>),
    SurfacesStored(Result<Vec<String>, String>),
    ProjectCreated(Result<(PathBuf, Vec<String>), String>),
}

/// The thread holding the format registry (plugin instances stay on it).
struct Worker {
    jobs: Sender<Job>,
    replies: Receiver<Reply>,
}

impl Worker {
    fn start() -> Self {
        let (jobs, rx) = channel::<Job>();
        let (tx, replies) = channel();
        std::thread::Builder::new()
            .name("formats".into())
            .spawn(move || {
                let mut registry = ohl_io::Registry::with_builtin();
                let host = ohl_plugins::WasmHost::new();
                while let Ok(job) = rx.recv() {
                    let reply = match job {
                        Job::Reload(dirs) => {
                            registry = ohl_io::Registry::with_builtin();
                            let mut errors = Vec::new();
                            for dir in dirs {
                                let r = match &host {
                                    Ok(h) => registry.add_plugin(h, &dir).map(|_| ()),
                                    Err(e) => Err(e.to_string()),
                                };
                                if let Err(e) = r {
                                    errors.push(format!("{}: {e}", dir.display()));
                                }
                            }
                            Reply::Formats(formats_of(&registry, errors))
                        }
                        Job::Export {
                            format,
                            model,
                            options,
                            path,
                        } => {
                            Reply::Exported(export(&mut registry, &format, &model, &options, &path))
                        }
                        Job::Import {
                            format,
                            path,
                            base,
                            options,
                        } => {
                            Reply::Imported(import(&mut registry, &format, &path, &base, &options))
                        }
                        Job::StoreSurfaces(job) => {
                            Reply::SurfacesStored(store_surfaces(&mut registry, &job))
                        }
                        Job::CreateProject(job) => {
                            Reply::ProjectCreated(create_project(&mut registry, &job))
                        }
                    };
                    if tx.send(reply).is_err() {
                        break;
                    }
                }
            })
            .expect("formats thread");
        Self { jobs, replies }
    }
}

/// Reads a surface file and keeps its surfaces in the project (names made
/// unique: " (2)"…); their names.
fn store_surfaces(r: &mut ohl_io::Registry, job: &SurfaceImport) -> Result<Vec<String>, String> {
    let session = ohl_session::Session::open(&job.project).map_err(|e| e.to_string())?;
    let stored = ohl_io::surfaces::import_into(
        r,
        &job.path,
        job.format.as_deref(),
        &job.options,
        &session,
        &job.user,
        job.name.as_deref(),
        ohl_io::surfaces::WhenTaken::Rename,
    )?;
    Ok(stored.into_iter().map(|s| s.name).collect())
}

/// Reads another program's file into a new project (line and library
/// files); its path and the reader's notes.
fn create_project(
    r: &mut ohl_io::Registry,
    job: &ProjectImport,
) -> Result<(PathBuf, Vec<String>), String> {
    let made = ohl_io::projects::create_project(
        r,
        &job.path,
        job.format.as_deref(),
        &job.options,
        &job.into,
        &job.user,
        job.clip_in,
    )?;
    let mut notes = made.project.notes;
    notes.extend(
        made.project
            .line
            .system_warnings()
            .into_iter()
            .map(|w| format!("warning: {w}")),
    );
    Ok((made.path, notes))
}

fn formats_of(r: &ohl_io::Registry, errors: Vec<String>) -> Formats {
    Formats {
        exporters: r
            .exporters()
            .map(|e| FormatInfo {
                id: e.id().into(),
                extension: e.extension().into(),
                description: e.description().into(),
                origin: e.origin(),
            })
            .collect(),
        importers: r
            .importers()
            .map(|e| FormatInfo {
                id: e.id().into(),
                extension: e.extension().into(),
                description: e.description().into(),
                origin: e.origin(),
            })
            .collect(),
        projects: r
            .project_importers()
            .map(|e| FormatInfo {
                id: e.id().into(),
                extension: e.extension().into(),
                description: e.description().into(),
                origin: e.origin(),
            })
            .collect(),
        surfaces: r
            .surface_importers()
            .map(|e| FormatInfo {
                id: e.id().into(),
                extension: e.extension().into(),
                description: e.description().into(),
                origin: e.origin(),
            })
            .collect(),
        errors,
    }
}

fn export(
    r: &mut ohl_io::Registry,
    format: &str,
    model: &ohl_io::ExportModel,
    options: &ohl_io::Options,
    path: &Path,
) -> Result<String, String> {
    let e = r
        .exporter(format)
        .ok_or_else(|| format!("no exporter {format}"))?;
    let started = std::time::Instant::now();
    let bytes = e.export(model, options)?;
    std::fs::write(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!(
        "exported {} ({format}, {} bytes, {:.1?})",
        path.display(),
        bytes.len(),
        started.elapsed()
    ))
}

fn import(
    r: &mut ohl_io::Registry,
    format: &str,
    path: &Path,
    base: &ohl_line::Line,
    options: &ohl_io::Options,
) -> Result<Box<ohl_line::Line>, String> {
    let i = r
        .importer(format)
        .ok_or_else(|| format!("no importer {format}"))?;
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let line = i
        .import(&data, Some(base), options)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    line.validate().map_err(|e| e.to_string())?;
    Ok(Box::new(line))
}

/// Import into the line or Export: the format, the file, its options.
#[derive(Debug, Clone, Default)]
pub struct FormatWindow {
    /// Export (else import).
    pub export: bool,
    /// Format id.
    pub format: String,
    /// Options, one `KEY=VALUE` per line.
    pub options: String,
    /// Running.
    pub busy: bool,
    /// The last error.
    pub error: Option<String>,
}

/// The plugin manager's state (remembered: the folders added, the plugins
/// turned off — in the settings file shared with OHL-Structure,
/// [`ohl_plugins::PluginSettings`], in the user's plugin directory).
#[derive(Default)]
pub struct Plugins {
    /// The user's plugin directory (installs go there); none: none listed.
    pub user_dir: Option<PathBuf>,
    /// The shared settings file (none without a user plugin directory:
    /// nothing is written).
    pub settings_path: Option<PathBuf>,
    /// Folders added, their plugins used in place.
    pub folders: Vec<PathBuf>,
    /// Plugins turned off, by name.
    pub disabled: BTreeSet<String>,
    /// What was found (see [`Plugins::refresh`]).
    pub listed: Vec<Listed>,
    /// The manager window is open.
    pub open: bool,
    /// The last thing done, error or not.
    pub status: Option<(String, bool)>,
    /// A removal waiting for confirmation.
    pub confirm_remove: Option<PathBuf>,
    /// The formats, once the worker loaded them.
    pub formats: Option<Formats>,
    /// The Import / Export window.
    pub window: Option<FormatWindow>,
    worker: Option<Worker>,
    /// The formats need loading again (plugins changed).
    stale: bool,
    /// A surface import running.
    pub storing: bool,
    /// The formats asked of the worker, not come yet.
    reloading: bool,
    /// The surfaces an import kept (taken by the surface manager).
    pub surfaces_stored: Option<Result<Vec<String>, String>>,
    /// A project being created from another program's file.
    pub creating: bool,
    /// The project created (its file and the reader's notes), or why not.
    pub project_created: Option<Result<(PathBuf, Vec<String>), String>>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Saved {
    folders: Vec<PathBuf>,
    disabled: BTreeSet<String>,
}

impl Plugins {
    /// The user's plugin directory: `plugins` in the application's data
    /// directory.
    pub fn default_user_dir() -> Option<PathBuf> {
        eframe::storage_dir(crate::STORAGE_ID).map(|d| d.join("plugins"))
    }

    /// As remembered, listing `user_dir` and the folders added: the
    /// settings shared with OHL-Structure (`settings.toml` in `user_dir`);
    /// before that file exists, what this application kept itself (its
    /// storage, as earlier versions did).
    pub fn load(storage: Option<&dyn eframe::Storage>, user_dir: Option<PathBuf>) -> Self {
        let settings_path = user_dir.as_deref().map(ohl_plugins::PluginSettings::path);
        let mut status = None;
        let shared = match settings_path
            .as_deref()
            .map(ohl_plugins::PluginSettings::load)
        {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                status = Some((format!("plugin settings not read: {e}"), true));
                None
            }
            None => None,
        };
        let saved: Saved = match shared {
            Some(s) => Saved {
                folders: s.folders,
                disabled: s.disabled,
            },
            None => storage
                .and_then(|s| s.get_string("plugins"))
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default(),
        };
        let mut p = Self {
            user_dir,
            settings_path,
            folders: saved.folders,
            disabled: saved.disabled,
            status,
            ..Self::default()
        };
        p.refresh();
        p
    }

    /// Writes the shared settings file now (OHL-Structure reads it), when
    /// there is one.
    pub fn persist(&self) -> Result<(), String> {
        let Some(path) = &self.settings_path else {
            return Ok(());
        };
        ohl_plugins::PluginSettings {
            folders: self.folders.clone(),
            disabled: self.disabled.clone(),
        }
        .save(path)
    }

    /// Reads the shared settings file again (changed in OHL-Structure).
    pub fn reload_settings(&mut self) {
        let Some(path) = &self.settings_path else {
            return;
        };
        match ohl_plugins::PluginSettings::load(path) {
            Ok(Some(s)) => {
                self.folders = s.folders;
                self.disabled = s.disabled;
            }
            Ok(None) => {}
            Err(e) => self.status = Some((format!("plugin settings not read: {e}"), true)),
        }
    }

    /// After a change of the folders or of what is turned off: written to
    /// the shared settings at once (an error goes to the status).
    fn changed_settings(&mut self) {
        if let Err(e) = self.persist() {
            self.status = Some((format!("plugin settings not written: {e}"), true));
        }
    }

    /// Remembers the folders added and the plugins turned off (in the
    /// shared settings file; without one, in this application's storage).
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if self.settings_path.is_some() {
            if let Err(e) = self.persist() {
                log::warn!("plugin settings: {e}");
            }
            return;
        }
        let saved = Saved {
            folders: self.folders.clone(),
            disabled: self.disabled.clone(),
        };
        if let Ok(t) = serde_json::to_string(&saved) {
            storage.set_string("plugins", t);
        }
    }

    /// Lists the plugins again: the user's first, then each folder's; a
    /// name found again is a duplicate. The formats are loaded again.
    pub fn refresh(&mut self) {
        let mut listed: Vec<Listed> = Vec::new();
        let mut names = BTreeSet::new();
        let roots = self
            .user_dir
            .iter()
            .map(|d| (d.clone(), true))
            .chain(self.folders.iter().map(|d| (d.clone(), false)));
        for (root, installed) in roots {
            for found in ohl_plugins::discover(&root) {
                let duplicate = !names.insert(found.name());
                listed.push(Listed {
                    found,
                    installed,
                    duplicate,
                });
            }
        }
        self.listed = listed;
        self.stale = true;
    }

    /// Whether the plugin is used (no problem, not turned off).
    pub fn enabled(&self, l: &Listed) -> bool {
        l.problem().is_none() && !self.disabled.contains(&l.found.name())
    }

    /// The table columns the plugins in use add (`[[column]]` of their
    /// manifests).
    pub fn table_columns(&self) -> Vec<crate::tables::PluginColumn> {
        self.listed
            .iter()
            .filter(|l| self.enabled(l))
            .filter_map(|l| l.found.manifest.as_ref().ok())
            .flat_map(|m| {
                m.columns.iter().map(|c| crate::tables::PluginColumn {
                    table: c.table.clone(),
                    key: m.field_key(c),
                    title: if c.title.is_empty() {
                        c.name.clone()
                    } else {
                        c.title.clone()
                    },
                    help: c.help.clone(),
                    values: c.values.clone(),
                    default: c.default.clone(),
                    plugin: format!("{} {}", m.plugin.name, m.plugin.version),
                    in_use: true,
                })
            })
            .collect()
    }

    /// The plugins in use giving sheet templates: name, folder.
    pub fn sheet_template_dirs(&self) -> Vec<(String, PathBuf)> {
        self.listed
            .iter()
            .filter(|l| !l.duplicate && !self.disabled.contains(&l.found.name()))
            .filter(|l| {
                l.found.provides(ohl_plugins::SHEET_TEMPLATES)
                    || l.found
                        .manifest
                        .as_ref()
                        .is_ok_and(|m| !m.sheet_templates.is_empty())
            })
            .map(|l| (l.found.name(), l.found.dir.clone()))
            .collect()
    }

    /// The directories of the check plugins in use.
    pub fn check_dirs(&self) -> Vec<PathBuf> {
        self.listed
            .iter()
            .filter(|l| self.enabled(l) && l.found.provides(ohl_plugins::CHECK))
            .map(|l| l.found.dir.clone())
            .collect()
    }

    /// The Python plugins in use: name, entry script.
    pub fn python_scripts(&self) -> Vec<(String, PathBuf)> {
        self.listed
            .iter()
            .filter(|l| self.enabled(l))
            .filter_map(|l| {
                let m = l.found.manifest.as_ref().ok()?;
                (m.plugin.tier == Tier::Python).then(|| (m.plugin.name.clone(), m.entry_path()))
            })
            .collect()
    }

    /// Installs the plugin of folder `from` for the user (replacing it when
    /// `replace`).
    pub fn install(&mut self, from: &Path, replace: bool) -> Result<PathBuf, String> {
        let root = self
            .user_dir
            .clone()
            .ok_or("no plugin directory for this user")?;
        let to = ohl_plugins::install(from, &root, replace).map_err(|e| e.to_string())?;
        self.refresh();
        Ok(to)
    }

    /// Removes an installed plugin (its directory in the user's plugin
    /// directory only).
    pub fn remove(&mut self, dir: &Path) -> Result<(), String> {
        let root = self.user_dir.as_ref().ok_or("no plugin directory")?;
        if dir.parent() != Some(root.as_path()) {
            return Err(format!("{} is not an installed plugin", dir.display()));
        }
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        self.refresh();
        Ok(())
    }

    /// Starts the worker (once) and loads the formats again when the
    /// plugins changed; takes the worker's replies. Returns an imported
    /// line, when one came.
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Box<ohl_line::Line>> {
        // Started when first needed.
        if self.worker.is_none() && !self.open && self.window.is_none() {
            return None;
        }
        let dirs: Vec<PathBuf> = self
            .listed
            .iter()
            .filter(|l| l.is_io() && self.enabled(l))
            .map(|l| l.found.dir.clone())
            .collect();
        let worker = self.worker.get_or_insert_with(Worker::start);
        if self.stale {
            self.stale = false;
            let _ = worker.jobs.send(Job::Reload(dirs));
            self.formats = None;
            self.reloading = true;
        }
        let mut imported = None;
        while let Ok(reply) = worker.replies.try_recv() {
            match reply {
                Reply::Formats(f) => {
                    self.formats = Some(f);
                    self.reloading = false;
                }
                Reply::Exported(r) => {
                    if let Some(w) = &mut self.window {
                        w.busy = false;
                        match r {
                            Ok(t) => {
                                self.status = Some((t, false));
                                self.window = None;
                            }
                            Err(e) => w.error = Some(e),
                        }
                    }
                }
                Reply::ProjectCreated(r) => {
                    self.creating = false;
                    self.project_created = Some(r);
                }
                Reply::SurfacesStored(r) => {
                    self.storing = false;
                    self.surfaces_stored = Some(r);
                }
                Reply::Imported(r) => {
                    if let Some(w) = &mut self.window {
                        w.busy = false;
                        match r {
                            Ok(line) => {
                                imported = Some(line);
                                self.window = None;
                            }
                            Err(e) => w.error = Some(e),
                        }
                    }
                }
            }
        }
        let waiting = ((self.open || self.window.is_some()) && self.formats.is_none())
            || self.window.as_ref().is_some_and(|w| w.busy)
            || self.storing
            || self.creating;
        if waiting {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        imported
    }

    /// The manager window.
    pub fn manager_ui(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut toggle: Option<String> = None;
        let mut remove: Option<PathBuf> = None;
        let mut drop_folder: Option<usize> = None;
        egui::Window::new("Plugin manager")
            .open(&mut open)
            .default_width(900.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Plugins add file formats (WebAssembly, sandboxed: no files, no network) \
                         and scripts (Python, run in Plugins › Python console). Interface version \
                         {}.{}. Installed plugins are kept in {}.",
                        ohl_plugins::SDK_VERSION.0,
                        ohl_plugins::SDK_VERSION.1,
                        self.user_dir
                            .as_ref()
                            .map_or("(none)".into(), |d| d.display().to_string())
                    ))
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button("Install from a folder…")
                        .on_hover_text(
                            "Copy a plugin (a folder with ohl-plugin.toml) into your plugins",
                        )
                        .clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        let replace = self.listed.iter().any(|l| {
                            l.installed
                                && ohl_plugins::Manifest::load(&dir)
                                    .is_ok_and(|m| m.plugin.name == l.found.name())
                        });
                        self.status = Some(match self.install(&dir, replace) {
                            Ok(to) => (format!("installed in {}", to.display()), false),
                            Err(e) => (format!("install: {e}"), true),
                        });
                    }
                    if ui
                        .button("Add a plugin folder…")
                        .on_hover_text(
                            "Use the plugins of a folder where they are (e.g. while writing one)",
                        )
                        .clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        if !self.folders.contains(&dir) {
                            self.folders.push(dir);
                        }
                        self.changed_settings();
                        self.refresh();
                    }
                    if ui
                        .button("Reload")
                        .on_hover_text(
                            "Look for plugins again (and read the plugin settings, shared with \
                             OHL-Structure, again)",
                        )
                        .clicked()
                    {
                        self.reload_settings();
                        self.refresh();
                    }
                });
                for (k, f) in self.folders.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("folder {}", f.display())).small());
                        if ui
                            .small_button("✖")
                            .on_hover_text("Stop using this folder")
                            .clicked()
                        {
                            drop_folder = Some(k);
                        }
                    });
                }
                ui.separator();
                if self.listed.is_empty() {
                    ui.label("No plugins.");
                }
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        egui::Grid::new("plugins")
                            .striped(true)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for h in ["plugin", "version", "kind", "provides", "status", ""] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for l in &self.listed {
                                    let name = l.found.name();
                                    let problem = l.problem();
                                    let mut on = self.enabled(l);
                                    if ui
                                        .add_enabled(
                                            problem.is_none(),
                                            egui::Checkbox::new(&mut on, &name),
                                        )
                                        .on_hover_text(format!(
                                            "Use this plugin ({})",
                                            l.found.dir.display()
                                        ))
                                        .changed()
                                    {
                                        toggle = Some(name.clone());
                                    }
                                    match &l.found.manifest {
                                        Ok(m) => {
                                            ui.label(&m.plugin.version);
                                            ui.label(m.plugin.tier.to_string());
                                            let provides = if l.is_io() {
                                                self.formats_of(&name)
                                            } else if m.plugin.tier == Tier::Python {
                                                "a script".into()
                                            } else {
                                                m.plugin.extension_points.join(", ")
                                            };
                                            ui.label(provides).on_hover_text(&m.plugin.description);
                                        }
                                        Err(_) => {
                                            for _ in 0..3 {
                                                ui.label("");
                                            }
                                        }
                                    }
                                    match &problem {
                                        Some(p) => crate::menu::error_label(ui, p),
                                        None if !on => ui.label("off"),
                                        None if l.installed => ui.label("installed"),
                                        None => ui.label("in its folder"),
                                    };
                                    if l.installed {
                                        if ui
                                            .small_button("Remove")
                                            .on_hover_text("Delete this installed copy")
                                            .clicked()
                                        {
                                            remove = Some(l.found.dir.clone());
                                        }
                                    } else {
                                        ui.label("");
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(f) = &self.formats {
                    for e in &f.errors {
                        crate::menu::error_label(ui, e);
                    }
                }
                if let Some((t, error)) = &self.status {
                    if *error {
                        crate::menu::error_label(ui, t);
                    } else {
                        ui.label(t);
                    }
                }
            });
        self.open = open;
        if let Some(name) = toggle {
            if !self.disabled.remove(&name) {
                self.disabled.insert(name);
            }
            self.changed_settings();
            self.stale = true;
        }
        if let Some(k) = drop_folder {
            self.folders.remove(k);
            self.changed_settings();
            self.refresh();
        }
        if let Some(dir) = remove {
            self.confirm_remove = Some(dir);
        }
        if let Some(dir) = self.confirm_remove.clone() {
            let mut answer = None;
            egui::Window::new("Remove the plugin?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!("Delete {} and everything in it?", dir.display()));
                    ui.horizontal(|ui| {
                        if ui.button("Remove").clicked() {
                            answer = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            answer = Some(false);
                        }
                    });
                });
            if let Some(yes) = answer {
                self.confirm_remove = None;
                if yes {
                    self.status = Some(match self.remove(&dir) {
                        Ok(()) => (format!("removed {}", dir.display()), false),
                        Err(e) => (format!("remove: {e}"), true),
                    });
                }
            }
        }
    }

    /// The formats a plugin gives (once loaded).
    fn formats_of(&self, name: &str) -> String {
        let Some(f) = &self.formats else {
            return "formats (loading…)".into();
        };
        let origin = format!("plugin {name}");
        let list: Vec<String> = f
            .exporters
            .iter()
            .map(|e| (e, "export"))
            .chain(f.importers.iter().map(|i| (i, "import")))
            .filter(|(e, _)| e.origin == origin)
            .map(|(e, d)| format!("{d} {}", e.id))
            .collect();
        if list.is_empty() {
            "formats (none loaded)".into()
        } else {
            list.join(", ")
        }
    }

    /// Creates a project from another program's file (on the worker); the
    /// result comes in [`Plugins::project_created`].
    pub fn create_project(&mut self, job: ProjectImport) {
        let worker = self.worker.get_or_insert_with(Worker::start);
        if worker.jobs.send(Job::CreateProject(job)).is_ok() {
            self.creating = true;
            self.project_created = None;
        }
    }

    /// The formats are wanted (a window lists them): the worker starts
    /// and loads them if it has not.
    pub fn want_formats(&mut self) {
        self.worker.get_or_insert_with(Worker::start);
        if self.formats.is_none() && !self.reloading {
            self.stale = true;
        }
    }

    /// Reads a surface file and keeps it in the project (on the worker);
    /// the result comes in [`Plugins::surfaces_stored`].
    pub fn store_surfaces(&mut self, job: SurfaceImport) {
        let worker = self.worker.get_or_insert_with(Worker::start);
        if worker.jobs.send(Job::StoreSurfaces(job)).is_ok() {
            self.storing = true;
            self.surfaces_stored = None;
        }
    }

    /// Opens File › Import into the line (`export` false) or File ›
    /// Export.
    pub fn open_window(&mut self, export: bool) {
        self.window = Some(FormatWindow {
            export,
            ..Default::default()
        });
    }

    /// The Import / Export window; the job goes to the worker.
    pub fn format_ui(
        &mut self,
        ctx: &egui::Context,
        model: &crate::model::Model,
        editable: Result<(), String>,
    ) {
        let Some(mut w) = self.window.take() else {
            return;
        };
        let mut open = true;
        let mut start: Option<PathBuf> = None;
        let title = if w.export {
            "Export"
        } else {
            "Import into the line"
        };
        let list: Vec<FormatInfo> = match &self.formats {
            Some(f) if w.export => f.exporters.clone(),
            Some(f) => f.importers.clone(),
            None => Vec::new(),
        };
        if !list.iter().any(|f| f.id == w.format)
            && let Some(f) = list.first()
        {
            w.format = f.id.clone();
        }
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                if self.formats.is_none() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("loading the formats…");
                    });
                    return;
                }
                let chosen = list.iter().find(|f| f.id == w.format);
                egui::ComboBox::from_label("format")
                    .selected_text(
                        chosen.map_or(String::new(), |f| format!("{} (.{})", f.id, f.extension)),
                    )
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for f in &list {
                            ui.selectable_value(
                                &mut w.format,
                                f.id.clone(),
                                format!("{} (.{}) — {}", f.id, f.extension, f.origin),
                            )
                            .on_hover_text(&f.description);
                        }
                    });
                if let Some(f) = chosen {
                    ui.label(egui::RichText::new(&f.description).small().weak());
                }
                ui.label("Options (one KEY=VALUE per line)");
                ui.add(
                    egui::TextEdit::multiline(&mut w.options)
                        .desired_rows(2)
                        .desired_width(480.0),
                );
                if w.export {
                    ui.label(
                        egui::RichText::new(format!(
                            "The line and its {} solved weather case(s).",
                            model.cases.len()
                        ))
                        .small()
                        .weak(),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(
                            "The file is read into the line you work on: one command, undoable.",
                        )
                        .small()
                        .weak(),
                    );
                    if let Err(why) = &editable {
                        crate::menu::error_label(ui, format!("read-only: {why}"));
                    }
                }
                if let Some(e) = &w.error {
                    crate::menu::error_label(ui, e);
                }
                if w.busy {
                    ui.spinner();
                } else if let Some(f) = chosen {
                    let can = w.export || editable.is_ok();
                    let label = if w.export {
                        "Export to…"
                    } else {
                        "Choose the file…"
                    };
                    if ui.add_enabled(can, egui::Button::new(label)).clicked() {
                        let dialog = rfd::FileDialog::new()
                            .add_filter(&f.id, std::slice::from_ref(&f.extension));
                        start = if w.export {
                            dialog
                                .set_file_name(format!("{}.{}", model.line.name, f.extension))
                                .save_file()
                        } else {
                            dialog.pick_file()
                        };
                    }
                }
            });
        if let Some(path) = start {
            self.run(&mut w, model, path);
        }
        if open {
            self.window = Some(w);
        }
    }

    /// Sends the window's job for `path` to the worker.
    pub fn run(&mut self, w: &mut FormatWindow, model: &crate::model::Model, path: PathBuf) {
        let options = match ohl_io::parse_options(&w.options) {
            Ok(o) => o,
            Err(e) => {
                w.error = Some(e);
                return;
            }
        };
        let Some(worker) = &self.worker else {
            return;
        };
        let job = if w.export {
            let cases: Vec<_> = model
                .cases
                .iter()
                .map(|c| {
                    (
                        c.case.name.clone(),
                        c.case.summary(),
                        c.shapes.clone(),
                        c.jumpers.clone(),
                    )
                })
                .collect();
            Job::Export {
                format: w.format.clone(),
                model: Box::new(ohl_io::ExportModel::new(&model.line, &cases)),
                options,
                path,
            }
        } else {
            Job::Import {
                format: w.format.clone(),
                path,
                base: Box::new(model.line.clone()),
                options,
            }
        };
        w.error = None;
        w.busy = worker.jobs.send(job).is_ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plugin folders and the plugins turned off come from the
    /// settings file shared with OHL-Structure (`settings.toml` in the
    /// user's plugin directory); a change is written back to it at once;
    /// without a user directory nothing is written.
    #[test]
    fn plugin_settings_are_shared() {
        let user = std::env::temp_dir().join(format!("ohl-app-shared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&user);
        let path = ohl_plugins::PluginSettings::path(&user);
        ohl_plugins::PluginSettings {
            folders: vec![PathBuf::from("/elsewhere/plugins")],
            disabled: ["geojson".to_owned()].into(),
        }
        .save(&path)
        .unwrap();
        let mut p = Plugins::load(None, Some(user.clone()));
        assert_eq!(p.settings_path.as_deref(), Some(path.as_path()));
        assert_eq!(p.folders, vec![PathBuf::from("/elsewhere/plugins")]);
        assert!(p.disabled.contains("geojson"));
        // Turned on again here: the file says so (OHL-Structure reads it).
        p.disabled.clear();
        p.changed_settings();
        let read = ohl_plugins::PluginSettings::load(&path).unwrap().unwrap();
        assert!(read.disabled.is_empty());
        // Changed elsewhere: Reload reads it.
        ohl_plugins::PluginSettings {
            folders: Vec::new(),
            disabled: ["en50341".to_owned()].into(),
        }
        .save(&path)
        .unwrap();
        p.reload_settings();
        assert!(p.folders.is_empty() && p.disabled.contains("en50341"));
        // No user directory: nothing shared.
        let none = Plugins::load(None, None);
        assert!(none.settings_path.is_none() && none.persist().is_ok());
        std::fs::remove_dir_all(&user).unwrap();
    }
}
