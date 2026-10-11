// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The editor windows of the tools: a structure (new or existing: id,
//! type, extension, orientation, position), the strings holding a wire
//! point (the structure's points, the strings between them and the wire
//! points, in the structure's own axes), a section (cable, circuit,
//! system, bundle, stringing, points). Applying one makes one session
//! command ([`Form::command`]).

use eframe::egui;
use ols_line::{
    InsulatorString, Line, Section, SectionPoint, Stringing, Structure, StructurePoint, WirePoint,
};
use ols_session::{Command, Kind};
use ols_structures::structures::{to_global, to_local};

use crate::model::Model;

/// `like` with its trailing number increased until no id in `taken` has
/// it (`T7` → `T8`; `S` → `S-1`).
pub fn next_id<'a>(taken: impl Iterator<Item = &'a str> + Clone, like: &str) -> String {
    let digits = like.len() - like.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (stem, num) = like.split_at(like.len() - digits);
    let width = num.len();
    let (stem, mut n) = match num.parse::<u64>() {
        Ok(n) => (stem.to_owned(), n + 1),
        Err(_) => (format!("{like}-"), 1),
    };
    loop {
        let id = format!("{stem}{n:0width$}");
        if !taken.clone().any(|t| t == id) {
            return id;
        }
        n += 1;
    }
}

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
    /// A new route's name being typed.
    pub new_route: String,
    /// A new route to make with it.
    pub make_route: Option<String>,
}

/// The alignment points a structure can be put on: (route as the views
/// name it, index, position, the structure tied to it).
pub fn alignment_points(line: &Line) -> Vec<(String, usize, [f64; 3], Option<String>)> {
    line.alignment_views()
        .into_iter()
        .filter(|r| !r.alignment.is_empty())
        .flat_map(|r| {
            r.alignment
                .iter()
                .enumerate()
                .map(|(k, a)| (r.id.clone(), k, a.position, a.structure.clone()))
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
/// left) and its distance, m.
pub fn polar(top: [f64; 3], at: [f64; 3]) -> (f64, f64, f64) {
    let d = [0, 1, 2].map(|k| at[k] - top[k]);
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1e-9 {
        return (0.0, 0.0, 0.0);
    }
    let down = (-d[2] / len).clamp(-1.0, 1.0).acos().to_degrees();
    let dir = if d[0].hypot(d[1]) < 1e-9 {
        0.0
    } else {
        d[0].atan2(d[1]).to_degrees().rem_euclid(360.0)
    };
    (down, dir, len)
}

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
}

impl Preset {
    /// All, in menus' order.
    pub const ALL: [Preset; 4] = [Preset::I, Preset::V, Preset::VAndString, Preset::Tak];

    /// Name in menus.
    pub fn name(self) -> &'static str {
        match self {
            Preset::I => "I-string",
            Preset::V => "V-string",
            Preset::VAndString => "V + string below",
            Preset::Tak => "TAK (two strain strings on a string)",
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
    pub calculation: ols_line::condition::Calculation,
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
    Jumper(JumperForm),
}

/// A jumper being changed: the jumper as it will be put.
#[derive(Debug, Clone, PartialEq)]
pub struct JumperForm {
    /// The jumper.
    pub jumper: ols_line::Jumper,
    /// A support clamp to be removed, waiting for its confirmation.
    pub remove: Option<usize>,
}

/// How a jumper is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JumperBy {
    Length,
    Sag,
    Rigid,
}

impl JumperForm {
    /// Jumper `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let stored = model.stored_line().unwrap_or(&model.line);
        let jumper = stored.jumpers.iter().find(|j| j.id == id)?.clone();
        Some(Self {
            jumper,
            remove: None,
        })
    }

    fn command(&self) -> Result<(String, Command), String> {
        let j = &self.jumper;
        Ok((
            format!("changed jumper {}", j.id),
            Command::Put {
                kind: Kind::Jumper,
                id: j.id.clone(),
                value: serde_json::to_value(j).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

fn jumper_ui(ui: &mut egui::Ui, f: &mut JumperForm, model: &Model) {
    let j = &mut f.jumper;
    let point = |p: &SectionPoint| format!("{} set {}", p.structure, p.set);
    egui::Grid::new("jumper-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("ends");
            ui.label(format!("{} → {}", point(&j.from), point(&j.to)));
            ui.end_row();
            ui.label("description");
            ui.add(
                egui::TextEdit::singleline(&mut j.description)
                    .desired_width(240.0)
                    .hint_text("what it is (free text)"),
            );
            ui.end_row();
            ui.label("cable");
            egui::ComboBox::from_id_salt("jumper-cable")
                .selected_text(&j.cable)
                .height(400.0)
                .show_ui(ui, |ui| {
                    for c in model.library.iter() {
                        ui.selectable_value(&mut j.cable, c.id.clone(), &c.id);
                    }
                });
            ui.end_row();
            ui.label("given by");
            let mut by = if j.rigid {
                JumperBy::Rigid
            } else if j.length.is_some() {
                JumperBy::Length
            } else {
                JumperBy::Sag
            };
            let before = by;
            ui.horizontal(|ui| {
                ui.radio_value(&mut by, JumperBy::Length, "length")
                    .on_hover_text("Its unstressed (cut) length at its temperature");
                ui.radio_value(&mut by, JumperBy::Sag, "sag")
                    .on_hover_text("Its sag in still air at its temperature: cut to that");
                ui.radio_value(&mut by, JumperBy::Rigid, "rigid")
                    .on_hover_text("A tube or bar: straight between its clamps");
            });
            if by != before {
                match by {
                    JumperBy::Length => {
                        j.rigid = false;
                        j.sag = None;
                        j.length = Some(j.length.unwrap_or(5.0));
                    }
                    JumperBy::Sag => {
                        j.rigid = false;
                        j.length = None;
                        j.sag = Some(j.sag.unwrap_or(1.0));
                    }
                    JumperBy::Rigid => {
                        j.rigid = true;
                        j.length = None;
                        j.sag = None;
                    }
                }
            }
            ui.end_row();
            match by {
                JumperBy::Length => {
                    ui.label("length");
                    if let Some(l) = &mut j.length {
                        value(ui, l, 0.01, " m");
                    }
                    ui.end_row();
                }
                JumperBy::Sag => {
                    ui.label("sag");
                    if let Some(s) = &mut j.sag {
                        value(ui, s, 0.01, " m");
                    }
                    ui.end_row();
                }
                JumperBy::Rigid => {}
            }
            if by != JumperBy::Rigid {
                ui.label("temperature");
                value(ui, &mut j.temperature, 0.5, " °C");
                ui.end_row();
                ui.label("stiffness EI");
                let mut given = j.bending_stiffness.is_some();
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut given, "given").changed() {
                        j.bending_stiffness = given.then_some(0.0);
                    }
                    match &mut j.bending_stiffness {
                        Some(ei) => {
                            value(ui, ei, 1.0, " N·m²").on_hover_text("0: hinged ends");
                        }
                        None => {
                            let mut k = j
                                .stiffness_factor
                                .unwrap_or(ols_line::jumper::DEFAULT_STIFFNESS_FACTOR);
                            ui.label("factor");
                            if ui
                                .add(egui::DragValue::new(&mut k).speed(0.01).range(0.01..=1.0))
                                .on_hover_text(
                                    "Share of the cable's solid-bar stiffness (0.2–0.35 usual)",
                                )
                                .changed()
                            {
                                j.stiffness_factor = Some(k);
                            }
                            if let Some(c) = model.library.get(&j.cable) {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:.0} N·m²",
                                        ols_line::jumper::jumper_bending_stiffness(j, c)
                                    ))
                                    .weak(),
                                );
                            }
                        }
                    }
                });
                ui.end_row();
                let d = ols_line::jumper::DEFAULT_TERMINAL_ANGLE_DEG;
                for (name, angle) in [
                    ("leaves from at", &mut j.angle_from_deg),
                    ("leaves to at", &mut j.angle_to_deg),
                ] {
                    ui.label(name);
                    let mut a = angle.unwrap_or(d);
                    if value(ui, &mut a, 1.0, "° below the conductor").changed() {
                        *angle = Some(a);
                    }
                    ui.end_row();
                }
            }
            ui.label("bundle");
            ui.horizontal(|ui| {
                let mut own = j.bundle.is_some();
                if ui
                    .checkbox(&mut own, "its own")
                    .on_hover_text("Else as the section at its first end")
                    .changed()
                {
                    j.bundle = own.then(|| model.line.jumper_bundle(j).count.max(1));
                    j.bundle_spacing = own.then(|| model.line.jumper_bundle(j).spacing).flatten();
                }
                if let Some(n) = &mut j.bundle {
                    ui.add(egui::DragValue::new(n).range(1..=8));
                    if *n > 1 {
                        let mut sp = j.bundle_spacing.unwrap_or(0.4);
                        ui.label("spacing");
                        if value(ui, &mut sp, 0.01, " m").changed() || j.bundle_spacing.is_none() {
                            j.bundle_spacing = Some(sp);
                        }
                    } else {
                        j.bundle_spacing = None;
                    }
                } else {
                    let b = model.line.jumper_bundle(j);
                    ui.label(egui::RichText::new(format!("as its section: {}", b.count)).weak());
                }
            });
            ui.end_row();
        });
    if j.support.is_some() {
        ui.label(egui::RichText::new("Its support string is kept (Jumpers table).").small());
    }
    if !j.rigid {
        clamps_ui(ui, j, &mut f.remove, model);
    }
}

/// Wire points of structure `id` a support clamp may hang on: without a
/// set, on one string or a V from structure points.
fn clamp_wires(model: &Model, id: &str) -> Vec<String> {
    model.line.structure(id).map_or(Vec::new(), |st| {
        st.wires
            .iter()
            .filter(|w| ols_line::clamp_strings(st, &w.label).is_ok())
            .map(|w| w.label.clone())
            .collect()
    })
}

/// A jumper's support clamps: each on a wire point of a structure, where
/// along the jumper, how long; added and removed (a removal confirmed).
fn clamps_ui(
    ui: &mut egui::Ui,
    j: &mut ols_line::Jumper,
    remove: &mut Option<usize>,
    model: &Model,
) {
    ui.separator();
    ui.label("Helping strings").on_hover_text(
        "Insulator strings of the structure holding the jumper between its tension clamps, \
         like suspension strings on the line: an I or a V to a wire point without a set. \
         Each holds the jumper in a clamp that swings and turns freely; the jumper runs \
         straight through the clamp. The strings take their weight and wind area.",
    );
    let ends: Vec<String> = {
        let mut v = vec![j.from.structure.clone()];
        if j.to.structure != j.from.structure {
            v.push(j.to.structure.clone());
        }
        v
    };
    let from = j.from.structure.clone();
    egui::Grid::new("jumper-clamps")
        .num_columns(5)
        .show(ui, |ui| {
            for (k, c) in j.supports.iter_mut().enumerate() {
                ui.label(format!("helping string {}", k + 1));
                let id = c.structure.clone().unwrap_or_else(|| from.clone());
                egui::ComboBox::from_id_salt(("clamp-structure", k))
                    .selected_text(&id)
                    .show_ui(ui, |ui| {
                        for s in &ends {
                            if ui.selectable_label(*s == id, s).clicked() {
                                c.structure = (*s != from).then(|| s.clone());
                            }
                        }
                    });
                let id = c.structure.clone().unwrap_or_else(|| from.clone());
                let wires = clamp_wires(model, &id);
                egui::ComboBox::from_id_salt(("clamp-wire", k))
                    .selected_text(&c.wire)
                    .show_ui(ui, |ui| {
                        if wires.is_empty() {
                            ui.label("no wire point without a set on strings");
                        }
                        for w in &wires {
                            ui.selectable_value(&mut c.wire, w.clone(), w);
                        }
                    })
                    .response
                    .on_hover_text("A wire point without a set, on one string or a V");
                ui.horizontal(|ui| {
                    ui.label("at");
                    ui.add(
                        egui::DragValue::new(&mut c.at)
                            .speed(0.005)
                            .range(0.01..=0.99)
                            .max_decimals(3),
                    )
                    .on_hover_text(
                        "Its middle, as a share of the jumper's length from its first end",
                    );
                    ui.label("long");
                    ui.add(
                        egui::DragValue::new(&mut c.clamp_length)
                            .speed(0.005)
                            .range(0.01..=1.0)
                            .max_decimals(3)
                            .suffix(" m"),
                    )
                    .on_hover_text("The length of its clamp: the jumper is straight through it");
                });
                if ui.button("Remove…").clicked() {
                    *remove = Some(k);
                }
                ui.end_row();
            }
        });
    if let Some(k) = *remove {
        if k >= j.supports.len() {
            *remove = None;
        } else {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Remove clamp {} (on {} of {})? Nothing else goes with it; its strings stay \
                     on the structure. Applied as one change.",
                    k + 1,
                    j.supports[k].wire,
                    j.supports[k].structure_id(j)
                ));
                if ui.button("Remove").clicked() {
                    j.supports.remove(k);
                    *remove = None;
                }
                if ui.button("Keep").clicked() {
                    *remove = None;
                }
            });
        }
    }
    let why_not = if j.support.is_some() {
        Some("It has a support string: helping strings replace it (remove it in the Jumpers table)")
    } else if j.length.is_none() {
        Some("A jumper held by helping strings is given by its length")
    } else {
        None
    };
    let add = ui
        .add_enabled(why_not.is_none(), egui::Button::new("Add helping string"))
        .on_hover_text(
            "Held by the strings to a wire point without a set of its first end's structure",
        );
    let add = match why_not {
        Some(why) => add.on_disabled_hover_text(why),
        None => add,
    };
    if add.clicked() {
        // After the last clamp, halfway to the end.
        let after = j.supports.last().map_or(0.0, |c| c.at);
        let mut c = ols_line::JumperClamp::new(
            clamp_wires(model, &from)
                .first()
                .cloned()
                .unwrap_or_default(),
            0.5 * (after + 1.0),
        );
        if j.supports.is_empty() {
            c.at = 0.5;
        }
        j.supports.push(c);
    }
}

