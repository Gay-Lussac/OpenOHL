// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Plugins in OHL-Structure (Plugins › Plugin manager), with `ols-plugins`
//! as in the line application: the plugins installed for the user and
//! those of the folders added, each with its version, tier and extension
//! points, what it does here (or why not, greyed), a tick to turn it off,
//! its details (what it describes) and why it cannot be used. The folders
//! and the plugins turned off are the settings shared with the line
//! application ([`ols_plugins::PluginSettings`], `settings.toml` in the
//! user's plugin directory): a plugin added or turned off in one is so in
//! the other.
//!
//! What applies here: **design-code** plugins (Load cases › Load cases from
//! a design code…: the plugin's `generate` on a parameter file, its load
//! cases taken as recipes taking the line's values); **check** plugins are
//! listed (member and joint code checks come later: nothing to run yet);
//! **io** plugins write and read the line's export model, not a structure
//! (listed under File › Export as not applicable); terrain and project
//! plugins and Python scripts belong to the line application.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ols_plugins::{Found, PluginSettings, Tier};

use crate::app::{StructureApp, error_label};

/// A plugin listed: where it was found, whether it is installed, a
/// duplicate.
#[derive(Debug, Clone)]
pub struct Listed {
    /// Its folder and manifest.
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
}

/// What extension point `point` does in OHL-Structure: `Ok` what it gives,
/// `Err` why it does not apply (shown greyed).
pub fn applies(point: &str) -> Result<&'static str, &'static str> {
    match point {
        ols_plugins::DESIGN_CODE => Ok("load cases from a design code (Load cases menu)"),
        ols_plugins::CHECK => Err(
            "checks: member and joint code checks will run here; nothing to run yet (the line \
             application runs its clearance checks)",
        ),
        "io" => Err(
            "file formats of the line: they take the line's export model, not a structure \
             (listed under File › Export)",
        ),
        "terrain" => Err("terrain surfaces: OHL-Design"),
        "project" => Err("projects from other programs: OHL-Design"),
        _ => Err("an extension point this application does not know"),
    }
}

/// What a plugin does here, in words, and whether anything applies.
pub fn applies_to(found: &Found) -> (String, bool) {
    let Ok(m) = &found.manifest else {
        return ("—".into(), false);
    };
    if m.plugin.tier == Tier::Python {
        return (
            "a Python script: run in OHL-Design (Plugins › Python console)".into(),
            false,
        );
    }
    if m.plugin.extension_points.is_empty() {
        return ("no extension point: nothing to run here".into(), false);
    }
    let mut yes = Vec::new();
    let mut no = Vec::new();
    for p in &m.plugin.extension_points {
        match applies(p) {
            Ok(t) => yes.push(t.to_owned()),
            Err(t) => no.push(format!("{p} — {t}")),
        }
    }
    let any = !yes.is_empty();
    yes.extend(no);
    (yes.join("; "), any)
}

/// A design-code plugin's result, as taken here.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeResult {
    /// The plugin.
    pub plugin: String,
    /// Weather cases (`.olswc` text).
    pub weather: String,
    /// Load cases (`.olslc` text).
    pub load_cases: String,
    /// The load cases' names.
    pub names: Vec<String>,
    /// What the plugin generated, assumed or left out.
    pub notes: Vec<String>,
}

/// Runs design-code plugin `dir`'s `generate` on a parameter file's text
/// (the plugin's own exit clauses apply: a time limit and a memory cap).
pub fn run_code(dir: &Path, parameters: &str) -> Result<CodeResult, String> {
    let host = ols_plugins::WasmHost::new().map_err(|e| e.to_string())?;
    let mut p = host.load_code(dir).map_err(|e| e.to_string())?;
    let plugin = p.manifest().plugin.name.clone();
    let g = p
        .generate(parameters)
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{plugin}: the parameter file: {e}"))?;
    let table: toml::Table = g
        .load_cases
        .parse()
        .map_err(|e| format!("{plugin}: its load cases do not read: {e}"))?;
    let names: Vec<String> = table
        .get("case")
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_owned))
        .collect();
    if names.is_empty() {
        return Err(format!("{plugin}: no load case generated"));
    }
    Ok(CodeResult {
        plugin,
        weather: g.weather_cases,
        load_cases: g.load_cases,
        names,
        notes: g.notes,
    })
}

