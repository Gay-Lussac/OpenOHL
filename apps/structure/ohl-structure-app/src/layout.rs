// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The panels of the window and where they are: docked with `egui_dock`
//! (as in the line application), each movable (drag its tab), closable
//! and opened again from View or from its menu; the arrangement is kept
//! between runs. By default the tables are on the left, the 3D view on the
//! right.

use eframe::egui;
use egui_dock::{DockState, NodeIndex, NodePath, TabViewer};
use serde::{Deserialize, Serialize};

use crate::app::{Sheet, StructureApp};

/// Key of the arrangement in the window's storage: the name from before
/// OpenOHL, kept so that the arrangement is not lost.
pub const DOCK_KEY: &str = "ols-structure-dock"; // storage key: kept

/// Key of the arrangement's version in the window's storage.
pub const DOCK_VERSION_KEY: &str = "ohl-structure-dock-version";

/// The arrangement's version: an arrangement kept by an older version
/// (none kept) gives way to the default once (owner, 2026-10-26: the
/// calmer arrangement — the 3D view in the middle, the Inspector on its
/// right, the tables as tabs below it).
pub const DOCK_VERSION: u32 = 2;

/// The menus of the work, in order: Geometry (level 1), Body (level 2),
/// Design (level 3), Load cases, Results, Line loads. Each panel belongs
/// to one (the 3D view and the level to none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Area {
    /// Level 1: points, insulators, wire points, sets, the file's fields.
    #[default]
    Geometry,
    /// Level 2: joints, members, generators, the pole, extensions.
    Body,
    /// Level 3: supports, sections per member, groups, sections,
    /// materials, the pole's design.
    Design,
    /// Load-case recipes and the load table.
    LoadCases,
    /// The finite-element results per case.
    Results,
    /// The line's values at the placed structure.
    InTheLine,
}

impl Area {
    /// Every area, in the order of the menus.
    pub const ALL: [Area; 6] = [
        Area::Geometry,
        Area::Body,
        Area::Design,
        Area::LoadCases,
        Area::Results,
        Area::InTheLine,
    ];

    /// Its menu's name.
    pub fn name(self) -> &'static str {
        match self {
            Area::Geometry => "Geometry",
            Area::Body => "Body",
            Area::Design => "Design",
            Area::LoadCases => "Load cases",
            Area::Results => "Results",
            Area::InTheLine => "Line loads",
        }
    }

    /// The panel the work of this area starts in.
    pub fn first_panel(self) -> Panel {
        match self {
            Area::Geometry => Panel::PointsInsulators,
            Area::Body => Panel::Joints,
            Area::Design => Panel::Supports,
            Area::LoadCases => Panel::LoadRecipes,
            Area::Results => Panel::Results,
            Area::InTheLine => Panel::InTheLine,
        }
    }
}

/// A panel of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Panel {
    /// The level, what the next one lacks (links to the rows), advice.
    Level,
    /// The structure in 3D.
    View3d,
    /// Attachment points and insulators.
    PointsInsulators,
    /// Wire points and set points.
    WiresSets,
    /// The file's own fields: name, height, feet, height extensions.
    StructureFields,
    /// Joints.
    Joints,
    /// Members with their groups.
    Members,
    /// Body generators.
    Generators,
    /// The pole's outline.
    Pole,
    /// Height extensions: pieces and legs.
    Extensions,
    /// Supports of the joints.
    Supports,
    /// Each member's section, kind, k.
    MemberSections,
    /// A section and kind per member group.
    Groups,
    /// The file's sections and materials.
    SectionsMaterials,
    /// The pole's design.
    PoleDesign,
    /// The load-case recipes.
    LoadRecipes,
    /// The evaluated load table.
    LoadTable,
    /// The finite-element results.
    Results,
    /// The line's values at the placed structure.
    InTheLine,
    /// The libraries read and where from, what the structure names but
    /// none has (File › Libraries).
    Libraries,
    /// What is selected, with its fields ([`crate::inspector`]).
    Inspector,
    /// A tower's joint in detail: the members arriving, their condition,
    /// the bolts ([`crate::joint_panel`]).
    JointDetails,
    /// The joint of Joint details drawn: a close-up or a face view, a
    /// pane beside the 3D view ([`crate::joint_panel`]); or the member of
    /// Member details laid flat ([`crate::member_panel`]).
    JointView,
    /// A tower's member in detail: its two ends, its capacities live
    /// ([`crate::member_panel`]).
    MemberDetails,
}

