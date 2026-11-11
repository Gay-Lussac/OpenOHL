// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The panels of the window and where they are: docked with `egui_dock`
//! (as in the line application), each movable (drag its tab), closable
//! and opened again from View or from its menu; the arrangement is kept
//! between runs. By default two panes (owner 2026-11-04): the text and
//! edit panels on the left, the 3D view on the right with the Detail view
//! as a tab on top of it.

use eframe::egui;
use egui_dock::{DockState, NodeIndex, NodePath, TabViewer};
use serde::{Deserialize, Serialize};

use crate::app::{Sheet, StructureApp};

/// Key of the arrangement in the window's storage: the name from before
/// OpenOHL, kept so that the arrangement is not lost.
pub const DOCK_KEY: &str = "ols-structure-dock"; // storage key: kept

/// Key of the arrangement's version in the window's storage.
pub const DOCK_VERSION_KEY: &str = "ohl-structure-dock-version";

/// The arrangement's version. One kept by version 1 (none kept) gives way
/// to the default once; one kept by version 2 only when it is that
/// version's default ([`is_old_default`]: the 3D view in the middle, the
/// Inspector on its right, the tables below), not when the user changed it
/// (owner 2026-11-04: the text and edit panels on the left, the views on
/// the right, two panes).
pub const DOCK_VERSION: u32 = 3;

/// The menus of the work, in order: Geometry (level 1), Body (level 2),
/// Design (level 3), Loads, Results. Each panel belongs to one (the 3D
/// view, the Detail view, the Inspector, the To do list and the
/// Libraries to none: View's main panels).
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
    /// Load-case recipes, the load table and the line's values at the
    /// placed structure (the Loads menu).
    LoadCases,
    /// The finite-element results per case.
    Results,
}

impl Area {
    /// Every area, in the order of the menus.
    pub const ALL: [Area; 5] = [
        Area::Geometry,
        Area::Body,
        Area::Design,
        Area::LoadCases,
        Area::Results,
    ];

    /// Its menu's name.
    pub fn name(self) -> &'static str {
        match self {
            Area::Geometry => "Geometry",
            Area::Body => "Body",
            Area::Design => "Design",
            Area::LoadCases => "Loads",
            Area::Results => "Results",
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
    /// none has (File › Import).
    Libraries,
    /// What is selected, with its fields ([`crate::inspector`]).
    Inspector,
    /// A tower's joint in detail: the members arriving, their condition,
    /// the bolts ([`crate::joint_panel`]).
    JointDetails,
    /// The joint of Joint details drawn: a close-up or a face view, a
    /// tab on top of the 3D view ([`crate::joint_panel`]); or the member of
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
                "On top of the 3D view (a tab of its pane): the joint of Joint details (a \
                 close-up or a face view, saved as SVG or PDF) or the member of Member details \
                 laid flat"
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
            Panel::LoadRecipes | Panel::LoadTable | Panel::InTheLine => Area::LoadCases,
            Panel::Results => Area::Results,
        })
    }

    /// View's main panels, in its order: the views, the Inspector, the
    /// To do list, the Libraries (the tables are in View › Tables and
    /// their menus).
    pub const MAIN: [Panel; 5] = [
        Panel::View3d,
        Panel::JointView,
        Panel::Inspector,
        Panel::Level,
        Panel::Libraries,
    ];

    /// Whether it applies to structure `t` (its kind): the Pole panels
    /// not to a tower without a pole, Joint and Member details not to a
    /// pole; why not. A structure of kind "other" shows them all (its
    /// panels say what they need).
    pub fn applies(self, t: &ohl_structures::StructureType) -> Result<(), &'static str> {
        use ohl_structures::structures::StructureKind as K;
        match self {
            Panel::Pole | Panel::PoleDesign
                if ohl_structure_edit::is_tower(t.kind) && t.pole.is_none() =>
            {
                Err("A pole's: this structure is a tower (Geometry › Structure kind)")
            }
            Panel::JointDetails | Panel::MemberDetails if t.kind == K::Pole => {
                Err("A tower's: this structure is a pole (Geometry › Structure kind)")
            }
            _ => Ok(()),
        }
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

/// The share of the window's width the left side (the text and edit
/// panels) takes by default; the views have the rest.
pub const LEFT_SHARE: f32 = 0.44;

/// Whether `panel` is a view (the 3D view, the Detail view): on the right
/// by default, the text and edit panels on the left.
pub fn is_view(panel: Panel) -> bool {
    matches!(panel, Panel::View3d | Panel::JointView)
}