/// An alignment point being changed.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentForm {
    /// Its alignment.
    pub route: String,
    /// Index in the alignment.
    pub index: usize,
    /// The alignment it goes to (`route`: stays).
    pub target: String,
    /// Its place in that alignment, from 1.
    pub order: usize,
    /// Position.
    pub position: [f64; 3],
    /// Tied under this structure (it follows it).
    pub structure: Option<String>,
    /// The point sits on the ground.
    pub on_ground: bool,
    /// What it is, in words.
    pub description: String,
    /// Its routes of its own (none: its alignment's).
    pub routes: Option<Vec<String>>,
}

/// The command writing `alignments` (the line's, edited) where they
/// differ from the stored line; those gone are deleted. One command (a
/// batch when several).
pub fn alignments_command(
    stored: &Line,
    alignments: &[ols_line::Alignment],
) -> Result<Command, String> {
    let mut commands = Vec::new();
    for (k, a) in alignments.iter().enumerate() {
        let same = stored.alignments.iter().find(|x| x.id == a.id);
        if same == Some(a) {
            continue;
        }
        if a.points.is_empty() {
            if same.is_some() {
                commands.push(Command::Delete {
                    kind: Kind::Alignment,
                    id: a.id.clone(),
                    confirm: true,
                });
            }
            continue;
        }
        commands.push(Command::Put {
            kind: Kind::Alignment,
            id: a.id.clone(),
            value: serde_json::to_value(a).map_err(|e| e.to_string())?,
            ord: if same.is_none() { Some(k as i64) } else { None },
        });
    }
    for a in &stored.alignments {
        if !alignments.iter().any(|x| x.id == a.id) {
            commands.push(Command::Delete {
                kind: Kind::Alignment,
                id: a.id.clone(),
                confirm: true,
            });
        }
    }
    Ok(match commands.len() {
        1 => commands.remove(0),
        _ => Command::Batch { commands },
    })
}

/// Points of the other alignments at `at` in plan (within 1 mm): the
/// junctions that move with it. (alignment index, point index).
fn junctions(
    alignments: &[ols_line::Alignment],
    at: [f64; 3],
    skip: (usize, usize),
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (r, a) in alignments.iter().enumerate() {
        for (k, p) in a.points.iter().enumerate() {
            if (r, k) != skip
                && r != skip.0
                && (p.position[0] - at[0]).hypot(p.position[1] - at[1]) < 1e-3
            {
                out.push((r, k));
            }
        }
    }
    out
}

/// The id a new alignment takes: `alignment 1`, `alignment 2`… (the
/// first free).
pub fn free_alignment_id(line: &Line) -> String {
    (1..)
        .map(|n| format!("alignment {n}"))
        .find(|id| !line.alignments.iter().any(|a| &a.id == id))
        .expect("a free name")
}

/// The id a new route takes: `route 1`, `route 2`… (the first free).
pub fn free_route_id(line: &Line) -> String {
    (1..)
        .map(|n| format!("route {n}"))
        .find(|id| !line.routes.iter().any(|r| &r.id == id))
        .expect("a free name")
}

/// A new route `id` named `name`: the command, what it does.
pub fn new_route(stored: &Line, id: &str, name: &str) -> Result<(String, Command), String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("a route needs a name".into());
    }
    if stored.routes.iter().any(|r| r.id == id) {
        return Err(format!("there is a route {id} already"));
    }
    let r = ols_line::Route::new(id, name.trim());
    Ok((
        format!("new route {id}"),
        Command::Put {
            kind: Kind::Route,
            id: id.to_owned(),
            value: serde_json::to_value(&r).map_err(|e| e.to_string())?,
            ord: Some(stored.routes.len() as i64),
        },
    ))
}

/// An alignment's stored points.
pub fn stored_alignment(stored: &Line, id: &str) -> Option<Vec<ols_line::AlignmentPoint>> {
    stored
        .alignments
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.points.clone())
}

/// The put replacing an alignment's points (its deletion without any).
pub fn alignment_put(
    stored: &Line,
    id: &str,
    points: Vec<ols_line::AlignmentPoint>,
) -> Result<Command, String> {
    let mut a = stored
        .alignments
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("no alignment {id}"))?
        .clone();
    if points.is_empty() {
        return Ok(Command::Delete {
            kind: Kind::Alignment,
            id: id.to_owned(),
            confirm: true,
        });
    }
    a.points = points;
    Ok(Command::Put {
        kind: Kind::Alignment,
        id: id.to_owned(),
        value: serde_json::to_value(&a).map_err(|e| e.to_string())?,
        ord: None,
    })
}

impl AlignmentForm {
    /// Point `index` of alignment `id`.
    pub fn of(model: &Model, id: &str, index: usize) -> Option<Self> {
        let stored = model.stored_line().unwrap_or(&model.line);
        let p = stored_alignment(stored, id)?.get(index)?.clone();
        Some(Self {
            route: id.to_owned(),
            index,
            target: id.to_owned(),
            order: index + 1,
            position: p.position,
            structure: p.structure,
            on_ground: false,
            description: p.description,
            routes: p.routes,
        })
    }

    fn command(&self, model: &Model) -> Result<(String, Command), String> {
        let stored = model
            .stored_line()
            .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
        let mut alignments = stored.alignments.clone();
        let from = alignments
            .iter()
            .position(|r| r.id == self.route)
            .ok_or_else(|| format!("no alignment {}", self.route))?;
        let to = alignments
            .iter()
            .position(|r| r.id == self.target)
            .ok_or_else(|| format!("no alignment {}", self.target))?;
        let old = alignments[from]
            .points
            .get(self.index)
            .ok_or("no such point")?
            .clone();
        let position = match &self.structure {
            Some(s) => {
                model
                    .line
                    .structure(s)
                    .ok_or_else(|| format!("no structure {s}"))?
                    .position
            }
            None => self.position,
        };
        // A junction (other alignments' points at the same place) moves
        // with it.
        let moved = (old.position[0] - position[0]).hypot(old.position[1] - position[1]) > 1e-9
            || (old.position[2] - position[2]).abs() > 1e-9;
        if moved {
            for (r, k) in junctions(&alignments, old.position, (from, self.index)) {
                let p = &mut alignments[r].points[k];
                p.position = position;
                p.structure = self.structure.clone();
            }
        }
        alignments[from].points.remove(self.index);
        let point = ols_line::AlignmentPoint {
            description: self.description.trim().to_owned(),
            position,
            structure: self.structure.clone(),
            routes: self.routes.clone(),
        };
        let at = self.order.max(1) - 1;
        let list = &mut alignments[to].points;
        list.insert(at.min(list.len()), point);
        let what = if from == to && at == self.index {
            format!(
                "changed point {} of alignment {}",
                self.index + 1,
                self.route
            )
        } else {
            format!(
                "point {} of alignment {} now point {} of {}",
                self.index + 1,
                self.route,
                at.min(alignments[to].points.len() - 1) + 1,
                self.target
            )
        };
        Ok((what, alignments_command(stored, &alignments)?))
    }
}

/// A new alignment point at `at`: in the nearest segment of the nearest
/// alignment. The command, and the point (alignment, index).
pub fn insert_alignment_point(
    model: &Model,
    at: [f64; 3],
) -> Result<(String, Command, String, usize), String> {
    let stored = model
        .stored_line()
        .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
    let mut best: Option<(f64, String, Vec<ols_line::AlignmentPoint>, usize)> = None;
    for a in &stored.alignments {
        let points = a.points.clone();
        // Distance in plan to each segment (one point: to it, after it),
        // and where it goes.
        let mut here = if let [p] = &points[..] {
            ((p.position[0] - at[0]).hypot(p.position[1] - at[1]), 1)
        } else {
            (f64::INFINITY, points.len())
        };
        for k in 0..points.len().saturating_sub(1) {
            let (a, b) = (points[k].position, points[k + 1].position);
            let d = [b[0] - a[0], b[1] - a[1]];
            let len2 = d[0] * d[0] + d[1] * d[1];
            let t = if len2 > 0.0 {
                ((at[0] - a[0]) * d[0] + (at[1] - a[1]) * d[1]) / len2
            } else {
                0.0
            };
            let tc = t.clamp(0.0, 1.0);
            let dist = (a[0] + tc * d[0] - at[0]).hypot(a[1] + tc * d[1] - at[1]);
            let index = if t < 0.0 && k == 0 {
                0
            } else if t > 1.0 && k + 2 == points.len() {
                points.len()
            } else {
                k + 1
            };
            if dist < here.0 {
                here = (dist, index);
            }
        }
        if best.as_ref().is_none_or(|b| here.0 < b.0) {
            best = Some((here.0, a.id.clone(), points, here.1));
        }
    }
    let (_, id, mut points, index) =
        best.ok_or("no alignment to add a point to: draw one first (New alignment tool)")?;
    points.insert(index.min(points.len()), ols_line::AlignmentPoint::free(at));
    Ok((
        format!("added point {} to alignment {id}", index + 1),
        alignment_put(stored, &id, points)?,
        id,
        index,
    ))
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
}

impl StructureForm {
    /// A new structure at `at`: the type and orientation of the nearest
    /// structure, the id after the last one's.
    pub fn new_at(model: &Model, at: [f64; 3]) -> Self {
        let line = &model.line;
        let nearest = line.structures.iter().min_by(|a, b| {
            let d = |s: &&Structure| (s.position[0] - at[0]).hypot(s.position[1] - at[1]);
            d(a).total_cmp(&d(b))
        });
        let like = line.structures.last().map_or("T0", |s| s.id.as_str());
        let id = next_id(line.structures.iter().map(|s| s.id.as_str()), like);
        let structure_type = nearest
            .and_then(|s| s.structure_type.clone())
            .or_else(|| model.types.as_ref()?.0.iter().next().map(|t| t.id.clone()));
        // On an alignment point near where it was asked for (5 m), not
        // taken yet: there, tied to it.
        let on_alignment = alignment_points(line)
            .into_iter()
            .filter(|p| p.3.is_none())
            .map(|p| ((p.2[0] - at[0]).hypot(p.2[1] - at[1]), p))
            .filter(|(d, _)| *d < 5.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| (p.0, p.1));
        let along = line_orientation(line, at, on_alignment.as_ref());
        let mut f = Self {
            new: true,
            id,
            structure_type,
            extension: None,
            height_adjust: 0.0,
            orientation_deg: along
                .or_else(|| nearest.map(|s| s.orientation_deg))
                .unwrap_or(0.0),
            position: at,
            on_ground: model.ground_at(at[0], at[1]).is_some(),
            description: String::new(),
            points: Vec::new(),
            on_alignment: None,
            along_line: along.is_some(),
            routes: Vec::new(),
            new_route: String::new(),
            make_route: None,
        };
        if let Some(on) = on_alignment {
            f.place_on(model, on);
        }
        f
    }

    /// Puts it on alignment point `on` (route, index): its position (the
    /// base on the ground there when there is ground), and the line's
    /// orientation there when it follows the line.
    pub fn place_on(&mut self, model: &Model, on: (String, usize)) {
        if let Some(p) = alignment_points(&model.line)
            .into_iter()
            .find(|p| p.0 == on.0 && p.1 == on.1)
        {
            self.position = p.2;
            if let Some(z) = model.ground_at(p.2[0], p.2[1]) {
                self.position[2] = z;
                self.on_ground = true;
            }
        }
        self.on_alignment = Some(on);
        self.follow_line(model);
    }

    /// Its orientation from the line, when it follows it.
    pub fn follow_line(&mut self, model: &Model) {
        if self.along_line
            && let Some(o) =
                line_orientation(&model.line, self.position, self.on_alignment.as_ref())
        {
            self.orientation_deg = o;
        }
    }

