// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
    /// A structure without a type in the pylon editor.
    StructureEditor,
    /// Checks › (the plugin's submenu) › Distance between conductors.
    PhaseDistance,
    /// Checks › Performed checks.
    CheckLibrary,
    /// What the line still needs, each step a link ([`crate::next_steps`]).
    NextSteps,
    /// A tool window docked as a panel ([`crate::dockable`]).
    Tool(crate::dockable::Panel),
    /// A cross-section view, by its number ([`crate::xsection`]).
    CrossSection(u32),
}

/// Where a tab's context menu moves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// With the pages on the left (Model, Clearances, Next steps).
    Left,
    /// With Properties, on the right.
    Right,
    /// A window of its own (still a page: dragged back in).
    Window,
}

impl Tab {
    /// The pages of the View menu (library pages: the Libraries menu).
    pub const VIEWS: [Tab; 9] = [
        Tab::NextSteps,
        Tab::View3d,
        Tab::Plan,
        Tab::Profile,
        Tab::Properties,
        Tab::Model,
        Tab::Clearances,
        Tab::Lod,
        Tab::People,
    ];

    /// A page for looking into the program (the model's numbers, the
    /// level of detail): View › Pages › Advanced.
    pub fn advanced(self) -> bool {
        matches!(self, Tab::Model | Tab::Lod)
    }

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
            Tab::StructureEditor => "Structure editor",
            Tab::PhaseDistance => "Distance between conductors",
            Tab::CheckLibrary => "Performed checks",
            Tab::NextSteps => "Next steps",
            Tab::Tool(p) => p.name(),
            Tab::CrossSection(_) => "Cross-section",
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

/// The check whose clearance violations the views draw (dimmed while it
/// is out of date, [`crate::revision::frame`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarksFrom {
    /// The Clearances tab.
    Clearances,
    /// A vertical or radial clearance window.
    Zone(ohl_clearance::Measure),
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
    /// The cross-section views open and the window making one
    /// ([`crate::xsection`]).
    pub xsections: crate::xsection::CrossSections,
    /// The "Go to structure" window, when open: the structure chosen.
    pub goto: Option<String>,
    /// What the last edit did (or why it was not done: `true`).
    pub status: Option<(String, bool)>,
    /// A deletion waiting for its confirmation: what it takes with it, and
    /// the deletion confirmed.
    pub confirm: Option<(Box<ohl_session::Impact>, crate::edit::Action)>,
    /// The editor window open (structure, string, section).
    pub editor: Option<crate::editors::Form>,
    /// String wire: the strings right-clicked so far.
    pub chain: Vec<ohl_line::SectionPoint>,
    /// String wire: the section whose points are picked again.
    pub restring: Option<String>,
    /// New alignment: the points right-clicked so far.
    pub new_alignment: Vec<ohl_line::AlignmentPoint>,
    /// The points laid out are the Edit alignment tool's: the first is a
    /// point of the alignment changed.
    pub reroute: bool,
    /// The coordinates of a point placed where nothing was to snap to.
    pub coord_dialog: Option<crate::alignment_tools::CoordDialog>,
    /// New alignment (Enter): the route(s) it belongs to.
    pub route_choice: Option<crate::alignment_tools::RouteChoice>,
    /// A project with no route yet: its first steps (the route's name and
    /// description typed), then its first alignment.
    pub first_steps: Option<(String, String)>,
    /// Section editor: the other sections of its circuit the change also
    /// sets, shown on Apply; a second Apply with the same list applies.
    pub circuit_warned: Option<Vec<crate::editors::CircuitMate>>,
    /// The structure window's preview (its camera, its image).
    pub structure_preview: crate::preview3d::Preview,
    /// The structure picker, open for the structure window.
    pub structure_picker: Option<crate::structure_picker::Picker>,
    /// The structure pickers' preview (its camera, its image).
    pub picker_preview: crate::preview3d::Preview,
    /// The window of the angles around structures (the angle ahead being
    /// typed), when open.
    pub angles_window: Option<f64>,
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
    /// A page or the structure screen opened from a tool in the 3D view:
    /// a captured mouse is freed in the next frame ([`popups`]).
    release_mouse: bool,
    /// Replace structure: the picker or the mapping window, when open.
    pub replace: Option<crate::replace::Replace>,
    /// The Delete tool on an alignment point on several alignments: the
    /// point and its alignments, ticked to take it off.
    pub point_delete: Option<(String, Vec<(String, bool)>)>,
    /// A deletion of a spread, or of a point a spread ends on, waiting
    /// for the choice of what goes with it.
    pub spread_delete: Option<SpreadDelete>,
    /// The strings are being taken into the library (its own command must
    /// not start it again).
    cataloguing: bool,
    /// The frame a tool window opened in (every pass of it: egui may
    /// discard a first one) ...
    popup_opened: Option<u64>,
    /// ... and this pass is in it: the window is put at `popup_at` (not
    /// where a window of the same id was in an earlier run).
    popup_fresh: bool,

    /// A position being typed in the properties: (structure, position).
    pub edit_position: Option<(String, [f64; 3])>,
    /// Clearance checks.
    pub clearances: crate::clearance::Clearances,
    /// Library editor pages opened.
    pub libraries:
        std::collections::BTreeMap<crate::library::LibraryKind, crate::library::LibraryPage>,
    /// The "Display weather cases" window, when open: a tick per case.
    pub display_window: Option<DisplayWindow>,
    /// A window of the Versions menu, when open.
    pub lines_window: Option<crate::project_lines::LinesWindow>,
    /// Versions › Project lines › Working tree.
    pub working_tree: crate::working_tree::TreeWindow,
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
    /// Versions › History is open.
    pub history_open: bool,
    /// Versions › Revisions and branches: revisions, branches, merges, reviews.
    pub versions: crate::versions::VersionsWindow,
    /// Checks › Structure loads.
    pub loads: crate::loads::LoadsPage,
    /// Checks › Thermal rating.
    pub thermal: crate::thermal::ThermalWindow,
    /// File › Bill of materials › Cables.
    pub bom: crate::bom::BomWindow,
    /// Structures › Spot structures.
    pub spot: crate::spot::SpotWindow,
    /// Tools › New pylon from a template…, when open.
    pub pylon_builder: Option<crate::pylon_builder::PylonBuilder>,
    /// Checks › Galloping ellipses.
    pub galloping: crate::galloping::GallopingWindow,
    /// Checks › Distance between conductors (LeV).
    pub phase_distance: crate::phase_distance::PhaseDistanceWindow,
    /// Checks › Performed checks: the library of saved reports.
    pub check_library: crate::check_library::CheckLibraryWindow,
    /// Checks › Vegetation.
    pub vegetation: crate::vegetation::VegetationWindow,
    /// Checks › Safety volumes.
    pub safety_volume: crate::safety_volume::SafetyVolumeWindow,
    /// Checks › As-built wires.
    pub wirefit: crate::wirefit::WireFitWindow,
    /// The ground built again from the LiDAR ground classes after their
    /// points changed ([`crate::lidar_edit::frame`]).
    pub lidar_ground: crate::job::Job<Option<ohl_terrain::Terrain>>,
    /// The check whose clearance violations the views draw.
    pub marks_from: MarksFrom,
    /// The Clearances tab's violations drawn (the tab open with a
    /// report); `None` before the first frame.
    pub clearances_drawn: Option<bool>,
    /// The ground is to be built again from the LiDAR ground classes
    /// (their points changed).
    pub lidar_ground_again: bool,
    /// The revision of the ground being built ([`crate::revision`]).
    pub lidar_ground_rev: u64,
    /// With no ground: what the ground was last built from the LiDAR for
    /// (the ground points' revision, the ground classes; none while there
    /// is a ground). With no ground (the active surface deleted, the
    /// class scheme changed) it is built once for each
    /// ([`crate::lidar_edit::frame`]).
    pub lidar_ground_tried: Option<(u64, Vec<u8>)>,
    /// Checks › Ground clearances › Vertical clearances.
    pub vertical_clearances: crate::clearance_zones::ZoneWindow,
    /// Checks › Ground clearances › Radial clearances.
    pub radial_clearances: crate::clearance_zones::ZoneWindow,
    /// Terrain › Terrain surfaces (TIN).
    pub surfaces: crate::surfaces::SurfacesWindow,
    /// Terrain › Survey data › Class colours, when open.
    pub class_window: Option<crate::class_colors::ClassWindow>,
    /// Help › Controls: the window of a view's controls, when open.
    pub controls_window: Option<ControlsPage>,
    /// Criteria › Creep-Stretch, when open: the weather cases chosen.
    pub creep_stretch_window: Option<ohl_line::condition::CreepStretch>,
    /// Criteria › Bimetallic conductor model, when open: the choice.
    pub bimetallic_window: Option<ohl_cables::BimetallicModel>,
    /// Criteria › Calculation options (a draft while open).
    pub solver_window: Option<ohl_line::SolverOptions>,
    /// A jumper joining two different circuits, waiting for the user
    /// ([`crate::circuit_join`]).
    pub circuit_join: Option<crate::circuit_join::CircuitJoin>,
    /// Criteria › Default values: the values being changed, and what the
    /// last export or import said (an error: true).
    pub defaults_window: Option<(ohl_line::Defaults, Option<(String, bool)>)>,
    /// Terrain › Coordinate system: its windows and the reprojection.
    pub crs: crate::crs::Windows,
    /// Terrain › Backgrounds: its windows and the import running.
    pub backgrounds: crate::backgrounds::Windows,
    /// A project to open next (a reprojected copy).
    pub open_project: Option<std::path::PathBuf>,
    /// Criteria › Design code.
    pub design_code: crate::codes::DesignCodeWindow,
    /// The Clearances tab's rule editor and its rule presets.
    pub rule_editor: crate::rule_editor::RuleEditor,
    /// Add jumper: the first string right-clicked.
    pub jumper_from: Option<ohl_line::SectionPoint>,
    /// Add jumper: the first end right-clicked when it is a tap (a clamp
    /// on a span point or a jumper point).
    pub jumper_tap: Option<ohl_line::Tap>,
    /// Add string: the first span point right-clicked (a spacer).
    pub spacer_from: Option<ohl_line::SpanPointRef>,
    /// Link tool: the attachment point chosen (structure, label), waiting
    /// for its joint ([`crate::link_tool`]).
    pub link_from: Option<(String, String)>,
    /// Add jumper: the helping strings right-clicked after it, in order
    /// (structure, wire point).
    pub jumper_via: Vec<(String, String)>,
    /// A page to bring to the front (with a table filter), from a tool.
    pub open_page: Option<(Tab, Option<String>)>,
    /// A structure without a type in the pylon editor.
    pub placed: Option<crate::placed_editor::PlacedEditor>,
    /// Structure files open in the structure application (Edit), watched.
    pub structure_apps: crate::structure_app::StructureApps,
    /// A menu was open last frame.
    pub menu_open: bool,
    /// Table pages opened.
    pub tables: std::collections::BTreeMap<crate::tables::TableKind, crate::tables::TablePage>,
    /// The line had structures when the plan last fitted: the first ones
    /// of an empty line fit it again (the 3D view does the same).
    plan_on_line: bool,
    /// Pages moved with their tab's context menu, done after the frame.
    pub dock_moves: Vec<(Tab, Place)>,
    /// View › Lines on the ground beside the line: the lines of the
    /// weather case shown (kept per case).
    pub ground_lines: crate::ground_lines::GroundLines,
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
            xsections: crate::xsection::CrossSections::default(),
            goto: None,
            status: None,
            confirm: None,
            editor: None,
            chain: Vec::new(),
            restring: None,
            new_alignment: Vec::new(),
            reroute: false,
            circuit_warned: None,
            structure_preview: Default::default(),
            structure_picker: None,
            picker_preview: Default::default(),
            angles_window: None,
            hotbar: crate::tools::Hotbar::default(),
            inventory: false,
            move_dialog: None,
            wheel: Default::default(),
            popup_at: egui::pos2(400.0, 200.0),
            popup_serial: 0,
            popup_open: false,
            release_mouse: false,
            replace: None,
            point_delete: None,
            spread_delete: None,
            cataloguing: false,
            popup_opened: None,
            popup_fresh: false,

            edit_position: None,
            libraries: Default::default(),
            tables: Default::default(),
            display_window: None,
            lines_window: None,
            working_tree: Default::default(),
            survey_window: None,
            survey_paint: None,
            survey_import: None,
            survey_export: None,
            survey_job: None,
            // The drawing settings kept in the project.
            drawings: crate::drawings::Drawings::loaded(&model),
            reopen: false,
            plugin_request: None,
            console: Default::default(),
            history_open: false,
            versions: Default::default(),
            loads: Default::default(),
            thermal: Default::default(),
            bom: Default::default(),
            spot: Default::default(),
            pylon_builder: None,
            galloping: Default::default(),
            phase_distance: Default::default(),
            check_library: Default::default(),
            vegetation: Default::default(),
            safety_volume: Default::default(),
            wirefit: Default::default(),
            lidar_ground: Default::default(),
            lidar_ground_again: false,
            marks_from: MarksFrom::Clearances,
            clearances_drawn: None,
            lidar_ground_rev: 0,
            // Tried when the project was read.
            lidar_ground_tried: model.ground.is_none().then(|| {
                (
                    model.revisions.ground_points,
                    model.scheme.codes_with_role("ground"),
                )
            }),
            vertical_clearances: crate::clearance_zones::ZoneWindow::new(
                ohl_clearance::Measure::Vertical,
            ),
            radial_clearances: crate::clearance_zones::ZoneWindow::new(
                ohl_clearance::Measure::Radial,
            ),
            surfaces: Default::default(),
            class_window: None,
            controls_window: None,
            creep_stretch_window: None,
            bimetallic_window: None,
            solver_window: None,
            circuit_join: None,
            defaults_window: None,
            crs: Default::default(),
            backgrounds: Default::default(),
            open_project: None,
            design_code: Default::default(),
            rule_editor: Default::default(),
            jumper_from: None,
            spacer_from: None,
            link_from: None,
            jumper_tap: None,
            jumper_via: Vec::new(),
            open_page: None,
            placed: None,
            structure_apps: Default::default(),
            ground_lines: Default::default(),
            coord_dialog: None,
            route_choice: None,
            // Step 1 of a new project: its route (then its alignment).
            first_steps: (model.line.routes.is_empty()
                && model.line.alignments.is_empty()
                && model.can_edit().is_ok())
            .then(|| ("route 1".to_owned(), String::new())),
            menu_open: false,
            plan_on_line: !model.line.structures.is_empty(),
            dock_moves: Vec::new(),
            clearances: crate::clearance::Clearances::new(&model, model.sources.rules.as_deref()),
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
    /// A structure type file opened on its own.
    Structure(Box<crate::structure_desk::StructureDesk>),
}

/// The OHL-Design application (the line application of OpenOHL).
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
    /// The colour tool's palette (this user's, remembered).
    pub palette: crate::color_tool::Palette,
    /// The angle that means ahead for structure types opened on their own
    /// (a project has its own), remembered on this computer.
    pub ahead_alone: f64,
    /// Files opened recently.
    pub recent: crate::files::Recent,
    /// What this user shows of each project (remembered locally).
    pub view_prefs: crate::model::ViewPrefsStore,
    /// The LiDAR class scheme (.ohlc) used for every project (remembered).
    pub default_class_scheme: Option<PathBuf>,
    /// The plugin manager, and the formats of File › Import / Export.
    pub plugins: crate::plugins::Plugins,
    /// The server this application hosts.
    pub hosting: Option<ohl_server::server::Server>,
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
    /// Help › About is open.
    pub about: bool,
    /// What the Help menu said without a project (a page not opened).
    help_status: Option<(String, bool)>,
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
    let [centre, _] = tree.split_left(
        centre,
        0.2,
        vec![Tab::Model, Tab::Clearances, Tab::NextSteps],
    );
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
            palette: Default::default(),
            ahead_alone: 0.0,
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
            about: false,
            help_status: None,
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
        // A structure type: its editor, on its own.
        if ohl_core::files::has_extension(
            &sources.file,
            ohl_structures::structures::STRUCTURE_FILE_EXTENSION,
        ) {
            self.state = match crate::structure_desk::StructureDesk::open(&sources.file) {
                Ok(d) => State::Structure(Box::new(d)),
                Err(e) => State::Failed(e),
            };
            let name = sources
                .file
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "{} — {name}",
                crate::APP_NAME
            )));
            self.sources = sources;
            return;
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
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{} — {name}",
            crate::APP_NAME
        )));
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
    /// Tests: the angle ahead the structure editor opened alone draws its
    /// compass with.
    #[cfg(test)]
    pub fn library_ahead_for_tests(&mut self) -> Option<f64> {
        match &self.state {
            State::Structure(d) => Some(d.page.workspace.ahead),
            _ => None,
        }
    }

    fn unsaved(&self) -> Vec<String> {
        if let State::Structure(d) = &self.state {
            return if d.dirty() {
                vec![format!("{} (structure type)", d.path.display())]
            } else {
                Vec::new()
            };
        }
        let State::Ready(r) = &self.state else {
            return Vec::new();
        };
        let mut out: Vec<String> = r
            .libraries
            .iter()
            .filter(|(_, p)| p.dirty())
            .map(|(k, _)| format!("{} page", k.library_name()))
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
            FileAction::Open(_)
                | FileAction::Close
                | FileAction::Quit
                | FileAction::New
                | FileAction::NewStarter
                | FileAction::OpenExample
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
            FileAction::Plugins => {
                self.plugins.open = true;
                self.plugins.refresh();
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
                        ctx.send_viewport_cmd(egui::ViewportCommand::Title(crate::APP_NAME.into()));
                    }
                }
            }
            FileAction::New => self.new_project = Some(Default::default()),
            FileAction::NewStarter => self.new_project = Some(crate::files::NewProject::starter()),
            FileAction::OpenExample => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Folder for a copy of the example")
                    .pick_folder()
                {
                    match crate::files::copy_example(&dir) {
                        Ok(s) => self.start(
                            Sources {
                                file: s.file,
                                terrain: s.terrain,
                                weather: s.weather,
                                ..self.template.clone()
                            },
                            ctx,
                        ),
                        Err(e) => self.state = State::Failed(e),
                    }
                }
            }
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
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(crate::APP_NAME.into()));
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
        let options = ohl_server::server::Options {
            bind: std::net::SocketAddr::new(w.bind, w.port),
            advertise: None,
            code: None,
            newcomers: w.newcomers,
            web: w.web.then(|| std::net::SocketAddr::new(w.bind, w.web_port)),
        };
        match ohl_server::server::Server::start(&file, options) {
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
            palette: Default::default(),
            ahead_alone: 0.0,
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
            about: false,
            help_status: None,
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

    /// Why the file did not open, if it did not.
    #[cfg(test)]
    pub fn failed(&self) -> Option<&str> {
        match &self.state {
            State::Failed(why) => Some(why),
            _ => None,
        }
    }

    /// The shown model, once loaded.
    #[cfg(test)]
    pub fn ready(&mut self) -> Option<&mut Ready> {
        match &mut self.state {
            State::Ready(r) => Some(r),
            _ => None,
        }
    }

    /// The pages and where they are.
    #[cfg(test)]
    pub fn dock(&mut self) -> &mut DockState<Tab> {
        &mut self.dock
    }
}

