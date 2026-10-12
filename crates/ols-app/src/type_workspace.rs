// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The pylon workspace of a structure type (`.olss`): the type in its own
//! axes (x right of the line, y ahead, z up, 0;0 at the centre), drawn in
//! its own 3D view, with tables of its joints, members, points, wire
//! points and strings. Joints, points and wire points are picked and
//! dragged in the view (snapped to a grid or to a joint); members are
//! added between two picked joints; strings from presets. Every change is
//! made in the file as edited (the library page saves it, and every
//! structure of the type follows); the level of detail shown is worked
//! out from what the file holds.

use eframe::egui;
use eframe::egui_wgpu;
use ols_render::{Bounds, Camera, GpuScene, Renderer, Scene};
use ols_structures::{InsulatorLibrary, LibraryItem, StructureType};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

/// Something picked in the view.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Handle {
    /// A joint (`[[node]]`), by id.
    Joint(String),
    /// A member, by id.
    Member(String),
    /// A point where strings hang, by label.
    Point(String),
    /// A wire point, by label.
    Wire(String),
}

impl Handle {
    fn describe(&self) -> String {
        match self {
            Handle::Joint(id) => format!("joint {id}"),
            Handle::Member(id) => format!("member {id}"),
            Handle::Point(l) => format!("point {l}"),
            Handle::Wire(l) => format!("wire point {l}"),
        }
    }
}

/// The table shown under the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sheet {
    /// Joints.
    #[default]
    Joints,
    /// Members.
    Members,
    /// Attachment points of the structure (where insulators hang).
    Points,
    /// Insulators (strings).
    Strings,
    /// Wire attachment points (where a string ends).
    Wires,
    /// Set points (where a wire is strung).
    Sets,
}

/// What the workspace asks of the application.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Save an insulator assembly (`.olsi` text) into the insulator library.
    SaveInsulator {
        /// Its id.
        id: String,
        /// The file's text.
        text: String,
    },
    /// Open the insulator library (to import, export, edit assemblies).
    OpenInsulators,
}

/// What goes with a deletion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Removal {
    /// Joints.
    pub joints: Vec<String>,
    /// Members.
    pub members: Vec<String>,
    /// Points.
    pub points: Vec<String>,
    /// Wire points.
    pub wires: Vec<String>,
    /// Strings.
    pub strings: Vec<String>,
}

impl Removal {
    /// One line per kind, for the confirmation.
    pub fn lines(&self) -> Vec<String> {
        [
            ("joint", &self.joints),
            ("member", &self.members),
            ("point", &self.points),
            ("wire point", &self.wires),
            ("string", &self.strings),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{} {k}(s): {}", v.len(), v.join(", ")))
        .collect()
    }
}

/// A drag moving a joint or a (wire) point.
#[derive(Debug, Clone)]
struct Move {
    handle: Handle,
    /// A point of the plane it moves in (facing the camera).
    on: [f64; 3],
    /// The plane's normal.
    normal: [f64; 3],
}

/// State of the workspace.
pub struct Workspace {
    /// Shown instead of the fields of the file.
    pub on: bool,
    /// The type as last read (kept while the file as edited does not read).
    shown: Option<StructureType>,
    /// The text `shown` was read from.
    read_from: String,
    camera: Option<Camera>,
    renderer: Option<Renderer>,
    gpu: Option<GpuScene>,
    texture: Option<egui::TextureId>,
    /// The scene needs building.
    dirty: bool,
    /// Picked, in order (two joints: a member between them).
    pub selected: Vec<Handle>,
    drag: Option<Move>,
    /// The table shown.
    pub sheet: Sheet,
    /// Grid a dragged joint or point snaps to, m (0: none).
    pub grid: f64,
    /// Length of a new string (presets), m.
    pub string_length: f64,
    /// A deletion waiting for its confirmation.
    pub confirm: Option<Removal>,
    /// The angle that means ahead (the compass), as the project or the
    /// user counts angles.
    pub ahead: f64,
    /// What the last action did (or why not: `true`).
    pub message: Option<(String, bool)>,
    /// What it asks of the application (taken by it).
    pub request: Option<Request>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            on: false,
            shown: None,
            read_from: String::new(),
            camera: None,
            renderer: None,
            gpu: None,
            texture: None,
            dirty: true,
            selected: Vec::new(),
            drag: None,
            sheet: Sheet::default(),
            grid: 0.05,
            string_length: 4.0,
            confirm: None,
            message: None,
            ahead: 0.0,
            request: None,
        }
    }
}

/// A position as written in files: `["1.250 m", …]`.
fn point_value(p: [f64; 3]) -> Item {
    let mut a = Array::new();
    for v in p {
        let v = if v.abs() < 5e-4 { 0.0 } else { v };
        a.push(format!("{v:.3} m"));
    }
    Item::Value(Value::Array(a))
}

/// The array of tables `key` of the file (made when missing).
fn array<'a>(doc: &'a mut DocumentMut, key: &str) -> &'a mut ArrayOfTables {
    if doc.get(key).and_then(Item::as_array_of_tables).is_none() {
        doc.insert(key, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    doc.get_mut(key)
        .and_then(Item::as_array_of_tables_mut)
        .expect("made above")
}

/// The table of `key` whose `field` is `value`.
fn entry<'a>(
    doc: &'a mut DocumentMut,
    key: &str,
    field: &str,
    value: &str,
) -> Option<&'a mut Table> {
    doc.get_mut(key)?
        .as_array_of_tables_mut()?
        .iter_mut()
        .find(|t| t.get(field).and_then(Item::as_str) == Some(value))
}

/// `like` when it is free, else the next id after it.
fn fresh<'a>(taken: impl Iterator<Item = &'a str> + Clone, like: &str) -> String {
    if taken.clone().any(|t| t == like) {
        crate::editors::next_id(taken, like)
    } else {
        like.to_owned()
    }
}

fn length(v: f64) -> Item {
    toml_edit::value(format!("{v:.3} m"))
}

/// The positions (local axes) of the joints, points and wire points of
/// `t`, and its strings (top, wire point).
#[derive(Default)]
pub(crate) struct Geometry {
    pub(crate) joints: Vec<(String, [f64; 3], bool)>,
    pub(crate) members: Vec<(String, [f64; 3], [f64; 3], bool)>,
    pub(crate) points: Vec<(String, [f64; 3])>,
    pub(crate) wires: Vec<(String, [f64; 3], Option<u32>)>,
    pub(crate) strings: Vec<(String, [f64; 3], [f64; 3])>,
    pub(crate) sticks: Vec<Vec<[f64; 3]>>,
}

pub(crate) fn geometry(t: &StructureType, insulators: &InsulatorLibrary) -> Geometry {
    let si = |p: &[ols_core::units::Length; 3]| p.map(|v| v.si());
    let mut g = Geometry {
        joints: t
            .nodes
            .iter()
            .map(|n| (n.id.clone(), si(&n.point), n.support.is_some()))
            .collect(),
        ..Default::default()
    };
    let joint = |id: &str| g.joints.iter().find(|j| j.0 == id).map(|j| j.1);
    g.members = t
        .members
        .iter()
        .filter_map(|m| {
            Some((
                m.id.clone(),
                joint(&m.nodes[0])?,
                joint(&m.nodes[1])?,
                m.section.is_some(),
            ))
        })
        .collect();
    // Placed with the feet on the ground at the feet's level: local axes.
    let foot = t.foot_level();
    if let Ok(placed) = t.place_strings([0.0, 0.0, foot], 0.0, None, insulators) {
        g.points = placed
            .points
            .iter()
            .map(|p| (p.label.clone(), p.position))
            .collect();
        g.wires = placed
            .wires
            .iter()
            .map(|w| (w.label.clone(), w.position, w.set))
            .collect();
        let at = |label: &str| {
            g.points
                .iter()
                .map(|p| (&p.0, p.1))
                .chain(g.wires.iter().map(|w| (&w.0, w.1)))
                .find(|(l, _)| *l == label)
                .map(|(_, p)| p)
        };
        g.strings = placed
            .strings
            .iter()
            .filter_map(|s| Some((s.label.clone(), at(&s.top)?, at(&s.wire)?)))
            .collect();
    } else {
        g.points = t
            .points
            .iter()
            .map(|p| (p.label.clone(), si(&p.position())))
            .collect();
    }
    if t.members.is_empty() {
        // The stick figure (or the body lines), back in local axes.
        if let Ok(lines) = t.stick(None) {
            g.sticks = lines
                .into_iter()
                .map(|l| l.into_iter().map(|p| [p[0], p[1], p[2] + foot]).collect())
                .collect();
        }
    } else {
        for b in &t.body {
            g.sticks.push(b.points.iter().map(si).collect());
        }
    }
    g
}

const STEEL: [u8; 4] = [215, 215, 220, 255];
const BARE: [u8; 4] = [200, 170, 110, 255];
const STICK: [u8; 4] = [150, 150, 160, 255];
const STRING: [u8; 4] = [120, 180, 255, 255];
const POINT: [u8; 4] = [255, 150, 60, 255];
const WIRE: [u8; 4] = [90, 200, 255, 255];
const PICKED: [u8; 4] = [255, 235, 60, 255];

impl Workspace {
    /// Reads the file as edited again when it changed (what is drawn
    /// follows).
    fn refresh(&mut self, doc: &DocumentMut) {
        let text = doc.to_string();
        if text == self.read_from && self.shown.is_some() {
            return;
        }
        if let Ok(t) = StructureType::parse(std::path::Path::new("type.olss"), &text) {
            self.shown = Some(t);
        }
        self.read_from = text;
        self.dirty = true;
    }

    /// Draws its scene again on the next frame.
    pub fn redraw(&mut self) {
        self.dirty = true;
    }

    /// The type as last read.
    pub fn shown(&self) -> Option<&StructureType> {
        self.shown.as_ref()
    }

