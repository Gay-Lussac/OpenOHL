// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Defining a tower's joints (owner 2026-11-02, stage 3 of the joint
//! details), in the Joint details panel:
//!
//! - every value of the connections table edited in place (given at that
//!   member end), cleared from its cell's menu (right-click: back to the
//!   member's or worked out);
//! - the **bolt layout** form (in the panel, above the table): n bolts in one to three gauge lines, their
//!   pitch, end and edge distances in one go, with how far they reach,
//!   what does not fit the leg and what breaks the design code's minimum
//!   distances ([`ohl_structure_link::bolt_rules`]);
//! - a member end's details **copied**: the whole joint to the joints
//!   like it (mirrors included), one end to both ends of every member of
//!   its group or class;
//! - **presets** (`Connections/connection-presets.ohlcp` next to the
//!   structure): an end saved as one, one applied to an end;
//! - **plates** added at the joint, their thickness and material, removed;
//! - where a member end **sits** on the member it bolts onto (outside,
//!   inside, nested: a lap joint), a member **continuous** through the
//!   joint into another (one part), **splices** (covers on the members'
//!   legs: two on a leg, two shear planes), 2026-11-05.
//!
//! Every action is one undo step.

use std::path::PathBuf;

use eframe::egui;
use ohl_structure_edit::connection_edits::{
    self, BoltLayout, CopyEnd, FieldValue, PRESETS_EXTENSION, Preset,
};
use ohl_structure_link::{
    Arrival, BoltRules, BoltedLeg, ConnectionKind, JointDetail, JointDetails, Source, Valued,
};

use ohl_structures::connections::{CoverFace, CoverLeg, Face, MAX_GAUGE_LINES};

use crate::app::StructureApp;
use crate::detail_tools::Rules;
use crate::joint_sketch as sketch;
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
    /// Two plates of a joint welded to each other (true) or not
    /// (2026-11-04: a welded plate is several plates welded together).
    Weld(String, String, bool),
    /// A weld's size, length or kind (plates `a` and `b`; 2026-11-04,
    /// `[[plate.weld]]`).
    WeldData(
        String,
        String,
        ohl_structure_edit::connection_edits::WeldEdit,
    ),
    /// A welded plate made at a joint: a plate per plane of the members
    /// arriving, welded together, every member bolted onto one.
    WeldedPlate(String),
    /// A member's roll, radians (owner 2026-11-04: its correction angle).
    Roll(String, f64),
    /// A bolt picked with the Detail view's Edit tool: its properties
    /// below (member, joint, its place among the end's bolts).
    PickBolt(String, String, usize),
    /// A plate picked with the Edit tool: its properties below.
    PickPlate(String),
    /// A plate's thickness (m) or material changed.
    Plate(String, Option<f64>, Option<String>),
    /// A plate removed.
    RemovePlate(String),
    /// A member selected.
    Select(String),
    /// A joint selected.
    SelectJoint(String),
    /// A member end's bolts as their places (a bolt added or taken away:
    /// the Detail view's tools), the bolts' kind when it gives none yet.
    BoltPoints(
        String,
        String,
        Vec<[f64; 2]>,
        Option<ohl_structure_edit::connection_edits::FieldValue>,
    ),
    /// A member end's bolts on each leg and the leg it bolts by (a bolt
    /// moved to the other leg, 2026-11-08), the bolts' kind when it gives
    /// none yet.
    LegBolts {
        /// The member.
        member: String,
        /// The joint.
        joint: String,
        /// On the leg bolted.
        bolted: Vec<[f64; 2]>,
        /// On the other leg.
        other: Vec<[f64; 2]>,
        /// The leg it bolts by (`long`, `short`, `both`; none: as it is).
        leg: Option<&'static str>,
        /// The bolts' kind.
        bolts: Option<ohl_structure_edit::connection_edits::FieldValue>,
    },
    /// A member end's cut and shift, m (the Member end tool).
    CutShift(String, String, f64, f64),
    /// A plate's outline written as drawn.
    PlateOutline(String, Vec<[f64; 3]>),
    /// A plate's outline corrected by hand (its corners, the plate moved;
    /// none: taken out, drawn round its bolts again), `what` the undo
    /// step's name.
    Outline {
        /// The plate.
        id: String,
        /// The outline, structure's axes, m; none: automatic.
        outline: Option<Vec<[f64; 3]>>,
        /// The undo step.
        what: String,
    },
    /// A bolt that fits only on a new gauge line: the line offered, to be
    /// confirmed (the bolts with it, the bolts' kind, the gauge, m, and the
    /// code's minimums in words).
    OfferLine {
        /// Member.
        member: String,
        /// Joint.
        joint: String,
        /// The end's bolts with the new one.
        points: Vec<[f64; 2]>,
        /// The bolts' kind when the end gives none yet.
        bolts: Option<ohl_structure_edit::connection_edits::FieldValue>,
        /// The new line's gauge, m.
        gauge: f64,
        /// The code's minimums, in words.
        words: String,
    },
    /// Something to say (an error: true).
    Say(String, bool),
    /// Member ends made one part (`on`) or bolted again, each (member,
    /// joint, the member it runs on into, on); one undo step for all.
    Continuous(Vec<(String, String, String, bool)>),
    /// A splice added at a joint joining these members.
    AddSplice(String, Vec<String>),
    /// A splice's cover on a face of a leg given a thickness, m (made when
    /// missing), or removed (none).
    Cover(String, CoverFace, Option<CoverLeg>, Option<f64>),
    /// A splice removed.
    RemoveSplice(String),
}

/// A splice's covers start this thick, m (2026-11-05: type their
/// thickness).
pub const COVER_THICKNESS: f64 = 0.008;

/// A new gauge line offered by the bolt tools, waiting for the user.
pub type Offer = Act;

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
    pub gauges: [f64; 3],
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
            gauges: self.gauges[..self.lines.clamp(1, MAX_GAUGE_LINES) as usize]
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
            lines: a.gauge_lines.value.unwrap_or(1).clamp(1, MAX_GAUGE_LINES),
            gauges: [
                gauges
                    .first()
                    .map_or(leg.map_or(30.0, |w| w / 2.0), |g| mm(*g)),
                gauges
                    .get(1)
                    .map_or(leg.map_or(60.0, |w| w * 2.0 / 3.0), |g| mm(*g)),
                gauges
                    .get(2)
                    .map_or(leg.map_or(90.0, |w| w * 4.0 / 5.0), |g| mm(*g)),
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
    /// A new gauge line offered by Add / Move bolt ([`Act::OfferLine`]).
    pub offer: Option<Offer>,
    /// The design code's bolt rules per member end, and the state they
    /// were worked out for (the file, the libraries, the analysis).
    rules: std::collections::HashMap<(String, String), (u64, Option<BoltRules>)>,
    /// Why there are no bolt rules when the structure's method is a
    /// plugin's that gives no minimum distances, and the file's revision
    /// it was asked for.
    no_rules: Option<(u64, Option<String>)>,
    /// The Continuous members window: (joint, member, the other, one part
    /// now, ticked).
    pub continuous: Option<Vec<ContinuousRow>>,
}

/// A row of the Continuous members window: joint, member, the other, one
/// part now, ticked.
pub type ContinuousRow = (String, String, String, bool, bool);

/// The bolt libraries read, and what did not read.
pub type BoltLibs = std::sync::Arc<(ohl_structures::bolts::BoltLibrary, Vec<String>)>;

