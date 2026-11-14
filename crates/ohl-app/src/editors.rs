// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The editor windows of the tools: a structure (new or existing: id,
//! type, extension, orientation, position), the strings holding a wire
//! point (the structure's points, the strings between them and the wire
//! points, in the structure's own axes), a section (cable, circuit,
//! system, bundle, stringing, points). Applying one makes one session
//! command ([`Form::command`]).

use eframe::egui;
use ohl_line::{
    InsulatorString, Line, Section, SectionPoint, Stringing, Structure, StructurePoint, WirePoint,
};
use ohl_session::{Command, Kind};
use ohl_structures::structures::{to_global, to_local};

use crate::model::Model;

mod alignments;
mod jumpers;
mod section_form;
mod string_form;
mod structure_form;
pub(crate) use alignments::*;
pub(crate) use jumpers::*;
pub(crate) use section_form::*;
use string_form::*;
pub(crate) use structure_form::*;

/// `like` with its trailing number increased until no id in `taken` has
/// it (`T7` → `T8`; `S` → `S-1`); shared with the structure application.
pub use ohl_structure_edit::next_id;

/// A structure being added or changed.
#[derive(Debug, Clone, PartialEq)]
pub struct StructureForm {
    /// Not in the line yet.
    pub new: bool,
    /// Id (editable when new).
    pub id: String,
    /// Structure type.
    pub structure_type: Option<String>,
    /// Height extension of the type.
    pub extension: Option<String>,
    /// Height of the type's feet above the ground (foundations), metres.
    pub height_adjust: f64,
    /// Orientation of the local x axis, degrees from east.
    pub orientation_deg: f64,
    /// Position of the base.
    pub position: [f64; 3],
    /// The base follows the ground.
    pub on_ground: bool,
    /// What it is, in words.
    pub description: String,
    /// Its own points where strings hang (a structure without a type), in
    /// its axes.
    pub points: Vec<PointRow>,
    /// Placed on this alignment point (route as the views name it, index):
    /// it stands there and the point is tied to it.
    pub on_alignment: Option<(String, usize)>,
    /// Its orientation follows the line: ahead along the alignment, its
    /// cross-arms (right to left) on the bisector of the line angle.
    pub along_line: bool,
    /// The routes it gives itself (empty: those of the alignments it
    /// stands on).
    pub routes: Vec<String>,
    /// Its weather case groups ([`ohl_line::Structure::weather_groups`]):
    /// kept as read, no longer used by the checks.
    pub weather_groups: Vec<String>,
    /// Its local conditions ([`ohl_line::Structure::local`]).
    pub local: ohl_line::local::LocalConditions,
    /// The soil resistivity measured at it, Ω·m
    /// ([`ohl_line::Structure::soil_resistivity_ohm_m`]).
    pub soil_resistivity_ohm_m: Option<f64>,
    /// The resistance of its earthing, Ω
    /// ([`ohl_line::Structure::earthing_resistance_ohm`]).
    pub earthing_resistance_ohm: Option<f64>,
    /// A new route's name being typed.
    pub new_route: String,
    /// A new route to make with it.
    pub make_route: Option<String>,
    /// A note of the window (Put it on the line again found no point).
    pub message: Option<String>,
    /// New on an alignment point: the alignment's other points that get a
    /// structure too, in the same command (made when first shown).
    pub more: Option<crate::placement::MorePoints>,
}

/// An alignment point a structure can be put on: (alignment, index there,
/// position, the structure tied to it, the point's name).
pub type ListedPoint = (String, usize, [f64; 3], Option<String>, String);