    /// What goes when `h` is deleted: a joint takes its members and the
    /// points on them; a member the points along it; a point or wire
    /// point the strings from or to it.
    pub fn removal(t: &StructureType, h: &Handle) -> Removal {
        let mut r = Removal::default();
        match h {
            Handle::Joint(id) => {
                r.joints.push(id.clone());
                r.members = t
                    .members
                    .iter()
                    .filter(|m| m.nodes.contains(id))
                    .map(|m| m.id.clone())
                    .collect();
            }
            Handle::Member(id) => r.members.push(id.clone()),
            Handle::Point(l) => r.points.push(l.clone()),
            Handle::Wire(l) => r.wires.push(l.clone()),
        }
        for p in &t.points {
            let on = p.node.as_ref().is_some_and(|n| r.joints.contains(n))
                || p.member.as_ref().is_some_and(|m| r.members.contains(m));
            if on && !r.points.contains(&p.label) {
                r.points.push(p.label.clone());
            }
        }
        for s in &t.strings {
            let gone = [&s.top, &s.wire]
                .iter()
                .any(|l| r.points.contains(l) || r.wires.contains(l));
            if gone {
                r.strings.push(s.label.clone());
            }
        }
        r
    }

    /// Removes everything in `r` from the file.
    pub fn remove(doc: &mut DocumentMut, r: &Removal) {
        for (key, field, gone) in [
            ("node", "id", &r.joints),
            ("member", "id", &r.members),
            ("point", "label", &r.points),
            ("wire", "label", &r.wires),
            ("string", "label", &r.strings),
        ] {
            if let Some(a) = doc.get_mut(key).and_then(Item::as_array_of_tables_mut) {
                a.retain(|t| {
                    !t.get(field)
                        .and_then(Item::as_str)
                        .is_some_and(|v| gone.iter().any(|g| g == v))
                });
            }
        }
    }

    /// Adds a joint at `at` (local axes); its id.
    pub fn add_joint(doc: &mut DocumentMut, t: Option<&StructureType>, at: [f64; 3]) -> String {
        let like = t
            .and_then(|t| t.nodes.last())
            .map_or("N1", |n| n.id.as_str());
        let id = crate::editors::next_id(
            t.into_iter()
                .flat_map(|t| t.nodes.iter().map(|n| n.id.as_str())),
            like,
        );
        let mut table = Table::new();
        table.insert("id", toml_edit::value(id.clone()));
        table.insert("point", point_value(at));
        array(doc, "node").push(table);
        id
    }

    /// Adds a member between joints `a` and `b`; its id (or why not).
    pub fn add_member(
        doc: &mut DocumentMut,
        t: &StructureType,
        a: &str,
        b: &str,
    ) -> Result<String, String> {
        if a == b {
            return Err("pick two different joints".into());
        }
        if t.members
            .iter()
            .any(|m| (m.nodes[0] == a && m.nodes[1] == b) || (m.nodes[0] == b && m.nodes[1] == a))
        {
            return Err(format!("joints {a} and {b} are joined already"));
        }
        let like = t.members.last().map_or("M1", |m| m.id.as_str());
        let id = crate::editors::next_id(t.members.iter().map(|m| m.id.as_str()), like);
        let mut table = Table::new();
        table.insert("id", toml_edit::value(id.clone()));
        let mut nodes = Array::new();
        nodes.push(a);
        nodes.push(b);
        table.insert("nodes", Item::Value(Value::Array(nodes)));
        // Like the last member: its section and kind.
        if let Some(last) = t.members.last() {
            if let Some(s) = &last.section {
                table.insert("section", toml_edit::value(s.clone()));
            }
            if let Some(k) = &last.kind {
                table.insert("kind", toml_edit::value(k.clone()));
            }
        }
        array(doc, "member").push(table);
        Ok(id)
    }

    fn labels(t: &StructureType) -> Vec<&str> {
        t.points
            .iter()
            .map(|p| p.label.as_str())
            .chain(t.wires.iter().map(|w| w.label.as_str()))
            .collect()
    }

    /// A point of the type at joint `joint`; its label.
    pub fn add_point_at_joint(doc: &mut DocumentMut, t: &StructureType, joint: &str) -> String {
        let label = fresh(Self::labels(t).into_iter(), "A1");
        let mut table = Table::new();
        table.insert("label", toml_edit::value(label.clone()));
        table.insert("node", toml_edit::value(joint));
        array(doc, "point").push(table);
        label
    }

    /// Presets: an I-string of `length` from `top` (a point or a wire
    /// point) down to a new wire point; or, with two tops, a V: two
    /// strings to one wire point `length` below the middle of the tops.
    /// No set: the set is put on it by hand (Set points). The new wire
    /// point's label.
    pub fn add_strings(
        doc: &mut DocumentMut,
        t: &StructureType,
        tops: &[String],
        length: f64,
    ) -> Result<String, String> {
        if tops.is_empty() || tops.len() > 2 {
            return Err("pick one point (an I-string) or two (a V)".into());
        }
        if !(length > 0.0 && length < 50.0) {
            return Err("the string length must be between 0 and 50 m".into());
        }
        let labels = Self::labels(t);
        let wire = fresh(labels.iter().copied(), "W1");
        let mut w = Table::new();
        w.insert("label", toml_edit::value(wire.clone()));
        array(doc, "wire").push(w);
        let mut taken: Vec<String> = t.strings.iter().map(|s| s.label.clone()).collect();
        for top in tops {
            let label = fresh(taken.iter().map(String::as_str), "S1");
            taken.push(label.clone());
            let mut s = Table::new();
            s.insert("label", toml_edit::value(label));
            s.insert("top", toml_edit::value(top.clone()));
            s.insert("wire", toml_edit::value(wire.clone()));
            s.insert("length", length_or_leg(t, tops, length));
            array(doc, "string").push(s);
        }
        Ok(wire)
    }

    /// Writes a moved joint or (wire) point at `at` into the file.
    fn put(
        doc: &mut DocumentMut,
        t: &StructureType,
        h: &Handle,
        at: [f64; 3],
    ) -> Result<(), String> {
        match h {
            Handle::Joint(id) => {
                let n = entry(doc, "node", "id", id).ok_or("no such joint")?;
                n.insert("point", point_value(at));
            }
            Handle::Point(l) => {
                let p = t
                    .points
                    .iter()
                    .find(|p| &p.label == l)
                    .ok_or("no such point")?;
                if let Some(n) = &p.node {
                    // On a joint: the joint moves.
                    return Self::put(doc, t, &Handle::Joint(n.clone()), at);
                }
                if p.member.is_some() {
                    return Err(format!("point {l} sits along a member: move its joints"));
                }
                let p = entry(doc, "point", "label", l).ok_or("no such point")?;
                p.insert("point", point_value(at));
            }
            Handle::Wire(l) => {
                let w = entry(doc, "wire", "label", l).ok_or("no such wire point")?;
                w.insert("point", point_value(at));
            }
            Handle::Member(_) => return Err("move its joints".into()),
        }
        Ok(())
    }

    /// Where `h` is drawn (local axes).
    fn position(g: &Geometry, h: &Handle) -> Option<[f64; 3]> {
        match h {
            Handle::Joint(id) => g.joints.iter().find(|j| &j.0 == id).map(|j| j.1),
            Handle::Point(l) => g.points.iter().find(|p| &p.0 == l).map(|p| p.1),
            Handle::Wire(l) => g.wires.iter().find(|w| &w.0 == l).map(|w| w.1),
            Handle::Member(id) => g
                .members
                .iter()
                .find(|m| &m.0 == id)
                .map(|m| [0, 1, 2].map(|k| 0.5 * (m.1[k] + m.2[k]))),
        }
    }

    fn scene(&self, g: &Geometry, foot: f64) -> Scene {
        let mut s = Scene::new([0.0; 3]);
        // The ground at the feet, a 1 m grid; the axes at the origin.
        let extent = g
            .joints
            .iter()
            .map(|j| j.1)
            .chain(g.points.iter().map(|p| p.1))
            .chain(g.wires.iter().map(|w| w.1))
            .chain(g.sticks.iter().flatten().copied())
            .map(|p| p[0].abs().max(p[1].abs()))
            .fold(5.0_f64, f64::max)
            .ceil()
            + 2.0;
        let n = extent as i64;
        for k in -n..=n {
            let v = k as f64;
            let c = if k % 5 == 0 {
                [70, 74, 82, 255]
            } else {
                [45, 48, 54, 255]
            };
            s.polyline(&[[v, -extent, foot], [v, extent, foot]], c, 1.0);
            s.polyline(&[[-extent, v, foot], [extent, v, foot]], c, 1.0);
        }
        s.polyline(&[[0.0; 3], [3.0, 0.0, 0.0]], [230, 80, 70, 255], 2.5);
        s.polyline(&[[0.0; 3], [0.0, 3.0, 0.0]], [90, 200, 90, 255], 2.5);
        s.polyline(&[[0.0; 3], [0.0, 0.0, 3.0]], [90, 140, 240, 255], 2.5);
        // The compass on the ground at the feet: the angles as counted.
        crate::editors::draw_compass(&mut s, compass_radius(g), self.ahead, None, foot);
        for l in &g.sticks {
            s.polyline(l, STICK, 1.5);
        }
        let picked = |h: Handle| self.selected.contains(&h);
        for (id, a, b, section) in &g.members {
            let c = if picked(Handle::Member(id.clone())) {
                PICKED
            } else if *section {
                STEEL
            } else {
                BARE
            };
            s.polyline(&[*a, *b], c, 1.8);
        }
        for (_, a, b) in &g.strings {
            s.polyline(&[*a, *b], STRING, 2.5);
        }
        for (id, p, support) in &g.joints {
            let (c, size) = match (picked(Handle::Joint(id.clone())), support) {
                (true, _) => (PICKED, 11.0),
                (false, true) => ([110, 220, 120, 255], 9.0),
                (false, false) => (STEEL, 6.0),
            };
            s.point(*p, c, size);
        }
        for (l, p) in &g.points {
            let c = if picked(Handle::Point(l.clone())) {
                PICKED
            } else {
                POINT
            };
            s.point(*p, c, 10.0);
        }
        for (l, p, _) in &g.wires {
            let c = if picked(Handle::Wire(l.clone())) {
                PICKED
            } else {
                WIRE
            };
            s.point(*p, c, 10.0);
        }
        s
    }