/// The Load cases from a design code window.
#[derive(Debug, Default)]
pub struct CodeWindow {
    /// The plugin chosen (its name).
    pub plugin: String,
    /// The parameter file chosen.
    pub parameters: Option<PathBuf>,
    /// Also write the weather and load case files beside the structure.
    pub save_files: bool,
    /// A run going on.
    pub(crate) running: Option<Receiver<Result<CodeResult, String>>>,
    /// The last result, or why not.
    pub result: Option<Result<CodeResult, String>>,
}

/// What a plugin describes, one line each, or why it does not load.
pub type Details = Result<Vec<String>, String>;

/// The plugin manager's state.
#[derive(Default)]
pub struct PluginsState {
    /// The user's plugin directory (shared with the line application;
    /// installs go there); none: nothing listed from there, nothing kept.
    pub user_dir: Option<PathBuf>,
    /// The folders added (shared settings).
    pub folders: Vec<PathBuf>,
    /// The plugins turned off, by name (shared settings).
    pub disabled: BTreeSet<String>,
    /// What was found.
    pub listed: Vec<Listed>,
    /// The manager window is open.
    pub open: bool,
    /// The last thing done, error or not.
    pub status: Option<(String, bool)>,
    /// A removal waiting for confirmation.
    pub confirm_remove: Option<PathBuf>,
    /// The details asked for, by plugin (what it describes, or why not).
    pub details: BTreeMap<String, Details>,
    /// Details being read.
    describing: Vec<(String, Receiver<Details>)>,
    /// The details shown (a plugin's name).
    pub shown: Option<String>,
    /// The design-code window.
    pub code: Option<CodeWindow>,
}

impl PluginsState {
    /// The user's plugin directory, shared with the line application:
    /// `plugins` in the line application's data directory (its storage
    /// id from before OpenOHL, [`crate::LINE_STORAGE_ID`]).
    pub fn default_user_dir() -> Option<PathBuf> {
        eframe::storage_dir(crate::LINE_STORAGE_ID).map(|d| d.join("plugins"))
    }

    /// The plugins of `user_dir` and of the folders of the shared settings.
    pub fn load(user_dir: Option<PathBuf>) -> Self {
        let mut p = Self {
            user_dir,
            ..Self::default()
        };
        p.reload_settings();
        p.refresh();
        p
    }

    /// The shared settings file (none without a user directory).
    pub fn settings_path(&self) -> Option<PathBuf> {
        self.user_dir.as_deref().map(PluginSettings::path)
    }

    /// The shared settings read again (the line application may have
    /// changed them).
    pub fn reload_settings(&mut self) {
        let Some(path) = self.settings_path() else {
            return;
        };
        match PluginSettings::load(&path) {
            Ok(Some(s)) => {
                self.folders = s.folders;
                self.disabled = s.disabled;
            }
            Ok(None) => {}
            Err(e) => self.status = Some((format!("plugin settings not read: {e}"), true)),
        }
    }

    /// The shared settings written now.
    pub fn persist(&mut self) {
        let Some(path) = self.settings_path() else {
            return;
        };
        let s = PluginSettings {
            folders: self.folders.clone(),
            disabled: self.disabled.clone(),
        };
        if let Err(e) = s.save(&path) {
            self.status = Some((format!("plugin settings not written: {e}"), true));
        }
    }

    /// Opens the manager (the shared settings read again first).
    pub fn open_manager(&mut self) {
        self.reload_settings();
        self.refresh();
        self.open = true;
    }

    /// Lists the plugins again: the user's first, then each folder's; a
    /// name found again is a duplicate.
    pub fn refresh(&mut self) {
        let mut listed = Vec::new();
        let mut names = BTreeSet::new();
        let roots = self
            .user_dir
            .iter()
            .map(|d| (d.clone(), true))
            .chain(self.folders.iter().map(|d| (d.clone(), false)));
        for (root, installed) in roots {
            for found in ols_plugins::discover(&root) {
                let duplicate = !names.insert(found.name());
                listed.push(Listed {
                    found,
                    installed,
                    duplicate,
                });
            }
        }
        self.listed = listed;
    }

    /// Whether the plugin is used (no problem, not turned off).
    pub fn enabled(&self, l: &Listed) -> bool {
        l.problem().is_none() && !self.disabled.contains(&l.found.name())
    }