/// The alignment points a structure can be put on ([`ListedPoint`]).
pub fn alignment_points(line: &Line) -> Vec<ListedPoint> {
    line.alignment_views()
        .into_iter()
        .filter(|r| !r.alignment.is_empty())
        .flat_map(|r| {
            r.alignment
                .iter()
                .enumerate()
                .map(|(k, a)| {
                    (
                        r.id.clone(),
                        k,
                        a.position,
                        a.structure.clone(),
                        a.id.clone(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The orientation (local x, to the right looking ahead, degrees
/// counter-clockwise from east) a structure takes on the line: on
/// alignment point `on` (route, index), ahead along its alignment and its
/// cross-arms on the bisector of the line angle there; elsewhere at `at`,
/// across the nearest stretch of the line. `None`: no line there.
pub fn line_orientation(line: &Line, at: [f64; 3], on: Option<&(String, usize)>) -> Option<f64> {
    let ahead = match on {
        Some((route, k)) => {
            let r = line
                .alignment_views()
                .into_iter()
                .find(|r| &r.id == route)?;
            let pts: Vec<[f64; 3]> = r.alignment.iter().map(|a| a.position).collect();
            let unit = |a: [f64; 3], b: [f64; 3]| {
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let n = dx.hypot(dy);
                (n > 1e-9).then(|| [dx / n, dy / n])
            };
            let back = (*k > 0).then(|| unit(pts[k - 1], pts[*k])).flatten();
            let fore = pts.get(k + 1).and_then(|n| unit(pts[*k], *n));
            match (back, fore) {
                (Some(a), Some(b)) => {
                    let s = [a[0] + b[0], a[1] + b[1]];
                    let n = s[0].hypot(s[1]);
                    if n < 1e-9 { b } else { [s[0] / n, s[1] / n] }
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => return None,
            }
        }
        None => line.direction_at([at[0], at[1]])?,
    };
    // Right of ahead: ahead turned clockwise.
    let right = [ahead[1], -ahead[0]];
    Some(right[1].atan2(right[0]).to_degrees())
}

/// A point of a structure in an editor, in the structure's axes (across:
/// to the right looking ahead, along: ahead, up from the base).
#[derive(Debug, Clone, PartialEq)]
pub struct PointRow {
    /// Label.
    pub label: String,
    /// Position, local.
    pub at: [f64; 3],
    /// What it is, in words.
    pub description: String,
}

/// A wire point in an editor, in the structure's axes.
#[derive(Debug, Clone, PartialEq)]
pub struct WireRow {
    /// Label.
    pub label: String,
    /// Position in still air, local.
    pub at: [f64; 3],
    /// Set: a wire is clamped here.
    pub set: Option<u32>,
    /// A counterweight hanging at it, N.
    pub weight: f64,
    /// With a set: a dead end (its strings are tension strings, lined up
    /// with their conductor), else a suspension point.
    pub dead_end: bool,
    /// What it is, in words.
    pub description: String,
    /// What it was when opened (written to its strings only when changed:
    /// a string's `strain` flag also keeps its weight from being
    /// estimated).
    pub dead_end_was: bool,
}

/// Where `at` is seen from `top` (local axes): its angle from straight
/// down (0–180°: 0 down, 90 level, 180 up), its direction in plan
/// (0–360°: 0 ahead along the line, 90 to the right, 180 back, 270 to the
/// left) and its distance, m; shared with the structure application.
pub use ohl_structure_edit::polar;

/// The point at angles and distance from `top` ([`polar`]).
pub fn from_polar(top: [f64; 3], down: f64, dir: f64, len: f64) -> [f64; 3] {
    let (down, dir) = (down.to_radians(), dir.to_radians());
    let h = len * down.sin();
    [
        top[0] + h * dir.sin(),
        top[1] + h * dir.cos(),
        top[2] - len * down.cos(),
    ]
}

/// What a string takes from an insulator assembly of the library.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryString {
    /// Assembly id.
    pub id: String,
    /// Length, m.
    pub length: f64,
    /// Weight, N (a V's: each leg's half).
    pub weight: f64,
    /// Wind area, m² (a V's: each leg's half).
    pub wind_area: f64,
    /// Axial stiffness EA, N.
    pub stiffness: f64,
    /// A tension (dead-end) assembly.
    pub strain: bool,
}

/// A string in an editor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StringRow {
    /// Label.
    pub label: String,
    /// Its top: a point or a wire point.
    pub top: String,
    /// Its wire point.
    pub wire: String,
    /// Insulator assembly from the library.
    pub insulator: Option<String>,
    /// Weight, N.
    pub weight: f64,
    /// Compression it may take, N (none: tension only).
    pub compression: Option<f64>,
    /// Area the wind blows on, m² (0: its insulator's, if any).
    pub wind_area: f64,
    /// Axial stiffness EA, N (kept as it was).
    pub stiffness: Option<f64>,
    /// A strain string, lined up with its conductor.
    pub strain: bool,
    /// What it is, in words.
    pub description: String,
    /// Its limits, kept and shown but not checked (empty: not given).
    pub limits: ohl_line::StringLimits,
    /// A post insulator's own values (line file version 17; carried
    /// through the window, never lost on Apply — 2026-11-14).
    pub post: PostRow,
}

/// A string's post insulator values in an editor ([`StringRow::post`]):
/// as on [`ohl_line::InsulatorString`], its own (not those a library
/// post gives when the line is read).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostRow {
    /// A post insulator.
    pub post: bool,
    /// On a hinged base (a braced post).
    pub hinged: bool,
    /// Its rated load across it at its top, N.
    pub cantilever_load: Option<f64>,
    /// Its clamp's exit angle from the horizontal, degrees.
    pub exit_angle_deg: Option<f64>,
    /// Its clamp's direction in plan, structure axes, degrees.
    pub exit_direction_deg: Option<f64>,
}

impl PostRow {
    /// String `x`'s own post values: the stored line's (a library post's
    /// `post` flag as resolved).
    pub fn of(model: &Model, structure: &str, x: &ohl_line::InsulatorString) -> Self {
        let own = model
            .stored_line()
            .and_then(|l| l.structure(structure))
            .and_then(|s| s.string(&x.label));
        let o = own.unwrap_or(x);
        Self {
            post: x.post,
            hinged: o.hinged,
            cantilever_load: o.cantilever_load,
            exit_angle_deg: o.exit_angle_deg,
            exit_direction_deg: o.exit_direction_deg,
        }
    }
}

/// Shapes of the strings of a new wire point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// One string hanging from a point.
    I,
    /// Two strings from two points to the wire point.
    V,
    /// A V and a string hanging from its tip.
    VAndString,
    /// A string to a wire point, two strain strings from it to two sets
    /// (back and ahead).
    Tak,
    /// An upright post insulator (a station post) standing on the point,
    /// its wire point 1.5 m above (2026-11-14).
    StationPost,
    /// A horizontal line post standing out 1.5 m from the point, to the
    /// side of the structure the point is on.
    LinePost,
    /// A braced line post: a hinged horizontal post and a brace from a
    /// point 1.5 m above to its top.
    BracedPost,
}

impl Preset {
    /// All, in menus' order.
    pub const ALL: [Preset; 7] = [
        Preset::I,
        Preset::V,
        Preset::VAndString,
        Preset::Tak,
        Preset::StationPost,
        Preset::LinePost,
        Preset::BracedPost,
    ];

    /// The post insulator shapes ([`Preset::ALL`]'s last three).
    pub const POSTS: [Preset; 3] = [Preset::StationPost, Preset::LinePost, Preset::BracedPost];

    /// Name in menus.
    pub fn name(self) -> &'static str {
        match self {
            Preset::I => "One string",
            Preset::V => "Two strings to one wire point",
            Preset::VAndString => "Two strings to one wire point, a string below",
            Preset::Tak => "A string, two tension strings from its wire point",
            Preset::StationPost => "Post insulator, upright (station post)",
            Preset::LinePost => "Post insulator, horizontal (line post)",
            Preset::BracedPost => "Post insulator, braced (hinged post and brace)",
        }
    }
}

/// The strings holding a wire point of a structure, being added or
/// changed: the strings and wire points that move with it, and the
/// structure's points they hang on (all of them, to hang on), in the
/// structure's axes.
#[derive(Debug, Clone, PartialEq)]
pub struct StringForm {
    /// Not on the structure yet.
    pub new: bool,
    /// Structure.
    pub structure: String,
    /// The set it was opened at (a new one: the set proposed).
    pub set: u32,
    /// The structure's points.
    pub points: Vec<PointRow>,
    /// The wire points.
    pub wires: Vec<WireRow>,
    /// The strings.
    pub strings: Vec<StringRow>,
    /// The labels of the strings and wire points when opened (those taken
    /// away are removed).
    pub opened: (Vec<String>, Vec<String>),
    /// The structure is typed and these are its own (over its type's).
    pub overrides_type: bool,
    /// Why the last change asked for was not made (shown in the window).
    pub note: Option<String>,
    /// A typed structure's new points: the attach joint each is linked to
    /// (its id, and where it is in the structure's axes) — only attach
    /// joints are offered ([`crate::link_tool::attach_joints`]).
    pub joints: std::collections::BTreeMap<String, (String, [f64; 3])>,
}

/// A section being added or changed./// A section being added or changed.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionForm {
    /// Not in the line yet.
    pub new: bool,
    /// Id (editable when new).
    pub id: String,
    /// Cable id.
    pub cable: String,
    /// Circuit.
    pub circuit: String,
    /// Phase label.
    pub phase_label: String,
    /// Electrical system.
    pub system: Option<String>,
    /// Subconductors.
    pub bundle: u32,
    /// Subconductor spacing, m.
    pub spacing: f64,
    /// Bundle rotation, degrees.
    pub rotation: f64,
    /// Attachments in line order.
    pub points: Vec<SectionPoint>,
    /// Stringing: the condition in the blocks, or the lengths clipped in.
    pub stringing: Stringing,
    /// Why the stringing was changed (points changed with lengths given).
    pub note: Option<String>,
    /// Cable condition and method it is solved with.
    pub calculation: ohl_line::condition::Calculation,
    /// Its weather case groups ([`ohl_line::Section::weather_groups`]).
    pub weather_groups: Vec<String>,
    /// Its local winds ([`ohl_line::Section::wind_factors`]).
    pub wind_factors: Vec<ohl_line::SpanWind>,
    /// Its local ice ([`ohl_line::Section::ice_factors`]).
    pub ice_factors: Vec<ohl_line::SpanWind>,
    /// Display colour (`#rrggbb`; none: its phase's).
    pub color: Option<String>,
    /// What it is, in words.
    pub description: String,
}

/// What the editor windows need from the application: the GPU (previews)
/// and the structure preview's state (the forms are copied every frame).
pub struct FormExtra<'a> {
    /// The GPU, for the previews.
    pub rs: Option<&'a eframe::egui_wgpu::RenderState>,
    /// The structure window's preview.
    pub preview: &'a mut crate::preview3d::Preview,
    /// The structure picker, open for the structure window.
    pub picker: &'a mut Option<crate::structure_picker::Picker>,
    /// The structure picker's preview.
    pub picker_preview: &'a mut crate::preview3d::Preview,
}

/// An editor window's content.
#[derive(Debug, Clone, PartialEq)]
pub enum Form {
    /// A structure.
    Structure(StructureForm),
    /// A string.
    String(StringForm),
    /// A section.
    Section(SectionForm),
    /// An alignment point.
    Alignment(AlignmentForm),
    /// A jumper.
    Jumper(Box<JumperForm>),
    /// Objects added on a span (the span tool).
    Span(crate::span_tool::SpanForm),
    /// A new spacer between two span points.
    Spacer(crate::span_tool::SpacerForm),
    /// LiDAR points: their coordinates and class.
    Points(crate::lidar_edit::PointsForm),
}

/// What the window asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Apply the form.
    Apply,
    /// Close without a change.
    Cancel,
    /// A typed structure's string: back to the type's.
    ResetString,
    /// Pick the section's points again with the String wire tool.
    PickPoints,
    /// Open Edit › Tools › New pylon from a template… (the window stays).
    NewPylon,
}

impl Form {
    /// Window title.
    pub fn title(&self) -> String {
        match self {
            Form::Structure(f) if f.new => "Add structure".into(),
            Form::Structure(f) => format!("Structure {}", f.id),
            Form::String(f) if f.new && f.strings.iter().any(|x| x.post.post) => {
                format!("Add post insulator to {}", f.structure)
            }
            Form::String(f) if f.new => format!("Add strings to {}", f.structure),
            Form::String(f) if f.set == 0 => {
                format!(
                    "Strings of {} (no wire ends there: a suspension string holding a jumper)",
                    f.structure
                )
            }
            Form::String(f) => format!("Strings of set {} of {}", f.set, f.structure),
            Form::Section(f) if f.new => "String wire: new section".into(),
            Form::Section(f) => format!("Section {}", f.id),
            Form::Alignment(f) => format!("Alignment point {}", f.id),
            Form::Jumper(f) if f.new => format!("New jumper {}", f.jumper.id),
            Form::Jumper(f) => format!("Jumper {}", f.jumper.id),
            Form::Span(f) => match (&f.editing, &f.jumper) {
                (Some(l), Some(j)) => format!("Point {l} of jumper {j}"),
                (Some(l), None) => format!("Point {l} of {}, span {}", f.section, f.span + 1),
                (None, Some(j)) => format!("On jumper {j}"),
                (None, None) => format!("On span {} of {}", f.span + 1, f.section),
            },
            Form::Spacer(f) if f.new => "Add spacer".into(),
            Form::Spacer(f) => format!("Spacer {}", f.spacer.id),
            Form::Points(f) => f.title(),
        }
    }

    /// The command applying the form (or resetting a string to its type),
    /// with what it does in words.
    pub fn command(&self, model: &Model, reset: bool) -> Result<(String, Command), String> {
        let stored = model.stored_line();
        match self {
            Form::Structure(f) => f.command_with_more(model),
            Form::String(f) => {
                if f.new
                    && let Some(set) = f.wires.iter().filter_map(|w| w.set).find(|set| {
                        model
                            .line
                            .structure(&f.structure)
                            .is_some_and(|s| s.wire_at_set(*set).is_some())
                    })
                {
                    return Err(format!(
                        "{} has a wire point with set {set} already",
                        f.structure
                    ));
                }
                f.command(stored, &model.line, reset)
            }
            Form::Section(f) => {
                // Its circuit's other sections take its name, phase and
                // system with it (one command).
                let (what, cmd) = f.command(stored)?;
                let mates = match stored {
                    Some(l) => f.circuit_puts(l, &f.id, &[])?,
                    None => Vec::new(),
                };
                if mates.is_empty() {
                    return Ok((what, cmd));
                }
                let ids: Vec<&str> = mates
                    .iter()
                    .filter_map(|(_, c)| match c {
                        Command::Put { id, .. } => Some(id.as_str()),
                        _ => None,
                    })
                    .collect();
                let what = format!("{what}, and its conductor path's {}", ids.join(", "));
                let mut commands = vec![cmd];
                commands.extend(mates.into_iter().map(|(_, c)| c));
                Ok((what, Command::Batch { commands }))
            }
            Form::Alignment(f) => f.command(model),
            Form::Jumper(f) => f.command(),
            Form::Span(f) => f.command(model),
            Form::Spacer(f) => f.command(model),
            Form::Points(f) => f.command(),
        }
    }

    /// The window's content; what was asked for.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        case: usize,
        extra: &mut FormExtra<'_>,
    ) -> Option<Outcome> {
        let mut out = None;
        let (what, more, tool) = self.intro();
        crate::tool_ui::intro_of(ui, what, more, tool);
        match self {
            Form::Structure(f) => {
                if structure_ui(ui, f, model, extra) {
                    out = Some(Outcome::NewPylon);
                }
            }
            Form::String(f) => {
                if string_ui(ui, f, model) {
                    out = Some(Outcome::ResetString);
                }
            }
            Form::Section(f) => {
                if section_ui(ui, f, model, case) {
                    out = Some(Outcome::PickPoints);
                }
            }
            Form::Alignment(f) => alignment_ui(ui, f, model),
            Form::Jumper(f) => jumper_ui(ui, f, model),
            Form::Span(f) => crate::span_tool::span_ui(ui, f),
            Form::Spacer(f) => crate::span_tool::spacer_ui(ui, f, model),
            Form::Points(f) => crate::lidar_edit::points_ui(ui, f, model),
        }
        // Read-only (a viewer's LiDAR point): nothing to apply.
        let editable = !matches!(self, Form::Points(f) if f.read_only.is_some());
        match crate::tool_ui::buttons(
            ui,
            "Apply",
            editable,
            Some("Read-only: nothing to apply"),
            crate::tool_ui::Keys::Both,
        ) {
            Some(crate::tool_ui::Pressed::Action) => out = Some(Outcome::Apply),
            Some(crate::tool_ui::Pressed::Cancel) => out = Some(Outcome::Cancel),
            None => {}
        }
        out
    }

    /// The window's opening sentence, its longer help and the tool whose
    /// page of the manual explains it.
    fn intro(&self) -> (&'static str, &'static str, crate::tools::Tool) {
        use crate::tools::Tool;
        const ONE_STEP: &str = "Apply makes it one change of the project: Edit › Undo (⌘Z) \
                                takes it back. Cancel (Escape) changes nothing. Hover a field \
                                for what it does.";
        match self {
            Form::Structure(f) if f.new => (
                "A new structure where you clicked: choose its type (its points and strings \
                 come with it), check where it stands, then Apply.",
                "Clicked on an alignment point, it stands on it, turned along the line; \
                 \"Also on other points of this alignment\" puts structures on the \
                 alignment's other points in the same step. Everything can be changed \
                 later with the Edit tool.\n\nApply makes it one change of the project: \
                 Edit › Undo (⌘Z) takes it back. Cancel (Escape) changes nothing. Hover a \
                 field for what it does.",
                Tool::AddStructure,
            ),
            Form::Structure(_) => (
                "This structure: its type, where it stands, how it is turned and the \
                 points its strings hang from.",
                ONE_STEP,
                Tool::Edit,
            ),
            Form::String(f) if f.new && f.strings.iter().any(|x| x.post.post) => (
                "A new post insulator standing on the point you clicked, its wire point at \
                 its top: choose upright, horizontal or braced (shape), its length and \
                 direction, its ratings, then Apply.",
                POST_HELP,
                Tool::AddPost,
            ),
            Form::String(f) if f.new => (
                "New strings hanging from the point you clicked: what each hangs from, \
                 its length and direction, and the wire point at its end.",
                STRING_HELP,
                Tool::AddString,
            ),
            Form::String(_) => (
                "The strings of this place on the structure: what each hangs from, its \
                 length, weight and limits, and the wire point at its end.",
                STRING_HELP,
                Tool::Edit,
            ),
            Form::Section(f) if f.new => (
                "The new section through the strings you clicked: choose its wire type, \
                 circuit and phase, and how it is sagged, then Apply to string it.",
                "A section is a wire strung in one piece between two tension points, over \
                 the suspension strings between them. Its spans are solved in every \
                 weather case as soon as it is strung.\n\nApply makes it one change of the \
                 project: Edit › Undo (⌘Z) takes it back. Cancel (Escape) gives this wire \
                 up; String wire stays held.",
                Tool::StringWire,
            ),
            Form::Section(_) => (
                "This section (the wire strung in one piece between two tension points): \
                 its wire type, circuit, phase and how it is sagged.",
                ONE_STEP,
                Tool::Edit,
            ),
            Form::Alignment(_) => (
                "This alignment point: where it is, and the alignments that run through it.",
                ONE_STEP,
                Tool::Edit,
            ),
            Form::Jumper(f) if f.new => (
                "A new jumper, the slack wire joining two section ends at a structure: its \
                 wire, its length or sag, and the clamps that hold it.",
                ONE_STEP,
                Tool::AddJumper,
            ),
            Form::Jumper(_) => (
                "This jumper, the slack wire joining two section ends at a structure: its \
                 wire, its length or sag, and the clamps that hold it.",
                ONE_STEP,
                Tool::Edit,
            ),
            Form::Span(f) if f.editing.is_some() => (
                "This point on the wire: where it is along the span, and the object on it.",
                ONE_STEP,
                Tool::Edit,
            ),
            Form::Span(_) => (
                "Something on this wire: a bare point, an object (a spacer, a marker ball, \
                 a weight…) or a load spread along it. Choose what and where, then Apply.",
                ONE_STEP,
                Tool::OnSpan,
            ),
            Form::Spacer(f) if f.new => (
                "A spacer between the two span points you clicked.",
                ONE_STEP,
                Tool::AddString,
            ),
            Form::Spacer(_) => ("This spacer between two span points.", ONE_STEP, Tool::Edit),
            Form::Points(_) => (
                "This LiDAR point: its coordinates and its class.",
                ONE_STEP,
                Tool::Edit,
            ),
        }
    }
}

