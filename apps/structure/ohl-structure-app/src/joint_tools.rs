// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Defining a tower's joints (owner 2026-11-02, stage 3 of the joint
//! details), in the Joint details panel:
//!
//! - every value of the connections table edited in place (given at that
//!   member end), cleared from its cell's menu (right-click: back to the
//!   member's or worked out);
//! - the **bolt layout** form (in the panel, above the table): n bolts in one or two gauge lines, their
//!   pitch, end and edge distances in one go, with how far they reach and
//!   what does not fit the leg (the minimums the code asks are typed:
//!   none is built in);
//! - a member end's details **copied**: the whole joint to the joints
//!   like it (mirrors included), one end to both ends of every member of
//!   its group or class;
//! - **presets** (`Connections/connection-presets.ohlcp` next to the
//!   structure): an end saved as one, one applied to an end;
//! - **plates** added at the joint, their thickness and material, removed.
//!
//! Every action is one undo step.

use std::path::PathBuf;

use eframe::egui;
use ohl_structure_edit::connection_edits::{
    self, BoltLayout, CopyEnd, FieldValue, PRESETS_EXTENSION, Preset,
};
use ohl_structure_link::{
    Arrival, BoltedLeg, ConnectionKind, JointDetail, JointDetails, Source, Valued,
};

use crate::app::StructureApp;
use ohl_structure_edit::Handle;

/// What a click in the panel asks.
#[derive(Debug, Clone)]
pub enum Act {
    /// A field of a member end given (`Some`) or cleared.
    Field {
        /// Member.
        member: String,
        /// Joint.
        joint: String,
        /// Field.
        field: &'static str,
        /// Value.
        value: Option<FieldValue>,
    },
    /// A member end cleared.
    Clear(String, String),
    /// The bolt layout form opened for a member end.
    OpenLayout(String, String),
    /// A member end's details to both ends of the members of its group
    /// (`true`) or class.
    ToMembers(String, String, bool),
    /// The joint's details to the joints like it.
    ToSimilar(String),
    /// The save-as-preset form opened for a member end.
    OpenPreset(String, String),
    /// A preset applied to a member end.
    ApplyPreset(String, String, String),
    /// A plate added at a joint.
    AddPlate(String),
    /// A plate's thickness (m) or material changed.
    Plate(String, Option<f64>, Option<String>),
    /// A plate removed.
    RemovePlate(String),
    /// A member selected.
    Select(String),
}

/// The bolt layout window's values, millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutEdit {
    /// Member.
    pub member: String,
    /// Joint.
    pub joint: String,
    /// How many bolts.
    pub count: u32,
    /// A bolt of the bolt libraries (its diameter, hole and grade).
    pub bolt: Option<String>,
    /// Diameter, mm.
    pub diameter: f64,
    /// Hole, mm (0: not given).
    pub hole: f64,
    /// Grade.
    pub grade: String,
    /// Gauge lines.
    pub lines: u32,
    /// Gauges, mm.
    pub gauges: [f64; 2],
    /// Pitch, mm.
    pub pitch: f64,
    /// End distance, mm.
    pub end: f64,
    /// Edge distance, mm (0: not given).
    pub edge: f64,
    /// The width of the leg bolted, mm (none: not an angle).
    pub leg: Option<f64>,
}

impl LayoutEdit {
    /// Its values as the file takes them.
    pub fn layout(&self) -> BoltLayout {
        let mm = |v: f64| v / 1000.0;
        BoltLayout {
            count: self.count,
            bolt: self.bolt.clone(),
            diameter: mm(self.diameter),
            hole: (self.hole > 0.0).then(|| mm(self.hole)),
            grade: Some(self.grade.clone()).filter(|g| !g.trim().is_empty()),
            gauges: self.gauges[..self.lines.clamp(1, 2) as usize]
                .iter()
                .map(|g| mm(*g))
                .collect(),
            pitch: mm(self.pitch),
            end: mm(self.end),
            edge: (self.edge > 0.0).then(|| mm(self.edge)),
        }
    }