    /// Structure `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        let local = |p| to_local(s.position, s.orientation_deg, p);
        Some(Self {
            new: false,
            id: id.to_owned(),
            structure_type: s.structure_type.clone(),
            extension: s.extension.clone(),
            height_adjust: s.height_adjust,
            orientation_deg: s.orientation_deg,
            position: s.position,
            on_ground: false,
            description: s.description.clone(),
            points: model
                .stored_line()
                .and_then(|l| l.structure(id))
                .map(|st| {
                    st.points
                        .iter()
                        .map(|p| PointRow {
                            description: p.description.clone(),
                            label: p.label.clone(),
                            at: local(p.position),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            on_alignment: None,
            along_line: false,
            routes: s.routes.clone(),
            new_route: String::new(),
            make_route: None,
        })
    }

    pub(crate) fn command(&self, stored: Option<&Line>) -> Result<(String, Command), String> {
        let (what, command) = self.structure_command(stored)?;
        // A route made with it: first, in the same command.
        let Some(name) = &self.make_route else {
            return Ok((what, command));
        };
        let stored = stored.ok_or("read-only: open a project to edit")?;
        let (_, route) = new_route(stored, name, name)?;
        Ok((
            format!("{what} on new route {name}"),
            Command::Batch {
                commands: vec![route, command],
            },
        ))
    }

    fn structure_command(&self, stored: Option<&Line>) -> Result<(String, Command), String> {
        let id = self.id.trim().to_owned();
        if id.is_empty() {
            return Err("the structure needs an id".into());
        }
        let s = if self.new {
            if stored.is_some_and(|l| l.structure(&id).is_some()) {
                return Err(format!("structure {id} exists already"));
            }
            Structure {
                orientation_deg: self.orientation_deg,
                structure_type: self.structure_type.clone(),
                extension: self.extension.clone(),
                height_adjust: self.height_adjust,
                description: self.description.trim().to_owned(),
                points: self.global_points(),
                routes: self.routes.clone(),
                ..Structure::new(id.clone(), self.position)
            }
        } else {
            let old = stored
                .and_then(|l| l.structure(&id))
                .ok_or_else(|| format!("no structure {id}"))?;
            let mut s = old.moved_to(self.position);
            if s.orientation_deg != self.orientation_deg {
                // Its own points and wire points turn with it about its base.
                let (position, from) = (s.position, s.orientation_deg);
                let rot = |p: [f64; 3]| {
                    to_global(position, self.orientation_deg, to_local(position, from, p))
                };
                for p in s
                    .points
                    .iter_mut()
                    .map(|p| &mut p.position)
                    .chain(s.wires.iter_mut().map(|w| &mut w.position))
                {
                    *p = rot(*p);
                }
                s.orientation_deg = self.orientation_deg;
            }
            if s.structure_type != self.structure_type {
                // The type's stick drawing comes with the new type.
                s.members.clear();
                s.omit.clear();
                s.omit_sets.clear();
            }
            s.structure_type = self.structure_type.clone();
            s.extension = self.extension.clone();
            s.height_adjust = self.height_adjust;
            s.description = self.description.trim().to_owned();
            s.routes = self.routes.clone();
            // Its own points as in the window (in its axes, turned and
            // moved with it).
            let mut points = self.global_points();
            for p in &s.points {
                if !points.iter().any(|q| q.label == p.label)
                    && s.strings.iter().any(|x| x.top == p.label)
                {
                    points.push(p.clone());
                }
            }
            s.points = points;
            s
        };
        let labels: std::collections::BTreeSet<&str> =
            self.points.iter().map(|p| p.label.trim()).collect();
        if labels.len() != self.points.len() || labels.contains("") {
            return Err("every point needs a label of its own".into());
        }
        let what = if self.new {
            format!("added structure {id}")
        } else {
            format!("changed structure {id}")
        };
        let put = Command::Put {
            kind: Kind::Structure,
            id: id.clone(),
            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
            ord: None,
        };
        // On an alignment point: tied to it (one command).
        let Some((route, index)) = &self.on_alignment else {
            return Ok((what, put));
        };
        let stored = stored.ok_or("read-only: open a project to edit")?;
        let key = route.clone();
        let mut alignment =
            stored_alignment(stored, &key).ok_or_else(|| format!("no alignment {route}"))?;
        let p = alignment
            .get_mut(*index)
            .ok_or_else(|| format!("alignment {route} has no point {}", index + 1))?;
        if let Some(other) = p.structure.as_ref().filter(|o| **o != id) {
            return Err(format!(
                "alignment point {} of {route} is under {other} already",
                index + 1
            ));
        }
        p.structure = Some(id.clone());
        p.position = [s.position[0], s.position[1], p.position[2]];
        Ok((
            format!("{what} on alignment point {} of {route}", index + 1),
            Command::Batch {
                commands: vec![put, alignment_put(stored, &key, alignment)?],
            },
        ))
    }

    /// Its own points in project coordinates.
    fn global_points(&self) -> Vec<StructurePoint> {
        self.points
            .iter()
            .map(|p| StructurePoint {
                description: p.description.clone(),
                label: p.label.trim().to_owned(),
                position: to_global(self.position, self.orientation_deg, p.at),
            })
            .collect()
    }
}

impl StringForm {
    /// A new I-string on structure `id`: the next free set, hanging 3 m
    /// from the top of the structure.
    pub fn new_on(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        let set = s.sets().max().unwrap_or(0) + 1;
        let top = s
            .points
            .iter()
            .map(|p| p.position[2])
            .fold(s.position[2] + 20.0, f64::max)
            - s.position[2];
        let mut form = Self {
            new: true,
            structure: id.to_owned(),
            set,
            points: s
                .points
                .iter()
                .map(|p| PointRow {
                    description: p.description.clone(),
                    label: p.label.clone(),
                    at: to_local(s.position, s.orientation_deg, p.position),
                })
                .collect(),
            wires: Vec::new(),
            strings: Vec::new(),
            opened: (Vec::new(), Vec::new()),
            overrides_type: false,
        };
        form.preset(Preset::I, [0.0, 0.0, top], model);
        Some(form)
    }

    /// The strings holding wire point `set` of structure `id` (as resolved).
    pub fn of(model: &Model, id: &str, set: u32) -> Option<Self> {
        let s = model.line.structure(id)?;
        Self::of_wire(model, id, &s.wire_at_set(set)?.label)
    }

    /// The strings holding wire point `wire` of structure `id` and those
    /// joined to them (as resolved); its set, else one among them, else 0
    /// (a jumper's helping string: no wire ends there).
    pub fn of_wire(model: &Model, id: &str, wire: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        let c = s.component(wire);
        let set = s
            .wire(wire)
            .and_then(|w| w.set)
            .or_else(|| c.wires.iter().find_map(|&i| s.wires[i].set))
            .unwrap_or(0);
        let local = |p| to_local(s.position, s.orientation_deg, p);
        let wires: Vec<WireRow> = c
            .wires
            .iter()
            .map(|&i| {
                let w = &s.wires[i];
                WireRow {
                    description: w.description.clone(),
                    label: w.label.clone(),
                    at: local(w.position),
                    set: w.set,
                    weight: w.weight,
                    dead_end: model.line.is_dead_end(s, &w.label),
                    dead_end_was: model.line.is_dead_end(s, &w.label),
                }
            })
            .collect();
        let strings: Vec<StringRow> = c
            .strings
            .iter()
            .map(|&i| {
                let x = &s.strings[i];
                StringRow {
                    description: x.description.clone(),
                    label: x.label.clone(),
                    top: x.top.clone(),
                    wire: x.wire.clone(),
                    insulator: x.insulator.clone(),
                    weight: x.weight,
                    compression: x.compression,
                    wind_area: x.wind_area,
                    stiffness: x.stiffness,
                    strain: x.strain,
                }
            })
            .collect();
        let overrides_type = s.structure_type.is_some()
            && model.stored_line().is_some_and(|l| {
                l.structure(id).is_some_and(|x| {
                    x.wires
                        .iter()
                        .any(|w| wires.iter().any(|r| r.label == w.label))
                })
            });
        Some(Self {
            new: false,
            structure: id.to_owned(),
            set,
            points: s
                .points
                .iter()
                .map(|p| PointRow {
                    description: p.description.clone(),
                    label: p.label.clone(),
                    at: local(p.position),
                })
                .collect(),
            opened: (
                strings.iter().map(|x| x.label.clone()).collect(),
                wires.iter().map(|w| w.label.clone()).collect(),
            ),
            wires,
            strings,
            overrides_type,
        })
    }

    /// The strings holding the wire point of string `label` of structure
    /// `id` (one with a set among them).
    pub fn of_string(model: &Model, id: &str, label: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        Self::of_wire(model, id, &s.string(label)?.wire)
    }

    /// A label like `stem` not taken on the structure nor in the form.
    fn free_label(&self, model: &Model, stem: &str) -> String {
        let s = model.line.structure(&self.structure);
        let taken = |l: &str| {
            self.points.iter().any(|p| p.label == l)
                || self.wires.iter().any(|w| w.label == l)
                || self.strings.iter().any(|x| x.label == l)
                || s.is_some_and(|s| s.node(l).is_some() || s.string(l).is_some())
        };
        if !taken(stem) {
            return stem.to_owned();
        }
        (2..)
            .map(|n| format!("{stem}.{n}"))
            .find(|l| !taken(l))
            .expect("a free label")
    }

    /// Replaces the strings and wire points by a preset hanging from `top`
    /// (local): new points, wire points and strings, the set(s) from the
    /// form's.
    pub fn preset(&mut self, preset: Preset, top: [f64; 3], model: &Model) {
        let set = self.set;
        // Points the earlier preset added (not on the structure) go.
        let on_structure = model.line.structure(&self.structure);
        self.points
            .retain(|p| on_structure.is_some_and(|s| s.point(&p.label).is_some()));
        self.wires.clear();
        self.strings.clear();
        let at = |d: [f64; 3]| [top[0] + d[0], top[1] + d[1], top[2] + d[2]];
        let point = |form: &mut Self, stem: &str, d: [f64; 3]| {
            let label = form.free_label(model, stem);
            form.points.push(PointRow {
                description: Default::default(),
                label: label.clone(),
                at: at(d),
            });
            label
        };
        let wire = |form: &mut Self, stem: &str, d: [f64; 3], set: Option<u32>| {
            let label = form.free_label(model, stem);
            form.wires.push(WireRow {
                description: Default::default(),
                label: label.clone(),
                at: at(d),
                set,
                weight: 0.0,
                dead_end: false,
                dead_end_was: false,
            });
            label
        };
        let string = |form: &mut Self, stem: &str, top: &str, wire: &str, strain: bool| {
            let label = form.free_label(model, stem);
            form.strings.push(StringRow {
                label,
                top: top.to_owned(),
                wire: wire.to_owned(),
                strain,
                ..Default::default()
            });
        };
        match preset {
            Preset::I => {
                let a = point(self, &format!("A{set}"), [0.0; 3]);
                let w = wire(self, &format!("W{set}"), [0.0, 0.0, -3.0], Some(set));
                string(self, &format!("S{set}"), &a, &w, false);
            }
            Preset::V | Preset::VAndString => {
                let l = point(self, &format!("A{set}-L"), [-2.0, 0.0, 0.0]);
                let r = point(self, &format!("A{set}-R"), [2.0, 0.0, 0.0]);
                let tip = if preset == Preset::V {
                    wire(self, &format!("W{set}"), [0.0, 0.0, -3.0], Some(set))
                } else {
                    wire(self, &format!("V{set}"), [0.0, 0.0, -3.0], None)
                };
                string(self, &format!("S{set}-L"), &l, &tip, false);
                string(self, &format!("S{set}-R"), &r, &tip, false);
                if preset == Preset::VAndString {
                    let w = wire(self, &format!("W{set}"), [0.0, 0.0, -3.5], Some(set));
                    string(self, &format!("S{set}"), &tip, &w, false);
                }
            }
            Preset::Tak => {
                let a = point(self, &format!("A{set}"), [0.0; 3]);
                let n = wire(self, &format!("N{set}"), [0.0, 0.0, -3.0], None);
                let b = wire(self, &format!("W{set}"), [0.0, -3.0, -3.0], Some(set));
                let f = wire(
                    self,
                    &format!("W{}", set + 1),
                    [0.0, 3.0, -3.0],
                    Some(set + 1),
                );
                string(self, &format!("S{set}"), &a, &n, false);
                string(self, &format!("S{set}-B"), &n, &b, true);
                string(self, &format!("S{set}-F"), &n, &f, true);
            }
        }
        // Its strain strings' wire points are dead ends.
        for w in &mut self.wires {
            w.dead_end = self.strings.iter().any(|x| x.wire == w.label && x.strain);
        }
    }

    /// String `k` takes an insulator assembly of the library: its values;
    /// the wire point it makes moves to the assembly's length (same
    /// angles), a strain clamp where the assembly is a tension one.
    pub fn take_from_library(&mut self, k: usize, a: &LibraryString) {
        let Some(x) = self.strings.get_mut(k) else {
            return;
        };
        x.insulator = Some(a.id.clone());
        (x.weight, x.wind_area, x.stiffness) = (a.weight, a.wind_area, Some(a.stiffness));
        let (top, wire) = (x.top.clone(), x.wire.clone());
        let owns = self.strings.iter().position(|y| y.wire == wire) == Some(k);
        let at = self
            .points
            .iter()
            .find(|p| p.label == top)
            .map(|p| p.at)
            .or_else(|| self.wires.iter().find(|w| w.label == top).map(|w| w.at));
        if owns
            && let Some(top) = at
            && let Some(w) = self.wires.iter_mut().find(|w| w.label == wire)
        {
            let (down, dir, _) = polar(top, w.at);
            w.at = from_polar(top, down, dir, a.length);
            if w.set.is_some() {
                w.dead_end = a.strain;
            }
        }
    }

    /// Takes away the wire points no string ends at any more, with the
    /// strings hanging from them (and so on).
    pub fn drop_unused_wires(&mut self) {
        loop {
            let unused: Vec<String> = self
                .wires
                .iter()
                .filter(|w| !self.strings.iter().any(|x| x.wire == w.label))
                .map(|w| w.label.clone())
                .collect();
            if unused.is_empty() {
                return;
            }
            self.wires.retain(|w| !unused.contains(&w.label));
            self.strings.retain(|x| !unused.contains(&x.top));
        }
    }

    /// A string's length, m (from its rows' positions).
    pub fn length(&self, x: &StringRow) -> Option<f64> {
        let at = |l: &str| {
            self.points
                .iter()
                .find(|p| p.label == l)
                .map(|p| p.at)
                .or_else(|| self.wires.iter().find(|w| w.label == l).map(|w| w.at))
        };
        let (a, b) = (at(&x.top)?, at(&x.wire)?);
        Some((0..3).map(|k| (b[k] - a[k]).powi(2)).sum::<f64>().sqrt())
    }

    /// The form put on structure `s` (as stored): its strings and wire
    /// points as the structure's own (replacing those it was opened with;
    /// for a typed structure, over the type's), its points where new or
    /// moved from `resolved` (the structure as resolved).
    pub fn apply_to(&self, s: &mut Structure, resolved: Option<&Structure>) {
        let global = |p| to_global(s.position, s.orientation_deg, p);
        for p in &self.points {
            let position = global(p.at);
            let same = resolved
                .and_then(|r| r.point(&p.label))
                .is_some_and(|q| (0..3).all(|k| (q.position[k] - position[k]).abs() < 1e-6));
            if same && s.point(&p.label).is_none() {
                continue;
            }
            s.points.retain(|q| q.label != p.label);
            s.points.push(StructurePoint {
                description: p.description.clone(),
                label: p.label.clone(),
                position,
            });
        }
        let (strings, wires) = &self.opened;
        s.strings.retain(|x| !strings.contains(&x.label));
        s.wires.retain(|w| !wires.contains(&w.label));
        for w in &self.wires {
            s.wires.retain(|v| v.label != w.label);
            s.wires.push(WirePoint {
                description: w.description.clone(),
                label: w.label.clone(),
                position: global(w.at),
                set: w.set,
                weight: w.weight,
            });
        }
        for x in &self.strings {
            // A string to a set: a tension string where the set is a dead
            // end, else a suspension one.
            let strain = match self.wires.iter().find(|w| w.label == x.wire) {
                Some(w) if w.set.is_some() && w.dead_end != w.dead_end_was => w.dead_end,
                _ => x.strain,
            };
            s.strings.retain(|y| y.label != x.label);
            s.strings.push(InsulatorString {
                description: x.description.clone(),
                label: x.label.clone(),
                top: x.top.clone(),
                wire: x.wire.clone(),
                insulator: x.insulator.clone(),
                weight: x.weight,
                compression: x.compression,
                wind_area: x.wind_area,
                stiffness: x.stiffness,
                strain,
            });
        }
        if s.structure_type.is_some() {
            let now: Vec<&str> = self
                .strings
                .iter()
                .map(|x| x.label.as_str())
                .chain(self.wires.iter().map(|w| w.label.as_str()))
                .collect();
            s.omit.retain(|l| !now.contains(&l.as_str()));
            s.omit_sets
                .retain(|set| !self.wires.iter().any(|w| w.set == Some(*set)));
            // Those taken away from the type's stay out.
            for l in strings.iter().chain(wires) {
                if !now.contains(&l.as_str()) && !s.omit.contains(l) {
                    s.omit.push(l.clone());
                }
            }
        }
    }

    fn command(
        &self,
        stored: Option<&Line>,
        resolved: &Line,
        reset: bool,
    ) -> Result<(String, Command), String> {
        let mut s = stored
            .and_then(|l| l.structure(&self.structure))
            .ok_or_else(|| format!("no structure {}", self.structure))?
            .clone();
        if reset {
            let (strings, wires) = &self.opened;
            s.strings.retain(|x| !strings.contains(&x.label));
            s.wires.retain(|w| !wires.contains(&w.label));
            s.omit
                .retain(|l| !strings.contains(l) && !wires.contains(l));
            s.omit_sets.retain(|set| *set != self.set);
        } else {
            self.apply_to(&mut s, resolved.structure(&self.structure));
        }
        let what = if reset {
            format!("set {} of {} back to its type", self.set, self.structure)
        } else {
            format!(
                "{} the strings of set {} of {}",
                if self.new { "added" } else { "changed" },
                self.set,
                self.structure
            )
        };
        Ok((
            what,
            Command::Put {
                kind: Kind::Structure,
                id: s.id.clone(),
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

/// A section of a circuit that a change of another section's circuit
/// name, phase or system also changes, and what changes on it.
pub type CircuitMate = (String, Vec<&'static str>);

impl SectionForm {
    /// The other sections of its circuit (its conductor path) that its
    /// circuit name, phase or system change too, when it changes them: an
    /// existing section (`original`: its id in the line), the fields it
    /// changes, on the sections that differ. A circuit keeps one name, one
    /// phase and one system.
    pub fn circuit_mates(&self, stored: &Line, original: &str) -> Vec<CircuitMate> {
        if self.new {
            return Vec::new();
        }
        let Some(old) = stored.sections.iter().find(|s| s.id == original) else {
            return Vec::new();
        };
        let circuit = old.circuit != self.circuit;
        let phase = old.phase_label != self.phase_label;
        let system = old.system != self.system;
        if !(circuit || phase || system) {
            return Vec::new();
        }
        let path = ols_line::electrical::conductor_paths(stored)
            .into_iter()
            .find(|p| p.iter().any(|id| id == original))
            .unwrap_or_default();
        stored
            .sections
            .iter()
            .filter(|m| m.id != original && path.contains(&m.id))
            .filter_map(|m| {
                let mut what = Vec::new();
                if circuit && m.circuit != self.circuit {
                    what.push("circuit");
                }
                if phase && m.phase_label != self.phase_label {
                    what.push("phase");
                }
                if system && m.system != self.system {
                    what.push("system");
                }
                (!what.is_empty()).then(|| (m.id.clone(), what))
            })
            .collect()
    }

    /// The puts giving the sections of [`Self::circuit_mates`] its circuit
    /// name, phase and system (those it changes), but those in `skip`
    /// (changed by hand alongside).
    pub fn circuit_puts(
        &self,
        stored: &Line,
        original: &str,
        skip: &[String],
    ) -> Result<Vec<(String, Command)>, String> {
        let mut out = Vec::new();
        for (id, what) in self.circuit_mates(stored, original) {
            if skip.contains(&id) {
                continue;
            }
            let Some(mut m) = stored.sections.iter().find(|s| s.id == id).cloned() else {
                continue;
            };
            if what.contains(&"circuit") {
                m.circuit = self.circuit.clone();
            }
            if what.contains(&"phase") {
                m.phase_label = self.phase_label.clone();
            }
            if what.contains(&"system") {
                m.system = self.system.clone();
            }
            out.push((
                format!("section {id}: {} as its circuit", what.join(", ")),
                Command::Put {
                    kind: Kind::Section,
                    id,
                    value: serde_json::to_value(&m).map_err(|e| e.to_string())?,
                    ord: None,
                },
            ));
        }
        Ok(out)
    }

    /// Section `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.sections.iter().find(|s| s.id == id)?;
        Some(Self {
            new: false,
            id: id.to_owned(),
            cable: s.cable.clone(),
            circuit: s.circuit.clone(),
            phase_label: s.phase_label.clone(),
            system: s.system.clone(),
            bundle: s.bundle,
            spacing: s.bundle_spacing.unwrap_or(0.4),
            rotation: s.bundle_rotation_deg.unwrap_or(0.0),
            points: s.points.clone(),
            stringing: s.stringing.clone(),
            note: None,
            calculation: s.calculation,
            color: s.color.clone(),
            description: s.description.clone(),
        })
    }

    /// A new section through `points`, like `template` (a section's id:
    /// cable, circuit, phase, system, bundle; its tension in case `case`).
    pub fn new_through(
        model: &Model,
        case: usize,
        points: Vec<SectionPoint>,
        template: Option<&str>,
    ) -> Self {
        let line = &model.line;
        let like = template
            .and_then(|t| line.sections.iter().find(|s| s.id == t))
            .or_else(|| {
                line.sections.iter().find(|s| {
                    s.points.iter().any(|p| {
                        points
                            .first()
                            .is_some_and(|q| p.structure == q.structure && p.set == q.set)
                    })
                })
            })
            .or_else(|| line.sections.first());
        let id = next_id(
            line.sections.iter().map(|s| s.id.as_str()),
            line.sections.last().map_or("S0", |s| s.id.as_str()),
        );
        let mut f = match like.and_then(|s| Self::of(model, &s.id)) {
            Some(f) => f,
            None => Self {
                new: true,
                id: String::new(),
                cable: model
                    .library
                    .iter()
                    .next()
                    .map_or(String::new(), |c| c.id.clone()),
                circuit: String::new(),
                phase_label: String::new(),
                system: None,
                bundle: 1,
                spacing: 0.4,
                rotation: 0.0,
                points: Vec::new(),
                stringing: Stringing::Tension {
                    horizontal_tension: 20_000.0,
                    temperature: 15.0,
                },
                note: None,
                calculation: Default::default(),
                color: None,
                description: String::new(),
            },
        };
        // In the blocks (rolling) by default: a clipped template's
        // condition, or its tension as solved in the shown case — its
        // lengths belong to its own spans.
        if f.stringing.clipped() {
            f.stringing = f.unclipped(model, case);
        }
        f.new = true;
        f.id = id;
        f.points = points;
        f.note = None;
        f
    }

    /// The condition to go back to the blocks with: the one it was sagged
    /// to before clipping in, else its tension as solved in case `case`
    /// (first span), at the clipping temperature.
    pub fn unclipped(&self, model: &Model, case: usize) -> Stringing {
        match &self.stringing {
            Stringing::UnstressedLengths {
                sagged: Some(s), ..
            } => (**s).clone(),
            Stringing::UnstressedLengths { temperature, .. } => {
                let h = model
                    .cases
                    .get(case)
                    .and_then(|c| c.shapes.iter().find(|x| x.section == self.id))
                    .and_then(|x| x.spans.first())
                    .map_or(20_000.0, |sp| sp.horizontal_tension);
                Stringing::Tension {
                    horizontal_tension: (h / 10.0).round() * 10.0,
                    temperature: *temperature,
                }
            }
            other => other.clone(),
        }
    }

    /// Clips in: each span's unstressed length at the condition, on the
    /// current geometry.
    pub fn clip_in(&mut self, model: &Model) -> Result<(), String> {
        if self.stringing.clipped() {
            return Ok(());
        }
        let cable = model
            .library
            .get(&self.cable)
            .ok_or_else(|| format!("cable {} is not in the wire library", self.cable))?;
        let section = self.section(None);
        let (lengths, temperature) = ols_line::solve::clip_lengths(&model.line, &section, cable)
            .map_err(|e| e.to_string())?;
        self.stringing = Stringing::UnstressedLengths {
            lengths,
            temperature,
            sagged: Some(Box::new(self.stringing.clone())),
        };
        Ok(())
    }

    /// Sets new points; a clipped section with another number of spans
    /// goes back to the blocks.
    pub fn set_points(&mut self, model: &Model, case: usize, points: Vec<SectionPoint>) {
        let spans = points.len().saturating_sub(1);
        if let Stringing::UnstressedLengths { lengths, .. } = &self.stringing
            && lengths.len() != spans
        {
            let n = lengths.len();
            self.stringing = self.unclipped(model, case);
            self.note = Some(format!(
                "{spans} spans now instead of {n}: back in the blocks, to be clipped in again"
            ));
        }
        if let Stringing::Sag { span: Some(k), .. } = &mut self.stringing
            && *k >= spans
        {
            *k = spans.saturating_sub(1);
        }
        self.points = points;
    }

    /// The section as typed (loads and colour kept from `old`).
    pub fn section(&self, old: Option<&Section>) -> Section {
        Section {
            description: self.description.trim().to_owned(),
            id: self.id.trim().to_owned(),
            cable: self.cable.clone(),
            circuit: self.circuit.clone(),
            phase_label: self.phase_label.clone(),
            system: self.system.clone(),
            bundle: self.bundle.max(1),
            bundle_spacing: (self.bundle > 1).then_some(self.spacing),
            bundle_rotation_deg: (self.bundle > 1 && self.rotation != 0.0).then_some(self.rotation),
            points: self.points.clone(),
            stringing: self.stringing.clone(),
            loads: old.map_or_else(Vec::new, |o| o.loads.clone()),
            color: self.color.clone(),
            calculation: self.calculation,
        }
    }

    pub(crate) fn command(&self, stored: Option<&Line>) -> Result<(String, Command), String> {
        let id = self.id.trim().to_owned();
        if id.is_empty() {
            return Err("the section needs an id".into());
        }
        if self.points.len() < 2 {
            return Err("a section needs two points or more".into());
        }
        let old = stored.and_then(|l| l.sections.iter().find(|s| s.id == id));
        if self.new && old.is_some() {
            return Err(format!("section {id} exists already"));
        }
        match &self.stringing {
            Stringing::Tension {
                horizontal_tension, ..
            } if *horizontal_tension <= 0.0 => {
                return Err("the tension must be above 0".into());
            }
            Stringing::Sag { sag, .. } if *sag <= 0.0 => {
                return Err("the sag must be above 0".into());
            }
            _ => {}
        }
        let s = self.section(old);
        Ok((
            if self.new {
                format!("strung section {id}")
            } else {
                format!("changed section {id}")
            },
            Command::Put {
                kind: Kind::Section,
                id,
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

impl Form {
    /// Window title.
    pub fn title(&self) -> String {
        match self {
            Form::Structure(f) if f.new => "Add structure".into(),
            Form::Structure(f) => format!("Structure {}", f.id),
            Form::String(f) if f.new => format!("Add strings to {}", f.structure),
            Form::String(f) if f.set == 0 => {
                format!(
                    "Strings of {} (no wire ends there: a jumper's helping string)",
                    f.structure
                )
            }
            Form::String(f) => format!("Strings of set {} of {}", f.set, f.structure),
            Form::Section(f) if f.new => "String wire: new section".into(),
            Form::Section(f) => format!("Section {}", f.id),
            Form::Alignment(f) => format!("Alignment point {} of {}", f.index + 1, f.route),
            Form::Jumper(f) => format!("Jumper {}", f.jumper.id),
        }
    }

    /// The command applying the form (or resetting a string to its type),
    /// with what it does in words.
    pub fn command(&self, model: &Model, reset: bool) -> Result<(String, Command), String> {
        let stored = model.stored_line();
        match self {
            Form::Structure(f) => f.command(stored),
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
                let what = format!("{what}, and its circuit's {}", ids.join(", "));
                let mut commands = vec![cmd];
                commands.extend(mates.into_iter().map(|(_, c)| c));
                Ok((what, Command::Batch { commands }))
            }
            Form::Alignment(f) => f.command(model),
            Form::Jumper(f) => f.command(),
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
        match self {
            Form::Structure(f) => structure_ui(ui, f, model, extra),
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
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Apply").on_hover_text("Enter").clicked() {
                out = Some(Outcome::Apply);
            }
            if ui.button("Cancel").on_hover_text("Escape").clicked() {
                out = Some(Outcome::Cancel);
            }
        });
        out
    }
}

fn value(ui: &mut egui::Ui, v: &mut f64, speed: f64, suffix: &str) -> egui::Response {
    ui.add(
        egui::DragValue::new(v)
            .speed(speed)
            .max_decimals(3)
            .suffix(suffix),
    )
}

fn structure_ui(
    ui: &mut egui::Ui,
    f: &mut StructureForm,
    model: &Model,
    extra: &mut FormExtra<'_>,
) {
    let types: Vec<&ols_structures::StructureType> = model
        .types
        .as_ref()
        .map(|(s, _)| s.iter().collect())
        .unwrap_or_default();
    let help = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).small().weak());
    };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(330.0);
            egui::Grid::new("structure-form").num_columns(2).show(ui, |ui| {
                ui.label("id");
                ui.add_enabled(f.new, egui::TextEdit::singleline(&mut f.id).desired_width(140.0));
                ui.end_row();
                ui.label("type").on_hover_text(
                    "A structure type of the library (its points and strings come from it), \
                     or none: its points given here, its strings added with Add string",
                );
                egui::ComboBox::from_id_salt("structure-type")
                    .selected_text(f.structure_type.as_deref().unwrap_or("(none: points given here)"))
                    .height(400.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.structure_type, None, "(none: points given here)");
                        for t in &types {
                            ui.selectable_value(
                                &mut f.structure_type,
                                Some(t.id.clone()),
                                format!("{} — {}", t.id, t.name),
                            );
                        }
                    });
                ui.end_row();
                let extensions: Vec<String> = f
                    .structure_type
                    .as_deref()
                    .and_then(|id| types.iter().find(|t| t.id == id))
                    .map(|t| t.extensions.iter().map(|e| e.name.clone()).collect())
                    .unwrap_or_default();
                if !extensions.is_empty() {
                    ui.label("extension");
                    egui::ComboBox::from_id_salt("structure-extension")
                        .selected_text(f.extension.as_deref().unwrap_or("(base)"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut f.extension, None, "(base)");
                            for e in &extensions {
                                ui.selectable_value(&mut f.extension, Some(e.clone()), e);
                            }
                        });
                    ui.end_row();
                }
                if f.structure_type.is_some() {
                    ui.label("height adjustment");
                    value(ui, &mut f.height_adjust, 0.01, " m")
                        .on_hover_text("Height of the type's feet above the ground, e.g. for foundations");
                    ui.end_row();
                }
                ui.label("description");
                ui.add(
                    egui::TextEdit::multiline(&mut f.description)
                        .desired_rows(2)
                        .desired_width(200.0)
                        .hint_text("what it is (free text)"),
                );
                ui.end_row();
                // Its routes: from the alignment it stands on, or its own.
                ui.label("routes").on_hover_text(
                    "The routes it is on. Standing on an alignment (a point, or within 5 m of \
                     it): that alignment's routes, by itself. Away from every alignment (a \
                     crossing line): tick its route(s) here",
                );
                ui.vertical(|ui| {
                    let line = model.stored_line().unwrap_or(&model.line);
                    let mut line_with = line.clone();
                    if let Some(r) = &f.make_route {
                        line_with.routes.push(ols_line::Route::new(r, r));
                    }
                    crate::alignment_tools::route_ticks(
                        ui,
                        "structure-routes",
                        &line_with,
                        &mut f.routes,
                        &mut f.new_route,
                        &mut f.make_route,
                    );
                    let found = if f.on_alignment.is_some() || !f.routes.is_empty() {
                        Vec::new()
                    } else {
                        let mut probe = line.clone();
                        let mut s = ols_line::Structure::new("(new)".to_owned(), f.position);
                        s.routes.clear();
                        probe.structures.push(s);
                        probe.routes_of("(new)")
                    };
                    if !f.routes.is_empty() {
                        ui.label(egui::RichText::new("its own (ticked)").small().weak());
                    } else if f.on_alignment.is_some() {
                        ui.label(
                            egui::RichText::new("those of the alignment point it stands on")
                                .small()
                                .weak(),
                        );
                    } else if found.is_empty() {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            egui::RichText::new(
                                "on no alignment: tick its route(s) (or put it on an alignment point)",
                            )
                            .small(),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(format!("{} (from the alignment)", found.join(", ")))
                                .small()
                                .weak(),
                        );
                    }
                });
                ui.end_row();
            });
            ui.separator();
            ui.strong("Where it stands");
            let points = alignment_points(&model.line);
            let mut on = f.on_alignment.is_some();
            ui.horizontal(|ui| {
                ui.radio_value(&mut on, false, "at coordinates");
                ui.add_enabled_ui(!points.is_empty(), |ui| {
                    ui.radio_value(&mut on, true, "on an alignment point")
                        .on_disabled_hover_text("The line has no alignment points yet");
                });
            });
            if on && f.on_alignment.is_none() {
                if let Some(p) = points.iter().find(|p| p.3.is_none()) {
                    f.place_on(model, (p.0.clone(), p.1));
                }
            } else if !on {
                f.on_alignment = None;
            }
            match f.on_alignment.clone() {
                Some((route, k)) => {
                    let text = |r: &str, k: usize, p: [f64; 3]| {
                        format!("{r}, point {} — E {:.1}, N {:.1}", k + 1, p[0], p[1])
                    };
                    let shown = points
                        .iter()
                        .find(|p| p.0 == route && p.1 == k)
                        .map_or(String::new(), |p| text(&p.0, p.1, p.2));
                    let mut chosen = None;
                    egui::ComboBox::from_id_salt("structure-alignment-point")
                        .selected_text(shown)
                        .width(300.0)
                        .height(300.0)
                        .show_ui(ui, |ui| {
                            for p in &points {
                                let taken = p.3.as_ref().filter(|s| **s != f.id);
                                let label = match taken {
                                    Some(s) => format!("{} (under {s})", text(&p.0, p.1, p.2)),
                                    None => text(&p.0, p.1, p.2),
                                };
                                if ui
                                    .add_enabled(
                                        taken.is_none(),
                                        egui::Button::selectable(p.0 == route && p.1 == k, label),
                                    )
                                    .clicked()
                                {
                                    chosen = Some((p.0.clone(), p.1));
                                }
                            }
                        });
                    if let Some(c) = chosen {
                        f.place_on(model, c);
                    }
                    help(ui, "It stands on the point, tied to it: the alignment follows it when it moves.");
                }
                None => {
                    let before = f.position;
                    egui::Grid::new("structure-position").num_columns(2).show(ui, |ui| {
                        for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
                            ui.label(name);
                            ui.add_enabled_ui(k < 2 || !f.on_ground, |ui| {
                                value(ui, &mut f.position[k], 0.1, " m");
                            });
                            ui.end_row();
                        }
                    });
                    ui.checkbox(&mut f.on_ground, "base on the ground");
                    if f.position != before {
                        f.follow_line(model);
                    }
                }
            }
            if f.on_ground
                && let Some(z) = model.ground_at(f.position[0], f.position[1])
            {
                f.position[2] = z;
            }
            ui.separator();
            ui.strong("Orientation");
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut f.along_line, "along the line")
                    .on_hover_text(
                        "Ahead along the alignment; its cross-arms (right to left) on the \
                         bisector of the line angle there",
                    )
                    .changed()
                {
                    f.follow_line(model);
                }
                ui.add_enabled_ui(!f.along_line, |ui| {
                    value(ui, &mut f.orientation_deg, 0.1, "°").on_hover_text(
                        "Direction of its right (local x: the cross-arms), counter-clockwise from east",
                    );
                });
            });
            help(ui, "Its right (the cross-arms), counter-clockwise from east; ahead is a quarter turn to the left of it.");
        });
        // The preview, in its own axes, with the compass.
        extra.preview.shows(&format!("{} {}", f.id, f.new));
        let (key, labels) = structure_preview_key(f, model);
        let size = egui::vec2(360.0, 360.0);
        extra.preview.ui(
            ui,
            extra.rs,
            size,
            key,
            || structure_scene(f, model),
            &labels,
        );
    });
    if f.structure_type.is_none() {
        ui.separator();
        ui.strong("Points where strings hang");
        help(
            ui,
            "In its axes: across (to the right looking ahead), along (ahead), up from its base. \
             Strings are added on them with the Add string tool (or Edit on a string).",
        );
        let used: Vec<String> = model
            .line
            .structure(&f.id)
            .map(|s| s.strings.iter().map(|x| x.top.clone()).collect())
            .unwrap_or_default();
        let mut remove = None;
        egui::Grid::new("structure-points")
            .num_columns(2)
            .show(ui, |ui| {
                for (k, p) in f.points.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut p.label).desired_width(90.0));
                    ui.horizontal(|ui| {
                        for (i, name) in ["across", "along", "up"].iter().enumerate() {
                            ui.label(*name);
                            value(ui, &mut p.at[i], 0.05, " m");
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut p.description)
                                .desired_width(140.0)
                                .hint_text("description"),
                        );
                        if used.contains(&p.label) {
                            ui.label(egui::RichText::new("strings hang from it").small().weak());
                        } else if ui
                            .small_button("×")
                            .on_hover_text("Take the point away")
                            .clicked()
                        {
                            remove = Some(k);
                        }
                    });
                    ui.end_row();
                }
            });
        if let Some(k) = remove {
            f.points.remove(k);
        }
        if ui.small_button("+ point").clicked() {
            let taken: Vec<&str> = f.points.iter().map(|p| p.label.as_str()).collect();
            let label = next_id(
                taken.iter().copied(),
                f.points.last().map_or("A0", |p| p.label.as_str()),
            );
            let at = f
                .points
                .last()
                .map_or([0.0, 0.0, 20.0], |p| [-p.at[0], p.at[1], p.at[2]]);
            f.points.push(PointRow {
                description: Default::default(),
                label,
                at,
            });
        }
    }
}

