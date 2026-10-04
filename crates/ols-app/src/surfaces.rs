// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Terrain › Terrain surfaces (TIN): the surfaces kept in the project.
//!
//! One is **active**: it is the ground — drawn, and used for clearances,
//! profiles and placing structures. Others can be **shown** too, each in
//! its colour (the "Other surfaces" layer). Surfaces are imported from
//! files (LandXML, OBJ, survey points triangulated, and the formats of
//! terrain plugins), built from the LiDAR ground classes, or kept from the
//! ground shown; renamed, exported (LandXML, OBJ) and deleted. Reading,
//! building and storing run in the background.
//!
//! Which surface is active and which are shown is each user's own choice,
//! remembered on their computer ([`crate::model::ViewPrefs`]) — never
//! written to the project nor sent to a server. The surfaces themselves
//! are part of the project: with a project hosted on a server they are
//! imported on the hosting computer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui;
use ols_session::surfaces::SurfaceInfo;
use ols_terrain::Terrain;
use serde::{Deserialize, Serialize};

use crate::model::Model;

/// The project setting naming the surface the checks use (clearances):
/// one for everyone, unlike what each user shows. None: each user's
/// ground.
pub const CHECK_SETTING: &str = "app.check_surface";

/// The surface the checks use, as kept in the project.
pub fn check_surface(session: &ols_session::Session) -> Option<String> {
    session.setting(CHECK_SETTING).ok().flatten()
}

/// The project setting of earlier builds (active surface and those
/// shown): read when this user has no choice of their own yet.
pub const SETTING: &str = "app.surfaces";

/// Which surface is the ground and which others are shown.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSettings {
    /// The ground.
    #[serde(default)]
    pub active: Option<String>,
    /// Others shown, with their colour.
    #[serde(default)]
    pub shown: BTreeMap<String, [u8; 3]>,
}

impl SurfaceSettings {
    /// As kept in a project by an earlier build.
    pub fn read(session: &ols_session::Session) -> Self {
        session
            .setting(SETTING)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
}

/// Another surface shown.
#[derive(Debug, Clone)]
pub struct OtherSurface {
    /// Name.
    pub name: String,
    /// The surface.
    pub terrain: Arc<Terrain>,
    /// Its colour.
    pub color: [u8; 3],
}

/// Colours given to surfaces shown, in turn.
const COLORS: [[u8; 3]; 6] = [
    [90, 170, 230],
    [230, 150, 60],
    [120, 200, 110],
    [210, 100, 200],
    [230, 220, 90],
    [150, 150, 150],
];

/// Reads a stored surface.
pub fn load(project: &Path, name: &str) -> Result<Terrain, String> {
    let session = ols_session::Session::open(project).map_err(|e| e.to_string())?;
    let (points, triangles) = session.surface(name).map_err(|e| e.to_string())?;
    Terrain::from_mesh(points, triangles).map_err(|e| format!("{name}: {e}"))
}

/// The project's session (editing, or kept read-only).
fn session(model: &Model) -> Option<&ols_session::Session> {
    match &model.editing {
        Ok(e) => Some(&e.session),
        Err(_) => model.viewer_session.as_ref(),
    }
}

/// What a background job gives back.
enum Done {
    /// A surface read: the ground (`true`) or one shown.
    Loaded(String, bool, Result<Arc<Terrain>, String>),
    /// The surface of the checks read.
    Check(String, Result<Arc<Terrain>, String>),
    /// Surfaces kept in the project.
    Stored(Result<Vec<String>, String>),
    /// A file written.
    Written(Result<String, String>),
}

/// Building a surface from the LiDAR ground classes.
#[derive(Debug, Clone)]
pub struct LidarBuild {
    /// Name of the new surface.
    pub name: String,
    /// Lowest point per square cell of this side, m.
    pub cell: f64,
    /// Longest triangle side, m (no ground across larger gaps).
    pub max_edge: f64,
    /// Only near the line (the corridor of the model), else the whole
    /// cloud.
    pub corridor: bool,
}

impl Default for LidarBuild {
    fn default() -> Self {
        Self {
            name: "LiDAR ground".into(),
            cell: 1.0,
            max_edge: 50.0,
            corridor: true,
        }
    }
}

/// The surface manager window.
#[derive(Default)]
pub struct SurfacesWindow {
    /// Open.
    pub open: bool,
    /// The surfaces of the project (read when opened and after changes).
    pub list: Vec<SurfaceInfo>,
    /// A rename being typed: surface, new name.
    rename: Option<(String, String)>,
    /// A deletion waiting for confirmation.
    confirm_delete: Option<String>,
    /// Background jobs: what, since, the answer.
    jobs: Vec<(String, Instant, Receiver<Done>)>,
    /// The last result.
    pub status: Option<(String, bool)>,
    /// Import: name (empty: the surface's own), format (empty: from the
    /// extension), options (`KEY=VALUE` lines).
    import_name: String,
    import_format: String,
    import_options: String,
    /// Build from LiDAR.
    pub lidar: LidarBuild,
    /// An import waiting for the worker's answer.
    importing: bool,
    /// A surface to read as the ground (made active by an import).
    pending_ground: Option<String>,
}

impl SurfacesWindow {
    /// Opens the window (the list read again).
    pub fn show(&mut self, model: &Model) {
        self.open = true;
        self.refresh(model);
    }