struct Viewer<'a> {
    ready: &'a mut Ready,
    /// The context (the docked tool panels' shared state).
    ctx: egui::Context,
    rs: Option<&'a egui_wgpu::RenderState>,
    lod: &'a mut crate::lod::LodSettings,
    /// The colour tool's palette.
    palette: &'a mut crate::color_tool::Palette,
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
        if let Tab::Tool(p) = tab {
            let title = crate::dockable::host(&self.ctx, |h| h.alive.get(p).cloned());
            return title.unwrap_or_else(|| p.name().to_owned()).into();
        }
        if let Tab::CrossSection(id) = tab {
            return self.ready.xsections.title(*id).into();
        }
        if dirty {
            format!("{} *", tab.title()).into()
        } else {
            tab.title().into()
        }
    }

    /// Right-click on a tab: where to put the page.
    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Tab, _path: egui_dock::NodePath) {
        for (place, text) in [
            (Place::Left, "Dock on the left"),
            (Place::Right, "Dock on the right (with Properties)"),
            (Place::Window, "Float in a window of its own"),
        ] {
            if ui.button(text).clicked() {
                self.ready.dock_moves.push((*tab, place));
                ui.close();
            }
        }
        ui.label(
            egui::RichText::new("Or drag its tab anywhere: beside, above or below another page.")
                .small()
                .weak(),
        );
    }

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn closeable(&mut self, _tab: &mut Tab) -> bool {
        true
    }

    /// Library pages scroll inside their own panels (the list, the
    /// editor): the dock's scroll area around them moved those panels out
    /// of the tab when scrolled sideways.
    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        match tab {
            // The panel scrolls itself ([`crate::dockable`]).
            Tab::Library(_) | Tab::Tool(_) => [false, false],
            _ => [true, true],
        }
    }

    fn on_close(&mut self, tab: &mut Tab) -> egui_dock::tab_viewer::OnCloseResponse {
        // A tool's panel closed: the tool closes.
        if let Tab::Tool(p) = tab {
            crate::dockable::host(&self.ctx, |h| h.closed.insert(*p));
            self.ctx.request_repaint();
        }
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
        // A cut's tab closed: the cut goes.
        if let Tab::CrossSection(id) = tab {
            self.ready.xsections.views.remove(id);
        }
        egui_dock::tab_viewer::OnCloseResponse::Close
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        let r = &mut *self.ready;
        let before = r.selection.clone();
        let snaps = r.effective_snaps();
        // Which click selects and which uses the held tool (option "Tools
        // act on left-click", kept with the 3D view's controls).
        let select_held = r.hotbar.tool() == crate::tools::Tool::Select;
        let buttons = crate::controls::click_buttons(r.view3d.controls.tools_on_left, select_held);
        let _timed = crate::perf::span(match tab {
            Tab::View3d => "tab 3d",
            Tab::Plan => "tab plan",
            Tab::Profile => "tab profile",
            Tab::CrossSection(_) => "tab cross-section",
            Tab::Properties => "tab properties",
            Tab::Model => "tab model",
            Tab::Clearances => "tab clearances",
            _ => "tab other",
        });
        let used = match tab {
            Tab::View3d => {
                r.view3d.link = r.link_overlay();
                let used = r.view3d.ui(
                    ui,
                    self.rs,
                    &r.model,
                    r.case,
                    &mut r.selection,
                    &r.visible,
                    &snaps,
                    self.lod,
                    select_held,
                );
                if let Some(done) = r.view3d.handles.done.take() {
                    r.handle_done(done);
                }
                if let Some(joint) = r.view3d.link.as_mut().and_then(|l| l.clicked.take()) {
                    r.link_to_joint(&joint);
                }
                // A cut's outline clicked: its panel in front.
                if let Some(id) = r.view3d.picked_cut.take() {
                    r.open_page = Some((Tab::CrossSection(id), None));
                }
                used
            }
            Tab::Plan => r.plan.ui(
                ui,
                &r.model,
                r.case,
                &mut r.selection,
                &snaps,
                &r.visible,
                r.drawings.preview(&r.model, r.case),
                buttons,
            ),
            Tab::Profile => {
                // Right-click: "Cross-section here" while Select is held.
                r.profile.cut_menu = select_held;
                r.profile.vegetation_layers = [
                    r.visible.contains(&Layer::GrowIn),
                    r.visible.contains(&Layer::FallIn),
                ];
                r.profile
                    .ui(
                        ui,
                        &r.model,
                        r.case,
                        &mut r.selection,
                        &snaps,
                        &mut r.drawings,
                        buttons,
                    )
                    .map(|target| crate::selection::Use {
                        target,
                        at: None,
                        on: None,
                    })
            }
            Tab::CrossSection(id) => {
                // A 3D view of the slab: the main view's layers, controls
                // and tools.
                let link = r.link_overlay();
                let used = r.xsections.ui(
                    *id,
                    ui,
                    crate::xsection::PanelCtx {
                        rs: self.rs,
                        model: &r.model,
                        case: r.case,
                        selection: &mut r.selection,
                        visible: &r.visible,
                        snaps: &snaps,
                        lod: self.lod,
                        select_held,
                        main: &mut r.view3d,
                        link,
                    },
                );
                // What its view did, as the main view's.
                if let Some(v) = r.xsections.views.get_mut(id) {
                    let view = &mut v.view;
                    if let Some(c) = view.picked_case.take() {
                        r.view3d.picked_case = Some(c);
                    }
                    if let Some(m) = view.picked_member.take() {
                        r.view3d.picked_member = Some(m);
                    }
                    if let Some(a) = view.area_picked.take() {
                        r.view3d.area_picked = Some(a);
                    }
                    let done = view.handles.done.take();
                    let joint = view.link.as_mut().and_then(|l| l.clicked.take());
                    if let Some(done) = done {
                        r.handle_done(done);
                    }
                    if let Some(joint) = joint {
                        r.link_to_joint(&joint);
                    }
                }
                used
            }
            Tab::Properties => {
                let editable = r.model.can_edit().is_ok();
                let mut start_link = None;
                let mut cut_at = None;
                let loads = r.loads.results.as_ref().and_then(|x| x.as_ref().ok());
                let action = crate::properties::ui(
                    ui,
                    &r.model,
                    r.case,
                    &mut r.selection,
                    editable.then_some(&mut r.edit_position),
                    crate::properties::PointExtras {
                        loads: loads.map(Vec::as_slice),
                        editable,
                        start_link: &mut start_link,
                        cut_at: &mut cut_at,
                    },
                );
                if let Some(a) = action {
                    r.perform(a);
                }
                if let Some(id) = cut_at {
                    r.cross_section_at(&id);
                }
                if let Some((structure, label)) = start_link {
                    r.hotbar.hold(crate::tools::Tool::Link);
                    r.use_tool(crate::selection::Use {
                        target: Selection::Point { structure, label },
                        at: None,
                        on: None,
                    });
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
            Tab::NextSteps => {
                egui::ScrollArea::vertical().show(ui, |ui| crate::next_steps::ui(ui, r));
                None
            }
            Tab::Tool(p) => {
                crate::dockable::record(ui, *p);
                None
            }
            Tab::StructureEditor => {
                r.placed_ui(ui, self.rs);
                None
            }
            Tab::PhaseDistance => {
                let out = r.phase_distance.ui(ui, &r.model);
                r.phase_distance.apply_values(&mut r.model);
                r.phase_distance_out(out);
                None
            }
            Tab::CheckLibrary => {
                let out = r.check_library.ui(ui, &r.model);
                r.check_library_out(out);
                None
            }
            Tab::Loads => {
                r.loads.poll();
                if let Some(id) = r.loads.ui(ui, &r.model) {
                    r.go_to(Selection::Structure(id));
                }
                // Colour structures by usage: the views follow.
                let marks = r.loads.usage_marks();
                if marks != r.model.usage_marks {
                    r.model.usage_marks = marks;
                    r.view3d.dirty.insert(Layer::Structures);
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
                let mut tools = crate::tables::TableTools {
                    palette: self.palette,
                    rs: self.rs,
                    preview: &mut r.picker_preview,
                };
                match page.ui(ui, kind, &r.model, r.case, &r.selection, &mut tools) {
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
                    Some(crate::tables::TableAction::Replace(id)) => r.start_replace(&id),
                    Some(crate::tables::TableAction::EditInStructureApp(id)) => {
                        r.edit_structure_in_app(&id);
                    }
                    Some(crate::tables::TableAction::CrossSection(id)) => {
                        r.cross_section_at(&id);
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
                let cx = crate::library::PageContext {
                    rs: self.rs,
                    line: Some(&r.model.line),
                    insulators: r.model.types.as_ref().map(|t| &t.1),
                    ahead: r.model.line.display.ahead_deg,
                };
                // The hotbar's Link tool held: the structure screen's too.
                page.workspace.link_held = r.hotbar.tool() == crate::tools::Tool::Link;
                let action = page.ui(ui, &sources, cx);
                let request = page.workspace.request.take();
                if let Some(action) = action {
                    r.library_saved(kind, action);
                }
                if let Some(req) = request {
                    r.workspace_request(req);
                }
                None
            }
        };
        // The weather case of the copy clicked in a view.
        if let Some(c) = r.view3d.picked_case.take().or(r.plan.picked_case.take())
            && c != r.model.selection_case
        {
            r.model.selection_case = c;
            r.view3d.dirty.insert(Layer::Selection);
        }
        if r.selection != before {
            r.view3d.dirty.insert(Layer::Selection);
        }
        // A member clicked in the 3D view: selected in the structure screen
        // when it shows the structure's type.
        if let Some((structure, member)) = r.view3d.picked_member.take() {
            r.select_member(&structure, &member);
        }
        // A right-click uses the held tool on what is there.
        if let Some(target) = used {
            r.use_tool(target);
        }
        // The profile's "Cross-section here".
        if let Some((alignment, station)) = r.profile.cut_request.take() {
            r.open_cut(crate::xsection::CutSpec {
                alignment,
                at: crate::xsection::At::Station(station),
                depth: crate::xsection::DEFAULT_DEPTH,
                width: r.drawings.line_width(),
            });
        }
    }
}

impl Ready {
    /// Opens the Cross-section window (C, View › Cross-section…): at the
    /// structure selected, mid-span of the span selected, else the start
    /// of the alignment the profile shows.
    pub fn open_cut_dialog(&mut self) {
        let alignment = self.profile_alignment();
        let width = self.drawings.line_width();
        self.xsections
            .open_dialog(&self.model, &self.selection, alignment, width);
        self.popup_open = false;
    }

    /// The first alignment of what the profile shows (by default the
    /// route with most structures).
    fn profile_alignment(&mut self) -> Option<String> {
        let views = self.xsections.alignments(&self.model.line);
        let show = self
            .profile
            .show
            .clone()
            .unwrap_or_else(|| crate::profile::default_show(&self.model.line, &views));
        views
            .iter()
            .find(|v| match &show {
                crate::profile::Show::Route(r) => &v.route == r,
                crate::profile::Show::Alignment(a) => &v.id == a,
                crate::profile::Show::All => true,
            })
            .map(|v| v.id.clone())
    }

    /// Opens cut `spec` in a tab of its own.
    pub fn open_cut(&mut self, spec: crate::xsection::CutSpec) {
        let id = self.xsections.open(spec);
        self.open_page = Some((Tab::CrossSection(id), None));
    }

    /// Opens a cut at structure `id` at once (its menu: Cross-section
    /// here), the depth by default, the line width across.
    pub fn cross_section_at(&mut self, id: &str) {
        let views = self.xsections.alignments(&self.model.line);
        let preferred = self.profile_alignment().unwrap_or_default();
        let d = crate::xsection::Dialog::new(
            &self.model,
            &views,
            &Selection::Structure(id.to_owned()),
            Some(preferred),
            self.drawings.line_width(),
        );
        if let Some(spec) = d.spec(&views) {
            self.open_cut(spec);
        }
    }

    /// Selects member `member` of structure `structure` in the structure
    /// screen (the Structure types page's pylon workspace) when it is open
    /// on the structure's type; whether it was.
    pub fn select_member(&mut self, structure: &str, member: &str) -> bool {
        let Some(type_id) = self
            .model
            .line
            .structure(structure)
            .and_then(|s| s.structure_type.clone())
        else {
            return false;
        };
        let Some(page) = self
            .libraries
            .get_mut(&crate::library::LibraryKind::StructureTypes)
        else {
            return false;
        };
        let w = &mut page.workspace;
        if !w.on || w.shown().is_none_or(|t| t.id != type_id) {
            return false;
        }
        w.selected = vec![crate::type_workspace::Handle::Member(member.to_owned())];
        w.redraw();
        true
    }

    /// What can be right-clicked now: the Snap menu's choice, and strings
    /// while a tool that works on them is held.
    pub fn effective_snaps(&self) -> crate::selection::Snaps {
        let mut s = self.snaps.clone();
        if self.hotbar.tool().picks_strings() {
            s.0.insert(crate::selection::Snap::Attachment);
        }
        if self.hotbar.tool().picks_points() {
            s.0.insert(crate::selection::Snap::Point);
        }
        // The rigid links shown are clicked like their attachment points.
        if self.visible.contains(&Layer::RigidLinks) {
            s.0.insert(crate::selection::Snap::RigidLink);
        }
        if self.hotbar.tool().picks_spans() {
            s.0.insert(crate::selection::Snap::Span);
        }
        if self.hotbar.tool().picks_jumpers() {
            s.0.insert(crate::selection::Snap::Jumper);
        }
        if self.hotbar.tool().picks_alignment() {
            s.0.insert(crate::selection::Snap::AlignmentPoint);
        }
        // New alignment points snap onto the survey and LiDAR points shown,
        // and onto structures.
        if matches!(
            self.hotbar.tool(),
            crate::tools::Tool::NewAlignment
                | crate::tools::Tool::AddAlignment
                | crate::tools::Tool::EditAlignment
        ) {
            s.0.insert(crate::selection::Snap::Structure);
            if self.visible.contains(&Layer::Ground) {
                s.0.insert(crate::selection::Snap::SurveyPoint);
            }
            if self.visible.contains(&Layer::Points) {
                s.0.insert(crate::selection::Snap::LidarPoint);
            }
        }
        s
    }

    /// Uses the held tool on what was clicked with the tool button in a
    /// view (a right-click, or a left-click with the option "Tools act on
    /// left-click"): Select
    /// selects, Go to opens its window, the editing tools act on the object
    /// (or, Add structure, on the ground point).
    pub fn use_tool(&mut self, u: crate::selection::Use) {
        // A window the click opens, or one it changes to another object,
        // opens next to the click (not where an earlier one was, maybe
        // off screen).
        let before = (
            self.editor.clone(),
            self.goto.clone(),
            self.coord_dialog.clone(),
            self.move_dialog.clone(),
            self.point_delete.clone(),
            self.spread_delete.clone(),
        );
        self.use_tool_at(u);
        log::info!(
            "{}: window {}{}",
            self.hotbar.tool().name(),
            self.editor
                .as_ref()
                .map_or("none".to_owned(), |f| format!("\"{}\"", f.title())),
            self.status
                .as_ref()
                .map_or(String::new(), |(s, _)| format!("; status: {s}"))
        );
        if (self.editor.is_some() && self.editor != before.0)
            || (self.goto.is_some() && self.goto != before.1)
            || (self.coord_dialog.is_some() && self.coord_dialog != before.2)
            || (self.move_dialog.is_some() && self.move_dialog != before.3)
            || (self.point_delete.is_some() && self.point_delete != before.4)
            || (self.spread_delete.is_some() && self.spread_delete != before.5)
        {
            self.popup_open = false;
        }
    }

    fn use_tool_at(&mut self, u: crate::selection::Use) {
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
        // A LiDAR point: its window (read-only for a viewer).
        if tool == Tool::Edit
            && let Selection::LidarPoint(p) = &target
        {
            match crate::lidar_edit::PointsForm::of(&self.model, &[*p]) {
                Ok(f) => self.editor = Some(crate::editors::Form::Points(f)),
                Err(e) => self.status = Some((e, true)),
            }
            self.select(target);
            return;
        }
        if let Err(why) = self.model.can_edit() {
            self.status = Some((format!("{}: read-only — {why}", tool.name()), true));
            return;
        }
        // A spread is deleted as itself; the other tools act on its span
        // (adding on it where the wire was clicked, editing its section).
        let target = match target {
            Selection::Spread { section, index } if tool != Tool::Delete => {
                match spread_span(&self.model.line, &section, index) {
                    Some(span) => Selection::Span { section, span },
                    None => Selection::Spread { section, index },
                }
            }
            t => t,
        };
        use crate::editors::{Form, SectionForm, StringForm, StructureForm};
        let model = &self.model;
        match (tool, &target) {
            (Tool::Move, Selection::Structure(id)) => {
                let id = id.clone();
                self.select(target);
                self.use_tool_on(&id);
            }
            (Tool::Replace, Selection::Structure(id)) => {
                let id = id.clone();
                self.select(target);
                self.start_replace(&id);
            }
            // A point on several alignments: which to take it off (a window).
            (Tool::Delete, Selection::AlignmentPoint { id })
                if self.model.line.point_alignments(id).len() > 1 =>
            {
                let on = self.model.line.point_alignments(id);
                self.point_delete = Some((id.clone(), on.into_iter().map(|a| (a, true)).collect()));
                self.select(target);
            }
            (Tool::Delete, _) => {
                if deletable(&self.model.line, &target) {
                    self.select(target.clone());
                    self.delete_object(&target);
                } else {
                    self.hint(tool);
                }
            }
            // Its pylon: the type in the pylon workspace, a structure
            // without a type in the structure editor (as Strings and points… in
            // Properties).
            (Tool::Edit, Selection::Structure(id)) => {
                let (id, typed) = (
                    id.clone(),
                    model
                        .line
                        .structure(id)
                        .and_then(|s| s.structure_type.clone()),
                );
                self.select(target);
                match typed {
                    Some(t) => self.edit_type(&t),
                    None => self.edit_structure(&id),
                }
                // The page or the structure screen needs the cursor.
                self.release_mouse = true;
            }
            (Tool::Edit, Selection::Attachment { structure, set }) => {
                self.editor = StringForm::of(model, structure, *set).map(Form::String);
                self.select(target);
            }
            (Tool::Edit, Selection::String { structure, label }) => {
                self.editor = StringForm::of_string(model, structure, label).map(Form::String);
                self.select(target);
            }
            (Tool::Edit, Selection::AlignmentPoint { id }) => {
                self.editor = crate::editors::AlignmentForm::of(model, id).map(Form::Alignment);
                self.select(target);
            }
            // A point: its own window (place, label, object).
            (Tool::Edit, Selection::JumperPoint { jumper, label }) => {
                self.editor =
                    crate::span_tool::SpanForm::edit_point(model, self.case, jumper, true, label)
                        .map(Form::Span);
                self.select(target);
            }
            (Tool::Edit, Selection::SpanPoint { section, label }) => {
                self.editor =
                    crate::span_tool::SpanForm::edit_point(model, self.case, section, false, label)
                        .map(Form::Span);
                self.select(target);
            }
            (Tool::Edit, Selection::Jumper(id)) => {
                // Its window, as a section's (it frees the mouse).
                self.editor =
                    crate::editors::JumperForm::of(model, id).map(|f| Form::Jumper(Box::new(f)));
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
                    .map(|j| ohl_session::Command::Put {
                        kind: ohl_session::Kind::Jumper,
                        id: j.id.clone(),
                        value: serde_json::to_value(j).expect("a jumper"),
                        ord: None,
                    })
                    .collect();
                if self.perform(crate::edit::Action::Command {
                    what: format!("added {n} jumpers at {id}"),
                    command: ohl_session::Command::Batch { commands },
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
            // A suspension string of the structure (to a wire point without
            // a set: not one holding a wire), after the first end: the
            // jumper passes through a clamp on it, in click order.
            (Tool::AddJumper, Selection::String { structure, label })
                if self.jumper_from.is_some() =>
            {
                let st = model.line.structure(structure);
                let wire = st.and_then(|s| s.string(label)).map(|x| x.wire.clone());
                let helping = st.zip(wire.as_ref()).is_some_and(|(s, w)| {
                    s.wire(w).is_some_and(|w| w.set.is_none())
                        && !s
                            .component(w)
                            .wires
                            .iter()
                            .any(|&i| s.wires[i].set.is_some())
                });
                match wire {
                    Some(w) if helping => {
                        if self
                            .jumper_via
                            .iter()
                            .any(|v| v.0 == *structure && v.1 == w)
                        {
                            self.status = Some((
                                format!("Add jumper: {structure} {w} holds the jumper already"),
                                true,
                            ));
                            return;
                        }
                        self.jumper_via.push((structure.clone(), w));
                        let via: Vec<String> = self
                            .jumper_via
                            .iter()
                            .map(|v| format!("{} {}", v.0, v.1))
                            .collect();
                        let from = self.jumper_from.clone().expect("checked");
                        self.status = Some((
                            format!(
                                "Add jumper from {} set {} through {}: right-click more suspension strings, then the other end",
                                from.structure,
                                from.set,
                                via.join(", ")
                            ),
                            false,
                        ));
                    }
                    _ => {
                        self.status = Some((
                            format!(
                                "Add jumper: {structure} {label} holds a wire (its wire point has a set): right-click a suspension string to a wire point without a set, or the other end"
                            ),
                            true,
                        ));
                    }
                }
            }
            // A tap: a clamp on a span point or on a jumper point.
            (Tool::AddJumper, Selection::SpanPoint { .. } | Selection::JumperPoint { .. }) => {
                let (tap, object) = match &target {
                    Selection::SpanPoint { section, label } => (
                        ohl_line::Tap::Span(ohl_line::SpanPointRef {
                            section: section.clone(),
                            point: label.clone(),
                        }),
                        model
                            .line
                            .sections
                            .iter()
                            .find(|s| &s.id == section)
                            .and_then(|s| s.span_points.iter().find(|p| &p.label == label))
                            .and_then(|p| p.object.clone()),
                    ),
                    Selection::JumperPoint { jumper, label } => (
                        ohl_line::Tap::Jumper {
                            jumper: jumper.clone(),
                            point: label.clone(),
                        },
                        model
                            .line
                            .jumpers
                            .iter()
                            .find(|j| &j.id == jumper)
                            .and_then(|j| j.points.iter().find(|p| &p.label == label))
                            .and_then(|p| p.object.clone()),
                    ),
                    _ => unreachable!("matched"),
                };
                if !object.is_some_and(|o| o.kind == ohl_line::ObjectKind::Clamp) {
                    self.status = Some((
                        format!(
                            "Add jumper: {} holds no clamp — put a clamp object there (Add on span, object, a tap clamp template), then start the jumper from it",
                            tap.text()
                        ),
                        true,
                    ));
                    return;
                }
                let first = match (self.jumper_from.take(), self.jumper_tap.take()) {
                    (_, Some(t)) if t == tap => {
                        self.status = Some(("Add jumper: cancelled".into(), false));
                        return;
                    }
                    (_, Some(t)) => Some(ohl_line::JumperEnd::tap(t)),
                    (Some(p), None) => Some(ohl_line::JumperEnd::Wire(p)),
                    (None, None) => None,
                };
                match first {
                    None => {
                        self.status = Some((
                            format!(
                                "Add jumper from the tap {}: right-click its other end (a string, or another tap's clamp; the same one again or Escape: cancel)",
                                tap.text()
                            ),
                            false,
                        ));
                        self.jumper_tap = Some(tap);
                    }
                    Some(from) => {
                        match crate::editors::new_tap_jumper(
                            model,
                            from,
                            ohl_line::JumperEnd::tap(tap.clone()),
                        ) {
                            Ok(j) => {
                                let what =
                                    format!("added jumper {} on the tap {}", j.id, tap.text());
                                self.add_jumper(j, what, true);
                            }
                            Err(e) => self.status = Some((e, true)),
                        }
                    }
                }
            }
            (Tool::AddJumper, Selection::Attachment { structure, set })
                if self.jumper_tap.is_some() =>
            {
                let p = ohl_line::SectionPoint::new(structure.clone(), *set);
                let tap = self.jumper_tap.take().expect("checked");
                match crate::editors::new_tap_jumper(
                    model,
                    ohl_line::JumperEnd::tap(tap.clone()),
                    ohl_line::JumperEnd::Wire(p),
                ) {
                    Ok(j) => {
                        let what = format!("added jumper {} from the tap {}", j.id, tap.text());
                        self.add_jumper(j, what, true);
                    }
                    Err(e) => self.status = Some((e, true)),
                }
            }
            (Tool::AddJumper, Selection::Attachment { structure, set }) => {
                let p = ohl_line::SectionPoint::new(structure.clone(), *set);
                // A jumper ends where a section's wire is clamped: a wire
                // point whose set no section uses is refused (a string the
                // jumper should pass through, its set given by mistake —
                // else one jumper would be laid as several).
                if !model.line.sections.iter().any(|s| s.points.contains(&p)) {
                    let wire = model
                        .line
                        .wire_point(&p)
                        .map_or(String::new(), |w| format!(" (wire point {})", w.label));
                    self.status = Some((
                        format!(
                            "Add jumper: set {set} on {structure}{wire} holds no section, so no jumper ends there. To pass the jumper through this string, untick its wire point's set in the string window (Edit on the string); then right-click it between the jumper's ends"
                        ),
                        true,
                    ));
                    return;
                }
                match self.jumper_from.take() {
                    None => {
                        self.status = Some((
                            format!(
                                "Add jumper from {structure} set {set}: right-click the suspension strings it passes through in order (if any), then the other end (the same one again or Escape: cancel)"
                            ),
                            false,
                        ));
                        self.jumper_from = Some(p);
                        self.jumper_via.clear();
                    }
                    Some(from) if from == p => {
                        self.jumper_via.clear();
                        self.status = Some(("Add jumper: cancelled".into(), false));
                    }
                    Some(from) if !self.jumper_via.is_empty() => {
                        let via = std::mem::take(&mut self.jumper_via);
                        match crate::editors::new_jumper_through(model, &from, &p, &via) {
                            Ok(j) => {
                                let what = format!(
                                    "added jumper {} through {} suspension strings",
                                    j.id,
                                    via.len()
                                );
                                // Its window: the lengths between its clamps.
                                self.add_jumper(j, what, true);
                            }
                            Err(e) => self.status = Some((e, true)),
                        }
                    }
                    Some(from) => match crate::editors::new_jumper(model, &from, &p, &[]) {
                        Ok(j) => {
                            let what = format!("added jumper {}", j.id);
                            self.add_jumper(j, what, false);
                        }
                        Err(e) => self.status = Some((e, true)),
                    },
                }
            }
            (Tool::NewAlignment | Tool::AddAlignment | Tool::EditAlignment, _) => {
                use crate::alignment_tools::{CoordDialog, CoordFor};
                let stored = model.stored_line().unwrap_or(&model.line);
                // What was right-clicked gives the point: an alignment
                // point (used as it is: a junction), a structure (tied
                // under it), a survey or LiDAR point shown (its
                // coordinates); else the coordinates are asked for. A new
                // point has no id yet.
                let point = match &target {
                    Selection::AlignmentPoint { id } => stored.alignment_point(id).cloned(),
                    Selection::Structure(id) => {
                        model.line.structure(id).map(|s| ohl_line::AlignmentPoint {
                            structure: Some(s.id.clone()),
                            ..ohl_line::AlignmentPoint::free("", s.position)
                        })
                    }
                    Selection::SurveyPoint(_) | Selection::LidarPoint(_) => {
                        crate::selection::position(model, &target)
                            .map(|p| ohl_line::AlignmentPoint::free("", p))
                    }
                    _ => None,
                };
                match tool {
                    // Always its window: the coordinates (from what was
                    // clicked) and whether it joins the nearest alignment.
                    Tool::AddAlignment => match point {
                        Some(p) if !p.id.is_empty() => {
                            self.status = Some((
                                format!("there is an alignment point here already: {}", p.id),
                                true,
                            ));
                        }
                        Some(p) => {
                            let mut d = CoordDialog::new(CoordFor::AddAlignment, Some(p.position));
                            d.structure = p.structure;
                            self.coord_dialog = Some(d);
                        }
                        None => {
                            self.coord_dialog = Some(CoordDialog::new(CoordFor::AddAlignment, u.at))
                        }
                    },
                    _ => {
                        let purpose = if tool == Tool::NewAlignment {
                            CoordFor::NewAlignment
                        } else {
                            CoordFor::Reroute
                        };
                        match point {
                            Some(p) => self.place_alignment_point(purpose, p),
                            None => self.coord_dialog = Some(CoordDialog::new(purpose, u.at)),
                        }
                    }
                }
            }
            (Tool::OnSpan, Selection::Jumper(id)) => {
                self.editor = crate::span_tool::SpanForm::on_jumper(model, self.case, id, u.on)
                    .map(Form::Span);
                self.select(target);
            }
            (Tool::OnSpan, Selection::Span { section, span }) => {
                self.editor =
                    crate::span_tool::SpanForm::new(model, self.case, section, *span, u.on)
                        .map(Form::Span);
                self.select(target);
            }
            // A spacer: from a span point to another.
            (Tool::AddString, Selection::SpanPoint { section, label }) => {
                let p = ohl_line::SpanPointRef {
                    section: section.clone(),
                    point: label.clone(),
                };
                match self.spacer_from.take() {
                    None => {
                        self.status = Some((
                            format!(
                                "Add spacer from {section} span point {label}: right-click the span point at its other end (the same one again or Escape: cancel)"
                            ),
                            false,
                        ));
                        self.spacer_from = Some(p);
                    }
                    Some(from) if from == p => {
                        self.status = Some(("Add spacer: cancelled".into(), false));
                    }
                    Some(from) => {
                        self.editor = Some(Form::Spacer(crate::span_tool::SpacerForm::new(
                            model, from, p,
                        )));
                    }
                }
                self.select(target);
            }
            (Tool::Edit, Selection::Span { section, .. }) => {
                self.editor = SectionForm::of(model, section).map(Form::Section);
                self.select(target);
            }
            (Tool::Edit, Selection::Spacer(id)) => {
                self.editor = crate::span_tool::SpacerForm::of(model, id).map(Form::Spacer);
                self.select(target);
            }
            // On a free alignment point: on it, tied to it (a point with a
            // structure already: where it is, the window gives the place).
            (Tool::AddStructure, Selection::AlignmentPoint { id }) => {
                let free = crate::editors::alignment_points(&model.line)
                    .into_iter()
                    .find(|p| p.4 == *id && p.3.is_none());
                let at = free
                    .as_ref()
                    .map(|p| p.2)
                    .or(u.at)
                    .or_else(|| model.line.alignment_point(id).map(|p| p.position))
                    .unwrap_or_else(|| structure_spot(model));
                let mut f = StructureForm::new_at(model, at);
                if let Some(p) = free {
                    f.place_on(model, (p.0, p.1));
                }
                self.editor = Some(Form::Structure(f));
                self.select(target);
            }
            // Where it was clicked; with no ground there, the first free
            // alignment point (the window gives its place).
            (Tool::AddStructure, _) => {
                let at = u.at.unwrap_or_else(|| structure_spot(model));
                self.editor = Some(Form::Structure(StructureForm::new_at(model, at)));
            }
            (
                Tool::AddString,
                Selection::Structure(id) | Selection::Attachment { structure: id, .. },
            ) => {
                self.editor = StringForm::new_on(model, id).map(Form::String);
                self.select(Selection::Structure(id.clone()));
            }
            (Tool::AddString, Selection::Point { structure, label }) => {
                self.editor = StringForm::hanging_from(model, structure, label).map(Form::String);
                self.select(target);
            }
            (Tool::Link, Selection::Point { structure, label }) => {
                match crate::link_tool::overlay(&self.model, structure, label) {
                    Ok(o) => {
                        self.status = Some((
                            format!(
                                "Link {label} of {structure}: right-click a ringed joint of its \
                                 type ({} joint(s) it may be linked to; Escape: cancel)",
                                o.joints.len()
                            ),
                            false,
                        ));
                        self.link_from = Some((structure.clone(), label.clone()));
                    }
                    Err(e) => {
                        self.link_from = None;
                        self.status = Some((format!("Link: {e}"), true));
                    }
                }
                self.select(target);
            }
            (Tool::Link, _) => {
                if self.link_from.take().is_some() {
                    self.status = Some((
                        "Link: cancelled (right-click a ringed joint to link the point)".into(),
                        false,
                    ));
                } else {
                    self.hint(tool);
                }
            }
            (Tool::StringWire, Selection::Attachment { structure, set }) => {
                let p = ohl_line::SectionPoint::new(structure.clone(), *set);
                if self.chain.last() == Some(&p) {
                    return;
                }
                if self.chain.contains(&p) {
                    self.status = Some((
                        format!("String wire: {structure} set {set} is in the section already"),
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
            (_, Selection::None) => {
                self.status = Some((
                    format!("{}: nothing here — {}", tool.name(), tool.help()),
                    true,
                ));
            }
            _ => self.hint(tool),
        }
    }

    /// The Link tool's overlay of the 3D view: the point chosen and the
    /// joints it may be linked to (none: the tool not held, or no point
    /// chosen; a point whose structure went is dropped).
    pub fn link_overlay(&mut self) -> Option<crate::link_tool::LinkOverlay> {
        if self.hotbar.tool() != crate::tools::Tool::Link {
            self.link_from = None;
            return None;
        }
        let (id, label) = self.link_from.clone()?;
        match crate::link_tool::overlay(&self.model, &id, &label) {
            Ok(mut o) => {
                if let Some(old) = &self.view3d.link
                    && old.structure == o.structure
                    && old.label == o.label
                {
                    o.hovered.clone_from(&old.hovered);
                }
                Some(o)
            }
            Err(_) => {
                self.link_from = None;
                None
            }
        }
    }

    /// The Link tool's second click: the point chosen linked to `joint`
    /// where it is (one command).
    pub fn link_to_joint(&mut self, joint: &str) {
        let Some((id, label)) = self.link_from.take() else {
            return;
        };
        match crate::link_tool::link_command(&self.model, &id, &label, joint) {
            Ok((what, command)) => {
                self.perform(crate::edit::Action::Command { what, command });
                self.selection = Selection::Point {
                    structure: id,
                    label,
                };
            }
            Err(e) => self.status = Some((format!("Link: {e}"), true)),
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
        // Edit… on a structure file: the structure application, nothing
        // saved yet.
        if let PageAction::EditInStructureApp(p) = &action {
            let id = self
                .model
                .types
                .as_ref()
                .and_then(|(t, _)| t.iter().find(|s| t.path(&s.id) == Some(p.as_path())))
                .map_or_else(
                    || {
                        p.file_stem()
                            .map_or(String::new(), |s| s.to_string_lossy().into_owned())
                    },
                    |s| s.id.clone(),
                );
            self.edit_file_in_structure_app(&p.clone(), &id);
            return;
        }
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
            PageAction::Saved(p) | PageAction::EditInStructureApp(p) => {
                (format!("saved {}", p.display()), None)
            }
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
                    "the line's wire types {} are not in it",
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
            library,
            project_library,
            ..
        } = changes;
        let mut done = true;
        // A library file of the project (the load types).
        if let Some((kind, name, text)) = project_library {
            let result = match &mut self.model.editing {
                Ok(e) => e
                    .session
                    .put_library_file(&e.user, &kind, &name, &text)
                    .map_err(|e| e.to_string()),
                Err(e) => Err(format!("read-only: {e}")),
            };
            match result {
                Ok(()) => {
                    self.status = Some((
                        format!("{} saved in the project", kind.replace('-', " ")),
                        false,
                    ))
                }
                Err(e) => {
                    self.status = Some((e, true));
                    done = false;
                }
            }
        }
        // The string library's files first (the commands use them).
        if let Some((first, _)) = library.first() {
            for (path, text) in &library {
                let result = match text {
                    Some(text) => path
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|()| std::fs::write(path, text)),
                    None => std::fs::remove_file(path),
                };
                if let Err(e) = result {
                    self.status = Some((format!("{}: {e}", path.display()), true));
                    return;
                }
            }
            self.library_saved(
                crate::library::LibraryKind::Insulators,
                crate::library::PageAction::Saved(first.clone()),
            );
            done = !self.status.as_ref().is_some_and(|(_, e)| *e);
        }
        let n = commands.len();
        if n > 0 && done {
            let (what, command) = if n == 1 {
                commands.remove(0)
            } else {
                (
                    format!("{} table: {n} changes", kind.title()),
                    ohl_session::Command::Batch {
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
        // A system's circuits should run the same way: said, not refused.
        use crate::tables::TableKind;
        if done
            && matches!(
                kind,
                TableKind::Sections
                    | TableKind::Systems
                    | TableKind::ConductorPaths
                    | TableKind::Circuits
                    | TableKind::Jumpers
            )
        {
            let warnings = self.model.line.system_warnings();
            if !warnings.is_empty() {
                let done_what = self
                    .status
                    .clone()
                    .map_or(String::new(), |s| format!("{}; ", s.0));
                // Done, with a warning (shown yellow).
                self.status = Some((
                    format!("{done_what}warning: {}", warnings.join("; ")),
                    false,
                ));
            }
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
        // A project's first weather case file: it uses it from now on.
        if self.model.sources.weather.is_none() {
            if let Some(dir) = path.parent()
                && let Err(e) = std::fs::create_dir_all(dir)
            {
                self.status = Some((format!("{}: {e}", dir.display()), true));
                return false;
            }
            self.model.sources.weather = Some(path.to_path_buf());
        }
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
        self.xsections.invalidate();
        self.drawings.invalidate_preview();
    }

    /// Selects `target`.
    pub fn select(&mut self, target: Selection) {
        self.selection = target;
        // Not a copy clicked in a view: the case shown.
        self.model.selection_case = None;
        self.view3d.dirty.insert(Layer::Selection);
    }

    /// The wire being strung, drawn.
    pub fn update_draft(&mut self) {
        self.view3d.draft = if self.new_alignment.is_empty() {
            self.chain
                .iter()
                .filter_map(|p| self.model.line.wire_point(p).map(|w| w.position))
                .collect()
        } else {
            self.new_alignment.iter().map(|p| p.position).collect()
        };
        self.view3d.dirty.insert(Layer::Draft);
    }

    /// New alignment: a point added (or taken back); the draft and the
    /// status follow.
    fn alignment_drafted(&mut self) {
        self.update_draft();
        // The first point of an empty line: the views go to it (it would
        // be out of sight where the empty views look).
        if self.new_alignment.len() == 1
            && self.model.line.alignments.is_empty()
            && self.model.line.structures.is_empty()
        {
            let p = self.new_alignment[0].position;
            if let Some(cam) = self.view3d.camera.as_mut() {
                cam.target = p;
                cam.distance = cam.distance.clamp(150.0, 600.0);
            }
            self.plan.centre_on(p);
        }
        let text = if self.reroute {
            format!(
                "Edit alignment: from {} through {} new point(s) — right-click the next, or another point of the alignment to join it there; Enter at an end: add them there; Backspace: undo a point, Escape: cancel",
                self.new_alignment.first().map_or("", |p| p.id.as_str()),
                self.new_alignment.len().saturating_sub(1)
            )
        } else {
            format!(
                "New alignment: {} points — right-click the next (an alignment point: a junction), Enter: create, Backspace: undo a point, Escape: cancel",
                self.new_alignment.len()
            )
        };
        self.status = Some((text, false));
    }

    /// A point for the alignment tools: the next point of the new
    /// alignment, one inserted into the nearest alignment, or a point on
    /// no alignment yet. A point without id is new; one with an id exists
    /// (shared).
    pub fn place_alignment_point(
        &mut self,
        purpose: crate::alignment_tools::CoordFor,
        p: ohl_line::AlignmentPoint,
    ) {
        use crate::alignment_tools::CoordFor;
        match purpose {
            CoordFor::NewAlignment => {
                if self.reroute {
                    self.new_alignment.clear();
                    self.reroute = false;
                }
                if !p.id.is_empty() && self.new_alignment.iter().any(|q| q.id == p.id) {
                    self.status = Some((
                        format!("New alignment: point {} is on it already", p.id),
                        true,
                    ));
                    return;
                }
                self.new_alignment.push(p);
                self.alignment_drafted();
            }
            CoordFor::Reroute => {
                if !self.reroute {
                    self.new_alignment.clear();
                }
                let stored = self.model.stored_line().unwrap_or(&self.model.line);
                let Some(first) = self.new_alignment.first().map(|q| q.id.clone()) else {
                    // It starts on a point of an alignment.
                    if p.id.is_empty() || stored.point_alignments(&p.id).is_empty() {
                        self.status = Some((
                            "Edit alignment: right-click a point of an alignment first".into(),
                            true,
                        ));
                        return;
                    }
                    self.reroute = true;
                    self.new_alignment.push(p);
                    self.alignment_drafted();
                    return;
                };
                // Another point of the same alignment: done.
                let together = !p.id.is_empty()
                    && p.id != first
                    && stored
                        .alignments
                        .iter()
                        .any(|a| a.points.contains(&first) && a.points.contains(&p.id));
                if together {
                    self.finish_reroute(Some(p.id));
                    return;
                }
                if !p.id.is_empty() && self.new_alignment.iter().any(|q| q.id == p.id) {
                    self.status = Some((
                        format!("Edit alignment: point {} is in it already", p.id),
                        true,
                    ));
                    return;
                }
                self.new_alignment.push(p);
                self.alignment_drafted();
            }
            CoordFor::AddAlignment => {
                match crate::editors::insert_alignment_point(&self.model, p) {
                    Ok((what, command, id)) => {
                        if self.perform(crate::edit::Action::Command { what, command }) {
                            self.select(Selection::AlignmentPoint { id });
                        }
                    }
                    Err(e) => self.status = Some((e, true)),
                }
            }
        }
    }

    /// Add alignment point: `p` (new) on no alignment yet.
    pub fn add_free_point(&mut self, p: ohl_line::AlignmentPoint) {
        let stored = self.model.stored_line().unwrap_or(&self.model.line);
        let id = stored.new_point_id();
        let point = ohl_line::AlignmentPoint {
            id: id.clone(),
            ..p
        };
        let command = ohl_session::Command::Put {
            kind: ohl_session::Kind::AlignmentPoint,
            id: id.clone(),
            value: serde_json::to_value(&point).expect("a point"),
            ord: None,
        };
        if self.perform(crate::edit::Action::Command {
            what: format!("new alignment point {id} (on no alignment)"),
            command,
        }) {
            self.select(Selection::AlignmentPoint { id });
        }
    }

    /// Edit alignment: the points laid out from a point of an alignment
    /// to `end` (another point of it), or — Enter, `None` — at its end. A
    /// first point in the middle of its alignments: a new alignment is
    /// offered (its route asked, as for New alignment).
    pub fn finish_reroute(&mut self, end: Option<String>) {
        let Some(first) = self.new_alignment.first().map(|p| p.id.clone()) else {
            return;
        };
        let middle: Vec<ohl_line::AlignmentPoint> = self.new_alignment[1..].to_vec();
        let stored = self.model.stored_line().unwrap_or(&self.model.line).clone();
        match crate::editors::reroute(&stored, &first, end.as_deref(), &middle) {
            Ok(crate::editors::Reroute::Done(what, command)) => {
                if self.perform(crate::edit::Action::Command { what, command }) {
                    self.new_alignment.clear();
                    self.reroute = false;
                    self.update_draft();
                }
            }
            Ok(crate::editors::Reroute::Branch) => {
                let on = stored.point_alignments(&first).join(", ");
                let points = std::mem::take(&mut self.new_alignment);
                self.reroute = false;
                self.update_draft();
                self.route_choice = Some(crate::alignment_tools::RouteChoice::new(
                    &stored,
                    points,
                    self.model.work_route.as_deref(),
                ));
                self.status = Some((
                    format!(
                        "Point {first} is in the middle of {on}: an alignment has two ends only — \
                         these points make a new alignment (choose its route)"
                    ),
                    false,
                ));
            }
            Err(e) => self.status = Some((e, true)),
        }
    }

    /// New alignment: Enter — its route is asked for (a new one offered
    /// when there is none).
    pub fn finish_new_alignment(&mut self) {
        if self.new_alignment.is_empty() {
            self.status = Some(("New alignment: right-click its first point".into(), true));
            return;
        }
        let points = std::mem::take(&mut self.new_alignment);
        let stored = self.model.stored_line().unwrap_or(&self.model.line);
        self.route_choice = Some(crate::alignment_tools::RouteChoice::new(
            stored,
            points,
            self.model.work_route.as_deref(),
        ));
    }

    /// The new alignment on `route` (made first when `new`): its new
    /// points made, the existing ones shared.
    pub fn make_new_alignment(&mut self, route: String, new: bool) {
        let Some(choice) = self.route_choice.take() else {
            return;
        };
        let stored = self.model.stored_line().unwrap_or(&self.model.line).clone();
        let mut commands = Vec::new();
        let mut line = stored.clone();
        if new {
            match crate::editors::new_route(&stored, &route, &route) {
                Ok((_, c)) => commands.push(c),
                Err(e) => {
                    self.status = Some((e, true));
                    self.route_choice = Some(choice);
                    return;
                }
            }
            line.routes.push(ohl_line::Route::new(&route, &route));
        }
        let mut ids = Vec::new();
        for p in &choice.points {
            if p.id.is_empty() {
                let id = line.new_point_id();
                let point = ohl_line::AlignmentPoint {
                    id: id.clone(),
                    ..p.clone()
                };
                commands.push(ohl_session::Command::Put {
                    kind: ohl_session::Kind::AlignmentPoint,
                    id: id.clone(),
                    value: serde_json::to_value(&point).expect("a point"),
                    ord: None,
                });
                line.alignment_points.push(point);
                ids.push(id);
            } else {
                ids.push(p.id.clone());
            }
        }
        let id = crate::editors::free_alignment_id(&line);
        let a = ohl_line::Alignment::new(&id, &route, ids.clone());
        commands.push(ohl_session::Command::Put {
            kind: ohl_session::Kind::Alignment,
            id: id.clone(),
            value: serde_json::to_value(&a).expect("an alignment"),
            ord: Some(line.alignments.len() as i64),
        });
        let command = match commands.len() {
            1 => commands.remove(0),
            _ => ohl_session::Command::Batch { commands },
        };
        if self.perform(crate::edit::Action::Command {
            what: format!("new alignment {id} on route {route}"),
            command,
        }) {
            self.update_draft();
            if let Some(first) = ids.into_iter().next() {
                self.select(Selection::AlignmentPoint { id: first });
            }
        } else {
            self.route_choice = Some(choice);
        }
    }

    /// New alignment: Escape — the points go; the tool stays held, the
    /// view keeps the mouse.
    pub fn cancel_new_alignment(&mut self) {
        self.new_alignment.clear();
        self.reroute = false;
        self.update_draft();
        self.status = Some(("New alignment: cancelled".into(), false));
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

    /// Cancels the wire being strung (Escape): its points go; the tool
    /// stays held, the view keeps the mouse.
    pub fn cancel_string_wire(&mut self) {
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
                let target = ohl_server::lock_target(
                    &self.model.project_line(),
                    ohl_session::Kind::Structure,
                    id,
                );
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
                    kind: ohl_session::Kind::Structure,
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
    /// Works on project line `name` from now on (this user only): the
    /// line is read and solved again, the views redrawn.
    pub fn switch_line(&mut self, name: &str) {
        match self.model.switch_line(name) {
            Ok(m) if !m.is_empty() => self.status = Some((m, false)),
            Ok(_) => {}
            Err(e) => self.status = Some((e, true)),
        }
        self.selection = Selection::None;
        self.clearances.stale = true;
        self.project_files_changed();
        self.view3d.dirty.extend(Layer::ALL);
        self.refresh_views();
    }

    /// The project's files the windows read (clearance rules, galloping,
    /// ambient cases, unit costs) chosen again: each project line has its
    /// own.
    fn project_files_changed(&mut self) {
        let Some(work) = self.model.library_dir.clone() else {
            return;
        };
        let ours =
            |p: &Option<std::path::PathBuf>| p.as_ref().is_some_and(|p| p.starts_with(&work));
        let first = |kind: &str| self.model.project_files(kind).into_iter().next();
        if ours(&self.galloping.file) {
            self.galloping.file = first("galloping");
        }
        if ours(&self.thermal.file) {
            match first("ambient") {
                Some(p) => self.thermal.choose(p),
                None => (self.thermal.file, self.thermal.ambient) = (None, None),
            }
        }
        if ours(&self.bom.cost_path) {
            self.bom.cost_path = first("costs");
        }
        if ours(&self.spot.cost_path) {
            self.spot.cost_path = first("costs");
        }
        self.clearances
            .refresh(&self.model, self.model.sources.rules.as_deref());
        self.vertical_clearances.line_changed(&self.model);
        self.radial_clearances.line_changed(&self.model);
    }

    pub fn project_op(&mut self, what: &str, op: ohl_server::Op) {
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
        // The weather cases the lines on the ground are measured from.
        let g = &self.model.ground_lines;
        if g.on && g.from == crate::ground_lines::From::OutermostConductor {
            need.extend(g.chosen_in(&self.model, self.case));
        }
        self.model.want(need, ctx);
        let done = self.model.poll_solving();
        if !done.is_empty() {
            self.status = Some((format!("solved: {}", done.join(", ")), false));
            self.clearances.stale = true;
            self.refresh_views();
        }
    }

    /// Every frame: the lines on the ground beside the line round the
    /// weather cases chosen (View › Lines on the ground beside the line),
    /// made when the cases, the line, its solutions, the ground or the
    /// settings change, handed to the views. Display only. The other
    /// wind side of a windy case is solved in the background (`ctx`
    /// repainted when done; none: now).
    pub fn ground_lines_frame(&mut self, ctx: Option<&egui::Context>) {
        if self
            .ground_lines
            .frame(&self.model, self.case, &self.model.ground_lines, ctx)
        {
            self.view3d.ground_lines = self.ground_lines.drawn.clone();
            self.plan.ground_lines = self.ground_lines.drawn.clone();
            self.view3d.dirty.insert(Layer::GroundLines);
        }
        // The profile follows the layer's tick too.
        self.profile.ground_lines = self
            .ground_lines
            .drawn
            .clone()
            .filter(|_| self.visible.contains(&Layer::GroundLines));
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
        let changed = std::mem::take(&mut self.model.lines_changed);
        if !changed.is_empty() {
            if changed.iter().any(String::is_empty)
                && let Err(e) = self.model.reread_line()
            {
                self.status = Some((e, true));
            }
            self.model.refresh_lines(&changed);
            self.view3d.dirty.insert(Layer::OtherLines);
            self.refresh_views();
        }
        let presence = ohl_server::Presence {
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
        let line = self.model.project_line();
        let wanted = self
            .move_dialog
            .iter()
            .map(|d| ohl_server::lock_target(&line, ohl_session::Kind::Structure, &d.id))
            .collect();
        self.model.hold(wanted);
    }

    /// Performs an edit and updates the views; whether it was made.
    /// Opens structure type `id` in the pylon workspace of the Structure
    /// types page.
    pub fn edit_type(&mut self, id: &str) {
        let kind = crate::library::LibraryKind::StructureTypes;
        let Some(path) = self
            .model
            .types
            .as_ref()
            .and_then(|(t, _)| t.path(id))
            .map(std::path::Path::to_path_buf)
        else {
            self.status = Some((format!("structure type {id}: no file read"), true));
            return;
        };
        let sources = self.model.sources.clone();
        let page = self
            .libraries
            .entry(kind)
            .or_insert_with(|| crate::library::LibraryPage::new(kind, &sources));
        if page.file.as_deref() != Some(path.as_path()) {
            if page.dirty() {
                self.status = Some((
                    "the Structure library page has unsaved changes: save or discard them first"
                        .into(),
                    true,
                ));
                self.open_page = Some((Tab::Library(kind), None));
                return;
            }
            page.add_file(&path);
        }
        page.workspace.on = true;
        self.open_page = Some((Tab::Library(kind), None));
    }

    /// **Edit**: structure type `id` in the structure application, on its
    /// file (in a project, the working copy); saved there, it comes back
    /// into the line ([`Ready::structure_apps_frame`]). From placed
    /// structure `placed`: with that place's line values and typed loads.
    pub fn edit_in_structure_app(&mut self, id: &str, placed: Option<&str>) {
        match crate::structure_app::prepare_edit(&self.model, id, placed) {
            Ok((path, placed)) => self.launch_structure_app(&path, id, placed),
            Err(e) => {
                log::info!("structure application: {e}");
                self.structure_apps.reports.push(crate::structure_app::Report {
                    title: "OHL-Structure was not started".into(),
                    text: format!("{e}: the structure type's file was not read (Structures › Structure library)."),
                    error: true,
                });
                self.status = Some((e, true));
            }
        }
    }

    /// The structure application on the structure file `path` (structure
    /// `id`).
    pub fn edit_file_in_structure_app(&mut self, path: &std::path::Path, id: &str) {
        self.launch_structure_app(path, id, None);
    }

    fn launch_structure_app(
        &mut self,
        path: &std::path::Path,
        id: &str,
        placed: Option<crate::structure_app::Placed>,
    ) {
        let insulators = crate::model::insulators_dir(&self.model.sources);
        let sections = self.model.sections_folder();
        let ahead = self.model.line.display.ahead_deg;
        let whose = placed.as_ref().map(|p| p.structure.clone());
        self.status = Some(
            match self.structure_apps.launch(
                path,
                id,
                Some(&insulators),
                ahead,
                Some(&sections),
                placed,
            ) {
                Ok(()) => (
                    match whose {
                        Some(s) => format!(
                            "structure {id} opened in OHL-Structure with the loads \
                             of {s}: saved there, the line follows"
                        ),
                        None => format!(
                            "structure {id} opened in OHL-Structure: saved there, \
                             the line follows"
                        ),
                    },
                    false,
                ),
                Err(e) => (e, true),
            },
        );
    }

    /// Each frame: the structure files open in the structure application
    /// looked at (twice a second); one saved there goes into the project
    /// and the libraries are read again, as a save of the Structure types
    /// page; typed loads saved there go into the project; the placed
    /// structures' line values written again when the line changed.
    pub fn structure_apps_frame(&mut self, ctx: &egui::Context) {
        use crate::structure_app::Change;
        if !self.structure_apps.active() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        for c in self.structure_apps.poll() {
            match c {
                Change::Structure(path, id) => self.structure_file_saved(&path, &id),
                Change::Loads(_, whose) => {
                    self.status = Some(match self.model.store_libraries() {
                        Ok(n) if n > 0 => {
                            (format!("typed loads of {whose} kept in the project"), false)
                        }
                        Ok(_) => (
                            format!("typed loads of {whose} saved beside the structure file"),
                            false,
                        ),
                        Err(e) => (format!("keeping the typed loads of {whose}: {e}"), true),
                    });
                }
                // Libraries imported in the structure application (its
                // File › Libraries, into the working copy's folders): into
                // the project, and read again, as a save of a library page.
                Change::Libraries(dir, id) => {
                    log::info!(
                        "structure application on {id}: library files changed in {}",
                        dir.display()
                    );
                    self.library_saved(
                        crate::library::LibraryKind::StructureTypes,
                        crate::library::PageAction::Saved(dir),
                    );
                }
            }
        }
        if let Some(e) = self.structure_apps.refresh_values(&self.model).pop() {
            self.status = Some((e, true));
        }
    }

    /// Structure file `path` (structure `id`) saved outside: stored into
    /// the project, the libraries read again; the Structure types page
    /// shows the new text when it has no changes of its own.
    pub fn structure_file_saved(&mut self, path: &std::path::Path, id: &str) {
        use crate::library::{LibraryKind, PageAction};
        let kind = LibraryKind::StructureTypes;
        self.library_saved(kind, PageAction::Saved(path.to_path_buf()));
        if let Some(page) = self.libraries.get_mut(&kind)
            && page.file.as_deref() == Some(path)
        {
            if page.dirty() {
                self.status = Some((
                    format!(
                        "structure {id} was saved in OHL-Structure; the Structure \
                         types page has changes of its own (Discard to see the saved file)"
                    ),
                    true,
                ));
            } else {
                page.open(path);
            }
        }
    }

    /// **Edit…** on structure `id` (from Properties, the Structures
    /// table): its type in the structure application with the loads of
    /// this place; one without a type has no file to open there: its
    /// structure screen, asking to make it a structure file first.
    pub fn edit_structure_in_app(&mut self, id: &str) {
        let typed = self
            .model
            .line
            .structure(id)
            .and_then(|s| s.structure_type.clone());
        match typed {
            Some(t) => self.edit_in_structure_app(&t, Some(id)),
            None => self.make_file_and_edit(id),
        }
    }

    /// Structure `id` without a type: its structure screen, with the
    /// type id asked for — saved, used and opened in the structure
    /// application.
    pub fn make_file_and_edit(&mut self, id: &str) {
        log::info!("structure application: {id} has no structure file, make it one first");
        self.edit_structure(id);
        if let Some(p) = self.placed.as_mut().filter(|p| p.id == id) {
            p.ask_file_and_edit();
            self.status = Some((
                format!(
                    "{id} has no structure file: give it a type id and press Save, use and edit"
                ),
                false,
            ));
        }
    }

    /// The structure whose drag handles the 3D view shows: the selected
    /// one (or that of the string, wire point or point selected) while its
    /// structure screen is open (`open`: whether a page is) — a structure
    /// without a type in the Structure editor, a typed one with its type on
    /// the Structure types page; none when the project cannot be edited.
    /// A drag waits while that editor, or an editor window on it, has
    /// changes the drag would not keep.
    pub fn handle_target(&self, open: impl Fn(&Tab) -> bool) -> Option<crate::handles::Target> {
        use crate::library::LibraryKind;
        self.model.can_edit().ok()?;
        let id = match &self.selection {
            Selection::Structure(id) => id,
            Selection::Attachment { structure, .. }
            | Selection::String { structure, .. }
            | Selection::Point { structure, .. } => structure,
            _ => return None,
        };
        let s = self.model.line.structure(id)?;
        let mut blocked = None;
        let grid = match &s.structure_type {
            None => {
                let p = self.placed.as_ref().filter(|p| &p.id == id)?;
                if !open(&Tab::StructureEditor) {
                    return None;
                }
                if p.dirty() {
                    blocked = Some(format!(
                        "the structure editor of {id} has changes not applied: apply or revert                          them before dragging its points"
                    ));
                }
                p.workspace.grid
            }
            Some(t) => {
                let kind = LibraryKind::StructureTypes;
                let page = self.libraries.get(&kind)?;
                let path = self.model.types.as_ref()?.0.path(t)?;
                if !(page.workspace.on
                    && page.file.as_deref() == Some(path)
                    && open(&Tab::Library(kind)))
                {
                    return None;
                }
                page.workspace.grid
            }
        };
        let window = match &self.editor {
            Some(crate::editors::Form::String(f)) => f.structure == *id,
            Some(crate::editors::Form::Structure(f)) => !f.new && f.id == *id,
            _ => false,
        };
        if window && blocked.is_none() {
            blocked = Some(format!(
                "a window editing {id} is open: apply or cancel it before dragging its points"
            ));
        }
        Some(crate::handles::Target {
            structure: id.clone(),
            grid,
            blocked,
        })
    }

    /// A drag of a handle in the 3D view ended: the point moved (one
    /// command, as typed in its table), or why not.
    pub fn handle_done(&mut self, done: crate::handles::Done) {
        use crate::handles::Done;
        match done {
            Done::Moved {
                structure,
                handle,
                at,
                onto,
            } => match crate::handles::command_onto(
                &self.model,
                &structure,
                &handle,
                at,
                onto.as_ref(),
            ) {
                Ok((what, command)) => {
                    log::info!("{what}");
                    if self.perform(crate::edit::Action::Command {
                        what: what.clone(),
                        command,
                    }) {
                        self.status = Some((what, false));
                        // The structure editor shows the structure as it is
                        // now (it had no changes of its own: the drag waits
                        // otherwise).
                        if let Some(p) = self.placed.as_mut().filter(|p| p.id == structure)
                            && let Some(s) = self.model.line.structure(&structure)
                        {
                            p.doc = crate::placed_editor::as_type(s);
                            p.applied();
                            p.workspace.redraw();
                        }
                    }
                }
                Err(e) => self.status = Some((e, true)),
            },
            Done::Refused(why) => self.status = Some((why, true)),
            Done::Cancelled => {
                self.status = Some(("drag cancelled: nothing changed".into(), false));
            }
        }
    }

    /// Opens structure `id` (one without a type) in the pylon editor.
    pub fn edit_structure(&mut self, id: &str) {
        if self
            .placed
            .as_ref()
            .is_some_and(|p| p.id != id && p.dirty())
        {
            self.status = Some((
                "the structure editor has changes not applied: apply or revert them first".into(),
                true,
            ));
        } else if self.placed.as_ref().is_none_or(|p| p.id != id) {
            match crate::placed_editor::PlacedEditor::open(&self.model.line, id) {
                Ok(p) => self.placed = Some(p),
                Err(e) => {
                    self.status = Some((e, true));
                    return;
                }
            }
        }
        self.open_page = Some((Tab::StructureEditor, None));
    }

    /// The structure editor page.
    fn placed_ui(&mut self, ui: &mut egui::Ui, rs: Option<&egui_wgpu::RenderState>) {
        use crate::placed_editor::PlacedAction;
        let Some(mut p) = self.placed.take() else {
            ui.label(
                "No structure open. Select a structure without a type and press \
                 Strings and points… in Properties.",
            );
            return;
        };
        let empty = ohl_structures::InsulatorLibrary::default();
        let insulators = self.model.types.as_ref().map_or(&empty, |(_, i)| i);
        let line = self.model.line.clone();
        let action = p.ui(ui, rs, insulators, &line);
        let request = p.workspace.request.take();
        let stored = self.model.stored_line().unwrap_or(&self.model.line).clone();
        match action {
            Some(PlacedAction::Apply) => match p.command(&stored, insulators) {
                Ok((what, command)) => {
                    if self.perform(crate::edit::Action::Command { what, command }) {
                        p.applied();
                        p.message = Some(("applied".into(), false));
                    }
                }
                Err(e) => p.message = Some((e, true)),
            },
            Some(PlacedAction::MakeType { id: type_id, edit }) => {
                match self.make_type(&p, &stored, &type_id) {
                    Ok(()) => {
                        log::info!("structure {} made structure type {type_id}", p.id);
                        self.status = Some((format!("{} has type {type_id} now", p.id), false));
                        self.placed = None;
                        self.edit_type(&type_id);
                        if edit {
                            self.edit_in_structure_app(&type_id, Some(&p.id));
                        }
                        return;
                    }
                    Err(e) => p.message = Some((e, true)),
                }
            }
            None => {}
        }
        if let Some(req) = request {
            self.workspace_request(req);
        }
        self.placed = Some(p);
    }

    /// What the pylon editor asks: an insulator saved into the insulator
    /// library (refused when its id is taken), the insulator library
    /// opened.
    pub fn workspace_request(&mut self, req: crate::type_workspace::Request) {
        use crate::library::{LibraryKind, PageAction};
        use crate::type_workspace::Request;
        match req {
            // Taken by the page (the file as saved) or the structure
            // without a type (a file first).
            Request::EditInStructureApp => {}
            Request::OpenInsulators => {
                self.open_page = Some((Tab::Library(LibraryKind::Insulators), None));
            }
            Request::SaveInsulator { id, text } => {
                let taken = self
                    .model
                    .types
                    .as_ref()
                    .is_some_and(|(_, i)| i.get(&id).is_some());
                let dir = LibraryKind::Insulators.new_file_dir(&self.model.sources);
                let path = dir.join(format!(
                    "{id}.{}",
                    ohl_structures::insulators::INSULATOR_FILE_EXTENSION
                ));
                let result = if taken || path.exists() {
                    Err(format!(
                        "there is an insulator assembly {id} already (rename it in the library)"
                    ))
                } else {
                    std::fs::create_dir_all(&dir)
                        .and_then(|()| std::fs::write(&path, &text))
                        .map_err(|e| format!("{}: {e}", path.display()))
                };
                match result {
                    Ok(()) => {
                        self.library_saved(
                            LibraryKind::Insulators,
                            PageAction::Saved(path.clone()),
                        );
                        self.status = Some((
                            format!("insulator assembly {id} saved to the library"),
                            false,
                        ));
                    }
                    Err(e) => self.status = Some((e, true)),
                }
            }
        }
    }

    /// The folder of the project's own structure types: its folder, else a
    /// Structures folder next to the line (never the shipped library).
    fn project_types_dir(&self) -> std::path::PathBuf {
        let sources = &self.model.sources;
        match &sources.structures {
            Some(_) => crate::library::LibraryKind::StructureTypes.new_file_dir(sources),
            None => crate::model::model_dir(sources).join("Structures"),
        }
    }

    /// Writes structure type `id` built from a pylon template (`replace`:
    /// the file of the type it replaces); the library is read again.
    pub fn save_built_type(
        &mut self,
        id: &str,
        text: &str,
        replace: Option<std::path::PathBuf>,
    ) -> Result<std::path::PathBuf, String> {
        let path = match replace {
            Some(p) => p,
            None => {
                let dir = self.project_types_dir();
                std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                let p = dir.join(format!("{id}.ohls"));
                if ohl_core::files::exists_by_either_name(&p) {
                    return Err(format!("{} exists already", p.display()));
                }
                p
            }
        };
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.library_saved(
            crate::library::LibraryKind::StructureTypes,
            crate::library::PageAction::Saved(path.clone()),
        );
        Ok(path)
    }

    /// Saves the editor's document as structure type `type_id` of the
    /// project and gives its structure that type (its own points, strings
    /// and sticks go: they come from the type).
    fn make_type(
        &mut self,
        p: &crate::placed_editor::PlacedEditor,
        stored: &ohl_line::Line,
        type_id: &str,
    ) -> Result<(), String> {
        let kind = crate::library::LibraryKind::StructureTypes;
        if self
            .model
            .types
            .as_ref()
            .is_some_and(|(t, _)| t.path(type_id).is_some())
        {
            return Err(format!("there is a structure type {type_id} already"));
        }
        let dir = self.project_types_dir();
        let path = dir.join(format!("{type_id}.ohls"));
        if ohl_core::files::exists_by_either_name(&path) {
            return Err(format!("{} exists already", path.display()));
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, p.type_text(type_id))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        self.library_saved(kind, crate::library::PageAction::Saved(path.clone()));
        let mut s = stored
            .structure(&p.id)
            .ok_or_else(|| format!("no structure {}", p.id))?
            .clone();
        s.structure_type = Some(type_id.to_owned());
        s.points.clear();
        s.strings.clear();
        s.wires.clear();
        s.members.clear();
        s.omit.clear();
        let command = ohl_session::Command::Put {
            kind: ohl_session::Kind::Structure,
            id: p.id.clone(),
            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
            ord: None,
        };
        if self.perform(crate::edit::Action::Command {
            what: format!("{} takes structure type {type_id}", p.id),
            command,
        }) {
            Ok(())
        } else {
            Err(self
                .status
                .clone()
                .map_or_else(|| "not changed".into(), |(m, _)| m))
        }
    }

    /// Adds jumper `j` (`what`: the change in words; `open_editor`: its
    /// window after). A jumper between two different circuits waits for
    /// the user first ([`crate::circuit_join`]).
    pub fn add_jumper(&mut self, j: ohl_line::Jumper, what: String, open_editor: bool) {
        if let Some(join) =
            crate::circuit_join::CircuitJoin::check(&self.model, &j, &what, open_editor)
        {
            self.status = Some((
                format!(
                    "Add jumper: {} joins two different circuits — choose in the window",
                    j.id
                ),
                false,
            ));
            self.circuit_join = Some(join);
            return;
        }
        let id = j.id.clone();
        let command = ohl_session::Command::Put {
            kind: ohl_session::Kind::Jumper,
            id: id.clone(),
            value: serde_json::to_value(&j).expect("a jumper"),
            ord: None,
        };
        self.jumper_added(
            id,
            crate::edit::Action::Command { what, command },
            open_editor,
        );
    }

    /// Performs the adding of jumper `id`, selects it, opens its window.
    fn jumper_added(&mut self, id: String, action: crate::edit::Action, open_editor: bool) {
        if self.perform(action) {
            self.select(Selection::Jumper(id.clone()));
            if open_editor {
                self.editor = crate::editors::JumperForm::of(&self.model, &id)
                    .map(|f| crate::editors::Form::Jumper(Box::new(f)));
            }
        }
    }

    /// Replace structure on structure `id`: the structure picker, then
    /// (when not every set matches) the mapping window
    /// ([`crate::replace`]).
    pub fn start_replace(&mut self, id: &str) {
        match crate::replace::Replace::start(&self.model, id) {
            Ok(r) => {
                self.replace = Some(r);
                self.popup_open = false;
            }
            Err(e) => self.status = Some((e, true)),
        }
    }

    pub fn perform(&mut self, action: crate::edit::Action) -> bool {
        use crate::edit::Action;
        // The strings an edit puts without a library label are taken into
        // the library in the same command (one undo takes both back).
        let action = match action {
            Action::Command { what, mut command } if !self.cataloguing => {
                self.label_strings_in(&mut command);
                Action::Command { what, command }
            }
            other => other,
        };
        self.perform_one(action)
    }

    /// The structures `command` puts (in a batch too): their strings
    /// without a library label get one, new library entries written first
    /// ([`ohl_line::Line::catalogue_strings`] on the line as it will be).
    fn label_strings_in(&mut self, command: &mut ohl_session::Command) {
        if self.model.can_edit().is_err() {
            return;
        }
        let mut puts: Vec<&mut serde_json::Value> = Vec::new();
        fn collect<'a>(c: &'a mut ohl_session::Command, out: &mut Vec<&'a mut serde_json::Value>) {
            match c {
                ohl_session::Command::Put {
                    kind: ohl_session::Kind::Structure,
                    value,
                    ..
                } => out.push(value),
                ohl_session::Command::Batch { commands } => {
                    for c in commands {
                        collect(c, out);
                    }
                }
                _ => {}
            }
        }
        collect(command, &mut puts);
        if puts.is_empty() {
            return;
        }
        let Some(mut line) = self.model.stored_line().cloned() else {
            return;
        };
        let mut put: Vec<String> = Vec::new();
        for value in &puts {
            let Ok(s) = serde_json::from_value::<ohl_line::Structure>((*value).clone()) else {
                return;
            };
            if !s.strings.iter().any(|x| x.insulator.is_none()) {
                continue;
            }
            put.push(s.id.clone());
            match line.structures.iter_mut().find(|t| t.id == s.id) {
                Some(t) => *t = s,
                None => line.structures.push(s),
            }
        }
        if put.is_empty() {
            return;
        }
        let empty_s = ohl_structures::StructureLibrary::default();
        let empty_i = ohl_structures::InsulatorLibrary::default();
        let (structures, library) = self
            .model
            .types
            .as_ref()
            .map_or((&empty_s, &empty_i), |(s, i)| (s, i));
        let Ok(resolved) = line.resolve(structures, library) else {
            return;
        };
        let mut c = line.catalogue_strings(&resolved, library);
        c.links.retain(|(s, _, _)| put.contains(s));
        let used: std::collections::BTreeSet<&str> =
            c.links.iter().map(|(_, _, id)| id.as_str()).collect();
        c.new.retain(|a| used.contains(a.id.as_str()));
        if c.is_empty() || !self.write_library_entries(&c.new) {
            return;
        }
        c.apply_to(&mut line);
        for value in puts {
            if let Some(id) = value.get("id").and_then(|v| v.as_str()).map(str::to_owned)
                && put.contains(&id)
                && let Some(s) = line.structure(&id)
            {
                *value = serde_json::to_value(s).expect("a structure");
            }
        }
    }

    /// Writes new insulator library entries as files (stored in the
    /// project, the libraries read again); false (the status says why)
    /// when one cannot be written.
    fn write_library_entries(&mut self, new: &[ohl_structures::InsulatorAssembly]) -> bool {
        if new.is_empty() {
            return true;
        }
        let dir = crate::library::LibraryKind::Insulators.new_file_dir(&self.model.sources);
        let mut first = None;
        for a in new {
            let path = dir.join(format!(
                "{}.{}",
                a.id,
                ohl_structures::insulators::INSULATOR_FILE_EXTENSION
            ));
            let written =
                std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, a.to_toml()));
            if let Err(e) = written {
                self.status = Some((format!("{}: {e}", path.display()), true));
                return false;
            }
            first.get_or_insert(path);
        }
        if let Some(path) = first {
            self.library_saved(
                crate::library::LibraryKind::Insulators,
                crate::library::PageAction::Saved(path),
            );
        }
        true
    }

    /// The model's own strings without a library label taken into the
    /// insulator library ([`ohl_line::Line::catalogue_strings`]): new
    /// entries written as library files (stored in the project), the
    /// strings labelled in one command. Only where the model can be
    /// edited; nothing to do: nothing happens.
    pub fn catalogue_strings(&mut self) {
        if self.model.can_edit().is_err() {
            return;
        }
        let Some(stored) = self.model.stored_line().cloned() else {
            return;
        };
        let empty = ohl_structures::InsulatorLibrary::default();
        let library = self.model.types.as_ref().map_or(&empty, |(_, i)| i);
        let c = stored.catalogue_strings(&self.model.line, library);
        if c.is_empty() {
            return;
        }
        if !self.write_library_entries(&c.new) {
            return;
        }
        self.cataloguing = true;
        let mut labelled = stored.clone();
        c.apply_to(&mut labelled);
        let touched: std::collections::BTreeSet<&str> =
            c.links.iter().map(|(s, _, _)| s.as_str()).collect();
        let commands = labelled
            .structures
            .iter()
            .filter(|s| touched.contains(s.id.as_str()))
            .map(|s| ohl_session::Command::Put {
                kind: ohl_session::Kind::Structure,
                id: s.id.clone(),
                value: serde_json::to_value(s).expect("a structure"),
                ord: None,
            })
            .collect();
        let what = format!(
            "strings taken into the library: {} labelled, {} new entries",
            c.links.len(),
            c.new.len()
        );
        log::info!("{what}");
        self.perform(crate::edit::Action::Command {
            what,
            command: ohl_session::Command::Batch { commands },
        });
        self.cataloguing = false;
    }

    fn perform_one(&mut self, action: crate::edit::Action) -> bool {
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
            Action::EditType { id } => {
                self.edit_type(&id);
                return true;
            }
            Action::EditInStructureApp { id, structure } => {
                self.edit_in_structure_app(&id, structure.as_deref());
                return true;
            }
            Action::EditStructure { id } => {
                self.edit_structure(&id);
                return true;
            }
            Action::MakeFileAndEdit { id } => {
                self.make_file_and_edit(&id);
                return true;
            }
            Action::ReplaceStructure { id } => {
                self.start_replace(&id);
                return true;
            }
            Action::EditPlacement { id } => {
                self.editor = crate::editors::StructureForm::of(&self.model, &id)
                    .map(crate::editors::Form::Structure);
                // Opened at the mouse, over a window already open.
                self.popup_open = false;
                return true;
            }
            Action::Command { what, command } => self.model.apply(&what, command),
            Action::DeleteAttachment {
                structure,
                set,
                confirm,
            } => self.model.apply(
                &format!("deleted set {set} of {structure}"),
                ohl_session::Command::DeleteAttachment {
                    structure,
                    set,
                    phase: None,
                    confirm,
                },
            ),
            Action::Move { id, position } => self.model.apply(
                &format!("moved {id}"),
                ohl_session::Command::MoveStructure {
                    id: id.clone(),
                    position,
                },
            ),
            Action::Delete { kind, id, confirm } => self.model.apply(
                &format!("deleted {} {id}", kind.as_str()),
                ohl_session::Command::Delete { kind, id, confirm },
            ),
            Action::Undo => self.model.undo(),
            Action::Redo => self.model.redo(),
        };
        let done = match result {
            Ok(report) => {
                log::info!("{report}");
                // Structures on no route (a line with routes): said, not
                // refused.
                let line = &self.model.line;
                let lost: Vec<String> = if line.routes.is_empty() {
                    Vec::new()
                } else {
                    line.structure_routes()
                        .into_iter()
                        .filter(|(_, r)| r.is_empty())
                        .map(|(id, _)| id)
                        .collect()
                };
                let report = match lost.len() {
                    0 => report,
                    n => format!(
                        "{report}; warning: {n} structure(s) on no route ({}{}) — put them on an \
                         alignment or give their route",
                        lost.iter().take(5).cloned().collect::<Vec<_>>().join(", "),
                        if n > 5 { "…" } else { "" }
                    ),
                };
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
            .retain(|p| self.model.line.wire_point(p).is_some());
        self.update_draft();
        self.refresh_views();
        done
    }

    /// After the line changed: the 3D layers of the line (not the ground
    /// and the LiDAR points: an edit does not change them, and indexing
    /// 5 million triangles took 0.3 s) and the profile again.
    pub(crate) fn refresh_views(&mut self) {
        self.view3d.dirty.extend(
            Layer::ALL
                .into_iter()
                .filter(|l| !matches!(l, Layer::Ground | Layer::Points)),
        );
        self.profile.invalidate();
        self.xsections.invalidate();
        self.drawings.invalidate_preview();
    }

    /// Whether the selected object is still in the line.
    fn selection_exists(&self) -> bool {
        let line = &self.model.line;
        match &self.selection {
            Selection::None | Selection::LidarPoint(_) | Selection::SurveyPoint(_) => true,
            Selection::Structure(id) => line.structure(id).is_some(),
            Selection::Jumper(id) => line.jumpers.iter().any(|j| &j.id == id),
            Selection::JumperPoint { jumper, label } => line
                .jumpers
                .iter()
                .any(|j| &j.id == jumper && j.points.iter().any(|p| &p.label == label)),
            Selection::Span { section, span } => line
                .sections
                .iter()
                .any(|s| &s.id == section && *span + 1 < s.points.len()),
            Selection::Attachment { structure, set } => line
                .wire_point(&ohl_line::SectionPoint::new(structure.clone(), *set))
                .is_some(),
            Selection::String { structure, label } => line
                .structure(structure)
                .is_some_and(|s| s.string(label).is_some()),
            Selection::Point { structure, label } => line
                .structure(structure)
                .is_some_and(|s| s.node(label).is_some()),
            Selection::AlignmentPoint { id } => line.alignment_point(id).is_some(),
            Selection::SpanPoint { section, label } => line
                .sections
                .iter()
                .any(|s| &s.id == section && s.span_points.iter().any(|p| &p.label == label)),
            Selection::Spread { section, index } => line
                .sections
                .iter()
                .any(|s| &s.id == section && *index < s.spreads.len()),
            Selection::Spacer(id) => line.spacers.iter().any(|s| &s.id == id),
        }
    }

    /// Deletes `target` (as the Delete tool, Delete selected): a spread,
    /// or a point a spread ends on, asks first what goes with it (the
    /// spread window); anything else is deleted (a confirmation when it
    /// takes more). False: nothing to delete.
    pub fn delete_object(&mut self, target: &Selection) -> bool {
        let line = &self.model.line;
        let ask = match target {
            Selection::Spread { section, index } => Some(SpreadDelete {
                section: section.clone(),
                spread: Some(*index),
                point: None,
            }),
            Selection::SpanPoint { section, label }
                if line
                    .sections
                    .iter()
                    .any(|s| &s.id == section && !s.spreads_at(label).is_empty()) =>
            {
                Some(SpreadDelete {
                    section: section.clone(),
                    spread: None,
                    point: Some(label.clone()),
                })
            }
            _ => None,
        };
        if let Some(ask) = ask {
            self.spread_delete = Some(ask);
            return true;
        }
        match delete_action(line, target) {
            Some(action) => {
                self.perform(action);
                true
            }
            None => false,
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
            (
                Selection::Jumper(j.id.clone()),
                line.end_structures(&j.from)
                    .into_iter()
                    .next()
                    .unwrap_or_default(),
            )
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

/// A deletion touching spread objects, asked first
/// ([`spread_delete_window`]): spread `spread` of section `section` (with
/// its points or not), or its point `point` with the spreads ending on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadDelete {
    /// Section id.
    pub section: String,
    /// The spread deleted (its index).
    pub spread: Option<usize>,
    /// The point deleted (the end of spreads).
    pub point: Option<String>,
}

/// The span spread `index` of section `section` lies on (that of its
/// first point).
pub(crate) fn spread_span(line: &ohl_line::Line, section: &str, index: usize) -> Option<usize> {
    let s = line.sections.iter().find(|s| s.id == section)?;
    let l = s.spreads.get(index)?;
    s.span_points
        .iter()
        .find(|p| p.label == l.from)
        .map(|p| p.span)
}

/// Whether `target` can be deleted ([`Ready::delete_object`]).
fn deletable(line: &ohl_line::Line, target: &Selection) -> bool {
    matches!(target, Selection::Spread { .. }) || delete_action(line, target).is_some()
}

/// What keeps span point `label` of section `sec` when spread `but`
/// goes: its object, the other spreads on it, the spacers clamped on it,
/// the jumpers on it (a tap). Empty: a bare point nothing else uses.
fn point_uses(
    line: &ohl_line::Line,
    sec: &ohl_line::Section,
    label: &str,
    but: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(o) = sec
        .span_points
        .iter()
        .find(|p| p.label == label)
        .and_then(|p| p.object.as_ref())
    {
        out.push(format!("holds a {}", o.kind.as_str()));
    }
    for k in sec.spreads_at(label).into_iter().filter(|k| *k != but) {
        out.push(format!(
            "ends spread {}",
            crate::properties::spread_name(sec, k)
        ));
    }
    for x in line.spacers.iter().filter(|x| {
        [&x.a, &x.b]
            .iter()
            .any(|e| e.section == sec.id && e.point == label)
    }) {
        out.push(format!("spacer {}", x.id));
    }
    for j in line.jumpers.iter().filter(|j| {
        j.taps()
            .any(|t| matches!(t, ohl_line::Tap::Span(r) if r.section == sec.id && r.point == label))
    }) {
        out.push(format!("jumper {} (a tap)", j.id));
    }
    out
}

/// The deletion of span points `points` and spreads `spreads` (indices)
/// of section `section`, with the spacers clamped on those points: one
/// command.
fn span_delete_action(
    line: &ohl_line::Line,
    section: &str,
    points: &[String],
    spreads: &[usize],
) -> Option<crate::edit::Action> {
    use ohl_session::Kind;
    let mut s = line.sections.iter().find(|s| s.id == section)?.clone();
    let names: Vec<String> = spreads
        .iter()
        .map(|k| crate::properties::spread_name(&s, *k))
        .collect();
    s.span_points.retain(|p| !points.contains(&p.label));
    let mut k = 0;
    s.spreads.retain(|_| {
        k += 1;
        !spreads.contains(&(k - 1))
    });
    let spacers: Vec<&ohl_line::Spacer> = line
        .spacers
        .iter()
        .filter(|x| {
            [&x.a, &x.b]
                .iter()
                .any(|e| e.section == section && points.contains(&e.point))
        })
        .collect();
    let mut commands: Vec<ohl_session::Command> = spacers
        .iter()
        .map(|x| ohl_session::Command::Delete {
            kind: Kind::Spacer,
            id: x.id.clone(),
            confirm: true,
        })
        .collect();
    commands.push(ohl_session::Command::Put {
        kind: Kind::Section,
        id: section.to_owned(),
        value: serde_json::to_value(&s).ok()?,
        ord: None,
    });
    let mut what = Vec::new();
    if !points.is_empty() {
        what.push(format!("span point(s) {}", points.join(", ")));
    }
    if !names.is_empty() {
        what.push(format!("spread(s) {}", names.join(", ")));
    }
    if !spacers.is_empty() {
        what.push(format!(
            "spacers {}",
            spacers
                .iter()
                .map(|x| x.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Some(crate::edit::Action::Command {
        what: format!("deleted {} of {section}", what.join(" and ")),
        command: ohl_session::Command::Batch { commands },
    })
}

/// The deletion of a right-clicked object: a structure, a string, a span's
/// section, a jumper.
fn delete_action(line: &ohl_line::Line, target: &Selection) -> Option<crate::edit::Action> {
    use crate::edit::Action;
    use ohl_session::Kind;
    let del = |kind, id: &String| Action::Delete {
        kind,
        id: id.clone(),
        confirm: false,
    };
    Some(match target {
        Selection::Structure(id) => del(Kind::Structure, id),
        Selection::Span { section, .. } => del(Kind::Section, section),
        Selection::Jumper(id) => del(Kind::Jumper, id),
        Selection::Spacer(id) => del(Kind::Spacer, id),
        // The jumper without it, and the jumpers hanging on it (a tap).
        Selection::JumperPoint { jumper, label } => {
            let mut j = line.jumpers.iter().find(|j| &j.id == jumper)?.clone();
            j.points.retain(|p| &p.label != label);
            let hanging: Vec<&ohl_line::Jumper> = line
                .jumpers
                .iter()
                .filter(|x| {
                    x.taps().any(|t| {
                        matches!(t, ohl_line::Tap::Jumper { jumper: h, point } if h == jumper && point == label)
                    })
                })
                .collect();
            let mut commands: Vec<ohl_session::Command> = hanging
                .iter()
                .map(|x| ohl_session::Command::Delete {
                    kind: Kind::Jumper,
                    id: x.id.clone(),
                    confirm: true,
                })
                .collect();
            commands.push(ohl_session::Command::Put {
                kind: Kind::Jumper,
                id: j.id.clone(),
                value: serde_json::to_value(&j).ok()?,
                ord: None,
            });
            Action::Command {
                what: if hanging.is_empty() {
                    format!("deleted point {label} of jumper {jumper}")
                } else {
                    format!(
                        "deleted point {label} of jumper {jumper} and the jumpers on it: {}",
                        hanging
                            .iter()
                            .map(|x| x.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
                command: ohl_session::Command::Batch { commands },
            }
        }
        // The section without it, and the spacers clamped on it (a
        // spread's end: asked first, [`Ready::delete_object`]).
        Selection::SpanPoint { section, label } => {
            span_delete_action(line, section, std::slice::from_ref(label), &[])?
        }
        Selection::Attachment { structure, set } => Action::DeleteAttachment {
            structure: structure.clone(),
            set: *set,
            confirm: false,
        },
        // A string: the strings of its set, with what goes with them.
        Selection::String { structure, label } => {
            let s = line.structure(structure)?;
            let c = s.component(&s.string(label)?.wire);
            let set = c.wires.iter().find_map(|&i| s.wires[i].set)?;
            Action::DeleteAttachment {
                structure: structure.clone(),
                set,
                confirm: false,
            }
        }
        // Off every alignment it is on (asked first when on several).
        Selection::AlignmentPoint { id } => Action::Delete {
            kind: ohl_session::Kind::AlignmentPoint,
            id: id.clone(),
            confirm: false,
        },
        _ => return None,
    })
}

/// C opens the Cross-section window (not while typing; ⌘C / Ctrl+C is
/// not taken).
fn cut_key(ctx: &egui::Context, r: &mut Ready) {
    if !ctx.text_edit_focused()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::C))
    {
        r.open_cut_dialog();
    }
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
    // A menu opened: what was going on stops, as with Escape — the mouse
    // is freed, a wire being strung and an alignment being laid out are
    // given up — so the menu's command works on the first click.
    let menu = egui::Popup::is_any_open(ctx);
    if menu && !r.menu_open {
        log::info!("a menu opened; mouse captured: {}", r.view3d.captured);
        if r.view3d.captured {
            crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
        }
        r.xsections.free_mouse(ctx);
        if !r.chain.is_empty() {
            r.cancel_string_wire();
        }
        if !r.new_alignment.is_empty() {
            r.cancel_new_alignment();
        }
    }
    r.menu_open = menu;
    if std::mem::take(&mut r.release_mouse) {
        if r.view3d.captured {
            log::info!("a page opened from the 3D view: the mouse is freed");
            crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
        }
        r.xsections.free_mouse(ctx);
    }
    r.popup_fresh = r.popup_opened == Some(ctx.cumulative_frame_nr());
    let open = r.goto.is_some()
        || r.coord_dialog.is_some()
        || r.route_choice.is_some()
        || r.move_dialog.is_some()
        || r.point_delete.is_some()
        || r.spread_delete.is_some()
        || r.confirm.is_some()
        || r.replace.is_some()
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
        // Kept on the screen (at least its top-left part).
        let screen = ctx.content_rect();
        let at = at.unwrap_or(egui::pos2(400.0, 200.0));
        r.popup_at = egui::pos2(
            at.x.clamp(screen.min.x, (screen.max.x - 320.0).max(screen.min.x)),
            at.y.clamp(screen.min.y, (screen.max.y - 240.0).max(screen.min.y)),
        );
        r.popup_serial += 1;
        r.popup_opened = Some(ctx.cumulative_frame_nr());
        r.popup_fresh = true;
        log::info!("a window opens at {:?}", r.popup_at);
        if r.view3d.captured {
            crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
        }
        r.xsections.free_mouse(ctx);
    }
    r.popup_open = open;
}

/// A tool window: opens next to [`Ready::popup_at`] (again each time it
/// opens, whatever a window of its id was in an earlier run: window
/// places are remembered between runs), can be dragged anywhere, also
/// partly off the screen.
fn tool_window(r: &Ready, key: &str, title: impl Into<egui::WidgetText>) -> egui::Window<'static> {
    let at = r.popup_at + egui::vec2(16.0, 16.0);
    let w = egui::Window::new(title).id(egui::Id::new((key.to_owned(), r.popup_serial)));
    // Fixed in the frame it opens: egui (0.36) puts a movable window back
    // where it was remembered, over `current_pos`.
    let w = if r.popup_fresh {
        w.fixed_pos(at)
    } else {
        w.default_pos(at)
    };
    w.collapsible(false).resizable(false).constrain(false)
}

/// The Cross-section window: Open cut (or Enter) opens a cut in a tab of
/// its own, Escape closes. A structure clicked in a view while it is open
/// is taken.
fn cut_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut d) = r.xsections.dialog.take() else {
        return;
    };
    d.follow(&r.selection);
    let views = r.xsections.alignments(&r.model.line);
    let mut out = crate::xsection::DialogOut::None;
    tool_window(r, "xsection", "Cross-section").show(ctx, |ui| {
        out = crate::xsection::dialog_ui(ui, &mut d, &r.model.line, &views);
    });
    if out == crate::xsection::DialogOut::None && !ctx.text_edit_focused() {
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            out = crate::xsection::DialogOut::Open;
        } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            out = crate::xsection::DialogOut::Close;
        }
    }
    match out {
        crate::xsection::DialogOut::Open => match d.spec(&views) {
            Some(spec) => r.open_cut(spec),
            None => r.xsections.dialog = Some(d),
        },
        crate::xsection::DialogOut::Close => {}
        crate::xsection::DialogOut::None => r.xsections.dialog = Some(d),
    }
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
    // back. Escape (mouse captured or not, a field focused) cancels it: the
    // points go; the tool stays held and the view keeps the mouse (the key
    // is taken: a second Escape frees the mouse). (The section window
    // opened by Enter: [`editor_window`].)
    // A handle being dragged in the 3D view: Escape gives the drag up.
    let cut_drag = r
        .xsections
        .views
        .values()
        .any(|v| v.view.handles.drag.is_some());
    if (r.view3d.handles.drag.is_some() || cut_drag)
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.view3d.handles.cancel();
        for v in r.xsections.views.values_mut() {
            v.view.handles.cancel();
        }
        r.status = Some(("drag cancelled: nothing changed".into(), false));
    }
    let windows = r.editor.is_some() || r.goto.is_some() || r.move_dialog.is_some();
    if !r.chain.is_empty()
        && !windows
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.cancel_string_wire();
    }
    // A jumper being laid out (Add jumper): Escape gives it up.
    if (r.jumper_from.is_some() || r.jumper_tap.is_some())
        && !windows
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.jumper_from = None;
        r.jumper_tap = None;
        r.jumper_via.clear();
        r.status = Some(("Add jumper: cancelled".into(), false));
    }
    // An attachment point chosen with the Link tool: Escape gives it up.
    if r.link_from.is_some()
        && !windows
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.link_from = None;
        r.status = Some(("Link: cancelled".into(), false));
    }
    // A spacer being laid out (Add string): Escape gives it up.
    if r.spacer_from.is_some()
        && !windows
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.spacer_from = None;
        r.status = Some(("Add spacer: cancelled".into(), false));
    }
    // A new alignment being laid out: the same keys.
    if !r.new_alignment.is_empty() && !windows {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            r.cancel_new_alignment();
        } else if !ctx.text_edit_focused() {
            let (enter, back) = ctx.input_mut(|i| {
                use egui::{Key, Modifiers};
                (
                    i.consume_key(Modifiers::NONE, Key::Enter),
                    i.consume_key(Modifiers::NONE, Key::Backspace),
                )
            });
            if enter && r.reroute {
                r.finish_reroute(None);
            } else if enter {
                r.finish_new_alignment();
            } else if back {
                r.new_alignment.pop();
                if r.new_alignment.is_empty() {
                    r.reroute = false;
                }
                r.alignment_drafted();
            }
        }
    }
    if !r.chain.is_empty() && !ctx.text_edit_focused() && !windows {
        let (enter, back) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            (
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Backspace),
            )
        });
        if enter {
            r.finish_chain();
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
            if r.inventory {
                r.xsections.free_mouse(ctx);
            }
        }
        if let Some(k) = slot {
            r.hotbar.active = k;
        }
        if delete && editable {
            let target = r.selection.clone();
            r.delete_object(&target);
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
/// A new project's first steps: 1. its route, 2. its alignment (the New
/// alignment tool held, with a hint).
fn first_steps_window(ctx: &egui::Context, r: &mut Ready) {
    let Some((name, description)) = &mut r.first_steps else {
        return;
    };
    let mut create = false;
    let mut later = false;
    egui::Window::new("New project: first steps")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new("1. The route").strong());
            ui.label(
                egui::RichText::new(
                    "A route is the path of the line: it is made of alignments, and the \
                     structures, wires and checks belong to it. Name it:",
                )
                .small(),
            );
            egui::Grid::new("first-route")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("name");
                    ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                    ui.end_row();
                    ui.label("description");
                    ui.add(
                        egui::TextEdit::singleline(description)
                            .desired_width(200.0)
                            .hint_text("what it is (free text)"),
                    );
                    ui.end_row();
                });
            ui.label(egui::RichText::new("2. Its alignment").strong());
            ui.label(
                egui::RichText::new(
                    "Then draw its alignment: the New alignment tool is held — right-click its \
                     points in order (a survey point, or coordinates typed), Enter to make it.",
                )
                .small(),
            );
            ui.horizontal(|ui| {
                create = ui
                    .add_enabled(
                        !name.trim().is_empty(),
                        egui::Button::new("Create the route"),
                    )
                    .clicked();
                later = ui.button("Later").clicked();
            });
        });
    if later {
        r.first_steps = None;
    }
    if create && let Some((name, description)) = r.first_steps.clone() {
        let stored = r.model.stored_line().unwrap_or(&r.model.line).clone();
        match crate::editors::new_route(&stored, &name, &name) {
            Ok((what, mut command)) => {
                if let ohl_session::Command::Put { value, .. } = &mut command {
                    value["description"] = description.trim().into();
                }
                if r.perform(crate::edit::Action::Command { what, command }) {
                    r.first_steps = None;
                    r.hotbar.hold(crate::tools::Tool::NewAlignment);
                    r.status = Some((
                        format!(
                            "Route {} made. Step 2, its alignment: right-click its points in \
                             order, Enter to make it",
                            name.trim()
                        ),
                        false,
                    ));
                }
            }
            Err(e) => r.status = Some((e, true)),
        }
    }
}

/// The alignment tools' windows: a point's coordinates, a new alignment's
/// routes.
fn alignment_windows(ctx: &egui::Context, r: &mut Ready) {
    if let Some(d) = &mut r.coord_dialog
        && let Some(done) = d.ui(ctx)
    {
        let (purpose, connect, structure) = (d.purpose, d.connect, d.structure.clone());
        r.coord_dialog = None;
        if let Ok(p) = done {
            let point = ohl_line::AlignmentPoint {
                structure,
                ..ohl_line::AlignmentPoint::free("", p)
            };
            if purpose == crate::alignment_tools::CoordFor::AddAlignment && !connect {
                r.add_free_point(point);
            } else {
                r.place_alignment_point(purpose, point);
            }
        }
    }
    // (The line copied only while the window is open: every frame.)
    let Some(stored) = r
        .route_choice
        .is_some()
        .then(|| r.model.stored_line().unwrap_or(&r.model.line).clone())
    else {
        return;
    };
    if let Some(c) = &mut r.route_choice
        && let Some(action) = c.ui(ctx, &stored)
    {
        use crate::alignment_tools::RouteChoiceAction;
        match action {
            RouteChoiceAction::Make { route, new } => r.make_new_alignment(route, new),
            RouteChoiceAction::Back => {
                if let Some(c) = r.route_choice.take() {
                    r.new_alignment = c.points;
                    r.alignment_drafted();
                }
            }
        }
    }
}

/// The pylon builder.
fn pylon_windows(ctx: &egui::Context, r: &mut Ready) {
    if let Some(mut b) = r.pylon_builder.take() {
        let mut open = true;
        if let Some(crate::pylon_builder::BuilderAction::Save {
            id,
            text,
            replace,
            notes,
        }) = b.ui(ctx, &r.model, &mut open)
        {
            let again = replace.is_some();
            b.message = Some(match r.save_built_type(&id, &text, replace) {
                Ok(p) => {
                    // Built again: the structures of that type follow.
                    if again {
                        r.refresh_views();
                    }
                    b.rebuild = Some(id.clone());
                    let mut m = format!("structure type {id} written: {}", p.display());
                    if !notes.is_empty() {
                        m.push_str("\nKept, to look at:");
                        for n in &notes {
                            m.push_str(&format!("\n- {n}"));
                        }
                    }
                    (m, !notes.is_empty())
                }
                Err(e) => (e, true),
            });
        }
        if open {
            r.pylon_builder = Some(b);
        }
    }
}

fn tool_windows(ctx: &egui::Context, r: &mut Ready, rs: Option<&eframe::egui_wgpu::RenderState>) {
    // The first structures of an empty line: the plan fits them.
    let on_line = !r.model.line.structures.is_empty();
    if on_line && !r.plan_on_line {
        r.plan.refit();
    }
    r.plan_on_line = on_line;
    // At the bottom of the 3D view, only while it is shown (and while the
    // inventory arranges it).
    let shown = r.view3d.drawn;
    r.view3d.drawn = false;
    r.plan.drawn = None;
    // Else at the bottom of a cut's panel shown.
    let mut cut_rect = None;
    for v in r.xsections.views.values_mut() {
        if std::mem::take(&mut v.view.drawn) && cut_rect.is_none() {
            cut_rect = v.view.rect;
        }
    }
    if shown || cut_rect.is_some() || r.inventory {
        let at = match (shown, r.view3d.rect, cut_rect) {
            (true, Some(v), _) => v.center_bottom(),
            (false, _, Some(c)) => c.center_bottom(),
            _ => ctx.content_rect().center_bottom(),
        };
        crate::tools::hotbar_ui(
            ctx,
            &mut r.hotbar,
            at,
            r.inventory,
            r.view3d.controls.tools_on_left,
        );
    }
    crate::tools::inventory_ui(ctx, &mut r.inventory, &mut r.hotbar);
    goto_window(ctx, r);
    cut_window(ctx, r);
    move_window(ctx, r);
    point_delete_window(ctx, r);
    spread_delete_window(ctx, r);
    first_steps_window(ctx, r);
    alignment_windows(ctx, r);
    crate::replace::window(ctx, r, rs);
    editor_window(ctx, r, rs);
    crate::lidar_edit::frame(ctx, r);
    display_window(ctx, r);
    angles_window(ctx, r);
    survey_window(ctx, r);
    survey_io_windows(ctx, r);
    r.drawings.show(ctx, &mut r.model, r.case);
    if std::mem::take(&mut r.model.drawings_setting_changed) {
        r.drawings.load(&r.model);
    }
    r.drawings.save(&mut r.model, ctx);
    controls_window(ctx, r);
    creep_stretch_window(ctx, r);
    bimetallic_window(ctx, r);
    solver_window(ctx, r);
    circuit_join_window(ctx, r);
    defaults_window(ctx, r);
    crs_windows(ctx, r);
    if r.backgrounds.show(ctx, &r.model) {
        r.reopen = true;
    }
    crate::project_lines::window(ctx, r);
    crate::working_tree::window(ctx, r);
    crate::thermal::window(ctx, &mut r.thermal, &r.model, r.clearances.rules.as_ref());
    r.bom.show(ctx, &r.model);
    r.spot.show(ctx, &r.model);
    pylon_windows(ctx, r);
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

/// The Terrain menu: the ground surface (TIN) and the LiDAR points, with
/// the classes drawn.
fn terrain_menu(ui: &mut egui::Ui, r: &mut Ready, out: &mut Vec<crate::menu::Command>) {
    ui.label(egui::RichText::new("Routes and alignments").small().weak());
    for kind in [
        crate::tables::TableKind::Routes,
        crate::tables::TableKind::Alignments,
        crate::tables::TableKind::AlignmentPoints,
    ] {
        let c = crate::menu::Command::Table(kind);
        if c.menu_item(ui) {
            out.push(c);
        }
    }
    ui.separator();
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
    ui.separator();
    crate::menu::options_menu(ui, "Backgrounds", |ui| crate::backgrounds::menu(ui, r));
    ui.menu_button("Coordinate system", |ui| {
        ui.label(
            egui::RichText::new(format!("Now: {}", ohl_crs::describe(&r.model.line.crs)))
                .small()
                .weak(),
        );
        ui.label(
            egui::RichText::new(format!(
                "Height datum: {}",
                crate::crs::describe_height_datum(&r.model)
            ))
            .small()
            .weak(),
        );
        if ui
            .button("Define…")
            .on_hover_text(
                "Which coordinate system the project's coordinates are in, and the vertical \
                 datum of its heights (nothing moves)",
            )
            .clicked()
        {
            r.crs.define = Some(crate::crs::Define::new(&r.model));
            ui.close();
        }
        if ui
            .button("Coordinate system calculator…")
            .on_hover_text("Converts points from one coordinate system to another")
            .clicked()
        {
            r.crs.calculator = Some(crate::crs::Calculator::new(&r.model));
            ui.close();
        }
        if ui
            .add_enabled(r.crs.job.is_none(), egui::Button::new("Reproject project…"))
            .on_hover_text(
                "Moves the whole project (lines, surfaces, point clouds) into another \
                 coordinate system, as a new project file",
            )
            .clicked()
        {
            r.crs.reproject = Some(crate::crs::Reproject::new());
            ui.close();
        }
    });
    ui.separator();
    ui.menu_button("Survey data", |ui| {
        if r.model.points.is_empty() {
            ui.add_enabled(false, egui::Button::new("LiDAR: no point cloud"));
            return;
        }
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
                 (.ohlc)",
            )
            .clicked()
        {
            r.class_window.get_or_insert_with(Default::default);
            ui.close();
        }
    });
}

/// The views whose controls Help › Controls shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlsPage {
    /// The 3D view.
    View3d,
    /// The plan.
    Plan,
    /// The profile.
    Profile,
    /// A cross-section view.
    CrossSection,
}

impl ControlsPage {
    /// Every page, in the menu's order.
    pub const ALL: [ControlsPage; 4] = [
        ControlsPage::View3d,
        ControlsPage::Plan,
        ControlsPage::Profile,
        ControlsPage::CrossSection,
    ];

    /// Name in the menu.
    pub fn name(self) -> &'static str {
        match self {
            ControlsPage::View3d => "3D",
            ControlsPage::Plan => "Plan",
            ControlsPage::Profile => "Profile",
            ControlsPage::CrossSection => "Cross-section",
        }
    }
}

/// The option "Tools act on left-click" (Tools menu, Help › Controls):
/// kept with the 3D view's controls, saved with the preferences.
fn tools_on_left_checkbox(ui: &mut egui::Ui, c: &mut crate::controls::Controls) {
    if ui
        .checkbox(&mut c.tools_on_left, "Tools act on left-click")
        .on_hover_text(
            "On: a left-click uses the tool held (Select held: it selects) and a \
             right-click selects. Off (default): a left-click selects, a right-click uses \
             the tool. Drags (orbit, look, pan) are not affected.",
        )
        .changed()
    {
        log::info!("controls: tools act on left-click {}", c.tools_on_left);
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

/// Help › Controls › 3D / Plan / Profile: how the view is moved and
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
        .show(ctx, |ui| {
            tools_on_left_checkbox(ui, &mut r.view3d.controls);
            ui.separator();
            let clicks = crate::controls::click_rows(r.view3d.controls.tools_on_left);
            let select_double = if r.view3d.controls.tools_on_left {
                "right double-click"
            } else {
                "double-click"
            };
            match page {
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
                                        // Keep the chosen speed and click option.
                                        let (speed, left) = (c.speed, c.tools_on_left);
                                        *c = p;
                                        c.speed = speed;
                                        c.tools_on_left = left;
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
                    rows.push(row("1–9, wheel", "choose a tool of the hotbar"));
                    rows.push(row("E", "tool inventory"));
                    rows.push(row("C", "cross-section: a cut across the line (a window)"));
                    controls_grid(ui, "controls-3d", &rows);
                }
                ControlsPage::Plan => {
                    let mut rows = vec![
                        row("drag (any button)", "pan"),
                        row("scroll", "zoom at the pointer"),
                    ];
                    rows.extend(clicks);
                    rows.push(row(select_double, "select and centre on it"));
                    rows.push(row("C", "cross-section: a cut across the line (a window)"));
                    controls_grid(ui, "controls-plan", &rows);
                }
                ControlsPage::Profile => {
                    let mut rows = vec![
                        row("drag (any button)", "move along the route"),
                        row("scroll", "zoom along the route at the pointer"),
                    ];
                    rows.extend(clicks);
                    rows.push(row(select_double, "show the whole route"));
                    let select = if r.view3d.controls.tools_on_left {
                        "right"
                    } else {
                        "left"
                    };
                    rows.push((
                        format!("Alt + {select} drag"),
                        "box: the LiDAR points dotted in it, their class changed together".into(),
                    ));
                    rows.push((
                        format!("Alt + Shift + {select} drag"),
                        "lasso: the same in a free shape (Esc: give up)".into(),
                    ));
                    rows.push(row(
                        "right-click (Select held)",
                        "a menu: Cross-section here (at that station)",
                    ));
                    rows.push(row("C", "cross-section: a cut across the line (a window)"));
                    controls_grid(ui, "controls-profile", &rows);
                }
                ControlsPage::CrossSection => {
                    let mut rows = vec![row(
                        "drags, scroll, keys",
                        "as in the 3D view (its control scheme: the 3D page)",
                    )];
                    rows.extend(clicks);
                    rows.push(row(
                        "Home, Back to the cut view",
                        "the front view: looking ahead, orthographic, 1:1",
                    ));
                    rows.push(row(
                        "Orthographic / Perspective",
                        "the projection (it opens orthographic)",
                    ));
                    rows.push(row(
                        ", (comma)",
                        "move the cut one step back along the line",
                    ));
                    rows.push(row(". (full stop)", "move the cut one step ahead"));
                    rows.push(row("⏮ ⏭", "move the cut to the structure before / after"));
                    rows.push(row("C", "another cut (a window)"));
                    rows.push(row(
                        "click its outline in the 3D view",
                        "its panel to the front",
                    ));
                    controls_grid(ui, "controls-xsection", &rows);
                }
            }
        });
    if !open {
        r.controls_window = None;
    }
}

/// Criteria › Bimetallic conductor model: what the outer strands of the
/// line's bimetallic cables do above the transition temperature, for the
/// cables whose wire library entry does not say; OK makes it one command
/// (every section is solved again).
/// Terrain › Coordinate system: Define (an edit of the line's settings),
/// Calculator, Reproject (a new project, opened when done).
fn crs_windows(ctx: &egui::Context, r: &mut Ready) {
    match r.crs.show(ctx, &r.model) {
        Some(crate::crs::Request::Define { crs, height_datum }) => {
            if let Ok(e) = &r.model.editing {
                let mut line = e.session.line().clone();
                line.crs = crs;
                line.height_datum = height_datum;
                let command = ohl_session::line_settings_change(e.session.line(), &line);
                r.perform(crate::edit::Action::Command {
                    what: "coordinate system".into(),
                    command,
                });
            }
        }
        Some(crate::crs::Request::Open(path)) => r.open_project = Some(path),
        None => {}
    }
}

fn bimetallic_window(ctx: &egui::Context, r: &mut Ready) {
    use ohl_cables::{BimetallicBehavior, DEFAULT_MAX_VIRTUAL_COMPRESSION};
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
                            Some(ohl_core::units::Stress::from_megapascals(mpa));
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
                crate::menu::error_label(ui, format!("read-only: {why}"));
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

/// Criteria › Calculation options: how the line's calculations are done
/// (one command, undoable; the sections it touches are solved again).
/// The measured cost of the jumpers' pull, for the window (release
/// builds): a test line, and an existing 25 km line where nearly every
/// section ends at a jumper.
const PULL_COST: &str = "measured: about 3× the time of the estimate on a test line (31 \
    sections, 21 at 15 jumpers), about 5× on an existing line where nearly every section ends at \
    a jumper (554 sections, 518 jumpers, 99 weather cases: 32 s ➡ 158 s, the sections over the \
    cores); sections away from jumpers are not solved again.";

/// Criteria › Default values: what new objects start with, for the
/// project; exported and imported as a file.
fn defaults_window(ctx: &egui::Context, r: &mut Ready) {
    let Some((mut draft, mut message)) = r.defaults_window.take() else {
        return;
    };
    let (mut ok, mut cancel) = (false, false);
    let mut open = true;
    egui::Window::new("Default values")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(
                "What new objects start with in this project, and what the calculations take \
                 where an object, a cable, a weather or ambient case or a rule file gives no \
                 value of its own. Display settings stay yours (View, Display options).",
            );
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    egui::Grid::new("defaults")
                        .num_columns(3)
                        .striped(true)
                        .show(ui, |ui| {
                            let program = ohl_line::Defaults::default();
                            let mut section = "";
                            for f in ohl_line::DEFAULT_FIELDS {
                                if f.section != section {
                                    section = f.section;
                                    ui.label(egui::RichText::new(section).strong());
                                    ui.end_row();
                                }
                                ui.label(f.title).on_hover_text(f.help);
                                let mut v = (f.get)(&draft);
                                let high = if f.high.is_finite() { f.high } else { f64::MAX };
                                let speed = (v.abs() * 0.01).max(0.001);
                                let mut d = egui::DragValue::new(&mut v)
                                    .speed(speed)
                                    .range(f.low..=high)
                                    .max_decimals(4);
                                if !f.unit.is_empty() {
                                    d = d.suffix(format!(" {}", f.unit));
                                }
                                if ui.add(d).on_hover_text(f.help).changed() {
                                    (f.set)(&mut draft, v);
                                }
                                let p = (f.get)(&program);
                                ui.label(
                                    egui::RichText::new(if v == p {
                                        "the program's".to_owned()
                                    } else {
                                        format!("program: {p}")
                                    })
                                    .small()
                                    .weak(),
                                );
                                ui.end_row();
                            }
                        });
                });
            if ui
                .add_enabled(!draft.is_default(), egui::Button::new("Program's values"))
                .on_hover_text("Back to the values OHL-Design starts with")
                .clicked()
            {
                draft = ohl_line::Defaults::default();
            }
            ui.separator();
            let ext = ohl_line::DEFAULTS_FILE_EXTENSION;
            ui.horizontal(|ui| {
                if ui
                    .button("Export…")
                    .on_hover_text(format!("Writes these values to a .{ext} file"))
                    .clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Default values", &[ext])
                        .set_file_name(format!("defaults.{ext}"))
                        .save_file()
                {
                    message = Some(match std::fs::write(&p, draft.to_text()) {
                        Ok(()) => (format!("exported to {}", p.display()), false),
                        Err(e) => (format!("{}: {e}", p.display()), true),
                    });
                }
                if ui
                    .button("Import…")
                    .on_hover_text(format!(
                        "Reads the values of a .{ext} file (OK keeps them for the project)"
                    ))
                    .clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Default values", &ohl_core::files::open_extensions(ext))
                        .pick_file()
                {
                    message = Some(
                        match std::fs::read_to_string(&p)
                            .map_err(|e| e.to_string())
                            .and_then(|t| ohl_line::Defaults::parse(&t))
                        {
                            Ok(d) => {
                                draft = d;
                                (format!("read {}", p.display()), false)
                            }
                            Err(e) => (format!("{}: {e}", p.display()), true),
                        },
                    );
                }
            });
            if let Some((m, error)) = &message {
                if *error {
                    crate::menu::error_label(ui, m.clone());
                } else {
                    ui.label(egui::RichText::new(m).small().weak());
                }
            }
            if let Err(why) = r.model.can_edit() {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            ui.horizontal(|ui| {
                ok = ui
                    .add_enabled(
                        r.model.can_edit().is_ok()
                            && draft != r.model.line.defaults
                            && draft.validate().is_ok(),
                        egui::Button::new("OK"),
                    )
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if ok {
        let command = r
            .model
            .editing
            .as_ref()
            .map(|e| e.session.defaults_command(draft));
        if let Ok(command) = command {
            r.perform(crate::edit::Action::Command {
                what: "default values".into(),
                command,
            });
        }
    }
    if open && !ok && !cancel {
        r.defaults_window = Some((draft, message));
    }
}

/// A jumper joining two different circuits: the user is told, compares
/// them and chooses the one that stays ([`crate::circuit_join`]).
fn circuit_join_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut join) = r.circuit_join.take() else {
        return;
    };
    match crate::circuit_join::window(ctx, &mut join) {
        crate::circuit_join::Outcome::Open => r.circuit_join = Some(join),
        crate::circuit_join::Outcome::Cancel => {
            r.status = Some((
                format!("Add jumper {}: cancelled, nothing changed", join.jumper.id),
                false,
            ));
        }
        crate::circuit_join::Outcome::Join => match join.command(&r.model) {
            Ok(command) => {
                let action = crate::edit::Action::Command {
                    what: join.what(),
                    command,
                };
                r.jumper_added(join.jumper.id.clone(), action, join.open_editor);
            }
            Err(e) => r.status = Some((e, true)),
        },
    }
}

fn solver_window(ctx: &egui::Context, r: &mut Ready) {
    use ohl_line::JumperLoads;
    let Some(mut draft) = r.solver_window else {
        return;
    };
    let jumpers = r.model.line.jumpers.len();
    let (mut ok, mut cancel) = (false, false);
    let mut open = true;
    egui::Window::new("Calculation options")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.strong("Jumpers on the clamps");
            ui.label(
                "What the sections at a jumper's ends carry at their clamps (clamps hanging \
                 from strings; a wire clamped to the structure leaves it to the structure). \
                 The jumper itself is solved between the clamps either way; the ruling-span \
                 method leaves jumpers out.",
            );
            for (j, text, help) in [
                (
                    JumperLoads::Estimate,
                    "An estimate (default): weight, ice and wind",
                    "Each end carries the weight, ice and wind (across the line, from the \
                     weather case's side) of the jumper up to its first helping string, half of \
                     it without",
                ),
                (
                    JumperLoads::Pull,
                    "The jumper's own pull",
                    "The force the solved jumper exerts on each clamp (its tension there, with \
                     its stiffness; not the moments of clamped ends): the sections and the \
                     jumpers are solved in turn until the clamps no longer move",
                ),
            ] {
                ui.radio_value(&mut draft.jumper_loads, j, text)
                    .on_hover_text(help);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{jumpers} jumper(s) on this line. Computation time: the estimate costs \
                     nothing more; the pull solves the sections at jumpers and the jumpers again \
                     in two or three passes — {}",
                    PULL_COST
                ))
                .small()
                .weak(),
            );
            ui.separator();
            ui.strong("Jumpers on taps");
            ui.label(
                "A jumper starting from a clamp on a span or on another jumper (a tap: a hinge, \
                 forces only): what the wire it hangs on carries.",
            );
            let taps = r.model.line.jumpers.iter().filter(|j| j.on_tap()).count();
            for (choice, text, help) in [
                (
                    ohl_line::TapLoads::Loop,
                    "The full loop (default)",
                    "The jumper's own force at its tap put on the wire (or jumper) it hangs on, \
                     both solved in turn until the tap no longer moves",
                ),
                (
                    ohl_line::TapLoads::Once,
                    "One calculation",
                    "The wire carries the jumper's estimate (weight, ice and wind up to its first \
                     support, half without), then the jumper is solved once: no loop",
                ),
            ] {
                ui.radio_value(&mut draft.taps, choice, text)
                    .on_hover_text(help);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{taps} jumper(s) on taps on this line. The loop solves the wires at taps and \
                     their jumpers again in passes (usually two or three); one calculation costs \
                     nothing more."
                ))
                .small()
                .weak(),
            );
            ui.separator();
            ui.strong("Ice and wind on the wires");
            ui.label(
                "Per what metre of the wire the finite elements carry the ice and the wind \
                 (the own weight is per metre of unstressed wire either way: it is conserved).",
            );
            for (choice, text, help) in [
                (
                    ohl_line::LoadsPer::Stretched,
                    "Per stretched metre (default, exact)",
                    "Ice and wind act on the conductor as it is, longer by its elastic strain: \
                     each element carries them times its stretch L/L0, found by a fixed point \
                     (three solves in practice)",
                ),
                (
                    ohl_line::LoadsPer::Unstretched,
                    "Per unstretched metre (as other programs)",
                    "The former treatment, which other programs appear to use: about T/EA, \
                     0.1 %, less ice and wind, so slightly less sag and swing than the exact \
                     treatment (on the unsafe side for clearances). A simplification, for \
                     comparing with other programs",
                ),
            ] {
                ui.radio_value(&mut draft.loads_per, choice, text)
                    .on_hover_text(help);
            }
            if draft.loads_per == ohl_line::LoadsPer::Unstretched {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    egui::RichText::new(
                        "Per unstretched metre: about 0.1 % less ice and wind than the conductor \
                         carries — slightly less sag and swing (unsafe side). Results say which \
                         was used.",
                    )
                    .small(),
                );
            } else {
                ui.label(
                    egui::RichText::new(
                        "The effect of the choice: about T/EA, 0.1 %, of the ice and wind (iced \
                         and windy cases about +0.03 to +0.07 % on the tension, sags within a few \
                         mm on the example line); still air is the same either way. The closed \
                         forms (ruling span) always take the stretched metre.",
                    )
                    .small()
                    .weak(),
                );
            }
            if let Err(why) = r.model.can_edit() {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            ui.horizontal(|ui| {
                ok = ui
                    .add_enabled(
                        r.model.can_edit().is_ok() && draft != r.model.line.solver,
                        egui::Button::new("OK"),
                    )
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if ok {
        let command = r
            .model
            .editing
            .as_ref()
            .map(|e| e.session.solver_command(draft));
        if let Ok(command) = command {
            r.perform(crate::edit::Action::Command {
                what: "calculation options".into(),
                command,
            });
        }
    }
    r.solver_window = (open && !ok && !cancel).then_some(draft);
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
                crate::menu::error_label(ui, format!("read-only: {why}"));
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

/// Versions › History: the operation log; admins reverse entries.
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
                    ui.colored_label(ui.visuals().warn_fg_color, "changes not applied");
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
/// Structures › Angles around structures: which angle means ahead. Angles
/// in plan around a structure turn clockwise seen from above.
fn angles_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut ahead) = r.angles_window else {
        return;
    };
    let (mut apply, mut close) = (false, false);
    let mut open = true;
    egui::Window::new("Angles around structures")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                "Angles in plan around a structure — the direction of a string, the compass of \
                 the structure window — turn clockwise seen from above. Which angle means ahead \
                 along the line is yours to choose for the project:",
            );
            ui.horizontal(|ui| {
                ui.label("ahead is at");
                ui.add(
                    egui::DragValue::new(&mut ahead)
                        .range(0.0..=359.0)
                        .speed(1.0)
                        .suffix("°"),
                );
                for a in [0.0, 90.0, 180.0, 270.0] {
                    if ui.small_button(format!("{a:.0}°")).clicked() {
                        ahead = a;
                    }
                }
            });
            let at = |q: f64| (ahead + q).rem_euclid(360.0);
            egui::Grid::new("angles-around").show(ui, |ui| {
                for (name, q, axis) in [
                    ("ahead", 0.0, "+y"),
                    ("right", 90.0, "+x"),
                    ("back", 180.0, "−y"),
                    ("left", 270.0, "−x"),
                ] {
                    ui.label(name);
                    ui.label(format!("{:.0}°", at(q)));
                    ui.label(egui::RichText::new(format!("structure axis {axis}")).weak());
                    ui.end_row();
                }
            });
            ui.label(
                egui::RichText::new(
                    "For the whole project (every user sees the same). The structure's axes stay: \
                     x to the right of the line, y ahead, z up.",
                )
                .small()
                .weak(),
            );
            ui.horizontal(|ui| {
                apply = ui.button("Apply").clicked();
                close = ui.button("Cancel").clicked();
            });
        });
    if apply {
        if let Some(stored) = r.model.stored_line() {
            let mut line = stored.clone();
            line.display.ahead_deg = ahead.rem_euclid(360.0);
            let command = ohl_session::line_settings_change(stored, &line);
            if r.perform(crate::edit::Action::Command {
                what: format!("ahead at {:.0}° around structures", line.display.ahead_deg),
                command,
            }) {
                r.angles_window = None;
                return;
            }
        } else {
            r.status = Some(("read-only: open a project to edit".into(), true));
        }
    }
    r.angles_window = (open && !close).then_some(ahead);
}