    /// The handle nearest to `at` (screen pixels) within `radius`: joints,
    /// points and wire points first, then members.
    fn pick(
        g: &Geometry,
        project: &impl Fn([f64; 3]) -> Option<[f64; 2]>,
        at: [f64; 2],
        radius: f64,
    ) -> Option<Handle> {
        let d = |p: [f64; 3]| {
            project(p).map(|q| ((q[0] - at[0]).powi(2) + (q[1] - at[1]).powi(2)).sqrt())
        };
        let nearest = |found: Vec<(Option<f64>, Handle)>| {
            found
                .into_iter()
                .filter_map(|(dist, h)| Some((dist.filter(|x| *x <= radius)?, h)))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|b| b.1)
        };
        let handles = g
            .points
            .iter()
            .map(|(l, p)| (d(*p), Handle::Point(l.clone())))
            .chain(
                g.wires
                    .iter()
                    .map(|(l, p, _)| (d(*p), Handle::Wire(l.clone()))),
            )
            .chain(
                g.joints
                    .iter()
                    .map(|(id, p, _)| (d(*p), Handle::Joint(id.clone()))),
            )
            .collect();
        if let Some(h) = nearest(handles) {
            return Some(h);
        }
        let mut members = Vec::new();
        for (id, a, b, _) in &g.members {
            let (Some(qa), Some(qb)) = (project(*a), project(*b)) else {
                continue;
            };
            let (dx, dy) = (qb[0] - qa[0], qb[1] - qa[1]);
            let len2 = dx * dx + dy * dy;
            let s = if len2 > 0.0 {
                (((at[0] - qa[0]) * dx + (at[1] - qa[1]) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (x, y) = (qa[0] + s * dx, qa[1] + s * dy);
            members.push((
                Some(((x - at[0]).powi(2) + (y - at[1]).powi(2)).sqrt()),
                Handle::Member(id.clone()),
            ));
        }
        nearest(members)
    }

    /// The workspace: level, tools, the view and the tables. `doc` is the
    /// file as edited; `true` when it was changed. `used_by`: the
    /// structures of the line with this type.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut DocumentMut,
        rs: Option<&egui_wgpu::RenderState>,
        insulators: &InsulatorLibrary,
        note: &str,
    ) -> bool {
        self.refresh(doc);
        let Some(t) = self.shown.clone() else {
            ui.label("The file does not read yet: see the error above (Fields to correct it).");
            return false;
        };
        let mut changed = false;
        // The level, worked out from the content, and what it lacks.
        ui.horizontal_wrapped(|ui| {
            let d = t.detail();
            ui.strong(format!("Level {} — {}", d.level(), d.name()));
            let missing = t.missing_for_design();
            if !missing.is_empty() && !t.members.is_empty() {
                ui.label(
                    egui::RichText::new(format!("for a full design: {}", missing.join(", ")))
                        .weak(),
                );
            }
            ui.separator();
            ui.label(format!(
                "{} joint(s), {} member(s), {} point(s), {} wire point(s), {} string(s)",
                t.nodes.len(),
                t.members.len(),
                t.points.len(),
                t.wires.len(),
                t.strings.len()
            ));
            ui.separator();
            ui.label(note);
        });
        changed |= self.tools(ui, doc, &t);
        if let Some((m, error)) = &self.message {
            let text = egui::RichText::new(m).small();
            if *error {
                crate::menu::error_label(ui, text);
            } else {
                ui.colored_label(egui::Color32::LIGHT_GREEN, text);
            }
        }
        changed |= self.confirm_ui(ui, doc, &t);
        let g = geometry(&t, insulators);
        egui::Panel::bottom(egui::Id::new("type-workspace-tables"))
            .resizable(true)
            .default_size(180.0)
            .show(ui, |ui| {
                changed |= self.tables(ui, doc, &t, &g, insulators);
            });
        egui::CentralPanel::default().show(ui, |ui| {
            changed |= self.view(ui, doc, rs, &t, &g);
        });
        if changed {
            self.refresh(doc);
        }
        changed
    }

    fn tools(&mut self, ui: &mut egui::Ui, doc: &mut DocumentMut, t: &StructureType) -> bool {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("snap");
            ui.add(
                egui::DragValue::new(&mut self.grid)
                    .range(0.0..=5.0)
                    .speed(0.01)
                    .max_decimals(3)
                    .suffix(" m"),
            )
            .on_hover_text("A dragged joint or point snaps to this grid (0: none), or onto a joint");
            ui.separator();
            if ui
                .button("Add joint")
                .on_hover_text("A joint above the joint picked (or at the centre)")
                .clicked()
            {
                let at = match self.selected.last() {
                    Some(Handle::Joint(id)) => t
                        .nodes
                        .iter()
                        .find(|n| &n.id == id)
                        .map(|n| n.point.map(|v| v.si()))
                        .map(|p| [p[0], p[1], p[2] + 1.0]),
                    _ => None,
                }
                .unwrap_or([0.0; 3]);
                let id = Self::add_joint(doc, Some(t), at);
                self.selected = vec![Handle::Joint(id.clone())];
                self.message = Some((format!("added joint {id}"), false));
                changed = true;
            }
            let joints: Vec<&String> = self
                .selected
                .iter()
                .filter_map(|h| match h {
                    Handle::Joint(id) => Some(id),
                    _ => None,
                })
                .collect();
            if ui
                .add_enabled(joints.len() == 2, egui::Button::new("Member"))
                .on_hover_text("A member between the two joints picked (Shift + click the second)")
                .on_disabled_hover_text("Pick two joints (Shift + click the second)")
                .clicked()
            {
                let (a, b) = (joints[0].clone(), joints[1].clone());
                match Self::add_member(doc, t, &a, &b) {
                    Ok(id) => {
                        self.message = Some((format!("added member {id} ({a}–{b})"), false));
                        // The second joint stays picked: the next member
                        // starts there.
                        self.selected = vec![Handle::Joint(b)];
                        changed = true;
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            if let [Handle::Joint(j)] = self.selected.clone().as_slice()
                && ui
                    .button("Point here")
                    .on_hover_text("A point where strings hang, on this joint (it moves with it)")
                    .clicked()
            {
                let l = Self::add_point_at_joint(doc, t, j);
                self.selected = vec![Handle::Point(l.clone())];
                self.message = Some((format!("added point {l} on joint {j}"), false));
                changed = true;
            }
            ui.separator();
            let tops: Vec<String> = self
                .selected
                .iter()
                .filter_map(|h| match h {
                    Handle::Point(l) | Handle::Wire(l) => Some(l.clone()),
                    _ => None,
                })
                .collect();
            ui.label("string");
            ui.add(
                egui::DragValue::new(&mut self.string_length)
                    .range(0.05..=50.0)
                    .speed(0.05)
                    .max_decimals(3)
                    .suffix(" m"),
            );
            for (name, n, help) in [
                ("I", 1, "An insulator from the point picked down to a new wire point (its set: Set points)"),
                ("V", 2, "A V: two strings from the two points picked to one wire point below their middle"),
            ] {
                if ui
                    .add_enabled(tops.len() == n, egui::Button::new(name))
                    .on_hover_text(help)
                    .on_disabled_hover_text(if n == 1 {
                        "Pick one point (or a wire point: a string below it)"
                    } else {
                        "Pick two points (Shift + click the second)"
                    })
                    .clicked()
                {
                    match Self::add_strings(doc, t, &tops, self.string_length) {
                        Ok(w) => {
                            self.message = Some((
                                format!("added the insulators to wire point {w}: put its set in Set points"),
                                false,
                            ));
                            self.selected = vec![Handle::Wire(w)];
                            changed = true;
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                }
            }
            ui.separator();
            if ui
                .add_enabled(self.selected.len() == 1, egui::Button::new("Delete…"))
                .on_hover_text("Delete what is picked, with what goes with it (asks first)")
                .clicked()
            {
                self.confirm = Some(Self::removal(t, &self.selected[0]));
            }
            if ui.button("Fit").on_hover_text("Show the whole pylon").clicked() {
                self.camera = None;
            }
            if !self.selected.is_empty() {
                ui.label(
                    egui::RichText::new(
                        self.selected
                            .iter()
                            .map(Handle::describe)
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                    .color(egui::Color32::from_rgb(255, 235, 60)),
                );
            }
        });
        changed
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui, doc: &mut DocumentMut, _t: &StructureType) -> bool {
        let Some(r) = self.confirm.clone() else {
            return false;
        };
        let mut changed = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("Delete, with everything that goes with it:");
            egui::ScrollArea::vertical()
                .max_height(90.0)
                .id_salt("type-workspace-removal")
                .show(ui, |ui| {
                    for l in r.lines() {
                        ui.label(l);
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    Self::remove(doc, &r);
                    self.selected.clear();
                    self.message = Some((format!("deleted {}", r.lines().join("; ")), false));
                    self.confirm = None;
                    changed = true;
                }
                if ui.button("Cancel").clicked() {
                    self.confirm = None;
                }
            });
        });
        changed
    }

    fn view(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut DocumentMut,
        rs: Option<&egui_wgpu::RenderState>,
        t: &StructureType,
        g: &Geometry,
    ) -> bool {
        let Some(rs) = rs else {
            ui.centered_and_justified(|ui| ui.label("The pylon's 3D view needs a GPU (wgpu)."));
            return false;
        };
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let foot = t.foot_level();
        let camera = self.camera.get_or_insert_with(|| {
            let mut b: Option<Bounds> = None;
            for p in g
                .joints
                .iter()
                .map(|j| j.1)
                .chain(g.points.iter().map(|p| p.1))
                .chain(g.wires.iter().map(|w| w.1))
                .chain(g.sticks.iter().flatten().copied())
                .chain([[0.0, 0.0, foot]])
            {
                let b = b.get_or_insert(Bounds { min: p, max: p });
                b.min = [0, 1, 2].map(|k| b.min[k].min(p[k]));
                b.max = [0, 1, 2].map(|k| b.max[k].max(p[k]));
            }
            let b = b.expect("the feet at least");
            let mut c = Camera::fit(&b);
            // The whole pylon in a view wider than high.
            let aspect = f64::from(rect.width() / rect.height().max(1.0));
            c.distance *= 1.3 * (1.0 / aspect.min(1.0)).max(1.0);
            c.pitch = 15f64.to_radians();
            c
        });
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let local = |p: egui::Pos2| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)];
        let cam = *camera;
        let projector = cam.projector(size);
        let project = |p: [f64; 3]| projector(p).map(|(q, _)| q);
        let mut changed = false;
        // Picking: a click picks, Shift + click adds.
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let shift = ui.input(|i| i.modifiers.shift);
            match Self::pick(g, &project, local(pos), 12.0) {
                Some(h) if shift => {
                    if let Some(k) = self.selected.iter().position(|x| *x == h) {
                        self.selected.remove(k);
                    } else {
                        self.selected.push(h);
                    }
                }
                Some(h) => self.selected = vec![h],
                None if !shift => self.selected.clear(),
                None => {}
            }
            self.dirty = true;
        }
        // A drag from a joint or point moves it; elsewhere it turns the view.
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
            && let Some(h) = Self::pick(g, &project, local(pos), 12.0)
            && !matches!(h, Handle::Member(_))
            && let Some(at) = Self::position(g, &h)
        {
            let eye = cam.eye();
            let n = [0, 1, 2].map(|k| eye[k] - cam.target[k]);
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            self.selected = vec![h.clone()];
            self.drag = Some(Move {
                handle: h,
                on: at,
                normal: n.map(|v| v / len),
            });
        }
        if let Some(m) = self.drag.clone() {
            if response.dragged_by(egui::PointerButton::Primary)
                && let Some(pos) = response.interact_pointer_pos()
                && let Some((a, d)) = cam.ray(local(pos), size)
            {
                let dn: f64 = (0..3).map(|k| d[k] * m.normal[k]).sum();
                if dn.abs() > 1e-9 {
                    let s: f64 = (0..3).map(|k| (m.on[k] - a[k]) * m.normal[k]).sum::<f64>() / dn;
                    let mut at = [0, 1, 2].map(|k| a[k] + s * d[k]);
                    if self.grid > 0.0 {
                        at = at.map(|v| (v / self.grid).round() * self.grid);
                    }
                    // Onto a joint near the pointer.
                    let here = local(pos);
                    if let Some((_, p, _)) = g.joints.iter().find(|j| {
                        Handle::Joint(j.0.clone()) != m.handle
                            && project(j.1)
                                .is_some_and(|q| (q[0] - here[0]).hypot(q[1] - here[1]) < 8.0)
                    }) {
                        at = *p;
                    }
                    if Self::position(g, &m.handle) != Some(at) {
                        match Self::put(doc, t, &m.handle, at) {
                            Ok(()) => {
                                changed = true;
                                self.message = Some((
                                    format!(
                                        "{} at {:.3}, {:.3}, {:.3} m",
                                        m.handle.describe(),
                                        at[0],
                                        at[1],
                                        at[2]
                                    ),
                                    false,
                                ));
                            }
                            Err(e) => {
                                self.message = Some((e, true));
                                self.drag = None;
                            }
                        }
                    }
                }
            }
            if !response.dragged() {
                self.drag = None;
            }
        } else if response.dragged() {
            let d = response.drag_delta();
            let camera = self.camera.as_mut().expect("set above");
            if response.dragged_by(egui::PointerButton::Primary) {
                camera.orbit(f64::from(d.x) * 0.008, f64::from(d.y) * 0.008);
            } else {
                camera.pan(f64::from(d.x), f64::from(d.y), size[1]);
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let camera = self.camera.as_mut().expect("set above");
                camera.zoom((-f64::from(scroll) * 0.002).exp());
            }
        }
        // Draw.
        let device = &rs.device;
        if self.dirty || changed || self.gpu.is_none() {
            self.gpu = Some(GpuScene::new(device, &self.scene(g, foot)));
            self.dirty = false;
        }
        let camera = *self.camera.as_ref().expect("set above");
        let renderer = self.renderer.get_or_insert_with(|| Renderer::new(device));
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        let layers: Vec<&GpuScene> = self.gpu.iter().collect();
        let texture = renderer.render(device, &rs.queue, &layers, &camera, px);
        let view = texture.create_view(&eframe::wgpu::TextureViewDescriptor {
            format: Some(eframe::wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        let mut egui_renderer = rs.renderer.write();
        let id = match self.texture {
            Some(id) => {
                egui_renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &view,
                    eframe::wgpu::FilterMode::Linear,
                    id,
                );
                id
            }
            None => {
                let id = egui_renderer.register_native_texture(
                    device,
                    &view,
                    eframe::wgpu::FilterMode::Linear,
                );
                self.texture = Some(id);
                id
            }
        };
        drop(egui_renderer);
        let painter = ui.painter_at(rect);
        painter.image(
            id,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        // Labels: points and wire points (with their set), what is picked.
        let projector = camera.projector(size);
        // Labels outside the dots, away from the middle of the view.
        let screen = |p: [f64; 3]| {
            projector(p).map(|(q, _)| {
                let at = rect.min + egui::vec2(q[0] as f32, q[1] as f32);
                if at.x < rect.center().x {
                    (at + egui::vec2(-7.0, -7.0), egui::Align2::RIGHT_BOTTOM)
                } else {
                    (at + egui::vec2(7.0, -7.0), egui::Align2::LEFT_BOTTOM)
                }
            })
        };
        let font = egui::FontId::proportional(12.0);
        let rgb = |c: [u8; 4]| egui::Color32::from_rgb(c[0], c[1], c[2]);
        for (p, text, color) in
            crate::editors::compass_texts(compass_radius(g), self.ahead, None, foot)
        {
            if let Some((q, _)) = projector(p) {
                painter.text(
                    rect.min + egui::vec2(q[0] as f32, q[1] as f32),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(11.0),
                    color,
                );
            }
        }
        for (l, p) in &g.points {
            if let Some((q, align)) = screen(*p) {
                painter.text(q, align, l, font.clone(), rgb(POINT));
            }
        }
        for (l, p, set) in &g.wires {
            if let Some((q, align)) = screen(*p) {
                let text = set.map_or(l.clone(), |s| format!("{l} (set {s})"));
                painter.text(q, align, text, font.clone(), rgb(WIRE));
            }
        }
        for h in &self.selected {
            if let (Handle::Joint(id) | Handle::Member(id), Some(p)) = (h, Self::position(g, h))
                && let Some((q, align)) = screen(p)
            {
                painter.text(q, align, id, font.clone(), rgb(PICKED));
            }
        }
        painter.text(
            rect.left_bottom() + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            "x right of the line (red), y ahead (green), z up (blue) — drag: turn, right-drag: pan, \
             wheel: zoom; drag a joint or point to move it; Shift + click: pick another",
            egui::FontId::proportional(11.0),
            egui::Color32::GRAY,
        );
        changed
    }

    fn tables(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut DocumentMut,
        t: &StructureType,
        g: &Geometry,
        insulators: &InsulatorLibrary,
    ) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            for (s, name, tip) in [
                (Sheet::Joints, "Joints", "The pylon's joints"),
                (Sheet::Members, "Members", "The pylon's members"),
                (
                    Sheet::Points,
                    "Attachment points",
                    "Where insulators hang on the structure: part of the pylon, moved only here",
                ),
                (
                    Sheet::Strings,
                    "Insulators",
                    "The strings: each from an attachment point (or another insulator's wire point) to its wire point, by its length and direction",
                ),
                (
                    Sheet::Wires,
                    "Wire points",
                    "Where an insulator ends: placed by its string(s); another insulator may hang from it, or a set be on it",
                ),
                (
                    Sheet::Sets,
                    "Set points",
                    "Where a wire is strung: a set number on a wire point; suspension or strain from its insulator",
                ),
            ] {
                ui.selectable_value(&mut self.sheet, s, name).on_hover_text(tip);
            }
        });
        let coord = |ui: &mut egui::Ui, v: &mut f64| {
            ui.add(
                egui::DragValue::new(v)
                    .speed(0.01)
                    .max_decimals(3)
                    .suffix(" m"),
            )
            .changed()
        };
        egui::ScrollArea::both()
            .id_salt("type-workspace-sheet")
            .auto_shrink(false)
            .show(ui, |ui| match self.sheet {
                Sheet::Joints => {
                    egui::Grid::new("type-joints").striped(true).show(ui, |ui| {
                        for h in ["joint", "x", "y", "z", "support"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for n in &t.nodes {
                            let h = Handle::Joint(n.id.clone());
                            if ui.selectable_label(self.selected.contains(&h), &n.id).clicked() {
                                self.selected = vec![h.clone()];
                                self.dirty = true;
                            }
                            let mut p = n.point.map(|v| v.si());
                            let mut moved = false;
                            for v in &mut p {
                                moved |= coord(ui, v);
                            }
                            if moved && Self::put(doc, t, &h, p).is_ok() {
                                changed = true;
                            }
                            let support = n
                                .support
                                .as_ref()
                                .map_or("—".to_owned(), |s| s.as_str().map_or(s.to_string(), str::to_owned));
                            let mut chosen = support.clone();
                            egui::ComboBox::from_id_salt(("support", &n.id))
                                .selected_text(&chosen)
                                .width(80.0)
                                .show_ui(ui, |ui| {
                                    for s in ["—", "pinned", "fixed"] {
                                        ui.selectable_value(&mut chosen, s.to_owned(), s);
                                    }
                                });
                            if chosen != support
                                && let Some(e) = entry(doc, "node", "id", &n.id)
                            {
                                if chosen == "—" {
                                    e.remove("support");
                                } else {
                                    e.insert("support", toml_edit::value(chosen));
                                }
                                changed = true;
                            }
                            ui.end_row();
                        }
                    });
                }
                Sheet::Members => {
                    let sections: Vec<String> = t
                        .sections
                        .iter()
                        .filter_map(|s| s.get("id")?.as_str().map(str::to_owned))
                        .collect();
                    egui::Grid::new("type-members").striped(true).show(ui, |ui| {
                        for h in ["member", "from", "to", "length", "section", "kind"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (m, geo) in t.members.iter().zip(&g.members) {
                            let h = Handle::Member(m.id.clone());
                            if ui.selectable_label(self.selected.contains(&h), &m.id).clicked() {
                                self.selected = vec![h];
                                self.dirty = true;
                            }
                            ui.label(&m.nodes[0]);
                            ui.label(&m.nodes[1]);
                            let l = (0..3).map(|k| (geo.2[k] - geo.1[k]).powi(2)).sum::<f64>().sqrt();
                            ui.label(format!("{l:.3} m"));
                            let current = m.section.clone().unwrap_or_else(|| "—".into());
                            let mut chosen = current.clone();
                            egui::ComboBox::from_id_salt(("section", &m.id))
                                .selected_text(&chosen)
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut chosen, "—".to_owned(), "— (a stick)");
                                    for s in &sections {
                                        ui.selectable_value(&mut chosen, s.clone(), s);
                                    }
                                });
                            let kind = m.kind.clone().unwrap_or_else(|| "truss".into());
                            let mut new_kind = kind.clone();
                            egui::ComboBox::from_id_salt(("kind", &m.id))
                                .selected_text(&new_kind)
                                .width(70.0)
                                .show_ui(ui, |ui| {
                                    for k in ["truss", "frame", "cable"] {
                                        ui.selectable_value(&mut new_kind, k.to_owned(), k);
                                    }
                                });
                            if (chosen != current || new_kind != kind)
                                && let Some(e) = entry(doc, "member", "id", &m.id)
                            {
                                if chosen == "—" {
                                    e.remove("section");
                                } else {
                                    e.insert("section", toml_edit::value(chosen));
                                }
                                if new_kind != kind {
                                    e.insert("kind", toml_edit::value(new_kind));
                                }
                                changed = true;
                            }
                            ui.end_row();
                        }
                    });
                    if sections.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "No [[section]] yet: add sections and materials in Fields for a full design.",
                            )
                            .weak(),
                        );
                    }
                }
                Sheet::Points => {
                    egui::Grid::new("type-points").striped(true).show(ui, |ui| {
                        for h in ["attachment point", "x", "y", "z", "on", "insulators"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for p in &t.points {
                            let hanging = t.strings.iter().filter(|x| x.top == p.label).count();
                            let h = Handle::Point(p.label.clone());
                            if ui.selectable_label(self.selected.contains(&h), &p.label).clicked() {
                                self.selected = vec![h.clone()];
                                self.dirty = true;
                            }
                            let mut at = p.position().map(|v| v.si());
                            let free = p.node.is_none() && p.member.is_none();
                            let mut moved = false;
                            for v in &mut at {
                                if free {
                                    moved |= coord(ui, v);
                                } else {
                                    ui.label(format!("{v:.3} m"));
                                }
                            }
                            if moved && Self::put(doc, t, &h, at).is_ok() {
                                changed = true;
                            }
                            ui.label(match (&p.node, &p.member, p.at) {
                                (Some(n), _, _) => format!("joint {n}"),
                                (_, Some(m), Some(f)) => format!("member {m} at {f}"),
                                _ => "—".into(),
                            });
                            ui.label(hanging.to_string());
                            ui.end_row();
                        }
                    });
                }
                Sheet::Wires => {
                    egui::Grid::new("type-wires").striped(true).show(ui, |ui| {
                        for (h, tip) in [
                            ("wire point", "Its label"),
                            ("x", "Placed by its insulator(s); typed only for one no insulator reaches (fixed)"),
                            ("y", ""),
                            ("z", ""),
                            ("held by", "The insulators ending here (two: a V)"),
                            ("below it", "Insulators hanging from it (a TAK, a V with a string below)"),
                            ("set", "A set on it: a wire is strung here (Set points)"),
                            ("counterweight", "A weight hanging at it"),
                        ] {
                            ui.strong(h).on_hover_text(tip);
                        }
                        ui.end_row();
                        for w in &t.wires {
                            let h = Handle::Wire(w.label.clone());
                            if ui.selectable_label(self.selected.contains(&h), &w.label).clicked() {
                                self.selected = vec![h.clone()];
                                self.dirty = true;
                            }
                            let held: Vec<&str> = t
                                .strings
                                .iter()
                                .filter(|x| x.wire == w.label)
                                .map(|x| x.label.as_str())
                                .collect();
                            let mut at = g
                                .wires
                                .iter()
                                .find(|x| x.0 == w.label)
                                .map_or([0.0; 3], |x| x.1);
                            if held.is_empty() {
                                let mut moved = false;
                                for v in &mut at {
                                    moved |= coord(ui, v);
                                }
                                if moved && Self::put(doc, t, &h, at).is_ok() {
                                    changed = true;
                                }
                            } else {
                                for v in at {
                                    ui.label(format!("{v:.3} m"))
                                        .on_hover_text("Placed by its insulator(s): change their length or direction");
                                }
                            }
                            ui.label(if held.is_empty() {
                                "— (fixed)".to_owned()
                            } else {
                                held.join(", ")
                            });
                            let below: Vec<&str> = t
                                .strings
                                .iter()
                                .filter(|x| x.top == w.label)
                                .map(|x| x.label.as_str())
                                .collect();
                            ui.label(below.join(", "));
                            ui.label(w.set.map_or("—".to_owned(), |s| s.to_string()));
                            let mut cw = w.weight.map_or(0.0, |f| f.si());
                            if ui
                                .add(egui::DragValue::new(&mut cw).range(0.0..=1e6).speed(10.0).max_decimals(0).suffix(" N"))
                                .changed()
                                && let Some(e) = entry(doc, "wire", "label", &w.label)
                            {
                                if cw > 0.0 {
                                    e.insert("weight", toml_edit::value(format!("{cw} N")));
                                } else {
                                    e.remove("weight");
                                }
                                changed = true;
                            }
                            ui.end_row();
                        }
                    });
                }
                Sheet::Sets => {
                    changed |= self.sets_sheet(ui, doc, t);
                }
                Sheet::Strings => {
                    changed |= self.strings_sheet(ui, doc, t, g, insulators);
                }
            });
        changed
    }
}

