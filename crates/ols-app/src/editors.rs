// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The editor windows of the tools: a structure (new or existing: id,
//! type, extension, orientation, position), a string (insulator and wire
//! points in the structure's own axes, weight, insulator, strain), a
//! section (cable, circuit, system, bundle, stringing, points). Applying
//! one makes one session command ([`Form::command`]).

use eframe::egui;
use ols_line::{Attachment, Line, Section, SectionPoint, Stringing, Structure};
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
    /// Orientation of the local x axis, degrees from east.
    pub orientation_deg: f64,
    /// Position of the base.
    pub position: [f64; 3],
    /// The base follows the ground.
    pub on_ground: bool,
}

/// A string being added or changed, its points in the structure's axes
/// (across: to the right looking ahead, along: ahead, up from the base).
#[derive(Debug, Clone, PartialEq)]
pub struct StringForm {
    /// Not on the structure yet.
    pub new: bool,
    /// Structure.
    pub structure: String,
    /// Set.
    pub set: u32,
    /// Phase.
    pub phase: u32,
    /// Label.
    pub label: String,
    /// Insulator (structure) point, local.
    pub top: [f64; 3],
    /// Wire (clamp) point, local.
    pub bottom: [f64; 3],
    /// V-string legs, local.
    pub legs: Option<[[f64; 3]; 2]>,
    /// Insulator weight, N.
    pub weight: f64,
    /// Insulator assembly from the library.
    pub insulator: Option<String>,
    /// Solved as a strain string.
    pub strain: bool,
    /// Labels of the top and the bottom (kept).
    pub top_label: String,
    /// Bottom label.
    pub bottom_label: String,
    /// The structure is typed and this string overrides its type's.
    pub overrides_type: bool,
}