    fn refresh(&mut self, model: &Model) {
        self.list = session(model)
            .and_then(|s| s.surfaces().ok())
            .unwrap_or_default();
    }

    /// Whether a job runs.
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty() || self.importing
    }

    fn spawn(
        &mut self,
        what: String,
        ctx: &egui::Context,
        f: impl FnOnce() -> Done + Send + 'static,
    ) {
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f());
            c.request_repaint();
        });
        self.jobs.push((what, Instant::now(), rx));
    }

    /// Loads surface `name` as the ground (`ground`) or as one shown.
    pub fn load(&mut self, model: &Model, name: &str, ground: bool, ctx: &egui::Context) {
        let file = model.sources.file.clone();
        let n = name.to_owned();
        self.spawn(format!("reading {name}"), ctx, move || {
            Done::Loaded(n.clone(), ground, load(&file, &n).map(Arc::new))
        });
    }

    /// Loads surface `name` for the checks.
    pub fn load_check(&mut self, model: &Model, name: &str, ctx: &egui::Context) {
        let file = model.sources.file.clone();
        let n = name.to_owned();
        self.spawn(format!("reading {name} for the checks"), ctx, move || {
            Done::Check(n.clone(), load(&file, &n).map(Arc::new))
        });
    }

    /// The project's surface for the checks (an edit: with a server, it
    /// follows everyone).
    fn set_check(&mut self, model: &mut Model, name: Option<String>, ctx: &egui::Context) {
        let op = ols_server::Op::SetSetting {
            key: CHECK_SETTING.into(),
            value: name.clone(),
        };
        match model.project_op("surface of the checks", op) {
            Ok(_) => {
                model.check_surface = name.clone();
                model.check_ground = None;
                if let Some(n) = name {
                    self.load_check(model, &n, ctx);
                }
            }
            Err(e) => self.status = Some((e.to_string(), true)),
        }
    }

    /// Takes finished jobs: surfaces read go into the model (the views
    /// follow: returns true when the ground or the shown surfaces
    /// changed).
    pub fn poll(&mut self, model: &mut Model, plugins: &mut crate::plugins::Plugins) -> bool {
        let mut changed = false;
        if self.importing
            && let Some(r) = plugins.surfaces_stored.take()
        {
            self.importing = false;
            self.stored(model, r);
        }
        let mut keep = Vec::new();
        for (what, t, rx) in std::mem::take(&mut self.jobs) {
            match rx.try_recv() {
                Ok(Done::Loaded(name, ground, r)) => match r {
                    Ok(terrain) => {
                        if ground {
                            if model.surface_settings.active.as_deref() == Some(name.as_str()) {
                                model.ground = Some(terrain);
                                model.ground_source = format!("surface {name}");
                                changed = true;
                            }
                        } else if let Some(color) = model.surface_settings.shown.get(&name) {
                            model.other_surfaces.retain(|o| o.name != name);
                            model.other_surfaces.push(OtherSurface {
                                name: name.clone(),
                                terrain,
                                color: *color,
                            });
                            changed = true;
                        }
                        self.status = Some((format!("{name} read in {:.1?}", t.elapsed()), false));
                    }
                    Err(e) => self.status = Some((format!("{what}: {e}"), true)),
                },
                Ok(Done::Check(name, r)) => match r {
                    Ok(terrain) => {
                        if model.check_surface.as_deref() == Some(name.as_str()) {
                            model.check_ground = Some(terrain);
                            changed = true;
                        }
                    }
                    Err(e) => self.status = Some((format!("{what}: {e}"), true)),
                },
                Ok(Done::Stored(r)) => self.stored(model, r),
                Ok(Done::Written(r)) => {
                    self.status = Some(match r {
                        Ok(t) => (t, false),
                        Err(e) => (e, true),
                    });
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => keep.push((what, t, rx)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        self.jobs = keep;
        changed
    }

    /// Surfaces kept: the list again; the first becomes the ground when
    /// there is none.
    fn stored(&mut self, model: &mut Model, r: Result<Vec<String>, String>) {
        match r {
            Ok(names) => {
                self.status = Some((format!("kept in the project: {}", names.join(", ")), false));
                // The model's session sees what another connection wrote.
                self.refresh(model);
                if model.surface_settings.active.is_none()
                    && let Some(first) = names.first()
                {
                    let mut s = model.surface_settings.clone();
                    s.active = Some(first.clone());
                    self.set(model, s);
                    self.pending_ground = Some(first.clone());
                }
            }
            Err(e) => self.status = Some((e, true)),
        }
    }

    /// This user's choice (remembered on this computer with the other
    /// view preferences, not in the project).
    fn set(&mut self, model: &mut Model, s: SurfaceSettings) {
        model.surface_settings = s;
    }
}

impl SurfacesWindow {
    /// The window. Returns true when the views must follow (the ground or
    /// the surfaces shown changed).
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &mut Model,
        plugins: &mut crate::plugins::Plugins,
    ) -> bool {
        let mut changed = self.poll(model, plugins);
        if let Some(name) = self.pending_ground.take() {
            self.load(model, &name, true, ctx);
        }
        if !self.open {
            return changed;
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let editable = model.can_edit();
        let joined = model.remote.is_some();
        let user = model
            .editing
            .as_ref()
            .map(|e| e.user.clone())
            .unwrap_or_default();
        let project = model.sources.file.clone();
        let is_project = project.extension().is_some_and(|e| e == "ols");
        let mut open = self.open;
        let mut action: Option<Action> = None;
        egui::Window::new("Terrain surfaces (TIN)")
            .open(&mut open)
            .default_width(880.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Triangulated surfaces kept in the project. The active one is the ground: \
                         drawn, used for clearances, profiles and placing structures. Others can be \
                         shown in their colour.",
                    )
                    .small()
                    .weak(),
                );
                ui.label(format!(
                    "Checks use: {}",
                    model
                        .check_surface
                        .as_deref()
                        .map_or("each user's ground".to_owned(), |n| format!("surface {n}"))
                ));
                ui.label(format!(
                    "Ground now: {}",
                    if model.ground.is_none() {
                        "none".to_owned()
                    } else if model.ground_source.is_empty() {
                        "shown".to_owned()
                    } else {
                        model.ground_source.clone()
                    }
                ));
                if !is_project {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        "A line file keeps no surfaces: open a project (.ols).",
                    );
                    return;
                }
                if let Err(why) = &editable {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("read-only: {why}"));
                }
                ui.separator();
                egui::Grid::new("surfaces")
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for h in [
                            "ground", "shown", "checks", "name", "points", "triangles", "source",
                            "kept", "",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for s in &self.list {
                            let active = model.surface_settings.active.as_deref()
                                == Some(s.name.as_str());
                            if ui
                                .radio(active, "")
                                .on_hover_text("The ground: drawn and used by the checks")
                                .clicked()
                                && !active
                            {
                                action = Some(Action::Activate(s.name.clone()));
                            }
                            let mut shown = model.surface_settings.shown.contains_key(&s.name);
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(!active, egui::Checkbox::new(&mut shown, ""))
                                    .on_hover_text("Show it too, in its colour")
                                    .changed()
                                {
                                    action = Some(Action::Show(s.name.clone(), shown));
                                }
                                if let Some(c) = model.surface_settings.shown.get(&s.name) {
                                    let mut col = *c;
                                    if ui.color_edit_button_srgb(&mut col).changed() {
                                        action = Some(Action::Color(s.name.clone(), col));
                                    }
                                }
                            });
                            let checks = model.check_surface.as_deref() == Some(s.name.as_str());
                            if ui
                                .add_enabled(editable.is_ok(), egui::RadioButton::new(checks, ""))
                                .on_hover_text(
                                    "The surface the checks (clearances) use, for everyone; a \
                                     second click: each user's ground",
                                )
                                .clicked()
                            {
                                action = Some(Action::Check((!checks).then(|| s.name.clone())));
                            }
                            match &mut self.rename {
                                Some((n, new)) if *n == s.name => {
                                    let r = ui.text_edit_singleline(new);
                                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                        action = Some(Action::Rename(n.clone(), new.clone()));
                                    }
                                }
                                _ => {
                                    ui.label(&s.name);
                                }
                            }
                            ui.label(s.points.to_string());
                            ui.label(s.triangles.to_string());
                            ui.label(egui::RichText::new(&s.source).small());
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} ({})",
                                    s.time.get(..16).unwrap_or(&s.time),
                                    s.user
                                ))
                                .small(),
                            );
                            ui.horizontal(|ui| {
                                let local = editable.is_ok() && !joined;
                                if ui
                                    .add_enabled(local, egui::Button::new("Rename").small())
                                    .clicked()
                                {
                                    self.rename = Some((s.name.clone(), s.name.clone()));
                                }
                                if ui.small_button("Export…").clicked() {
                                    action = Some(Action::Export(s.name.clone()));
                                }
                                if ui
                                    .add_enabled(local, egui::Button::new("Delete").small())
                                    .clicked()
                                {
                                    self.confirm_delete = Some(s.name.clone());
                                }
                            });
                            ui.end_row();
                        }
                    });
                if self.list.is_empty() {
                    ui.label("No surface in the project yet.");
                }
                ui.separator();
                let can_store = editable.is_ok() && !joined;
                if joined {
                    ui.label(
                        egui::RichText::new(
                            "Joined on a server: surfaces are imported and built on the hosting \
                             computer.",
                        )
                        .small()
                        .weak(),
                    );
                }
                ui.add_enabled_ui(can_store && !self.busy(), |ui| {
                    ui.strong("Import a file");
                    ui.horizontal(|ui| {
                        ui.label("name");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.import_name)
                                .hint_text("its own")
                                .desired_width(140.0),
                        );
                        let formats = plugins
                            .formats
                            .as_ref()
                            .map(|f| f.surfaces.clone())
                            .unwrap_or_default();
                        egui::ComboBox::from_label("format")
                            .selected_text(if self.import_format.is_empty() {
                                "from the extension".to_owned()
                            } else {
                                self.import_format.clone()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.import_format,
                                    String::new(),
                                    "from the extension",
                                );
                                for f in &formats {
                                    ui.selectable_value(
                                        &mut self.import_format,
                                        f.id.clone(),
                                        format!("{} (.{}) — {}", f.id, f.extension, f.origin),
                                    )
                                    .on_hover_text(&f.description);
                                }
                            });
                        ui.label("options");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.import_options)
                                .hint_text("max_edge=50")
                                .desired_width(120.0),
                        );
                        if ui
                            .button("Import…")
                            .on_hover_text(
                                "LandXML, OBJ, survey points (CSV/XYZ, triangulated) and the \
                                 formats of terrain plugins (Plugins › Plugin manager)",
                            )
                            .clicked()
                            && let Some(path) = rfd::FileDialog::new().pick_file()
                        {
                            action = Some(Action::Import(path));
                        }
                    });
                    ui.strong("Build from the LiDAR ground classes");
                    ui.horizontal(|ui| {
                        ui.label("name");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.lidar.name).desired_width(140.0),
                        );
                        ui.label("cell");
                        ui.add(
                            egui::DragValue::new(&mut self.lidar.cell)
                                .range(0.1..=50.0)
                                .speed(0.1)
                                .suffix(" m"),
                        )
                        .on_hover_text("The lowest ground point of each square cell is kept");
                        ui.label("longest side");
                        ui.add(
                            egui::DragValue::new(&mut self.lidar.max_edge)
                                .range(1.0..=10_000.0)
                                .suffix(" m"),
                        )
                        .on_hover_text("No ground across larger gaps in the data");
                        ui.checkbox(&mut self.lidar.corridor, "near the line only");
                        if ui
                            .add_enabled(model.cloud.is_some(), egui::Button::new("Build"))
                            .on_disabled_hover_text("The project has no point cloud")
                            .clicked()
                        {
                            action = Some(Action::Lidar);
                        }
                    });
                    if model.ground.is_some()
                        && model.surface_settings.active.is_none()
                        && ui
                            .button("Keep the ground shown as a surface")
                            .on_hover_text(
                                "The ground built when the model opened (LiDAR or survey file), \
                                 kept in the project: the next opening reads it instead",
                            )
                            .clicked()
                    {
                        action = Some(Action::KeepGround);
                    }
                });
                for (what, t, _) in &self.jobs {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("{what} — {:.1} s", t.elapsed().as_secs_f64()));
                    });
                }
                if self.importing {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("importing…");
                    });
                }
                if let Some((t, error)) = &self.status {
                    if *error {
                        ui.colored_label(egui::Color32::LIGHT_RED, t);
                    } else {
                        ui.label(t);
                    }
                }
            });
        self.open = open;
        if let Some(name) = self.confirm_delete.clone() {
            let mut answer = None;
            egui::Window::new("Delete the surface?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Delete surface \"{name}\" from the project? This cannot be undone \
                         (export it first to keep a copy)."
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            answer = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            answer = Some(false);
                        }
                    });
                });
            if let Some(yes) = answer {
                self.confirm_delete = None;
                if yes {
                    action = Some(Action::Delete(name));
                }
            }
        }
        if let Some(a) = action {
            changed |= self.act(a, model, plugins, &user, &project, ctx);
        }
        changed
    }

    /// Does what the user chose (also from tests).
    pub(crate) fn act(
        &mut self,
        a: Action,
        model: &mut Model,
        plugins: &mut crate::plugins::Plugins,
        user: &str,
        project: &Path,
        ctx: &egui::Context,
    ) -> bool {
        match a {
            Action::Activate(name) => {
                let mut s = model.surface_settings.clone();
                s.shown.remove(&name);
                s.active = Some(name.clone());
                model.other_surfaces.retain(|o| o.name != name);
                self.set(model, s);
                self.load(model, &name, true, ctx);
                true
            }
            Action::Check(name) => {
                self.set_check(model, name, ctx);
                true
            }
            Action::Show(name, on) => {
                let mut s = model.surface_settings.clone();
                if on {
                    let color = COLORS[s.shown.len() % COLORS.len()];
                    s.shown.insert(name.clone(), color);
                    self.set(model, s);
                    self.load(model, &name, false, ctx);
                    false
                } else {
                    s.shown.remove(&name);
                    model.other_surfaces.retain(|o| o.name != name);
                    self.set(model, s);
                    true
                }
            }
            Action::Color(name, color) => {
                let mut s = model.surface_settings.clone();
                s.shown.insert(name.clone(), color);
                for o in &mut model.other_surfaces {
                    if o.name == name {
                        o.color = color;
                    }
                }
                self.set(model, s);
                true
            }
            Action::Rename(from, to) => {
                self.rename = None;
                let to = to.trim().to_owned();
                if to == from {
                    return false;
                }
                let r = session(model).ok_or("no project".to_owned()).and_then(|s| {
                    s.rename_surface(user, &from, &to)
                        .map_err(|e| e.to_string())
                });
                match r {
                    Ok(()) => {
                        let mut s = model.surface_settings.clone();
                        if s.active.as_deref() == Some(from.as_str()) {
                            s.active = Some(to.clone());
                            model.ground_source = format!("surface {to}");
                        }
                        if let Some(c) = s.shown.remove(&from) {
                            s.shown.insert(to.clone(), c);
                        }
                        if model.check_surface.as_deref() == Some(from.as_str()) {
                            let op = ols_server::Op::SetSetting {
                                key: CHECK_SETTING.into(),
                                value: Some(to.clone()),
                            };
                            if model.project_op("surface of the checks", op).is_ok() {
                                model.check_surface = Some(to.clone());
                            }
                        }
                        for o in &mut model.other_surfaces {
                            if o.name == from {
                                o.name = to.clone();
                            }
                        }
                        self.set(model, s);
                        self.status = Some((format!("{from} renamed {to}"), false));
                    }
                    Err(e) => self.status = Some((e, true)),
                }
                self.refresh(model);
                false
            }
            Action::Delete(name) => {
                let r = session(model)
                    .ok_or("no project".to_owned())
                    .and_then(|s| s.delete_surface(user, &name).map_err(|e| e.to_string()));
                let mut changed = false;
                match r {
                    Ok(()) => {
                        let mut s = model.surface_settings.clone();
                        if s.active.as_deref() == Some(name.as_str()) {
                            s.active = None;
                            model.ground = None;
                            model.ground_source.clear();
                            changed = true;
                        }
                        if s.shown.remove(&name).is_some() {
                            model.other_surfaces.retain(|o| o.name != name);
                            changed = true;
                        }
                        if model.check_surface.as_deref() == Some(name.as_str()) {
                            self.set_check(model, None, ctx);
                        }
                        self.set(model, s);
                        self.status = Some((format!("{name} deleted"), false));
                    }
                    Err(e) => self.status = Some((e, true)),
                }
                self.refresh(model);
                changed
            }
            Action::Export(name) => {
                if let Some(out) = rfd::FileDialog::new()
                    .add_filter("LandXML", &["xml"])
                    .add_filter("OBJ", &["obj"])
                    .set_file_name(format!("{name}.xml"))
                    .save_file()
                {
                    let file = project.to_owned();
                    self.spawn(format!("writing {name}"), ctx, move || {
                        Done::Written(export(&file, &name, &out))
                    });
                }
                false
            }
            Action::Import(path) => {
                let options = self
                    .import_options
                    .split([',', '\n'])
                    .filter_map(|kv| {
                        kv.split_once('=')
                            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
                    })
                    .collect();
                plugins.store_surfaces(crate::plugins::SurfaceImport {
                    path,
                    format: (!self.import_format.is_empty()).then(|| self.import_format.clone()),
                    options,
                    project: project.to_owned(),
                    user: user.to_owned(),
                    name: (!self.import_name.trim().is_empty())
                        .then(|| self.import_name.trim().to_owned()),
                });
                self.importing = plugins.storing;
                false
            }
            Action::Lidar => {
                let (Some(cloud), scheme) = (model.cloud.clone(), model.scheme.clone()) else {
                    return false;
                };
                let build = self.lidar.clone();
                let corridor = build.corridor.then(|| {
                    let line = &model.line;
                    let polylines: Vec<Vec<[f64; 2]>> = line
                        .sections
                        .iter()
                        .map(|s| {
                            s.points
                                .iter()
                                .filter_map(|p| line.attachment(p))
                                .map(|a| [a.wire_point[0], a.wire_point[1]])
                                .collect()
                        })
                        .collect();
                    ols_lidar::Corridor::new(&polylines, model.sources.corridor)
                });
                let (file, user) = (project.to_owned(), user.to_owned());
                self.spawn(format!("building {}", build.name), ctx, move || {
                    Done::Stored(build_from_lidar(
                        &cloud, &scheme, corridor, &build, &file, &user,
                    ))
                });
                false
            }
            Action::KeepGround => {
                let Some(g) = model.ground.clone() else {
                    return false;
                };
                let source = if model.ground_source.is_empty() {
                    "the ground shown".to_owned()
                } else {
                    model.ground_source.clone()
                };
                let (file, user) = (project.to_owned(), user.to_owned());
                self.spawn("keeping the ground".into(), ctx, move || {
                    Done::Stored(keep(&file, &user, "ground", &source, &g))
                });
                false
            }
        }
    }
}