impl Workspace {
    /// A new insulator from `top` (an attachment point or a wire point):
    /// from the library (`assembly`: its length, weight, stiffness, wind
    /// area; strain when it is a strain assembly) or of `length`, hanging
    /// (or, strain, level along the line: 0°), to `wire` (an existing wire
    /// point: a V with the insulator there; none: a new wire point). Its
    /// label and its wire point's.
    pub fn new_string(
        doc: &mut DocumentMut,
        t: &StructureType,
        top: &str,
        assembly: Option<&ols_structures::InsulatorAssembly>,
        length: f64,
        wire: Option<&str>,
    ) -> Result<(String, String), String> {
        if !Self::labels(t).contains(&top) {
            return Err(format!("no attachment point or wire point {top}"));
        }
        if !(length > 0.0 && length < 50.0) && assembly.is_none() {
            return Err("the insulator length must be between 0 and 50 m".into());
        }
        let wire = match wire {
            Some(w) => w.to_owned(),
            None => {
                let w = fresh(Self::labels(t).into_iter(), "W1");
                let mut table = Table::new();
                table.insert("label", toml_edit::value(w.clone()));
                array(doc, "wire").push(table);
                w
            }
        };
        if wire == top {
            return Err("an insulator cannot end where it hangs from".into());
        }
        let label = fresh(t.strings.iter().map(|x| x.label.as_str()), "S1");
        let mut x = Table::new();
        x.insert("label", toml_edit::value(label.clone()));
        x.insert("top", toml_edit::value(top));
        x.insert("wire", toml_edit::value(wire.clone()));
        match assembly {
            Some(a) => {
                x.insert("insulator", toml_edit::value(a.id.clone()));
                if a.kind == ols_structures::AssemblyKind::Strain {
                    x.insert("strain", toml_edit::value(true));
                    x.insert("angle", toml_edit::value("90 deg"));
                }
            }
            None => {
                x.insert("length", length_item(length));
            }
        }
        array(doc, "string").push(x);
        Ok((label, wire))
    }