    /// From what an end shows now (given, the member's, worked out or
    /// assumed alike: a start to change).
    pub fn of(a: &Arrival, joint: &str) -> Self {
        let mm = |v: f64| (v * 1000.0 * 10.0).round() / 10.0;
        let b = a.bolts.value.as_ref();
        let gauges = a.gauges.value.clone().unwrap_or_default();
        let leg = a.profile.as_ref().and_then(|p| p.angle).map(|g| {
            let long = g.b.max(g.h);
            let short = g.b.min(g.h);
            mm(if a.leg.value == Some(BoltedLeg::Short) {
                short
            } else {
                long
            })
        });
        Self {
            member: a.member.clone(),
            joint: joint.to_owned(),
            count: b.map_or(2, |b| b.count),
            bolt: b.and_then(|b| b.bolt.clone()),
            diameter: b.map_or(16.0, |b| mm(b.diameter)),
            hole: b.and_then(|b| b.hole).map_or(0.0, mm),
            grade: b.and_then(|b| b.grade.clone()).unwrap_or_default(),
            lines: a.gauge_lines.value.unwrap_or(1).clamp(1, 2),
            gauges: [
                gauges
                    .first()
                    .map_or(leg.map_or(30.0, |w| w / 2.0), |g| mm(*g)),
                gauges
                    .get(1)
                    .map_or(leg.map_or(60.0, |w| w * 2.0 / 3.0), |g| mm(*g)),
            ],
            pitch: a.pitch.value.map_or(50.0, mm),
            end: a.end_distance.value.map_or(30.0, mm),
            edge: a.edge_distance.value.map_or(0.0, mm),
            leg,
        }
    }
}

/// The joint tools' state.
#[derive(Default)]
pub struct JointTools {
    /// The bolt layout window, open.
    pub layout: Option<LayoutEdit>,
    /// The save-as-preset window: member end and the name typed.
    pub preset: Option<(String, String, String)>,
    /// The presets read: the file and them (or why not).
    presets: Option<(PathBuf, Result<Vec<Preset>, String>)>,
    /// The bolt libraries read (`Bolts/` beside the structure), for the
    /// libraries' stamp and folder; what did not read.
    bolts: Option<(u64, PathBuf, BoltLibs)>,
}

/// The bolt libraries read, and what did not read.
pub type BoltLibs = std::sync::Arc<(ohl_structures::bolts::BoltLibrary, Vec<String>)>;

impl StructureApp {
    /// The presets file of the structure open:
    /// `Connections/connection-presets.ohlcp` next to it.
    pub(crate) fn presets_path(&self) -> Option<PathBuf> {
        let dir = self.file.as_ref()?.path()?.parent()?.to_path_buf();
        Some(
            dir.join("Connections")
                .join(format!("connection-presets.{PRESETS_EXTENSION}")),
        )
    }

