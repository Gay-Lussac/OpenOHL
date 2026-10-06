// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The application: loads a model in the background, then shows it in
//! dockable views that share the weather case and the selection.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use eframe::egui_wgpu;
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};

use crate::model::{self, Model, Sources};
use crate::plan::PlanView;
use crate::profile::ProfileView;
use crate::scene::Layer;
use crate::selection::Selection;
use crate::view3d::View3d;

/// The pages (tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    /// 3D view.
    View3d,
    /// Plan view.
    Plan,
    /// Profile view.
    Profile,
    /// Properties of the selection.
    Properties,
    /// Layers, weather cases, load log.
    Model,
    /// Clearance violations.
    Clearances,
    /// A library editor.
    Library(crate::library::LibraryKind),
    /// A table of the line.
    Table(crate::tables::TableKind),
    /// The level-of-detail table of the 3D view.
    Lod,
    /// The Python console.
    Python,
    /// Who works on the project (server mode).
    People,
    /// The loads on the structures.
    Loads,
}

impl Tab {
    /// The pages of the View menu (library pages: the Libraries menu).
    pub const VIEWS: [Tab; 8] = [
        Tab::View3d,
        Tab::Plan,
        Tab::Profile,
        Tab::Properties,
        Tab::Model,
        Tab::Clearances,
        Tab::Lod,
        Tab::People,
    ];

    /// Title on its tab.
    pub fn title(self) -> &'static str {
        match self {
            Tab::View3d => "3D",
            Tab::Plan => "Plan",
            Tab::Profile => "Profile",
            Tab::Properties => "Properties",
            Tab::Model => "Model",
            Tab::Clearances => "Clearances",
            Tab::Library(k) => k.title(),
            Tab::Table(k) => k.title(),
            Tab::Lod => "Level of detail",
            Tab::Python => "Python console",
            Tab::People => "People",
            Tab::Loads => "Structure loads",
        }
    }
}

enum Message {
    Progress(String),
    Done(Result<Box<Model>, String>),
}

/// The "Move structure" window: the structure, where it was, where it is
/// being moved (previewed live), whether its base stays on the ground.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveDialog {
    /// Structure.
    pub id: String,
    /// Its position when the window opened.
    pub start: [f64; 3],
    /// The position typed.
    pub position: [f64; 3],
    /// The base follows the ground (z from the ground at E, N).
    pub on_ground: bool,
    /// The position shown (previewed) now.
    pub shown: [f64; 3],
}

/// A window of the Lines menu.
#[derive(Debug, Clone, PartialEq)]
pub enum LinesWindow {
    /// Save the working line as a project line: name, note.
    Create(String, String),
    /// Delete a project line.
    Delete(String),
    /// Make a project line the working line.
    Restore(String),
}

/// A plugin window asked for from the menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginRequest {
    /// Plugins › Plugin manager.
    Manager,
    /// File › Import into the line.
    Import,
    /// File › Export.
    Export,
}

/// What is shown once the model is loaded.
pub struct Ready {
    /// The model.
    pub model: Model,
    /// Weather case shown.
    pub case: usize,
    /// Selected object.
    pub selection: Selection,
    /// What can be clicked.
    pub snaps: crate::selection::Snaps,
    /// Visible 3D layers.
    pub visible: HashSet<Layer>,
    /// 3D view.
    pub view3d: View3d,
    /// Plan view.
    pub plan: PlanView,
    /// Profile view.
    pub profile: ProfileView,
    /// The "Go to structure" window, when open: the structure chosen.
    pub goto: Option<String>,
    /// What the last edit did (or why it was not done: `true`).
    pub status: Option<(String, bool)>,
    /// A deletion waiting for its confirmation: what it takes with it, and
    /// the deletion confirmed.
    pub confirm: Option<(Box<ols_session::Impact>, crate::edit::Action)>,
    /// The editor window open (structure, string, section).
    pub editor: Option<crate::editors::Form>,
    /// String wire: the strings right-clicked so far.
    pub chain: Vec<ols_line::SectionPoint>,
    /// String wire: the section whose points are picked again.
    pub restring: Option<String>,
    /// Tools: the hotbar (the active tool says what a click does).
    pub hotbar: crate::tools::Hotbar,
    /// The inventory window is open.
    pub inventory: bool,
    /// The "Move structure" window, when open.
    pub move_dialog: Option<MoveDialog>,
    /// Turns wheel events into hotbar steps.
    wheel: crate::tools::WheelSteps,
    /// Where tool windows open (the crosshair, or the mouse).
    pub popup_at: egui::Pos2,
    /// Counts the windows opened: a new one opens at `popup_at` again.
    popup_serial: u64,
    /// A tool window is open.
    popup_open: bool,

    /// A position being typed in the properties: (structure, position).
    pub edit_position: Option<(String, [f64; 3])>,
    /// Clearance checks.
    pub clearances: crate::clearance::Clearances,
    /// Library editor pages opened.
    pub libraries:
        std::collections::BTreeMap<crate::library::LibraryKind, crate::library::LibraryPage>,
    /// The "Display weather cases" window, when open: a tick per case.
    pub display_window: Option<DisplayWindow>,
    /// A window of the Lines menu, when open.
    pub lines_window: Option<LinesWindow>,
    /// The "Survey data display options" window, when open: its changes
    /// (applied with its Apply button).
    pub survey_window: Option<SurveyDraft>,
    /// Dragging over its classes: the state they all get (shown).
    pub survey_paint: Option<bool>,
    /// The Import LAS/LAZ window.
    pub survey_import: Option<crate::survey_io::ImportDialog>,
    /// The Export LAS/LAZ window.
    pub survey_export: Option<crate::survey_io::ExportDialog>,
    /// An import or export running.
    pub survey_job: Option<crate::survey_io::Job>,
    /// File › Export drawings: the window and the export running.
    pub drawings: crate::drawings::Drawings,
    /// Open the project again (after an import).
    pub reopen: bool,
    /// The plugin manager or a format window asked for (from the menus).
    pub plugin_request: Option<PluginRequest>,
    /// The Python console's page.
    pub console: crate::python::Console,
    /// Edit › History is open.
    pub history_open: bool,
    /// Edit › Versions: revisions, branches, merges, reviews.
    pub versions: crate::versions::VersionsWindow,
    /// Checks › Structure loads.
    pub loads: crate::loads::LoadsPage,
    /// Checks › Thermal rating.
    pub thermal: crate::thermal::ThermalWindow,
    /// Checks › Bill of materials.
    pub bom: crate::bom::BomWindow,
    /// Tools › Spot structures.
    pub spot: crate::spot::SpotWindow,
    /// Checks › Galloping ellipses.
    pub galloping: crate::galloping::GallopingWindow,
    /// Checks › Vegetation.
    pub vegetation: crate::vegetation::VegetationWindow,
    /// Checks › As-built wires.
    pub wirefit: crate::wirefit::WireFitWindow,
    /// Terrain › Terrain surfaces (TIN).
    pub surfaces: crate::surfaces::SurfacesWindow,
    /// Terrain › Survey data › Class colours, when open.
    pub class_window: Option<crate::class_colors::ClassWindow>,
    /// Parameters › Controls: the window of a view's controls, when open.
    pub controls_window: Option<ControlsPage>,
    /// Criteria › Creep-Stretch, when open: the weather cases chosen.
    pub creep_stretch_window: Option<ols_line::condition::CreepStretch>,
    /// Criteria › Bimetallic conductor model, when open: the choice.
    pub bimetallic_window: Option<ols_cables::BimetallicModel>,
    /// Criteria › Design code.
    pub design_code: crate::codes::DesignCodeWindow,
    /// Add jumper: the first string right-clicked.
    pub jumper_from: Option<ols_line::SectionPoint>,
    /// A page to bring to the front (with a table filter), from a tool.
    pub open_page: Option<(Tab, Option<String>)>,
    /// Table pages opened.
    pub tables: std::collections::BTreeMap<crate::tables::TableKind, crate::tables::TablePage>,
}

impl Ready {
    /// A model ready to show.
    pub fn new(model: Model) -> Self {
        let mut view3d = View3d::default();
        view3d.rebuild_all();
        let mut visible: HashSet<Layer> = Layer::ALL.into_iter().collect();
        visible.remove(&Layer::OtherCases);
        Self {
            case: 0,
            selection: Selection::None,
            snaps: crate::selection::Snaps::default(),
            visible,
            view3d,
            plan: PlanView::default(),
            profile: ProfileView::default(),
            goto: None,
            status: None,
            confirm: None,
            editor: None,
            chain: Vec::new(),
            restring: None,
            hotbar: crate::tools::Hotbar::default(),
            inventory: false,
            move_dialog: None,
            wheel: Default::default(),
            popup_at: egui::pos2(400.0, 200.0),
            popup_serial: 0,
            popup_open: false,

            edit_position: None,
            libraries: Default::default(),
            tables: Default::default(),
            display_window: None,
            lines_window: None,
            survey_window: None,
            survey_paint: None,
            survey_import: None,
            survey_export: None,
            survey_job: None,
            drawings: Default::default(),
            reopen: false,
            plugin_request: None,
            console: Default::default(),
            history_open: false,
            versions: Default::default(),
            loads: Default::default(),
            thermal: Default::default(),
            bom: Default::default(),
            spot: Default::default(),
            galloping: Default::default(),
            vegetation: Default::default(),
            wirefit: Default::default(),
            surfaces: Default::default(),
            class_window: None,
            controls_window: None,
            creep_stretch_window: None,
            bimetallic_window: None,
            design_code: Default::default(),
            jumper_from: None,
            open_page: None,
            clearances: crate::clearance::Clearances::new(
                &model.sources.file,
                model.sources.rules.as_deref(),
            ),
            model,
        }
    }
}

enum State {
    /// No file open: the start page.
    Empty,
    Loading {
        rx: Receiver<Message>,
        progress: Vec<String>,
    },
    Ready(Box<Ready>),
    Failed(String),
}

/// The OLS CAD application.
pub struct App {
    sources: Sources,
    /// Navigation scheme to start with.
    pub controls: crate::controls::Controls,
    /// What can be clicked, to start with.
    pub snaps: crate::selection::Snaps,
    /// Hotbar to start with.
    pub hotbar: crate::tools::Hotbar,
    /// The quick bar under the menus.
    pub quickbar: crate::menu::QuickBar,
    /// Level of detail of the 3D view.
    pub lod: crate::lod::LodSettings,
    /// Files opened recently.
    pub recent: crate::files::Recent,
    /// What this user shows of each project (remembered locally).
    pub view_prefs: crate::model::ViewPrefsStore,
    /// The LiDAR class scheme (.olsc) used for every project (remembered).
    pub default_class_scheme: Option<PathBuf>,
    /// The plugin manager, and the formats of File › Import / Export.
    pub plugins: crate::plugins::Plugins,
    /// The server this application hosts.
    pub hosting: Option<ols_server::server::Server>,
    /// The project file hosted (opened again when hosting stops).
    hosted_file: Option<PathBuf>,
    /// File › Host this project.
    pub host_window: Option<crate::remote::HostWindow>,
    /// File › Join a server.
    pub join_window: Option<crate::remote::JoinWindow>,
    /// File › New project from another program.
    pub import_project: Option<crate::files::ImportProjectWindow>,
    /// What the reader said of the project it created.
    pub import_notes: Option<(PathBuf, Vec<String>)>,
    /// Hosting: joining its own server.
    hosting_join: Option<std::sync::mpsc::Receiver<crate::remote::JoinEvent>>,
    /// Joined: given to the model once it is loaded.
    pub pending_remote: Option<Box<crate::remote::Remote>>,
    /// Where joined projects are kept.
    pub joined_dir: PathBuf,
    /// Options for files opened from the File menu (point budget, user…).
    pub template: Sources,
    /// What the File menu asked for, done after the frame.
    pub file_action: Option<crate::files::FileAction>,
    /// The New project window.
    new_project: Option<crate::files::NewProject>,
    /// An action waiting for "discard the unsaved changes?".
    confirm_discard: Option<crate::files::FileAction>,
    /// The "Customize the quick bar" window is open.
    customize: bool,
    state: State,
    dock: DockState<Tab>,
    render_state: Option<egui_wgpu::RenderState>,
}

/// The default arrangement: 3D and plan in the middle, properties right,
/// model and layers left, profile below.
pub fn default_dock() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::View3d, Tab::Plan]);
    let tree = dock.main_surface_mut();
    let [centre, _] = tree.split_right(NodeIndex::root(), 0.74, vec![Tab::Properties]);
    let [centre, _] = tree.split_left(centre, 0.2, vec![Tab::Model, Tab::Clearances]);
    tree.split_below(centre, 0.66, vec![Tab::Profile]);
    dock
}

impl App {
    /// No file open (the start page); files opened later take `template`'s
    /// options (point budget, corridor, user).
    pub fn empty(template: Sources, render_state: Option<egui_wgpu::RenderState>) -> Self {
        Self {
            sources: template.clone(),
            state: State::Empty,
            dock: default_dock(),
            render_state,
            controls: crate::controls::Controls::default(),
            snaps: crate::selection::Snaps::default(),
            hotbar: crate::tools::Hotbar::default(),
            quickbar: Default::default(),
            lod: Default::default(),
            recent: Default::default(),
            view_prefs: Default::default(),
            default_class_scheme: None,
            plugins: Default::default(),
            hosting: None,
            hosted_file: None,
            host_window: None,
            join_window: None,
            import_project: None,
            import_notes: None,
            hosting_join: None,
            pending_remote: None,
            joined_dir: crate::remote::default_joined_dir(),
            template: Sources {
                file: PathBuf::new(),
                library: None,
                structures: None,
                insulators: None,
                weather: None,
                terrain: None,
                pointcloud: None,
                rules: None,
                ..template
            },
            file_action: None,
            new_project: None,
            confirm_discard: None,
            customize: false,
        }
    }