    /// Deletes insulator `label`. Its wire point, held by no other
    /// insulator: removed when nothing else is on it, else kept where it
    /// is (fixed: its position written).
    pub fn delete_string(doc: &mut DocumentMut, t: &StructureType, g: &Geometry, label: &str) {
        let Some(x) = t.strings.iter().find(|x| x.label == label) else {
            return;
        };
        if let Some(a) = doc.get_mut("string").and_then(Item::as_array_of_tables_mut) {
            a.retain(|e| e.get("label").and_then(Item::as_str) != Some(label));
        }
        let others = t
            .strings
            .iter()
            .any(|y| y.label != label && y.wire == x.wire);
        if others {
            return;
        }
        let w = t.wires.iter().find(|w| w.label == x.wire);
        let used = w.is_some_and(|w| w.set.is_some()) || t.strings.iter().any(|y| y.top == x.wire);
        if used {
            let at = g.wires.iter().find(|v| v.0 == x.wire).map(|v| v.1);
            if let (Some(at), Some(e)) = (at, entry(doc, "wire", "label", &x.wire)) {
                e.insert("point", point_value(at));
            }
        } else if let Some(a) = doc.get_mut("wire").and_then(Item::as_array_of_tables_mut) {
            a.retain(|e| e.get("label").and_then(Item::as_str) != Some(x.wire.as_str()));
        }
    }

