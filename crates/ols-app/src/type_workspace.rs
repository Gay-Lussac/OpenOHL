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
    /// Points.
    Points,
    /// Wire points.
    Wires,
    /// Strings.
    Strings,
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
    /// point) down to a new wire point with the next free set; or, with
    /// two tops, a V: two strings to one wire point `length` below the
    /// middle of the tops. The new wire point's label.
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
        let set = (1..)
            .find(|s| !t.wires.iter().any(|w| w.set == Some(*s)))
            .expect("a free set");
        let mut w = Table::new();
        w.insert("label", toml_edit::value(wire.clone()));
        w.insert("set", toml_edit::value(i64::from(set)));
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
        used_by: Option<usize>,
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
            ui.label(match used_by {
                None => "opened alone (no line)".to_owned(),
                Some(0) => "used by no structure of the line".to_owned(),
                Some(1) => "used by 1 structure of the line (it follows when saved)".to_owned(),
                Some(n) => format!("used by {n} structures of the line (they follow when saved)"),
            });
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
                changed |= self.tables(ui, doc, &t, &g);
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
                ("I", 1, "An I-string from the point picked down to a new wire point (the next set)"),
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
                            self.message = Some((format!("added the strings to wire point {w}"), false));
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
    ) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            for (s, name) in [
                (Sheet::Joints, "Joints"),
                (Sheet::Members, "Members"),
                (Sheet::Points, "Points"),
                (Sheet::Wires, "Wire points"),
                (Sheet::Strings, "Strings"),
            ] {
                ui.selectable_value(&mut self.sheet, s, name);
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
                        for h in ["point", "x", "y", "z", "on"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for p in &t.points {
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
                            ui.end_row();
                        }
                    });
                }
                Sheet::Wires => {
                    egui::Grid::new("type-wires").striped(true).show(ui, |ui| {
                        for h in ["wire point", "set", "x", "y", "z", "placed"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for w in &t.wires {
                            let h = Handle::Wire(w.label.clone());
                            if ui.selectable_label(self.selected.contains(&h), &w.label).clicked() {
                                self.selected = vec![h.clone()];
                                self.dirty = true;
                            }
                            let mut set = w.set.map_or(0, i64::from);
                            if ui
                                .add(egui::DragValue::new(&mut set).range(0..=99_999))
                                .on_hover_text("0: no set (strings meet here, no wire ends here)")
                                .changed()
                                && let Some(e) = entry(doc, "wire", "label", &w.label)
                            {
                                if set == 0 {
                                    e.remove("set");
                                } else {
                                    e.insert("set", toml_edit::value(set));
                                }
                                changed = true;
                            }
                            let mut at = g
                                .wires
                                .iter()
                                .find(|x| x.0 == w.label)
                                .map_or([0.0; 3], |x| x.1);
                            let mut moved = false;
                            for v in &mut at {
                                moved |= coord(ui, v);
                            }
                            if moved && Self::put(doc, t, &h, at).is_ok() {
                                changed = true;
                            }
                            if w.point.is_some() {
                                if ui
                                    .small_button("from its strings")
                                    .on_hover_text("Place it from its strings again")
                                    .clicked()
                                    && let Some(e) = entry(doc, "wire", "label", &w.label)
                                {
                                    e.remove("point");
                                    changed = true;
                                }
                            } else {
                                ui.label("from its strings");
                            }
                            ui.end_row();
                        }
                    });
                }
                Sheet::Strings => {
                    egui::Grid::new("type-strings").striped(true).show(ui, |ui| {
                        for h in [
                            "string",
                            "top",
                            "wire point",
                            "insulator / length",
                            "weight",
                            "wind area",
                            "compression",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for s in &t.strings {
                            ui.label(&s.label);
                            ui.label(&s.top);
                            ui.label(&s.wire);
                            ui.label(match (&s.insulator, s.length) {
                                (Some(i), _) => i.clone(),
                                (None, Some(l)) => format!("{:.3} m", l.si()),
                                _ => "—".into(),
                            });
                            ui.label(s.weight.map_or("—".into(), |w| format!("{:.0} N", w.si())));
                            ui.label(s.wind_area.map_or("—".into(), |w| format!("{:.3} m²", w.si())));
                            ui.label(
                                s.compression
                                    .map_or("tension only".into(), |c| format!("{:.0} N", c.si())),
                            );
                            ui.end_row();
                        }
                    });
                    ui.label(
                        egui::RichText::new(
                            "Pick a point (or two) and use the I / V presets above; the other fields in Fields.",
                        )
                        .weak(),
                    );
                }
            });
        changed
    }
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
        assert_eq!((wp.position, wp.set), ([-6.0, 0.0, 26.0], Some(1)));
        let v = Workspace::add_strings(&mut doc, &t, &["A1".into(), a2], 3.0).unwrap();
        let t = read(&doc);
        let placed = t
            .place_strings([0.0; 3], 0.0, None, &InsulatorLibrary::default())
            .unwrap();
        let vp = placed.wires.iter().find(|x| x.label == v).unwrap();
        assert!((vp.position[2] - 27.0).abs() < 1e-3 && vp.position[0].abs() < 1e-9);
        assert_eq!(vp.set, Some(2));
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
}