impl StructureApp {
    /// The largest axial force of `member` over the last analysis' cases,
    /// N (none: no analysis, or the member not in it).
    pub(crate) fn largest_force(&self, member: &str) -> Option<f64> {
        let run = self.results.results.as_ref()?.as_ref().ok()?;
        run.cases
            .iter()
            .filter_map(|c| c.fe.as_ref()?.as_ref().ok())
            .filter_map(|r| r.members.iter().find(|m| m.id == member))
            .map(|m| m.axial.abs())
            .reduce(f64::max)
    }

    /// "no minimum distances from the plugin: …" when the structure's
    /// member check is a plugin's method and that plugin gives none (the
    /// bolt tools then say so, and never show another code's); asked
    /// again when the file changes.
    pub(crate) fn no_bolt_rules(&mut self) -> Option<String> {
        let f = self.file.as_ref()?;
        let rev = f.revision();
        match &self.joint_tools.no_rules {
            Some((r, why)) if *r == rev => why.clone(),
            _ => {
                let why = ohl_structure_link::no_bolt_rules(f.text_ref());
                self.joint_tools.no_rules = Some((rev, why.clone()));
                why
            }
        }
    }

    /// The design code's bolt rules of the member ends `ends` (member,
    /// joint), worked out again when the file, the libraries or the
    /// analysis change.
    pub(crate) fn bolt_rules_of(&mut self, ends: &[(String, String)]) -> Rules {
        let mut out = Rules::new();
        let Some(rev) = self.file.as_ref().map(|f| f.revision()) else {
            return out;
        };
        let bolts = self.bolt_library();
        for (m, j) in ends {
            let force = self.largest_force(m);
            let key = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (
                    rev,
                    self.libs_stamp,
                    self.results.generation,
                    force.map(f64::to_bits),
                )
                    .hash(&mut h);
                h.finish()
            };
            let cached = self.joint_tools.rules.get(&(m.clone(), j.clone()));
            let r = match cached {
                Some((k, r)) if *k == key => r.clone(),
                _ => {
                    let f = self.file.as_ref().expect("checked above");
                    let r = ohl_structure_link::bolt_rules(
                        f.text_ref(),
                        &self.loads.sections,
                        &bolts.0,
                        m,
                        j,
                        force,
                    );
                    self.joint_tools
                        .rules
                        .insert((m.clone(), j.clone()), (key, r.clone()));
                    r
                }
            };
            if let Some(r) = r {
                out.insert((m.clone(), j.clone()), r);
            }
        }
        out
    }

    /// The window asking to confirm a new gauge line (Add / Move bolt).
    pub(crate) fn offer_window(&mut self, ctx: &egui::Context, details: &JointDetails) {
        let Some(Act::OfferLine {
            member,
            joint,
            points,
            bolts,
            gauge,
            words,
        }) = self.joint_tools.offer.clone()
        else {
            return;
        };
        let mut answer: Option<bool> = None;
        egui::Window::new("New gauge line?")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "{member} at {joint}: the bolt has no room on the gauge lines near where \
                     it was put. A new gauge line at {:.0} mm from the heel keeps the code's \
                     distances:",
                    gauge * 1000.0
                ));
                ui.label(egui::RichText::new(&words).small());
                ui.horizontal(|ui| {
                    if ui.button("Add the line and the bolt").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        answer = Some(false);
                    }
                });
            });
        match answer {
            Some(true) => {
                self.joint_tools.offer = None;
                self.apply_joint_acts(vec![Act::BoltPoints(member, joint, points, bolts)], details);
            }
            Some(false) => self.joint_tools.offer = None,
            None => {}
        }
    }

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
                // A kind given to an end running on as one part: both
                // ends bolted again first (the same undo step).
                let was = (field == "kind")
                    .then(|| {
                        f.structure()
                            .continuous_with(&member, &joint)
                            .map(str::to_owned)
                    })
                    .flatten();
                let r = f.edit(&format!("{field} of {member} at {joint}"), |doc, t| {
                    if let Some(o) = &was {
                        connection_edits::set_continuous(doc, t, &member, &joint, o, false)?;
                    }
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
            Act::Continuous(ends) => {
                let n = ends.len();
                let what = match ends.as_slice() {
                    [(m, j, o, true)] => format!("{m} continuous into {o} at {j}"),
                    [(m, j, o, false)] => format!("{m} and {o} bolted again at {j}"),
                    _ => format!("Continuous members ({n})"),
                };
                let r = f.edit(&what, |doc, t| {
                    let mut said = Vec::new();
                    for (m, j, o, on) in &ends {
                        if let Err(e) = connection_edits::set_continuous(doc, t, m, j, o, *on) {
                            said.push(format!("{m} at {j}: {e}"));
                        }
                    }
                    said
                });
                Some(if r.is_empty() {
                    (format!("{}: one undo step", what.to_lowercase()), false)
                } else {
                    (r.join("; "), true)
                })
            }
            Act::AddSplice(joint, members) => {
                // Angles: covers on both legs; else on the flat part.
                let angles = details.joint(&joint).is_some_and(|j| {
                    j.arrivals
                        .iter()
                        .filter(|a| members.contains(&a.member))
                        .all(|a| a.profile.as_ref().is_some_and(|p| p.angle.is_some()))
                });
                let legs: Vec<CoverLeg> = if angles {
                    vec![CoverLeg::Long, CoverLeg::Short]
                } else {
                    Vec::new()
                };
                let id = f.edit(&format!("Splice at {joint}"), |doc, t| {
                    connection_edits::add_splice(doc, t, &joint, &members, &legs, COVER_THICKNESS)
                });
                Some((
                    format!(
                        "splice {id} at {joint} joining {}: covers {:.0} mm outer and inner on                          {} (two shear planes); each member end bolts onto it with its own bolts",
                        members.join(", "),
                        COVER_THICKNESS * 1000.0,
                        if angles { "both legs" } else { "its flat part" }
                    ),
                    false,
                ))
            }
            Act::Cover(id, face, leg, thickness) => {
                let r = f.edit(&format!("Cover of splice {id}"), |doc, _| {
                    connection_edits::set_cover(doc, &id, face, leg, thickness)
                });
                r.err().map(|e| (e, true))
            }
            Act::RemoveSplice(id) => {
                f.edit(&format!("Remove splice {id}"), |doc, _| {
                    connection_edits::remove_splice(doc, &id)
                });
                Some((
                    format!("splice {id} removed (what bolted onto it: not given)"),
                    false,
                ))
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
                self.joints.picked = Some(crate::joint_panel::Picked::Plate(id.clone()));
                Some((
                    format!(
                        "plate {id} at {joint}, 10 mm thick: type its thickness; members bolt \
                         onto it with Onto"
                    ),
                    false,
                ))
            }
            Act::Weld(a, b, on) => {
                let r = f.edit(&format!("Plates {a} and {b} welded"), |doc, t| {
                    connection_edits::weld_pair(doc, t, &a, &b, on)
                });
                Some(match r {
                    Ok(()) if on => (format!("plates {a} and {b}: welded together"), false),
                    Ok(()) => (format!("plates {a} and {b}: no longer welded"), false),
                    Err(e) => (e, true),
                })
            }
            Act::WeldData(a, b, edit) => {
                let r = f.edit(&format!("Weld {a}–{b}"), |doc, t| {
                    connection_edits::set_weld(doc, t, &a, &b, edit)
                });
                r.err().map(|e| (e, true))
            }
            Act::WeldedPlate(joint) => match details.joint(&joint) {
                Some(j) => {
                    let (plan, left) = ohl_structure_link::welded_plan(j);
                    if plan.is_empty() {
                        Some((
                            format!(
                                "joint {joint}: no member bolting onto the joint's leg{}",
                                if left.is_empty() {
                                    String::new()
                                } else {
                                    format!(" ({})", left.join("; "))
                                }
                            ),
                            true,
                        ))
                    } else {
                        let groups: Vec<Vec<String>> =
                            plan.iter().map(|p| p.members.clone()).collect();
                        let thickness = j
                            .plates
                            .iter()
                            .map(|p| p.thickness)
                            .fold(0.010f64, f64::max);
                        let ids = f.edit(&format!("Welded plate at {joint}"), |doc, t| {
                            connection_edits::make_welded_plates(doc, t, &joint, &groups, thickness)
                        });
                        let what: Vec<String> = ids
                            .iter()
                            .zip(&groups)
                            .map(|(id, g)| format!("{id} ({})", g.join(", ")))
                            .collect();
                        Some((
                            format!(
                                "joint {joint}: a welded plate of {} plate(s) {:.0} mm, welded \
                                 together: {}{}",
                                ids.len(),
                                thickness * 1000.0,
                                what.join(", "),
                                if left.is_empty() {
                                    String::new()
                                } else {
                                    format!("; {}", left.join("; "))
                                }
                            ),
                            false,
                        ))
                    }
                }
                None => Some((format!("no joint {joint}"), true)),
            },
            Act::Roll(member, roll) => {
                f.edit(&format!("Roll of {member}"), |doc, _| {
                    ohl_structure_edit::edits::set_member_roll(doc, &member, roll)
                });
                Some((
                    format!(
                        "{member}: roll {:.1}° (its cross-section turned about its axis, its \
                         axes for the analysis too)",
                        roll.to_degrees()
                    ),
                    false,
                ))
            }
            Act::PickBolt(member, joint, index) => {
                self.joints.picked = Some(crate::joint_panel::Picked::Bolt {
                    member,
                    joint,
                    index,
                });
                self.focus_details();
                None
            }
            Act::PickPlate(id) => {
                self.joints.picked = Some(crate::joint_panel::Picked::Plate(id));
                self.focus_details();
                None
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
            Act::SelectJoint(joint) => {
                self.selected = vec![Handle::Joint(joint)];
                None
            }
            Act::BoltPoints(member, joint, points, bolts) => {
                let n = points.len();
                // Both legs bolted: the other leg's bolts kept where they
                // are (the face view edits the leg bolted).
                let other: Option<Vec<[f64; 2]>> = details
                    .joint(&joint)
                    .and_then(|j| j.arrivals.iter().find(|a| a.member == member))
                    .filter(|a| !a.other_flat.is_empty())
                    .map(|a| a.other_flat.clone());
                f.edit(&format!("Bolts of {member} at {joint}"), |doc, _| {
                    connection_edits::set_bolt_points(
                        doc,
                        &member,
                        &joint,
                        &points,
                        other.as_deref(),
                        bolts.as_ref(),
                    )
                });
                Some((
                    format!(
                        "{member} at {joint}: {n} bolt(s) where placed (its layout now their \
                         places; the bolt layout form makes a regular one again)"
                    ),
                    false,
                ))
            }
            Act::LegBolts {
                member,
                joint,
                bolted,
                other,
                leg,
                bolts,
            } => {
                let (n, m) = (bolted.len(), other.len());
                f.edit(&format!("Bolts of {member} at {joint}"), |doc, _| {
                    connection_edits::set_bolt_points(
                        doc,
                        &member,
                        &joint,
                        &bolted,
                        Some(&other),
                        bolts.as_ref(),
                    );
                    if let Some(l) = leg {
                        let _ = connection_edits::set_connection_field(
                            doc,
                            &member,
                            &joint,
                            "leg",
                            Some(&FieldValue::Word(l.to_owned())),
                        );
                    }
                    // One leg only: no second plate for the other leg.
                    if m == 0 {
                        let _ = connection_edits::set_connection_field(
                            doc,
                            &member,
                            &joint,
                            "other_onto",
                            None,
                        );
                    }
                });
                Some((
                    if m == 0 {
                        format!(
                            "{member} at {joint}: {n} bolt(s), one leg ({})",
                            leg.unwrap_or("as it was")
                        )
                    } else {
                        format!("{member} at {joint}: both legs bolted, {n} + {m} bolt(s)")
                    },
                    false,
                ))
            }
            Act::CutShift(member, joint, cut, shift) => {
                f.edit(&format!("End of {member} at {joint}"), |doc, _| {
                    let _ = connection_edits::set_connection_field(
                        doc,
                        &member,
                        &joint,
                        "cut",
                        Some(&FieldValue::Length(cut)),
                    );
                    let value = (shift != 0.0).then_some(FieldValue::SignedLength(shift));
                    let _ = connection_edits::set_connection_field(
                        doc,
                        &member,
                        &joint,
                        "shift",
                        value.as_ref(),
                    );
                });
                Some((
                    format!(
                        "{member} at {joint}: steel stops {:.0} mm from the work point, shift \
                         {:+.0} mm{}",
                        cut * 1000.0,
                        shift * 1000.0,
                        if shift != 0.0 {
                            " — its force passes beside the work point (an eccentricity)"
                        } else {
                            ""
                        }
                    ),
                    false,
                ))
            }
            act @ Act::OfferLine { .. } => {
                self.joint_tools.offer = Some(act);
                None
            }
            Act::Say(text, error) => Some((text, error)),
            Act::PlateOutline(id, outline) => {
                let r = f.edit(&format!("Outline of plate {id}"), |doc, _| {
                    connection_edits::set_plate_outline(doc, &id, &outline)
                });
                Some(match r {
                    Ok(()) => (
                        format!("plate {id}: its outline written around its bolts"),
                        false,
                    ),
                    Err(e) => (e, true),
                })
            }
            Act::Outline { id, outline, what } => {
                let r = f.edit(&what, |doc, _| match &outline {
                    Some(o) => connection_edits::set_plate_outline(doc, &id, o),
                    None => connection_edits::clear_plate_outline(doc, &id),
                });
                Some(match r {
                    Ok(()) if outline.is_none() => (
                        format!("plate {id}: drawn round its bolts again (its outline worked out)"),
                        false,
                    ),
                    Ok(()) => (format!("plate {id}: {}", what.to_lowercase()), false),
                    Err(e) => (e, true),
                })
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
        let rules = self
            .bolt_rules_of(&[(e.member.clone(), e.joint.clone())])
            .remove(&(e.member.clone(), e.joint.clone()));
        let no_rules = if rules.is_none() {
            self.no_bolt_rules()
        } else {
            None
        };
        let mut open = true;
        let mut apply = false;
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Bolt layout — {} at {}", e.member, e.joint))
                    .on_hover_text(
                        "The bolts of this member end in one go, checked against the minimum \
                         distances of the design code of the capacity method (the simple \
                         check has none).",
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
            // A value that may be left out: "not given" instead of a false
            // 0.0 mm (UI review 2026-11-11); typed 0 or cleared: not given.
            let optional = |ui: &mut egui::Ui, label: &str, v: &mut f64, tip: &str| {
                ui.label(label).on_hover_text(tip);
                ui.scope(|ui| {
                    crate::style::field_visuals(ui);
                    ui.add(
                        egui::DragValue::new(v)
                            .range(0.0..=2000.0)
                            .speed(0.5)
                            .custom_formatter(|x, _| {
                                if x == 0.0 {
                                    "not given".to_owned()
                                } else {
                                    format!("{x:.1} mm")
                                }
                            })
                            .custom_parser(|s| {
                                let s = s.trim().trim_end_matches("mm").trim();
                                if s.is_empty() || s == "not given" {
                                    Some(0.0)
                                } else {
                                    s.parse().ok()
                                }
                            }),
                    )
                    .on_hover_text(tip)
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
                                    "no bolt library (File › Import › Bolt libraries)",
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
                    optional(
                        ui,
                        "hole",
                        &mut e.hole,
                        "The hole's diameter. Not given: nothing is written for it at this end \
                         (a library bolt brings its own hole; else the member's design data).",
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
                    .on_hover_text("The second staggered by half a pitch");
                ui.radio_value(&mut e.lines, 3, "3")
                    .on_hover_text("The middle one staggered by half a pitch");
                let [mut g1, mut g2, mut g3] = e.gauges;
                num(ui, "gauge", &mut g1, "From the heel along the leg bolted");
                if e.lines >= 2 {
                    num(ui, "and", &mut g2, "From the heel along the leg bolted");
                }
                if e.lines >= 3 {
                    num(ui, "and", &mut g3, "From the heel along the leg bolted");
                }
                e.gauges = [g1, g2, g3];
            });
            ui.horizontal_wrapped(|ui| {
                num(ui, "Pitch", &mut e.pitch, "Between bolts along a line");
                num(
                    ui,
                    "end",
                    &mut e.end,
                    "From the member's end to the first bolt",
                );
                optional(
                    ui,
                    "edge",
                    &mut e.edge,
                    "From the bolts to the leg's edge. Not given: nothing is written for it at \
                     this end (the table's Edge distance then shows where its value comes from).",
                );
            });
            let l = e.layout();
            let misfits = l.misfits(e.leg.map(|w| w / 1000.0));
            for m in &misfits {
                crate::app::error_label(ui, m.clone());
            }
            // The design code's minimums, and what of the layout breaks
            // them (the bolts laid out as the joints lay them: every second
            // line staggered by half a pitch; from the steel's end).
            if let Some(why) = &no_rules {
                crate::app::error_label(
                    ui,
                    format!(
                        "{why} \u{2014} the layout is not checked against minimum distances \
                         (NOT ON THE SAFE SIDE)"
                    ),
                );
            }
            if let Some(r) = &rules {
                ui.label(egui::RichText::new(r.words()).small());
                let lines = l.gauges.len().max(1) as u32;
                let pts: Vec<[f64; 2]> = (0..l.count)
                    .map(|k| {
                        let line = (k % lines) as usize;
                        let stagger = if line % 2 == 1 { 0.5 } else { 0.0 };
                        let along = l.end + (f64::from(k / lines) + stagger) * l.pitch;
                        [along, l.gauges.get(line).copied().unwrap_or(0.0)]
                    })
                    .collect();
                let mut said: Vec<String> = Vec::new();
                for b in ohl_structure_link::breaches(r, &pts, 0.0) {
                    let w = format!("bolt {}: {}", b.bolt + 1, b.what);
                    if !said.contains(&w) {
                        said.push(w);
                    }
                }
                for w in said {
                    crate::app::error_label(ui, w);
                }
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
/// Bolts placed by hand (along the member, across the leg; none when the
/// connection gives no places): their gauges (the distinct places across,
/// mm, "30" or "30/60") and the spacing between neighbours on a line
/// (smallest to largest, mm: "50", "50\u{2013}60"; one bolt per line: "\u{2014}").
pub(crate) fn hand_spacing(points: &[[ohl_core::units::Length; 2]]) -> Option<(String, String)> {
    if points.is_empty() {
        return None;
    }
    // To 0.1 mm.
    let tenth = |l: ohl_core::units::Length| (l.si() * 1e4).round() as i64;
    let mut lines: std::collections::BTreeMap<i64, Vec<i64>> = Default::default();
    for p in points {
        lines.entry(tenth(p[1])).or_default().push(tenth(p[0]));
    }
    let show = |v: i64| {
        let x = v as f64 / 10.0;
        if x.fract() == 0.0 {
            format!("{x:.0}")
        } else {
            format!("{x:.1}")
        }
    };
    let gauges = lines.keys().map(|g| show(*g)).collect::<Vec<_>>().join("/");
    let mut steps: Vec<i64> = Vec::new();
    for along in lines.values_mut() {
        along.sort_unstable();
        steps.extend(along.windows(2).map(|w| w[1] - w[0]));
    }
    let pitch = match (steps.iter().min(), steps.iter().max()) {
        (Some(a), Some(b)) if a == b => show(*a),
        (Some(a), Some(b)) => format!("{}\u{2013}{}", show(*a), show(*b)),
        _ => "\u{2014}".to_owned(),
    };
    Some((gauges, pitch))
}

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
    typed_mm_signed(s).filter(|v| *v >= 0.0)
}

/// A length typed in mm that may be negative (a shift).
pub(crate) fn typed_mm_signed(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Ok(v) = s.parse::<f64>() {
        return v.is_finite().then_some(v / 1000.0);
    }
    s.parse::<ohl_core::units::Length>()
        .ok()
        .map(|l| l.si())
        .filter(|v| v.is_finite())
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
    length_cell_signed(ui, buffers, key, v, tip, false)
}

/// A length cell that may take a negative value (`signed`: a shift).
fn length_cell_signed(
    ui: &mut egui::Ui,
    buffers: &mut std::collections::HashMap<String, String>,
    key: String,
    v: &Valued<f64>,
    tip: &str,
    signed: bool,
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
            } else if signed {
                typed_mm_signed(&text).map(Some)
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
    let rows: Vec<(&JointDetail, &Arrival)> = j.arrivals.iter().map(|a| (j, a)).collect();
    rows_table(ui, t, details, &rows, false, buffers, presets)
}

/// A connections table of `rows` (a joint and one of its member ends):
/// the first column the member (`by_joint` false: a joint's table) or the
/// joint (a member's table, [`crate::member_panel`]).
pub fn rows_table(
    ui: &mut egui::Ui,
    t: &ohl_structures::StructureType,
    details: &JointDetails,
    rows: &[(&JointDetail, &Arrival)],
    by_joint: bool,
    buffers: &mut std::collections::HashMap<String, String>,
    presets: &[Preset],
) -> Vec<Act> {
    let mut acts = Vec::new();
    egui::ScrollArea::horizontal()
        .id_salt(("joint-connections", by_joint))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new(("joint-connections-grid", by_joint))
                .striped(true)
                .spacing([8.0, 3.0])
                .show(ui, |ui| {
                    for (h, tip) in [
                        if by_joint {
                            ("Joint", "The joint at this end (click: select it)")
                        } else {
                            ("Member", "The member (click: select it)")
                        },
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
                            "What it bolts onto: a member meeting here, a plate or a splice",
                        ),
                        ("Face", FACE_TIP),
                        ("Leg", "The angle's leg bolted"),
                        ("Bolts", "Count × diameter, grade (click: the bolt layout)"),
                        ("Planes", PLANES_TIP),
                        // The values drawn in "What the values mean"
                        // (2026-11-04): the same names and hover texts.
                        (sketch::LINES.header, sketch::LINES.tip),
                        (sketch::GAUGE.header, sketch::GAUGE.tip),
                        (sketch::PITCH.header, sketch::PITCH.tip),
                        (sketch::END.header, sketch::END.tip),
                        (sketch::EDGE.header, sketch::EDGE.tip),
                        ("Eccentric", "Loaded off its axis (one leg bolted)"),
                        (
                            "Restrained",
                            "Held against turning (two bolts or more into a stiffer member)",
                        ),
                        (sketch::ECC.header, sketch::ECC.tip),
                        (sketch::STOPS.header, sketch::STOPS.tip),
                        (sketch::SHIFT.header, sketch::SHIFT.tip),
                        ("", ""),
                    ] {
                        ui.strong(h).on_hover_text(tip);
                    }
                    ui.end_row();
                    for (j, a) in rows {
                        row(ui, t, details, j, a, by_joint, buffers, presets, &mut acts);
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
    by_joint: bool,
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
    if by_joint && ui.link(jid).on_hover_text("click: select it").clicked() {
        acts.push(Act::SelectJoint(jid.to_owned()));
    }
    let mut name = egui::RichText::new(m);
    if a.modelling {
        name = name.weak();
    }
    if !by_joint
        && ui
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
    // Held (continuous: into a member in line of the same section only).
    acts.extend(held_cell(ui, t, j, a));
    // Onto: the members meeting here, the plates and splices here.
    let ontos: Vec<String> = j
        .arrivals
        .iter()
        .filter(|b| b.member != a.member)
        .map(|b| b.member.clone())
        .chain(j.plates.iter().map(|p| p.id.clone()))
        .chain(j.splices.iter().map(|s| s.id.clone()))
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
    // Where it sits on the member it bolts onto.
    let onto_member = a
        .onto
        .value
        .as_ref()
        .is_some_and(|o| j.arrivals.iter().any(|b| &b.member == o));
    if onto_member && a.continuous.is_none() {
        let faces: Vec<String> = Face::ALL.iter().map(|f| f.as_str().to_owned()).collect();
        let mut text = tinted(ui, &a.sits, shown(&a.sits, |f| f.as_str().to_owned()));
        if a.misfit {
            text = text.color(ui.visuals().error_fg_color);
        }
        if let Some(v) = choice(
            ui,
            ("face", m, jid),
            text,
            &faces,
            &format!("{} — {FACE_TIP}", a.sits.source.words()),
        ) {
            acts.push(field("face", v.map(FieldValue::Word)));
        }
    } else {
        ui.label("—").on_hover_text(if a.continuous.is_some() {
            "One part through the joint: it sits nowhere"
        } else {
            "Onto a plate or a splice: on its face (a face is chosen onto a member)"
        });
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
    ui.vertical(|ui| {
        if let Some(v) = choice(
            ui,
            ("leg", m, jid),
            tinted(ui, &a.leg, shown(&a.leg, |l| l.as_str().to_owned())),
            &legs,
            a.leg.source.words(),
        ) {
            acts.push(field("leg", v.map(FieldValue::Word)));
        }
        // Both legs bolted: the other leg onto a plate of its own
        // (2026-11-08: two plates at 90°, apart or welded).
        let plates: Vec<String> = j
            .plates
            .iter()
            .map(|p| p.id.clone())
            .filter(|p| a.onto.value.as_ref() != Some(p))
            .collect();
        if a.leg.value == Some(BoltedLeg::Both) && !plates.is_empty() {
            let text = match &a.other_onto {
                Some(o) => egui::RichText::new(format!("other leg: {o}")),
                None => egui::RichText::new("other leg: same").weak(),
            };
            if let Some(v) = choice(
                ui,
                ("other-onto", m, jid),
                text.small(),
                &plates,
                "The plate the other leg bolts onto (a second plate at 90°, apart or welded \
                 to the first); not given: the same as Onto",
            ) {
                acts.push(field("other_onto", v.map(FieldValue::Word)));
            }
        }
    });
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
    // The shear planes, worked out from the plies (given with the bolts
    // wins).
    let (planes, why) = &a.planes;
    ui.label(tinted(ui, planes, shown(planes, |n| n.to_string())))
        .on_hover_text(match why {
            Some(w) => format!("{}: {w} — {PLANES_TIP}", planes.source.words()),
            None => format!("{} — {PLANES_TIP}", planes.source.words()),
        });
    // Gauge lines.
    let lines: Vec<String> = (1..=MAX_GAUGE_LINES).map(|n| n.to_string()).collect();
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
    // Bolts placed by hand (the Detail view's tools): the picture draws
    // those places, so the table says them too — never the layout's gauge
    // and pitch beside a picture that shows others (UI review 2026-11-11).
    let by_hand = t
        .connection(m, jid)
        .map(|c| hand_spacing(&c.bolt_points))
        .filter(|s| s.is_some())
        .flatten();
    if let Some((gauges, pitch)) = &by_hand {
        let tip = "The bolts of this end were placed by hand (the Detail view's bolt tools): \
                   these are their real places, as drawn. The bolt layout (click Bolts) lays \
                   them out regularly again.";
        ui.label(egui::RichText::new(format!("{gauges} by hand")).italics())
            .on_hover_text(format!("Across the leg, from the heel, mm. {tip}"));
        ui.label(egui::RichText::new(format!("{pitch} by hand")).italics())
            .on_hover_text(format!(
                "Between neighbouring bolts on a line, smallest to largest, mm. {tip}"
            ));
    }
    // The first gauge (both in the layout window).
    let gauge = Valued {
        value: a.gauges.value.as_ref().and_then(|g| g.first().copied()),
        source: a.gauges.source,
    };
    if by_hand.is_some() {
        // Said above.
    } else if let Some(v) = length_cell(ui, buffers, format!("jg-{m}-{jid}"), &gauge, "Gauge") {
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
        if key == "jp" && by_hand.is_some() {
            continue;
        }
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
    if let Some(x) = length_cell(
        ui,
        buffers,
        format!("jc-{m}-{jid}"),
        &a.cut,
        "Where the steel stops",
    ) {
        acts.push(field("cut", x.map(FieldValue::Length)));
    }
    let shift = Valued {
        value: (a.shift.0 != 0.0).then_some(a.shift.0),
        source: if a.shift.0 != 0.0 {
            Source::Given
        } else {
            Source::Missing
        },
    };
    if let Some(x) = length_cell_signed(
        ui,
        buffers,
        format!("js-{m}-{jid}"),
        &shift,
        "Sideways shift",
        true,
    ) {
        acts.push(field(
            "shift",
            x.filter(|v| *v != 0.0).map(FieldValue::SignedLength),
        ));
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

/// The joint's own tools: the joints like it, its plates and their welds.
pub fn joint_bar(
    ui: &mut egui::Ui,
    t: &ohl_structures::StructureType,
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
            .on_hover_text(
                "A gusset plate on this joint's leg (10 mm to start: type its thickness); a joint \
                 may have several, welded together with Welded to",
            )
            .clicked()
        {
            acts.push(Act::AddPlate(j.id.clone()));
        }
        let (plan, _) = ohl_structure_link::welded_plan(j);
        let r = ui
            .add_enabled(
                !plan.is_empty(),
                egui::Button::new("Make a welded plate for every member"),
            )
            .on_hover_text(format!(
                "The joint's plates replaced by a welded plate: one plate per plane the members \
                 arriving lie in with the leg, welded together, each member bolted onto its plate \
                 — {} — each plate round its members' bolts with the design code's end and edge \
                 distances in the plate. One undo step.",
                plan.iter()
                    .map(|p| p.members.join(" + "))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ))
            .on_disabled_hover_text("No member bolting onto the joint's leg here");
        if r.clicked() {
            acts.push(Act::WeldedPlate(j.id.clone()));
        }
        acts.extend(add_splice_menu(ui, t, j));
    });
    acts.extend(splices_box(ui, j, buffers));
    if !j.plates.is_empty() {
        egui::Grid::new("joint-plates")
            .striped(true)
            .show(ui, |ui| {
                for h in [
                    "Plate",
                    "Welded to",
                    "Thickness mm",
                    "Material",
                    "Members onto it",
                    "",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for p in &j.plates {
                    if ui
                        .link(&p.id)
                        .on_hover_text("Its properties and edges above")
                        .clicked()
                    {
                        acts.push(Act::PickPlate(p.id.clone()));
                    }
                    acts.extend(welded_to_menu(ui, j, p));
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
                    if p.outline.len() < 3 {
                        // Nothing bolted onto it, no outline given
                        // (2026-11-04): nothing drawn, no bolt clamps it.
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("none: {}", ohl_structure_link::NO_OUTLINE),
                        )
                        .on_hover_text(
                            "Set a member end's Onto to this plate (the connections table \
                             below): the plate is then drawn round its bolts, and their grip \
                             takes its thickness",
                        );
                    } else {
                        ui.label(if p.members.is_empty() {
                            "—".to_owned()
                        } else {
                            p.members.join(", ")
                        })
                        .on_hover_text(format!(
                            "Their bolts clamp the plate: {:.0} mm of their grip",
                            p.thickness * 1000.0
                        ));
                    }
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
        acts.extend(weld_rows(ui, t, j, None, buffers));
    }
    acts
}

/// The welds of joint `j` (only those of plate `only`, when given): a row
/// per pair of plates welded to each other with the weld's kind, size
/// (the fillet's throat a) and length (none: the seam's, shown as worked
/// out); each change one undo step (2026-11-04, `[[plate.weld]]`).
pub fn weld_rows(
    ui: &mut egui::Ui,
    t: &ohl_structures::StructureType,
    j: &JointDetail,
    only: Option<&str>,
    buffers: &mut std::collections::HashMap<String, String>,
) -> Vec<Act> {
    use ohl_structure_edit::connection_edits::WeldEdit;
    use ohl_structures::connections::WeldKind;
    let mut acts = Vec::new();
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    for (i, p) in j.plates.iter().enumerate() {
        for q in &p.welded_to {
            let k = j.plates.iter().position(|x| &x.id == q);
            if k.is_some_and(|k| k > i) && only.is_none_or(|o| o == p.id || o == q.as_str()) {
                pairs.push((p.id.as_str(), q.as_str()));
            }
        }
    }
    if pairs.is_empty() {
        return acts;
    }
    let seam = |a: &str, b: &str| {
        j.plates
            .iter()
            .flat_map(|p| p.welds.iter().map(move |w| (p.id.as_str(), w)))
            .find(|(p, w)| (*p == a && w.with == b) || (*p == b && w.with == a))
            .map(|(_, w)| w.seam_length())
    };
    ui.push_id(("welds", only), |ui| {
        egui::Grid::new("joint-welds").striped(true).show(ui, |ui| {
            for h in ["Weld", "Kind", "Size a mm", "Length mm", ""] {
                ui.strong(h);
            }
            ui.end_row();
            for (a, b) in pairs {
                let w = t.weld_between(a, b);
                ui.label(format!("{a} – {b}"));
                let kind = w.map(|w| w.kind()).unwrap_or_default();
                let mut k = kind;
                egui::ComboBox::from_id_salt(("weld-kind", a, b))
                    .selected_text(k.words())
                    .show_ui(ui, |ui| {
                        for x in WeldKind::ALL {
                            ui.selectable_value(&mut k, x, x.words());
                        }
                    });
                if k != kind {
                    acts.push(Act::WeldData(
                        a.to_owned(),
                        b.to_owned(),
                        WeldEdit {
                            kind: Some(k),
                            ..WeldEdit::default()
                        },
                    ));
                }
                let size = Valued {
                    value: w.and_then(|w| w.size).map(|v| v.si()),
                    source: if w.is_some_and(|w| w.size.is_some()) {
                        Source::Given
                    } else {
                        Source::Missing
                    },
                };
                if let Some(v) = length_cell(
                    ui,
                    buffers,
                    format!("weld-a-{a}-{b}"),
                    &size,
                    "The fillet's throat a (SIA 263 6.3.2.6); empty: not given, the weld not \
                     checked",
                ) {
                    acts.push(Act::WeldData(
                        a.to_owned(),
                        b.to_owned(),
                        WeldEdit {
                            size: Some(v),
                            ..WeldEdit::default()
                        },
                    ));
                }
                let given = w.and_then(|w| w.length).map(|v| v.si());
                let length = match (given, seam(a, b)) {
                    (Some(l), _) => Valued {
                        value: Some(l),
                        source: Source::Given,
                    },
                    (None, Some(s)) => Valued {
                        value: Some(s),
                        source: Source::WorkedOut,
                    },
                    (None, None) => Valued {
                        value: None,
                        source: Source::Missing,
                    },
                };
                if let Some(v) = length_cell(
                    ui,
                    buffers,
                    format!("weld-l-{a}-{b}"),
                    &length,
                    "Its whole length (both sides together when welded on both); not given: \
                     the seam's length where the plates meet, one side",
                ) {
                    acts.push(Act::WeldData(
                        a.to_owned(),
                        b.to_owned(),
                        WeldEdit {
                            length: Some(v),
                            ..WeldEdit::default()
                        },
                    ));
                }
                ui.label(
                    egui::RichText::new(match (seam(a, b), w.and_then(|w| w.size)) {
                        (None, _) => "the plates do not meet: no seam".to_owned(),
                        (Some(_), None) if kind == WeldKind::Fillet => {
                            "no size: not checked".to_owned()
                        }
                        (Some(s), _) => format!("seam {:.0} mm", s * 1000.0),
                    })
                    .small()
                    .weak(),
                );
                ui.end_row();
            }
        });
    });
    acts
}

/// A plate's Welded to: an options menu (it stays open on a tick) with a
/// box per other plate of the joint, ticked when the two are welded to
/// each other; what was ticked or unticked.
pub fn welded_to_menu(
    ui: &mut egui::Ui,
    j: &JointDetail,
    p: &ohl_structure_link::PlateDetail,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let others: Vec<&ohl_structure_link::PlateDetail> =
        j.plates.iter().filter(|q| q.id != p.id).collect();
    let title = if p.welded_to.is_empty() {
        "not welded".to_owned()
    } else {
        p.welded_to.join(", ")
    };
    if others.is_empty() {
        ui.label("—").on_hover_text("No other plate at the joint");
        return acts;
    }
    ui.push_id(("welded-to", &p.id), |ui| {
        crate::layout::options_menu(ui, &title, |ui| {
            ui.label(
                egui::RichText::new(
                    "Welded to (one welded plate: several plates welded together so that every \
                     member bolting here reaches one)",
                )
                .small()
                .weak(),
            );
            for q in &others {
                let mut on = p.welded_to.contains(&q.id);
                if ui.checkbox(&mut on, q.id.as_str()).changed() {
                    acts.push(Act::Weld(p.id.clone(), q.id.clone(), on));
                }
            }
        });
    });
    acts
}

/// What the Face column means (its header's and cells' hover).
pub const FACE_TIP: &str = "Where it sits on the member it bolts onto: outside — on the outer \
     face of that member's leg (the heel's side); inside — on its inner face; nested — an angle \
     heel into heel inside the other (a lap joint, one shear plane; its heel taken as ground to \
     clear the other's root fillet; it must fit: each leg at most the other's less its thickness \
     and root radius, the end and edge distances kept on both parts). Its effect: the member \
     drawn on that face, the bolts through both legs; nested: the other runs on past the work \
     point over the bolts. Not given: worked out from how it is turned. Red: it does not fit \
     as given (said under the table)";

/// What the Planes column means.
pub const PLANES_TIP: &str = "Shear planes per bolt: worked out from the plies the bolts pass \
     through — a splice's covers on the leg, one per cover (two: double shear, the bolts' shear \
     capacity twice); lapped or nested onto a member or a plate, one. Given with the bolts \
     (shear_planes) wins, said when it differs";

/// The Held cell of member end `a`: bolted, welded, pinned, fixed, or
/// continuous into a member meeting here (offered only when the two can be
/// one part: in line within 1°, the same section, roll and reference —
/// otherwise greyed, the reason on hover), written on both ends.
fn held_cell(
    ui: &mut egui::Ui,
    t: &ohl_structures::StructureType,
    j: &JointDetail,
    a: &Arrival,
) -> Vec<Act> {
    let (m, jid) = (a.member.as_str(), j.id.as_str());
    let mut acts = Vec::new();
    let field = |value: Option<FieldValue>| Act::Field {
        member: m.to_owned(),
        joint: jid.to_owned(),
        field: "kind",
        value,
    };
    let text = match &a.continuous {
        Some(o) => egui::RichText::new(format!("continuous into {o}"))
            .color(source_color(ui, Source::Given)),
        None => tinted(ui, &a.kind, shown(&a.kind, |k| k.as_str().to_owned())),
    };
    egui::ComboBox::from_id_salt(("held", m, jid))
        .selected_text(text)
        .width(80.0)
        .show_ui(ui, |ui| {
            for k in ConnectionKind::ALL
                .iter()
                .filter(|k| **k != ConnectionKind::Continuous)
            {
                if ui.selectable_label(false, k.as_str()).clicked() {
                    acts.push(field(Some(FieldValue::Word(k.as_str().to_owned()))));
                }
            }
            ui.separator();
            for b in j.arrivals.iter().filter(|b| b.member != a.member) {
                let why = t.continuous_problem(m, &b.member, jid);
                let on = a.continuous.as_deref() == Some(b.member.as_str());
                let r = ui
                    .add_enabled(
                        why.is_none(),
                        egui::Button::selectable(on, format!("continuous into {}", b.member)),
                    )
                    .on_hover_text(format!(
                        "One part through the joint: {m} runs on into {} — no cut, no shift, no \
                         bolts here; the analysis joins the two rigidly; one solid drawn; one \
                         part in the lists. Both ends written, one undo step",
                        b.member
                    ))
                    .on_disabled_hover_text(why.unwrap_or_default());
                if r.clicked() && !on {
                    acts.push(Act::Continuous(vec![(
                        m.to_owned(),
                        jid.to_owned(),
                        b.member.clone(),
                        true,
                    )]));
                }
            }
            if let Some(o) = &a.continuous
                && ui
                    .selectable_label(false, "not continuous (bolted again)")
                    .on_hover_text("Both ends' continuous kind removed: one undo step")
                    .clicked()
            {
                acts.push(Act::Continuous(vec![(
                    m.to_owned(),
                    jid.to_owned(),
                    o.clone(),
                    false,
                )]));
            }
            ui.separator();
            if ui
                .selectable_label(false, "not given here")
                .on_hover_text("Back to the member's design data or what is worked out")
                .clicked()
            {
                acts.push(field(None));
            }
        })
        .response
        .on_hover_text(a.kind.source.words());
    acts
}

/// The "+ Splice" options menu of joint `j`: a tick box per member meeting
/// here (stays open on a tick; at first the members in line with another
/// of the same section), and Add: a splice joining the ticked, covers 8
/// mm outer and inner on every leg.
fn add_splice_menu(
    ui: &mut egui::Ui,
    t: &ohl_structures::StructureType,
    j: &JointDetail,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let key = egui::Id::new(("splice-ticks", j.id.as_str()));
    let mut ticked: Vec<String> = ui.data(|d| d.get_temp(key)).unwrap_or_else(|| {
        // At first: the members in line with another of the same section.
        let mut out = Vec::new();
        for a in &j.arrivals {
            for b in &j.arrivals {
                if a.member < b.member
                    && t.continuous_problem(&a.member, &b.member, &j.id).is_none()
                {
                    out.push(a.member.clone());
                    out.push(b.member.clone());
                }
            }
        }
        out.dedup();
        out
    });
    ui.push_id(("add-splice", j.id.as_str()), |ui| {
        crate::layout::options_menu(ui, "+ Splice", |ui| {
            ui.label(
                egui::RichText::new(
                    "Covers bolting members together at this joint: tick the member ends it \
                     joins. Each bolts onto it with its own bolts; a cover outer and inner on \
                     each leg (8 mm to start): two shear planes.",
                )
                .small()
                .weak(),
            );
            for a in &j.arrivals {
                let mut on = ticked.contains(&a.member);
                if ui.checkbox(&mut on, a.member.as_str()).changed() {
                    if on {
                        ticked.push(a.member.clone());
                    } else {
                        ticked.retain(|x| x != &a.member);
                    }
                }
            }
            if ui
                .add_enabled(!ticked.is_empty(), egui::Button::new("Add the splice"))
                .on_hover_text("One undo step; the ends ticked bolt onto it (their Onto)")
                .on_disabled_hover_text("Tick the member ends it joins")
                .clicked()
            {
                let mut members: Vec<String> = j
                    .arrivals
                    .iter()
                    .filter(|a| ticked.contains(&a.member))
                    .map(|a| a.member.clone())
                    .collect();
                members.dedup();
                acts.push(Act::AddSplice(j.id.clone(), members));
                ui.close();
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(key, ticked));
    acts
}

/// The splices of joint `j`: per cover its face, leg and thickness
/// (typed), the shear planes of each member end onto it as worked out, a
/// cover added or removed, the splice removed.
fn splices_box(
    ui: &mut egui::Ui,
    j: &JointDetail,
    buffers: &mut std::collections::HashMap<String, String>,
) -> Vec<Act> {
    let mut acts = Vec::new();
    for s in &j.splices {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Splice {}", s.id)).on_hover_text(
                    "Covers bolting members together: each member end bolts onto it with its \
                     own bolts through its leg and the covers on that leg",
                );
                ui.label(format!("joins {}", s.members.join(", ")));
                if ui
                    .small_button("🗑")
                    .on_hover_text("Remove the splice (what bolted onto it: not given)")
                    .clicked()
                {
                    acts.push(Act::RemoveSplice(s.id.clone()));
                }
            });
            // The shear planes of each end onto it, worked out.
            for a in j
                .arrivals
                .iter()
                .filter(|a| a.onto.value.as_deref() == Some(s.id.as_str()))
            {
                let (planes, why) = &a.planes;
                ui.label(tinted(
                    ui,
                    planes,
                    egui::RichText::new(format!(
                        "{}: {} shear plane(s){} — bolts clamp {:.0} + {:.0} mm",
                        a.member,
                        planes.value.map_or("—".to_owned(), |n| n.to_string()),
                        why.as_ref()
                            .map(|w| format!(", {} ({w})", planes.source.words()))
                            .unwrap_or_else(|| format!(", {}", planes.source.words())),
                        a.grip[0] * 1000.0,
                        a.grip[1] * 1000.0
                    ))
                    .small(),
                ))
                .on_hover_text(PLANES_TIP);
            }
            egui::Grid::new(("splice-covers", s.id.as_str()))
                .striped(true)
                .show(ui, |ui| {
                    for h in ["Cover", "Thickness mm", "Outline", ""] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for c in &s.covers {
                        ui.label(c.words());
                        let v = Valued {
                            value: Some(c.thickness),
                            source: Source::Given,
                        };
                        let key = format!("ct-{}-{}", s.id, c.words());
                        if let Some(Some(x)) = length_cell(ui, buffers, key, &v, "Its thickness")
                            && x > 0.0
                        {
                            acts.push(Act::Cover(s.id.clone(), c.face, c.leg, Some(x)));
                        }
                        let words = if c.outline_given {
                            "given"
                        } else if c.outline.len() >= 3 {
                            "round the bolts"
                        } else {
                            "none"
                        };
                        ui.label(words).on_hover_text(if c.edges.is_empty() {
                            c.notes.join("; ")
                        } else {
                            c.edges.join("\n")
                        });
                        if ui
                            .small_button("🗑")
                            .on_hover_text("Remove this cover (one shear plane fewer on that leg)")
                            .clicked()
                        {
                            acts.push(Act::Cover(s.id.clone(), c.face, c.leg, None));
                        }
                        ui.end_row();
                    }
                });
            // The covers not there yet, to add.
            let legs: Vec<Option<CoverLeg>> = if s.covers.iter().any(|c| c.leg.is_some()) {
                vec![Some(CoverLeg::Long), Some(CoverLeg::Short)]
            } else {
                vec![None]
            };
            let missing: Vec<(CoverFace, Option<CoverLeg>)> = legs
                .iter()
                .flat_map(|l| CoverFace::ALL.map(|f| (f, *l)))
                .filter(|(f, l)| !s.covers.iter().any(|c| c.face == *f && c.leg == *l))
                .collect();
            if !missing.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for (f, l) in missing {
                        let name = match l {
                            Some(l) => format!("+ {} {}", f.as_str(), l.as_str()),
                            None => format!("+ {}", f.as_str()),
                        };
                        if ui
                            .small_button(name)
                            .on_hover_text(format!(
                                "A cover {:.0} mm on that face (one shear plane more on the leg)",
                                COVER_THICKNESS * 1000.0
                            ))
                            .clicked()
                        {
                            acts.push(Act::Cover(s.id.clone(), f, l, Some(COVER_THICKNESS)));
                        }
                    }
                });
            }
            for n in s
                .notes
                .iter()
                .chain(s.covers.iter().flat_map(|c| c.notes.iter()))
            {
                ui.label(egui::RichText::new(n).small().weak());
            }
        });
    }
    acts
}

/// Every pair of members that can be one part through a joint (in line
/// within 1°, the same section, roll and reference: `continuous_problem`
/// none), joint by joint: (joint, member, the other, already one part).
pub fn continuous_pairs(t: &ohl_structures::StructureType) -> Vec<(String, String, String, bool)> {
    let mut out = Vec::new();
    for n in &t.nodes {
        let here: Vec<&str> = t
            .members
            .iter()
            .filter(|m| m.nodes.contains(&n.id))
            .map(|m| m.id.as_str())
            .collect();
        for (k, a) in here.iter().enumerate() {
            for b in &here[k + 1..] {
                if t.continuous_problem(a, b, &n.id).is_none() {
                    let on = t.continuous_with(a, &n.id) == Some(*b);
                    out.push((n.id.clone(), (*a).to_owned(), (*b).to_owned(), on));
                }
            }
        }
    }
    out
}

impl StructureApp {
    /// Design › Continuous members…: the pairs that can be one part
    /// through a joint, each with a tick box (at first ticked: proposed),
    /// applied on confirm in one undo step — never set by itself.
    pub(crate) fn open_continuous_window(&mut self) {
        let Some(f) = self.file.as_ref() else {
            return;
        };
        let pairs = continuous_pairs(f.structure());
        self.joint_tools.continuous = Some(
            pairs
                .into_iter()
                .map(|(j, a, b, on)| (j, a, b, on, true))
                .collect(),
        );
    }

    /// The Continuous members window ([`Self::open_continuous_window`]).
    pub(crate) fn continuous_window(&mut self, ctx: &egui::Context) {
        let Some(mut list) = self.joint_tools.continuous.take() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Continuous members")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    "Members in line (within 1°) of the same section, roll and reference at a \
                     joint can run on through it as one part: no cut, no shift, no bolts there; \
                     the analysis joins them rigidly; one solid drawn; one part in the lists. \
                     Ticked: one part. Nothing is written before Apply.",
                );
                if list.is_empty() {
                    ui.label(egui::RichText::new("No pair in line of the same section.").weak());
                }
                ui.horizontal(|ui| {
                    if ui.small_button("Tick all").clicked() {
                        list.iter_mut().for_each(|x| x.4 = true);
                    }
                    if ui.small_button("Tick none").clicked() {
                        list.iter_mut().for_each(|x| x.4 = false);
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt("continuous-pairs")
                    .max_height(320.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (j, a, b, was, tick) in list.iter_mut() {
                            let text = format!(
                                "{a} into {b} at {j}{}",
                                if *was { " (one part now)" } else { "" }
                            );
                            ui.checkbox(tick, text);
                        }
                    });
                let changes = list.iter().filter(|x| x.3 != x.4).count();
                if ui
                    .add_enabled(changes > 0, egui::Button::new(format!("Apply ({changes})")))
                    .on_hover_text("The ticks written on both ends of each pair: one undo step")
                    .on_disabled_hover_text("Nothing changed")
                    .clicked()
                {
                    apply = true;
                }
            });
        if apply {
            let ends: Vec<(String, String, String, bool)> = list
                .iter()
                .filter(|x| x.3 != x.4)
                .map(|(j, a, b, _, tick)| (a.clone(), j.clone(), b.clone(), *tick))
                .collect();
            if let Some(Ok(d)) = self.joint_details().as_deref().map(|d| d.as_ref()) {
                self.apply_joint_acts(vec![Act::Continuous(ends)], d);
            } else {
                self.apply_joint_acts(vec![Act::Continuous(ends)], &JointDetails::default());
            }
            return;
        }
        if open {
            self.joint_tools.continuous = Some(list);
        }
    }
}