impl Panel {
    /// Every panel, in the order of the View menu.
    pub const ALL: [Panel; 24] = [
        Panel::Level,
        Panel::View3d,
        Panel::JointView,
        Panel::Inspector,
        Panel::Libraries,
        Panel::PointsInsulators,
        Panel::WiresSets,
        Panel::StructureFields,
        Panel::Joints,
        Panel::Members,
        Panel::Generators,
        Panel::Pole,
        Panel::Extensions,
        Panel::Supports,
        Panel::MemberSections,
        Panel::Groups,
        Panel::SectionsMaterials,
        Panel::PoleDesign,
        Panel::JointDetails,
        Panel::MemberDetails,
        Panel::LoadRecipes,
        Panel::LoadTable,
        Panel::Results,
        Panel::InTheLine,
    ];

    /// Its title (on its tab and in the View menu).
    pub fn title(self) -> &'static str {
        match self {
            Panel::Level => "To do",
            Panel::View3d => "3D view",
            Panel::PointsInsulators => "Points & strings",
            Panel::WiresSets => "Wire points & sets",
            Panel::StructureFields => "Structure fields",
            Panel::Joints => "Joints",
            Panel::Members => "Members",
            Panel::Generators => "Generators",
            Panel::Pole => "Pole",
            Panel::Extensions => "Extensions",
            Panel::Supports => "Supports",
            Panel::MemberSections => "Member cross-sections",
            Panel::Groups => "Groups",
            Panel::SectionsMaterials => "Cross-sections & materials",
            Panel::PoleDesign => "Pole design",
            Panel::LoadRecipes => "Load case editor",
            Panel::LoadTable => "Load table",
            Panel::Results => "FE results",
            Panel::InTheLine => "Line loads",
            Panel::Libraries => "Libraries",
            Panel::Inspector => "Inspector",
            Panel::JointDetails => "Joint details",
            Panel::JointView => "Detail view",
            Panel::MemberDetails => "Member details",
        }
    }

    /// What it shows (the menus' hover text).
    pub fn tip(self) -> &'static str {
        match self {
            Panel::Level => {
                "What the next level lacks, then advice: each item shows its row and selects its \
                 object"
            }
            Panel::View3d => {
                "The structure in 3D: select, and the hotbar's tools (move, add, link, delete)"
            }
            Panel::Inspector => "What is selected, with its fields to edit",
            Panel::PointsInsulators => {
                "Where strings hang on the structure, and the strings from them"
            }
            Panel::WiresSets => "Where a string ends, and the set numbers where a wire is strung",
            Panel::StructureFields => "Its name, height, feet and extensions",
            Panel::Joints => "The body's joints",
            Panel::Members => "The body's members between two joints, with their groups",
            Panel::Generators => {
                "Build the body from a few dimensions: legs and panels, crossarms, a peak, a \
                 face copied; rebuild it"
            }
            Panel::Pole => "The pole's outside lines: diameters by height, round or polygonal",
            Panel::Extensions => {
                "Extensions: each one's piece below the feet (joints, members) and its \
                 leg extensions"
            }
            Panel::Supports => "Which joints are supported, and how",
            Panel::MemberSections => "Each member's cross-section, kind and buckling factor",
            Panel::Groups => "A cross-section and kind for every member of a group",
            Panel::SectionsMaterials => {
                "The [[section]] and [[material]] tables of the file, and the cross-section library's \
                 cross-sections"
            }
            Panel::PoleDesign => {
                "What the pole check needs: material, wall (or solid) or cross-section, strength"
            }
            Panel::LoadRecipes => {
                "The structure's load cases: line forces and the loads on the structure itself"
            }
            Panel::LoadTable => {
                "The recipes evaluated: per case, the loads at each point or member"
            }
            Panel::Results => "The cases of the load table solved by the finite-element model",
            Panel::JointView => {
                "Beside the 3D view: the joint of Joint details (a close-up or a face view, saved \
                 as SVG or PDF) or the member of Member details laid flat"
            }
            Panel::MemberDetails => {
                "A tower's member in detail: its two ends' bolts and holes (the joints' own), its \
                 capacities worked out live, the usage under the last run's forces"
            }
            Panel::JointDetails => {
                "A tower's joint in detail: the members arriving and from where, how each end \
                 is held, the leg bolted, the bolts; a close-up and a face view"
            }
            Panel::InTheLine => "The line's values at the placed structure, per point and case",
            Panel::Libraries => {
                "The libraries the structure reads (cross-sections, insulator assemblies, poles, models, \
                 recipes, templates), where from, and what it names that none has"
            }
        }
    }

    /// The menu it belongs to (none: the 3D view, the level, the
    /// libraries).
    pub fn area(self) -> Option<Area> {
        Some(match self {
            Panel::Level
            | Panel::View3d
            | Panel::Libraries
            | Panel::Inspector
            | Panel::JointView => return None,
            Panel::PointsInsulators | Panel::WiresSets | Panel::StructureFields => Area::Geometry,
            Panel::Joints
            | Panel::Members
            | Panel::Generators
            | Panel::Pole
            | Panel::Extensions => Area::Body,
            Panel::Supports
            | Panel::MemberSections
            | Panel::Groups
            | Panel::SectionsMaterials
            | Panel::PoleDesign
            | Panel::JointDetails
            | Panel::MemberDetails => Area::Design,
            Panel::LoadRecipes | Panel::LoadTable => Area::LoadCases,
            Panel::Results => Area::Results,
            Panel::InTheLine => Area::InTheLine,
        })
    }

    /// The panels of `area`, in the order of its menu.
    pub fn of(area: Area) -> impl Iterator<Item = Panel> {
        Panel::ALL
            .into_iter()
            .filter(move |p| p.area() == Some(area))
    }

    /// The tables it shows (several: chosen at its top), with their names
    /// and what they hold; none: a panel of its own kind.
    pub fn sheets(self) -> &'static [(Sheet, &'static str, &'static str)] {
        match self {
            Panel::PointsInsulators => &[
                (
                    Sheet::Points,
                    "Attachment points",
                    "Where strings hang on the structure",
                ),
                (
                    Sheet::Strings,
                    "Strings",
                    "Each from a point (or another string's wire point) to its wire point",
                ),
            ],
            Panel::WiresSets => &[
                (
                    Sheet::Wires,
                    "Wire points",
                    "Where a string ends: placed by its string(s)",
                ),
                (
                    Sheet::Sets,
                    "Sets",
                    "Where a wire is strung: a set number on a wire point",
                ),
            ],
            Panel::StructureFields => &[(Sheet::Structure, "Structure", "")],
            Panel::Joints => &[(Sheet::Joints, "Joints", "")],
            Panel::Members => &[(Sheet::Members, "Members", "")],
            Panel::Generators => &[(Sheet::Generators, "Generators", "")],
            Panel::Pole => &[(Sheet::Pole, "Pole", "")],
            Panel::Extensions => &[(Sheet::Extensions, "Extensions", "")],
            Panel::Supports => &[(Sheet::Supports, "Supports", "")],
            Panel::MemberSections => &[(Sheet::Design, "Member cross-sections", "")],
            Panel::Groups => &[(Sheet::Groups, "Groups", "")],
            Panel::SectionsMaterials => &[
                (
                    Sheet::Sections,
                    "Cross-sections",
                    "The [[section]] tables of the file",
                ),
                (
                    Sheet::Materials,
                    "Materials",
                    "The [[material]] tables of the file",
                ),
                (
                    Sheet::Library,
                    "Cross-section library",
                    "The cross-section library's cross-sections (the project line's): new, edit, \
                     duplicate, delete, their properties computed as typed",
                ),
            ],
            Panel::PoleDesign => &[(Sheet::PoleDesign, "Pole design", "")],
            _ => &[],
        }
    }

    /// The panel that shows `sheet`.
    pub fn of_sheet(sheet: Sheet) -> Panel {
        Panel::ALL
            .into_iter()
            .find(|p| p.sheets().iter().any(|s| s.0 == sheet))
            .unwrap_or(Panel::PointsInsulators)
    }
}