/// A user's choice in the window.
pub(crate) enum Action {
    Activate(String),
    Check(Option<String>),
    Show(String, bool),
    Color(String, [u8; 3]),
    Rename(String, String),
    Delete(String),
    Export(String),
    Import(PathBuf),
    Lidar,
    KeepGround,
}

/// Writes a stored surface as LandXML or OBJ (by the extension).
fn export(project: &Path, name: &str, out: &Path) -> Result<String, String> {
    let t = load(project, name)?;
    let text = match out.extension().and_then(|e| e.to_str()) {
        Some("obj") => ols_terrain::formats::write_obj(name, &t),
        _ => ols_terrain::formats::write_landxml(&[(name, &t)]),
    };
    std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(format!("{name} written to {}", out.display()))
}

/// Keeps a surface under a free name like `base`.
fn keep(
    project: &Path,
    user: &str,
    base: &str,
    source: &str,
    t: &Terrain,
) -> Result<Vec<String>, String> {
    let session = ols_session::Session::open(project).map_err(|e| e.to_string())?;
    let taken: Vec<String> = session
        .surfaces()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|i| i.name)
        .collect();
    let mut name = base.to_owned();
    let mut k = 2;
    while taken.contains(&name) {
        name = format!("{base} ({k})");
        k += 1;
    }
    session
        .put_surface(user, &name, source, t.points(), t.triangles())
        .map_err(|e| e.to_string())?;
    session.checkpoint().map_err(|e| e.to_string())?;
    Ok(vec![name])
}