/// What the structure window's preview shows, as a key (built again when
/// it changes), and its labels.
fn structure_preview_key(f: &StructureForm, model: &Model) -> (u64, Vec<crate::preview3d::Label>) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{f:?}").hash(&mut h);
    model.line.display.ahead_deg.to_bits().hash(&mut h);
    if let Some(s) = model.line.structure(&f.id) {
        format!("{:?}{:?}", s.strings, s.wires).hash(&mut h);
    }
    let g = structure_geometry(f, model);
    let mut labels = compass_labels(f, model, &g);
    let orange = egui::Color32::from_rgb(255, 160, 80);
    let blue = egui::Color32::from_rgb(110, 200, 255);
    for (l, p) in &g.points {
        labels.push(([p[0], p[1], p[2] + 0.8], l.clone(), orange));
    }
    for (l, p, set) in &g.wires {
        let text = set.map_or(l.clone(), |s| format!("{l} (set {s})"));
        labels.push(([p[0], p[1], p[2] - 0.8], text, blue));
    }
    (h.finish(), labels)
}

/// The structure in its own axes (base at 0): its type's, or its own
/// points with the strings it has.
fn structure_geometry(f: &StructureForm, model: &Model) -> crate::type_workspace::Geometry {
    let none = ols_structures::InsulatorLibrary::default();
    if let Some((types, insulators)) = &model.types
        && let Some(t) = f.structure_type.as_deref().and_then(|id| types.get(id))
    {
        let mut g = crate::type_workspace::geometry(t, insulators);
        // The base at 0.
        let foot = t.foot_level();
        let down = |p: &mut [f64; 3]| p[2] -= foot;
        g.joints.iter_mut().for_each(|j| down(&mut j.1));
        g.members.iter_mut().for_each(|m| {
            down(&mut m.1);
            down(&mut m.2)
        });
        g.points.iter_mut().for_each(|p| down(&mut p.1));
        g.wires.iter_mut().for_each(|w| down(&mut w.1));
        g.strings.iter_mut().for_each(|s| {
            down(&mut s.1);
            down(&mut s.2)
        });
        g.sticks.iter_mut().flatten().for_each(down);
        return g;
    }
    let _ = &none;
    let mut g = crate::type_workspace::Geometry {
        points: f.points.iter().map(|p| (p.label.clone(), p.at)).collect(),
        ..Default::default()
    };
    if let Some(s) = model.line.structure(&f.id) {
        let local = |p| to_local(s.position, s.orientation_deg, p);
        g.wires = s
            .wires
            .iter()
            .map(|w| (w.label.clone(), local(w.position), w.set))
            .collect();
        let at = |l: &str| {
            g.points
                .iter()
                .find(|p| p.0 == l)
                .map(|p| p.1)
                .or_else(|| g.wires.iter().find(|w| w.0 == l).map(|w| w.1))
        };
        g.strings = s
            .strings
            .iter()
            .filter_map(|x| Some((x.label.clone(), at(&x.top)?, at(&x.wire)?)))
            .collect();
    }
    // A mast to the highest point and arms to the points.
    let top = g.points.iter().map(|p| p.1[2]).fold(5.0, f64::max);
    g.sticks.push(vec![[0.0; 3], [0.0, 0.0, top]]);
    for (_, p) in &g.points {
        g.sticks.push(vec![[0.0, 0.0, p[2]], *p]);
    }
    g
}