    /// The presets (read again when the structure's folder changes or a
    /// preset is saved).
    pub(crate) fn presets(&mut self) -> Vec<Preset> {
        let Some(path) = self.presets_path() else {
            return Vec::new();
        };
        if self
            .joint_tools
            .presets
            .as_ref()
            .is_none_or(|p| p.0 != path)
        {
            let read = match std::fs::read_to_string(&path) {
                Ok(text) => connection_edits::parse_presets(&text)
                    .map_err(|e| format!("{}: {e}", path.display())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(e) => Err(format!("{}: {e}", path.display())),
            };
            self.joint_tools.presets = Some((path, read));
        }
        match &self.joint_tools.presets {
            Some((_, Ok(p))) => p.clone(),
            _ => Vec::new(),
        }
    }

    /// The bolt libraries of the structure open (`Bolts/` beside it),
    /// read again when the libraries change (an import).
    pub(crate) fn bolt_library(&mut self) -> BoltLibs {
        let Some(dirs) = self
            .folders()
            .map(|f| f.read(crate::libraries::LibKind::Bolts))
        else {
            return Default::default();
        };
        let dir = dirs.first().cloned().unwrap_or_default();
        let stamp = self.libs_stamp;
        if let Some((s, d, l)) = &self.joint_tools.bolts
            && *s == stamp
            && *d == dir
        {
            return l.clone();
        }
        let l: BoltLibs = std::sync::Arc::new(ohl_structures::bolts::BoltLibrary::load_all(&dirs));
        self.joint_tools.bolts = Some((stamp, dir, l.clone()));
        l
    }

    /// Why the presets could not be read (none: read, or no file).
    pub(crate) fn presets_error(&self) -> Option<String> {
        match &self.joint_tools.presets {
            Some((_, Err(e))) => Some(e.clone()),
            _ => None,
        }
    }

    /// Does what `acts` ask, each one undo step.
    pub(crate) fn apply_joint_acts(&mut self, acts: Vec<Act>, details: &JointDetails) {
        for act in acts {
            self.apply_joint_act(act, details);
        }
    }

    fn apply_joint_act(&mut self, act: Act, details: &JointDetails) {
        let presets = match &act {
            Act::ApplyPreset(..) => self.presets(),
            _ => Vec::new(),
        };
        let Some(f) = self.file.as_mut() else {
            return;
        };
        let message: Option<(String, bool)> = match act {
            Act::Field {
                member,
                joint,
                field,
                value,
            } => {
                let r = f.edit(&format!("{field} of {member} at {joint}"), |doc, _| {
                    connection_edits::set_connection_field(
                        doc,
                        &member,
                        &joint,
                        field,
                        value.as_ref(),
                    )
                });
                r.err().map(|e| (e, true))
            }
            Act::Clear(member, joint) => {
                f.edit(&format!("Clear {member} at {joint}"), |doc, _| {
                    connection_edits::clear_connection(doc, &member, &joint)
                });
                Some((
                    format!(
                        "{member} at {joint}: back to the member's design data and what is worked out"
                    ),
                    false,
                ))
            }
            Act::OpenLayout(member, joint) => {
                let a = details
                    .joint(&joint)
                    .and_then(|j| j.arrivals.iter().find(|a| a.member == member));
                if let Some(a) = a {
                    self.joint_tools.layout = Some(LayoutEdit::of(a, &joint));
                }
                None
            }
            Act::ToMembers(member, joint, by_group) => {
                let targets = member_targets(f.structure(), details, &member, &joint, by_group);
                let n = targets.len();
                let what = if by_group { "group" } else { "class" };
                f.edit(&format!("{member} at {joint} to its {what}"), |doc, _| {
                    let fields = connection_edits::carried(doc, &member, &joint);
                    for (m, j) in &targets {
                        connection_edits::put_carried(doc, m, j, &fields);
                    }
                });
                Some((
                    format!(
                        "{member} at {joint}: its details on {n} member end(s) of its {what} (what \
                         they bolt onto kept)"
                    ),
                    false,
                ))
            }
            Act::ToSimilar(joint) => {
                let similar = ohl_structure_link::similar_joints(details, &joint);
                let t = f.structure_arc();
                let mut ends = Vec::new();
                for s in &similar {
                    for (from, to) in &s.pairs {
                        let onto = t
                            .connection(from, &joint)
                            .and_then(|c| c.onto.as_ref())
                            .and_then(|o| s.pairs.iter().find(|p| &p.0 == o))
                            .map(|p| p.1.clone());
                        ends.push(CopyEnd {
                            from_member: from.clone(),
                            from_joint: joint.clone(),
                            to_member: to.clone(),
                            to_joint: s.joint.clone(),
                            onto,
                        });
                    }
                }
                f.edit(&format!("Joint {joint} to the joints like it"), |doc, _| {
                    connection_edits::copy_ends(doc, &t, &ends);
                });
                Some((
                    format!(
                        "joint {joint}: its details on {} joint(s) like it ({})",
                        similar.len(),
                        similar
                            .iter()
                            .map(|s| s.joint.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    false,
                ))
            }
            Act::OpenPreset(member, joint) => {
                self.joint_tools.preset = Some((member, joint, String::new()));
                None
            }
            Act::ApplyPreset(member, joint, name) => {
                match presets.iter().find(|p| p.name == name) {
                    Some(p) => {
                        f.edit(
                            &format!("Preset {name} on {member} at {joint}"),
                            |doc, _| connection_edits::put_carried(doc, &member, &joint, &p.fields),
                        );
                        Some((format!("preset {name} on {member} at {joint}"), false))
                    }
                    None => Some((format!("no preset {name}"), true)),
                }
            }
            Act::AddPlate(joint) => {
                let id = f.edit(&format!("Plate at {joint}"), |doc, t| {
                    connection_edits::add_plate(doc, t, &joint, 0.010)
                });
                Some((
                    format!(
                        "plate {id} at {joint}, 10 mm thick: type its thickness; members bolt onto \
                         it with Onto"
                    ),
                    false,
                ))
            }
            Act::Plate(id, thickness, material) => {
                let r = f.edit(&format!("Plate {id}"), |doc, _| {
                    connection_edits::set_plate(doc, &id, thickness, material.as_deref())
                });
                r.err().map(|e| (e, true))
            }
            Act::RemovePlate(id) => {
                f.edit(&format!("Remove plate {id}"), |doc, _| {
                    connection_edits::remove_plate(doc, &id)
                });
                Some((
                    format!("plate {id} removed (what bolted onto it: not given)"),
                    false,
                ))
            }
            Act::Select(member) => {
                self.selected = vec![Handle::Member(member)];
                None
            }
        };
        if message.is_some() {
            self.message = message;
        }
    }

    /// The bolt layout and save-as-preset forms, in the panel above its
    /// table (no window of their own).
    pub(crate) fn joint_forms(&mut self, ui: &mut egui::Ui) {
        self.layout_form(ui);
        self.preset_form(ui);
    }

    fn layout_form(&mut self, ui: &mut egui::Ui) {
        let Some(mut e) = self.joint_tools.layout.take() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Bolt layout — {} at {}", e.member, e.joint))
                    .on_hover_text(
                        "The bolts of this member end in one go. The minimum distances are the \
                         design code's: type them (none is built in).",
                    );
                if ui.small_button("Close").clicked() {
                    open = false;
                }
            });
            let num = |ui: &mut egui::Ui, label: &str, v: &mut f64, tip: &str| {
                ui.label(label).on_hover_text(tip);
                // A field in the row (not right-aligned as in a table).
                ui.scope(|ui| {
                    crate::style::field_visuals(ui);
                    ui.add(
                        egui::DragValue::new(v)
                            .range(0.0..=2000.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                });
            };
            let lib = self.bolt_library();
            ui.horizontal_wrapped(|ui| {
                ui.label("Bolt");
                egui::ComboBox::from_id_salt("bolt-layout-library")
                    .selected_text(e.bolt.clone().unwrap_or_else(|| "typed here".into()))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(e.bolt.is_none(), "typed here")
                            .clicked()
                        {
                            e.bolt = None;
                        }
                        for b in &lib.0.bolts {
                            if ui
                                .selectable_label(
                                    e.bolt.as_deref() == Some(b.id.as_str()),
                                    b.title(),
                                )
                                .on_hover_text(b.id.clone())
                                .clicked()
                            {
                                e.bolt = Some(b.id.clone());
                                e.diameter = b.diameter.si() * 1000.0;
                                e.hole = b.hole.map_or(0.0, |h| h.si() * 1000.0);
                                e.grade = b.grade.clone().unwrap_or_default();
                            }
                        }
                        if lib.0.bolts.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "no bolt library (File › Libraries › Import, Bolt libraries)",
                                )
                                .weak(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(
                        "A bolt of the bolt libraries (Bolts/ beside the structure): its diameter, \
                         hole and grade are the library's",
                    );
            });
            let typed = e.bolt.is_none();
            ui.horizontal_wrapped(|ui| {
                ui.label("Bolts");
                ui.scope(|ui| {
                    crate::style::field_visuals(ui);
                    ui.add(egui::DragValue::new(&mut e.count).range(1..=40))
                });
                ui.add_enabled_ui(typed, |ui| {
                    num(ui, "×  diameter", &mut e.diameter, "The bolt's diameter");
                    num(
                        ui,
                        "hole",
                        &mut e.hole,
                        "The hole's diameter (0: not given)",
                    );
                    ui.label("grade");
                    ui.add(egui::TextEdit::singleline(&mut e.grade).desired_width(60.0))
                        .on_hover_text("As the standard names it (8.8, A394 type 0…)");
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Gauge lines");
                ui.radio_value(&mut e.lines, 1, "1");
                ui.radio_value(&mut e.lines, 2, "2")
                    .on_hover_text("Staggered by half a pitch");
                let [mut g1, mut g2] = e.gauges;
                num(ui, "gauge", &mut g1, "From the heel along the leg bolted");
                if e.lines == 2 {
                    num(ui, "and", &mut g2, "From the heel along the leg bolted");
                }
                e.gauges = [g1, g2];
            });
            ui.horizontal_wrapped(|ui| {
                num(ui, "Pitch", &mut e.pitch, "Between bolts along a line");
                num(
                    ui,
                    "end",
                    &mut e.end,
                    "From the member's end to the first bolt",
                );
                num(
                    ui,
                    "edge",
                    &mut e.edge,
                    "From the bolts to the leg's edge (0: not given)",
                );
            });
            let l = e.layout();
            let misfits = l.misfits(e.leg.map(|w| w / 1000.0));
            for m in &misfits {
                crate::app::error_label(ui, m.clone());
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "The bolts reach {:.0} mm along the member{}.",
                    l.reach() * 1000.0,
                    e.leg
                        .map(|w| format!("; the leg bolted is {w:.0} mm wide"))
                        .unwrap_or_default()
                ));
                if ui
                    .add_enabled(misfits.is_empty(), egui::Button::new("Apply"))
                    .on_hover_text("Written at this member end: one undo step")
                    .clicked()
                {
                    apply = true;
                }
            });
        });
        if apply {
            if let Some(f) = self.file.as_mut() {
                let l = e.layout();
                f.edit(
                    &format!("Bolt layout of {} at {}", e.member, e.joint),
                    |doc, _| connection_edits::set_bolt_layout(doc, &e.member, &e.joint, &l),
                );
                self.message = Some((
                    format!(
                        "bolt layout of {} at {}: {} × M{:.0}",
                        e.member, e.joint, e.count, e.diameter
                    ),
                    false,
                ));
            }
            return;
        }
        if open {
            self.joint_tools.layout = Some(e);
        }
    }