/// The ground classes of the point cloud (lowest point per cell),
/// triangulated, kept in the project. Exit clause: 10 minutes of decoding.
fn build_from_lidar(
    cloud: &str,
    scheme: &ols_lidar::ClassScheme,
    corridor: Option<ols_lidar::Corridor>,
    build: &LidarBuild,
    project: &Path,
    user: &str,
) -> Result<Vec<String>, String> {
    let (file, name) = match cloud.split_once('#') {
        Some((f, n)) => (f.to_owned(), Some(n.to_owned())),
        None => (cloud.to_owned(), None),
    };
    let source = ols_pointcloud::CloudSource::open(Path::new(&file), name.as_deref())
        .map_err(|e| e.to_string())?;
    let mut is_ground = [false; 256];
    for c in scheme.codes_with_role("ground") {
        is_ground[c as usize] = true;
    }
    let mut cells: std::collections::HashMap<(i64, i64), usize> = std::collections::HashMap::new();
    let mut points: Vec<[f64; 3]> = Vec::new();
    let cell = build.cell.max(0.1);
    let budget = ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
    ols_pointcloud::PointSource::visit(&source, None, None, &budget, &mut |x, y, z, c| {
        if !is_ground[c as usize] || corridor.as_ref().is_some_and(|k| !k.contains(x, y)) {
            return;
        }
        let key = ((x / cell).floor() as i64, (y / cell).floor() as i64);
        match cells.get(&key) {
            Some(&i) if points[i][2] <= z => {}
            Some(&i) => points[i] = [x, y, z],
            None => {
                cells.insert(key, points.len());
                points.push([x, y, z]);
            }
        }
    })
    .map_err(|e| e.to_string())?;
    if points.len() < 3 {
        return Err("fewer than three ground points (classes with the ground role)".into());
    }
    let t = Terrain::from_xyz(&points, Some(build.max_edge)).map_err(|e| e.to_string())?;
    let source = format!(
        "LiDAR ground classes, {} m cells, sides ≤ {} m{}",
        cell,
        build.max_edge,
        if corridor.is_some() {
            ", near the line"
        } else {
            ""
        }
    );
    keep(project, user, build.name.trim(), &source, &t)
}