/// The compass's radius: beyond the structure's reach in plan.
fn compass_radius(g: &crate::type_workspace::Geometry) -> f64 {
    let reach = g
        .points
        .iter()
        .map(|p| p.1)
        .chain(g.wires.iter().map(|w| w.1))
        .chain(g.joints.iter().map(|j| j.1))
        .chain(g.sticks.iter().flatten().copied())
        .map(|p| p[0].hypot(p[1]))
        .fold(3.0, f64::max);
    (reach * 1.25).max(0.35 * height(g))
}

/// The structure's height (its highest point).
fn height(g: &crate::type_workspace::Geometry) -> f64 {
    g.points
        .iter()
        .map(|p| p.1)
        .chain(g.wires.iter().map(|w| w.1))
        .chain(g.sticks.iter().flatten().copied())
        .map(|p| p[2])
        .fold(0.0, f64::max)
}

/// Where a plan angle of the project's convention points, in the
/// structure's axes (clockwise seen from above, ahead at the project's
/// angle ahead).
pub(crate) fn compass_direction(angle: f64, ahead: f64) -> [f64; 2] {
    let phi = (angle - ahead).to_radians();
    [phi.sin(), phi.cos()]
}

fn compass_labels(
    f: &StructureForm,
    model: &Model,
    g: &crate::type_workspace::Geometry,
) -> Vec<crate::preview3d::Label> {
    compass_texts(
        compass_radius(g),
        model.line.display.ahead_deg,
        Some(f.orientation_deg),
        0.0,
    )
}