    /// Loads `sources` in the background (the file open is closed first).
    pub fn start(&mut self, mut sources: Sources, ctx: &egui::Context) {
        self.close();
        if sources.view.is_none() {
            sources.view = self.view_prefs.get(&sources.file);
        }
        // The class scheme: given, else this project's choice (an empty
        // path: the point cloud's own), else the one for every project.
        if sources.class_scheme.is_none() {
            sources.class_scheme = sources
                .view
                .as_ref()
                .and_then(|v| v.class_scheme.clone())
                .or_else(|| self.default_class_scheme.clone());
        }
        if !sources.file.as_os_str().is_empty() {
            self.recent.add(&sources.file);
        }
        let (tx, rx) = channel();
        let s = sources.clone();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let report = |m: String| {
                let _ = tx.send(Message::Progress(m));
                c.request_repaint();
            };
            let result = model::load(s, &report).map(Box::new);
            let _ = tx.send(Message::Done(result));
            c.request_repaint();
        });
        let name = sources
            .file
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{name} — OLS CAD")));
        self.sources = sources;
        self.state = State::Loading {
            rx,
            progress: Vec::new(),
        };
    }

    /// Closes the file open: the project is written through, the view
    /// settings are kept for the next file; the start page shows.
    pub fn close(&mut self) {
        if let State::Ready(r) = &self.state {
            if let Ok(e) = &r.model.editing
                && let Err(err) = e.session.checkpoint()
            {
                log::warn!("closing: {err}");
            }
            self.view_prefs
                .set(&r.model.sources.file, r.model.view_prefs());
            self.controls = r.view3d.controls.clone();
            self.snaps = r.snaps.clone();
            self.hotbar = r.hotbar.clone();
        }
        self.state = State::Empty;
    }

    /// Whether the file open has changes not saved yet (library pages,
    /// table edits waiting for Apply).
    fn unsaved(&self) -> Vec<String> {
        let State::Ready(r) = &self.state else {
            return Vec::new();
        };
        let mut out: Vec<String> = r
            .libraries
            .iter()
            .filter(|(_, p)| p.dirty())
            .map(|(k, _)| format!("{} library page", k.title()))
            .collect();
        out.extend(
            r.tables
                .iter()
                .filter(|(_, p)| p.pending() > 0)
                .map(|(k, p)| format!("{} table: {} changes not applied", k.title(), p.pending())),
        );
        out
    }

    /// Does what the File menu asked for; `confirmed`: unsaved changes may
    /// go.
    fn file_action(
        &mut self,
        ctx: &egui::Context,
        action: crate::files::FileAction,
        confirmed: bool,
    ) {
        use crate::files::FileAction;
        let leaves = matches!(
            action,
            FileAction::Open(_) | FileAction::Close | FileAction::Quit | FileAction::Leave
        );
        if leaves && !confirmed && !self.unsaved().is_empty() {
            self.confirm_discard = Some(action);
            return;
        }
        if matches!(
            action,
            FileAction::Open(_) | FileAction::Close | FileAction::Quit | FileAction::New
        ) {
            self.stop_hosting();
        }
        match action {
            FileAction::Join => {
                self.join_window = Some(crate::remote::JoinWindow {
                    user: crate::files::user(),
                    ..Default::default()
                });
            }
            FileAction::Host => self.host_window = Some(Default::default()),
            FileAction::ImportProject => {
                self.import_project = Some(Default::default());
            }
            FileAction::Leave => {
                let hosted = self.hosted_file.take();
                self.stop_hosting();
                self.close();
                match hosted {
                    Some(file) => self.start(
                        Sources {
                            file,
                            ..self.template.clone()
                        },
                        ctx,
                    ),
                    None => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Title("OLS CAD".into()));
                    }
                }
            }
            FileAction::New => self.new_project = Some(Default::default()),
            FileAction::Open(path) => {
                if let Some(file) = path.or_else(crate::files::pick_file) {
                    self.start(
                        Sources {
                            file,
                            ..self.template.clone()
                        },
                        ctx,
                    );
                }
            }
            FileAction::Close => {
                self.close();
                ctx.send_viewport_cmd(egui::ViewportCommand::Title("OLS CAD".into()));
            }
            FileAction::Quit => {
                self.close();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Stops the server this application hosts (if it does).
    fn stop_hosting(&mut self) {
        self.hosting_join = None;
        if let Some(s) = self.hosting.take() {
            // The replica's connection goes first.
            if let State::Ready(r) = &mut self.state {
                r.model.remote = None;
            }
            s.stop();
        }
    }

    /// Hosts the project open: a server on its file, joined by this
    /// application like any client.
    fn start_hosting(&mut self, ctx: &egui::Context) {
        let Some(w) = &mut self.host_window else {
            return;
        };
        let State::Ready(r) = &self.state else {
            return;
        };
        let file = r.model.sources.file.clone();
        let user = r
            .model
            .editing
            .as_ref()
            .map(|e| e.user.clone())
            .unwrap_or_else(|_| crate::files::user());
        if let Ok(e) = &r.model.editing {
            let _ = e.session.checkpoint();
        }
        let options = ols_server::server::Options {
            bind: std::net::SocketAddr::new(w.bind, w.port),
            advertise: None,
            code: None,
            newcomers: w.newcomers,
            web: w.web.then(|| std::net::SocketAddr::new(w.bind, w.web_port)),
        };
        match ols_server::server::Server::start(&file, options) {
            Ok(server) => {
                let mut own = server.invitation_for(&user);
                own.address = format!("127.0.0.1:{}", server.local_addr.port());
                self.hosting_join = Some(crate::remote::start_join(
                    own,
                    user,
                    self.joined_dir.clone(),
                    true,
                    ctx.clone(),
                ));
                self.hosting = Some(server);
                self.hosted_file = Some(file);
                self.host_window = None;
            }
            Err(e) => w.error = Some(e),
        }
    }

    /// Joined (from File › Join a server, or hosting): the replica opens,
    /// the connection is given to it once loaded.
    fn joined(
        &mut self,
        file: PathBuf,
        mut remote: Box<crate::remote::Remote>,
        ctx: &egui::Context,
    ) {
        if let Some(s) = &self.hosting {
            remote.invitation = s.invitation.clone();
            remote.secret = Some(s.secret().to_owned());
            remote.web = s
                .web_link(None)
                .and_then(|l| l.split_once('#').map(|(base, _)| base.to_owned()));
        }
        let user = remote.link.user.clone();
        let sources = Sources {
            file,
            user: Some(user),
            ..self.template.clone()
        };
        // Opening closes the file open (not the server).
        self.start(sources, ctx);
        self.pending_remote = Some(remote);
    }

    /// File › New project from another program: the window, the job on
    /// the format worker, the project opened with the reader's notes.
    fn import_project_windows(&mut self, ctx: &egui::Context) {
        if self.import_project.is_some() {
            self.plugins.want_formats();
            // The worker answers between frames (also without a project).
            if !matches!(self.state, State::Ready(_)) {
                let _ = self.plugins.poll(ctx);
            }
            let formats = self
                .plugins
                .formats
                .as_ref()
                .map(|f| f.projects.clone())
                .unwrap_or_default();
            let busy = self.plugins.creating;
            let mut open = true;
            let job = self
                .import_project
                .as_mut()
                .and_then(|w| w.ui(ctx, &mut open, &formats, busy));
            if let Some(job) = job {
                self.plugins.create_project(job);
            }
            match self.plugins.project_created.take() {
                Some(Ok((file, notes))) => {
                    self.import_project = None;
                    self.import_notes = Some((file.clone(), notes));
                    self.file_action = Some(crate::files::FileAction::Open(Some(file)));
                }
                Some(Err(e)) => {
                    if let Some(w) = &mut self.import_project {
                        w.error = Some(e);
                    }
                }
                None if !open => self.import_project = None,
                None => {}
            }
        }
        if let Some((file, notes)) = &self.import_notes {
            let mut open = true;
            egui::Window::new("Project created")
                .open(&mut open)
                .default_width(640.0)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "{} — what was read, left out or assumed:",
                        file.display()
                    ));
                    egui::ScrollArea::vertical()
                        .max_height(400.0)
                        .show(ui, |ui| {
                            for n in notes {
                                ui.label(format!("• {n}"));
                            }
                        });
                });
            if !open {
                self.import_notes = None;
            }
        }
    }

    /// The server-mode windows: Host, Join, hosting's own join.
    fn share_windows(&mut self, ctx: &egui::Context) {
        if self.host_window.is_some() {
            let mut open = true;
            let start = self
                .host_window
                .as_mut()
                .is_some_and(|w| w.ui(ctx, &mut open));
            if start {
                self.start_hosting(ctx);
            } else if !open {
                self.host_window = None;
            }
        }
        if let Some(w) = &mut self.join_window {
            let mut open = true;
            let done = w.ui(ctx, &mut open, &self.joined_dir.clone());
            if !open {
                self.join_window = None;
            }
            if let Some((file, remote)) = done {
                self.join_window = None;
                self.stop_hosting();
                self.joined(file, remote, ctx);
            }
        }
        if let Some(rx) = &self.hosting_join {
            let mut done = None;
            while let Ok(e) = rx.try_recv() {
                if let crate::remote::JoinEvent::Done(r) = e {
                    done = Some(r);
                }
            }
            match done {
                Some(Ok((file, remote))) => {
                    self.hosting_join = None;
                    self.joined(file, remote, ctx);
                }
                Some(Err(e)) => {
                    self.hosting_join = None;
                    self.hosted_file = None;
                    if let Some(s) = self.hosting.take() {
                        s.stop();
                    }
                    self.host_window = Some(crate::remote::HostWindow {
                        error: Some(format!("joining the server: {e}")),
                        ..Default::default()
                    });
                }
                None => {
                    egui::Window::new("Starting the server")
                        .collapsible(false)
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("copying the project for this application…");
                            });
                        });
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
    }

    /// An app showing an already loaded model (tests).
    #[cfg(test)]
    pub fn with_model(model: Model, render_state: Option<egui_wgpu::RenderState>) -> Self {
        Self {
            sources: model.sources.clone(),
            state: State::Ready(Box::new(Ready::new(model))),
            dock: default_dock(),
            render_state,
            controls: crate::controls::Controls::default(),
            snaps: crate::selection::Snaps::default(),
            hotbar: crate::tools::Hotbar::default(),
            quickbar: Default::default(),
            lod: Default::default(),
            recent: Default::default(),
            view_prefs: Default::default(),
            default_class_scheme: None,
            plugins: Default::default(),
            hosting: None,
            hosted_file: None,
            host_window: None,
            join_window: None,
            import_project: None,
            import_notes: None,
            hosting_join: None,
            pending_remote: None,
            joined_dir: crate::remote::default_joined_dir(),
            template: Sources::default(),
            file_action: None,
            new_project: None,
            confirm_discard: None,
            customize: false,
        }
    }

    /// Brings a page to the front (tests).
    #[cfg(test)]
    pub fn show_tab(&mut self, tab: Tab) {
        if let Some(path) = self.dock.find_tab(&tab) {
            let _ = self.dock.set_active_tab(path);
        }
    }

    /// Asks for a File menu action (tests).
    #[cfg(test)]
    pub fn file_action_for_test(&mut self, action: crate::files::FileAction) {
        self.file_action = Some(action);
    }

    /// Why loading failed, if it did (tests).
    #[cfg(test)]
    pub fn failure(&self) -> Option<&str> {
        match &self.state {
            State::Failed(e) => Some(e),
            _ => None,
        }
    }

    /// Whether no file is open (tests).
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        matches!(self.state, State::Empty)
    }

    /// The shown model, once loaded.
    #[cfg(test)]
    pub fn ready(&mut self) -> Option<&mut Ready> {
        match &mut self.state {
            State::Ready(r) => Some(r),
            _ => None,
        }
    }
}

struct Viewer<'a> {
    ready: &'a mut Ready,
    rs: Option<&'a egui_wgpu::RenderState>,
    lod: &'a mut crate::lod::LodSettings,
    /// The Python plugins' scripts (for the console).
    scripts: Vec<(String, PathBuf)>,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        let dirty = match tab {
            Tab::Library(k) => self.ready.libraries.get(k).is_some_and(|p| p.dirty()),
            Tab::Table(k) => self.ready.tables.get(k).is_some_and(|p| p.pending() > 0),
            _ => false,
        };
        if dirty {
            format!("{} *", tab.title()).into()
        } else {
            tab.title().into()
        }
    }

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn closeable(&mut self, _tab: &mut Tab) -> bool {
        true
    }

    fn on_close(&mut self, tab: &mut Tab) -> egui_dock::tab_viewer::OnCloseResponse {
        // A library page with unsaved changes stays open.
        if let Tab::Library(k) = tab
            && let Some(p) = self.ready.libraries.get_mut(k)
            && p.dirty()
        {
            p.message = Some(("Save or discard the changes before closing".into(), true));
            return egui_dock::tab_viewer::OnCloseResponse::Ignore;
        }
        if let Tab::Table(k) = tab
            && let Some(p) = self.ready.tables.get_mut(k)
            && p.pending() > 0
        {
            p.message = Some(("Apply or revert the changes before closing".into(), true));
            return egui_dock::tab_viewer::OnCloseResponse::Ignore;
        }
        egui_dock::tab_viewer::OnCloseResponse::Close
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        let r = &mut *self.ready;
        let before = r.selection.clone();
        let snaps = r.effective_snaps();
        let used = match tab {
            Tab::View3d => r.view3d.ui(
                ui,
                self.rs,
                &r.model,
                r.case,
                &mut r.selection,
                &r.visible,
                &snaps,
                self.lod,
            ),
            Tab::Plan => r.plan.ui(ui, &r.model, r.case, &mut r.selection, &snaps),
            Tab::Profile => r
                .profile
                .ui(ui, &r.model, r.case, &mut r.selection, &snaps)
                .map(|target| crate::selection::Use { target, at: None }),
            Tab::Properties => {
                let editable = r.model.can_edit().is_ok();
                let action = crate::properties::ui(
                    ui,
                    &r.model,
                    r.case,
                    &mut r.selection,
                    editable.then_some(&mut r.edit_position),
                );
                if let Some(a) = action {
                    r.perform(a);
                }
                None
            }
            Tab::Model => {
                model_panel(ui, r);
                None
            }
            Tab::Clearances => {
                clearance_panel(ui, r);
                None
            }
            Tab::Loads => {
                r.loads.poll();
                if let Some(id) = r.loads.ui(ui, &r.model) {
                    r.go_to(Selection::Structure(id));
                }
                None
            }
            Tab::People => {
                match crate::remote::people_ui(ui, &mut r.model) {
                    Some(crate::remote::PeopleAction::GoTo(c)) => {
                        if let Some(cam) = r.view3d.camera.as_mut() {
                            cam.target = c;
                        }
                        r.plan.centre_on(c);
                    }
                    Some(crate::remote::PeopleAction::Op(what, op)) => r.project_op(&what, op),
                    None => {}
                }
                None
            }
            Tab::Python => {
                if let Some(path) = r.console.ui(ui, &self.scripts) {
                    r.console.run_file(path);
                }
                None
            }
            Tab::Lod => {
                let view3d = &r.view3d;
                if self
                    .lod
                    .ui(ui, &|l| view3d.lod_stats(l), view3d.stream_stats())
                {
                    ui.ctx().request_repaint();
                }
                None
            }
            Tab::Table(kind) => {
                let kind = *kind;
                let page = r.tables.entry(kind).or_default();
                match page.ui(ui, kind, &r.model, r.case, &r.selection) {
                    Some(crate::tables::TableAction::Select(s)) => {
                        r.go_to(s);
                    }
                    Some(crate::tables::TableAction::Apply(commands)) => {
                        r.apply_table(kind, commands);
                    }
                    Some(crate::tables::TableAction::ShowCase(k))
                        if k < r.model.cases.len() && k != r.case =>
                    {
                        r.set_case(k);
                    }
                    Some(crate::tables::TableAction::ShowCase(_)) | None => {}
                }
                None
            }
            Tab::Library(kind) => {
                let kind = *kind;
                let sources = r.model.sources.clone();
                let page = r
                    .libraries
                    .entry(kind)
                    .or_insert_with(|| crate::library::LibraryPage::new(kind, &sources));
                if let Some(action) = page.ui(ui, &sources) {
                    r.library_saved(kind, action);
                }
                None
            }
        };
        if r.selection != before {
            r.view3d.dirty.insert(Layer::Selection);
        }
        // A right-click uses the held tool on what is there.
        if let Some(target) = used {
            r.use_tool(target);
        }
    }
}

impl Ready {
    /// What can be right-clicked now: the Snap menu's choice, and strings
    /// while a tool that works on them is held.
    pub fn effective_snaps(&self) -> crate::selection::Snaps {
        let mut s = self.snaps.clone();
        if self.hotbar.tool().picks_strings() {
            s.0.insert(crate::selection::Snap::Attachment);
        }
        if self.hotbar.tool().picks_alignment() {
            s.0.insert(crate::selection::Snap::AlignmentPoint);
        }
        s
    }