/// What a post insulator is (the Add post insulator window's help).
pub(crate) const POST_HELP: &str = "A post insulator stands on a point of the structure — its \
     base, earthed — and holds a wire point at its other end. Fixed (bolted), it holds the \
     wire point rigidly as a cantilever: its load goes to its base with the moment of its \
     length. Hinged (a braced line post), it turns about its base and only pushes or pulls: \
     a brace string to its top holds it. The shapes: upright (a station post), horizontal \
     (a line post) or braced; 1.5 m long to start with — set its length and direction. Its \
     ratings (the load across its top, compression, tension) are checked in Checks › String \
     checks; a jumper ending at its top leaves its clamp at a fixed exit angle.\n\nApply \
     makes it one change of the project: Edit › Undo (⌘Z) takes it back. Cancel (Escape) \
     changes nothing. Hover a field for what it does.";

/// What a string is (the strings' windows' help).
pub(crate) const STRING_HELP: &str = "A string is one straight insulating member: from the point it hangs \
     from (on the structure, or the bottom of another string) to its wire point, which it \
     makes; placed from its top by an angle from straight down, a direction and its length \
     (where it is before the wire is strung; once strung it is drawn where the calculation \
     puts it). With a set number a wire is clamped at the wire point (sections and jumpers \
     end on it); without one, other strings may hang from it. A second leg of a V ends at \
     another string's wire point.\n\nApply makes it one change of the project: Edit › Undo \
     (⌘Z) takes it back. Cancel (Escape) changes nothing. Hover a field for what it does.";

