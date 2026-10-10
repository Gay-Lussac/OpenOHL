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
}

/// A point of a structure in an editor, in the structure's axes (across:
/// to the right looking ahead, along: ahead, up from the base).
#[derive(Debug, Clone, PartialEq)]
pub struct PointRow {
    /// Label.
    pub label: String,
    /// Position, local.
    pub at: [f64; 3],
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
    /// Route as the views name it (`main` for the line's own alignment).
    pub route: String,
    /// Index in the route's alignment.
    pub index: usize,
    /// The alignment it goes to (as the views name it; `route`: stays).
    pub target: String,
    /// Its place in that alignment, from 1.
    pub order: usize,
    /// Position.
    pub position: [f64; 3],
    /// Tied under this structure (it follows it).
    pub structure: Option<String>,
    /// The point sits on the ground.
    pub on_ground: bool,
}

/// The line's routes with their alignments, every one named: a line
/// without routes gives its own alignment as route `main` (all its
/// structures, as the views show it).
pub fn explicit_routes(stored: &Line) -> Vec<ols_line::Route> {
    if stored.routes.is_empty() {
        let mut main = stored.effective_routes().remove(0);
        main.id = "main".into();
        vec![main]
    } else {
        stored.routes.clone()
    }
}

/// The command writing `routes` (from [`explicit_routes`], edited) where
/// they differ from the stored line: a line that had no routes keeps its
/// own alignment while `main` is its only one; with another, its routes
/// are written and its own alignment emptied. One command (a batch when
/// several).
pub fn routes_command(stored: &Line, routes: &[ols_line::Route]) -> Result<Command, String> {
    let put = |r: &ols_line::Route, ord: Option<i64>| -> Result<Command, String> {
        Ok(Command::Put {
            kind: Kind::Route,
            id: r.id.clone(),
            value: serde_json::to_value(r).map_err(|e| e.to_string())?,
            ord,
        })
    };
    let mut commands = Vec::new();
    if stored.routes.is_empty() {
        if let [main] = routes
            && main.id == "main"
        {
            return Ok(ols_session::line_settings_put(
                stored,
                Some(main.alignment.clone()),
                None,
            ));
        }
        commands.push(ols_session::line_settings_put(
            stored,
            Some(Vec::new()),
            None,
        ));
        for (k, r) in routes.iter().enumerate() {
            commands.push(put(r, Some(k as i64))?);
        }
    } else {
        for r in routes {
            if stored.routes.iter().find(|x| x.id == r.id) != Some(r) {
                commands.push(put(r, None)?);
            }
        }
    }
    Ok(match commands.len() {
        1 => commands.remove(0),
        _ => Command::Batch { commands },
    })
}