    /// Uses the held tool on what was right-clicked in a view: Select
    /// selects, Go to opens its window, the editing tools act on the object
    /// (or, Add structure, on the ground point).
    pub fn use_tool(&mut self, u: crate::selection::Use) {
        use crate::tools::Tool;
        let tool = self.hotbar.tool();
        let target = u.target;
        match tool {
            Tool::Select => {
                self.select(target);
                return;
            }
            Tool::GoTo => {
                self.goto = Some(match &target {
                    Selection::Structure(id) => id.clone(),
                    _ => self
                        .model
                        .line
                        .structures
                        .first()
                        .map(|s| s.id.clone())
                        .unwrap_or_default(),
                });
                return;
            }
            _ => {}
        }
        if let Err(why) = self.model.can_edit() {
            self.status = Some((format!("{}: read-only — {why}", tool.name()), true));
            return;
        }
        use crate::editors::{Form, SectionForm, StringForm, StructureForm};
        let model = &self.model;
        match (tool, &target) {
            (Tool::Move, Selection::Structure(id)) => {
                let id = id.clone();
                self.select(target);
                self.use_tool_on(&id);
            }
            (Tool::Delete, _) => {
                if let Some(action) = delete_action(&self.model.line, &target) {
                    self.select(target);
                    self.perform(action);
                } else {
                    self.hint(tool);
                }
            }
            (Tool::Edit, Selection::Structure(id)) => {
                self.editor = StructureForm::of(model, id).map(Form::Structure);
                self.select(target);
            }
            (
                Tool::Edit,
                Selection::Attachment {
                    structure,
                    set,
                    phase,
                },
            ) => {
                self.editor = StringForm::of(model, structure, *set, *phase).map(Form::String);
                self.select(target);
            }
            (Tool::Edit, Selection::AlignmentPoint { route, index }) => {
                self.editor =
                    crate::editors::AlignmentForm::of(model, route, *index).map(Form::Alignment);
                if self.editor.is_none() {
                    self.status = Some((
                        "Edit: this route follows its structures — give it points first (Add alignment point, or the Alignment points table)".into(),
                        true,
                    ));
                }
                self.select(target);
            }
            (Tool::Edit, Selection::Jumper(id)) => {
                // No window: its row in the Jumpers table.
                self.open_page = Some((
                    Tab::Table(crate::tables::TableKind::Jumpers),
                    Some(id.clone()),
                ));
                self.select(target);
            }
            (Tool::AddJumper, Selection::Structure(id)) => {
                let (jumpers, unpaired) = crate::editors::missing_jumpers(model, id);
                let id = id.clone();
                if jumpers.is_empty() {
                    self.status = Some((
                        format!(
                            "Add jumper: none missing at {id}{}",
                            if unpaired.is_empty() {
                                String::new()
                            } else {
                                format!(" ({})", unpaired.join("; "))
                            }
                        ),
                        !unpaired.is_empty(),
                    ));
                    return;
                }
                let n = jumpers.len();
                let commands = jumpers
                    .iter()
                    .map(|j| ols_session::Command::Put {
                        kind: ols_session::Kind::Jumper,
                        id: j.id.clone(),
                        value: serde_json::to_value(j).expect("a jumper"),
                        ord: None,
                    })
                    .collect();
                if self.perform(crate::edit::Action::Command {
                    what: format!("added {n} jumpers at {id}"),
                    command: ols_session::Command::Batch { commands },
                }) && !unpaired.is_empty()
                {
                    self.status = Some((
                        format!(
                            "added {n} jumpers at {id}; not joined: {}",
                            unpaired.join("; ")
                        ),
                        false,
                    ));
                }
            }
            (
                Tool::AddJumper,
                Selection::Attachment {
                    structure,
                    set,
                    phase,
                },
            ) => {
                let p = ols_line::SectionPoint {
                    structure: structure.clone(),
                    set: *set,
                    phase: *phase,
                };
                match self.jumper_from.take() {
                    None => {
                        self.status = Some((
                            format!(
                                "Add jumper from {structure} {set}:{phase}: right-click the other string (the same one again: cancel)"
                            ),
                            false,
                        ));
                        self.jumper_from = Some(p);
                    }
                    Some(from) if from == p => {
                        self.status = Some(("Add jumper: cancelled".into(), false));
                    }
                    Some(from) => match crate::editors::new_jumper(model, &from, &p, &[]) {
                        Ok(j) => {
                            let id = j.id.clone();
                            if self.perform(crate::edit::Action::Command {
                                what: format!("added jumper {id}"),
                                command: ols_session::Command::Put {
                                    kind: ols_session::Kind::Jumper,
                                    id: id.clone(),
                                    value: serde_json::to_value(&j).expect("a jumper"),
                                    ord: None,
                                },
                            }) {
                                self.select(Selection::Jumper(id));
                            }
                        }
                        Err(e) => self.status = Some((e, true)),
                    },
                }
            }
            (Tool::AddAlignment, _) => match u.at {
                Some(at) => match crate::editors::insert_alignment_point(model, at) {
                    Ok((what, command, route, index)) => {
                        if self.perform(crate::edit::Action::Command { what, command }) {
                            self.select(Selection::AlignmentPoint { route, index });
                        }
                    }
                    Err(e) => self.status = Some((e, true)),
                },
                None => {
                    self.status = Some((
                        "Add alignment point: right-click the ground (3D or plan view)".into(),
                        true,
                    ));
                }
            },
            (Tool::Edit, Selection::Span { section, .. }) => {
                self.editor = SectionForm::of(model, section).map(Form::Section);
                self.select(target);
            }
            (Tool::AddStructure, _) => match u.at {
                Some(at) => {
                    self.editor = Some(Form::Structure(StructureForm::new_at(model, at)));
                }
                None => {
                    self.status = Some((
                        "Add structure: right-click the ground (3D or plan view)".into(),
                        true,
                    ));
                }
            },
            (
                Tool::AddString,
                Selection::Structure(id) | Selection::Attachment { structure: id, .. },
            ) => {
                self.editor = StringForm::new_on(model, id).map(Form::String);
                self.select(Selection::Structure(id.clone()));
            }
            (
                Tool::StringWire,
                Selection::Attachment {
                    structure,
                    set,
                    phase,
                },
            ) => {
                let p = ols_line::SectionPoint {
                    structure: structure.clone(),
                    set: *set,
                    phase: *phase,
                };
                if self.chain.last() == Some(&p) {
                    return;
                }
                if self.chain.contains(&p) {
                    self.status = Some((
                        format!("String wire: {structure} {set}:{phase} is in the section already"),
                        true,
                    ));
                    return;
                }
                self.chain.push(p);
                self.update_draft();
                self.status = Some((
                    format!(
                        "String wire: {} points — right-click the next string, Enter: section, Backspace: undo a point, Escape: cancel",
                        self.chain.len()
                    ),
                    false,
                ));
            }
            _ => self.hint(tool),
        }
    }

    /// "Tool: what to right-click" in the status bar.
    fn hint(&mut self, tool: crate::tools::Tool) {
        self.status = Some((format!("{}: {}", tool.name(), tool.help()), true));
    }

    /// After a library page saved a file (or chose the weather cases): the
    /// model reads its libraries again and solves.
    pub fn library_saved(
        &mut self,
        kind: crate::library::LibraryKind,
        action: crate::library::PageAction,
    ) {
        use crate::library::PageAction;
        // The project keeps its libraries: the working copy saved goes in.
        let in_project = match self.model.store_libraries() {
            Ok(n) => n > 0,
            Err(e) => {
                self.status = Some((format!("keeping the libraries in the project: {e}"), true));
                return;
            }
        };
        let (what, previous) = match action {
            PageAction::Saved(p) if in_project => (
                format!(
                    "saved {} in the project",
                    p.file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                ),
                None,
            ),
            PageAction::Saved(p) => (format!("saved {}", p.display()), None),
            PageAction::UseForModel(p) => {
                let before = self.model.sources.clone();
                kind.use_for(&mut self.model.sources, &p);
                (
                    format!("{} from {}", kind.title().to_lowercase(), p.display()),
                    Some(before),
                )
            }
        };
        // A library chosen for the model must have the line's cables.
        let reloaded = self.model.reload_libraries().and_then(|report| {
            let line = &self.model.line;
            let mut missing: Vec<&str> = line
                .sections
                .iter()
                .map(|s| s.cable.as_str())
                .chain(line.jumpers.iter().map(|j| j.cable.as_str()))
                .filter(|c| self.model.library.get(c).is_none())
                .collect();
            missing.sort_unstable();
            missing.dedup();
            match (&previous, missing.is_empty()) {
                (Some(_), false) => Err(format!(
                    "the line's cables {} are not in it",
                    missing.join(", ")
                )),
                _ => Ok(report),
            }
        });
        match reloaded {
            Ok(report) => {
                log::info!("{what}: {report}");
                if previous.is_some() && self.case >= self.model.cases.len() {
                    self.case = 0;
                }
                if let (Ok(e), Some(p)) = (&self.model.editing, &self.model.sources.weather) {
                    crate::model::remember_weather(&e.session, p);
                }
                // The other pages list the files the model now reads.
                let sources = self.model.sources.clone();
                for (k, page) in self.libraries.iter_mut() {
                    if *k != kind {
                        page.files = k.files(&sources);
                    }
                }
                if let Some(page) = self.libraries.get_mut(&kind) {
                    page.message = Some((format!("{what}; {report}"), false));
                }
                self.status = Some((format!("{what}; {report}"), false));
            }
            Err(e) => {
                if let Some(before) = previous {
                    self.model.sources = before;
                    // Back to the libraries it had.
                    if let Err(e) = self.model.reload_libraries() {
                        log::warn!("reading the previous libraries again: {e}");
                    }
                }
                let m = format!("{what}, but the model does not load with it: {e}");
                log::warn!("{m}");
                if let Some(page) = self.libraries.get_mut(&kind) {
                    page.message = Some((m.clone(), true));
                }
                self.status = Some((m, true));
            }
        }
        if !self.selection_exists() {
            self.selection = Selection::None;
        }
        self.clearances.stale = true;
        self.refresh_views();
    }

    /// Selects `target` and brings the views to it.
    pub fn go_to(&mut self, target: Selection) {
        self.select(target);
        if let Some(f) = self.selection.focus(&self.model, self.case) {
            if let Some(cam) = self.view3d.camera.as_mut() {
                cam.target = f;
                cam.distance = cam.distance.min(150.0);
            }
            self.plan.centre_on(f);
        }
    }

    /// Makes a table's changes: the sections' display cases, the session
    /// commands as one, the file written; its pending changes go when done.
    pub fn apply_table(&mut self, kind: crate::tables::TableKind, changes: crate::tables::Changes) {
        let crate::tables::Changes {
            mut commands,
            file,
            display,
            ..
        } = changes;
        let mut done = true;
        let n = commands.len();
        if n > 0 {
            let (what, command) = if n == 1 {
                commands.remove(0)
            } else {
                (
                    format!("{} table: {n} changes", kind.title()),
                    ols_session::Command::Batch {
                        commands: commands.into_iter().map(|c| c.1).collect(),
                    },
                )
            };
            done = self.perform(crate::edit::Action::Command { what, command });
        }
        if done && let Some((path, text)) = file {
            done = self.write_table_file(&path, &text);
        }
        if done && !display.is_empty() {
            // Display cases set in the table: shown as each section's own.
            self.model.cases_shown = crate::model::CasesShown::PerSection;
            for (id, cases) in display {
                if cases.is_empty() {
                    self.model.display_cases.remove(&id);
                } else {
                    self.model.display_cases.insert(id, cases);
                }
            }
            if n == 0 {
                self.status = Some(("display weather cases set".into(), false));
            }
            self.refresh_views();
        }
        if let Some(page) = self.tables.get_mut(&kind) {
            if done {
                page.clear();
                page.message = self.status.clone();
            } else {
                page.message = self
                    .status
                    .clone()
                    .map(|(m, _)| (m, true))
                    .or(Some(("not applied".into(), true)));
            }
        }
    }

    /// Writes a table's file (weather cases), then the model reads its
    /// libraries again; an open editor page of that file reads it again.
    /// Whether it worked.
    pub fn write_table_file(&mut self, path: &std::path::Path, text: &str) -> bool {
        use crate::library::LibraryKind;
        if let Err(e) = std::fs::write(path, text) {
            self.status = Some((format!("{}: {e}", path.display()), true));
            return false;
        }
        let shown = self.model.shown_cases.clone();
        self.library_saved(
            LibraryKind::WeatherCases,
            crate::library::PageAction::Saved(path.to_path_buf()),
        );
        self.show_cases(shown);
        if let Some(page) = self.libraries.get_mut(&LibraryKind::WeatherCases)
            && page.file.as_deref() == Some(path)
            && !page.dirty()
        {
            page.open(path);
        }
        !self.status.as_ref().is_some_and(|s| s.1)
    }

    /// ⌘S / Ctrl+S: library pages with changes that read are saved (the
    /// model reads them again), the project file is written through; table
    /// changes stay pending (Apply makes them).
    pub fn save_all(&mut self) {
        let mut parts = Vec::new();
        let mut problems = Vec::new();
        let kinds: Vec<crate::library::LibraryKind> = self
            .libraries
            .iter()
            .filter(|(_, p)| p.dirty())
            .map(|(k, _)| *k)
            .collect();
        for k in kinds {
            let page = self.libraries.get_mut(&k).expect("listed");
            match page.save() {
                Ok(path) => {
                    parts.push(format!("{}", path.display()));
                    self.library_saved(k, crate::library::PageAction::Saved(path));
                }
                Err(e) => problems.push(format!("{}: {e}", k.title())),
            }
        }
        match &self.model.editing {
            Ok(e) => match e.session.checkpoint() {
                Ok(()) => parts.insert(0, "project file".into()),
                Err(err) => problems.push(format!("project file: {err}")),
            },
            Err(_) => problems.push("a line file is not saved here (open a project)".into()),
        }
        let pending: usize = self.tables.values().map(|p| p.pending()).sum();
        if pending > 0 {
            problems.push(format!(
                "{pending} table changes still pending — Apply them in their tables"
            ));
        }
        let text = format!(
            "saved: {}{}",
            if parts.is_empty() {
                "nothing".into()
            } else {
                parts.join(", ")
            },
            if problems.is_empty() {
                String::new()
            } else {
                format!(" — {}", problems.join("; "))
            }
        );
        log::info!("{text}");
        self.status = Some((text, !problems.is_empty()));
    }

    /// Shows weather case `k`.
    pub fn set_case(&mut self, k: usize) {
        self.show_cases(vec![k]);
    }

    /// Shows weather cases `cases` for the line (the first one reported
    /// and checked); the sections' own display cases are kept.
    pub fn show_cases(&mut self, cases: Vec<usize>) {
        let cases: Vec<usize> = cases
            .into_iter()
            .filter(|k| *k < self.model.cases.len())
            .collect();
        let Some(&first) = cases.first() else {
            return;
        };
        self.case = first;
        self.model.shown_cases = cases;
        self.clearances.stale = true;
        self.view3d.dirty.extend([
            Layer::Wires,
            Layer::OtherCases,
            Layer::Structures,
            Layer::Selection,
        ]);
        self.profile.invalidate();
    }

    /// Selects `target`.
    pub fn select(&mut self, target: Selection) {
        self.selection = target;
        self.view3d.dirty.insert(Layer::Selection);
    }

    /// The wire being strung, drawn.
    pub fn update_draft(&mut self) {
        self.view3d.draft = self
            .chain
            .iter()
            .filter_map(|p| self.model.line.attachment(p).map(|a| a.wire_point))
            .collect();
        self.view3d.dirty.insert(Layer::Draft);
    }

    /// Ends the wire being strung: its section's window (a new section, or
    /// the one whose points are picked again).
    pub fn finish_chain(&mut self) {
        use crate::editors::{Form, SectionForm};
        if self.chain.len() < 2 {
            self.status = Some(("String wire: right-click two strings or more".into(), true));
            return;
        }
        let points = std::mem::take(&mut self.chain);
        let form = match self.restring.take() {
            Some(id) => SectionForm::of(&self.model, &id).map(|mut f| {
                f.set_points(&self.model, self.case, points);
                f
            }),
            None => {
                let template = match &self.selection {
                    Selection::Span { section, .. } => Some(section.clone()),
                    _ => None,
                };
                Some(SectionForm::new_through(
                    &self.model,
                    self.case,
                    points,
                    template.as_deref(),
                ))
            }
        };
        self.editor = form.map(Form::Section);
        self.update_draft();
    }

    /// Gives up the wire being strung.
    pub fn cancel_chain(&mut self) {
        self.chain.clear();
        self.restring = None;
        self.update_draft();
        self.status = Some(("String wire: cancelled".into(), false));
    }

    /// The held Move or Delete tool used on structure `id`.
    pub fn use_tool_on(&mut self, id: &str) {
        use crate::tools::Tool;
        let tool = self.hotbar.tool();
        if tool.edits()
            && let Err(why) = self.model.can_edit()
        {
            self.status = Some((format!("{}: read-only — {why}", tool.name()), true));
            return;
        }
        match tool {
            Tool::Move => {
                let target = ols_server::lock_target(ols_session::Kind::Structure, id);
                if let Some(user) = self.model.locked_by_other(&target) {
                    self.status = Some((format!("{id} is being moved by {user}"), true));
                    return;
                }
                // A preview still open goes back first (it may be this one).
                self.cancel_move_preview();
                if let Some(p) = self.model.line.structure(id).map(|s| s.position) {
                    // The window frees the mouse when it opens ([`popups`]).
                    self.move_dialog = Some(MoveDialog {
                        id: id.to_owned(),
                        start: p,
                        position: p,
                        on_ground: false,
                        shown: p,
                    });
                }
            }
            Tool::Delete => {
                self.perform(crate::edit::Action::Delete {
                    kind: ols_session::Kind::Structure,
                    id: id.to_owned(),
                    confirm: false,
                });
            }
            _ => {}
        }
    }

    /// Ends a Move window's preview without moving.
    fn cancel_move_preview(&mut self) {
        if let Some(d) = self.move_dialog.take()
            && d.shown != d.start
        {
            self.perform(crate::edit::Action::CancelPreview { id: d.id });
        }
    }

    /// A project edit (library, user, project line, rollback…), reported
    /// in the status bar; the views follow.
    pub fn project_op(&mut self, what: &str, op: ols_server::Op) {
        self.status = Some(match self.model.project_op(what, op) {
            Ok(report) => {
                self.clearances.stale = true;
                (report, false)
            }
            Err(e) => (e.to_string(), true),
        });
        self.refresh_views();
    }

    /// Every frame: the weather cases shown (and the sections' own) are
    /// solved in the background when they are not yet; finished ones
    /// replace their placeholder.
    pub fn solving_frame(&mut self, ctx: &egui::Context) {
        let mut need = self.model.shown(self.case);
        need.push(self.case);
        need.extend(self.model.display_cases.values().flatten().copied());
        self.model.want(need, ctx);
        let done = self.model.poll_solving();
        if !done.is_empty() {
            self.status = Some((format!("solved: {}", done.join(", ")), false));
            self.clearances.stale = true;
            self.refresh_views();
        }
    }

    /// Server mode, every frame: what others did, what this user shows
    /// them, the locks of what they edit.
    pub fn remote_frame(&mut self) {
        if self.model.remote.is_none() {
            return;
        }
        if let Some(report) = self.model.poll_remote() {
            if !report.0.is_empty() {
                self.status = Some(report);
            }
            self.clearances.stale = true;
            self.refresh_views();
        }
        let presence = ols_server::Presence {
            selection: self.selection.describe(),
            camera: self.view3d.camera.as_ref().map(|c| c.target),
        };
        self.model.show_presence(presence);
        if let Some(r) = &self.model.remote {
            let peers: Vec<(String, [f64; 3])> = r
                .peers
                .iter()
                .filter(|p| p.user != r.link.user)
                .filter_map(|p| Some((p.user.clone(), p.presence.camera?)))
                .collect();
            if peers != self.plan.peers {
                self.plan.peers = peers;
            }
        }
        let wanted = self
            .move_dialog
            .iter()
            .map(|d| ols_server::lock_target(ols_session::Kind::Structure, &d.id))
            .collect();
        self.model.hold(wanted);
    }