fn display_window(ctx: &egui::Context, r: &mut Ready) {
    use crate::model::{CasesShown, WindDisplay};
    let Some(mut w) = r.display_window.clone() else {
        return;
    };
    let (mut apply, mut cancel) = (false, false);
    let window = tool_window(r, "display-cases", "Display weather cases");
    let panel = crate::dockable::Panel::DisplayCases;
    let shown = crate::dockable::show(ctx, panel, panel.name(), window, None, |ui| {
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
    cancel |= shown == crate::dockable::Shown::Closed;
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
/// Where a new structure goes when no place is given: the first free
/// alignment point, else by the last structure, else the model's origin.
fn structure_spot(model: &crate::model::Model) -> [f64; 3] {
    crate::editors::alignment_points(&model.line)
        .into_iter()
        .find(|p| p.3.is_none())
        .map(|p| p.2)
        .or_else(|| model.line.structures.last().map(|s| s.position))
        .unwrap_or(model.origin)
}

fn editor_window(ctx: &egui::Context, r: &mut Ready, rs: Option<&eframe::egui_wgpu::RenderState>) {
    use crate::editors::{Form, Outcome};
    if !matches!(r.editor, Some(Form::Structure(_))) {
        r.structure_picker = None;
    }
    let Some(mut form) = r.editor.clone() else {
        return;
    };
    let mut outcome = None;
    let window = tool_window(r, "editor", form.title());
    let title = form.title();
    let shown = crate::dockable::show(
        ctx,
        crate::dockable::Panel::Editor,
        title,
        window,
        None,
        |ui| {
            let mut extra = crate::editors::FormExtra {
                rs,
                preview: &mut r.structure_preview,
                picker: &mut r.structure_picker,
                picker_preview: &mut r.picker_preview,
            };
            outcome = form.ui(ui, &r.model, r.case, &mut extra);
            if let Some(mates) = &r.circuit_warned {
                crate::menu::message(
                    ui,
                    egui::RichText::new(circuit_warning(mates)).small(),
                    Some(ui.visuals().warn_fg_color),
                );
            }
            if let Some((text, true)) = &r.status {
                crate::menu::error_label(ui, egui::RichText::new(text).small());
            }
        },
    );
    // Its tab closed (docked): as Cancel.
    if shown == crate::dockable::Shown::Closed {
        outcome = Some(Outcome::Cancel);
    }
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
    if let Some(o) = &outcome {
        log::info!("window \"{}\": {o:?}", form.title());
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
                        set: f.wires.iter().find_map(|w| w.set).unwrap_or(f.set),
                    }),
                    // The point where it is now.
                    Form::Points(f) => {
                        if let Some(p) = f.drawn_after(&r.model) {
                            r.select(Selection::LidarPoint(p));
                        }
                    }
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
    // A section whose circuit name, phase or system change takes the rest
    // of its circuit along: shown first, applied on a second Apply.
    let mates = match (&outcome, &form) {
        (Some(Outcome::Apply), Form::Section(f)) => r
            .model
            .stored_line()
            .map(|l| f.circuit_mates(l, &f.id))
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    if !mates.is_empty() && r.circuit_warned.as_ref() != Some(&mates) {
        r.circuit_warned = Some(mates);
        r.editor = Some(form);
        return;
    }
    if outcome.is_some() {
        r.circuit_warned = None;
    }
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
        Some(Outcome::NewPylon) => {
            r.pylon_builder = Some(crate::pylon_builder::PylonBuilder::new(&r.model));
            r.editor = Some(form);
        }
        Some(Outcome::Cancel) => {
            r.editor = None;
            r.status = None;
            // The section window of a wire being strung: that wire is
            // given up (String wire stays held).
            if matches!(form, Form::Section(_)) && r.hotbar.tool() == crate::tools::Tool::StringWire
            {
                r.cancel_string_wire();
            }
        }
        None => r.editor = Some(form),
    }
}

/// What a section's change also sets on its circuit's other sections.
fn circuit_warning(mates: &[crate::editors::CircuitMate]) -> String {
    let list: Vec<String> = mates
        .iter()
        .map(|(id, what)| format!("{id} ({})", what.join(", ")))
        .collect();
    format!(
        "Also changed, the same circuit: {}. Apply again to change them all.",
        list.join(", ")
    )
}

/// The Delete tool's window for an alignment point on several alignments:
/// off all of them (the point deleted) or off the ones ticked; Escape
/// closes it.
fn point_delete_window(ctx: &egui::Context, r: &mut Ready) {
    let Some((id, mut ticks)) = r.point_delete.clone() else {
        return;
    };
    // None: still open; Some(None): delete it; Some(Some(list)): off these.
    let mut chosen: Option<Option<Vec<String>>> = None;
    let mut close = false;
    let stored = r.model.stored_line().unwrap_or(&r.model.line).clone();
    tool_window(r, "point-delete", format!("Delete alignment point {id}")).show(ctx, |ui| {
        ui.label(format!(
            "{id} is on {} alignments. Delete it from all of them, or take it off the ones ticked \
             (it stays on the others; the points around keep their order).",
            ticks.len()
        ));
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .show(ui, |ui| {
                for (a, on) in &mut ticks {
                    let route = stored
                        .alignment_by_id(a)
                        .map_or(String::new(), |x| x.route.clone());
                    ui.checkbox(on, format!("{a} (route {route})"));
                }
            });
        ui.horizontal(|ui| {
            if ui.small_button("tick all").clicked() {
                ticks.iter_mut().for_each(|t| t.1 = true);
            }
            if ui.small_button("tick none").clicked() {
                ticks.iter_mut().for_each(|t| t.1 = false);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button("Delete from all")
                .on_hover_text("The point is deleted: off every alignment")
                .clicked()
            {
                chosen = Some(None);
            }
            let ticked: Vec<String> = ticks.iter().filter(|t| t.1).map(|t| t.0.clone()).collect();
            if ui
                .add_enabled(
                    !ticked.is_empty(),
                    egui::Button::new("Take it off the ticked ones"),
                )
                .on_hover_text("Off these alignments only (all ticked: the point is deleted)")
                .clicked()
            {
                chosen = Some((ticked.len() < ticks.len()).then_some(ticked));
            }
            close |= ui.button("Cancel").clicked();
        });
    });
    close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    match chosen {
        Some(None) => {
            r.point_delete = None;
            r.perform(crate::edit::Action::Delete {
                kind: ohl_session::Kind::AlignmentPoint,
                id,
                confirm: false,
            });
        }
        Some(Some(off)) => {
            r.point_delete = None;
            let mut alignments = stored.alignments.clone();
            for a in alignments.iter_mut().filter(|a| off.contains(&a.id)) {
                a.points.retain(|p| *p != id);
            }
            match crate::editors::alignments_command(&stored, &alignments) {
                Ok(command) => {
                    r.perform(crate::edit::Action::Command {
                        what: format!("took alignment point {id} off {}", off.join(", ")),
                        command,
                    });
                }
                Err(e) => r.status = Some((e, true)),
            }
        }
        None if close => r.point_delete = None,
        None => r.point_delete = Some((id, ticks)),
    }
}

/// The window of a deletion touching spreads: a spread deleted alone or
/// with its points (those bare and used by nothing else), or a point a
/// spread ends on deleted with its spreads (a spread needs both its
/// points); what goes listed, Escape (Cancel) closes it.
fn spread_delete_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(ask) = r.spread_delete.clone() else {
        return;
    };
    let line = r.model.stored_line().unwrap_or(&r.model.line).clone();
    let Some(sec) = line.sections.iter().find(|s| s.id == ask.section) else {
        r.spread_delete = None;
        return;
    };
    let section = sec.id.clone();
    // None: still open; Some(None): cancelled; Some(Some(..)): the points
    // and spreads to delete.
    let mut chosen: Option<Option<(Vec<String>, Vec<usize>)>> = None;
    let spread_text = |k: usize| {
        sec.spreads.get(k).map_or_else(String::new, |l| {
            format!(
                "{} from {} to {} ({} N/m{})",
                crate::properties::spread_name(sec, k),
                l.from,
                l.to,
                l.load,
                if l.per_subconductor {
                    " on each subconductor"
                } else {
                    ""
                }
            )
        })
    };
    match (&ask.spread, &ask.point) {
        (Some(k), _) => {
            let Some(l) = sec.spreads.get(*k) else {
                r.spread_delete = None;
                return;
            };
            let ends: Vec<(String, Vec<String>)> = [&l.from, &l.to]
                .into_iter()
                .map(|p| (p.clone(), point_uses(&line, sec, p, *k)))
                .collect();
            let free: Vec<String> = ends
                .iter()
                .filter(|(_, uses)| uses.is_empty())
                .map(|(p, _)| p.clone())
                .collect();
            let title = format!("Delete spread {}?", crate::properties::spread_name(sec, *k));
            tool_window(r, "spread-delete", title).show(ctx, |ui| {
                ui.label(format!("Spread {} of section {section}.", spread_text(*k)));
                ui.label("Its points:");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for (p, uses) in &ends {
                            let text = if uses.is_empty() {
                                format!("  {p}: bare, nothing else uses it")
                            } else {
                                format!("  {p}: stays — {}", uses.join("; "))
                            };
                            ui.label(egui::RichText::new(text).small());
                        }
                    });
                ui.label("It is one log entry: undo brings everything back.");
                ui.horizontal(|ui| {
                    if ui
                        .button("Spread only")
                        .on_hover_text("Its points stay (bare points on the span)")
                        .clicked()
                    {
                        chosen = Some(Some((Vec::new(), vec![*k])));
                    }
                    if ui
                        .add_enabled(!free.is_empty(), egui::Button::new("Spread and its points"))
                        .on_hover_text(format!(
                            "Its points deleted too: {} (a point that holds an object or is \
                             used by something else stays)",
                            free.join(", ")
                        ))
                        .on_disabled_hover_text(
                            "Both its points hold an object or are used by something else",
                        )
                        .clicked()
                    {
                        chosen = Some(Some((free.clone(), vec![*k])));
                    }
                    if ui.button("Cancel").clicked() {
                        chosen = Some(None);
                    }
                });
            });
        }
        (None, Some(p)) => {
            let spreads = sec.spreads_at(p);
            let spacers: Vec<String> = line
                .spacers
                .iter()
                .filter(|x| {
                    [&x.a, &x.b]
                        .iter()
                        .any(|e| e.section == section && &e.point == p)
                })
                .map(|x| x.id.clone())
                .collect();
            tool_window(r, "spread-delete", format!("Delete span point {p}?")).show(ctx, |ui| {
                ui.label(format!(
                    "Span point {p} of section {section} ends {} spread(s): a spread needs both \
                     its points. Delete the point and the whole spread(s)?",
                    spreads.len()
                ));
                ui.label("This also deletes:");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for k in &spreads {
                            ui.label(
                                egui::RichText::new(format!("  spread {}", spread_text(*k)))
                                    .small(),
                            );
                        }
                        for x in &spacers {
                            ui.label(egui::RichText::new(format!("  spacer {x}")).small());
                        }
                    });
                ui.label(
                    "Their other points stay. It is one log entry: undo brings everything back.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Delete the point and its spreads").clicked() {
                        chosen = Some(Some((vec![p.clone()], spreads.clone())));
                    }
                    if ui.button("Cancel").clicked() {
                        chosen = Some(None);
                    }
                });
            });
        }
        (None, None) => {
            r.spread_delete = None;
            return;
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        chosen = Some(None);
    }
    match chosen {
        Some(Some((points, spreads))) => {
            r.spread_delete = None;
            if let Some(action) = span_delete_action(&line, &section, &points, &spreads) {
                r.perform(action);
            }
        }
        Some(None) => r.spread_delete = None,
        None => {}
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
/// (with "check after edits" on) or no check ran yet. Its violations are
/// drawn while the Clearances tab is open (docked or moved too), taken off
/// the views once it is closed, back when it is opened again (as the
/// vegetation panel's layers).
fn clearance_frame(ctx: &egui::Context, r: &mut Ready, tab_open: bool) {
    let arrived = r.clearances.poll();
    let shown = tab_open && r.clearances.report.is_some();
    if (arrived || r.clearances_drawn != Some(shown))
        && (shown || r.marks_from == MarksFrom::Clearances)
    {
        r.marks_from = MarksFrom::Clearances;
        let f = if shown { r.clearances.findings() } else { &[] };
        r.view3d.marks = f.iter().map(|f| (f.at, f.target)).collect();
        r.plan.marks = f.iter().map(|f| f.at).collect();
        r.view3d.dirty.insert(Layer::Clearances);
    }
    r.clearances_drawn = Some(shown);
    // Checks run on demand (Run, Checks menu), or after every edit
    // when asked.
    if r.clearances.stale && r.clearances.auto && r.clearances.report.is_some() {
        start_clearance_check(ctx, r);
    }
}

/// What every clearance shown is measured to.
pub(crate) const SURFACE_NOTE: &str =
    "Clearances to the conductor's surface (iced in a case with ice).";

/// What a vertical or radial clearance window asks: its violations drawn
/// (layer "Clearance violations"), a span gone to.
impl Ready {
    /// What the Distance between conductors page asks: a report saved, the
    /// result's weather case shown, its span gone to, its line drawn.
    fn phase_distance_out(&mut self, out: crate::phase_distance::Out) {
        if let Some((what, command)) = out.apply {
            self.perform(crate::edit::Action::Command { what, command });
        }
        if let Some(name) = &out.case
            && let Some(k) = self.model.cases.iter().position(|c| &c.case.name == name)
            && k != self.case
        {
            self.set_case(k);
        }
        if let Some(target) = out.go {
            self.go_to(target);
        }
        if let Some(m) = out.measure {
            // The view on the line's middle, close enough to read.
            let mid = [0, 1, 2].map(|i| (m.0[i] + m.1[i]) / 2.0);
            if let Some(cam) = self.view3d.camera.as_mut() {
                cam.target = mid;
                cam.distance = cam.distance.min(60.0);
            }
            self.plan.centre_on(mid);
            self.view3d.measure = Some(m);
        }
    }

    /// What the Performed checks page asks: a report opened in its check's
    /// page, a change applied.
    pub(crate) fn check_library_out(&mut self, out: crate::check_library::Out) {
        if let Some(r) = out.open {
            match r.check.as_str() {
                crate::phase_distance::CHECK_ID => {
                    if let Err(e) = self.phase_distance.load(&r, &self.model) {
                        self.model.log.push(e);
                    }
                }
                crate::vegetation::CHECK_ID => {
                    if let Err(e) = self.vegetation.load_saved(&r, &self.model) {
                        self.status = Some((e, true));
                    }
                }
                crate::safety_volume::CHECK_ID => {
                    if let Err(e) = self.safety_volume.load_saved(&r, &self.model) {
                        self.status = Some((e, true));
                    }
                }
                other => self
                    .model
                    .log
                    .push(format!("no page for the check \"{other}\" here")),
            }
        }
        if let Some((what, command)) = out.apply {
            self.perform(crate::edit::Action::Command { what, command });
        }
    }
}

pub(crate) fn clearance_zone_out(
    r: &mut Ready,
    measure: ohl_clearance::Measure,
    out: crate::clearance_zones::Out,
) {
    // A preset's weather cases: the model reads its weather cases again.
    if let Some(p) = out.weather_saved {
        r.library_saved(
            crate::library::LibraryKind::WeatherCases,
            crate::library::PageAction::Saved(p),
        );
    }
    if let Some(p) = out.weather_use {
        r.library_saved(
            crate::library::LibraryKind::WeatherCases,
            crate::library::PageAction::UseForModel(p),
        );
    }
    if let Some((plugin, p)) = out.plugin_preset
        && let Some(wc) = &p.weather
        && let Err(e) = crate::rule_editor::preset_cases(r, &plugin, &p.name, &p.weather_text, wc)
    {
        r.status = Some((
            format!("preset {plugin} {}: weather cases: {e}", p.name),
            true,
        ));
    }
    if let Some(anchor) = out.guide {
        r.status = Some(
            match crate::help::open_section(crate::clearance_zones::GUIDE, anchor) {
                Ok(m) => (format!("user guide opened: {m}"), false),
                Err(e) => (e, true),
            },
        );
    }
    if out.hide_marks && r.marks_from == MarksFrom::Zone(measure) {
        // Its window closed: its violations off the views.
        r.plan.marks.clear();
        r.view3d.marks.clear();
        r.view3d.dirty.insert(Layer::Clearances);
    }
    if let Some(marks) = out.marks {
        r.marks_from = MarksFrom::Zone(measure);
        r.plan.marks = marks.iter().map(|m| m.0).collect();
        r.view3d.marks = marks;
        r.view3d.dirty.insert(Layer::Clearances);
    }
    if let Some((target, at)) = out.go {
        r.go_to(target);
        if let Some(cam) = r.view3d.camera.as_mut() {
            cam.target = at;
            cam.distance = cam.distance.min(80.0);
        }
        r.plan.centre_on(at);
    }
}

fn start_clearance_check(ctx: &egui::Context, r: &mut Ready) {
    r.clearances.set_defaults(&r.model.line.defaults);
    // The check needs its weather case solved, and its ground.
    r.model.ensure_solved(r.case);
    r.model.ensure_check_ground();
    let m = &r.model;
    let Some(solved) = m.cases.get(r.case) else {
        return;
    };
    let scope = m.scope();
    r.clearances.start(ctx, |rules| crate::clearance::Input {
        line: m.line.clone(),
        case: solved.case.name.clone(),
        // The route worked on only (View › Work on route).
        shapes: match &scope {
            Some(on) => solved
                .shapes
                .iter()
                .filter(|s| on.sections.contains(&s.section))
                .cloned()
                .collect(),
            None => solved.shapes.clone(),
        },
        jumpers: match &scope {
            Some(on) => solved
                .jumpers
                .iter()
                .filter(|j| on.jumpers.contains(&j.jumper))
                .cloned()
                .collect(),
            None => solved.jumpers.clone(),
        },
        rules,
        scheme: m.scheme.clone(),
        cloud: m.cloud.clone(),
        ground: m.ground_for_checks(),
        corridor: m.sources.corridor,
    });
}

/// The three ways to check clearances, in words (the Checks menu and the
/// Clearances page).
pub const CLEARANCES_ABOUT: &str = "To the ground and LiDAR points: rules typed in the \
     windows below. Between the wires of this line: the rules of a rule file, run by Check \
     wire clearances.";

/// Checks › Wire clearances › Add a rule file: a clearance rule file
/// (.ohlr) chosen on the computer, kept in the project (next to a line
/// file) and chosen on the Clearances page.
fn add_rule_file(r: &mut Ready) {
    let Some(from) = rfd::FileDialog::new()
        .add_filter(
            "Clearance rules",
            &ohl_core::files::open_extensions(ohl_clearance::rules::RULE_FILE_EXTENSION),
        )
        .pick_file()
    else {
        return;
    };
    r.status = Some(match keep_rule_file(&mut r.model, &from) {
        Ok(to) => {
            r.clearances
                .refresh(&r.model, r.model.sources.rules.as_deref());
            if let Some(k) = r
                .clearances
                .files
                .iter()
                .position(|p| p.file_name() == to.file_name())
            {
                r.clearances.choose(k);
            }
            r.open_page = Some((Tab::Clearances, None));
            (
                format!("rule file {}", crate::menu::file_label(&r.model, &to)),
                false,
            )
        }
        Err(e) => (e, true),
    });
}

/// Copies rule file `from` among the project's rules (its working copy,
/// kept in the project), else next to the line file; checked first.
fn keep_rule_file(
    model: &mut crate::model::Model,
    from: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    ohl_clearance::RuleSet::load(from)?;
    let name = from.file_name().ok_or("no file name")?;
    let dir = match (&model.library_dir, &model.editing) {
        (Some(w), Ok(_)) => w.join("rules"),
        _ => ohl_core::files::dir_of(&model.sources.file),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let to = dir.join(name);
    std::fs::copy(from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    model.store_libraries()?;
    Ok(to)
}

/// The Clearances tab: rule file, the violations of the shown case (click
/// one to go there).
fn clearance_panel(ui: &mut egui::Ui, r: &mut Ready) {
    // Run in the footer at the bottom left, always visible (owner,
    // 2026-10-21); the rest scrolls above it.
    let mut check =
        crate::check_help::body(ui, "clearances-page", |ui| clearance_panel_body(ui, r));
    if !r.clearances.files.is_empty() {
        crate::check_help::footer(ui, "clearances-page", |ui| {
            let c = &mut r.clearances;
            check |= crate::check_help::run_button(
                ui,
                !c.running(),
                "Check wire clearances: the rules of this file, the weather case shown",
                None,
            );
            ui.checkbox(&mut c.auto, "after every edit");
            if c.running() {
                ui.spinner();
            }
        });
    }
    if check {
        r.clearances.stale = true;
        start_clearance_check(ui.ctx(), r);
    }
}

/// The Clearances tab above its footer: true to run the check (Run again).
fn clearance_panel_body(ui: &mut egui::Ui, r: &mut Ready) -> bool {
    r.clearances.set_defaults(&r.model.line.defaults);
    if r.clearances.files.is_empty() {
        ui.label(egui::RichText::new("No clearance rule file in the project yet.").strong());
        ui.label(
            "The clearances between the wires of this line and to exclusion zones are \
             checked by the rules of a clearance rule file (.ohlr): add one, then Check wire \
             clearances (Checks menu or the quick bar). Other lines: `ohl line --other-line`.",
        );
        if ui
            .button("Add a rule file…")
            .on_hover_text(
                "A clearance rule file (.ohlr) kept in the project; examples/starter/rules has \
                 one with invented values to learn with",
            )
            .clicked()
        {
            add_rule_file(r);
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "To the ground and the LiDAR points: the vertical and radial clearance windows \
                 (rules typed there, starting with the ground's).",
            )
            .small()
            .weak(),
        );
        ui.horizontal(|ui| {
            if ui.button("Vertical clearances…").clicked() {
                r.vertical_clearances.show(&r.model);
            }
            if ui.button("Radial clearances…").clicked() {
                r.radial_clearances.show(&r.model);
            }
        });
        return false;
    }
    let c = &mut r.clearances;
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
    match &c.rules {
        Some(Err(e)) => {
            crate::menu::error_label(ui, e);
        }
        Some(Ok(rules)) => {
            ui.label(egui::RichText::new(format!("{} rules", rules.len())).small());
        }
        None => {}
    }
    crate::rule_editor::ui(ui, r);
    let c = &mut r.clearances;
    if c.stale
        && !c.running()
        && c.report.is_some()
        && crate::revision::banner(ui, "the model or the case shown", true)
    {
        check = true;
    }
    let mut go: Option<crate::clearance::Finding> = None;
    match &c.report {
        Some(Err(e)) => {
            crate::menu::error_label(ui, e);
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
                ui.colored_label(crate::menu::ok_color(ui), head);
            } else {
                ui.colored_label(ui.visuals().error_fg_color, head);
            }
            for (rule, text) in &rep.rules {
                ui.label(egui::RichText::new(format!("{rule}: {text}")).small());
            }
            for note in &rep.notes {
                ui.label(egui::RichText::new(note).small().weak());
            }
            ui.label(egui::RichText::new(SURFACE_NOTE).small().weak());
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
    if let Some(f) = go {
        r.selection = f.wire;
        r.view3d.dirty.insert(Layer::Selection);
        if let Some(cam) = r.view3d.camera.as_mut() {
            cam.target = f.at;
            cam.distance = cam.distance.min(80.0);
        }
        r.plan.centre_on(f.at);
    }
    check
}

/// Shows `tab`: brings it to the front if it is open; library pages open
/// together in a floating window, tables in another, the structure editor
/// in its own (each can be docked), the other pages next to the first one.
pub fn open_page(dock: &mut DockState<Tab>, tab: Tab) {
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
        return;
    }
    // A cut: a tab beside the profile (or of the first group).
    if let Tab::CrossSection(_) = tab {
        let beside = dock
            .find_tab_from(|t| matches!(t, Tab::CrossSection(_)))
            .or_else(|| dock.find_tab(&Tab::Profile));
        match beside {
            Some(path) => {
                dock.set_focused_node_and_surface(path.node_path());
                dock.push_to_focused_leaf(tab);
            }
            None => dock.push_to_first_leaf(tab),
        }
        return;
    }
    let group = |t: &Tab| match t {
        Tab::Library(_) => 1,
        Tab::Table(_) => 2,
        Tab::Python => 3,
        Tab::StructureEditor => 4,
        Tab::PhaseDistance | Tab::CheckLibrary => 5,
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
                w.set_size(match group(&tab) {
                    2 => egui::vec2(1250.0, 560.0),
                    4 => egui::vec2(1150.0, 760.0),
                    5 => egui::vec2(1050.0, 780.0),
                    _ => egui::vec2(900.0, 620.0),
                });
            }
        }
        return;
    }
    dock.push_to_first_leaf(tab);
}