    fn preset_form(&mut self, ui: &mut egui::Ui) {
        let Some((member, joint, mut name)) = self.joint_tools.preset.take() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("Save {member} at {joint} as a preset"));
                if ui.small_button("Close").clicked() {
                    open = false;
                }
            });
            ui.label("What is given at this end (not what it bolts onto, not bolt points).");
            ui.horizontal(|ui| {
                ui.label("Name");
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    save = true;
                }
            });
            if ui
                .add_enabled(!name.trim().is_empty(), egui::Button::new("Save"))
                .clicked()
            {
                save = true;
            }
        });
        if save && !name.trim().is_empty() {
            self.message = Some(match self.save_preset(&member, &joint, name.trim()) {
                Ok(path) => (
                    format!("preset {} saved: {}", name.trim(), path.display()),
                    false,
                ),
                Err(e) => (e, true),
            });
            return;
        }
        if open {
            self.joint_tools.preset = Some((member, joint, name));
        }
    }

    /// Saves `member`'s end at `joint` as preset `name` (replacing one of
    /// that name).
    pub(crate) fn save_preset(
        &mut self,
        member: &str,
        joint: &str,
        name: &str,
    ) -> Result<PathBuf, String> {
        let path = self
            .presets_path()
            .ok_or("save the structure first: presets go next to it")?;
        let f = self.file.as_ref().ok_or("no structure open")?;
        let fields = connection_edits::carried(f.doc(), member, joint);
        if fields.is_empty() {
            return Err(format!(
                "{member} at {joint}: nothing given at this end to save"
            ));
        }
        self.joint_tools.presets = None;
        if let Some(e) = self.presets_error() {
            return Err(e);
        }
        let mut presets = self.presets();
        if let Some(e) = self.presets_error() {
            return Err(e);
        }
        presets.retain(|p| p.name != name);
        presets.push(Preset {
            name: name.to_owned(),
            fields,
        });
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, connection_edits::presets_text(&presets))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        self.joint_tools.presets = None;
        Ok(path)
    }
}

