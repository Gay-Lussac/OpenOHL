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

mod clearance_panel;
mod drafting;
mod edits;
mod menu_bar;
mod object_windows;
mod settings_windows;
mod structure_edit;
mod tool_use;
mod viewer;
mod windows;
pub(crate) use clearance_panel::*;
pub(crate) use edits::*;
use menu_bar::*;
pub(crate) use object_windows::*;
pub(crate) use settings_windows::*;
pub(crate) use viewer::*;
pub(crate) use windows::*;

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
    /// Checks › (the plugin's submenu) › Distance between conductors under
    /// unequal wind.
    UnequalWind,
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
            Tab::UnequalWind => "Under unequal wind",
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
    /// The wire tension check (its spans above the allowed tension, else
    /// the worst, at their most tensioned support).
    Tensions,
    /// The string check (its strings beyond a limit, else the worst, at
    /// their wire points).
    Strings,
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
    /// Checks › Wire tensions (2026-10-22).
    pub wire_tensions: crate::wire_tensions::WireTensionsWindow,
    /// Checks › String checks….
    pub string_checks: crate::string_checks::StringChecksWindow,
    /// Export › Plans: the route, situation and phase plans.
    pub plans: crate::plans_window::PlansWindow,
    /// Checks › Distance between conductors (LeV).
    pub phase_distance: crate::phase_distance::PhaseDistanceWindow,
    /// The distance between conductors under unequal wind (the same
    /// window, its own check).
    pub unequal_wind: crate::phase_distance::PhaseDistanceWindow,
    /// The weather cases shown before a check's result showed its own,
    /// and the case it showed: shown again when its page closes (unless
    /// another case was chosen meanwhile).
    pub cases_before_check: Option<(Vec<usize>, usize)>,
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
    /// Criteria › Merge duplicate weather cases….
    pub merge_cases: crate::merge_cases::MergeCasesWindow,
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
            wire_tensions: Default::default(),
            string_checks: Default::default(),
            plans: Default::default(),
            phase_distance: Default::default(),
            cases_before_check: None,
            unequal_wind: crate::phase_distance::PhaseDistanceWindow::new(
                crate::phase_distance::Kind::UnequalWind,
            ),
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
            merge_cases: Default::default(),
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

impl Ready {
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
        // (The lines on the ground take the cases displayed: these.)
        need.extend(self.model.display_cases.values().flatten().copied());
        // The project lines shown, in the same cases (from the line worked
        // on's solutions, only what differs solved again).
        self.model.want_others(&need, ctx);
        if self.model.poll_others() {
            self.view3d.dirty.insert(crate::scene::Layer::OtherLines);
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
    /// weather cases displayed and the project lines displayed (the layer
    /// "Other project lines" shown; View › Lines on the ground beside the
    /// line), made when the cases, the line, its solutions, the project
    /// lines, the ground or the settings change, handed to the views.
    /// Display only. The other wind side of a windy case is solved in the
    /// background (`ctx` repainted when done; none: now).
    pub fn ground_lines_frame(&mut self, ctx: Option<&egui::Context>) {
        let projects = self.visible.contains(&Layer::OtherLines);
        if self.ground_lines.frame(
            &self.model,
            self.case,
            &self.model.ground_lines,
            projects,
            ctx,
        ) {
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
                    || ready.wire_tensions.open
                    || ready.string_checks.open
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
                if std::mem::take(&mut ready.unequal_wind.open) {
                    open_page(&mut self.dock, Tab::UnequalWind);
                }
                if std::mem::take(&mut ready.check_library.open) {
                    open_page(&mut self.dock, Tab::CheckLibrary);
                }
                // The line of a result gone with its page (or a new run, a
                // report opened: no result picked).
                let (tab, window) = match ready.view3d.measure_from {
                    crate::phase_distance::Kind::Span => {
                        (Tab::PhaseDistance, &ready.phase_distance)
                    }
                    crate::phase_distance::Kind::UnequalWind => {
                        (Tab::UnequalWind, &ready.unequal_wind)
                    }
                };
                let closed = self.dock.find_tab(&tab).is_none();
                if (ready.view3d.measure.is_some() || !ready.view3d.measure_wires.is_empty())
                    && (closed || window.picked.is_none())
                {
                    ready.view3d.measure = None;
                    ready.view3d.measure_wires.clear();
                }
                // Its page closed: the cases shown before it, back.
                if closed && let Some((before, showed)) = ready.cases_before_check.take() {
                    let untouched =
                        ready.case == showed && ready.model.shown(ready.case) == vec![showed];
                    if untouched {
                        ready.show_cases(before);
                    }
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
                let out =
                    ready
                        .wire_tensions
                        .ui(&ctx, &mut ready.model, &ready.rule_editor.presets.list);
                wire_tensions_out(ready, out);
                let out =
                    ready
                        .string_checks
                        .ui(&ctx, &mut ready.model, &ready.rule_editor.presets.list);
                string_checks_out(ready, out);
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
                        ready.xsections.invalidate();
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
                    ready.xsections.invalidate();
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