/// The tab group of the pages on the left (Model…) or on the right
/// (Properties), if there is one.
fn page_group(dock: &DockState<Tab>, place: Place) -> Option<egui_dock::NodePath> {
    let wanted: &[Tab] = match place {
        Place::Left => &[Tab::Model, Tab::Clearances, Tab::NextSteps],
        Place::Right => &[Tab::Properties],
        Place::Window => &[],
    };
    wanted
        .iter()
        .find_map(|t| dock.find_tab(t))
        .map(|p| p.node_path())
}

/// Moves page `tab` to `place` (its tab's context menu).
pub fn move_page(dock: &mut DockState<Tab>, tab: Tab, place: Place) {
    let Some(src) = dock.find_tab(&tab) else {
        return;
    };
    match place {
        Place::Window => {
            let rect =
                egui::Rect::from_min_size(egui::pos2(160.0, 120.0), egui::vec2(820.0, 600.0));
            dock.detach_tab(src, rect);
        }
        _ => {
            let Some(dst) = page_group(dock, place) else {
                return;
            };
            if dst != src.node_path() {
                dock.move_tab(src, (dst, egui_dock::TabInsert::Append));
            }
        }
    }
    if let Some(path) = dock.find_tab(&tab) {
        let _ = dock.set_active_tab(path);
    }
}