/// Both ends of every member of `member`'s group (or class), but `member`
/// at `joint` itself.
fn member_targets(
    t: &ohl_structures::StructureType,
    details: &JointDetails,
    member: &str,
    joint: &str,
    by_group: bool,
) -> Vec<(String, String)> {
    let class_of = |m: &str| {
        details
            .joints
            .iter()
            .flat_map(|j| j.arrivals.iter())
            .find(|a| a.member == m)
            .and_then(|a| a.class.clone())
    };
    let group = t
        .members
        .iter()
        .find(|m| m.id == member)
        .and_then(|m| m.group.clone());
    let class = class_of(member);
    let mut out = Vec::new();
    for m in &t.members {
        let same = if by_group {
            group.is_some() && m.group == group
        } else {
            class.is_some() && class_of(&m.id) == class
        };
        if !same {
            continue;
        }
        for n in &m.nodes {
            if !(m.id == member && n == joint) {
                out.push((m.id.clone(), n.clone()));
            }
        }
    }
    out
}

/// How many member ends `member`'s group or class has (for the menu).
pub fn targets_count(
    t: &ohl_structures::StructureType,
    details: &JointDetails,
    member: &str,
    joint: &str,
    by_group: bool,
) -> usize {
    member_targets(t, details, member, joint, by_group).len()
}

/// The colour a value is written in, by where it comes from.
pub fn source_color(ui: &egui::Ui, s: Source) -> egui::Color32 {
    let dark = ui.visuals().dark_mode;
    match s {
        Source::Given | Source::Member => ui.visuals().text_color(),
        Source::WorkedOut => {
            if dark {
                egui::Color32::from_rgb(120, 180, 255)
            } else {
                egui::Color32::from_rgb(20, 90, 180)
            }
        }
        Source::Assumed => {
            if dark {
                egui::Color32::from_rgb(235, 170, 60)
            } else {
                egui::Color32::from_rgb(170, 100, 0)
            }
        }
        Source::Missing => ui.visuals().weak_text_color(),
    }
}

fn shown<T>(v: &Valued<T>, text: impl Fn(&T) -> String) -> egui::RichText {
    egui::RichText::new(v.value.as_ref().map_or("—".to_owned(), text))
}

fn tinted<T>(ui: &egui::Ui, v: &Valued<T>, t: egui::RichText) -> egui::RichText {
    let t = t.color(source_color(ui, v.source));
    if v.source == Source::Member {
        t.italics()
    } else {
        t
    }
}

fn mm(x: &f64) -> String {
    let v = (x * 10000.0).round() / 10.0;
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

/// A length typed in mm ("30", "30 mm", "0.03 m"); none: not a length.
fn typed_mm(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Ok(v) = s.parse::<f64>() {
        return (v.is_finite() && v >= 0.0).then_some(v / 1000.0);
    }
    s.parse::<ohl_core::units::Length>()
        .ok()
        .map(|l| l.si())
        .filter(|v| v.is_finite() && *v >= 0.0)
}

/// A choice cell: the value shown (in its colour), its options, "not
/// given" to clear.
fn choice(
    ui: &mut egui::Ui,
    id: (&str, &str, &str),
    shown_text: egui::RichText,
    options: &[String],
    tip: &str,
) -> Option<Option<String>> {
    let mut out = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown_text)
        .width(80.0)
        .show_ui(ui, |ui| {
            for o in options {
                if ui.selectable_label(false, o).clicked() {
                    out = Some(Some(o.clone()));
                }
            }
            ui.separator();
            if ui
                .selectable_label(false, "not given here")
                .on_hover_text("Back to the member's design data or what is worked out")
                .clicked()
            {
                out = Some(None);
            }
        })
        .response
        .on_hover_text(tip);
    out
}