/// The default arrangement (owner, 2026-11-04: "the text and edit
/// parameters on the left and 3d view with toolbar on the right"): two
/// panes. On the left the text and edit panels as tabs — the Inspector,
/// the To do list and the tables of the joints and members, so that every
/// title shows whole (the others — points and strings, supports,
/// wire points, generators, loads, results … — open beside them from
/// their menus; Joint and
/// Member details there too, in front, when a joint or member is opened in
/// the Detail view). On the right the 3D view with its hotbar; the Detail
/// view opens as a tab on top of it, not in a third pane. The header
/// (file, level, to do) is above them all, not a panel.
pub fn default_dock() -> DockState<Panel> {
    // Four tabs, with room for the one a level's area brings to the front
    // when a file opens (Points & strings, Supports …): six did not fit
    // the pane, the last title cut off (UI review 2026-11-11).
    let mut dock = DockState::new(vec![
        Panel::Inspector,
        Panel::Level,
        Panel::Joints,
        Panel::Members,
    ]);
    dock.main_surface_mut()
        .split_right(NodeIndex::root(), LEFT_SHARE, vec![Panel::View3d]);
    if let Some(path) = dock.find_tab(&Panel::Inspector) {
        let _ = dock.set_active_tab(path);
    }
    dock
}

/// The shapes of the main surface the defaults before 2026-11-04 had
/// ([`shape`]): the 3D view in the middle, the Inspector on its right, the
/// tables below (version 2, owner 2026-10-26), with the Detail view beside
/// the 3D view and the details beside the tables as they opened by
/// default. A kept arrangement of this shape is an old default, not the
/// user's own: it gives way to the new default.
const OLD_DEFAULTS: [&str; 3] = [
    "H(V(3d,t),i)",
    "H(V(H(3d,dv),t),i)",
    "H(V(H(3d,dv),H(t,d)),i)",
];

/// The main surface's arrangement in a few letters: a split `H(..,..)`
/// (side by side) or `V(..,..)` (one above the other), a leaf by what it
/// holds — `3d` the 3D view alone, `dv` the Detail view alone, `i` the
/// Inspector and the To do list, `d` Joint or Member details alone, `t`
/// tables, `x` anything else (a mix the user made).
pub fn shape(dock: &DockState<Panel>) -> String {
    fn leaf(tabs: &[Panel]) -> &'static str {
        let all = |f: fn(&Panel) -> bool| !tabs.is_empty() && tabs.iter().all(f);
        if tabs == [Panel::View3d] {
            "3d"
        } else if tabs == [Panel::JointView] {
            "dv"
        } else if all(|p| matches!(p, Panel::Inspector | Panel::Level)) {
            "i"
        } else if all(|p| matches!(p, Panel::JointDetails | Panel::MemberDetails)) {
            "d"
        } else if all(|p| p.area().is_some()) {
            "t"
        } else {
            "x"
        }
    }
    fn walk(nodes: &[&egui_dock::Node<Panel>], at: usize, out: &mut String) {
        match nodes.get(at) {
            Some(egui_dock::Node::Leaf(l)) => out.push_str(leaf(l.tabs())),
            Some(n @ (egui_dock::Node::Horizontal(_) | egui_dock::Node::Vertical(_))) => {
                let h = matches!(n, egui_dock::Node::Horizontal(_));
                out.push_str(if h { "H(" } else { "V(" });
                walk(nodes, NodeIndex(at).left().0, out);
                out.push(',');
                walk(nodes, NodeIndex(at).right().0, out);
                out.push(')');
            }
            _ => out.push('-'),
        }
    }
    let nodes: Vec<_> = dock.main_surface().iter().collect();
    let mut out = String::new();
    walk(&nodes, 0, &mut out);
    out
}

/// Whether a kept arrangement of an older version is one of the old
/// defaults ([`OLD_DEFAULTS`]; a window with the Load case editor alone, as
/// it opens, allowed): replaced by the new default. One the user changed
/// (panes moved or split, tabs mixed or floated) is not: it is kept as
/// theirs.
pub fn is_old_default(dock: &DockState<Panel>) -> bool {
    let windows = dock.iter_surfaces().skip(1).all(|s| {
        s.node_tree().is_none_or(|t| {
            t.iter()
                .filter_map(|n| n.get_leaf())
                .all(|l| l.tabs().iter().all(|p| *p == Panel::LoadRecipes))
        })
    });
    windows && OLD_DEFAULTS.contains(&shape(dock).as_str())
}