/// The docked tool panels follow their tools ([`crate::dockable`]): a
/// panel asked for is docked (on the right, with Properties) or floats
/// again; one whose tool closed goes with it.
fn sync_docked(ctx: &egui::Context, dock: &mut DockState<Tab>) {
    use crate::dockable::Panel;
    let (alive, docked) = crate::dockable::host(ctx, |h| {
        for (p, d) in std::mem::take(&mut h.requests) {
            if d {
                h.docked.insert(p);
            } else {
                h.docked.remove(&p);
            }
        }
        h.closed.retain(|p| h.alive.contains_key(p));
        (h.alive.clone(), h.docked.clone())
    });
    for p in Panel::ALL {
        let tab = Tab::Tool(p);
        let want = alive.contains_key(&p) && docked.contains(&p);
        match (dock.find_tab(&tab), want) {
            (Some(path), false) => {
                dock.remove_tab(path);
            }
            (None, true) => {
                match page_group(dock, Place::Right).or_else(|| page_group(dock, Place::Left)) {
                    Some(node) => {
                        dock.set_focused_node_and_surface(node);
                        dock.push_to_focused_leaf(tab);
                    }
                    None => dock.push_to_first_leaf(tab),
                }
                ctx.request_repaint();
            }
            _ => {}
        }
    }
}