/// A length cell typed in mm; right-click: not given here.
fn length_cell(
    ui: &mut egui::Ui,
    buffers: &mut std::collections::HashMap<String, String>,
    key: String,
    v: &Valued<f64>,
    tip: &str,
) -> Option<Option<f64>> {
    let current = v.value.as_ref().map(mm).unwrap_or_default();
    let mut text = buffers
        .get(&key)
        .cloned()
        .unwrap_or_else(|| current.clone());
    let color = source_color(ui, v.source);
    let r = ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(44.0)
                .text_color(color)
                .hint_text("—"),
        )
        .on_hover_text(format!(
            "{tip} ({}); right-click: not given here",
            v.source.words()
        ));
    let mut out = None;
    if r.lost_focus() {
        buffers.remove(&key);
        if text.trim() != current.trim() {
            out = if text.trim().is_empty() {
                Some(None)
            } else {
                typed_mm(&text).map(Some)
            };
        }
    } else if r.has_focus() {
        buffers.insert(key, text);
    }
    r.context_menu(|ui| {
        if ui.button("Not given here").clicked() {
            out = Some(None);
            ui.close();
        }
    });
    out
}

/// The connections table, editable: one row per member end; the acts
/// asked.
#[allow(clippy::too_many_arguments)]
pub fn table(
    ui: &mut egui::Ui,
    t: &ohl_structures::StructureType,
    details: &JointDetails,
    j: &JointDetail,
    buffers: &mut std::collections::HashMap<String, String>,
    presets: &[Preset],
) -> Vec<Act> {
    let mut acts = Vec::new();
    egui::ScrollArea::horizontal()
        .id_salt("joint-connections")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("joint-connections-grid")
                .striped(true)
                .spacing([8.0, 3.0])
                .show(ui, |ui| {
                    for (h, tip) in [
                        ("Member", "The member (click: select it)"),
                        ("End", "Its first (i) or second (j) joint is this one"),
                        ("Class", "Its class (design data) or group"),
                        ("From", "The face it lies in"),
                        (
                            "Angle",
                            "To the member it bolts onto (or the joint's leg), degrees",
                        ),
                        ("Held", "Bolted, welded, pinned or fixed"),
                        (
                            "Onto",
                            "What it bolts onto: a member meeting here or a plate",
                        ),
                        ("Leg", "The angle's leg bolted"),
                        ("Bolts", "Count × diameter, grade (click: the bolt layout)"),
                        ("Lines", "Gauge lines"),
                        (
                            "Gauge mm",
                            "From the heel along the leg bolted (the first line)",
                        ),
                        ("Pitch mm", "Between bolts along a line"),
                        ("End mm", "From the member's end to the first bolt"),
                        ("Edge mm", "From the bolts to the leg's edge"),
                        ("Eccentric", "Loaded off its axis (one leg bolted)"),
                        (
                            "Restrained",
                            "Held against turning (two bolts or more into a stiffer member)",
                        ),
                        ("e mm", "The eccentricity: the gauge line from the centroid"),
                        ("", ""),
                    ] {
                        ui.strong(h).on_hover_text(tip);
                    }
                    ui.end_row();
                    for a in &j.arrivals {
                        row(ui, t, details, j, a, buffers, presets, &mut acts);
                        ui.end_row();
                    }
                });
        });
    acts
}