    /// Performs an edit and updates the views; whether it was made.
    pub fn perform(&mut self, action: crate::edit::Action) -> bool {
        use crate::edit::{Action, EditError};
        let retry = action.confirmed();
        let result = match action {
            Action::PreviewMove { id, position } => {
                self.model.preview_move(self.case, &id, position);
                self.refresh_views();
                return true;
            }
            Action::CancelPreview { id } => {
                self.model.cancel_preview(self.case, &id);
                self.refresh_views();
                return true;
            }
            Action::Command { what, command } => self.model.apply(&what, command),
            Action::DeleteAttachment {
                structure,
                set,
                phase,
                confirm,
            } => self.model.apply(
                &format!("deleted string {set}:{phase} of {structure}"),
                ols_session::Command::DeleteAttachment {
                    structure,
                    set,
                    phase,
                    confirm,
                },
            ),
            Action::Move { id, position } => self.model.apply(
                &format!("moved {id}"),
                ols_session::Command::MoveStructure {
                    id: id.clone(),
                    position,
                },
            ),
            Action::Delete { kind, id, confirm } => self.model.apply(
                &format!("deleted {} {id}", kind.as_str()),
                ols_session::Command::Delete { kind, id, confirm },
            ),
            Action::Undo => self.model.undo(),
            Action::Redo => self.model.redo(),
        };
        let done = match result {
            Ok(report) => {
                log::info!("{report}");
                self.status = Some((report, false));
                self.confirm = None;
                self.clearances.stale = true;
                true
            }
            Err(EditError::NeedsConfirmation(impact)) => {
                self.confirm = retry.map(|a| (impact, a));
                false
            }
            Err(e) => {
                log::warn!("{e}");
                self.status = Some((e.to_string(), true));
                false
            }
        };
        self.edit_position = None;
        if !self.selection_exists() {
            self.selection = Selection::None;
        }
        self.chain
            .retain(|p| self.model.line.attachment(p).is_some());
        self.update_draft();
        self.refresh_views();
        done
    }

    /// After the line changed: the 3D layers of the line (not the ground
    /// and the LiDAR points: an edit does not change them, and indexing
    /// 5 million triangles took 0.3 s) and the profile again.
    fn refresh_views(&mut self) {
        self.view3d.dirty.extend(
            Layer::ALL
                .into_iter()
                .filter(|l| !matches!(l, Layer::Ground | Layer::Points)),
        );
        self.profile.invalidate();
    }

    /// Whether the selected object is still in the line.
    fn selection_exists(&self) -> bool {
        let line = &self.model.line;
        match &self.selection {
            Selection::None | Selection::LidarPoint(_) => true,
            Selection::Structure(id) => line.structure(id).is_some(),
            Selection::Jumper(id) => line.jumpers.iter().any(|j| &j.id == id),
            Selection::Span { section, span } => line
                .sections
                .iter()
                .any(|s| &s.id == section && *span + 1 < s.points.len()),
            Selection::Attachment {
                structure,
                set,
                phase,
            } => line
                .attachment(&ols_line::SectionPoint {
                    structure: structure.clone(),
                    set: *set,
                    phase: *phase,
                })
                .is_some(),
            Selection::AlignmentPoint { route, index } => {
                crate::selection::alignment_point(&self.model, route, *index).is_some()
            }
        }
    }

    /// Selects structure `id` and brings every view to it: the 3D camera
    /// within 150 m, the plan centred on it.
    /// Shows weather case `k` and goes to section or jumper `id` that
    /// failed in it: selected, the views brought to it (to its structure
    /// when it has no shape in that case).
    pub fn go_to_failure(&mut self, k: usize, id: &str) {
        if k != self.case && k < self.model.cases.len() {
            self.set_case(k);
        }
        let line = &self.model.line;
        let (target, structure) = if let Some(j) = line.jumpers.iter().find(|j| j.id == id) {
            (Selection::Jumper(j.id.clone()), j.from.structure.clone())
        } else if let Some(s) = line.sections.iter().find(|s| s.id == id) {
            (
                Selection::Span {
                    section: s.id.clone(),
                    span: 0,
                },
                s.points
                    .first()
                    .map(|p| p.structure.clone())
                    .unwrap_or_default(),
            )
        } else {
            return;
        };
        if self.selection_focus(&target).is_none() {
            self.go_to_structure(&structure);
        }
        self.go_to(target);
    }

    fn selection_focus(&self, target: &Selection) -> Option<[f64; 3]> {
        target.focus(&self.model, self.case)
    }

    pub fn go_to_structure(&mut self, id: &str) {
        self.selection = Selection::Structure(id.to_owned());
        self.view3d.dirty.insert(Layer::Selection);
        if let Some(f) = self.selection.focus(&self.model, self.case) {
            if let Some(cam) = self.view3d.camera.as_mut() {
                cam.target = f;
                cam.distance = cam.distance.min(150.0);
            }
            self.plan.centre_on(f);
        }
    }
}

/// The deletion of a right-clicked object: a structure, a string, a span's
/// section, a jumper.
fn delete_action(line: &ols_line::Line, target: &Selection) -> Option<crate::edit::Action> {
    use crate::edit::Action;
    use ols_session::Kind;
    let del = |kind, id: &String| Action::Delete {
        kind,
        id: id.clone(),
        confirm: false,
    };
    Some(match target {
        Selection::Structure(id) => del(Kind::Structure, id),
        Selection::Span { section, .. } => del(Kind::Section, section),
        Selection::Jumper(id) => del(Kind::Jumper, id),
        Selection::Attachment {
            structure,
            set,
            phase,
        } => Action::DeleteAttachment {
            structure: structure.clone(),
            set: *set,
            phase: *phase,
            confirm: false,
        },
        Selection::AlignmentPoint { route, index } => Action::Command {
            what: format!("deleted alignment point {} of {route}", index + 1),
            command: ols_session::Command::RemoveAlignmentPoint {
                route: crate::editors::route_key(line, route),
                index: *index,
            },
        },
        _ => return None,
    })
}

/// G opens "Go to structure" (not while typing).
fn goto_key(ctx: &egui::Context, r: &mut Ready) {
    let open_key = !ctx.text_edit_focused() && ctx.input(|i| i.key_pressed(egui::Key::G));
    if open_key && r.goto.is_none() {
        let current = match &r.selection {
            Selection::Structure(id) => Some(id.clone()),
            _ => None,
        };
        r.goto = current.or_else(|| r.model.line.structures.first().map(|s| s.id.clone()));
    }
}

/// When a tool window opens (Go to, Move, a deletion's confirmation): it
/// opens at the crosshair in first person (else at the mouse), and a
/// captured mouse is freed — a window needs the cursor.
fn popups(ctx: &egui::Context, r: &mut Ready) {
    let open = r.goto.is_some()
        || r.move_dialog.is_some()
        || r.confirm.is_some()
        || r.editor.is_some()
        || r.display_window.is_some()
        || r.lines_window.is_some();
    if open && !r.popup_open {
        let crosshair = r.view3d.rect.map(|x| x.center());
        let at = if r.view3d.captured {
            crosshair
        } else {
            ctx.pointer_latest_pos().or(crosshair)
        };
        r.popup_at = at.unwrap_or(egui::pos2(400.0, 200.0));
        r.popup_serial += 1;
        if r.view3d.captured {
            crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
        }
    }
    r.popup_open = open;
}

/// A tool window: opens next to [`Ready::popup_at`] (again each time it
/// opens), can be dragged anywhere, also partly off the screen.
fn tool_window(r: &Ready, key: &str, title: impl Into<egui::WidgetText>) -> egui::Window<'static> {
    egui::Window::new(title)
        .id(egui::Id::new((key.to_owned(), r.popup_serial)))
        .default_pos(r.popup_at + egui::vec2(16.0, 16.0))
        .collapsible(false)
        .resizable(false)
        .constrain(false)
}

/// The Go to window: Enter goes, Escape closes.
fn goto_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut chosen) = r.goto.clone() else {
        return;
    };
    let mut go = false;
    let mut close = false;
    tool_window(r, "goto", "Go to structure").show(ctx, |ui| {
        egui::ComboBox::from_id_salt("goto-structure")
            .selected_text(&chosen)
            .width(220.0)
            .height(400.0)
            .show_ui(ui, |ui| {
                for s in &r.model.line.structures {
                    let text = match &s.structure_type {
                        Some(t) => format!("{}  ({t})", s.id),
                        None => s.id.clone(),
                    };
                    ui.selectable_value(&mut chosen, s.id.clone(), text);
                }
            });
        ui.horizontal(|ui| {
            go |= ui.button("Go").clicked();
            close |= ui.button("Close").clicked();
        });
    });
    go |= ctx.input(|i| i.key_pressed(egui::Key::Enter));
    close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if go {
        r.go_to_structure(&chosen);
        r.goto = None;
    } else if close {
        r.goto = None;
    } else {
        r.goto = Some(chosen);
    }
}

/// Shortcuts (not while typing in a text field): ⌘Z undo, Shift+⌘Z redo,
/// Delete or Backspace deletes the selected structure, M holds the Move
/// tool, E opens the inventory, 1–9 choose a hotbar slot; the wheel scrolls
/// the hotbar while the mouse is captured.
fn edit_keys(ctx: &egui::Context, r: &mut Ready) {
    use crate::tools::Tool;
    // The wire being strung: Enter ends it, Backspace takes the last point
    // back, Escape (with a free mouse) gives it up.
    let windows = r.editor.is_some() || r.goto.is_some() || r.move_dialog.is_some();
    if !r.chain.is_empty() && !ctx.text_edit_focused() && !windows {
        let captured = r.view3d.captured;
        let (enter, back, escape) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            (
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Backspace),
                !captured && i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if enter {
            r.finish_chain();
        } else if escape {
            r.cancel_chain();
        } else if back {
            r.chain.pop();
            r.update_draft();
        }
    }
    if !ctx.text_edit_focused() {
        // Redo first: Shift+⌘Z also matches ⌘Z when shift is ignored.
        let (save, redo, undo, delete, move_key, inventory, slot) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            let save = i.consume_key(Modifiers::COMMAND, Key::S);
            let redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
            let undo = !redo && i.consume_key(Modifiers::COMMAND, Key::Z);
            let digits = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ];
            (
                save,
                redo,
                undo,
                i.consume_key(Modifiers::NONE, Key::Delete)
                    || i.consume_key(Modifiers::NONE, Key::Backspace),
                i.consume_key(Modifiers::NONE, Key::M),
                i.consume_key(Modifiers::NONE, Key::E),
                digits
                    .iter()
                    .position(|k| i.consume_key(Modifiers::NONE, *k)),
            )
        });
        let editable = r.model.can_edit().is_ok();
        if save {
            r.save_all();
        }
        if undo && editable {
            r.perform(crate::edit::Action::Undo);
        }
        if redo && editable {
            r.perform(crate::edit::Action::Redo);
        }
        if move_key {
            r.hotbar.hold(Tool::Move);
        }
        if r.inventory && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            r.inventory = false;
        }
        if inventory {
            r.inventory = !r.inventory;
            if r.inventory && r.view3d.captured {
                crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
            }
        }
        if let Some(k) = slot {
            r.hotbar.active = k;
        }
        if delete
            && editable
            && let Some(action) = delete_action(&r.model.line, &r.selection)
        {
            r.perform(action);
        }
    }
    // Captured mouse: the wheel (without Ctrl) scrolls the hotbar, wheel
    // down to the next slot.
    if r.view3d.captured {
        let (lines, points, now) = ctx.input(|i| {
            let (mut lines, mut points) = (0.0f32, 0.0f32);
            for e in &i.events {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = e
                    && !modifiers.ctrl
                {
                    log::debug!("hotbar wheel: {unit:?} {delta:?}");
                    match unit {
                        egui::MouseWheelUnit::Point => points += delta.y,
                        _ => lines += delta.y.signum(),
                    }
                }
            }
            (lines, points, i.time)
        });
        if let Some(step) = r.wheel.step(lines, points, now) {
            r.hotbar.scroll(step);
        }
    }
}

/// The tool windows: hotbar, inventory, Go to, Move structure, the
/// editor, the confirmation of a deletion.
fn tool_windows(ctx: &egui::Context, r: &mut Ready) {
    // At the bottom of the 3D view, only while it is shown (and while the
    // inventory arranges it).
    let shown = r.view3d.drawn;
    r.view3d.drawn = false;
    r.plan.drawn = None;
    if shown || r.inventory {
        let at = match (shown, r.view3d.rect) {
            (true, Some(v)) => v.center_bottom(),
            _ => ctx.content_rect().center_bottom(),
        };
        crate::tools::hotbar_ui(ctx, &mut r.hotbar, at, r.inventory);
    }
    crate::tools::inventory_ui(ctx, &mut r.inventory, &mut r.hotbar);
    goto_window(ctx, r);
    move_window(ctx, r);
    editor_window(ctx, r);
    display_window(ctx, r);
    survey_window(ctx, r);
    survey_io_windows(ctx, r);
    r.drawings.show(ctx, &r.model, r.case);
    controls_window(ctx, r);
    creep_stretch_window(ctx, r);
    bimetallic_window(ctx, r);
    lines_window(ctx, r);
    crate::thermal::window(ctx, &mut r.thermal, &r.model, r.clearances.rules.as_ref());
    r.bom.show(ctx, &r.model);
    r.spot.show(ctx, &r.model);
    let Some((impact, confirmed)) = r.confirm.clone() else {
        return;
    };
    let mut decision: Option<bool> = None;
    tool_window(r, "confirm", format!("Delete {}?", impact.what())).show(ctx, |ui| {
        ui.label("This also deletes or changes:");
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for (title, items) in impact.groups() {
                    if items.is_empty() {
                        continue;
                    }
                    ui.label(egui::RichText::new(format!("{title} ({})", items.len())).strong());
                    for it in items {
                        ui.label(egui::RichText::new(format!("  {it}")).small());
                    }
                }
            });
        ui.label("It is one log entry: undo brings everything back.");
        ui.horizontal(|ui| {
            if ui.button("Delete all").clicked() {
                decision = Some(true);
            }
            if ui.button("Cancel").clicked() {
                decision = Some(false);
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        decision = Some(false);
    }
    match decision {
        Some(true) => {
            r.perform(confirmed);
        }
        Some(false) => r.confirm = None,
        None => {}
    }
}

/// The Lines menu: the project lines (frozen states of the line, e.g. as
/// built and as planned) to show next to the working line, to make the
/// working line; create and delete them.
/// The Terrain menu: the ground surface (TIN) and the LiDAR points, with
/// the classes drawn.
fn terrain_menu(ui: &mut egui::Ui, r: &mut Ready) {
    let layer = |ui: &mut egui::Ui, layer: Layer, name: &str, r: &mut Ready| {
        let mut on = r.visible.contains(&layer);
        if ui.checkbox(&mut on, name).changed() {
            if on {
                r.visible.insert(layer);
            } else {
                r.visible.remove(&layer);
            }
        }
    };
    ui.label(egui::RichText::new("Ground surface").small().weak());
    if ui
        .button("Terrain surfaces (TIN)…")
        .on_hover_text(
            "The surfaces kept in the project: import, build from LiDAR, the active one (the \
             ground), others shown, export, delete",
        )
        .clicked()
    {
        r.surfaces.show(&r.model);
        ui.close();
    }
    if r.model.ground.is_some() {
        layer(ui, Layer::Ground, "Ground (active surface) shown", r);
    } else {
        ui.add_enabled(false, egui::Button::new("Ground: none"));
    }
    if !r.model.other_surfaces.is_empty() {
        layer(ui, Layer::Surfaces, "Other surfaces shown", r);
    }
    ui.separator();
    ui.menu_button("Survey data", |ui| {
        let busy = r.survey_job.is_some();
        if ui
            .add_enabled(!busy, egui::Button::new("Import LAS/LAZ…"))
            .on_hover_text("Add LiDAR tiles to the project's point cloud (or a new one)")
            .clicked()
        {
            r.survey_import = Some(crate::survey_io::ImportDialog::new(&r.model));
            ui.close();
        }
        if ui
            .add_enabled(
                !busy && r.model.cloud.is_some(),
                egui::Button::new("Export LAS/LAZ…"),
            )
            .on_hover_text("Write the project's point cloud (or part of it) to a LAS/LAZ file")
            .clicked()
        {
            r.survey_export = Some(Default::default());
            ui.close();
        }
        ui.separator();
        if r.model.points.is_empty() {
            ui.add_enabled(false, egui::Button::new("LiDAR: no point cloud"));
            return;
        }
        layer(ui, Layer::Points, "LiDAR points", r);
        if ui
            .button("Survey data display options…")
            .on_hover_text("The LiDAR classes shown: tick or untick as many as you like")
            .clicked()
        {
            if r.survey_window.is_none() {
                r.survey_window = Some(SurveyDraft::of(&r.model));
            }
            ui.close();
        }
        if ui
            .button("Class colours…")
            .on_hover_text(
                "The display colour of the points by class number, from a class scheme file \
                 (.olsc)",
            )
            .clicked()
        {
            r.class_window.get_or_insert_with(Default::default);
            ui.close();
        }
    });
}

/// The views whose controls Parameters › Controls shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlsPage {
    /// The 3D view.
    View3d,
    /// The plan.
    Plan,
    /// The profile.
    Profile,
}

impl ControlsPage {
    /// Every page, in the menu's order.
    pub const ALL: [ControlsPage; 3] = [
        ControlsPage::View3d,
        ControlsPage::Plan,
        ControlsPage::Profile,
    ];

    /// Name in the menu.
    pub fn name(self) -> &'static str {
        match self {
            ControlsPage::View3d => "3D",
            ControlsPage::Plan => "Plan",
            ControlsPage::Profile => "Profile",
        }
    }
}