/// Closes `tab` if it is open.
fn close_page(dock: &mut DockState<Tab>, tab: Tab) {
    if let Some(path) = dock.find_tab(&tab) {
        dock.remove_tab(path);
    }
}

/// The open file's entries of the File menu: Save, every import and
/// export, the bill of materials.
fn file_entries(ui: &mut egui::Ui, r: &mut Ready, out: &mut Vec<crate::menu::Command>) {
    use crate::menu::Command;
    if Command::Save.menu_item(ui) {
        out.push(Command::Save);
        ui.close();
    }
    let busy = r.survey_job.is_some();
    ui.menu_button("Import", |ui| {
        if ui
            .button("Into the line…")
            .on_hover_text(
                "Read a file into the line you work on (built-in formats and those of the \
                 plugins); one command, undoable",
            )
            .clicked()
        {
            r.plugin_request = Some(PluginRequest::Import);
            ui.close();
        }
        if ui
            .add_enabled(!busy, egui::Button::new("LiDAR points (LAS/LAZ)…"))
            .on_hover_text("Add LiDAR tiles to the project's point cloud (or a new one)")
            .clicked()
        {
            r.survey_import = Some(crate::survey_io::ImportDialog::new(&r.model));
            ui.close();
        }
        if ui
            .button("Terrain surface (TIN)…")
            .on_hover_text(
                "Terrain › Terrain surfaces, at its import: a surface file (built-in formats \
                 and those of the plugins) kept in the project",
            )
            .clicked()
        {
            r.surfaces.show(&r.model);
            ui.close();
        }
    });
    ui.menu_button("Export", |ui| {
        if ui
            .button("The line…")
            .on_hover_text(
                "Write the line and its solved wires in a format (built-in or from a plugin)",
            )
            .clicked()
        {
            r.plugin_request = Some(PluginRequest::Export);
            ui.close();
        }
        if ui
            .button("Drawings…")
            .on_hover_text(
                "Plan and profile sheets and profile exports of the routes: SVG, DXF, PDF, PNG, \
                 CSV, XML, at chosen scales and station range",
            )
            .clicked()
        {
            let (rules, markers) = drawing_checks(r);
            r.drawings.set_checks(rules, markers);
            r.drawings.open();
            ui.close();
        }
        if ui
            .add_enabled(
                !busy && r.model.cloud.is_some(),
                egui::Button::new("LiDAR points (LAS/LAZ)…"),
            )
            .on_hover_text("Write the project's point cloud (or part of it) to a LAS/LAZ file")
            .clicked()
        {
            r.survey_export = Some(Default::default());
            ui.close();
        }
        if ui
            .button("Terrain surfaces (TIN)…")
            .on_hover_text("Terrain › Terrain surfaces: export a surface kept in the project")
            .clicked()
        {
            r.surfaces.show(&r.model);
            ui.close();
        }
        if ui
            .add_enabled(
                r.model.library_dir.is_some(),
                egui::Button::new("Libraries…"),
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
    });
    ui.menu_button("Bill of materials", |ui| {
        if ui
            .button("Wires…")
            .on_hover_text(
                "The wire lengths of each wire type (and their stringing), priced with a cost file",
            )
            .clicked()
        {
            r.bom.open(&r.model, ui.ctx());
            ui.close();
        }
        ui.label(
            egui::RichText::new("Structures, insulator assemblies, hardware: not done yet")
                .small()
                .weak(),
        );
    });
}

/// The menus after File, in the order of the work: Edit (with Snap),
/// View (pages, cases shown, layers), Terrain, Structures, Sections,
/// Criteria, Checks, Tools, Versions, Plugins, Help.
/// Commands go to `out`.
fn menus(
    ui: &mut egui::Ui,
    r: &mut Ready,
    dock: &mut DockState<Tab>,
    out: &mut Vec<crate::menu::Command>,
    customize: &mut bool,
    about: &mut bool,
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
        ui.separator();
        crate::menu::options_menu(ui, "Snap: what can be clicked", |ui| {
            snap_panel(ui, &mut r.snaps)
        });
    });
    crate::menu::options_menu(ui, "View", |ui| {
        item(ui, Command::FitViews, out);
        ui.separator();
        ui.label(egui::RichText::new("Pages").small().weak());
        let mut page = |ui: &mut egui::Ui, tab: Tab| {
            let mut open = dock.find_tab(&tab).is_some();
            if ui.checkbox(&mut open, tab.title()).changed() {
                if open {
                    open_page(dock, tab);
                } else {
                    close_page(dock, tab);
                }
            }
        };
        for tab in Tab::VIEWS.into_iter().filter(|t| !t.advanced()) {
            page(ui, tab);
        }
        crate::menu::options_menu(ui, "Advanced", |ui| {
            for tab in Tab::VIEWS.into_iter().filter(|t| t.advanced()) {
                page(ui, tab);
            }
        });
        ui.separator();
        // Ticks keep it open.
        crate::menu::options_menu(ui, "Work on route", |ui| {
            ui.label(
                egui::RichText::new(
                    "The profile, the checks and the structure loads keep to it; the rest is \
                     drawn faded (yours only)",
                )
                .small()
                .weak(),
            );
            let mut chosen = r.model.work_route.clone();
            ui.radio_value(&mut chosen, None, "All routes");
            for route in &r.model.line.routes {
                ui.radio_value(&mut chosen, Some(route.id.clone()), &route.id);
            }
            if chosen != r.model.work_route {
                r.model.work_route = chosen.clone();
                if let Some(id) = chosen {
                    r.profile.show = Some(crate::profile::Show::Route(id));
                }
                r.clearances.stale = true;
                r.refresh_views();
            }
        });
        item(ui, Command::DisplayCases, out);
        if ui
            .button("Cross-section… (C)")
            .on_hover_text(
                "A cut across the line at a station or a structure: a 3D panel of its own, holding only the slab",
            )
            .clicked()
        {
            r.open_cut_dialog();
            ui.close();
        }
        crate::menu::options_menu(ui, "Profile aspect ratio", |ui| {
            // Remembered with the other display choices when the app saves.
            crate::profile::ratio_menu(ui, &mut r.model.profile_ratio);
        });
        crate::menu::options_menu(ui, "Sheet preview", |ui| r.drawings.preview_menu(ui));
        // Ticks keep it open; this user's, remembered with the project.
        crate::menu::options_menu(ui, "Lines on the ground beside the line", |ui| {
            let routes = crate::ground_lines::routes(&r.model.line);
            let cases: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
            let mut build = false;
            crate::ground_lines::menu(
                ui,
                &mut r.model.ground_lines,
                &routes,
                &cases,
                r.case,
                r.model.sources.corridor,
                r.ground_lines.drawn.as_deref(),
                &mut build,
            );
            // Terrain › Terrain surfaces, at Build from LiDAR.
            if build {
                r.surfaces.show(&r.model);
            }
        });
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
        ui.separator();
        if ui.button("Customize the quick bar…").clicked() {
            *customize = true;
            ui.close();
        }
    });
    crate::menu::options_menu(ui, "Terrain", |ui| terrain_menu(ui, r, out));
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
        item(ui, Command::Table(TableKind::Insulators), out);
        ui.separator();
        crate::menu::options_menu(ui, "Display options", |ui| {
            ui.label(
                egui::RichText::new("Labels in each view (yours only)")
                    .small()
                    .weak(),
            );
            if r.model.section_display.structures_ui(ui) {
                r.refresh_views();
            }
        });
        // Adding structures is the Add structure tool (on an alignment
        // point: also on the other points of that alignment, one command);
        // New pylon from a template is in the Tools menu (Structures).
        ui.separator();
        if ui
            .button("Spot structures…")
            .on_hover_text("The least-cost structures along a route (proposal saved as a new line)")
            .clicked()
        {
            r.spot.open(&r.model);
            ui.close();
        }
        if ui
            .button("Angles around structures…")
            .on_hover_text(
                "Which angle means ahead along the line (string directions, the compass)",
            )
            .clicked()
        {
            r.angles_window = Some(r.model.line.display.ahead_deg);
            ui.close();
        }
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
        item(ui, Command::Table(TableKind::SpanObjects), out);
        item(ui, Command::Table(TableKind::LoadTypes), out);
        item(ui, Command::Table(TableKind::Spacers), out);
        item(ui, Command::Table(TableKind::Systems), out);
        item(ui, Command::Table(TableKind::ConductorPaths), out);
        item(ui, Command::Table(TableKind::Circuits), out);
        ui.separator();
        crate::menu::options_menu(ui, "Display options", |ui| {
            ui.label(
                egui::RichText::new("How the wires are drawn and what is written next to them, in each view (yours only)")
                    .small()
                    .weak(),
            );
            if r.model.section_display.ui(ui) {
                // Colours, line codes and arrows: drawn again.
                r.refresh_views();
            }
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
            .button("Calculation options…")
            .on_hover_text(
                "How the calculations are done for the whole line: what of the jumpers the \
                 sections carry at their clamps",
            )
            .clicked()
        {
            r.solver_window = Some(r.model.line.solver);
            ui.close();
        }
        if ui
            .button("Default values…")
            .on_hover_text(
                "What new objects start with in this project and what the calculations take \
                 where nothing else gives a value (jumpers, insulators, Q, thermal, clearances); \
                 exported and imported as a file",
            )
            .clicked()
        {
            r.defaults_window = Some((r.model.line.defaults, None));
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
        crate::structure_app::loads_menu(ui, r);
    });
    ui.menu_button("Checks", |ui| {
        ui.label(
            egui::RichText::new("Run on demand; the model is not changed")
                .small()
                .weak(),
        );
        if ui
            .button("Performed checks…")
            .on_hover_text(
                "The library of the checks saved on this project line: whether the line changed \
                 since, status, note; open one again, export, import",
            )
            .clicked()
        {
            r.check_library.show();
            ui.close();
        }
        ui.separator();
        ui.label(egui::RichText::new("Structure loads").strong());
        if ui
            .button("Compute structure loads")
            .on_hover_text(
                "The loads on the structures at every attachment, in each load case \
                 (Criteria › Load cases library)",
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
        ui.label(egui::RichText::new(CLEARANCES_ABOUT).small().weak());
        ui.menu_button("Ground clearances", |ui| {
            ui.label(
                egui::RichText::new(
                    "From the wires to the ground and the LiDAR points: the rules are typed \
                     in each window (it starts with the ground's) and kept in the project",
                )
                .small()
                .weak(),
            );
            if ui
                .button("Vertical clearances…")
                .on_hover_text(
                    "Ground and LiDAR points straight under the wires, within a width on \
                     each side; rules by voltage, weather case, distance and target; the wind \
                     stepped; results in the window",
                )
                .clicked()
            {
                r.vertical_clearances.show(&r.model);
                ui.close();
            }
            if ui
                .button("Radial clearances…")
                .on_hover_text(
                    "The direct distance from the wires to the ground and LiDAR points; rules \
                     by voltage, weather case, distance and target; the wind stepped; results \
                     in the window",
                )
                .clicked()
            {
                r.radial_clearances.show(&r.model);
                ui.close();
            }
        });
        // Ticks keep it open.
        crate::menu::options_menu(ui, "Wire clearances", |ui| {
            ui.label(
                egui::RichText::new(
                    "Between the wires of this line and to exclusion zones: the rules of a \
                     rule file (.ohlr), in the weather case shown; results on the Clearances \
                     page (other lines: `ohl line --other-line`)",
                )
                .small()
                .weak(),
            );
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
            if ui
                .button("Add a rule file…")
                .on_hover_text("A clearance rule file (.ohlr) kept in the project")
                .clicked()
            {
                ui.close();
                add_rule_file(r);
            }
            if ui.button("Show results").clicked() {
                open_page(dock, Tab::Clearances);
                ui.close();
            }
        });
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
        ui.label(egui::RichText::new("Analyses").strong());
        if ui
            .button("Galloping ellipses…")
            .on_hover_text(
                "Ellipses of the wires of each span by a galloping parameter file (.ohlg)",
            )
            .clicked()
        {
            r.galloping.show(&r.model, r.case);
            ui.close();
        }
        // The checks of the check plugins in use, in their own submenus
        // (none without them).
        let checks: Vec<(Vec<String>, ohl_io::checks::PluginCheck)> = r
            .model
            .checks
            .iter()
            .map(|c| (c.info.menu.clone(), c.clone()))
            .collect();
        if let Some(id) = plugin_checks_menu(ui, &checks) {
            if id == ohl_io::checks::PHASE_DISTANCE {
                r.phase_distance.show(&r.model);
            }
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
            .button("Safety volumes…")
            .on_hover_text(
                "The space within a distance (by voltage) of the wires in every position of \
                 the weather cases, for work near the line; exported as DXF, IFC, KML",
            )
            .clicked()
        {
            r.safety_volume.show(r.case);
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
        ui.add_enabled(false, egui::Button::new("String swing (to come)"));
    });
    // A tick menu (the option below): ticking it keeps the menu open;
    // choosing a tool closes it.
    crate::menu::options_menu(ui, "Tools", |ui| {
        item(ui, Command::Inventory, out);
        tools_on_left_checkbox(ui, &mut r.view3d.controls);
        // Theme by theme, a line between them (as the inventory groups them).
        let mut theme = "";
        for t in crate::tools::Tool::ALL {
            if t.theme() != theme {
                // The Structures group ends with its command (a window,
                // no tool held).
                if theme == "Structures" {
                    item(ui, Command::NewPylon, out);
                }
                ui.separator();
                theme = t.theme();
            }
            let held = r.hotbar.tool() == t;
            let mut b = egui::Button::selectable(held, (t.image(ui.ctx(), 16.0), t.name()))
                .image_tint_follows_text_color(true);
            if let Some(k) = t.key() {
                b = b.shortcut_text(k);
            }
            let help = crate::controls::click_words(t.help(), r.view3d.controls.tools_on_left);
            if ui.add(b).on_hover_text(help.as_ref()).clicked() {
                out.push(Command::Hold(t));
                ui.close();
            }
        }
    });
    crate::menu::options_menu(ui, "Versions", |ui| {
        crate::project_lines::menu(ui, r);
        ui.separator();
        if ui
            .button("History…")
            .on_hover_text("The operation log: who did what; admins reverse entries")
            .clicked()
        {
            r.history_open = true;
            ui.close();
        }
        if ui
            .button("Revisions and branches…")
            .on_hover_text("Revisions, branches, tags, diff, merge, reviews and edit rights")
            .clicked()
        {
            r.versions.open = true;
            ui.close();
        }
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
                "Scripts driving the project through the ohl module: read the line and results, \
                 edit with session commands (undoable)",
            )
            .clicked()
        {
            r.open_page = Some((Tab::Python, None));
            ui.close();
        }
    });
    ui.menu_button("Help", |ui| {
        crate::help::manual_entries(ui, &mut r.status);
        if ui
            .button(format!("About {}", crate::APP_NAME))
            .on_hover_text("Its version, what it is, its licence")
            .clicked()
        {
            *about = true;
            ui.close();
        }
        ui.separator();
        if ui
            .button("Next steps")
            .on_hover_text(
                "What a line still needs, computed from the project: each step a link to \
                 where it is done",
            )
            .clicked()
        {
            r.open_page = Some((Tab::NextSteps, None));
            ui.close();
        }
        ui.menu_button("Controls", |ui| {
            for page in ControlsPage::ALL {
                if ui
                    .button(page.name())
                    .on_hover_text(format!(
                        "How the {} view is moved and used: the keys and mouse",
                        page.name()
                    ))
                    .clicked()
                {
                    r.controls_window = Some(page);
                    ui.close();
                }
            }
        });
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
            if editable {
                let target = r.selection.clone();
                r.delete_object(&target);
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
            r.xsections.free_mouse(ctx);
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
        Command::NewPylon => {
            r.pylon_builder = Some(crate::pylon_builder::PylonBuilder::new(&r.model));
        }
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
                .color(ui.visuals().error_fg_color),
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
        // Warnings of the solved sections (2026-10-22: a state beyond the
        // stress-strain curves' range; strings beyond their compression),
        // each a link like the failures; a long list scrolls.
        for (k, c) in r.model.cases.iter().enumerate() {
            let notes: Vec<(&str, &str)> = c
                .shapes
                .iter()
                .flat_map(|s| s.flags.iter().map(|f| (s.section.as_str(), f.as_str())))
                .collect();
            if notes.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{}: {} warnings", c.case.name, notes.len()))
                    .color(ui.visuals().warn_fg_color),
            )
            .id_salt(("warnings", k))
            .default_open(notes.len() <= 10)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("warnings-scroll", k))
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for (id, f) in &notes {
                            if ui
                                .link(egui::RichText::new(*id).small())
                                .on_hover_text(*f)
                                .clicked()
                            {
                                clicked = Some((k, (*id).to_owned()));
                            }
                            ui.label(egui::RichText::new(*f).small().weak());
                        }
                    });
            });
        }
        if let Some((k, id)) = clicked {
            r.go_to_failure(k, &id);
        }
        ui.separator();
        ui.label(egui::RichText::new(r.model.sources.file.display().to_string()).small());
        // What was read when the project opened: folded (a long list of
        // files and libraries).
        egui::CollapsingHeader::new(format!("Loaded ({} lines)", r.model.log.len()))
            .id_salt("load-log")
            .default_open(false)
            .show(ui, |ui| {
                for l in &r.model.log {
                    ui.label(egui::RichText::new(l).small());
                }
            });
    });
}