/// String `x`'s own limits (as stored; not those its insulator gives,
/// which the resolved line has too); a type's string: as resolved.
fn own_limits(
    model: &Model,
    structure: &str,
    x: &ohl_line::InsulatorString,
) -> ohl_line::StringLimits {
    model
        .stored_line()
        .and_then(|l| l.structure(structure))
        .and_then(|s| s.string(&x.label))
        .map_or(x.limits(), |o| o.limits())
}

/// An optional limit: ticked, given (`default` when ticked first);
/// unticked, not given. `angle`: degrees from 0 to 180, else a force, N.
/// Whether it changed.
pub(crate) fn optional_value(
    ui: &mut egui::Ui,
    name: &str,
    v: &mut Option<f64>,
    default: f64,
    angle: bool,
    tip: &str,
) -> bool {
    let mut on = v.is_some();
    let mut changed = ui.checkbox(&mut on, name).on_hover_text(tip).changed();
    if changed {
        *v = on.then_some(default);
    }
    if let Some(x) = v {
        let drag = if angle {
            egui::DragValue::new(x)
                .range(0.0..=180.0)
                .speed(0.5)
                .suffix("°")
        } else {
            egui::DragValue::new(x)
                .range(1.0..=1e9)
                .speed(100.0)
                .suffix(" N")
        };
        changed |= ui.add(drag.max_decimals(3)).on_hover_text(tip).changed();
    }
    changed
}