/// A grid of input → action rows.
fn controls_grid(ui: &mut egui::Ui, id: &str, rows: &[(String, String)]) {
    egui::Grid::new(id).striped(true).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(egui::RichText::new(k).strong());
            ui.label(v);
            ui.end_row();
        }
    });
}

/// Parameters › Controls › 3D / Plan / Profile: how the view is moved and
/// used (the 3D view: its navigation scheme, speed, look sensitivity).
fn controls_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(page) = r.controls_window else {
        return;
    };
    let mut open = true;
    let row = |k: &str, v: &str| (k.to_owned(), v.to_owned());
    egui::Window::new(format!("Controls: {}", page.name()))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| match page {
            ControlsPage::View3d => {
                let c = &mut r.view3d.controls;
                ui.horizontal(|ui| {
                    ui.label("Navigation scheme");
                    egui::ComboBox::from_id_salt("controls")
                        .selected_text(c.name)
                        .show_ui(ui, |ui| {
                            for p in crate::controls::Controls::presets() {
                                if ui.selectable_label(c.name == p.name, p.name).clicked() {
                                    log::info!("controls: {}", p.name);
                                    // Keep the chosen speed.
                                    let speed = c.speed;
                                    *c = p;
                                    c.speed = speed;
                                }
                            }
                        });
                });
                ui.add(
                    egui::Slider::new(&mut c.speed, 1.0..=2000.0)
                        .text("speed, m/s")
                        .logarithmic(true),
                );
                if c.capture {
                    // Shown ×1000 (milliradians per pixel): a readable number.
                    let mut mrad = c.mouse_sensitivity * 1000.0;
                    if ui
                        .add(
                            egui::Slider::new(&mut mrad, 0.3..=10.0)
                                .text("look sensitivity")
                                .logarithmic(true),
                        )
                        .changed()
                    {
                        c.mouse_sensitivity = mrad / 1000.0;
                    }
                }
                ui.separator();
                let mut rows = c.describe();
                rows.push(row("right-click", "use the tool held (hotbar)"));
                rows.push(row("1–9, wheel", "choose a tool of the hotbar"));
                rows.push(row("E", "tool inventory"));
                controls_grid(ui, "controls-3d", &rows);
            }
            ControlsPage::Plan => controls_grid(
                ui,
                "controls-plan",
                &[
                    row("drag", "pan"),
                    row("scroll", "zoom at the pointer"),
                    row("click", "select"),
                    row("double-click", "select and centre on it"),
                    row("right-click", "use the tool held (hotbar)"),
                ],
            ),
            ControlsPage::Profile => controls_grid(
                ui,
                "controls-profile",
                &[
                    row("drag", "move along the route"),
                    row("scroll", "zoom along the route at the pointer"),
                    row("click", "select"),
                    row("double-click", "show the whole route"),
                    row("right-click", "use the tool held (hotbar)"),
                ],
            ),
        });
    if !open {
        r.controls_window = None;
    }
}

/// Criteria › Bimetallic conductor model: what the outer strands of the
/// line's bimetallic cables do above the transition temperature, for the
/// cables whose wire library entry does not say; OK makes it one command
/// (every section is solved again).
fn bimetallic_window(ctx: &egui::Context, r: &mut Ready) {
    use ols_cables::{BimetallicBehavior, DEFAULT_MAX_VIRTUAL_COMPRESSION};
    let Some(mut draft) = r.bimetallic_window else {
        return;
    };
    // The line's bimetallic cables: those following this choice, those
    // with their own.
    let (mut follow, mut own) = (Vec::new(), Vec::new());
    for s in &r.model.line.sections {
        if let Some(c) = r.model.library.get(&s.cable).filter(|c| c.core.is_some()) {
            let list = if c.bimetallic_behavior.is_some() {
                &mut own
            } else {
                &mut follow
            };
            if !list.contains(&c.id) {
                list.push(c.id.clone());
            }
        }
    }
    let (mut ok, mut cancel) = (false, false);
    let mut open = true;
    egui::Window::new("Bimetallic conductor model")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.label(
                "The outer strands of a bimetallic conductor (e.g. aluminium over steel) expand \
                 more with heat than the core: above a transition temperature they carry no more \
                 tension. Above it:",
            );
            ui.radio_value(
                &mut draft.behavior,
                BimetallicBehavior::BirdCage,
                "The outer strands take no compression (bird cage)",
            );
            ui.radio_value(
                &mut draft.behavior,
                BimetallicBehavior::Compression,
                "The outer strands can go into compression (slightly larger sags when hot)",
            );
            ui.add_enabled_ui(draft.behavior == BimetallicBehavior::Compression, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Largest virtual compressive stress");
                    let mut mpa = draft
                        .max_virtual_compression
                        .map_or(DEFAULT_MAX_VIRTUAL_COMPRESSION, |s| s.si())
                        / 1e6;
                    if ui
                        .add(
                            egui::DragValue::new(&mut mpa)
                                .range(0.0..=1e6)
                                .speed(10.0)
                                .suffix(" MPa"),
                        )
                        .changed()
                    {
                        draft.max_virtual_compression =
                            Some(ols_core::units::Stress::from_megapascals(mpa));
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "Virtual stress = actual stress × Ao / At (Ao: the outer strands' area, \
                         At: the whole conductor's)",
                    )
                    .small()
                    .weak(),
                );
            });
            ui.separator();
            let list = |v: &Vec<String>| {
                if v.is_empty() {
                    "none".to_owned()
                } else {
                    v.join(", ")
                }
            };
            ui.label(
                egui::RichText::new(format!(
                    "Following this choice: {}. With their own (wire library): {}.",
                    list(&follow),
                    list(&own)
                ))
                .small(),
            );
            if let Err(why) = r.model.can_edit() {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("read-only: {why}"));
            }
            ui.horizontal(|ui| {
                ok = ui
                    .add_enabled(r.model.can_edit().is_ok(), egui::Button::new("OK"))
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if ok {
        // The default stress needs no value.
        if draft.behavior == BimetallicBehavior::BirdCage
            || draft
                .max_virtual_compression
                .is_some_and(|s| (s.si() - DEFAULT_MAX_VIRTUAL_COMPRESSION).abs() < 1.0)
        {
            draft.max_virtual_compression = None;
        }
        let command = r
            .model
            .editing
            .as_ref()
            .map(|e| e.session.bimetallic_command(draft));
        if let Ok(command) = command {
            r.perform(crate::edit::Action::Command {
                what: "bimetallic conductor model".into(),
                command,
            });
        }
    }
    r.bimetallic_window = (open && !ok && !cancel).then_some(draft);
}

/// Criteria › Creep-Stretch: the weather case for the final after creep
/// and the weather case(s) for the final after load; OK makes it one
/// command (undoable) and every section is solved again.
fn creep_stretch_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut draft) = r.creep_stretch_window.clone() else {
        return;
    };
    let names: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let (mut ok, mut cancel) = (false, false);
    let mut open = true;
    egui::Window::new("Creep-Stretch")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.strong("Weather case for final after creep");
            ui.label(
                egui::RichText::new(
                    "Creep: the wire stretches permanently under tension over time, so its \
                     tensions after creep are lower than initially. Normally an everyday case the \
                     wires see over long periods.",
                )
                .small()
                .weak(),
            );
            egui::ComboBox::from_id_salt("creep-case")
                .selected_text(draft.creep_case.as_deref().unwrap_or("— none —"))
                .width(300.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.creep_case, None, "— none —");
                    for n in &names {
                        ui.selectable_value(&mut draft.creep_case, Some(n.clone()), n);
                    }
                });
            ui.add_space(10.0);
            ui.strong("Weather case(s) for final after load");
            ui.label(
                egui::RichText::new(
                    "A short exposure to an extreme load stretches the wire permanently. With \
                     several cases, the one giving the most stretch is used, for each wire.",
                )
                .small()
                .weak(),
            );
            let mut remove = None;
            egui::Grid::new("load-cases").striped(true).show(ui, |ui| {
                for (k, case) in draft.load_cases.iter_mut().enumerate() {
                    ui.label(format!("{}", k + 1));
                    egui::ComboBox::from_id_salt(("load-case", k))
                        .selected_text(case.as_str())
                        .width(300.0)
                        .show_ui(ui, |ui| {
                            for n in &names {
                                ui.selectable_value(case, n.clone(), n);
                            }
                        });
                    if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                        remove = Some(k);
                    }
                    ui.end_row();
                }
            });
            if let Some(k) = remove {
                draft.load_cases.remove(k);
            }
            if ui.button("+ Add a weather case").clicked()
                && let Some(n) = names.iter().find(|n| !draft.load_cases.contains(n))
            {
                draft.load_cases.push(n.clone());
            }
            ui.separator();
            if let Err(why) = r.model.can_edit() {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("read-only: {why}"));
            }
            ui.horizontal(|ui| {
                ok = ui
                    .add_enabled(r.model.can_edit().is_ok(), egui::Button::new("OK"))
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if ok {
        draft.load_cases.dedup();
        let command = r
            .model
            .editing
            .as_ref()
            .map(|e| e.session.creep_stretch_command(draft.clone()));
        if let Ok(command) = command {
            r.perform(crate::edit::Action::Command {
                what: "creep-stretch weather cases".into(),
                command,
            });
        }
    }
    r.creep_stretch_window = (open && !ok && !cancel).then_some(draft);
}

/// Edit › History: the operation log; admins reverse entries.
fn history_window(ctx: &egui::Context, r: &mut Ready) {
    if !r.history_open {
        return;
    }
    let mut open = true;
    let mut op = None;
    egui::Window::new("History")
        .open(&mut open)
        .default_width(760.0)
        .show(ctx, |ui| op = crate::remote::history_ui(ui, &r.model));
    r.history_open = open;
    if let Some((what, op)) = op {
        r.project_op(&what, op);
    }
}

/// The Import / Export LAS/LAZ windows and the job they started.
fn survey_io_windows(ctx: &egui::Context, r: &mut Ready) {
    if let Some(d) = &mut r.survey_import {
        let mut open = true;
        if let Some(job) = d.ui(ctx, &r.model, &mut open) {
            r.survey_job = Some(job);
            open = false;
        }
        if !open {
            r.survey_import = None;
        }
    }
    if let Some(d) = &mut r.survey_export {
        let mut open = true;
        if let Some(job) = d.ui(ctx, &r.model, &mut open) {
            r.survey_job = Some(job);
            open = false;
        }
        if !open {
            r.survey_export = None;
        }
    }
    if let Some(job) = &mut r.survey_job
        && crate::survey_io::job_window(ctx, job)
    {
        // An import succeeded: the project again, with its new points.
        r.reopen = job.reopen && matches!(job.done, Some(Ok(_)));
        r.survey_job = None;
    }
}

/// A LiDAR class: code, name, roles, points loaded, points in the cloud.
type SurveyClass = (u8, String, Vec<String>, u64, Option<u64>);

/// The LiDAR classes of the point cloud (every class of the cloud, loaded
/// or not).
fn survey_classes(m: &Model) -> Vec<SurveyClass> {
    let mut loaded: std::collections::BTreeMap<u8, u64> = Default::default();
    for (_, c) in &m.points {
        *loaded.entry(*c).or_default() += 1;
    }
    let mut total: std::collections::BTreeMap<u8, Option<u64>> =
        loaded.keys().map(|c| (*c, None)).collect();
    for (c, n) in &m.class_counts {
        total.insert(*c, Some(*n));
    }
    total
        .into_iter()
        .map(|(code, t)| {
            let def = m.scheme.class(code);
            (
                code,
                def.map_or_else(|| "unknown".to_owned(), |c| c.name.clone()),
                def.map_or_else(Vec::new, |c| c.roles.clone()),
                loaded.get(&code).copied().unwrap_or(0),
                t,
            )
        })
        .collect()
}

/// Groups of classes by role, for showing or hiding them together.
const SURVEY_GROUPS: [(&str, &[&str]); 6] = [
    ("ground", &["ground"]),
    ("vegetation", &["vegetation"]),
    ("buildings", &["building", "substation", "tank"]),
    (
        "wires and structures",
        &[
            "conductor",
            "wire",
            "jumper",
            "structure",
            "pole",
            "insulator",
        ],
    ),
    ("traffic", &["road", "railway", "ropeway", "bridge"]),
    ("noise", &["noise"]),
];

/// The "Survey data display options" window: a tick per LiDAR class, kept
/// open while classes are ticked and unticked (each change shows at once);
/// all, none, invert, by group, a filter.
/// Height of the class list of the survey window, points (it scrolls).
const SURVEY_LIST_HEIGHT: f32 = 340.0;

/// What the "Survey data display options" window changes, kept there
/// until applied: reloading the points at every tick lagged, and a drag
/// over several classes missed some of them.
#[derive(Debug, Clone, PartialEq)]
pub struct SurveyDraft {
    /// Filter of the class list.
    pub filter: String,
    /// Classes hidden.
    pub hidden: std::collections::BTreeSet<u8>,
    /// Points shown near the camera, %.
    pub percent: f64,
}

impl SurveyDraft {
    /// What the model shows now.
    pub fn of(model: &Model) -> Self {
        Self {
            filter: String::new(),
            hidden: model.hidden_classes.clone(),
            percent: model.points_percent,
        }
    }

    /// Whether it differs from what the model shows.
    fn differs(&self, model: &Model) -> bool {
        self.hidden != model.hidden_classes || self.percent != model.points_percent
    }
}

fn survey_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut draft) = r.survey_window.clone() else {
        return;
    };
    let classes = survey_classes(&r.model);
    let mut open = true;
    let (mut apply, mut revert) = (false, false);
    let pending = draft.differs(&r.model);
    let mut changed = false;
    let SurveyDraft {
        filter,
        hidden,
        percent,
    } = &mut draft;
    let paint = &mut r.survey_paint;
    egui::Window::new("Survey data display options")
        .open(&mut open)
        .collapsible(false)
        .resizable([true, false])
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Classes shown:");
                if ui.button("All").clicked() {
                    hidden.clear();
                    changed = true;
                }
                if ui.button("None").clicked() {
                    *hidden = classes.iter().map(|c| c.0).collect();
                    changed = true;
                }
                if ui.button("Invert").clicked() {
                    for c in &classes {
                        if !hidden.remove(&c.0) {
                            hidden.insert(c.0);
                        }
                    }
                    changed = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("By kind:");
                for (name, roles) in SURVEY_GROUPS {
                    let members: Vec<u8> = classes
                        .iter()
                        .filter(|c| c.2.iter().any(|r| roles.iter().any(|g| r.contains(g))))
                        .map(|c| c.0)
                        .collect();
                    if members.is_empty() {
                        continue;
                    }
                    let all_shown = members.iter().all(|c| !hidden.contains(c));
                    let mut on = all_shown;
                    if ui
                        .checkbox(&mut on, name)
                        .on_hover_text(format!("{} classes", members.len()))
                        .changed()
                    {
                        for c in members {
                            if on {
                                hidden.remove(&c);
                            } else {
                                hidden.insert(c);
                            }
                        }
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.text_edit_singleline(filter);
            });
            ui.label(
                egui::RichText::new(
                    "Click a class to show or hide it; press and drag up or down to give \
                     every class passed the same state.",
                )
                .small()
                .weak(),
            );
            ui.separator();
            let f = filter.to_lowercase();
            let (pressed, down) =
                ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down()));
            let pointer = ui.input(|i| i.pointer.interact_pos());
            if !down {
                *paint = None;
            }
            // The list scrolls inside the window (a cloud may have dozens
            // of classes).
            // A fixed height: sized from the space left, the window grew a
            // little every frame until it filled the screen.
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .max_height(SURVEY_LIST_HEIGHT)
                .show(ui, |ui| {
                    for (code, name, roles, loaded, total) in &classes {
                        let text = format!("{code} {name}");
                        if !f.is_empty()
                            && !text.to_lowercase().contains(&f)
                            && !roles.iter().any(|r| r.contains(&f))
                        {
                            continue;
                        }
                        let mut on = !hidden.contains(code);
                        let row = ui
                            .horizontal(|ui| {
                                ui.checkbox(&mut on, "");
                                ui.label(&text);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} points",
                                                total.unwrap_or(*loaded)
                                            ))
                                            .small()
                                            .weak(),
                                        );
                                    },
                                );
                            })
                            .response;
                        // The whole row on top of its checkbox: press to
                        // flip it, drag over others to give them its state.
                        let row = ui
                            .interact(
                                row.rect,
                                ui.id().with(("survey-row", code)),
                                egui::Sense::click_and_drag(),
                            )
                            .on_hover_text(roles.join(", "));
                        let shown = !hidden.contains(code);
                        let target = if pressed && row.hovered() {
                            *paint = Some(!shown);
                            Some(!shown)
                        } else {
                            match (*paint, pointer) {
                                (Some(t), Some(p))
                                    if down && p.y >= row.rect.top() && p.y < row.rect.bottom() =>
                                {
                                    Some(t)
                                }
                                _ => None,
                            }
                        };
                        if let Some(t) = target
                            && t != shown
                        {
                            if t {
                                hidden.remove(code);
                            } else {
                                hidden.insert(*code);
                            }
                            changed = true;
                        }
                    }
                });
            ui.separator();
            // How many points: near the camera; farther away the level of
            // detail and the render distance thin them further.
            ui.horizontal(|ui| {
                ui.label("Points shown near the camera:");
                let slider = ui.add(
                    egui::Slider::new(percent, 0.1..=100.0)
                        .logarithmic(true)
                        .suffix(" % of the cloud"),
                );
                changed |= slider.changed();
            });
            ui.label(
                egui::RichText::new(
                    "100 %: every point of the survey near the camera (classes of up to \
                     20 000 points are always whole). Farther away the Level of detail \
                     table's shares and render distance reduce them.",
                )
                .small()
                .weak(),
            );
            ui.separator();
            ui.horizontal(|ui| {
                let pending = pending || changed;
                if ui
                    .add_enabled(pending, egui::Button::new("Apply"))
                    .on_hover_text("Show the classes and the share of points chosen")
                    .clicked()
                {
                    apply = true;
                }
                if ui
                    .add_enabled(pending, egui::Button::new("Revert"))
                    .on_hover_text("Back to what is shown")
                    .clicked()
                {
                    revert = true;
                }
                if pending {
                    ui.colored_label(egui::Color32::YELLOW, "changes not applied");
                }
            });
        });
    if revert {
        let filter = draft.filter.clone();
        draft = SurveyDraft::of(&r.model);
        draft.filter = filter;
    }
    if apply {
        if draft.hidden != r.model.hidden_classes {
            r.model.hidden_classes = draft.hidden.clone();
            r.view3d.dirty.insert(Layer::Points);
        }
        if draft.percent != r.model.points_percent {
            // The 3D view reads the points again at the new share (this
            // user's choice, remembered on this computer).
            r.model.points_percent = draft.percent;
        }
    }
    // Closed: changes not applied are dropped.
    r.survey_window = open.then_some(draft);
}