#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut egui::Ui,
    t: &ohl_structures::StructureType,
    details: &JointDetails,
    j: &JointDetail,
    a: &Arrival,
    buffers: &mut std::collections::HashMap<String, String>,
    presets: &[Preset],
    acts: &mut Vec<Act>,
) {
    let (m, jid) = (a.member.as_str(), j.id.as_str());
    let field = |field: &'static str, value: Option<FieldValue>| Act::Field {
        member: m.to_owned(),
        joint: jid.to_owned(),
        field,
        value,
    };
    let mut name = egui::RichText::new(m);
    if a.modelling {
        name = name.weak();
    }
    if ui
        .link(name)
        .on_hover_text(if a.modelling {
            "a modelling member (not design-checked); click: select it"
        } else {
            "click: select it"
        })
        .clicked()
    {
        acts.push(Act::Select(m.to_owned()));
    }
    ui.label(if a.end == 0 { "i" } else { "j" });
    ui.label(
        a.class
            .clone()
            .or_else(|| a.group.clone())
            .unwrap_or_else(|| "—".into()),
    );
    ui.label(&a.face);
    match &a.angle {
        Some((to, deg)) => ui
            .label(format!("{deg:.1}°"))
            .on_hover_text(format!("to {to}")),
        None => ui.label("—"),
    };
    // Held.
    let kinds: Vec<String> = ConnectionKind::ALL
        .iter()
        .map(|k| k.as_str().to_owned())
        .collect();
    if let Some(v) = choice(
        ui,
        ("held", m, jid),
        tinted(ui, &a.kind, shown(&a.kind, |k| k.as_str().to_owned())),
        &kinds,
        a.kind.source.words(),
    ) {
        acts.push(field("kind", v.map(FieldValue::Word)));
    }
    // Onto: the members meeting here, the plates here.
    let ontos: Vec<String> = j
        .arrivals
        .iter()
        .filter(|b| b.member != a.member)
        .map(|b| b.member.clone())
        .chain(j.plates.iter().map(|p| p.id.clone()))
        .collect();
    if let Some(v) = choice(
        ui,
        ("onto", m, jid),
        tinted(ui, &a.onto, shown(&a.onto, |o| o.clone())),
        &ontos,
        a.onto.source.words(),
    ) {
        acts.push(field("onto", v.map(FieldValue::Word)));
    }
    // Leg.
    let legs: Vec<String> = [
        BoltedLeg::Long,
        BoltedLeg::Short,
        BoltedLeg::Both,
        BoltedLeg::None,
    ]
    .iter()
    .map(|l| l.as_str().to_owned())
    .collect();
    if let Some(v) = choice(
        ui,
        ("leg", m, jid),
        tinted(ui, &a.leg, shown(&a.leg, |l| l.as_str().to_owned())),
        &legs,
        a.leg.source.words(),
    ) {
        acts.push(field("leg", v.map(FieldValue::Word)));
    }
    // Bolts: the layout window.
    let bolts = tinted(
        ui,
        &a.bolts,
        shown(&a.bolts, |b| {
            let mut s = format!("{} × M{:.0}", b.count, b.diameter * 1000.0);
            if let Some(g) = &b.grade {
                s += &format!(" {g}");
            }
            s
        }),
    );
    let r = ui
        .add(egui::Button::new(bolts).small())
        .on_hover_text(format!(
            "{} — click: the bolt layout; right-click: not given here",
            a.bolts.source.words()
        ));
    if r.clicked() {
        acts.push(Act::OpenLayout(m.to_owned(), jid.to_owned()));
    }
    r.context_menu(|ui| {
        if ui.button("Not given here").clicked() {
            acts.push(field("bolts", None));
            ui.close();
        }
    });
    // Gauge lines.
    let lines: Vec<String> = vec!["1".into(), "2".into()];
    if let Some(v) = choice(
        ui,
        ("lines", m, jid),
        tinted(ui, &a.gauge_lines, shown(&a.gauge_lines, |n| n.to_string())),
        &lines,
        a.gauge_lines.source.words(),
    ) {
        acts.push(field(
            "gauge_lines",
            v.and_then(|s| s.parse().ok()).map(FieldValue::Count),
        ));
    }
    // The first gauge (both in the layout window).
    let gauge = Valued {
        value: a.gauges.value.as_ref().and_then(|g| g.first().copied()),
        source: a.gauges.source,
    };
    if let Some(v) = length_cell(ui, buffers, format!("jg-{m}-{jid}"), &gauge, "Gauge") {
        let lines = a.gauge_lines.value.unwrap_or(1).max(1) as usize;
        acts.push(field(
            "gauges",
            v.map(|g| {
                let mut all = a.gauges.value.clone().unwrap_or_default();
                all.resize(lines, g);
                all[0] = g;
                FieldValue::Lengths(all)
            }),
        ));
    }
    for (key, v, f, tip) in [
        ("jp", &a.pitch, "pitch", "Pitch"),
        ("je", &a.end_distance, "end_distance", "End distance"),
        ("jd", &a.edge_distance, "edge_distance", "Edge distance"),
    ] {
        if let Some(x) = length_cell(ui, buffers, format!("{key}-{m}-{jid}"), v, tip) {
            acts.push(field(f, x.map(FieldValue::Length)));
        }
    }
    let flags: Vec<String> = vec!["yes".into(), "no".into()];
    for (key, v, f) in [
        ("ecc", &a.eccentric, "eccentric"),
        ("rest", &a.restrained, "restrained"),
    ] {
        if let Some(x) = choice(
            ui,
            (key, m, jid),
            tinted(
                ui,
                v,
                shown(v, |b| if *b { "yes" } else { "no" }.to_owned()),
            ),
            &flags,
            v.source.words(),
        ) {
            acts.push(field(f, x.map(|s| FieldValue::Flag(s == "yes"))));
        }
    }
    if let Some(x) = length_cell(
        ui,
        buffers,
        format!("jx-{m}-{jid}"),
        &a.eccentricity,
        "Eccentricity",
    ) {
        acts.push(field("eccentricity", x.map(FieldValue::Length)));
    }
    // The end's menu.
    ui.menu_button("…", |ui| {
        if ui.button("Bolt layout…").clicked() {
            acts.push(Act::OpenLayout(m.to_owned(), jid.to_owned()));
            ui.close();
        }
        let given = t.connection(m, jid).is_some();
        if ui
            .add_enabled(given, egui::Button::new("Clear this end"))
            .on_hover_text("Back to the member's design data and what is worked out")
            .on_disabled_hover_text("Nothing given at this end")
            .clicked()
        {
            acts.push(Act::Clear(m.to_owned(), jid.to_owned()));
            ui.close();
        }
        ui.separator();
        for (by_group, what) in [(true, "group"), (false, "class")] {
            let n = targets_count(t, details, m, jid, by_group);
            let name = if by_group {
                t.members
                    .iter()
                    .find(|x| x.id == m)
                    .and_then(|x| x.group.clone())
            } else {
                a.class.clone()
            };
            let label = match &name {
                Some(g) => format!("Same on every end of {what} {g} ({n})"),
                None => format!("Same on every end of its {what} (none)"),
            };
            if ui
                .add_enabled(given && n > 0, egui::Button::new(label))
                .on_hover_text(
                    "What is given at this end, on both ends of every member of the same \
                     group or class (what they bolt onto and their bolt points kept)",
                )
                .on_disabled_hover_text(
                    "Give something at this end first; a group or class with other members",
                )
                .clicked()
            {
                acts.push(Act::ToMembers(m.to_owned(), jid.to_owned(), by_group));
                ui.close();
            }
        }
        ui.separator();
        if ui
            .add_enabled(given, egui::Button::new("Save as a preset…"))
            .on_disabled_hover_text("Nothing given at this end")
            .clicked()
        {
            acts.push(Act::OpenPreset(m.to_owned(), jid.to_owned()));
            ui.close();
        }
        ui.menu_button("Apply a preset", |ui| {
            if presets.is_empty() {
                ui.label(egui::RichText::new("no presets yet (Save as a preset…)").weak());
            }
            for p in presets {
                if ui.button(&p.name).clicked() {
                    acts.push(Act::ApplyPreset(
                        m.to_owned(),
                        jid.to_owned(),
                        p.name.clone(),
                    ));
                    ui.close();
                }
            }
        });
    });
}