/// The default arrangement (owner, 2026-10-26: calmer): the 3D view
/// large in the middle with its hotbar, the Inspector on its right (the
/// To do list behind it), and below the view one area of tables as tabs —
/// geometry, body, design, loads, results. The header (file, level, to
/// do) is above them all, not a panel.
pub fn default_dock() -> DockState<Panel> {
    let mut dock = DockState::new(vec![Panel::View3d]);
    let tree = dock.main_surface_mut();
    let [view, _] = tree.split_right(NodeIndex::root(), 0.8, vec![Panel::Inspector, Panel::Level]);
    // As many tables as show their titles (Groups, Line loads and the
    // others open beside them from their menus).
    tree.split_below(
        view,
        0.6,
        vec![
            Panel::PointsInsulators,
            Panel::WiresSets,
            Panel::Joints,
            Panel::Members,
            Panel::Generators,
            Panel::Supports,
            Panel::MemberSections,
            Panel::LoadTable,
            Panel::Results,
        ],
    );
    if let Some(path) = dock.find_tab(&Panel::Inspector) {
        let _ = dock.set_active_tab(path);
    }
    dock
}

/// The leaf a panel opened again goes to: one with a panel of its menu,
/// else one with any table, else none.
fn home(dock: &DockState<Panel>, panel: Panel) -> Option<NodePath> {
    let loads = |p: &Panel| {
        matches!(
            p.area(),
            Some(Area::LoadCases | Area::Results | Area::InTheLine)
        )
    };
    // The Detail view's details keep their leaf to themselves.
    let table =
        |p: &Panel| p.area().is_some() && !matches!(p, Panel::JointDetails | Panel::MemberDetails);
    let same = |p: &Panel| table(p) && p.area() == panel.area();
    let near = |p: &Panel| table(p) && loads(p) == loads(&panel);
    let any = |p: &Panel| table(p);
    [
        dock.find_tab_from(same),
        dock.find_tab_from(near),
        dock.find_tab_from(any),
    ]
    .into_iter()
    .flatten()
    .next()
    .map(|t| t.node_path())
}