impl eframe::App for App {
    /// The window closes: the project too (everything written into the
    /// project file, every connection to it closed: no -wal and -shm files
    /// left beside it), and the server it hosts stops.
    fn on_exit(&mut self) {
        self.stop_hosting();
        self.close();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("ahead-alone", self.ahead_alone.to_string());
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
        self.palette.save(storage);
        self.recent.save(storage);
        self.plugins.save(storage);
        if let State::Ready(r) = &self.state {
            self.view_prefs
                .set(&r.model.sources.file, r.model.view_prefs());
        }
        self.view_prefs.save(storage);
        storage.set_string("ecw-converter", crate::backgrounds::converter());
        storage.set_string(
            "class-scheme",
            self.default_class_scheme
                .as_ref()
                .map_or(String::new(), |p| p.display().to_string()),
        );
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let _timed = crate::perf::span("app ui");
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
                    ready.catalogue_strings();
                    ready.view3d.controls = self.controls.clone();
                    ready.snaps = self.snaps.clone();
                    ready.hotbar = self.hotbar.clone();
                    // A line without structures yet: what it needs next.
                    if ready.model.line.structures.is_empty() {
                        open_page(&mut self.dock, Tab::NextSteps);
                    }
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
                    ui.menu_button("Help", |ui| {
                        crate::help::manual_entries(ui, &mut self.help_status);
                        if ui.button(format!("About {}", crate::APP_NAME)).clicked() {
                            self.about = true;
                            ui.close();
                        }
                    });
                    if let Some((text, error)) = &self.help_status {
                        let t = egui::RichText::new(text).small();
                        if *error {
                            crate::menu::error_label(ui, t);
                        } else {
                            ui.label(t.weak());
                        }
                    }
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
                    crate::menu::error_label(ui, e.as_str());
                });
            }
            State::Structure(desk) => {
                desk.ui(ui, self.render_state.as_ref(), &mut self.ahead_alone)
            }
            State::Ready(ready) => {
                let hooks = crate::perf::span("frame hooks");
                ready.remote_frame();
                {
                    let _t = crate::perf::span("hooks: solving");
                    ready.solving_frame(ui.ctx());
                }
                ready.structure_apps_frame(ui.ctx());
                crate::structure_app::report_window(ui.ctx(), &mut ready.structure_apps);
                history_window(ui.ctx(), ready);
                crate::versions::window(ui.ctx(), ready);
                goto_key(ui.ctx(), ready);
                cut_key(ui.ctx(), ready);
                edit_keys(ui.ctx(), ready);
                {
                    let _t = crate::perf::span("hooks: clearances");
                    let tab_open = self.dock.find_tab(&Tab::Clearances).is_some();
                    clearance_frame(ui.ctx(), ready, tab_open);
                }
                {
                    let _t = crate::perf::span("hooks: popups");
                    popups(ui.ctx(), ready);
                }
                crate::dockable::begin_frame(ui.ctx());
                {
                    let _t = crate::perf::span("hooks: tool windows");
                    tool_windows(ui.ctx(), ready, self.render_state.as_ref());
                }
                drop(hooks);
                let bars = crate::perf::span("status and menus");
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
                            // Tool hints are written for a right-click.
                            let text = crate::controls::click_words(
                                text,
                                ready.view3d.controls.tools_on_left,
                            );
                            let t = egui::RichText::new(text.as_ref()).small();
                            if *error {
                                crate::menu::error_label(ui, t);
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
                            &mut self.about,
                        );
                        // The quick bar right of the menus (Undo, Redo,
                        // Save first), as in OHL-Structure.
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
                    ui.horizontal(|ui| {
                        ui.strong(&ready.model.line.name);
                        ui.separator();
                        ui.label(ready.selection.describe());
                    });
                });
                drop(bars);
                let windows = crate::perf::span("plugins and windows");
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
                        command: ohl_session::Command::ReplaceLine { line },
                    });
                }
                self.plugins.manager_ui(&ctx);
                // The plugins' table columns (installed, removed, turned
                // on or off).
                let columns = self.plugins.table_columns();
                if ready.model.plugin_columns != columns {
                    ready.model.plugin_columns = columns;
                }
                ready.model.set_check_plugins(self.plugins.check_dirs());
                ready
                    .drawings
                    .set_template_plugins(self.plugins.sheet_template_dirs());
                crate::codes::window(&ctx, ready, &self.plugins);
                crate::rule_editor::presets_window(&ctx, ready, &self.plugins);
                let go = [
                    ready.galloping.ui(&ctx, &ready.model),
                    ready.wirefit.ui(&ctx, &ready.model),
                ];
                if let Some(target) = go.into_iter().flatten().next() {
                    ready.go_to(target);
                }
                // The vegetation panel (its plugin presets read once).
                if ready.vegetation.open
                    || ready.safety_volume.open
                    || ready.vertical_clearances.open
                    || ready.radial_clearances.open
                {
                    crate::rule_editor::ensure_presets(&ctx, ready, &self.plugins);
                }
                let out = ready.vegetation.ui(
                    &ctx,
                    &ready.model,
                    &ready.rule_editor.presets.list,
                    &mut self.palette,
                );
                crate::vegetation::handle(ready, out);
                // Its report drawn on its layers while the panel is open,
                // taken off once it is closed.
                if let Some(overlay) = ready.vegetation.overlay_change(&ready.model) {
                    let lines = ready.vegetation.isoclearance_lines();
                    ready.drawings.set_isoclearance(lines);
                    ready.view3d.vegetation = overlay.clone();
                    ready.plan.vegetation = overlay.clone();
                    ready.profile.set_vegetation(overlay);
                    ready
                        .view3d
                        .dirty
                        .extend([Layer::GrowIn, Layer::FallIn, Layer::Isoclearance]);
                }
                // The safety volumes: drawn while the window is open.
                let out =
                    ready
                        .safety_volume
                        .ui(&ctx, &ready.model, &ready.rule_editor.presets.list);
                crate::safety_volume::handle(ready, out);
                if let Some(overlay) = ready.safety_volume.overlay_change() {
                    ready.view3d.safety_volume = overlay.clone();
                    ready.plan.safety_volume = overlay;
                    ready.view3d.dirty.insert(Layer::SafetyVolume);
                }
                // The checks' pages asked for: opened (floating first).
                if std::mem::take(&mut ready.phase_distance.open) {
                    open_page(&mut self.dock, Tab::PhaseDistance);
                }
                if std::mem::take(&mut ready.check_library.open) {
                    open_page(&mut self.dock, Tab::CheckLibrary);
                }
                // The line of a result gone with its page (or a new run, a
                // report opened: no result picked).
                if ready.view3d.measure.is_some()
                    && (self.dock.find_tab(&Tab::PhaseDistance).is_none()
                        || ready.phase_distance.picked.is_none())
                {
                    ready.view3d.measure = None;
                }
                for (measure, out) in [
                    (
                        ohl_clearance::Measure::Vertical,
                        ready.vertical_clearances.ui(
                            &ctx,
                            &mut ready.model,
                            &ready.rule_editor.presets.list,
                        ),
                    ),
                    (
                        ohl_clearance::Measure::Radial,
                        ready.radial_clearances.ui(
                            &ctx,
                            &mut ready.model,
                            &ready.rule_editor.presets.list,
                        ),
                    ),
                ] {
                    clearance_zone_out(ready, measure, out);
                }
                // Layers drawn from results out of date: dimmed.
                crate::revision::frame(ready);
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
                if let Some(path) = ready.open_project.take() {
                    self.file_action = Some(crate::files::FileAction::Open(Some(path)));
                } else if std::mem::take(&mut ready.reopen) {
                    self.file_action = Some(crate::files::FileAction::Open(Some(
                        ready.model.sources.file.clone(),
                    )));
                }
                if let Some(tab) = crate::menu::take_page_request(&ctx) {
                    ready.release_mouse = true;
                    open_page(&mut self.dock, tab);
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
                drop(windows);
                sync_docked(&ctx, &mut self.dock);
                // The structure being edited: its handles in the 3D view.
                let dock = &self.dock;
                ready.view3d.handles.target = ready.handle_target(|t| dock.find_tab(t).is_some());
                ready.ground_lines_frame(Some(&ctx));
                // The cuts' panels: what the main view rebuilds, rebuilt;
                // their slabs outlined in the main view.
                ready.xsections.hand_on(&mut ready.view3d);
                ready.view3d.cut_outlines = ready.xsections.outlines();
                let _dock = crate::perf::span("dock");
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
                                    ctx: ctx.clone(),
                                    rs: self.render_state.as_ref(),
                                    lod: &mut self.lod,
                                    palette: &mut self.palette,
                                    scripts: self.plugins.python_scripts(),
                                },
                            );
                    });
                for (tab, place) in std::mem::take(&mut ready.dock_moves) {
                    move_page(&mut self.dock, tab, place);
                }
                // Changes made while the pages were drawn: the cuts' panels
                // follow in the next frame.
                if ready.xsections.hand_on(&mut ready.view3d) {
                    ctx.request_repaint();
                }
            }
        }
        if self.about {
            let mut open = true;
            egui::Window::new(format!("About {}", crate::APP_NAME))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(420.0)
                .show(&ctx, crate::help::about_ui);
            self.about = open;
        }
        // The File menu's windows and actions.
        self.share_windows(&ctx);
        self.import_project_windows(&ctx);
        // The plugin manager without a project (the start screen's
        // Plugins…): with one, it is drawn with the project's windows.
        if !matches!(self.state, State::Ready(_)) && self.plugins.open {
            let _ = self.plugins.poll(&ctx);
            self.plugins.manager_ui(&ctx);
        }
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