/// Points of the other alignments at `at` in plan (within 1 mm): the
/// junctions that move with it. (route index, point index).
fn junctions(
    routes: &[ols_line::Route],
    at: [f64; 3],
    skip: (usize, usize),
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (r, route) in routes.iter().enumerate() {
        for (k, p) in route.alignment.iter().enumerate() {
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

/// A new alignment through `points`, in order: a point taken from another
/// alignment is a junction (its position, its structure if tied). The
/// command, what it does, and the new route's id.
pub fn new_alignment(
    model: &Model,
    points: Vec<ols_line::AlignmentPoint>,
) -> Result<(String, Command, String), String> {
    let stored = model
        .stored_line()
        .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
    if points.len() < 2 {
        return Err("an alignment needs at least two points".into());
    }
    let mut routes = explicit_routes(stored);
    let id = (2..)
        .map(|n| format!("alignment {n}"))
        .find(|id| !routes.iter().any(|r| &r.id == id))
        .expect("a free name");
    routes.push(ols_line::Route {
        id: id.clone(),
        name: id.clone(),
        alignment: points,
        structures: Vec::new(),
    });
    Ok((
        format!("new alignment {id}"),
        routes_command(stored, &routes)?,
        id,
    ))
}

/// The session's name of a route's alignment: `"line"` for the line's own
/// (a line without routes; the views call it `main`).
pub fn route_key(line: &Line, route: &str) -> String {
    if line.routes.is_empty() {
        "line".into()
    } else {
        route.to_owned()
    }
}

/// A route's stored alignment (by its session name).
pub fn stored_alignment(stored: &Line, key: &str) -> Option<Vec<ols_line::AlignmentPoint>> {
    if key == "line" {
        Some(stored.alignment.clone())
    } else {
        stored
            .routes
            .iter()
            .find(|r| r.id == key)
            .map(|r| r.alignment.clone())
    }
}

/// The put replacing a route's alignment (the line settings for `"line"`).
pub fn alignment_put(
    stored: &Line,
    key: &str,
    alignment: Vec<ols_line::AlignmentPoint>,
) -> Result<Command, String> {
    if key == "line" {
        return Ok(ols_session::line_settings_put(
            stored,
            Some(alignment),
            None,
        ));
    }
    let mut r = stored
        .routes
        .iter()
        .find(|r| r.id == key)
        .ok_or_else(|| format!("no route {key}"))?
        .clone();
    r.alignment = alignment;
    Ok(Command::Put {
        kind: Kind::Route,
        id: key.to_owned(),
        value: serde_json::to_value(&r).map_err(|e| e.to_string())?,
        ord: None,
    })
}

impl AlignmentForm {
    /// Point `index` of route `route` (as the views name it).
    pub fn of(model: &Model, route: &str, index: usize) -> Option<Self> {
        let stored = model.stored_line().unwrap_or(&model.line);
        let p = stored_alignment(stored, &route_key(&model.line, route))?
            .get(index)?
            .clone();
        Some(Self {
            route: route.to_owned(),
            index,
            target: route.to_owned(),
            order: index + 1,
            position: p.position,
            structure: p.structure,
            on_ground: false,
        })
    }

    fn command(&self, model: &Model) -> Result<(String, Command), String> {
        let stored = model
            .stored_line()
            .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
        let mut routes = explicit_routes(stored);
        let from = routes
            .iter()
            .position(|r| r.id == self.route)
            .ok_or_else(|| format!("no alignment {}", self.route))?;
        let to = routes
            .iter()
            .position(|r| r.id == self.target)
            .ok_or_else(|| format!("no alignment {}", self.target))?;
        let old = routes[from]
            .alignment
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
            for (r, k) in junctions(&routes, old.position, (from, self.index)) {
                let p = &mut routes[r].alignment[k];
                p.position = position;
                p.structure = self.structure.clone();
            }
        }
        routes[from].alignment.remove(self.index);
        let point = ols_line::AlignmentPoint {
            position,
            structure: self.structure.clone(),
        };
        let at = self.order.max(1) - 1;
        let list = &mut routes[to].alignment;
        list.insert(at.min(list.len()), point);
        let what = if from == to && at == self.index {
            format!(
                "changed alignment point {} of {}",
                self.index + 1,
                self.route
            )
        } else {
            format!(
                "alignment point {} of {} now point {} of {}",
                self.index + 1,
                self.route,
                at.min(routes[to].alignment.len() - 1) + 1,
                self.target
            )
        };
        Ok((what, routes_command(stored, &routes)?))
    }
}

/// A new alignment point at `at`: in the nearest segment of the nearest
/// alignment (a route following its structures first gets points under
/// them). The command, and the point (route as the views name it, index).
pub fn insert_alignment_point(
    model: &Model,
    at: [f64; 3],
) -> Result<(String, Command, String, usize), String> {
    let stored = model
        .stored_line()
        .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
    let mut best: Option<(f64, String, Vec<ols_line::AlignmentPoint>, usize)> = None;
    for r in model.line.effective_routes() {
        let key = route_key(&model.line, &r.id);
        let mut points = stored_alignment(stored, &key).unwrap_or_default();
        if points.len() < 2 {
            points = model.line.alignment_under_structures(&r);
        }
        if points.is_empty() {
            continue;
        }
        // Distance in plan to each segment, and where it goes.
        let mut here = (f64::INFINITY, points.len());
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
            best = Some((here.0, r.id.clone(), points, here.1));
        }
    }
    let (_, route, mut points, index) = best.ok_or("no route to add a point to")?;
    let key = route_key(&model.line, &route);
    points.insert(
        index.min(points.len()),
        ols_line::AlignmentPoint {
            position: at,
            structure: None,
        },
    );
    Ok((
        format!("added alignment point {} to {route}", index + 1),
        alignment_put(stored, &key, points)?,
        route,
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
        Self {
            new: true,
            id,
            structure_type,
            extension: None,
            height_adjust: 0.0,
            orientation_deg: nearest.map_or(0.0, |s| s.orientation_deg),
            position: at,
            on_ground: model.ground_at(at[0], at[1]).is_some(),
        }
    }

    /// Structure `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        Some(Self {
            new: false,
            id: id.to_owned(),
            structure_type: s.structure_type.clone(),
            extension: s.extension.clone(),
            height_adjust: s.height_adjust,
            orientation_deg: s.orientation_deg,
            position: s.position,
            on_ground: false,
        })
    }

    pub(crate) fn command(&self, stored: Option<&Line>) -> Result<(String, Command), String> {
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
            s
        };
        let what = if self.new {
            format!("added structure {id}")
        } else {
            format!("changed structure {id}")
        };
        Ok((
            what,
            Command::Put {
                kind: Kind::Structure,
                id,
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
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
                    label: w.label.clone(),
                    at: local(w.position),
                    set: w.set,
                    weight: w.weight,
                    dead_end: s.strings_to(&w.label).any(|x| x.strain),
                }
            })
            .collect();
        let strings: Vec<StringRow> = c
            .strings
            .iter()
            .map(|&i| {
                let x = &s.strings[i];
                StringRow {
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
                label: label.clone(),
                at: at(d),
            });
            label
        };
        let wire = |form: &mut Self, stem: &str, d: [f64; 3], set: Option<u32>| {
            let label = form.free_label(model, stem);
            form.wires.push(WireRow {
                label: label.clone(),
                at: at(d),
                set,
                weight: 0.0,
                dead_end: false,
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
                Some(w) if w.set.is_some() => w.dead_end,
                _ => x.strain,
            };
            s.strings.retain(|y| y.label != x.label);
            s.strings.push(InsulatorString {
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
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model, case: usize) -> Option<Outcome> {
        let mut out = None;
        match self {
            Form::Structure(f) => structure_ui(ui, f, model),
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

fn structure_ui(ui: &mut egui::Ui, f: &mut StructureForm, model: &Model) {
    egui::Grid::new("structure-form").num_columns(2).show(ui, |ui| {
        ui.label("id");
        ui.add_enabled(f.new, egui::TextEdit::singleline(&mut f.id).desired_width(140.0));
        ui.end_row();
        ui.label("type");
        let types: Vec<&ols_structures::StructureType> = model
            .types
            .as_ref()
            .map(|(s, _)| s.iter().collect())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("structure-type")
            .selected_text(f.structure_type.as_deref().unwrap_or("(none: strings given)"))
            .height(400.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.structure_type, None, "(none: strings given)");
                for t in &types {
                    ui.selectable_value(&mut f.structure_type, Some(t.id.clone()), &t.id);
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
        ui.label("orientation");
        value(ui, &mut f.orientation_deg, 0.1, "°").on_hover_text(
            "Direction of the cross-arms (local x, to the right looking ahead), counter-clockwise from east",
        );
        ui.end_row();
        for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
            ui.label(name);
            ui.add_enabled_ui(k < 2 || !f.on_ground, |ui| {
                value(ui, &mut f.position[k], 0.1, " m");
            });
            ui.end_row();
        }
    });
    ui.checkbox(&mut f.on_ground, "base on the ground");
    if f.on_ground
        && let Some(z) = model.ground_at(f.position[0], f.position[1])
    {
        f.position[2] = z;
    }
    if f.new && f.structure_type.is_none() {
        ui.label(
            egui::RichText::new("Without a type, add its strings with the Add string tool.")
                .small(),
        );
    }
}

/// Returns true when "back to the type" is clicked.
fn string_ui(ui: &mut egui::Ui, f: &mut StringForm, model: &Model) -> bool {
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
    let insulators: Vec<String> = model
        .types
        .as_ref()
        .map(|(_, i)| i.iter().map(|a| a.id.clone()).collect())
        .unwrap_or_default();
    egui::ScrollArea::vertical()
        .max_height(460.0)
        .show(ui, |ui| {
            ui.strong("Strings");
            help(
                ui,
                "A string is an insulator chain: from the point it hangs from (on the structure, \
                 or the bottom of another string) to its wire point.",
            );
            let wires: Vec<String> = f.wires.iter().map(|w| w.label.clone()).collect();
            let lengths: Vec<Option<f64>> = f.strings.iter().map(|x| f.length(x)).collect();
            let mut remove = None;
            for (k, length) in lengths.iter().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    let x = &mut f.strings[k];
                    ui.horizontal(|ui| {
                        ui.label("string");
                        ui.add(egui::TextEdit::singleline(&mut x.label).desired_width(110.0));
                        match length {
                            Some(l) => ui.label(format!("{l:.3} m long")),
                            None => ui.label("—"),
                        };
                        if ui.small_button("×").on_hover_text("Take this string away").clicked() {
                            remove = Some(k);
                        }
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
                                for w in wires.iter().filter(|w| **w != x.wire) {
                                    ui.selectable_value(&mut x.top, w.clone(), w);
                                }
                            });
                        ui.label("to wire point").on_hover_text(
                            "Its bottom: where the wire is clamped, or where other strings meet",
                        );
                        egui::ComboBox::from_id_salt(("string-wire", k))
                            .selected_text(&x.wire)
                            .show_ui(ui, |ui| {
                                for w in &wires {
                                    ui.selectable_value(&mut x.wire, w.clone(), w);
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
                        if !insulators.is_empty() || x.insulator.is_some() {
                            egui::ComboBox::from_id_salt(("string-insulator", k))
                                .selected_text(x.insulator.as_deref().unwrap_or("(no insulator)"))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut x.insulator, None, "(no insulator)");
                                    for i in &insulators {
                                        ui.selectable_value(&mut x.insulator, Some(i.clone()), i);
                                    }
                                })
                                .response
                                .on_hover_text("An assembly of the insulator library (its weight and wind area)");
                        }
                    });
                    ui.horizontal(|ui| {
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
                });
            }
            if let Some(k) = remove {
                f.strings.remove(k);
            }
            if ui
                .small_button("+ string")
                .on_hover_text("Another string, e.g. the second leg of a V")
                .clicked()
                && let (Some(top), Some(wire)) = (point_names.first(), f.wires.first())
            {
                let label = f.free_label(model, "S");
                let (top, wire) = (top.clone(), wire.label.clone());
                f.strings.push(StringRow {
                    label,
                    top,
                    wire,
                    ..Default::default()
                });
            }
            ui.separator();
            ui.strong("Wire points");
            help(
                ui,
                "Where strings end. With a set number a wire is clamped there (sections and \
                 jumpers end on it); without one, other strings hang from it (a V's tip) or a \
                 jumper's helping string holds the jumper there. Placed from the top of the \
                 first string to it: an angle from straight down, a direction and a length — \
                 where it is before the wire is strung; once strung it is drawn where the \
                 calculation puts it.",
            );
            let top_of = |f: &StringForm, wire: &str| -> Option<(String, [f64; 3])> {
                let x = f.strings.iter().find(|x| x.wire == wire)?;
                let at = f
                    .points
                    .iter()
                    .find(|p| p.label == x.top)
                    .map(|p| p.at)
                    .or_else(|| f.wires.iter().find(|w| w.label == x.top).map(|w| w.at))?;
                Some((x.top.clone(), at))
            };
            let mut remove = None;
            for k in 0..f.wires.len() {
                let top = top_of(f, &f.wires[k].label);
                let default_set = f.set.max(1);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    let w = &mut f.wires[k];
                    ui.horizontal(|ui| {
                        ui.label("wire point");
                        ui.add(egui::TextEdit::singleline(&mut w.label).desired_width(110.0));
                        let mut has = w.set.is_some();
                        if ui
                            .checkbox(&mut has, "set")
                            .on_hover_text("A wire is clamped here: sections and jumpers end on its set number")
                            .changed()
                        {
                            w.set = has.then_some(default_set);
                        }
                        if let Some(set) = &mut w.set {
                            ui.add(egui::DragValue::new(set).range(1..=9999));
                            ui.label("dead end");
                            egui::ComboBox::from_id_salt(("wire-dead-end", k))
                                .selected_text(if w.dead_end { "yes" } else { "no" })
                                .width(50.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut w.dead_end, true, "yes");
                                    ui.selectable_value(&mut w.dead_end, false, "no");
                                })
                                .response
                                .on_hover_text(
                                    "Yes: a dead end, its strings are tension strings lined up \
                                     with their conductor. No: a suspension point, its strings hang",
                                );
                        }
                        if ui.small_button("×").on_hover_text("Take this wire point away (and its strings)").clicked() {
                            remove = Some(k);
                        }
                    });
                    match &top {
                        Some((label, at)) => {
                            let (mut down, mut dir, mut len) = polar(*at, w.at);
                            let mut changed = false;
                            ui.horizontal(|ui| {
                                ui.label(format!("from {label}:"));
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
                                        "Direction in plan, in the structure's axes: 0° ahead along \
                                         the line, 90° to the right, 180° back, 270° to the left",
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
                                    .on_hover_text("Distance from the top: the string's length")
                                    .changed();
                            });
                            if changed {
                                w.at = from_polar(*at, down, dir.rem_euclid(360.0), len.max(0.01));
                            }
                        }
                        None => {
                            ui.horizontal(|ui| {
                                for (i, name) in ["across", "along", "up"].iter().enumerate() {
                                    ui.label(*name);
                                    value(ui, &mut w.at[i], 0.05, " m");
                                }
                            });
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label("counterweight");
                        value(ui, &mut w.weight, 10.0, " N")
                            .on_hover_text("A weight hanging at the clamp (0: none)");
                        w.weight = w.weight.max(0.0);
                    });
                });
            }
            if let Some(k) = remove {
                let label = f.wires.remove(k).label;
                f.strings.retain(|x| x.wire != label && x.top != label);
            }
            if ui
                .small_button("+ wire point")
                .on_hover_text("A wire point for a string to end on (a V's tip, a TAK's node…)")
                .clicked()
            {
                let label = f.free_label(model, "W");
                let at = f.wires.last().map_or([0.0, 0.0, 17.0], |w| w.at);
                f.wires.push(WireRow {
                    label,
                    at: [at[0], at[1], at[2] - 1.0],
                    set: None,
                    weight: 0.0,
                    dead_end: false,
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
                f.points.push(PointRow { label, at });
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
    let routes = explicit_routes(model.stored_line().unwrap_or(&model.line));
    egui::Grid::new("alignment-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("alignment");
            egui::ComboBox::from_id_salt("alignment-route")
                .selected_text(f.target.as_str())
                .show_ui(ui, |ui| {
                    for r in &routes {
                        ui.selectable_value(&mut f.target, r.id.clone(), &r.id);
                    }
                })
                .response
                .on_hover_text("The alignment this point belongs to");
            ui.end_row();
            ui.label("order");
            let n = routes
                .iter()
                .find(|r| r.id == f.target)
                .map_or(1, |r| r.alignment.len() + usize::from(f.target != f.route));
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