/// Shows `panel`: brought to the front of its leaf and focused; closed, it
/// opens where it belongs (next to a panel of its menu; the 3D view on the
/// right, the level above the tables; the Load case editor in a window of
/// its own).
pub fn open_panel(dock: &mut DockState<Panel>, panel: Panel) {
    if dock.find_tab(&panel).is_none() {
        let tables = home(dock, Panel::PointsInsulators);
        match panel {
            // The Load case editor in a window of its own, large enough
            // to see a case's two parts at once (owner, 2026-10-25:
            // "easily understandable"); dragged into the dock if wanted.
            Panel::LoadRecipes => {
                let surface = dock.add_window(vec![panel]);
                if let Some(w) = dock.get_window_state_mut(surface) {
                    w.set_position(egui::pos2(120.0, 80.0));
                    w.set_size(egui::vec2(980.0, 760.0));
                }
            }
            Panel::View3d => {
                dock.main_surface_mut()
                    .split_right(NodeIndex::root(), 0.45, vec![panel]);
            }
            // The joint drawn in a pane of its own on the right of the 3D
            // view (owner, 2026-11-02: "opens on the tower window as a new
            // pane … easier to not fill the view with screens").
            Panel::JointView => match dock.find_tab(&Panel::View3d) {
                Some(view) if view.surface == egui_dock::SurfaceIndex::main() => {
                    dock.main_surface_mut()
                        .split_right(view.node, 0.5, vec![panel]);
                }
                _ => dock.push_to_first_leaf(panel),
            },
            // Joint and Member details below the Detail view, in a leaf
            // of their own (owner 2026-11-03: the detail view's bottom
            // panel shows only what belongs to it, not the tables, loads
            // and results).
            Panel::JointDetails | Panel::MemberDetails => {
                let other = if panel == Panel::JointDetails {
                    Panel::MemberDetails
                } else {
                    Panel::JointDetails
                };
                let main = |p: &egui_dock::TabPath| p.surface == egui_dock::SurfaceIndex::main();
                match (
                    dock.find_tab(&other),
                    tables.filter(|t| t.surface == egui_dock::SurfaceIndex::main()),
                    dock.find_tab(&Panel::JointView),
                ) {
                    (Some(at), _, _) if details_only(dock, at.node_path()) => {
                        dock.set_focused_node_and_surface(at.node_path());
                        dock.push_to_focused_leaf(panel);
                    }
                    // Beside the tables, under the Detail view (as wide as
                    // it, the tables' height).
                    (_, Some(t), Some(view)) if main(&view) => {
                        dock.main_surface_mut()
                            .split_right(t.node, 0.5, vec![panel]);
                    }
                    (_, _, Some(view)) if main(&view) => {
                        dock.main_surface_mut()
                            .split_below(view.node, 0.55, vec![panel]);
                    }
                    _ => match home(dock, panel) {
                        Some(at) => {
                            dock.set_focused_node_and_surface(at);
                            dock.push_to_focused_leaf(panel);
                        }
                        None => dock.push_to_first_leaf(panel),
                    },
                }
            }
            // The Inspector and the To do list on the right of the view,
            // together.
            Panel::Level | Panel::Inspector => {
                let other = if panel == Panel::Level {
                    Panel::Inspector
                } else {
                    Panel::Level
                };
                match (dock.find_tab(&other), dock.find_tab(&Panel::View3d)) {
                    (Some(at), _) => {
                        dock.set_focused_node_and_surface(at.node_path());
                        dock.push_to_focused_leaf(panel);
                    }
                    (None, Some(view)) if view.surface == egui_dock::SurfaceIndex::main() => {
                        dock.main_surface_mut()
                            .split_right(view.node, 0.8, vec![panel]);
                    }
                    _ => match tables {
                        Some(at) if at.surface == egui_dock::SurfaceIndex::main() => {
                            dock.main_surface_mut()
                                .split_above(at.node, 0.2, vec![panel]);
                        }
                        _ => dock.push_to_first_leaf(panel),
                    },
                }
            }
            _ => match home(dock, panel) {
                Some(at) => {
                    dock.set_focused_node_and_surface(at);
                    dock.push_to_focused_leaf(panel);
                }
                None if dock.find_tab(&Panel::View3d).is_some() => {
                    // Only the 3D view (and the level) left: the tables on
                    // its left again.
                    dock.main_surface_mut()
                        .split_left(NodeIndex::root(), 0.45, vec![panel]);
                }
                None => dock.push_to_first_leaf(panel),
            },
        }
    }
    if let Some(path) = dock.find_tab(&panel) {
        let _ = dock.set_active_tab(path);
        dock.set_focused_node_and_surface(path.node_path());
    }
}