/// The leaf a text or edit panel opened again goes to: one without a
/// view (the left), the main window's first — one with the other of its
/// pair (the Inspector and the To do list; Joint and Member details), else
/// one with a panel of its menu, else the one with the Inspector or the To
/// do list, else any; none: no such leaf.
fn home(dock: &DockState<Panel>, panel: Panel) -> Option<NodePath> {
    let pair = match panel {
        Panel::Inspector => Some(Panel::Level),
        Panel::Level => Some(Panel::Inspector),
        Panel::JointDetails => Some(Panel::MemberDetails),
        Panel::MemberDetails => Some(Panel::JointDetails),
        _ => None,
    };
    let mut leaves: Vec<(NodePath, &[Panel])> = dock
        .iter_leaves()
        .filter(|(_, l)| !l.tabs().iter().any(|p| is_view(*p)))
        .map(|(at, l)| (at, l.tabs()))
        .collect();
    leaves.sort_by_key(|(at, _)| at.surface != egui_dock::SurfaceIndex::main());
    let with = |f: &dyn Fn(&Panel) -> bool| {
        leaves
            .iter()
            .find(|(_, tabs)| tabs.iter().any(f))
            .map(|(at, _)| *at)
    };
    pair.and_then(|o| with(&|p| *p == o))
        .or_else(|| panel.area().and_then(|a| with(&|p| p.area() == Some(a))))
        .or_else(|| with(&|p| matches!(p, Panel::Inspector | Panel::Level)))
        .or_else(|| with(&|_| true))
}

/// The leaf of the views (the 3D view's, else the Detail view's): where a
/// view opened again goes, on top of the other.
fn views(dock: &DockState<Panel>) -> Option<NodePath> {
    dock.find_tab(&Panel::View3d)
        .or_else(|| dock.find_tab(&Panel::JointView))
        .map(|t| t.node_path())
}

/// A new pane for `panel` on its side of the whole main window: a view on
/// the right, a text or edit panel on the left.
fn new_side(dock: &mut DockState<Panel>, panel: Panel) {
    let tree = dock.main_surface_mut();
    if tree.is_empty() {
        dock.push_to_first_leaf(panel);
    } else if is_view(panel) {
        tree.split_right(NodeIndex::root(), LEFT_SHARE, vec![panel]);
    } else {
        tree.split_left(NodeIndex::root(), LEFT_SHARE, vec![panel]);
    }
}

/// Shows `panel`: brought to the front of its leaf and focused; closed, it
/// opens on its side (owner 2026-11-04): a view with the other view (the
/// Detail view on top of the 3D view, a tab of the same pane), a text or
/// edit panel on the left with the others (next to a panel of its menu);
/// the Load case editor in a window of its own.
pub fn open_panel(dock: &mut DockState<Panel>, panel: Panel) {
    if dock.find_tab(&panel).is_none() {
        let at = if is_view(panel) {
            views(dock)
        } else {
            home(dock, panel)
        };
        match (panel, at) {
            // The Load case editor in a window of its own, large enough
            // to see a case's two parts at once (owner, 2026-10-25:
            // "easily understandable"); dragged into the dock if wanted.
            (Panel::LoadRecipes, _) => {
                let surface = dock.add_window(vec![panel]);
                if let Some(w) = dock.get_window_state_mut(surface) {
                    w.set_position(egui::pos2(120.0, 80.0));
                    w.set_size(egui::vec2(980.0, 760.0));
                }
            }
            (_, Some(at)) => {
                dock.set_focused_node_and_surface(at);
                dock.push_to_focused_leaf(panel);
            }
            (_, None) => new_side(dock, panel),
        }
    }
    if let Some(path) = dock.find_tab(&panel) {
        let _ = dock.set_active_tab(path);
        dock.set_focused_node_and_surface(path.node_path());
    }
}

/// Joint or Member details in a pane with a view (where they would hide
/// it): moved to the left, with the text and edit panels. Anywhere else
/// they stay (the user's place).
pub fn settle_details(dock: &mut DockState<Panel>, panel: Panel) {
    let Some(at) = dock.find_tab(&panel) else {
        return;
    };
    let with_view = dock
        .leaf(at.node_path())
        .is_ok_and(|l| l.tabs().iter().any(|p| is_view(*p)));
    if with_view {
        dock.remove_tab(at);
        open_panel(dock, panel);
    }
}

/// Whether `a` and `b` are tabs of one pane.
pub fn same_leaf(dock: &DockState<Panel>, a: Panel, b: Panel) -> bool {
    match (dock.find_tab(&a), dock.find_tab(&b)) {
        (Some(a), Some(b)) => a.node_path() == b.node_path(),
        _ => false,
    }
}