fn lines_menu(ui: &mut egui::Ui, r: &mut Ready) {
    let snapshots = match &r.model.editing {
        Ok(e) => e.session.snapshots().map_err(|e| e.to_string()),
        Err(why) => Err(format!("read-only: {why}")),
    };
    let snapshots = match snapshots {
        Ok(s) => s,
        Err(e) => {
            ui.label(egui::RichText::new(e).small());
            return;
        }
    };
    ui.menu_button("Project lines", |ui| {
        ui.label(
            egui::RichText::new(format!("{} — the working line (edited)", r.model.line.name))
                .strong(),
        );
        if snapshots.is_empty() {
            ui.label(egui::RichText::new("No saved line yet: Lines › Create line…").small());
        }
        let mut toggle: Option<(String, bool)> = None;
        for s in &snapshots {
            ui.horizontal(|ui| {
                let mut on = r.model.other_lines.iter().any(|o| o.name == s.name);
                let colour = r
                    .model
                    .other_lines
                    .iter()
                    .find(|o| o.name == s.name)
                    .map(|o| egui::Color32::from_rgb(o.color[0], o.color[1], o.color[2]));
                let mut text = egui::RichText::new(&s.name);
                if let Some(c) = colour {
                    text = text.color(c);
                }
                if ui
                    .checkbox(&mut on, text)
                    .on_hover_text(format!(
                        "saved {} by {}{}\ntick: show it next to the working line",
                        s.time.get(..16).unwrap_or(&s.time).replace('T', " "),
                        s.user,
                        if s.note.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", s.note)
                        }
                    ))
                    .changed()
                {
                    toggle = Some((s.name.clone(), on));
                }
                if ui.small_button("make it the working line…").clicked() {
                    r.lines_window = Some(LinesWindow::Restore(s.name.clone()));
                    ui.close();
                }
            });
        }
        if let Some((name, on)) = toggle {
            if on {
                match r.model.show_line(&name) {
                    Ok(m) => r.status = Some((m, false)),
                    Err(e) => r.status = Some((e, true)),
                }
            } else {
                r.model.other_lines.retain(|o| o.name != name);
            }
            r.view3d.dirty.insert(Layer::OtherLines);
        }
    });
    ui.separator();
    if ui
        .button("Create line…")
        .on_hover_text(
            "Freeze the working line as it is now (e.g. as built), to compare or draw later",
        )
        .clicked()
    {
        let name = crate::editors::next_id(snapshots.iter().map(|s| s.name.as_str()), "line 0");
        r.lines_window = Some(LinesWindow::Create(name, String::new()));
        ui.close();
    }
    ui.add_enabled_ui(!snapshots.is_empty(), |ui| {
        ui.menu_button("Delete line", |ui| {
            for s in &snapshots {
                if ui.button(&s.name).clicked() {
                    r.lines_window = Some(LinesWindow::Delete(s.name.clone()));
                    ui.close();
                }
            }
        });
    });
}

/// The windows of the Lines menu.
fn lines_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut w) = r.lines_window.clone() else {
        return;
    };
    let (mut ok, mut cancel) = (false, false);
    let title = match &w {
        LinesWindow::Create(..) => "Create project line".to_owned(),
        LinesWindow::Delete(n) => format!("Delete project line {n}?"),
        LinesWindow::Restore(n) => format!("Make {n} the working line?"),
    };
    tool_window(r, "lines", title).show(ctx, |ui| {
        match &mut w {
            LinesWindow::Create(name, note) => {
                ui.label("Saves the working line as it is now, to show next to it later (e.g. as built, as planned) or to draw.");
                egui::Grid::new("create-line").num_columns(2).show(ui, |ui| {
                    ui.label("name");
                    ui.text_edit_singleline(name);
                    ui.end_row();
                    ui.label("note");
                    ui.text_edit_singleline(note);
                    ui.end_row();
                });
            }
            LinesWindow::Delete(_) => {
                ui.label("The project line is deleted; the working line is not touched. This is not undone with ⌘Z.");
            }
            LinesWindow::Restore(_) => {
                ui.label("The working line is replaced by this project line — one log entry: ⌘Z (Undo) brings the working line back. To keep the working line as it is now, Create line first.");
            }
        }
        ui.horizontal(|ui| {
            ok |= ui
                .button(match &w {
                    LinesWindow::Create(..) => "Create",
                    LinesWindow::Delete(_) => "Delete",
                    LinesWindow::Restore(_) => "Make it the working line",
                })
                .clicked();
            cancel |= ui.button("Cancel").on_hover_text("Escape").clicked();
        });
    });
    cancel |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if cancel {
        r.lines_window = None;
        return;
    }
    if !ok {
        r.lines_window = Some(w);
        return;
    }
    let Ok(e) = &r.model.editing else {
        r.status = Some(("read-only: open a project to edit".into(), true));
        r.lines_window = None;
        return;
    };
    let result = match &w {
        LinesWindow::Create(name, note) => {
            let what = format!("project line \"{}\" saved", name.trim());
            r.model
                .project_op(
                    &what,
                    ols_server::Op::SaveSnapshot {
                        name: name.clone(),
                        note: note.clone(),
                    },
                )
                .map(|_| what)
                .map_err(|e| e.to_string())
        }
        LinesWindow::Delete(name) => {
            let what = format!("project line \"{name}\" deleted");
            r.model
                .project_op(&what, ols_server::Op::DeleteSnapshot { name: name.clone() })
                .map(|_| what)
                .map_err(|e| e.to_string())
        }
        LinesWindow::Restore(name) => match e.session.snapshot(name) {
            Ok(line) => {
                // One command: undone with the others.
                r.perform(crate::edit::Action::Command {
                    what: format!("made project line {name} the working line"),
                    command: ols_session::Command::ReplaceLine {
                        line: Box::new(line),
                    },
                });
                r.lines_window = None;
                return;
            }
            Err(e) => Err(e.to_string()),
        },
    };
    match result {
        Ok(m) => {
            if let LinesWindow::Delete(name) = &w {
                r.model.other_lines.retain(|o| &o.name != name);
                r.view3d.dirty.insert(Layer::OtherLines);
            }
            r.status = Some((m, false));
            r.lines_window = None;
        }
        Err(e) => {
            r.status = Some((e, true));
            r.lines_window = Some(w);
        }
    }
}

/// The state of the "Display weather cases" window.
#[derive(Debug, Clone)]
pub struct DisplayWindow {
    /// The cases ticked (for every section).
    pub ticks: Vec<bool>,
    /// For every section, or each section's own (Sections table).
    pub mode: crate::model::CasesShown,
    /// The wind side shown.
    pub wind: crate::model::WindDisplay,
}

/// The "Display weather cases" window: the same cases for every section
/// (ticked here; the Sections table is not changed) or each section's own
/// display cases from the Sections table; and the side the wind comes
/// from. Apply (Enter) shows them; display only — this user's choice.
fn display_window(ctx: &egui::Context, r: &mut Ready) {
    use crate::model::{CasesShown, WindDisplay};
    let Some(mut w) = r.display_window.clone() else {
        return;
    };
    let (mut apply, mut cancel) = (false, false);
    tool_window(r, "display-cases", "Display weather cases").show(ctx, |ui| {
        ui.radio_value(
            &mut w.mode,
            CasesShown::AllSections,
            "The same weather cases for every section",
        )
        .on_hover_text("Display only: the Sections table is not changed");
        ui.add_enabled_ui(w.mode == CasesShown::AllSections, |ui| {
            ui.indent("cases", |ui| {
                // A box of fixed height, scrolled: long lists of cases keep
                // the window small.
                crate::properties::cases_box(ui, "display-cases-list", |ui| {
                    for (k, c) in r.model.cases.iter().enumerate() {
                        if let Some(t) = w.ticks.get_mut(k) {
                            ui.checkbox(t, &c.case.name).on_hover_text(c.case.summary());
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        w.ticks.iter_mut().for_each(|t| *t = true);
                    }
                    if ui.small_button("None").clicked() {
                        w.ticks.iter_mut().for_each(|t| *t = false);
                    }
                });
            });
        });
        ui.radio_value(
            &mut w.mode,
            CasesShown::PerSection,
            "Each section's display weather case (Sections table)",
        )
        .on_hover_text(
            "Sections without one: the cases ticked above (the first one when none is ticked)",
        );
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Wind");
            egui::ComboBox::from_id_salt("wind-side")
                .selected_text(w.wind.name())
                .show_ui(ui, |ui| {
                    for side in WindDisplay::ALL {
                        ui.selectable_value(&mut w.wind, side, side.name());
                    }
                });
        })
        .response
        .on_hover_text(
            "Looking ahead along the line. Both: each case with wind drawn twice; the wind from \
             the right is the one reported, checked and clicked",
        );
        ui.label(
            egui::RichText::new(
                "Display only, yours: the first case is the one reported, checked and clicked.",
            )
            .small()
            .weak(),
        );
        ui.horizontal(|ui| {
            apply |= ui.button("Apply").on_hover_text("Enter").clicked();
            cancel |= ui.button("Cancel").on_hover_text("Escape").clicked();
        });
    });
    apply |= ctx.input(|i| i.key_pressed(egui::Key::Enter));
    cancel |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if apply {
        let mut cases: Vec<usize> = (0..w.ticks.len()).filter(|k| w.ticks[*k]).collect();
        if cases.is_empty() {
            cases.push(r.case.min(r.model.cases.len().saturating_sub(1)));
        }
        if w.wind != r.model.wind {
            r.model.set_wind(w.wind);
        }
        r.model.cases_shown = w.mode;
        r.show_cases(cases);
        r.display_window = None;
    } else if cancel {
        r.display_window = None;
    } else {
        r.display_window = Some(w);
    }
}

/// The editor window (structure, string, section): Apply (Enter) makes one
/// command and closes it — unless the session refuses it (the reason is
/// shown, the window stays); Cancel (Escape) closes it.
fn editor_window(ctx: &egui::Context, r: &mut Ready) {
    use crate::editors::{Form, Outcome};
    let Some(mut form) = r.editor.clone() else {
        return;
    };
    let mut outcome = None;
    tool_window(r, "editor", form.title()).show(ctx, |ui| {
        outcome = form.ui(ui, &r.model, r.case);
        if let Some((text, true)) = &r.status {
            ui.colored_label(egui::Color32::LIGHT_RED, egui::RichText::new(text).small());
        }
    });
    if outcome.is_none() {
        outcome = ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                Some(Outcome::Cancel)
            } else if i.key_pressed(egui::Key::Enter) {
                Some(Outcome::Apply)
            } else {
                None
            }
        });
    }
    let apply = |r: &mut Ready, form: Form, reset: bool| match form.command(&r.model, reset) {
        Ok((what, command)) => {
            if r.perform(crate::edit::Action::Command { what, command }) {
                r.editor = None;
                // Select what was made.
                match &form {
                    Form::Structure(f) if f.new => r.select(Selection::Structure(f.id.clone())),
                    Form::Section(f) if f.new => r.select(Selection::Span {
                        section: f.id.clone(),
                        span: 0,
                    }),
                    Form::String(f) if f.new => r.select(Selection::Attachment {
                        structure: f.structure.clone(),
                        set: f.set,
                        phase: f.phase,
                    }),
                    _ => {}
                }
            } else {
                r.editor = Some(form);
            }
        }
        Err(e) => {
            r.status = Some((e, true));
            r.editor = Some(form);
        }
    };
    match outcome {
        Some(Outcome::Apply) => apply(r, form, false),
        Some(Outcome::ResetString) => apply(r, form, true),
        Some(Outcome::PickPoints) => {
            r.editor = None;
            if let Form::Section(f) = &form
                && !f.new
            {
                r.restring = Some(f.id.clone());
            }
            r.chain.clear();
            r.hotbar.hold(crate::tools::Tool::StringWire);
            r.status = Some((
                "String wire: right-click the strings in line order, then Enter".into(),
                false,
            ));
        }
        Some(Outcome::Cancel) => {
            r.editor = None;
            r.status = None;
        }
        None => r.editor = Some(form),
    }
}

/// The "Move structure" window: E, N, z (or on the ground), previewed
/// live; Apply (Enter) moves it with one command, Cancel (Escape) puts it
/// back.
fn move_window(ctx: &egui::Context, r: &mut Ready) {
    use crate::edit::Action;
    let Some(mut d) = r.move_dialog.clone() else {
        return;
    };
    let (mut apply, mut cancel) = (false, false);
    tool_window(r, "move", format!("Move structure {}", d.id)).show(ctx, |ui| {
        egui::Grid::new("move-grid").num_columns(3).show(ui, |ui| {
            for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
                ui.label(name);
                let enabled = k < 2 || !d.on_ground;
                ui.add_enabled(
                    enabled,
                    egui::DragValue::new(&mut d.position[k])
                        .speed(0.1)
                        .max_decimals(3),
                );
                ui.label(
                    egui::RichText::new(format!("Δ {:+.3} m", d.position[k] - d.start[k])).small(),
                );
                ui.end_row();
            }
        });
        ui.checkbox(&mut d.on_ground, "base on the ground");
        let horizontal = (d.position[0] - d.start[0]).hypot(d.position[1] - d.start[1]);
        ui.label(egui::RichText::new(format!("moved {horizontal:.3} m horizontally")).small());
        ui.horizontal(|ui| {
            apply |= ui.button("Apply").on_hover_text("Enter").clicked();
            cancel |= ui.button("Cancel").on_hover_text("Escape").clicked();
        });
    });
    if d.on_ground
        && let Some(z) = r.model.ground_at(d.position[0], d.position[1])
    {
        d.position[2] = z;
    }
    apply |= ctx.input(|i| i.key_pressed(egui::Key::Enter));
    cancel |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if cancel {
        r.cancel_move_preview();
        return;
    }
    if apply {
        r.move_dialog = None;
        if d.position != d.start {
            r.perform(Action::Move {
                id: d.id,
                position: d.position,
            });
        }
        return;
    }
    if d.position != d.shown {
        d.shown = d.position;
        r.perform(Action::PreviewMove {
            id: d.id.clone(),
            position: d.position,
        });
    }
    r.move_dialog = Some(d);
}

/// Clearances: takes a finished check, starts one when the model changed
/// (with "check after edits" on) or no check ran yet.
fn clearance_frame(ctx: &egui::Context, r: &mut Ready) {
    if r.clearances.poll() {
        let f = r.clearances.findings();
        r.view3d.marks = f.iter().map(|f| (f.at, f.target)).collect();
        r.plan.marks = f.iter().map(|f| f.at).collect();
        r.view3d.dirty.insert(Layer::Clearances);
    }
    // Checks run on demand (Check now, Checks menu), or after every edit
    // when asked.
    if r.clearances.stale && r.clearances.auto && r.clearances.report.is_some() {
        start_clearance_check(ctx, r);
    }
}