    /// Turns plugin `name` on or off (kept in the shared settings).
    pub fn set_enabled(&mut self, name: &str, on: bool) {
        if on {
            self.disabled.remove(name);
        } else {
            self.disabled.insert(name.to_owned());
        }
        self.persist();
    }

    /// Adds a plugin folder (kept in the shared settings).
    pub fn add_folder(&mut self, dir: PathBuf) {
        if !self.folders.contains(&dir) {
            self.folders.push(dir);
        }
        self.persist();
        self.refresh();
    }

    /// The design-code plugins in use: name and folder.
    pub fn codes(&self) -> Vec<(String, PathBuf)> {
        self.listed
            .iter()
            .filter(|l| self.enabled(l) && l.found.provides(ols_plugins::DESIGN_CODE))
            .map(|l| (l.found.name(), l.found.dir.clone()))
            .collect()
    }

    /// The file-format plugins in use (they take the line's model).
    pub fn io_plugins(&self) -> Vec<String> {
        self.listed
            .iter()
            .filter(|l| self.enabled(l) && l.found.provides("io"))
            .map(|l| l.found.name())
            .collect()
    }

    /// Installs the plugin of folder `from` for the user.
    pub fn install(&mut self, from: &Path, replace: bool) -> Result<PathBuf, String> {
        let root = self
            .user_dir
            .clone()
            .ok_or("no plugin directory for this user")?;
        let to = ols_plugins::install(from, &root, replace).map_err(|e| e.to_string())?;
        self.refresh();
        Ok(to)
    }

    /// Removes an installed plugin (in the user's plugin directory only).
    pub fn remove(&mut self, dir: &Path) -> Result<(), String> {
        let root = self.user_dir.as_ref().ok_or("no plugin directory")?;
        if dir.parent() != Some(root.as_path()) {
            return Err(format!("{} is not an installed plugin", dir.display()));
        }
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        self.refresh();
        Ok(())
    }

    /// Reads what plugin `l` describes, on a thread (compiling a plugin
    /// takes a moment).
    fn describe(&mut self, l: &Listed) {
        let name = l.found.name();
        if self.describing.iter().any(|(n, _)| *n == name) {
            return;
        }
        let dir = l.found.dir.clone();
        let python = l
            .found
            .manifest
            .as_ref()
            .is_ok_and(|m| m.plugin.tier == Tier::Python);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = if python {
                Ok(vec!["a Python script (not loaded here)".to_owned()])
            } else {
                ols_plugins::WasmHost::new()
                    .and_then(|h| h.describe_any(&dir))
                    .map(|d| {
                        let mut lines = vec![
                            format!("{} {}: {}", d.info.name, d.info.version, d.info.description),
                            format!("worlds: {}", d.worlds.join(", ")),
                        ];
                        lines.extend(d.details);
                        lines
                    })
                    .map_err(|e| e.to_string())
            };
            let _ = tx.send(r);
        });
        self.describing.push((name, rx));
    }

    /// Takes the details read and the design code's result, when they
    /// came; asks for frames while waiting.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        for (k, (name, rx)) in self.describing.iter().enumerate() {
            if let Ok(r) = rx.try_recv() {
                self.details.insert(name.clone(), r);
                done.push(k);
            }
        }
        for k in done.into_iter().rev() {
            self.describing.remove(k);
        }
        if let Some(w) = &mut self.code
            && let Some(rx) = &w.running
            && let Ok(r) = rx.try_recv()
        {
            w.result = Some(r);
            w.running = None;
        }
        let waiting =
            !self.describing.is_empty() || self.code.as_ref().is_some_and(|w| w.running.is_some());
        if waiting {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    /// Whether a design-code run is going on.
    #[cfg(test)]
    pub fn code_running(&self) -> bool {
        self.code.as_ref().is_some_and(|w| w.running.is_some())
    }

    /// Starts the design-code plugin chosen on the parameter file chosen
    /// (on a thread).
    pub fn start_code(&mut self) -> Result<(), String> {
        let codes = self.codes();
        let w = self.code.as_mut().ok_or("no design-code window")?;
        let dir = codes
            .iter()
            .find(|(n, _)| *n == w.plugin)
            .map(|(_, d)| d.clone())
            .ok_or("choose a design-code plugin")?;
        let path = w.parameters.clone().ok_or("choose a parameter file")?;
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(run_code(&dir, &text));
        });
        w.running = Some(rx);
        w.result = None;
        Ok(())
    }

    /// Waits for the design-code run (tests), at most `limit`.
    #[cfg(test)]
    pub fn wait_code(&mut self, limit: std::time::Duration) {
        let started = std::time::Instant::now();
        while self.code_running() && started.elapsed() < limit {
            if let Some(w) = &mut self.code
                && let Some(rx) = &w.running
                && let Ok(r) = rx.recv_timeout(std::time::Duration::from_millis(100))
            {
                w.result = Some(r);
                w.running = None;
            }
        }
    }
}