/// Whether `panel` is shown: open and the tab in front of its leaf (not
/// behind another tab).
pub fn is_shown(dock: &DockState<Panel>, panel: Panel) -> bool {
    dock.find_tab(&panel)
        .is_some_and(|at| dock.leaf(at.node_path()).is_ok_and(|l| l.active == at.tab))
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
/// it, only a click outside it or a command (which closes it itself) does.
/// As OHL-Design's (`ohl_app::menu::options_menu`, kept apart: the
/// structure application does not depend on the line application): one
/// taller than [`MENU_HEIGHT`] of the screen when last shown scrolls; the
/// button's text is kept short ([`MENU_TITLE`] characters), the whole on
/// hover.
pub fn options_menu<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) {
    use egui::containers::menu::{MenuButton, MenuConfig, SubMenuButton, is_in_menu};
    let config = MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let max = (ui.ctx().content_rect().height() * MENU_HEIGHT).max(200.0);
    let key = ui.id().with(("options-menu-height", title));
    let tall = ui
        .ctx()
        .data(|d| d.get_temp::<f32>(key))
        .is_some_and(|h| h > max);
    let body = |ui: &mut egui::Ui| {
        let (r, height) = if tall {
            let out = egui::ScrollArea::vertical()
                .id_salt(("options-menu", title))
                .max_height(max)
                .show(ui, add);
            (out.inner, out.content_size.y)
        } else {
            let out = ui.scope(add);
            (out.inner, out.response.rect.height())
        };
        ui.ctx().data_mut(|d| d.insert_temp(key, height));
        r
    };
    let short = shortened(title);
    let response = if is_in_menu(ui) {
        SubMenuButton::new(short.as_str())
            .config(config)
            .ui(ui, body)
            .0
    } else {
        MenuButton::new(short.as_str())
            .config(config)
            .ui(ui, body)
            .0
    };
    if short != title {
        response.on_hover_text(title);
    }
}

/// A tick menu button's text: at most [`MENU_TITLE`] characters, cut with
/// "…".
fn shortened(title: &str) -> String {
    if title.chars().count() <= MENU_TITLE {
        title.to_owned()
    } else {
        let mut s: String = title.chars().take(MENU_TITLE - 1).collect();
        s.push('…');
        s
    }
}

/// The longest text a tick menu's button shows, characters.
const MENU_TITLE: usize = 40;

/// The most of the screen's height a tick menu takes (it scrolls beyond).
const MENU_HEIGHT: f32 = 0.8;

/// Where a panel goes from its tab's right-click menu (as OHL-Design's
/// pages).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// On the left, with the text and edit panels (the Inspector, the
    /// tables).
    Left,
    /// On the right, with the views (the 3D view, the Detail view).
    Right,
    /// A window of its own (still a panel: dragged back in).
    Window,
}

/// Moves `panel` to `place` (its tab's right-click menu): into the pane of
/// that side (the views' on the right, the text and edit panels' on the
/// left); none there, a new pane on that side.
pub fn move_panel(dock: &mut DockState<Panel>, panel: Panel, place: Place) {
    let Some(src) = dock.find_tab(&panel) else {
        return;
    };
    match place {
        Place::Window => {
            let rect =
                egui::Rect::from_min_size(egui::pos2(160.0, 120.0), egui::vec2(820.0, 600.0));
            dock.detach_tab(src, rect);
        }
        Place::Left | Place::Right => {
            let right = place == Place::Right;
            let dst = if right {
                dock.find_tab_from(|p| *p != panel && is_view(*p))
                    .map(|t| t.node_path())
            } else {
                dock.iter_leaves()
                    .filter(|(_, l)| {
                        l.tabs().iter().any(|p| *p != panel)
                            && !l.tabs().iter().any(|p| is_view(*p))
                    })
                    .min_by_key(|(at, _)| at.surface != egui_dock::SurfaceIndex::main())
                    .map(|(at, _)| at)
            };
            match dst {
                Some(dst) if dst == src.node_path() => {}
                Some(dst) => dock.move_tab(src, (dst, egui_dock::TabInsert::Append)),
                None => {
                    dock.remove_tab(src);
                    let tree = dock.main_surface_mut();
                    if tree.is_empty() {
                        dock.push_to_first_leaf(panel);
                    } else if right {
                        tree.split_right(NodeIndex::root(), LEFT_SHARE, vec![panel]);
                    } else {
                        tree.split_left(NodeIndex::root(), LEFT_SHARE, vec![panel]);
                    }
                }
            }
        }
    }
    if let Some(path) = dock.find_tab(&panel) {
        let _ = dock.set_active_tab(path);
    }
}

/// Draws the panels into the dock.
pub struct Viewer<'a> {
    /// The application (its dock taken out while it is drawn).
    pub app: &'a mut StructureApp,
    /// The panels drawn this frame.
    pub drawn: Vec<Panel>,
    /// Panels to move (their tabs' right-click menus), once drawn.
    pub moves: Vec<(Panel, Place)>,
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

    /// Right-click on a tab: where to put the panel (as OHL-Design's
    /// pages); moved once the dock is drawn.
    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Panel, _path: NodePath) {
        for (place, text) in [
            (Place::Left, "Dock on the left (text and edit)"),
            (Place::Right, "Dock on the right (views)"),
            (Place::Window, "Float in a window of its own"),
        ] {
            if ui.button(text).clicked() {
                self.moves.push((*tab, place));
                ui.close();
            }
        }
        ui.label(
            egui::RichText::new("Or drag its tab anywhere: beside, above or below another panel.")
                .small()
                .weak(),
        );
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