fn start_clearance_check(ctx: &egui::Context, r: &mut Ready) {
    // The check needs its weather case solved, and its ground.
    r.model.ensure_solved(r.case);
    r.model.ensure_check_ground();
    let m = &r.model;
    let Some(solved) = m.cases.get(r.case) else {
        return;
    };
    r.clearances.start(ctx, |rules| crate::clearance::Input {
        line: m.line.clone(),
        case: solved.case.name.clone(),
        shapes: solved.shapes.clone(),
        jumpers: solved.jumpers.clone(),
        rules,
        scheme: m.scheme.clone(),
        cloud: m.cloud.clone(),
        ground: m.ground_for_checks(),
        corridor: m.sources.corridor,
    });
}

/// The Clearances tab: rule file, the violations of the shown case (click
/// one to go there).
fn clearance_panel(ui: &mut egui::Ui, r: &mut Ready) {
    let c = &mut r.clearances;
    if c.files.is_empty() {
        ui.label("No rule file (.olsr) next to the model or in libraries/clearance-rules.");
        ui.label(egui::RichText::new("Start with --rules FILE.olsr.").small());
        return;
    }
    let name = |p: &std::path::Path| {
        p.file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
    };
    let mut chosen = c.chosen;
    egui::ComboBox::from_id_salt("rule-file")
        .selected_text(
            chosen
                .and_then(|k| c.files.get(k))
                .map_or("choose rules…".into(), |p| name(p)),
        )
        .width(220.0)
        .show_ui(ui, |ui| {
            for (k, p) in c.files.iter().enumerate() {
                ui.selectable_value(&mut chosen, Some(k), name(p))
                    .on_hover_text(p.display().to_string());
            }
        });
    if chosen != c.chosen
        && let Some(k) = chosen
    {
        c.choose(k);
    }
    let mut check = false;
    ui.horizontal(|ui| {
        check = ui.button("Check now").clicked();
        ui.checkbox(&mut c.auto, "after every edit");
        if c.running() {
            ui.spinner();
        }
    });
    match &c.rules {
        Some(Err(e)) => {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
        Some(Ok(rules)) => {
            ui.label(egui::RichText::new(format!("{} rules", rules.len())).small());
        }
        None => {}
    }
    if c.stale && !c.running() && c.report.is_some() {
        ui.colored_label(egui::Color32::YELLOW, "out of date: the model changed");
    }
    let mut go: Option<crate::clearance::Finding> = None;
    match &c.report {
        Some(Err(e)) => {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
        Some(Ok(rep)) => {
            let n = rep.findings.len();
            let head = format!(
                "case {}: {n} violation{} ({:.1?})",
                rep.case,
                if n == 1 { "" } else { "s" },
                rep.took
            );
            if n == 0 {
                ui.colored_label(egui::Color32::LIGHT_GREEN, head);
            } else {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 100), head);
            }
            for (rule, text) in &rep.rules {
                ui.label(egui::RichText::new(format!("{rule}: {text}")).small());
            }
            for note in &rep.notes {
                ui.label(egui::RichText::new(note).small().weak());
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for f in &rep.findings {
                    let other = f
                        .other
                        .as_ref()
                        .map_or(String::new(), |o| format!(" – {}", o.describe()));
                    let class = f.class.map_or(String::new(), |k| {
                        format!(
                            ", class {k}{}",
                            r.model
                                .scheme
                                .class(k)
                                .map_or(String::new(), |d| format!(" {}", d.name))
                        )
                    });
                    let text = format!(
                        "{}{other} — {}: {} {:.2} m < {:.2} m{class}{}",
                        f.wire.describe(),
                        f.rule,
                        f.measure,
                        f.distance,
                        f.required,
                        if f.count > 1 {
                            format!(" ({} below)", f.count)
                        } else {
                            String::new()
                        }
                    );
                    let selected = r.selection == f.wire;
                    if ui
                        .selectable_label(selected, egui::RichText::new(text).small())
                        .clicked()
                    {
                        go = Some(f.clone());
                    }
                }
            });
        }
        None if c.running() => {
            ui.label("checking…");
        }
        None => {}
    }
    if check {
        r.clearances.stale = true;
        start_clearance_check(ui.ctx(), r);
    }
    if let Some(f) = go {
        r.selection = f.wire;
        r.view3d.dirty.insert(Layer::Selection);
        if let Some(cam) = r.view3d.camera.as_mut() {
            cam.target = f.at;
            cam.distance = cam.distance.min(80.0);
        }
        r.plan.centre_on(f.at);
    }
}

/// Shows `tab`: brings it to the front if it is open; library pages open
/// together in a floating window, the other pages next to the first one.
pub fn open_page(dock: &mut DockState<Tab>, tab: Tab) {
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
        return;
    }
    let group = |t: &Tab| match t {
        Tab::Library(_) => 1,
        Tab::Table(_) => 2,
        Tab::Python => 3,
        _ => 0,
    };
    if group(&tab) > 0 {
        if let Some(path) = dock.find_tab_from(|t| group(t) == group(&tab)) {
            dock.set_focused_node_and_surface(path.node_path());
            dock.push_to_focused_leaf(tab);
        } else {
            let surface = dock.add_window(vec![tab]);
            if let Some(w) = dock.get_window_state_mut(surface) {
                w.set_position(egui::pos2(140.0, 110.0));
                w.set_size(if group(&tab) == 2 {
                    egui::vec2(1250.0, 560.0)
                } else {
                    egui::vec2(900.0, 620.0)
                });
            }
        }
        return;
    }
    dock.push_to_first_leaf(tab);
}

/// Closes `tab` if it is open.
fn close_page(dock: &mut DockState<Tab>, tab: Tab) {
    if let Some(path) = dock.find_tab(&tab) {
        dock.remove_tab(path);
    }
}

/// The open file's entries of the File menu: Save, Export libraries.
fn file_entries(ui: &mut egui::Ui, r: &mut Ready, out: &mut Vec<crate::menu::Command>) {
    use crate::menu::Command;
    if Command::Save.menu_item(ui) {
        out.push(Command::Save);
        ui.close();
    }
    if ui
        .button("Import into the line…")
        .on_hover_text(
            "Read a file into the working line (built-in formats and those of the plugins); \
             one command, undoable",
        )
        .clicked()
    {
        r.plugin_request = Some(PluginRequest::Import);
        ui.close();
    }
    if ui
        .button("Export…")
        .on_hover_text(
            "Write the line and its solved wires in a format (built-in or from a plugin)",
        )
        .clicked()
    {
        r.plugin_request = Some(PluginRequest::Export);
        ui.close();
    }
    if ui
        .button("Export drawings…")
        .on_hover_text(
            "Plan and profile sheets and profile exports of the routes: SVG, DXF, PDF, PNG, \
             CSV, XML, at chosen scales and station range",
        )
        .clicked()
    {
        r.drawings.window.get_or_insert_with(Default::default);
        ui.close();
    }
    if ui
        .add_enabled(
            r.model.library_dir.is_some(),
            egui::Button::new("Export libraries…"),
        )
        .on_hover_text(
            "Write the project's libraries (wires, structure types, insulators, weather \
             cases) as files into a folder: to share them or compare them",
        )
        .clicked()
    {
        ui.close();
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            r.status = Some(match r.model.export_libraries(&dir) {
                Ok(n) => (
                    format!("{n} library files written to {}", dir.display()),
                    false,
                ),
                Err(e) => (format!("export: {e}"), true),
            });
        }
    }
}

/// The menus after File: Edit (with Snap), View (pages, layers, cases),
/// Lines, Tools, Checks, Terrain, Structures, Sections, Plugins, Help.
/// Commands go to `out`.
fn menus(
    ui: &mut egui::Ui,
    r: &mut Ready,
    dock: &mut DockState<Tab>,
    out: &mut Vec<crate::menu::Command>,
    customize: &mut bool,
) {
    use crate::menu::Command;
    let item = |ui: &mut egui::Ui, c: Command, out: &mut Vec<Command>| {
        if c.menu_item(ui) {
            out.push(c);
            ui.close();
        }
    };
    ui.menu_button("Edit", |ui| {
        let editable = r.model.can_edit().is_ok();
        ui.add_enabled_ui(editable, |ui| {
            item(ui, Command::Undo, out);
            item(ui, Command::Redo, out);
            item(ui, Command::DeleteSelected, out);
        });
        item(ui, Command::ClearSelection, out);
        item(ui, Command::GoTo, out);
        if ui
            .button("History…")
            .on_hover_text("The operation log: who did what; admins reverse entries")
            .clicked()
        {
            r.history_open = true;
            ui.close();
        }
        if ui
            .button("Versions…")
            .on_hover_text("Revisions, branches, tags, diff, merge, reviews and edit rights")
            .clicked()
        {
            r.versions.open = true;
            ui.close();
        }
        ui.separator();
        crate::menu::options_menu(ui, "Snap: what can be clicked", |ui| {
            snap_panel(ui, &mut r.snaps)
        });
    });
    crate::menu::options_menu(ui, "View", |ui| {
        item(ui, Command::FitViews, out);
        ui.separator();
        ui.label(egui::RichText::new("Pages").small().weak());
        for tab in Tab::VIEWS {
            let mut open = dock.find_tab(&tab).is_some();
            if ui.checkbox(&mut open, tab.title()).changed() {
                if open {
                    open_page(dock, tab);
                } else {
                    close_page(dock, tab);
                }
            }
        }
        ui.separator();
        crate::menu::options_menu(ui, "Layers", |ui| {
            for layer in Layer::ALL {
                let mut on = r.visible.contains(&layer);
                if ui.checkbox(&mut on, layer.name()).changed() {
                    if on {
                        r.visible.insert(layer);
                    } else {
                        r.visible.remove(&layer);
                    }
                }
            }
        });
        item(ui, Command::DisplayCases, out);
        ui.separator();
        if ui.button("Customize the quick bar…").clicked() {
            *customize = true;
            ui.close();
        }
    });
    crate::menu::options_menu(ui, "Lines", |ui| lines_menu(ui, r));
    ui.menu_button("Tools", |ui| {
        item(ui, Command::Inventory, out);
        if ui
            .button("Spot structures…")
            .on_hover_text("The least-cost structures along a route (proposal saved as a new line)")
            .clicked()
        {
            r.spot.open(&r.model);
            ui.close();
        }
        ui.separator();
        for t in crate::tools::Tool::ALL {
            let held = r.hotbar.tool() == t;
            let mut b = egui::Button::selectable(held, format!("{} {}", t.icon(), t.name()));
            if let Some(k) = t.key() {
                b = b.shortcut_text(k);
            }
            if ui.add(b).on_hover_text(t.help()).clicked() {
                out.push(Command::Hold(t));
                ui.close();
            }
        }
    });
    ui.menu_button("Checks", |ui| {
        ui.label(
            egui::RichText::new("Run on demand; the model is not changed")
                .small()
                .weak(),
        );
        ui.separator();
        ui.label(egui::RichText::new("Structure loads").strong());
        if ui
            .button("Compute structure loads")
            .on_hover_text(
                "The loads on the structures at every attachment, in each load case \
                 (Criteria › Load cases)",
            )
            .clicked()
        {
            if !r.loads.running() {
                r.loads.start(&r.model, ui.ctx());
            }
            r.open_page = Some((Tab::Loads, None));
            ui.close();
        }
        ui.separator();
        ui.label(egui::RichText::new("Clearances").strong());
        item(ui, Command::RunChecks, out);
        let c = &mut r.clearances;
        ui.checkbox(&mut c.auto, "also after every edit");
        if !c.files.is_empty() {
            ui.menu_button("Rule file", |ui| {
                let mut chosen = None;
                for (k, p) in c.files.iter().enumerate() {
                    let name = p
                        .file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                    if ui.radio(c.chosen == Some(k), name).clicked() {
                        chosen = Some(k);
                    }
                }
                if let Some(k) = chosen {
                    c.choose(k);
                }
            });
        }
        if ui.button("Show results").clicked() {
            open_page(dock, Tab::Clearances);
            ui.close();
        }
        ui.separator();
        ui.label(egui::RichText::new("Thermal rating").strong());
        if ui
            .button("Thermal rating…")
            .on_hover_text(
                "Ampacity per section and ambient case (IEEE 738, CIGRE TB 601 / 207), and the \
                 highest temperature the clearances allow",
            )
            .clicked()
        {
            r.thermal.show(&r.model);
            ui.close();
        }
        ui.separator();
        ui.label(egui::RichText::new("Quantities").strong());
        if ui
            .button("Bill of materials…")
            .on_hover_text(
                "Structures, insulators, wire lengths and hardware, priced with a cost file",
            )
            .clicked()
        {
            r.bom.open(&r.model, ui.ctx());
            ui.close();
        }
        ui.separator();
        ui.label(egui::RichText::new("Analyses").strong());
        if ui
            .button("Galloping ellipses…")
            .on_hover_text(
                "Ellipses of the wires of each span by a galloping parameter file (.olsg)",
            )
            .clicked()
        {
            r.galloping.show(&r.model, r.case);
            ui.close();
        }
        if ui
            .button("Vegetation…")
            .on_hover_text("Vegetation points near the wires, growth and trees falling in")
            .clicked()
        {
            r.vegetation.show(r.case);
            ui.close();
        }
        if ui
            .button("As-built wires…")
            .on_hover_text("Catenaries fitted to the LiDAR wire points, against the model")
            .clicked()
        {
            r.wirefit.open = true;
            ui.close();
        }
        ui.separator();
        ui.add_enabled(false, egui::Button::new("Tension limits (to come)"));
        ui.add_enabled(false, egui::Button::new("Insulator swing (to come)"));
    });
    crate::menu::options_menu(ui, "Terrain", |ui| terrain_menu(ui, r));
    ui.menu_button("Structures", |ui| {
        use crate::library::LibraryKind;
        use crate::tables::TableKind;
        ui.label(egui::RichText::new("Libraries").small().weak());
        item(ui, Command::Library(LibraryKind::StructureTypes), out);
        item(ui, Command::Library(LibraryKind::Insulators), out);
        ui.separator();
        ui.label(egui::RichText::new("Tables").small().weak());
        item(ui, Command::Table(TableKind::Structures), out);
        item(ui, Command::Table(TableKind::Strings), out);
        item(ui, Command::Table(TableKind::Alignments), out);
    });
    ui.menu_button("Sections", |ui| {
        use crate::library::LibraryKind;
        use crate::tables::TableKind;
        ui.label(egui::RichText::new("Libraries").small().weak());
        item(ui, Command::Library(LibraryKind::Cables), out);
        ui.separator();
        ui.label(egui::RichText::new("Tables").small().weak());
        item(ui, Command::Table(TableKind::Sections), out);
        item(ui, Command::Table(TableKind::Jumpers), out);
        item(ui, Command::Table(TableKind::Systems), out);
        ui.separator();
        item(ui, Command::DisplayCases, out);
        crate::menu::options_menu(ui, "Display options", |ui| {
            ui.label(
                egui::RichText::new("Written next to the wires, in each view (yours only)")
                    .small()
                    .weak(),
            );
            r.model.section_display.ui(ui);
        });
    });
    ui.menu_button("Criteria", |ui| {
        use crate::library::LibraryKind;
        use crate::tables::TableKind;
        if ui
            .button("Weather cases")
            .on_hover_text("Every weather case in one table: wind, ice, temperatures")
            .clicked()
        {
            out.push(Command::Table(TableKind::WeatherCases));
            ui.close();
        }
        item(ui, Command::Library(LibraryKind::WeatherCases), out);
        item(ui, Command::Library(LibraryKind::LoadCases), out);
        if ui
            .button("Creep-Stretch…")
            .on_hover_text(
                "The weather cases for the permanent stretch due to creep and due to load \
                 (sections calculated after Creep, after Load or for Max Sag)",
            )
            .clicked()
        {
            r.creep_stretch_window = Some(r.model.line.creep_stretch.clone());
            ui.close();
        }
        if ui
            .button("Bimetallic conductor model…")
            .on_hover_text(
                "What the outer strands of bimetallic cables do above the transition \
                 temperature: bird cage or compression (cables that give their own keep it)",
            )
            .clicked()
        {
            r.bimetallic_window = Some(r.model.line.bimetallic);
            ui.close();
        }
        if ui
            .button("Design code…")
            .on_hover_text(
                "Weather and load cases from a design-code plugin and your parameter file",
            )
            .clicked()
        {
            r.design_code.open = true;
            ui.close();
        }
        ui.separator();
        item(ui, Command::DisplayCases, out);
    });
    ui.menu_button("Parameters", |ui| {
        ui.menu_button("Controls", |ui| {
            for page in ControlsPage::ALL {
                if ui
                    .button(page.name())
                    .on_hover_text(format!("How the {} view is moved and used", page.name()))
                    .clicked()
                {
                    r.controls_window = Some(page);
                    ui.close();
                }
            }
        });
    });
    ui.menu_button("Plugins", |ui| {
        if ui
            .button("Plugin manager…")
            .on_hover_text("The plugins installed and found: turn on or off, install, remove")
            .clicked()
        {
            r.plugin_request = Some(PluginRequest::Manager);
            ui.close();
        }
        if ui
            .button("Python console")
            .on_hover_text(
                "Scripts driving the project through the ols module: read the line and results, \
                 edit with session commands (undoable)",
            )
            .clicked()
        {
            r.open_page = Some((Tab::Python, None));
            ui.close();
        }
    });
    ui.menu_button("Help", |ui| {
        ui.label(format!("OLS CAD {}", env!("CARGO_PKG_VERSION")));
        if ui
            .button("Keys and mouse")
            .on_hover_text("Parameters › Controls › 3D: the navigation scheme and every key")
            .clicked()
        {
            r.controls_window = Some(ControlsPage::View3d);
            ui.close();
        }
    });
}