/// The joint's own tools: the joints like it, its plates.
pub fn joint_bar(
    ui: &mut egui::Ui,
    details: &JointDetails,
    j: &JointDetail,
    buffers: &mut std::collections::HashMap<String, String>,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let similar = ohl_structure_link::similar_joints(details, &j.id);
    ui.horizontal_wrapped(|ui| {
        let r = ui
            .add_enabled(
                !similar.is_empty(),
                egui::Button::new(format!("Copy to the {} joint(s) like it", similar.len())),
            )
            .on_hover_text(format!(
                "Every member end's connection of this joint on the joints with the same members \
                 (cross-section, class, angle within 1°; mirrors included), their connections \
                 replaced, plates copied: {}",
                similar
                    .iter()
                    .map(|s| s.joint.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .on_disabled_hover_text("No joint with the same members");
        if r.clicked() {
            acts.push(Act::ToSimilar(j.id.clone()));
        }
        if ui
            .button("+ Plate")
            .on_hover_text("A gusset plate at this joint (10 mm to start: type its thickness)")
            .clicked()
        {
            acts.push(Act::AddPlate(j.id.clone()));
        }
    });
    if !j.plates.is_empty() {
        egui::Grid::new("joint-plates")
            .striped(true)
            .show(ui, |ui| {
                for h in ["Plate", "Thickness mm", "Material", "Members onto it", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for p in &j.plates {
                    ui.label(&p.id);
                    let v = Valued {
                        value: Some(p.thickness),
                        source: Source::Given,
                    };
                    if let Some(Some(x)) =
                        length_cell(ui, buffers, format!("pt-{}", p.id), &v, "Thickness")
                    {
                        acts.push(Act::Plate(p.id.clone(), Some(x), None));
                    }
                    if let Some(text) = crate::tables::text_cell(
                        ui,
                        buffers,
                        format!("pm-{}", p.id),
                        p.material.as_deref().unwrap_or(""),
                        "—",
                        "Its material (empty: not given)",
                    ) {
                        acts.push(Act::Plate(p.id.clone(), None, Some(text)));
                    }
                    ui.label(if p.members.is_empty() {
                        "—".to_owned()
                    } else {
                        p.members.join(", ")
                    });
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Remove the plate")
                        .clicked()
                    {
                        acts.push(Act::RemovePlate(p.id.clone()));
                    }
                    ui.end_row();
                }
            });
    }
    acts
}