    /// Insulator `label` as an assembly of the insulator library (`.olsi`
    /// text): its length, weight, wind area, strain or suspension (a leg of
    /// a V: a V-string assembly).
    pub fn assembly_text(
        t: &StructureType,
        g: &Geometry,
        insulators: &InsulatorLibrary,
        label: &str,
        id: &str,
    ) -> Result<String, String> {
        let x = t
            .strings
            .iter()
            .find(|x| x.label == label)
            .ok_or_else(|| format!("no insulator {label}"))?;
        let a = x.insulator.as_deref().and_then(|i| insulators.get(i));
        let at = |l: &str| {
            g.points
                .iter()
                .map(|p| (&p.0, p.1))
                .chain(g.wires.iter().map(|w| (&w.0, w.1)))
                .find(|(k, _)| *k == l)
                .map(|(_, p)| p)
        };
        let length = match (a, x.length) {
            (Some(a), _) => a.length.si(),
            (None, Some(l)) => l.si(),
            (None, None) => at(&x.top)
                .zip(at(&x.wire))
                .map(|(p, q)| (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>().sqrt())
                .ok_or("its length is not known")?,
        };
        let legs = t.strings.iter().filter(|y| y.wire == x.wire).count();
        let kind = if legs >= 2 {
            "v_string"
        } else if x.strain {
            "strain"
        } else {
            "suspension"
        };
        let weight = x
            .weight
            .map(|w| w.si())
            .or(a.map(|a| a.weight.si()))
            .unwrap_or(0.0)
            * if legs >= 2 { legs as f64 } else { 1.0 };
        let wind = x
            .wind_area
            .map(|w| w.si())
            .or(a.map(|a| a.wind_area.si()))
            .unwrap_or(0.0)
            * if legs >= 2 { legs as f64 } else { 1.0 };
        let stiffness = a.map_or(5e7, |a| a.axial_stiffness.si());
        let mut text = String::new();
        text.push_str("format = \"ols-insulator\"\nversion = 1\n");
        text.push_str(&format!("id = {}\n", toml_edit::value(id)));
        text.push_str(&format!(
            "name = {}\n",
            toml_edit::value(format!("{id} (from insulator {label} of {})", t.id))
        ));
        text.push_str(&format!("kind = \"{kind}\"\n"));
        text.push_str(&format!("length = \"{} m\"\n", round3(length)));
        text.push_str(&format!("weight = \"{} N\"\n", round3(weight)));
        text.push_str(&format!("axial_stiffness = \"{stiffness} N\"\n"));
        if wind > 0.0 {
            text.push_str(&format!("wind_area = \"{} m²\"\n", round3(wind)));
        }
        if legs >= 2 {
            // The V's half width: from the middle of the legs' tops.
            let tops: Vec<[f64; 3]> = t
                .strings
                .iter()
                .filter(|y| y.wire == x.wire)
                .filter_map(|y| at(&y.top))
                .collect();
            if let [p, q, ..] = tops.as_slice() {
                let half = 0.5 * (p[0] - q[0]).hypot(p[1] - q[1]);
                text.push_str(&format!("half_width = \"{} m\"\n", round3(half)));
            }
        }
        ols_structures::InsulatorAssembly::parse(std::path::Path::new("new.olsi"), &text)
            .map_err(|e| e.message)?;
        Ok(text)
    }

    /// The Insulators sheet: each insulator's top (an attachment point or
    /// another insulator's wire point), wire point, kind (suspension or
    /// strain), assembly from the library (or a length), its direction (an
    /// angle from straight down and one in plan; a V's legs follow where
    /// they meet), weight, wind area, compression, description — all
    /// editable — and New, From library, Save to library, Delete.
    fn strings_sheet(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut DocumentMut,
        t: &StructureType,
        g: &Geometry,
        insulators: &InsulatorLibrary,
    ) -> bool {
        use crate::editors::polar;
        let mut changed = false;
        let ahead = self.ahead;
        let at = |label: &str| {
            g.points
                .iter()
                .map(|p| (&p.0, p.1))
                .chain(g.wires.iter().map(|w| (&w.0, w.1)))
                .find(|(l, _)| *l == label)
                .map(|(_, p)| p)
        };
        let tops: Vec<&str> = t
            .points
            .iter()
            .map(|p| p.label.as_str())
            .chain(t.wires.iter().map(|w| w.label.as_str()))
            .collect();
        let assemblies: Vec<&ols_structures::InsulatorAssembly> = insulators.iter().collect();
        // New, from the library: from the point picked (else the first).
        let picked = self
            .selected
            .iter()
            .find_map(|h| match h {
                Handle::Point(l) | Handle::Wire(l) => Some(l.clone()),
                _ => None,
            })
            .or_else(|| t.points.first().map(|p| p.label.clone()));
        ui.horizontal_wrapped(|ui| {
            let from = picked.clone().unwrap_or_default();
            let can = picked.is_some();
            if ui
                .add_enabled(can, egui::Button::new("+ New insulator"))
                .on_hover_text(format!(
                    "From {from} (the point picked), {:.2} m, hanging, to a new wire point",
                    self.string_length
                ))
                .on_disabled_hover_text("Add an attachment point first")
                .clicked()
            {
                match Self::new_string(doc, t, &from, None, self.string_length, None) {
                    Ok((l, w)) => {
                        self.message = Some((format!("insulator {l} from {from} to {w}"), false));
                        self.selected = vec![Handle::Wire(w)];
                        changed = true;
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            let mut chosen: Option<String> = None;
            ui.add_enabled_ui(can && !assemblies.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("insulator-from-library")
                    .selected_text("From the library…")
                    .height(300.0)
                    .show_ui(ui, |ui| {
                        for a in &assemblies {
                            if ui
                                .selectable_label(false, format!("{} — {}", a.id, a.name))
                                .on_hover_text(format!(
                                    "{:?}: {:.3} m, {:.0} N",
                                    a.kind,
                                    a.length.si(),
                                    a.weight.si()
                                ))
                                .clicked()
                            {
                                chosen = Some(a.id.clone());
                            }
                        }
                    })
                    .response
                    .on_hover_text(format!(
                        "An insulator of the library from {from} (the point picked)"
                    ));
            });
            if let Some(id) = chosen
                && let Some(a) = assemblies.iter().find(|a| a.id == id)
            {
                match Self::new_string(doc, t, &from, Some(a), a.length.si(), None) {
                    Ok((l, w)) => {
                        self.message =
                            Some((format!("insulator {l} ({id}) from {from} to {w}"), false));
                        self.selected = vec![Handle::Wire(w)];
                        changed = true;
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            ui.label("length");
            ui.add(
                egui::DragValue::new(&mut self.string_length)
                    .range(0.05..=50.0)
                    .speed(0.05)
                    .max_decimals(3)
                    .suffix(" m"),
            );
            ui.separator();
            if ui
                .button("Insulator library…")
                .on_hover_text("The insulator assemblies: import, export, edit, delete")
                .clicked()
            {
                self.request = Some(Request::OpenInsulators);
            }
        });
        let mut delete: Option<String> = None;
        let mut save: Option<String> = None;
        egui::Grid::new("type-strings").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("insulator", "Its label"),
                ("hangs from", "An attachment point of the structure, or another insulator's wire point (a TAK)"),
                ("wire point", "Where it ends: a new one, or another insulator's (two make a V)"),
                ("kind", "Suspension: the wire runs through its set. Strain: the wire ends at its set"),
                ("assembly", "An assembly of the insulator library (its length, weight, wind area), or none: a length"),
                ("length", "From its top to its wire point: it places the wire point"),
                ("angle", "From straight down: 0° hanging, 90° level, 180° up"),
                ("direction", "In plan, clockwise seen from above (ahead at the project's angle)"),
                ("weight", "Its weight (default: its assembly's)"),
                ("wind area", "The area the wind blows on (default: its assembly's)"),
                ("compression", "The push it may take (V legs); none: tension only"),
                ("description", "What it is, in words"),
                ("", ""),
            ] {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            for s in &t.strings {
                let Some(e) = entry(doc, "string", "label", &s.label) else {
                    continue;
                };
                let a = s
                    .insulator
                    .as_ref()
                    .and_then(|i| assemblies.iter().find(|a| &a.id == i).copied());
                // Its geometry changed: its wire point is placed again.
                let mut geo = false;
                ui.label(&s.label);
                // Hangs from.
                let mut top = s.top.clone();
                egui::ComboBox::from_id_salt(("string-top", &s.label))
                    .selected_text(&top)
                    .show_ui(ui, |ui| {
                        for l in tops.iter().filter(|l| **l != s.wire) {
                            ui.selectable_value(&mut top, (*l).to_owned(), *l);
                        }
                    });
                if top != s.top {
                    e.insert("top", toml_edit::value(top));
                    changed = true;
                    geo = true;
                }
                // Its wire point: another insulator's makes a V.
                let mut wire = s.wire.clone();
                egui::ComboBox::from_id_salt(("string-wire", &s.label))
                    .selected_text(&wire)
                    .show_ui(ui, |ui| {
                        for w in t.wires.iter().filter(|w| w.label != s.top) {
                            ui.selectable_value(&mut wire, w.label.clone(), &w.label);
                        }
                    });
                if wire != s.wire {
                    e.insert("wire", toml_edit::value(wire));
                    changed = true;
                    geo = true;
                }
                // Kind.
                let mut strain = s.strain;
                egui::ComboBox::from_id_salt(("string-clamp", &s.label))
                    .selected_text(if strain { "strain" } else { "suspension" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut strain, false, "suspension");
                        ui.selectable_value(&mut strain, true, "strain");
                    });
                if strain != s.strain {
                    if strain {
                        e.insert("strain", toml_edit::value(true));
                    } else {
                        e.remove("strain");
                    }
                    changed = true;
                }
                // Assembly.
                let mut pick = s.insulator.clone();
                egui::ComboBox::from_id_salt(("string-insulator", &s.label))
                    .selected_text(pick.as_deref().unwrap_or("— (a length)"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut pick, None, "— (a length)");
                        for a in &assemblies {
                            ui.selectable_value(&mut pick, Some(a.id.clone()), &a.id)
                                .on_hover_text(format!(
                                    "{}: {:.3} m, {:.0} N",
                                    a.name,
                                    a.length.si(),
                                    a.weight.si()
                                ));
                        }
                    });
                let ends = at(&s.top).zip(at(&s.wire));
                let len_now = ends.map_or(0.0, |(a, b)| polar(a, b).2);
                if pick != s.insulator {
                    match pick.as_ref().and_then(|i| assemblies.iter().find(|a| &a.id == i)) {
                        Some(a) => {
                            e.insert("insulator", toml_edit::value(a.id.clone()));
                            for k in ["length", "weight", "wind_area"] {
                                e.remove(k);
                            }
                            if a.kind == ols_structures::AssemblyKind::Strain {
                                e.insert("strain", toml_edit::value(true));
                            } else {
                                e.remove("strain");
                            }
                        }
                        None => {
                            e.remove("insulator");
                            e.insert("length", length_item(len_now.max(0.01)));
                        }
                    }
                    changed = true;
                    geo = true;
                }
                // Length: it places the wire point.
                let mut l = a.map_or(s.length.map_or(len_now, |l| l.si()), |a| a.length.si());
                if a.is_some() {
                    ui.label(format!("{l:.3} m")).on_hover_text("Its assembly's");
                } else if ui
                    .add(egui::DragValue::new(&mut l).range(0.01..=50.0).speed(0.01).max_decimals(3).suffix(" m"))
                    .changed()
                {
                    e.insert("length", length_item(l));
                    changed = true;
                    geo = true;
                }
                // Direction: its own, or (a V's leg) where the legs meet.
                let legs = t.strings.iter().filter(|x| x.wire == s.wire).count();
                if legs == 1 {
                    let (angle, direction) = s.angles();
                    let (mut dn, mut dr) = (angle, (direction + ahead).rem_euclid(360.0));
                    let a1 = ui.add(egui::DragValue::new(&mut dn).range(0.0..=180.0).speed(0.5).max_decimals(2).suffix("°"));
                    let a2 = ui.add(egui::DragValue::new(&mut dr).range(0.0..=360.0).speed(0.5).max_decimals(2).suffix("°"));
                    if a1.changed() || a2.changed() {
                        e.insert("angle", toml_edit::value(format!("{dn} deg")));
                        e.insert("direction", toml_edit::value(format!("{} deg", (dr - ahead).rem_euclid(360.0))));
                        e.remove("side");
                        // Written before without a length: its length now.
                        if s.length.is_none() && a.is_none() {
                            e.insert("length", length_item(len_now.max(0.01)));
                        }
                        changed = true;
                        geo = true;
                    }
                } else {
                    let (dn, dr, _) = ends.map_or((0.0, 0.0, 0.0), |(a, b)| polar(a, b));
                    let why = "A leg of a V: its direction follows where the legs meet (their lengths)";
                    ui.label(format!("{dn:.2}°")).on_hover_text(why);
                    ui.label(format!("{:.2}°", (dr + ahead).rem_euclid(360.0))).on_hover_text(why);
                }
                // Weight, wind area, compression.
                let mut w = s.weight.map(|w| w.si()).or(a.map(|a| a.weight.si())).unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut w).range(0.0..=1e6).speed(1.0).max_decimals(1).suffix(" N")).changed() {
                    e.insert("weight", toml_edit::value(format!("{w} N")));
                    changed = true;
                }
                let mut wa = s.wind_area.map(|w| w.si()).or(a.map(|a| a.wind_area.si())).unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut wa).range(0.0..=100.0).speed(0.005).max_decimals(3).suffix(" m²")).changed() {
                    e.insert("wind_area", toml_edit::value(format!("{wa} m²")));
                    changed = true;
                }
                ui.horizontal(|ui| {
                    let mut push = s.compression.is_some();
                    let mut c = s.compression.map_or(0.0, |c| c.si());
                    let a = ui.checkbox(&mut push, "");
                    let b = ui.add_enabled(push, egui::DragValue::new(&mut c).range(0.0..=1e7).speed(10.0).max_decimals(0).suffix(" N"));
                    if a.changed() || b.changed() {
                        if push {
                            e.insert("compression", toml_edit::value(format!("{c} N")));
                        } else {
                            e.remove("compression");
                        }
                        changed = true;
                    }
                });
                let mut d = s.description.clone();
                if ui.add(egui::TextEdit::singleline(&mut d).desired_width(160.0)).changed() {
                    if d.trim().is_empty() {
                        e.remove("description");
                    } else {
                        e.insert("description", toml_edit::value(d));
                    }
                    changed = true;
                }
                ui.horizontal(|ui| {
                    if ui
                        .small_button("Save to library")
                        .on_hover_text("Its length, weight, wind area and kind as an assembly of the insulator library")
                        .clicked()
                    {
                        save = Some(s.label.clone());
                    }
                    if ui.small_button("🗑").on_hover_text("Delete this insulator").clicked() {
                        delete = Some(s.label.clone());
                    }
                });
                ui.end_row();
                // Its geometry changed: its wire point is placed by it again
                // (a position written before goes).
                if geo {
                    // The other insulators to it (a V) without a length
                    // (written before): their length now, so it can be
                    // placed from them.
                    for x in t.strings.iter().filter(|x| {
                        x.wire == s.wire && x.label != s.label && x.length.is_none() && x.insulator.is_none()
                    }) {
                        let l = at(&x.top)
                            .zip(at(&x.wire))
                            .map_or(0.01, |(p, q)| polar(p, q).2.max(0.01));
                        if let Some(o) = entry(doc, "string", "label", &x.label) {
                            o.insert("length", length_item(l));
                        }
                    }
                    if let Some(w) = entry(doc, "wire", "label", &s.wire) {
                        w.remove("point");
                    }
                }
            }
        });
        if let Some(label) = delete {
            Self::delete_string(doc, t, g, &label);
            self.message = Some((format!("deleted insulator {label}"), false));
            changed = true;
        }
        if let Some(label) = save {
            let id = format!("{}-{label}", t.id);
            match Self::assembly_text(t, g, insulators, &label, &id) {
                Ok(text) => self.request = Some(Request::SaveInsulator { id, text }),
                Err(e) => self.message = Some((e, true)),
            }
        }
        if t.strings.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No insulator yet: pick an attachment point (or add one), then + New insulator or From the library.",
                )
                .weak(),
            );
        }
        changed
    }

    /// The Set points sheet: each set — its number (typed), the wire
    /// point it is on, suspension or strain (from its insulators), a
    /// delete; and a set added on a wire point without one.
    fn sets_sheet(&mut self, ui: &mut egui::Ui, doc: &mut DocumentMut, t: &StructureType) -> bool {
        let mut changed = false;
        let kind = |w: &str| {
            let on: Vec<&ols_structures::TypeString> =
                t.strings.iter().filter(|x| x.wire == w).collect();
            if on.is_empty() {
                "fixed (no insulator)"
            } else if on.iter().any(|x| x.strain) {
                "strain"
            } else {
                "suspension"
            }
        };
        let mut drop_set: Option<String> = None;
        egui::Grid::new("type-sets").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("set", "Its number: sections and jumpers end here (unique on the structure)"),
                ("on wire point", "The wire point it is on"),
                ("kind", "From its insulator: suspension (the wire runs through) or strain (the wire ends)"),
                ("", ""),
            ] {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            let mut sets: Vec<&ols_structures::TypeWire> = t.wires.iter().filter(|w| w.set.is_some()).collect();
            sets.sort_by_key(|w| w.set);
            for w in sets {
                let set = w.set.expect("filtered");
                let mut n = i64::from(set);
                if ui.add(egui::DragValue::new(&mut n).range(1..=99_999)).changed() {
                    let taken = t.wires.iter().any(|x| x.label != w.label && x.set.map(i64::from) == Some(n));
                    if taken {
                        self.message = Some((format!("set {n} is on another wire point already"), true));
                    } else if let Some(e) = entry(doc, "wire", "label", &w.label) {
                        e.insert("set", toml_edit::value(n));
                        changed = true;
                    }
                }
                // Moved to another wire point (one without a set).
                let mut on = w.label.clone();
                egui::ComboBox::from_id_salt(("set-on", set))
                    .selected_text(&on)
                    .show_ui(ui, |ui| {
                        for x in t.wires.iter().filter(|x| x.set.is_none() || x.label == w.label) {
                            ui.selectable_value(&mut on, x.label.clone(), &x.label);
                        }
                    });
                if on != w.label {
                    if let Some(e) = entry(doc, "wire", "label", &w.label) {
                        e.remove("set");
                    }
                    if let Some(e) = entry(doc, "wire", "label", &on) {
                        e.insert("set", toml_edit::value(i64::from(set)));
                    }
                    changed = true;
                }
                ui.label(kind(&w.label));
                if ui.small_button("🗑").on_hover_text("Take the set off (the wire point stays)").clicked() {
                    drop_set = Some(w.label.clone());
                }
                ui.end_row();
            }
        });
        if let Some(l) = drop_set
            && let Some(e) = entry(doc, "wire", "label", &l)
        {
            e.remove("set");
            changed = true;
        }
        // A new set on a wire point without one.
        let free: Vec<&str> = t
            .wires
            .iter()
            .filter(|w| w.set.is_none())
            .map(|w| w.label.as_str())
            .collect();
        let mut add: Option<String> = None;
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!free.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("set-add")
                    .selected_text("+ Set on wire point…")
                    .show_ui(ui, |ui| {
                        for l in &free {
                            if ui.selectable_label(false, *l).clicked() {
                                add = Some((*l).to_owned());
                            }
                        }
                    })
                    .response
                    .on_hover_text("A set where a wire will be strung: its number is the next free one; change it here");
            });
        });
        if let Some(l) = add {
            let next = (1..)
                .find(|s| !t.wires.iter().any(|w| w.set == Some(*s)))
                .expect("a free set");
            if let Some(e) = entry(doc, "wire", "label", &l) {
                e.insert("set", toml_edit::value(i64::from(next)));
                changed = true;
                self.message = Some((format!("set {next} on {l}"), false));
            }
        }
        changed
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn length_item(v: f64) -> Item {
    length(v)
}