/// Runs a command of the menus or the quick bar.
fn run_command(
    ctx: &egui::Context,
    r: &mut Ready,
    dock: &mut DockState<Tab>,
    c: crate::menu::Command,
) {
    use crate::edit::Action;
    use crate::menu::Command;
    let editable = r.model.can_edit().is_ok();
    match c {
        Command::Undo if editable => {
            r.perform(Action::Undo);
        }
        Command::Redo if editable => {
            r.perform(Action::Redo);
        }
        Command::Undo | Command::Redo => {}
        Command::FitViews => {
            r.view3d.camera = None;
            r.plan.refit();
        }
        Command::ClearSelection => r.select(Selection::None),
        Command::DeleteSelected => {
            if editable && let Some(a) = delete_action(&r.model.line, &r.selection) {
                r.perform(a);
            }
        }
        Command::GoTo => {
            r.goto = match &r.selection {
                Selection::Structure(id) => Some(id.clone()),
                _ => r.model.line.structures.first().map(|s| s.id.clone()),
            };
        }
        Command::Inventory => {
            r.inventory = true;
            if r.view3d.captured {
                crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
            }
        }
        Command::RunChecks => {
            r.clearances.stale = true;
            start_clearance_check(ctx, r);
            open_page(dock, Tab::Clearances);
        }
        Command::Library(k) => open_page(dock, Tab::Library(k)),
        Command::Save => r.save_all(),
        Command::DisplayCases => {
            let shown = r.model.shown(r.case);
            r.display_window = Some(DisplayWindow {
                ticks: (0..r.model.cases.len())
                    .map(|k| shown.contains(&k))
                    .collect(),
                mode: r.model.cases_shown,
                wind: r.model.wind,
            });
        }
        Command::Table(k) => open_page(dock, Tab::Table(k)),
        Command::Hold(t) => r.hotbar.hold(t),
    }
}

fn snap_panel(ui: &mut egui::Ui, snaps: &mut crate::selection::Snaps) {
    use crate::selection::Snap;
    ui.label(egui::RichText::new("What can be clicked").strong());
    for kind in Snap::ALL {
        let mut on = snaps.on(kind);
        if ui.checkbox(&mut on, kind.name()).changed() {
            if on {
                snaps.0.insert(kind);
            } else {
                snaps.0.remove(&kind);
            }
        }
    }
    ui.horizontal(|ui| {
        if ui.button("All").clicked() {
            snaps.0 = Snap::ALL.into_iter().collect();
        }
        if ui.button("None").clicked() {
            snaps.0.clear();
        }
        if ui.button("Default").clicked() {
            *snaps = crate::selection::Snaps::default();
        }
    });
    ui.label(
        egui::RichText::new(
            "At about the same distance, points win over the lines they sit on, structures over wires.",
        )
        .small(),
    );
}

fn model_panel(ui: &mut egui::Ui, r: &mut Ready) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(egui::RichText::new("Layers").strong());
        for layer in Layer::ALL {
            let mut on = r.visible.contains(&layer);
            if ui.checkbox(&mut on, layer.name()).changed() {
                if on {
                    r.visible.insert(layer);
                } else {
                    r.visible.remove(&layer);
                }
            }
        }
        if let Some(cam) = r.view3d.camera.as_mut() {
            ui.add(
                egui::Slider::new(&mut cam.exaggeration, 1.0..=10.0)
                    .text("height ×")
                    .logarithmic(true),
            );
        }
        ui.separator();
        // Failures by weather case (the cases are chosen with Display
        // weather cases).
        // Each one a link: that weather case is shown and the views go to
        // the section or jumper (to its structure when it has no shape).
        let mut clicked: Option<(usize, String)> = None;
        let pending: Vec<&str> = r
            .model
            .cases
            .iter()
            .filter(|c| !c.ready)
            .map(|c| c.case.name.as_str())
            .collect();
        if !pending.is_empty() {
            ui.label(
                egui::RichText::new(format!("Solved when shown: {}", pending.join(", ")))
                    .small()
                    .weak(),
            );
        }
        for (k, c) in r.model.cases.iter().enumerate() {
            if c.failed.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(
                egui::RichText::new(format!(
                    "{}: {} sections or jumpers failed",
                    c.case.name,
                    c.failed.len()
                ))
                .color(egui::Color32::LIGHT_RED),
            )
            .id_salt(("failed", k))
            .default_open(c.failed.len() <= 10)
            .show(ui, |ui| {
                for f in &c.failed {
                    let (id, why) = f.split_once(": ").unwrap_or((f.as_str(), ""));
                    if ui
                        .link(egui::RichText::new(id).small())
                        .on_hover_text(why)
                        .clicked()
                    {
                        clicked = Some((k, id.to_owned()));
                    }
                    ui.label(egui::RichText::new(why).small().weak());
                }
            });
        }
        if let Some((k, id)) = clicked {
            r.go_to_failure(k, &id);
        }
        ui.separator();
        ui.label(egui::RichText::new("Loaded").strong());
        ui.label(egui::RichText::new(r.model.sources.file.display().to_string()).small());
        for l in &r.model.log {
            ui.label(egui::RichText::new(l).small());
        }
    });
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        match &self.state {
            State::Ready(r) => {
                r.view3d.controls.save(storage);
                r.snaps.save(storage);
                r.hotbar.save(storage);
            }
            _ => {
                self.controls.save(storage);
                self.snaps.save(storage);
                self.hotbar.save(storage);
            }
        }
        self.quickbar.save(storage);
        self.lod.save(storage);
        self.recent.save(storage);
        self.plugins.save(storage);
        if let State::Ready(r) = &self.state {
            self.view_prefs
                .set(&r.model.sources.file, r.model.view_prefs());
        }
        self.view_prefs.save(storage);
        storage.set_string(
            "class-scheme",
            self.default_class_scheme
                .as_ref()
                .map_or(String::new(), |p| p.display().to_string()),
        );
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let State::Loading { rx, progress } = &mut self.state {
            let mut done = None;
            loop {
                match rx.try_recv() {
                    Ok(Message::Progress(p)) => progress.push(p),
                    Ok(Message::Done(r)) => done = Some(r),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if done.is_none() {
                            done = Some(Err("loading stopped (see the terminal)".into()));
                        }
                        break;
                    }
                }
            }
            match done {
                Some(Ok(mut model)) => {
                    if let Some(remote) = self.pending_remote.take() {
                        model.attach(*remote);
                    }
                    let mut ready = Ready::new(*model);
                    ready.view3d.controls = self.controls.clone();
                    ready.snaps = self.snaps.clone();
                    ready.hotbar = self.hotbar.clone();
                    self.state = State::Ready(Box::new(ready));
                }
                Some(Err(e)) => self.state = State::Failed(e),
                None => {}
            }
        }
        let file = self.sources.file.display().to_string();
        let ctx = ui.ctx().clone();
        crate::files::file_keys(&ctx, &mut self.file_action);
        if !matches!(self.state, State::Ready(_)) {
            // Only the File menu without a model.
            egui::Panel::top("top").show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    crate::files::file_menu(
                        ui,
                        &self.recent,
                        None,
                        crate::files::Shared::No,
                        &mut self.file_action,
                        |_| {},
                    );
                });
            });
        }
        match &mut self.state {
            State::Empty => {
                egui::CentralPanel::default().show(ui, |ui| {
                    crate::files::start_page(ui, &self.recent, &mut self.file_action);
                });
            }
            State::Loading { progress, .. } => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading(format!("Loading {file}"));
                    ui.spinner();
                    for p in progress.iter() {
                        ui.label(p);
                    }
                });
            }
            State::Failed(e) => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading(format!("Could not load {file}"));
                    ui.colored_label(egui::Color32::LIGHT_RED, e.as_str());
                });
            }
            State::Ready(ready) => {
                ready.remote_frame();
                ready.solving_frame(ui.ctx());
                history_window(ui.ctx(), ready);
                crate::versions::window(ui.ctx(), ready);
                goto_key(ui.ctx(), ready);
                edit_keys(ui.ctx(), ready);
                clearance_frame(ui.ctx(), ready);
                popups(ui.ctx(), ready);
                tool_windows(ui.ctx(), ready);
                egui::Panel::bottom("status").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        match ready.model.can_edit() {
                            Ok(()) => ui.label(
                                egui::RichText::new(format!(
                                    "editing as {}",
                                    ready.model.editing.as_ref().map_or("", |e| e.user.as_str())
                                ))
                                .small(),
                            ),
                            Err(why) => {
                                ui.label(egui::RichText::new(format!("read-only: {why}")).small())
                            }
                        };
                        if !ready.model.solving.is_empty() {
                            ui.separator();
                            ui.spinner();
                            let names: Vec<&str> = ready
                                .model
                                .solving
                                .iter()
                                .filter_map(|(k, _, _)| ready.model.cases.get(*k))
                                .map(|c| c.case.name.as_str())
                                .collect();
                            ui.label(
                                egui::RichText::new(format!("solving {}…", names.join(", ")))
                                    .small(),
                            );
                        }
                        if let Some((text, error)) = &ready.status {
                            ui.separator();
                            let t = egui::RichText::new(text).small();
                            if *error {
                                ui.colored_label(egui::Color32::LIGHT_RED, t);
                            } else {
                                ui.label(t);
                            }
                        }
                    });
                });
                let mut commands: Vec<crate::menu::Command> = Vec::new();
                egui::Panel::top("top").show(ui, |ui| {
                    egui::MenuBar::new().ui(ui, |ui| {
                        let path = ready.model.sources.file.clone();
                        let shared = match (&ready.model.remote, &self.hosting) {
                            (None, _) => crate::files::Shared::No,
                            (Some(_), Some(_)) => crate::files::Shared::Hosting,
                            (Some(_), None) => crate::files::Shared::Joined,
                        };
                        crate::files::file_menu(
                            ui,
                            &self.recent,
                            Some(&path),
                            shared,
                            &mut self.file_action,
                            |ui| file_entries(ui, ready, &mut commands),
                        );
                        menus(
                            ui,
                            ready,
                            &mut self.dock,
                            &mut commands,
                            &mut self.customize,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.strong(&ready.model.line.name);
                        ui.separator();
                        ui.label(ready.selection.describe());
                        ui.separator();
                        if let Some(c) = self.quickbar.ui(ui) {
                            commands.push(c);
                        }
                        if ui
                            .small_button("⚙")
                            .on_hover_text("Customize the quick bar")
                            .clicked()
                        {
                            self.customize = true;
                        }
                    });
                });
                for c in commands {
                    run_command(ui.ctx(), ready, &mut self.dock, c);
                }
                match ready.plugin_request.take() {
                    Some(PluginRequest::Manager) => {
                        self.plugins.open = true;
                        self.plugins.refresh();
                    }
                    Some(PluginRequest::Import) => self.plugins.open_window(false),
                    Some(PluginRequest::Export) => self.plugins.open_window(true),
                    None => {}
                }
                for (what, arg) in ready.console.poll(&ctx) {
                    let answer = crate::python::serve(ready, &what, &arg);
                    crate::python::answer(answer);
                }
                if ready.surfaces.open {
                    self.plugins.want_formats();
                }
                if let Some(line) = self.plugins.poll(&ctx) {
                    ready.perform(crate::edit::Action::Command {
                        what: format!("imported {}", line.name),
                        command: ols_session::Command::ReplaceLine { line },
                    });
                }
                self.plugins.manager_ui(&ctx);
                crate::codes::window(&ctx, ready, &self.plugins);
                let go = [
                    ready.galloping.ui(&ctx, &ready.model),
                    ready.vegetation.ui(&ctx, &ready.model),
                    ready.wirefit.ui(&ctx, &ready.model),
                ];
                if let Some(target) = go.into_iter().flatten().next() {
                    ready.go_to(target);
                }
                if let Some(mut w) = ready.class_window.take() {
                    let mut open = true;
                    let default = self.default_class_scheme.clone();
                    let action = w.ui(&ctx, &mut open, &mut ready.model, default.as_ref());
                    if let Some(a) = action {
                        use crate::class_colors::ClassAction;
                        let chosen = match a {
                            ClassAction::Recolor => None,
                            ClassAction::UseForProject(p) => Some(p),
                            ClassAction::UseForAll(p) => {
                                self.default_class_scheme = p.clone();
                                Some(p)
                            }
                        };
                        if let Some(p) = chosen {
                            match ready.model.use_class_scheme(p) {
                                Ok(()) => w.dirty = false,
                                Err(e) => w.status = Some((e, true)),
                            }
                        }
                        ready.view3d.recolor_points();
                        ready.refresh_views();
                    }
                    if open {
                        ready.class_window = Some(w);
                    }
                }
                let ground_was = ready.model.ground.as_ref().map(std::sync::Arc::as_ptr);
                let mut ground_shown = ready.visible.contains(&Layer::Ground);
                let shown_before = ground_shown;
                if ready
                    .surfaces
                    .ui(&ctx, &mut ready.model, &mut self.plugins, &mut ground_shown)
                {
                    // A new ground is shown; the tick shows or hides it.
                    let new_ground =
                        ready.model.ground.as_ref().map(std::sync::Arc::as_ptr) != ground_was;
                    if new_ground || (ground_shown && !shown_before) {
                        ready.visible.insert(Layer::Ground);
                    } else if !ground_shown && shown_before {
                        ready.visible.remove(&Layer::Ground);
                    }
                    if !ready.model.other_surfaces.is_empty() {
                        ready.visible.insert(Layer::Surfaces);
                    }
                    // The surfaces themselves are drawn again (the other
                    // layers follow them: draped alignments, clearances).
                    ready.view3d.dirty.insert(Layer::Ground);
                    ready.view3d.dirty.insert(Layer::Surfaces);
                    ready.clearances.stale = true;
                    ready.refresh_views();
                }
                let editable = ready.model.can_edit().map_err(|e| e.to_string());
                self.plugins.format_ui(&ctx, &ready.model, editable);
                if std::mem::take(&mut ready.reopen) {
                    self.file_action = Some(crate::files::FileAction::Open(Some(
                        ready.model.sources.file.clone(),
                    )));
                }
                if let Some((tab, filter)) = ready.open_page.take() {
                    open_page(&mut self.dock, tab);
                    if let (Tab::Table(k), Some(f)) = (tab, filter) {
                        ready.tables.entry(k).or_default().filter = f;
                    }
                }
                if self.customize {
                    let mut open = true;
                    egui::Window::new("Customize the quick bar")
                        .open(&mut open)
                        .collapsible(false)
                        .show(ui.ctx(), |ui| self.quickbar.customize_ui(ui));
                    self.customize = open;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        DockArea::new(&mut self.dock)
                            .show_close_buttons(true)
                            .show_leaf_close_all_buttons(false)
                            .show_inside(
                                ui,
                                &mut Viewer {
                                    ready,
                                    rs: self.render_state.as_ref(),
                                    lod: &mut self.lod,
                                    scripts: self.plugins.python_scripts(),
                                },
                            );
                    });
            }
        }
        // The File menu's windows and actions.
        self.share_windows(&ctx);
        self.import_project_windows(&ctx);
        if let Some(np) = &mut self.new_project {
            let mut open = true;
            if let Some(path) = np.ui(&ctx, &mut open) {
                self.new_project = None;
                self.file_action = Some(crate::files::FileAction::Open(Some(path)));
            } else if !open {
                self.new_project = None;
            }
        }
        if let Some(action) = self.confirm_discard.clone() {
            let lost = self.unsaved();
            let mut choice = None;
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.label("These changes are not saved:");
                    for l in &lost {
                        ui.label(format!("• {l}"));
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Discard them").clicked() {
                            choice = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(false);
                        }
                    });
                });
            match choice {
                Some(true) => {
                    self.confirm_discard = None;
                    self.file_action(&ctx, action, true);
                }
                Some(false) => self.confirm_discard = None,
                None => {}
            }
        }
        if let Some(action) = self.file_action.take() {
            self.file_action(&ctx, action, false);
            // Its window or the next state shows now, not at the next input.
            ctx.request_repaint();
        }
    }
}