impl StructureApp {
    /// The Plugins menu: the manager, the design codes.
    pub(crate) fn plugins_menu(&mut self, ui: &mut egui::Ui) {
        if ui
            .button("Plugin manager…")
            .on_hover_text(
                "The plugins (shared with OHL-Design): folders, on and off, what each does here",
            )
            .clicked()
        {
            ui.close();
            self.plugins.open_manager();
        }
        self.code_item(ui);
    }

    /// Load cases from a design code… (Plugins and Load cases menus).
    pub(crate) fn code_item(&mut self, ui: &mut egui::Ui) {
        let has = self.file.is_some();
        let codes = self.plugins.codes();
        let r = ui
            .add_enabled(
                has && !codes.is_empty(),
                egui::Button::new("Load cases from a design code…"),
            )
            .on_hover_text(
                "A design-code plugin's load cases from your parameter file, taken as recipes \
                 taking the line's values",
            )
            .on_disabled_hover_text(if has {
                "No design-code plugin in use (Plugins › Plugin manager: add the folder of the \
                 code plugins)"
            } else {
                "Open a structure first"
            });
        if r.clicked() {
            ui.close();
            let plugin = codes.first().map(|c| c.0.clone()).unwrap_or_default();
            self.plugins.code = Some(CodeWindow {
                plugin,
                ..CodeWindow::default()
            });
        }
    }

    /// The design code's load cases taken as recipes (one undo step), and
    /// the generated files written beside the structure when asked. What
    /// was done.
    pub fn take_code_cases(&mut self, r: &CodeResult, save_files: bool) -> Result<String, String> {
        let f = self.file.as_mut().ok_or("no structure open")?;
        let written = f.edit("load cases from a design code", |doc, _| {
            ols_structure_edit::take_line_load_cases(doc, &r.names)
        });
        let mut m = if written.is_empty() {
            format!(
                "every load case of {} has a recipe of its name already",
                r.plugin
            )
        } else {
            format!(
                "load cases of {} taken as recipes: {}",
                r.plugin,
                written.join(", ")
            )
        };
        if save_files {
            let dir = f
                .path()
                .and_then(Path::parent)
                .ok_or("save the structure first: the files go beside it")?
                .to_path_buf();
            let weather = dir.join(format!("{}.olswc", r.plugin));
            let cases = dir.join(format!("{}.olslc", r.plugin));
            for (p, t) in [(&weather, &r.weather), (&cases, &r.load_cases)] {
                std::fs::write(p, t).map_err(|e| format!("{}: {e}", p.display()))?;
            }
            m.push_str(&format!(
                "; written {} and {} (give them to the line: Criteria › Weather cases, Load \
                 cases)",
                weather.display(),
                cases.display()
            ));
        }
        Ok(m)
    }

    /// The plugin manager and the design-code windows.
    pub(crate) fn plugin_windows(&mut self, ctx: &egui::Context) {
        self.plugins.poll(ctx);
        self.manager_window(ctx);
        self.code_window(ctx);
    }