/// The compass's texts (radius `r`, at height `z`): the four angles as
/// the project counts them (`ahead`), with ahead, right, back, left; and
/// north when the structure's orientation is known.
pub(crate) fn compass_texts(
    r: f64,
    ahead: f64,
    orientation: Option<f64>,
    z: f64,
) -> Vec<crate::preview3d::Label> {
    let r = r * 1.12;
    let mut out = Vec::new();
    let grey = egui::Color32::from_rgb(200, 200, 200);
    for a in [0.0, 90.0, 180.0, 270.0] {
        let d = compass_direction(a, ahead);
        let word = match ((a - ahead).rem_euclid(360.0)).round() as i64 {
            0 => " ahead",
            90 => " right",
            180 => " back",
            270 => " left",
            _ => "",
        };
        out.push(([d[0] * r, d[1] * r, z], format!("{a:.0}°{word}"), grey));
    }
    if let Some(o) = orientation {
        let o = o.to_radians();
        out.push((
            [o.sin() * r * 0.8, o.cos() * r * 0.8, z],
            "N".into(),
            egui::Color32::from_rgb(120, 170, 255),
        ));
    }
    out
}

/// The compass on the ground (radius `r`, at height `z`): a ring with
/// ticks every 15° as the project counts them, ahead (green) and right
/// (red) from the centre, north (blue) when the orientation is known.
pub(crate) fn draw_compass(
    s: &mut ols_render::Scene,
    r: f64,
    ahead: f64,
    orientation: Option<f64>,
    z: f64,
) {
    let ring: Vec<[f64; 3]> = (0..=72)
        .map(|k| {
            let t = f64::from(k) * std::f64::consts::TAU / 72.0;
            [r * t.cos(), r * t.sin(), z]
        })
        .collect();
    s.polyline(&ring, [110, 115, 125, 255], 1.5);
    for k in 0..24 {
        let d = compass_direction(f64::from(k) * 15.0, ahead);
        let inner = if k % 6 == 0 {
            0.82
        } else if k % 2 == 0 {
            0.9
        } else {
            0.95
        };
        s.polyline(
            &[
                [d[0] * r * inner, d[1] * r * inner, z],
                [d[0] * r, d[1] * r, z],
            ],
            [150, 155, 165, 255],
            1.5,
        );
    }
    s.polyline(
        &[[0.0, 0.0, z], [0.0, r * 0.75, z]],
        [90, 200, 90, 255],
        3.0,
    );
    s.polyline(
        &[[0.0, 0.0, z], [r * 0.75, 0.0, z]],
        [230, 80, 70, 255],
        3.0,
    );
    if let Some(o) = orientation {
        let o = o.to_radians();
        s.polyline(
            &[[0.0, 0.0, z], [o.sin() * r * 0.6, o.cos() * r * 0.6, z]],
            [120, 170, 255, 255],
            2.0,
        );
    }
}

/// The structure window's preview: the ground's compass, the structure's
/// sticks, points, strings and wire points.
fn structure_scene(f: &StructureForm, model: &Model) -> ols_render::Scene {
    let g = structure_geometry(f, model);
    let mut s = ols_render::Scene::new([0.0; 3]);
    let r = compass_radius(&g);
    let ahead = model.line.display.ahead_deg;
    draw_compass(&mut s, r, ahead, Some(f.orientation_deg), 0.0);
    for l in &g.sticks {
        s.polyline(l, [215, 215, 220, 255], 2.0);
    }
    for (_, a, b, _) in &g.members {
        s.polyline(&[*a, *b], [215, 215, 220, 255], 1.5);
    }
    for (_, a, b) in &g.strings {
        s.polyline(&[*a, *b], [120, 180, 255, 255], 2.5);
    }
    for (_, p) in &g.points {
        s.point(*p, [255, 150, 60, 255], 9.0);
    }
    for (_, p, _) in &g.wires {
        s.point(*p, [90, 200, 255, 255], 9.0);
    }
    s
}