/// A section being added or changed.
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
        Some(Self { jumper })
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
    let point = |p: &SectionPoint| format!("{} {}:{}", p.structure, p.set, p.phase);
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
                id: id.clone(),
                position: self.position,
                orientation_deg: self.orientation_deg,
                structure_type: self.structure_type.clone(),
                extension: self.extension.clone(),
                attachments: Vec::new(),
                omit: Vec::new(),
                members: Vec::new(),
            }
        } else {
            let old = stored
                .and_then(|l| l.structure(&id))
                .ok_or_else(|| format!("no structure {id}"))?;
            let mut s = old.moved_to(self.position);
            if s.orientation_deg != self.orientation_deg {
                // Explicit strings turn with it about its base.
                let turn = self.orientation_deg - s.orientation_deg;
                for a in &mut s.attachments {
                    let rot = |p: [f64; 3]| {
                        to_global(
                            s.position,
                            self.orientation_deg,
                            to_local(s.position, self.orientation_deg - turn, p),
                        )
                    };
                    a.insulator_point = rot(a.insulator_point);
                    a.wire_point = rot(a.wire_point);
                    if let Some(l) = &mut a.legs {
                        *l = l.map(rot);
                    }
                }
                s.orientation_deg = self.orientation_deg;
            }
            if s.structure_type != self.structure_type {
                // The type's stick drawing comes with the new type.
                s.members.clear();
                s.omit.clear();
            }
            s.structure_type = self.structure_type.clone();
            s.extension = self.extension.clone();
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
    /// A new string on structure `id`: the next free set, hanging 3 m from
    /// the top of the structure.
    pub fn new_on(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        let set = s.attachments.iter().map(|a| a.set).max().unwrap_or(0) + 1;
        let top = s
            .attachments
            .iter()
            .map(|a| a.insulator_point[2])
            .fold(s.position[2] + 20.0, f64::max)
            - s.position[2];
        Some(Self {
            new: true,
            structure: id.to_owned(),
            set,
            phase: 1,
            label: String::new(),
            top: [0.0, 0.0, top],
            bottom: [0.0, 0.0, top - 3.0],
            legs: None,
            weight: 0.0,
            insulator: None,
            strain: false,
            top_label: String::new(),
            bottom_label: String::new(),
            overrides_type: false,
        })
    }

    /// String `set`:`phase` of structure `id` (as resolved).
    pub fn of(model: &Model, id: &str, set: u32, phase: u32) -> Option<Self> {
        let s = model.line.structure(id)?;
        let a = s
            .attachments
            .iter()
            .find(|a| (a.set, a.phase) == (set, phase))?;
        let local = |p| to_local(s.position, s.orientation_deg, p);
        let overrides_type = s.structure_type.is_some()
            && model.stored_line().is_some_and(|l| {
                l.structure(id).is_some_and(|x| {
                    x.attachments
                        .iter()
                        .any(|a| (a.set, a.phase) == (set, phase))
                })
            });
        Some(Self {
            new: false,
            structure: id.to_owned(),
            set,
            phase,
            label: a.label.clone(),
            top: local(a.insulator_point),
            bottom: local(a.wire_point),
            legs: a.legs.map(|l| l.map(local)),
            weight: a.insulator_weight,
            insulator: a.insulator.clone(),
            strain: a.strain_member,
            top_label: a.top_label.clone(),
            bottom_label: a.bottom_label.clone(),
            overrides_type,
        })
    }

    /// String length, m.
    pub fn length(&self) -> f64 {
        let d = [0, 1, 2].map(|k| self.bottom[k] - self.top[k]);
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    /// This string put on structure `s` (as stored), replacing the one
    /// with its set and phase (and no longer left out of its type).
    pub fn apply_to(&self, s: &mut Structure) {
        let key = (self.set, self.phase);
        let global = |p| to_global(s.position, s.orientation_deg, p);
        let a = Attachment {
            set: self.set,
            phase: self.phase,
            label: self.label.clone(),
            insulator_point: global(self.top),
            wire_point: global(self.bottom),
            insulator_weight: self.weight,
            insulator: self.insulator.clone(),
            // Set from the insulator library when the line is resolved.
            insulator_wind_area: 0.0,
            legs: self.legs.map(|l| l.map(global)),
            top_label: self.top_label.clone(),
            bottom_label: self.bottom_label.clone(),
            strain_member: self.strain,
        };
        s.attachments.retain(|x| (x.set, x.phase) != key);
        s.omit.retain(|k| *k != [self.set, self.phase]);
        s.attachments.push(a);
        s.attachments.sort_by_key(|a| (a.set, a.phase));
    }

    fn command(&self, stored: Option<&Line>, reset: bool) -> Result<(String, Command), String> {
        let mut s = stored
            .and_then(|l| l.structure(&self.structure))
            .ok_or_else(|| format!("no structure {}", self.structure))?
            .clone();
        if reset {
            s.attachments
                .retain(|x| (x.set, x.phase) != (self.set, self.phase));
            s.omit.retain(|k| *k != [self.set, self.phase]);
        } else {
            self.apply_to(&mut s);
        }
        let what = if reset {
            format!(
                "string {}:{} of {} back to its type",
                self.set, self.phase, self.structure
            )
        } else {
            format!(
                "{} string {}:{} of {}",
                if self.new { "added" } else { "changed" },
                self.set,
                self.phase,
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
            color: old.and_then(|o| o.color.clone()),
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
            Form::String(f) if f.new => format!("Add string to {}", f.structure),
            Form::String(f) => format!("String {}:{} of {}", f.set, f.phase, f.structure),
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
                    && model.line.structure(&f.structure).is_some_and(|s| {
                        s.attachments
                            .iter()
                            .any(|a| (a.set, a.phase) == (f.set, f.phase))
                    })
                {
                    return Err(format!(
                        "{} has a string {}:{} already",
                        f.structure, f.set, f.phase
                    ));
                }
                f.command(stored, reset)
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

fn xyz(ui: &mut egui::Ui, label: &str, p: &mut [f64; 3], names: [&str; 3]) {
    ui.label(label);
    ui.horizontal(|ui| {
        for k in 0..3 {
            ui.label(names[k]);
            value(ui, &mut p[k], 0.05, " m");
        }
    });
    ui.end_row();
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
    let axes = ["across", "along", "up"];
    egui::Grid::new("string-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("set : phase");
            ui.add_enabled_ui(f.new, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut f.set).range(1..=999));
                    ui.label(":");
                    ui.add(egui::DragValue::new(&mut f.phase).range(1..=99));
                });
            });
            ui.end_row();
            ui.label("label");
            ui.add(egui::TextEdit::singleline(&mut f.label).desired_width(140.0));
            ui.end_row();
            xyz(ui, "insulator point", &mut f.top, axes);
            xyz(ui, "wire point", &mut f.bottom, axes);
            if let Some(legs) = &mut f.legs {
                xyz(ui, "V leg 1", &mut legs[0], axes);
                xyz(ui, "V leg 2", &mut legs[1], axes);
            }
            ui.label("length");
            let mut len = f.length();
            if value(ui, &mut len, 0.01, " m")
                .on_hover_text("Changes the wire point: straight below the insulator point")
                .changed()
            {
                f.bottom = [f.top[0], f.top[1], f.top[2] - len.max(0.0)];
            }
            ui.end_row();
            ui.label("weight");
            value(ui, &mut f.weight, 1.0, " N");
            ui.end_row();
            let insulators: Vec<String> = model
                .types
                .as_ref()
                .map(|(_, i)| i.iter().map(|a| a.id.clone()).collect())
                .unwrap_or_default();
            if !insulators.is_empty() || f.insulator.is_some() {
                ui.label("insulator");
                egui::ComboBox::from_id_salt("string-insulator")
                    .selected_text(f.insulator.as_deref().unwrap_or("(none)"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.insulator, None, "(none)");
                        for i in &insulators {
                            ui.selectable_value(&mut f.insulator, Some(i.clone()), i);
                        }
                    });
                ui.end_row();
            }
        });
    ui.checkbox(&mut f.strain, "strain string (free along its conductor)");
    ui.label(
        egui::RichText::new("In the structure's axes: across (to the right looking ahead), along (ahead), up from its base.")
            .small(),
    );
    if f.overrides_type {
        ui.label(egui::RichText::new("This string overrides the structure type's.").small());
        reset = ui.button("Back to the type's string").clicked();
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
        .map(|p| format!("{} {}:{}", p.structure, p.set, p.phase))
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
        line.attachment(from)
            .ok_or_else(|| format!("no string {} {}:{}", from.structure, from.set, from.phase))?,
        line.attachment(to)
            .ok_or_else(|| format!("no string {} {}:{}", to.structure, to.set, to.phase))?,
    );
    let d = [0, 1, 2].map(|k| b.wire_point[k] - a.wire_point[k]);
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