/// A string's limits (kept, shown, not checked by OpenOHL): the tension
/// it is rated for and the swings it may take, each optional. Whether one
/// changed.
pub(crate) fn limits_ui(ui: &mut egui::Ui, l: &mut ohl_line::StringLimits) -> bool {
    const NOTE: &str = "Compared with the string as solved by Checks › String checks (and \
                        `ohl strings`) in the weather cases of its rules; a plugin may read \
                        it too. Unticked: not given (its insulator's, if the library entry \
                        gives one)";
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("limits").on_hover_text(NOTE);
        changed |= optional_value(
            ui,
            "max load",
            &mut l.max_load,
            100_000.0,
            false,
            &format!("The tension it is rated for. {NOTE}"),
        );
        changed |= optional_value(
            ui,
            "max swing",
            &mut l.max_swing_deg,
            60.0,
            true,
            &format!("The swing from straight down it may take, any direction. {NOTE}"),
        );
    });
    ui.horizontal(|ui| {
        ui.add_space(40.0);
        changed |= optional_value(
            ui,
            "across",
            &mut l.max_swing_across_deg,
            60.0,
            true,
            &format!("The swing across the line it may take, either side. {NOTE}"),
        );
        changed |= optional_value(
            ui,
            "along",
            &mut l.max_swing_along_deg,
            30.0,
            true,
            &format!("The swing along the line it may take, either way. {NOTE}"),
        );
    });
    changed
}

fn value(ui: &mut egui::Ui, v: &mut f64, speed: f64, suffix: &str) -> egui::Response {
    crate::style::field(
        ui,
        egui::DragValue::new(v)
            .speed(speed)
            .max_decimals(3)
            .suffix(suffix),
    )
}