/// Returns true when "back to the type" is clicked.
fn string_ui(ui: &mut egui::Ui, f: &mut StringForm, model: &Model) -> bool {
    // Directions in plan as the project counts them.
    let ahead = model.line.display.ahead_deg;
    let mut reset = false;
    if f.new {
        ui.horizontal(|ui| {
            ui.label("set");
            ui.add(egui::DragValue::new(&mut f.set).range(1..=9999));
            ui.label("shape");
            let top = f
                .strings
                .first()
                .and_then(|x| f.points.iter().find(|p| p.label == x.top))
                .map_or([0.0, 0.0, 20.0], |p| p.at);
            let mut chosen = None;
            egui::ComboBox::from_id_salt("string-preset")
                .selected_text("replace with…")
                .show_ui(ui, |ui| {
                    for p in Preset::ALL {
                        if ui.selectable_label(false, p.name()).clicked() {
                            chosen = Some(p);
                        }
                    }
                });
            if let Some(p) = chosen {
                f.preset(p, top, model);
            }
        });
    }
    let help = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).small().weak());
    };
    let structure = model.line.structure(&f.structure);
    // Points of the structure strings may hang from (with the new ones of
    // the form), and where each is (local).
    let on_structure: Vec<String> = structure
        .map(|s| s.points.iter().map(|p| p.label.clone()).collect())
        .unwrap_or_default();
    let new_points: Vec<usize> = (0..f.points.len())
        .filter(|&k| !on_structure.contains(&f.points[k].label))
        .collect();
    let point_names: Vec<String> = on_structure
        .iter()
        .cloned()
        .chain(new_points.iter().map(|&k| f.points[k].label.clone()))
        .collect();
    egui::ScrollArea::vertical()
        .max_height(460.0)
        .show(ui, |ui| {
            ui.strong("Strings");
            help(
                ui,
                "A string is an insulator chain: from the point it hangs from (on the structure, \
                 or the bottom of another string) to its wire point, which it makes: placed from \
                 its top by an angle from straight down, a direction and its length (where it is \
                 before the wire is strung; once strung it is drawn where the calculation puts \
                 it). With a set number a wire is clamped at the wire point (sections and jumpers \
                 end on it); without one, other strings may hang from it. A second leg of a V \
                 ends at another string's wire point.",
            );
            // (name, a V's leg, what the string takes).
            let library: Vec<(String, bool, LibraryString)> = model
                .types
                .as_ref()
                .map(|(_, lib)| {
                    lib.iter()
                        .map(|a| {
                            use ols_structures::AssemblyKind as K;
                            let v = a.kind == K::VString;
                            let share = if v { 0.5 } else { 1.0 };
                            (
                                a.name.clone(),
                                v,
                                LibraryString {
                                    id: a.id.clone(),
                                    length: a.length.si(),
                                    weight: a.weight.si() * share,
                                    wind_area: a.wind_area.si() * share,
                                    stiffness: a.axial_stiffness.si(),
                                    strain: a.kind == K::Strain,
                                },
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let tops: Vec<Option<[f64; 3]>> = f
                .strings
                .iter()
                .map(|x| {
                    f.points
                        .iter()
                        .find(|p| p.label == x.top)
                        .map(|p| p.at)
                        .or_else(|| f.wires.iter().find(|w| w.label == x.top).map(|w| w.at))
                })
                .collect();
            let lengths: Vec<Option<f64>> = f.strings.iter().map(|x| f.length(x)).collect();
            // The string that makes each wire point: the first to it.
            let owner = |f: &StringForm, wire: &str| f.strings.iter().position(|x| x.wire == wire);
            let mut remove = None;
            for (k, length) in lengths.iter().enumerate() {
                let owns = owner(f, &f.strings[k].wire) == Some(k);
                let others: Vec<String> = f
                    .wires
                    .iter()
                    .filter(|w| w.label != f.strings[k].wire)
                    .map(|w| w.label.clone())
                    .collect();
                let mut ends_at: Option<Option<String>> = None;
                let mut pick: Option<usize> = None;
                // A wire point renamed: (old, new) for its strings.
                let mut rename: Option<(String, String)> = None;
                // Names its wire point may not take (its own current one
                // aside).
                let wire_now = f.strings[k].wire.clone();
                let structure = model.line.structure(&f.structure);
                let names: Vec<String> = f
                    .points
                    .iter()
                    .map(|p| p.label.clone())
                    .chain(f.wires.iter().map(|w| w.label.clone()))
                    .chain(f.strings.iter().map(|x| x.label.clone()))
                    .chain(structure.into_iter().flat_map(|s| {
                        s.points
                            .iter()
                            .map(|p| p.label.clone())
                            .chain(s.wires.iter().map(|w| w.label.clone()))
                            .chain(s.strings.iter().map(|x| x.label.clone()))
                    }))
                    .filter(|l| *l != wire_now)
                    .collect();
                let clash = |text: &str, own: &str| text != own && names.iter().any(|l| l == text);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    let x = &mut f.strings[k];
                    ui.horizontal(|ui| {
                        ui.label("string");
                        ui.add(egui::TextEdit::singleline(&mut x.label).desired_width(110.0));
                        match length {
                            Some(l) => ui.label(format!("{l:.3} m long")),
                            None => ui.label("—"),
                        };
                        if ui
                            .small_button("×")
                            .on_hover_text("Take this string away, with the wire point it makes")
                            .clicked()
                        {
                            remove = Some(k);
                        }
                    });
                    if !library.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label("from the library");
                            egui::ComboBox::from_id_salt(("string-library", k))
                                .selected_text(x.insulator.as_deref().unwrap_or("(given here)"))
                                .height(300.0)
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(x.insulator.is_none(), "(given here)").clicked() {
                                        x.insulator = None;
                                    }
                                    for (n, a) in library.iter().enumerate() {
                                        let text = format!(
                                            "{} — {}, {:.2} m, {:.0} N{}",
                                            a.2.id,
                                            a.0,
                                            a.2.length,
                                            a.2.weight,
                                            if a.2.strain { ", tension" } else if a.1 { ", V leg" } else { "" }
                                        );
                                        if ui
                                            .selectable_label(x.insulator.as_deref() == Some(a.2.id.as_str()), text)
                                            .clicked()
                                        {
                                            pick = Some(n);
                                        }
                                    }
                                })
                                .response
                                .on_hover_text(
                                    "An insulator assembly of the library: its length, weight, wind \
                                     area and stiffness are taken; a tension assembly makes a dead end",
                                );
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("description");
                        ui.add(
                            egui::TextEdit::singleline(&mut x.description)
                                .desired_width(260.0)
                                .hint_text("what it is (free text)"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("hangs from")
                            .on_hover_text("The point of the structure it is attached to");
                        egui::ComboBox::from_id_salt(("string-top", k))
                            .selected_text(&x.top)
                            .height(300.0)
                            .show_ui(ui, |ui| {
                                ui.label(egui::RichText::new("points of the structure").small().weak());
                                for t in &point_names {
                                    ui.selectable_value(&mut x.top, t.clone(), t);
                                }
                                ui.separator();
                                ui.label(
                                    egui::RichText::new("the bottom of another string").small().weak(),
                                );
                                for w in &others {
                                    ui.selectable_value(&mut x.top, w.clone(), w);
                                }
                            });
                        ui.label("ends at").on_hover_text(
                            "Its own wire point (made by it), or another string's (a V's second leg)",
                        );
                        let shown = if owns {
                            format!("its own wire point ({})", x.wire)
                        } else {
                            format!("{}'s wire point", x.wire)
                        };
                        egui::ComboBox::from_id_salt(("string-ends", k))
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(owns, "its own wire point").clicked() && !owns {
                                    ends_at = Some(None);
                                }
                                for w in &others {
                                    if ui.selectable_label(false, format!("the wire point {w}")).clicked() {
                                        ends_at = Some(Some(w.clone()));
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("weight");
                        value(ui, &mut x.weight, 1.0, " N")
                            .on_hover_text("Its own weight, half at each end");
                        ui.label("wind area");
                        value(ui, &mut x.wind_area, 0.01, " m²").on_hover_text(
                            "Area the wind blows on (0: its insulator's, if any); half at each end",
                        );
                        let mut both = x.compression.is_some();
                        if ui
                            .checkbox(&mut both, "may push, up to")
                            .on_hover_text(
                                "It acts both ways (e.g. a V leg designed to push); beyond this it \
                                 is flagged. Unticked: tension only, slack when pushed",
                            )
                            .changed()
                        {
                            x.compression = both.then_some(1000.0);
                        }
                        if let Some(c) = &mut x.compression {
                            value(ui, c, 10.0, " N");
                            *c = c.max(0.0);
                        }
                    });
                    // The wire point it makes.
                    if owns
                        && let Some(top) = tops[k]
                        && let Some(w) = f.wires.iter_mut().find(|w| w.label == f.strings[k].wire)
                    {
                        // Shown in the project's convention (ahead at its
                        // angle, clockwise seen from above).
                        let (mut down, dir, mut len) = polar(top, w.at);
                        let mut dir = (dir + ahead).rem_euclid(360.0);
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            ui.label("down/up");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut down)
                                        .range(0.0..=180.0)
                                        .speed(0.5)
                                        .max_decimals(2)
                                        .suffix("°"),
                                )
                                .on_hover_text("Angle from straight down: 0° down, 90° level, 180° straight up")
                                .changed();
                            ui.label("direction");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut dir)
                                        .range(0.0..=360.0)
                                        .speed(0.5)
                                        .max_decimals(2)
                                        .suffix("°"),
                                )
                                .on_hover_text(
                                    format!(
                                        "Direction in plan, clockwise seen from above: ahead along the line {:.0}°, \
                                         to the right {:.0}°, back {:.0}°, to the left {:.0}° \
                                         (Structures › Angles around structures)",
                                        ahead,
                                        (ahead + 90.0) % 360.0,
                                        (ahead + 180.0) % 360.0,
                                        (ahead + 270.0) % 360.0
                                    ),
                                )
                                .changed();
                            ui.label("length");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut len)
                                        .range(0.01..=50.0)
                                        .speed(0.01)
                                        .max_decimals(3)
                                        .suffix(" m"),
                                )
                                .on_hover_text("Its length: from its top to its wire point")
                                .changed();
                        });
                        if changed {
                            w.at = from_polar(top, down, (dir - ahead).rem_euclid(360.0), len.max(0.01));
                        }
                        ui.horizontal(|ui| {
                            ui.label("wire point");
                            // Typed into a copy: its strings follow each
                            // valid name; one taken or empty waits (shown
                            // red) and is dropped when the field is left.
                            let id = ui.id().with(("wire-label", k));
                            let (own, mut text) = ui
                                .data_mut(|d| d.get_temp::<(String, String)>(id))
                                .unwrap_or_else(|| (w.label.clone(), w.label.clone()));
                            let bad = if text.trim().is_empty() {
                                Some("give it a name")
                            } else if clash(&text, &own) {
                                Some("taken: another point, wire point or string has this name")
                            } else {
                                None
                            };
                            let mut edit = egui::TextEdit::singleline(&mut text).desired_width(90.0);
                            if bad.is_some() {
                                edit = edit.text_color(egui::Color32::LIGHT_RED);
                            }
                            let r = ui.add(edit);
                            if let Some(why) = bad {
                                r.clone().on_hover_text(why);
                            }
                            if r.changed() {
                                let ok = !text.trim().is_empty() && !clash(&text, &own);
                                if ok && text != w.label {
                                    rename = Some((w.label.clone(), text.clone()));
                                    w.label = text.clone();
                                }
                                ui.data_mut(|d| d.insert_temp(id, (own, text)));
                            } else if !r.has_focus() {
                                ui.data_mut(|d| d.remove::<(String, String)>(id));
                            }
                            let mut has = w.set.is_some();
                            if ui
                                .checkbox(&mut has, "set")
                                .on_hover_text(
                                    "A wire is clamped here: sections and jumpers end on its set number. \
                                     Without: other strings may hang from it (a V's tip, a TAK's node)",
                                )
                                .changed()
                            {
                                w.set = has.then_some(f.set.max(1));
                            }
                            if let Some(set) = &mut w.set {
                                ui.add(egui::DragValue::new(set).range(1..=9999));
                                ui.label("clamp");
                                egui::ComboBox::from_id_salt(("wire-dead-end", k))
                                    .selected_text(if w.dead_end { "strain" } else { "suspension" })
                                    .width(90.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut w.dead_end, false, "suspension");
                                        ui.selectable_value(&mut w.dead_end, true, "strain");
                                    })
                                    .response
                                    .on_hover_text(
                                        "Suspension: the wire runs through, its strings hang. Strain: \
                                         a dead end, its strings are tension strings lined up with \
                                         their conductor",
                                    );
                            }
                            ui.label("counterweight");
                            value(ui, &mut w.weight, 10.0, " N")
                                .on_hover_text("A weight hanging at the clamp (0: none)");
                            w.weight = w.weight.max(0.0);
                        });
                        ui.horizontal(|ui| {
                            ui.label("wire point description");
                            ui.add(
                                egui::TextEdit::singleline(&mut w.description)
                                    .desired_width(220.0)
                                    .hint_text("what it is (free text)"),
                            );
                        });
                    }
                });
                // Its strings follow a wire point renamed.
                if let Some((old, new)) = rename {
                    for x in &mut f.strings {
                        if x.wire == old {
                            x.wire.clone_from(&new);
                        }
                        if x.top == old {
                            x.top.clone_from(&new);
                        }
                    }
                }
                // The library's values (and the wire point at its length).
                if let Some(n) = pick {
                    let a = library[n].2.clone();
                    f.take_from_library(k, &a);
                }
                match ends_at {
                    Some(Some(w)) => {
                        f.strings[k].wire = w;
                        f.drop_unused_wires();
                    }
                    Some(None) => {
                        let label = f.free_label(model, "W");
                        let top = tops[k].unwrap_or([0.0, 0.0, 20.0]);
                        f.wires.push(WireRow { description: Default::default(),
                            label: label.clone(),
                            at: from_polar(top, 0.0, 0.0, lengths[k].unwrap_or(3.0)),
                            set: None,
                            weight: 0.0,
                            dead_end: false,
                            dead_end_was: false,
                        });
                        f.strings[k].wire = label;
                    }
                    None => {}
                }
            }
            if let Some(k) = remove {
                f.strings.remove(k);
                f.drop_unused_wires();
            }
            // Wire points no string reaches: the wire clamped straight to
            // the structure.
            for w in f.wires.iter_mut() {
                if f.strings.iter().any(|x| x.wire == w.label) {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.label(format!("wire point {} — clamped straight to the structure", w.label));
                    for (i, name) in ["across", "along", "up"].iter().enumerate() {
                        ui.label(*name);
                        value(ui, &mut w.at[i], 0.05, " m");
                    }
                });
            }
            if ui
                .small_button("+ string")
                .on_hover_text("Another string with its own wire point, 3 m straight down (to set)")
                .clicked()
                && let Some(top) = point_names.first().cloned()
            {
                let label = f.free_label(model, "S");
                let wire = f.free_label(model, "W");
                let at = f
                    .points
                    .iter()
                    .find(|p| p.label == top)
                    .map_or([0.0, 0.0, 20.0], |p| p.at);
                f.wires.push(WireRow { description: Default::default(),
                    label: wire.clone(),
                    at: from_polar(at, 0.0, 0.0, 3.0),
                    set: None,
                    weight: 0.0,
                    dead_end: false,
                    dead_end_was: false,
                });
                f.strings.push(StringRow {
                    label,
                    top,
                    wire,
                    ..Default::default()
                });
            }
            ui.separator();
            ui.strong("New points on the structure");
            help(
                ui,
                "A string hangs from a point of the structure; the structure's points are listed \
                 under \"hangs from\". Add one here where none fits (across: to the right looking \
                 ahead, along: ahead, up: from the base).",
            );
            let mut drop = None;
            for &k in &new_points {
                let p = &mut f.points[k];
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.label).desired_width(90.0));
                    for (i, name) in ["across", "along", "up"].iter().enumerate() {
                        ui.label(*name);
                        value(ui, &mut p.at[i], 0.05, " m");
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut p.description)
                            .desired_width(140.0)
                            .hint_text("description"),
                    );
                    if ui.small_button("×").clicked() {
                        drop = Some(k);
                    }
                });
            }
            if let Some(k) = drop {
                let label = f.points[k].label.clone();
                if !f.strings.iter().any(|x| x.top == label) {
                    f.points.remove(k);
                }
            }
            if ui.small_button("+ point").clicked() {
                let label = f.free_label(model, "A");
                let at = f.points.last().map_or([0.0, 0.0, 20.0], |p| p.at);
                f.points.push(PointRow { description: Default::default(), label, at });
            }
        });
    if f.overrides_type {
        ui.label(
            egui::RichText::new("These strings are the structure's own, over its type's.").small(),
        );
        reset = ui.button("Back to the type's strings").clicked();
    }
    reset
}