/// The compass's radius: beyond the pylon's reach in plan, a third of its
/// height at least.
fn compass_radius(g: &Geometry) -> f64 {
    let pts = || {
        g.joints
            .iter()
            .map(|j| j.1)
            .chain(g.points.iter().map(|p| p.1))
            .chain(g.wires.iter().map(|w| w.1))
            .chain(g.sticks.iter().flatten().copied())
    };
    let reach = pts().map(|p| p[0].hypot(p[1])).fold(3.0, f64::max);
    let (lo, hi) = pts().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
        (a.min(p[2]), b.max(p[2]))
    });
    (reach * 1.25).max(0.35 * (hi - lo).max(0.0))
}

/// The length written for a preset string: `length` for an I-string; for
/// a V the leg that puts the wire point `length` below the middle of the
/// tops.
fn length_or_leg(t: &StructureType, tops: &[String], length: f64) -> Item {
    if tops.len() != 2 {
        return self::length(length);
    }
    let at = |l: &str| {
        t.points
            .iter()
            .find(|p| p.label == l)
            .map(|p| p.position().map(|v| v.si()))
    };
    match (at(&tops[0]), at(&tops[1])) {
        (Some(a), Some(b)) => {
            let half = 0.5 * (0..3).map(|k| (b[k] - a[k]).powi(2)).sum::<f64>().sqrt();
            self::length(half.hypot(length))
        }
        _ => self::length(length),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PYLON: &str = r#"id = "P"
name = "p"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "pinned"

[[node]]
id = "N2"
point = ["0 m", "0 m", "30 m"]

[[node]]
id = "N3"
point = ["-6 m", "0 m", "30 m"]

[[member]]
id = "M1"
nodes = ["N1", "N2"]

[[member]]
id = "M2"
nodes = ["N2", "N3"]

[[point]]
label = "A1"
node = "N3"
"#;

    fn read(doc: &DocumentMut) -> StructureType {
        StructureType::parse(std::path::Path::new("p.olss"), &doc.to_string()).unwrap()
    }

    #[test]
    fn joints_members_points_and_strings_are_added() {
        let mut doc: DocumentMut = PYLON.parse().unwrap();
        let t = read(&doc);
        assert_eq!(t.detail(), ols_structures::Detail::Outline);
        let id = Workspace::add_joint(&mut doc, Some(&t), [6.0, 0.0, 30.0]);
        assert_eq!(id, "N4");
        let t = read(&doc);
        assert_eq!(
            Workspace::add_member(&mut doc, &t, "N2", "N4").unwrap(),
            "M3"
        );
        let t = read(&doc);
        assert!(Workspace::add_member(&mut doc, &t, "N4", "N2").is_err());
        let a2 = Workspace::add_point_at_joint(&mut doc, &t, "N4");
        let t = read(&doc);
        assert_eq!(t.point_local(&a2, None), Some([6.0, 0.0, 30.0]));
        // An I under A1, a V from A1 and A2.
        let w = Workspace::add_strings(&mut doc, &t, &["A1".into()], 4.0).unwrap();
        let t = read(&doc);
        let placed = t
            .place_strings([0.0; 3], 0.0, None, &InsulatorLibrary::default())
            .unwrap();
        let wp = placed.wires.iter().find(|x| x.label == w).unwrap();
        // No set: set numbers are put by hand.
        assert_eq!((wp.position, wp.set), ([-6.0, 0.0, 26.0], None));
        let v = Workspace::add_strings(&mut doc, &t, &["A1".into(), a2], 3.0).unwrap();
        let t = read(&doc);
        let placed = t
            .place_strings([0.0; 3], 0.0, None, &InsulatorLibrary::default())
            .unwrap();
        let vp = placed.wires.iter().find(|x| x.label == v).unwrap();
        assert!((vp.position[2] - 27.0).abs() < 1e-3 && vp.position[0].abs() < 1e-9);
        assert_eq!(vp.set, None);
    }

    #[test]
    fn deleting_a_joint_takes_its_members_points_and_strings() {
        let mut doc: DocumentMut = PYLON.parse().unwrap();
        let t = read(&doc);
        Workspace::add_strings(&mut doc, &t, &["A1".into()], 4.0).unwrap();
        let t = read(&doc);
        let r = Workspace::removal(&t, &Handle::Joint("N3".into()));
        assert_eq!(r.joints, vec!["N3"]);
        assert_eq!(r.members, vec!["M2"]);
        assert_eq!(r.points, vec!["A1"]);
        assert_eq!(r.strings, vec!["S1"]);
        Workspace::remove(&mut doc, &r);
        let t = read(&doc);
        assert_eq!((t.nodes.len(), t.members.len(), t.points.len()), (2, 1, 0));
        assert!(t.strings.is_empty());
    }

    #[test]
    fn moving_a_point_on_a_joint_moves_the_joint() {
        let mut doc: DocumentMut = PYLON.parse().unwrap();
        let t = read(&doc);
        Workspace::put(&mut doc, &t, &Handle::Point("A1".into()), [-7.0, 0.0, 31.0]).unwrap();
        let t = read(&doc);
        assert_eq!(t.point_local("A1", None), Some([-7.0, 0.0, 31.0]));
        assert_eq!(t.nodes[2].point[0].si(), -7.0);
    }

    /// Insulators: a new one from an attachment point (its wire point
    /// placed by its length, hanging), a second to the same wire point (a
    /// V: where their lengths meet), one from the library (strain: level),
    /// saved back to the library, deleted (its wire point with it when
    /// nothing else is on it).
    #[test]
    fn insulators_are_made_from_the_library_saved_and_deleted() {
        let mut doc: DocumentMut = PYLON.parse().unwrap();
        let mut t = read(&doc);
        let a2 = Workspace::add_point_at_joint(&mut doc, &t, "N2");
        t = read(&doc);
        let none = InsulatorLibrary::default();
        let (s1, w1) = Workspace::new_string(&mut doc, &t, "A1", None, 3.0, None).unwrap();
        t = read(&doc);
        let g = geometry(&t, &none);
        let at = |g: &Geometry, l: &str| g.wires.iter().find(|w| w.0 == l).unwrap().1;
        assert_eq!(at(&g, &w1), [-6.0, 0.0, 27.0]);
        // A V: a second insulator to the same wire point, from A2 (6 m away).
        let (_, w) = Workspace::new_string(&mut doc, &t, &a2, None, 5.0, Some(&w1)).unwrap();
        assert_eq!(w, w1);
        // (The first leg 5 m too: they meet 4 m below the middle.)
        let e = entry(&mut doc, "string", "label", &s1).unwrap();
        e.insert("length", length(5.0));
        t = read(&doc);
        let g = geometry(&t, &none);
        let v = at(&g, &w1);
        assert!(
            (v[0] + 3.0).abs() < 1e-9 && (v[2] - 26.0).abs() < 1e-9,
            "{v:?}"
        );
        // From the library: a strain assembly, level (ahead).
        let lib_dir = std::env::temp_dir().join(format!("ols-ws-ins-{}", std::process::id()));
        std::fs::create_dir_all(&lib_dir).unwrap();
        std::fs::write(
            lib_dir.join("D.olsi"),
            "id = \"D\"\nname = \"dead end\"\nkind = \"strain\"\nlength = \"4 m\"\nweight = \"1 kN\"\n",
        )
        .unwrap();
        let lib = InsulatorLibrary::load_dir(&lib_dir).unwrap();
        let d = lib.get("D").unwrap();
        let (s3, w3) = Workspace::new_string(&mut doc, &t, "A1", Some(d), 4.0, None).unwrap();
        t = read(&doc);
        let g = geometry(&t, &lib);
        assert_eq!(at(&g, &w3), [-6.0, 4.0, 30.0]);
        assert!(t.strings.iter().find(|x| x.label == s3).unwrap().strain);
        // Saved to the library: a strain assembly of its length and weight.
        let text = Workspace::assembly_text(&t, &g, &lib, &s3, "D2").unwrap();
        let back = ols_structures::InsulatorAssembly::parse(std::path::Path::new("D2.olsi"), &text)
            .unwrap();
        assert_eq!(back.kind, ols_structures::AssemblyKind::Strain);
        assert!((back.length.si() - 4.0).abs() < 1e-9 && (back.weight.si() - 1000.0).abs() < 1e-9);
        // The V's leg: a V-string assembly, half width from its tops.
        let leg = Workspace::assembly_text(&t, &g, &lib, &s1, "V2").unwrap();
        assert!(
            leg.contains("kind = \"v_string\"") && leg.contains("half_width = \"3 m\""),
            "{leg}"
        );
        // Deleted: its own wire point goes; the V's stays (the other leg).
        Workspace::delete_string(&mut doc, &t, &g, &s3);
        Workspace::delete_string(&mut doc, &t, &g, &s1);
        t = read(&doc);
        assert!(t.wires.iter().all(|w| w.label != w3));
        assert!(t.wires.iter().any(|w| w.label == w1));
        std::fs::remove_dir_all(&lib_dir).ok();
    }
}