    fn manager_window(&mut self, ctx: &egui::Context) {
        let p = &mut self.plugins;
        if !p.open {
            return;
        }
        let mut open = true;
        let mut toggle: Option<(String, bool)> = None;
        let mut remove: Option<PathBuf> = None;
        let mut drop_folder: Option<usize> = None;
        let mut describe: Option<Listed> = None;
        egui::Window::new("Plugin manager")
            .open(&mut open)
            .default_width(960.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Plugins (WebAssembly, sandboxed) add design codes, checks and file \
                         formats. Interface version {}.{}. Installed plugins are kept in {}; \
                         the folders and what is turned off are shared with OHL-Design ({}).",
                        ols_plugins::SDK_VERSION.0,
                        ols_plugins::SDK_VERSION.1,
                        p.user_dir
                            .as_ref()
                            .map_or("(none)".into(), |d| d.display().to_string()),
                        p.settings_path()
                            .map_or("not kept".into(), |d| d.display().to_string()),
                    ))
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button("Install from a folder…")
                        .on_hover_text(
                            "Copy a plugin (a folder with ols-plugin.toml) into your plugins",
                        )
                        .clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        let replace = p.listed.iter().any(|l| {
                            l.installed
                                && ols_plugins::Manifest::load(&dir)
                                    .is_ok_and(|m| m.plugin.name == l.found.name())
                        });
                        p.status = Some(match p.install(&dir, replace) {
                            Ok(to) => (format!("installed in {}", to.display()), false),
                            Err(e) => (format!("install: {e}"), true),
                        });
                    }
                    if ui
                        .button("Add a plugin folder…")
                        .on_hover_text(
                            "Use the plugins of a folder where they are (e.g. the code plugins \
                             of the OpenOHL folder, plugins/codes)",
                        )
                        .clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        p.add_folder(dir);
                    }
                    if ui
                        .button("Reload")
                        .on_hover_text("Read the shared settings and look for plugins again")
                        .clicked()
                    {
                        p.reload_settings();
                        p.refresh();
                    }
                });
                for (k, f) in p.folders.iter().enumerate() {
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
                if p.listed.is_empty() {
                    ui.label("No plugins.");
                }
                egui::ScrollArea::vertical()
                    .id_salt("plugins")
                    .max_height(360.0)
                    .show(ui, |ui| {
                        egui::Grid::new("plugins-grid")
                            .striped(true)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for h in [
                                    "plugin",
                                    "version",
                                    "tier",
                                    "extension points",
                                    "in OHL-Structure",
                                    "status",
                                    "",
                                ] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for l in &p.listed {
                                    let name = l.found.name();
                                    let problem = l.problem();
                                    let mut on = p.enabled(l);
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
                                        toggle = Some((name.clone(), on));
                                    }
                                    match &l.found.manifest {
                                        Ok(m) => {
                                            ui.label(&m.plugin.version);
                                            ui.label(m.plugin.tier.to_string());
                                            let points = if m.plugin.extension_points.is_empty() {
                                                "—".to_owned()
                                            } else {
                                                m.plugin.extension_points.join(", ")
                                            };
                                            ui.label(points).on_hover_text(&m.plugin.description);
                                        }
                                        Err(_) => {
                                            for _ in 0..3 {
                                                ui.label("");
                                            }
                                        }
                                    }
                                    let (what, any) = applies_to(&l.found);
                                    let text = egui::RichText::new(what).small();
                                    ui.label(if any { text } else { text.weak() });
                                    match &problem {
                                        Some(e) => {
                                            ui.vertical(|ui| {
                                                ui.set_max_width(260.0);
                                                error_label(ui, e.as_str());
                                            });
                                        }
                                        None if !on => {
                                            ui.label("off");
                                        }
                                        None if l.installed => {
                                            ui.label("installed");
                                        }
                                        None => {
                                            ui.label("in its folder");
                                        }
                                    }
                                    ui.horizontal(|ui| {
                                        if ui
                                            .small_button("Details")
                                            .on_hover_text(
                                                "What it describes (loaded in each of its worlds)",
                                            )
                                            .clicked()
                                        {
                                            describe = Some(l.clone());
                                        }
                                        if l.installed
                                            && ui
                                                .small_button("Remove")
                                                .on_hover_text("Delete this installed copy")
                                                .clicked()
                                        {
                                            remove = Some(l.found.dir.clone());
                                        }
                                    });
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(name) = &p.shown {
                    ui.separator();
                    ui.strong(format!("Details of {name}"));
                    match p.details.get(name) {
                        None => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("loading the plugin…");
                            });
                        }
                        Some(Ok(lines)) => {
                            egui::ScrollArea::vertical()
                                .id_salt("plugin-details")
                                .max_height(140.0)
                                .show(ui, |ui| {
                                    for l in lines {
                                        ui.label(l);
                                    }
                                });
                        }
                        Some(Err(e)) => error_label(ui, e.as_str()),
                    }
                }
                if let Some((t, error)) = &p.status {
                    if *error {
                        error_label(ui, t.as_str());
                    } else {
                        ui.label(t);
                    }
                }
            });
        p.open = open;
        if let Some((name, on)) = toggle {
            p.set_enabled(&name, on);
        }
        if let Some(k) = drop_folder {
            p.folders.remove(k);
            p.persist();
            p.refresh();
        }
        if let Some(l) = describe {
            p.shown = Some(l.found.name());
            p.details.remove(&l.found.name());
            p.describe(&l);
        }
        if let Some(dir) = remove {
            p.confirm_remove = Some(dir);
        }
        if let Some(dir) = p.confirm_remove.clone() {
            let mut answer = None;
            let mut still = true;
            egui::Window::new("Remove the plugin?")
                .collapsible(false)
                .resizable(false)
                .open(&mut still)
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
            if !still {
                answer = Some(false);
            }
            if let Some(yes) = answer {
                p.confirm_remove = None;
                if yes {
                    p.status = Some(match p.remove(&dir) {
                        Ok(()) => (format!("removed {}", dir.display()), false),
                        Err(e) => (format!("remove: {e}"), true),
                    });
                }
            }
        }
    }

    fn code_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.plugins.code.take() else {
            return;
        };
        let codes = self.plugins.codes();
        let mut open = true;
        let mut run = false;
        let mut take: Option<CodeResult> = None;
        egui::Window::new("Load cases from a design code")
            .collapsible(false)
            .open(&mut open)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "The plugin generates the line's weather and load cases from your \
                         parameter file (each value from your standard: the plugins' example \
                         files hold invented placeholders). Its load cases become recipes \
                         taking the line's factored values in those cases (from a placed \
                         structure: Open in OHL-Structure… in OHL-Design).",
                    )
                    .small()
                    .weak(),
                );
                egui::ComboBox::from_label("design code")
                    .selected_text(w.plugin.clone())
                    .show_ui(ui, |ui| {
                        for (name, dir) in &codes {
                            ui.selectable_value(&mut w.plugin, name.clone(), name)
                                .on_hover_text(dir.display().to_string());
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("Parameter file…").clicked()
                        && let Some(p) = rfd::FileDialog::new()
                            .add_filter("Parameters", &["toml"])
                            .pick_file()
                    {
                        w.parameters = Some(p);
                    }
                    ui.label(
                        w.parameters
                            .as_ref()
                            .map_or("none chosen".into(), |p| p.display().to_string()),
                    );
                });
                ui.checkbox(
                    &mut w.save_files,
                    "Also write the weather and load case files beside the structure",
                );
                if w.running.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("the plugin is running…");
                    });
                } else if ui
                    .add_enabled(
                        w.parameters.is_some() && !w.plugin.is_empty(),
                        egui::Button::new("▶ Generate"),
                    )
                    .clicked()
                {
                    run = true;
                }
                match &w.result {
                    Some(Ok(r)) => {
                        ui.label(format!(
                            "{} load case(s): {}",
                            r.names.len(),
                            r.names.join(", ")
                        ));
                        if !r.notes.is_empty() {
                            egui::ScrollArea::vertical()
                                .id_salt("code-notes")
                                .max_height(120.0)
                                .show(ui, |ui| {
                                    for n in &r.notes {
                                        ui.label(egui::RichText::new(n).small());
                                    }
                                });
                        }
                        if ui
                            .button("Take them as recipes")
                            .on_hover_text("One recipe per load case (one undo step)")
                            .clicked()
                        {
                            take = Some(r.clone());
                        }
                    }
                    Some(Err(e)) => error_label(ui, e.as_str()),
                    None => {}
                }
            });
        self.plugins.code = Some(w);
        if run
            && let Err(e) = self.plugins.start_code()
            && let Some(w) = &mut self.plugins.code
        {
            w.result = Some(Err(e));
        }
        if let Some(r) = take {
            let save = self.plugins.code.as_ref().is_some_and(|w| w.save_files);
            self.message = Some(match self.take_code_cases(&r, save) {
                Ok(m) => (m, false),
                Err(e) => (e, true),
            });
            self.focus(crate::layout::Panel::LoadRecipes);
        }
        if !open {
            self.plugins.code = None;
        }
    }
}