/// The checks of the plugins as menus: each in its submenu (`menu`, outer
/// first), as its plugin gives it; the id of the one clicked. A check this
/// OpenOHL does not measure for is shown, not usable.
fn plugin_checks_menu(
    ui: &mut egui::Ui,
    checks: &[(Vec<String>, ohl_io::checks::PluginCheck)],
) -> Option<String> {
    let mut clicked = None;
    let mut submenus: Vec<&String> = Vec::new();
    for (path, _) in checks {
        if let Some(first) = path.first()
            && !submenus.contains(&first)
        {
            submenus.push(first);
        }
    }
    for name in submenus {
        let inner: Vec<(Vec<String>, ohl_io::checks::PluginCheck)> = checks
            .iter()
            .filter(|(p, _)| p.first() == Some(name))
            .map(|(p, c)| (p[1..].to_vec(), c.clone()))
            .collect();
        ui.menu_button(name, |ui| {
            if let Some(id) = plugin_checks_menu(ui, &inner) {
                clicked = Some(id);
            }
        });
    }
    for (_, c) in checks.iter().filter(|(p, _)| p.is_empty()) {
        let known = c.info.id == ohl_io::checks::PHASE_DISTANCE;
        let help = c
            .info
            .explanation
            .split("\n\n")
            .next()
            .unwrap_or_default()
            .replace('\n', " ");
        let b = ui
            .add_enabled(known, egui::Button::new(format!("{}…", c.info.title)))
            .on_hover_text(format!("{help} ({})", c.plugin))
            .on_disabled_hover_text(format!(
                "{}: this version of OpenOHL does not measure for \"{}\"",
                c.plugin, c.info.id
            ));
        if b.clicked() {
            clicked = Some(c.info.id.clone());
        }
    }
    clicked
}

/// The project's ground clearance rules (Checks › Ground clearances ›
/// Vertical clearances, else the rule file chosen in Checks ›
/// Clearances) and the last clearance check's violations: what the
/// drawings take for the clearance line and the markers.
fn drawing_checks(r: &Ready) -> (Vec<ohl_clearance::Rule>, Vec<([f64; 3], String)>) {
    let file = crate::clearance_zones::ZoneWindow::project_file(
        &r.model,
        ohl_clearance::zone::Measure::Vertical,
    );
    let ground = |rules: Vec<ohl_clearance::Rule>| -> Vec<ohl_clearance::Rule> {
        rules
            .into_iter()
            .filter(|x| x.target == ohl_clearance::Target::Ground && x.vertical.is_some())
            .collect()
    };
    let mut rules = if file.is_file() {
        ohl_clearance::RuleSet::load_with(&file, &(&r.model.line.defaults).into())
            .map(|s| ground(s.rules))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if rules.is_empty()
        && let Some(Ok(chosen)) = &r.clearances.rules
    {
        rules = ground(chosen.clone());
    }
    let markers = match &r.clearances.report {
        Some(Ok(report)) => report
            .findings
            .iter()
            .map(|f| {
                (
                    f.at,
                    format!(
                        "{} {}: {:.2} m < {:.2} m",
                        f.rule, f.measure, f.distance, f.required
                    ),
                )
            })
            .collect(),
        _ => Vec::new(),
    };
    (rules, markers)
}