/// Returns true when "pick points again" is clicked.
/// A sagging condition in words.
pub fn describe(s: &Stringing) -> String {
    match s {
        Stringing::Tension {
            horizontal_tension,
            temperature,
        } => format!("{:.1} kN at {temperature} °C", horizontal_tension / 1000.0),
        Stringing::Sag {
            sag,
            temperature,
            span,
        } => format!(
            "a sag of {sag:.2} m in {} at {temperature} °C",
            span.map_or("the ruling span".into(), |k| format!("span {}", k + 1))
        ),
        Stringing::UnstressedLengths { lengths, .. } => {
            format!("{} unstressed lengths", lengths.len())
        }
    }
}

fn section_ui(ui: &mut egui::Ui, f: &mut SectionForm, model: &Model, case: usize) -> bool {
    let line = &model.line;
    egui::Grid::new("section-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("id");
            ui.add_enabled(
                f.new,
                egui::TextEdit::singleline(&mut f.id).desired_width(140.0),
            );
            ui.end_row();
            ui.label("description");
            ui.add(
                egui::TextEdit::singleline(&mut f.description)
                    .desired_width(240.0)
                    .hint_text("what it is (free text)"),
            );
            ui.end_row();
            ui.label("cable");
            egui::ComboBox::from_id_salt("section-cable")
                .selected_text(&f.cable)
                .width(240.0)
                .height(400.0)
                .show_ui(ui, |ui| {
                    for c in model.library.iter() {
                        ui.selectable_value(&mut f.cable, c.id.clone(), &c.id)
                            .on_hover_text(&c.name);
                    }
                });
            ui.end_row();
            ui.label("circuit");
            ui.add(egui::TextEdit::singleline(&mut f.circuit).desired_width(200.0));
            ui.end_row();
            ui.label("phase");
            ui.add(egui::TextEdit::singleline(&mut f.phase_label).desired_width(60.0));
            ui.end_row();
            ui.label("calculation");
            egui::ComboBox::from_id_salt("section-calculation")
                .selected_text(f.calculation.label())
                .show_ui(ui, |ui| {
                    for c in ols_line::condition::Calculation::ALL {
                        ui.selectable_value(&mut f.calculation, c, c.label());
                    }
                })
                .response
                .on_hover_text(
                    "Cable condition (Initial, after Creep, after Load, Max Sag of the two) and \
                     method (RS: ruling span; FE: finite elements)",
                );
            ui.end_row();
            ui.label("system");
            egui::ComboBox::from_id_salt("section-system")
                .selected_text(
                    f.system
                        .as_deref()
                        .unwrap_or("(from the jumpers, or earthed)"),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.system, None, "(from the jumpers, or earthed)");
                    for s in &line.systems {
                        ui.selectable_value(&mut f.system, Some(s.id.clone()), &s.id);
                    }
                });
            ui.end_row();
            ui.label("bundle");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut f.bundle)
                        .range(1..=8)
                        .suffix(" wires"),
                );
                if f.bundle > 1 {
                    value(ui, &mut f.spacing, 0.01, " m apart");
                    value(ui, &mut f.rotation, 1.0, "°");
                }
            });
            ui.end_row();
        });
    ui.separator();
    // Status: in the blocks (rolling) or clipped in.
    let mut clipped = f.stringing.clipped();
    ui.horizontal(|ui| {
        ui.label("status");
        ui.radio_value(&mut clipped, false, "in the blocks")
            .on_hover_text("Unclipped: the wire rolls over the suspension clamps — one horizontal tension in all spans in every weather case; moving a structure sags it again to its condition");
        ui.radio_value(&mut clipped, true, "clipped in")
            .on_hover_text("Each span keeps its unstressed length: a structure moved changes the spans next to it, the suspension strings swing to balance them");
    });
    if clipped != f.stringing.clipped() {
        if clipped {
            if let Err(e) = f.clip_in(model) {
                f.note = Some(format!("cannot clip in: {e}"));
            } else {
                f.note = None;
            }
        } else {
            f.stringing = f.unclipped(model, case);
            f.note = None;
        }
    }
    let spans = f.points.len().saturating_sub(1);
    match &mut f.stringing {
        Stringing::Tension { .. } | Stringing::Sag { .. } => {
            // The sagging condition: a tension, or a sag.
            let temperature = f.stringing.temperature();
            let mut by_sag = matches!(f.stringing, Stringing::Sag { .. });
            ui.horizontal(|ui| {
                ui.label("sagged to");
                ui.radio_value(&mut by_sag, false, "a tension");
                ui.radio_value(&mut by_sag, true, "a sag");
            });
            if by_sag != matches!(f.stringing, Stringing::Sag { .. }) {
                f.stringing = if by_sag {
                    Stringing::Sag {
                        sag: 10.0,
                        temperature,
                        span: None,
                    }
                } else {
                    Stringing::Tension {
                        horizontal_tension: 20_000.0,
                        temperature,
                    }
                };
            }
            match &mut f.stringing {
                Stringing::Tension {
                    horizontal_tension,
                    temperature,
                } => {
                    let mut kn = *horizontal_tension / 1000.0;
                    ui.horizontal(|ui| {
                        ui.label("horizontal tension");
                        if value(ui, &mut kn, 0.1, " kN").changed() {
                            *horizontal_tension = kn * 1000.0;
                        }
                        ui.label("at");
                        value(ui, temperature, 0.5, " °C");
                    });
                }
                Stringing::Sag {
                    sag,
                    temperature,
                    span,
                } => {
                    ui.horizontal(|ui| {
                        ui.label("sag");
                        value(ui, sag, 0.01, " m");
                        ui.label("at");
                        value(ui, temperature, 0.5, " °C");
                    });
                    ui.horizontal(|ui| {
                        ui.label("in");
                        egui::ComboBox::from_id_salt("sag-span")
                            .selected_text(
                                span.map_or("the ruling span".into(), |k| {
                                    format!("span {}", k + 1)
                                }),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(span, None, "the ruling span");
                                for k in 0..spans {
                                    ui.selectable_value(span, Some(k), format!("span {}", k + 1));
                                }
                            });
                    });
                }
                Stringing::UnstressedLengths { .. } => {}
            }
            ui.label(
                egui::RichText::new(
                    "Still air, strings plumb; the same horizontal tension in every span.",
                )
                .small(),
            );
        }
        Stringing::UnstressedLengths {
            lengths,
            temperature,
            sagged,
        } => {
            ui.horizontal(|ui| {
                ui.label("unstressed lengths at");
                value(ui, temperature, 0.5, " °C");
            });
            egui::Grid::new("section-lengths").show(ui, |ui| {
                for (k, l) in lengths.iter_mut().enumerate() {
                    ui.label(format!("span {}", k + 1));
                    value(ui, l, 0.001, " m");
                    ui.end_row();
                }
            });
            if let Some(c) = sagged {
                ui.label(
                    egui::RichText::new(format!("clipped in after sagging to {}", describe(c)))
                        .small(),
                );
            }
        }
    }
    if let Some(n) = &f.note {
        ui.colored_label(egui::Color32::YELLOW, n);
    }
    ui.separator();
    ui.label(format!("{} points:", f.points.len()));
    let text: Vec<String> = f
        .points
        .iter()
        .map(|p| format!("{} set {}", p.structure, p.set))
        .collect();
    ui.label(egui::RichText::new(text.join(" → ")).small());
    ui.button("Pick points again")
        .on_hover_text("With the String wire tool: right-click the strings in order, Enter")
        .clicked()
}

fn alignment_ui(ui: &mut egui::Ui, f: &mut AlignmentForm, model: &Model) {
    let stored = model.stored_line().unwrap_or(&model.line);
    let routes = &stored.alignments;
    egui::Grid::new("alignment-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("alignment");
            egui::ComboBox::from_id_salt("alignment-route")
                .selected_text(f.target.as_str())
                .show_ui(ui, |ui| {
                    for r in routes {
                        ui.selectable_value(&mut f.target, r.id.clone(), &r.id);
                    }
                })
                .response
                .on_hover_text("The alignment this point belongs to");
            ui.end_row();
            ui.label("description");
            ui.add(
                egui::TextEdit::singleline(&mut f.description)
                    .desired_width(240.0)
                    .hint_text("what it is (free text)"),
            );
            ui.end_row();

            ui.label("order");
            let n = routes
                .iter()
                .find(|r| r.id == f.target)
                .map_or(1, |r| r.points.len() + usize::from(f.target != f.route));
            ui.add(egui::DragValue::new(&mut f.order).range(1..=n.max(1)))
                .on_hover_text("Its place along the alignment: 1 is the first point");
            ui.end_row();
            ui.label("tied to");
            egui::ComboBox::from_id_salt("alignment-structure")
                .selected_text(f.structure.as_deref().unwrap_or("(free)"))
                .height(400.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.structure, None, "(free)");
                    for s in &model.line.structures {
                        ui.selectable_value(&mut f.structure, Some(s.id.clone()), &s.id);
                    }
                });
            ui.end_row();
            let free = f.structure.is_none();
            for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
                ui.label(name);
                ui.add_enabled_ui(free && (k < 2 || !f.on_ground), |ui| {
                    value(ui, &mut f.position[k], 0.1, " m");
                });
                ui.end_row();
            }
        });
    if f.structure.is_none() {
        ui.checkbox(&mut f.on_ground, "on the ground");
        if f.on_ground
            && let Some(z) = model.ground_at(f.position[0], f.position[1])
        {
            f.position[2] = z;
        }
    } else {
        ui.label(
            egui::RichText::new("A tied point sits under its structure and follows it.").small(),
        );
    }
    ui.label(
        egui::RichText::new(
            "The profile view follows the alignment points. Moved, a point shared with \
             another alignment (a junction, at the same place) moves with it.",
        )
        .small(),
    );
}

/// A new jumper from `from` to `to`: the cable of the section at `from`,
/// a sag of 15 % of the distance (0.3–3 m) at 15 °C, the next free id.
pub fn new_jumper(
    model: &Model,
    from: &SectionPoint,
    to: &SectionPoint,
    taken: &[String],
) -> Result<ols_line::Jumper, String> {
    let line = &model.line;
    if from == to {
        return Err("a jumper joins two different strings".into());
    }
    let (a, b) = (
        line.wire_point(from)
            .ok_or_else(|| format!("no wire point {} set {}", from.structure, from.set))?,
        line.wire_point(to)
            .ok_or_else(|| format!("no wire point {} set {}", to.structure, to.set))?,
    );
    let d = [0, 1, 2].map(|k| b.position[k] - a.position[k]);
    let chord = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let cable = line
        .sections
        .iter()
        .find(|s| s.points.contains(from))
        .or_else(|| line.sections.iter().find(|s| s.points.contains(to)))
        .map(|s| s.cable.clone())
        .or_else(|| model.library.iter().next().map(|c| c.id.clone()))
        .ok_or("no cable for the jumper")?;
    let like = line.jumpers.last().map_or("J0", |j| j.id.as_str());
    let id = next_id(
        line.jumpers
            .iter()
            .map(|j| j.id.as_str())
            .chain(taken.iter().map(String::as_str)),
        like,
    );
    serde_json::from_value(serde_json::json!({
        "id": id,
        "cable": cable,
        "from": from,
        "to": to,
        "sag": ((0.15 * chord).clamp(0.3, 3.0) * 100.0).round() / 100.0,
        "temperature": 15.0,
    }))
    .map_err(|e| e.to_string())
}

/// The jumpers missing at structure `id`: between the ends of two
/// sections there (their first or last point) of the same circuit and
/// phase, where no jumper is yet. Also what could not be paired.
pub fn missing_jumpers(model: &Model, id: &str) -> (Vec<ols_line::Jumper>, Vec<String>) {
    let line = &model.line;
    let jumpered = |p: &SectionPoint| line.jumpers.iter().any(|j| &j.from == p || &j.to == p);
    // (circuit, phase) → the free section ends at this structure.
    let mut ends: std::collections::BTreeMap<(String, String), Vec<(String, SectionPoint)>> =
        std::collections::BTreeMap::new();
    for s in &line.sections {
        let (Some(first), Some(last)) = (s.points.first(), s.points.last()) else {
            continue;
        };
        for p in [first, last] {
            if p.structure == id && !jumpered(p) {
                ends.entry((s.circuit.clone(), s.phase_label.clone()))
                    .or_default()
                    .push((s.id.clone(), p.clone()));
            }
        }
    }
    let mut out = Vec::new();
    let mut unpaired = Vec::new();
    for ((circuit, phase), list) in ends {
        let mut list = list;
        list.dedup_by(|a, b| a.1 == b.1);
        if list.len() == 2 && list[0].0 != list[1].0 {
            let taken: Vec<String> = out
                .iter()
                .map(|j: &ols_line::Jumper| j.id.clone())
                .collect();
            match new_jumper(model, &list[0].1, &list[1].1, &taken) {
                Ok(j) => out.push(j),
                Err(e) => unpaired.push(e),
            }
        } else if list.len() > 1 {
            unpaired.push(format!(
                "{circuit} {phase}: {} section ends — join them string by string",
                list.len()
            ));
        }
    }
    (out, unpaired)
}