/// Whether the leaf at `at` holds Joint and Member details only (the
/// Detail view's own details).
pub fn details_only(dock: &DockState<Panel>, at: NodePath) -> bool {
    dock.leaf(at).is_ok_and(|l| {
        l.tabs()
            .iter()
            .all(|p| matches!(p, Panel::JointDetails | Panel::MemberDetails))
    })
}

/// Joint or Member details open among other panels (an arrangement kept
/// from before 2026-11-03): moved below the Detail view, alone there.
pub fn settle_details(dock: &mut DockState<Panel>, panel: Panel) {
    let Some(at) = dock.find_tab(&panel) else {
        return;
    };
    if details_only(dock, at.node_path()) || dock.find_tab(&Panel::JointView).is_none() {
        return;
    }
    dock.remove_tab(at);
    open_panel(dock, panel);
}

/// Closes `panel` if it is open.
pub fn close_panel(dock: &mut DockState<Panel>, panel: Panel) {
    if let Some(path) = dock.find_tab(&panel) {
        dock.remove_tab(path);
    }
}

/// The panel in front in the leaf last clicked.
pub fn focused(dock: &mut DockState<Panel>) -> Option<Panel> {
    dock.find_active_focused().map(|(_, p)| *p)
}

/// A menu (or a submenu) of options to tick: ticking a box never closes
/// it, only a click outside it or a command (which closes it itself) does
/// (the same as the line application's).
pub fn options_menu<'a, R>(
    ui: &mut egui::Ui,
    title: impl egui::IntoAtoms<'a>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) {
    use egui::containers::menu::{MenuButton, MenuConfig, SubMenuButton, is_in_menu};
    let config = MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    if is_in_menu(ui) {
        SubMenuButton::new(title).config(config).ui(ui, add);
    } else {
        MenuButton::new(title).config(config).ui(ui, add);
    }
}

/// Draws the panels into the dock.
pub struct Viewer<'a> {
    /// The application (its dock taken out while it is drawn).
    pub app: &'a mut StructureApp,
    /// The panels drawn this frame.
    pub drawn: Vec<Panel>,
}

impl TabViewer for Viewer<'_> {
    type Tab = Panel;

    fn title(&mut self, tab: &mut Panel) -> egui::WidgetText {
        tab.title().into()
    }

    fn id(&mut self, tab: &mut Panel) -> egui::Id {
        egui::Id::new(("structure-panel", *tab))
    }

    fn closeable(&mut self, _tab: &mut Panel) -> bool {
        true
    }

    /// The tables and the view scroll inside themselves; the level, the
    /// loads and the results in the dock's scroll area.
    fn scroll_bars(&self, tab: &Panel) -> [bool; 2] {
        match tab {
            Panel::Level | Panel::Inspector => [false, true],
            Panel::Libraries
            | Panel::LoadRecipes
            | Panel::LoadTable
            | Panel::Results
            | Panel::InTheLine => [true, true],
            _ => [false, false],
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        self.drawn.push(*tab);
        self.app.panel_ui(ui, *tab);
    }
}
