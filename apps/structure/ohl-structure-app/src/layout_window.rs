// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Lay out joints** (2026-11-18, stage 4 of the automatic joint layout):
//! Design › Lay out joints… and Joint details › Lay out joint open one
//! tool window (the tool-window pattern): which joints (the joint shown,
//! the joints picked, a member group's, the whole tower; with their
//! similar and symmetric joints), the settings (the gap between steel,
//! the longest pull-back, the shift limit, the bolt where an end has none,
//! a connection preset per member class, the plates of a corner bolted or
//! welded, values given by hand replaced or kept), then a preview worked
//! out on a copy (on a thread of its own): what changes per joint, the
//! joints left for a hand detail with why, the clashes before and after,
//! the time it took, each joint's close-up before and after, and on
//! request the members' usage before and after for the last run's cases.
//! **Apply** writes the preview as one undo step; nothing is written
//! before. The layout follows later edits ([`StructureApp::follow_setup`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_structure_edit::Handle;
use ohl_structure_link::{BoltArea, JointDetails, LayoutRecord, LayoutRun, LayoutSettings, RunAsk};

use crate::app::StructureApp;

/// The manual's section of the window (page, anchor).
pub const LAYOUT_PAGE: (&str, &str) = ("user-guide/joint-layout", LAYOUT_ANCHOR);
const LAYOUT_ANCHOR: &str = "lay-out-joints";

/// The member classes a preset may be given for.
pub const CLASSES: [&str; 4] = ["bracing", "redundant", "crossing", "leg"];

/// The diameter of the bolt picked when the libraries have none, mm (the
/// drawing's usual bolt; an assumption, changed in the window).
const BOLT_START_MM: f64 = 16.0;

/// Which joints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// The joint shown in Joint details.
    Joint(String),
    /// The joints picked.
    Picked,
    /// The joints of a member group.
    Group(String),
    /// Every joint.
    Tower,
}

/// The preview: a run on a copy, and the joints in detail before and
/// after.
pub struct Preview {
    /// The inputs it was worked out for.
    pub key: u64,
    /// The file's revision it was worked out on.
    pub revision: u64,
    /// The run.
    pub run: LayoutRun,
    /// The joints asked in detail before and after.
    pub before: JointDetails,
    /// After.
    pub after: JointDetails,
    /// O44 L13: the leg and chord pieces the joints asked say are turned
    /// (from the file as it is, before any turn ticked), each with whether
    /// it is a chord (not a leg) — proposed, applied only when ticked.
    pub turns: Vec<(ohl_structure_link::LegRoll, bool)>,
    /// With turns ticked: the run without them (joints laid out whole,
    /// member ends laid out, plates) — their measured effect.
    pub without: Option<(usize, usize, usize)>,
    /// O44 L18: the leg's and chord's runs at the joints asked that their
    /// joints' planes turn (from the file as it is), each with whether its
    /// roll is automatic (set by the program: Orient angles or the layout —
    /// an import's) or given by hand (a proposal), and whether the joints it
    /// reaches are laid out better with it (the run's guard: at none fewer
    /// ends nor as many on more plates, in all more ends or as many on fewer
    /// plates). Ticked by default where both.
    pub plane_turns: Vec<(ohl_structure_link::plane_rolls::PlaneRoll, bool, bool)>,
    /// Each proposal's measured effect (by its key: [`turn_key`]): every
    /// joint its members reach laid out without and with it (all the
    /// proposals of its kind applied together), joints laid out whole,
    /// member ends, plates there — never judged on one joint.
    pub effects: BTreeMap<String, (RunCounts, RunCounts)>,
    /// O44 L25: the half or quarter turns the layout record offers for an
    /// outstanding leg pointing into the flange (the roll the designer's:
    /// never applied unasked), each with its row (its effect as weighed),
    /// and whether it is ticked (applied with the run).
    pub flange_offered: Vec<(
        String,
        ohl_structure_link::flange_turns::FlangeTurnRow,
        bool,
    )>,
}

/// Joints laid out whole, member ends laid out, plates.
pub type RunCounts = (usize, usize, usize);

/// A turn proposal's key: `j:` and the leg-rotation check's piece (L11),
/// `p:` and the run's first piece (the joints' planes, L18).
fn turn_key(planes: bool, member: &str) -> String {
    format!("{}{member}", if planes { "p:" } else { "j:" })
}

/// Whether run `after` lays the joints `at` out better than `before` (the
/// run's guard, joint by joint: at none fewer ends, nor as many on more
/// plates; in all more ends, or as many on fewer plates).
fn helps_at(before: &LayoutRun, after: &LayoutRun, at: &BTreeSet<String>) -> bool {
    let of = |r: &LayoutRun, j: &str| {
        r.joints
            .iter()
            .find(|o| o.joint == j && o.written)
            .map_or((0, 0), |o| (o.plan.ends.len(), o.plates.len()))
    };
    let (mut e0, mut p0, mut e1, mut p1) = (0, 0, 0, 0);
    for j in at {
        let (a, b) = (of(before, j), of(after, j));
        if b.0 < a.0 || (b.0 == a.0 && b.1 > a.1) {
            return false;
        }
        e0 += a.0;
        p0 += a.1;
        e1 += b.0;
        p1 += b.1;
    }
    e1 > e0 || (e1 == e0 && p1 < p0)
}

/// A run's joints laid out whole, member ends laid out and plates at the
/// joints `at`.
fn counts_at(r: &LayoutRun, at: &BTreeSet<String>) -> (usize, usize, usize) {
    let mut c = (0, 0, 0);
    for o in r.laid_out().filter(|o| at.contains(&o.joint)) {
        c.0 += usize::from(o.hand.is_empty());
        c.1 += o.plan.ends.len();
        c.2 += o.plates.len();
    }
    c
}

/// The joints the members `members` reach in `t` (both ends of each).
fn reach_of<'a>(
    t: &ohl_structures::StructureType,
    members: impl IntoIterator<Item = &'a String>,
) -> BTreeSet<String> {
    let set: BTreeSet<&String> = members.into_iter().collect();
    t.members
        .iter()
        .filter(|m| set.contains(&m.id))
        .flat_map(|m| m.nodes.iter().cloned())
        .collect()
}

/// `text` with the runs of `rolls` turned (each piece by its run's turn).
fn with_plane_turns<'a>(
    text: &str,
    rolls: impl Iterator<Item = &'a ohl_structure_link::plane_rolls::PlaneRoll>,
) -> Result<String, String> {
    ohl_structure_link::plane_rolls::with_turns(text, rolls)
}

/// A run's joints laid out whole, member ends laid out and plates.
fn run_counts(r: &LayoutRun) -> (usize, usize, usize) {
    (
        r.laid_out().filter(|o| o.hand.is_empty()).count(),
        r.laid_out().map(|o| o.plan.ends.len()).sum(),
        r.laid_out().map(|o| o.plates.len()).sum(),
    )
}

/// The leg and chord pieces of `d` (the joints asked in detail) whose
/// joints say they are turned, flagged, each with whether it is a chord.
fn proposed_turns(d: &JointDetails) -> Vec<(ohl_structure_link::LegRoll, bool)> {
    let legs = ohl_structure_link::leg_rolls(d);
    let chords = ohl_structure_link::leg_roll::piece_rolls(
        d,
        &ohl_structure_link::joint_classes::chord_pieces,
    );
    let mut out: Vec<(ohl_structure_link::LegRoll, bool)> = Vec::new();
    for (r, chord) in legs
        .into_iter()
        .map(|r| (r, false))
        .chain(chords.into_iter().map(|r| (r, true)))
    {
        // (A piece in line with one listed is turned with it: once.)
        if r.flagged()
            && !out
                .iter()
                .any(|(x, _)| x.member == r.member || x.partners.contains(&r.member))
        {
            out.push((r, chord));
        }
    }
    out
}

/// `text` with the turns `ticked` (by the proposal's piece) of `turns`
/// applied: each piece and the pieces in line with it turned alike, its
/// roll to a hundredth of a degree.
fn with_turns(
    text: &str,
    turns: &[(ohl_structure_link::LegRoll, bool)],
    ticked: &BTreeSet<String>,
) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let t = <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
        std::path::Path::new("s.ohls"),
        text,
    )
    .map_err(|e| e.message)?;
    for (r, _) in turns.iter().filter(|(r, _)| ticked.contains(&r.member)) {
        let Some(turn) = r.turn else { continue };
        for m in std::iter::once(&r.member).chain(r.partners.iter()) {
            let now = t
                .members
                .iter()
                .find(|x| &x.id == m)
                .map_or(0.0, ohl_structure_edit::edits::member_roll);
            let mut deg = (now.to_degrees() + turn) % 360.0;
            if deg > 180.0 {
                deg -= 360.0;
            } else if deg <= -180.0 {
                deg += 360.0;
            }
            deg = (deg * 100.0).round() / 100.0;
            ohl_structure_edit::edits::set_member_roll(&mut doc, m, deg.to_radians());
        }
    }
    Ok(doc.to_string())
}

/// The members' usage before and after for the last run's cases.
pub struct UsageCompare {
    /// The preview's key it is of.
    pub key: u64,
    /// Member, its largest usage before and after.
    pub rows: Vec<(String, Option<f64>, Option<f64>)>,
    /// Cases solved.
    pub cases: usize,
    /// How long, s.
    pub seconds: f64,
}

pub(crate) type Job<T> = Option<(u64, Receiver<Result<T, String>>)>;

/// The Lay out joints window.
pub struct LayoutWindow {
    /// Which joints.
    pub scope: Scope,
    /// With their similar and symmetric joints.
    pub similar: bool,
    /// The gap between steel, mm.
    pub gap_mm: f64,
    /// The longest pull-back, mm.
    pub pull_mm: f64,
    /// The shift limit, mm.
    pub shift_mm: f64,
    /// The bolt picked (a library bolt).
    pub bolt: Option<String>,
    /// Its diameter, mm (typed when no library bolt).
    pub diameter_mm: f64,
    /// A preset per member class (by name).
    pub presets: BTreeMap<String, String>,
    /// The plates of a corner welded together (else each bolted to its
    /// flange).
    pub welded: bool,
    /// Values given by hand replaced too.
    pub replace: bool,
    /// O44 L30: Solve the clashes after the run at every joint it leaves
    /// not whole (off by default: slow).
    pub solve_left: bool,
    /// The preview.
    pub preview: Option<Preview>,
    job: Job<Preview>,
    /// Why there is no preview.
    pub error: Option<String>,
    /// The joint whose close-ups are shown.
    pub shown: Option<String>,
    /// The two close-ups' views (before, after; turned together).
    pub views: Box<(
        crate::joint_panel::JointPanel,
        crate::joint_panel::JointPanel,
    )>,
    /// The usage before and after.
    pub usage: Option<UsageCompare>,
    usage_job: Job<UsageCompare>,
    /// O44 L13: the leg and chord turns ticked (by the proposal's piece):
    /// applied with the run (one undo step); none by default.
    pub turns: BTreeSet<String>,
    /// O44 L18: the plane turns ticked or not (by the run's first piece);
    /// a run not listed: ticked where its roll is automatic.
    pub plane_ticks: BTreeMap<String, bool>,
    /// O44 L25: the offered turns for an outstanding leg pointing into the
    /// flange ticked (by member): applied with the run; none by default.
    pub flange_ticks: BTreeSet<String>,
    /// O44 L24: the detailing practice the run takes and where it comes
    /// from (none: the default) — the structure's own, else its code
    /// plugin's, else the default ([`StructureApp::layout_practice`]).
    pub practice: Option<ohl_structure_link::RecordedPractice>,
    /// O44 L24: why a practice given is not used (refused, or the plugin
    /// failed): the default is used, said.
    pub practice_note: Option<String>,
}

impl LayoutWindow {
    /// The settings as the engine takes them.
    pub fn settings(&self, bolts: &ohl_structures::bolts::BoltLibrary) -> LayoutSettings {
        let lib = self.bolt.as_deref().and_then(|id| bolts.get(id));
        let d = lib.map_or(self.diameter_mm / 1000.0, |b| b.diameter.si());
        let s = LayoutSettings {
            gap: self.gap_mm / 1000.0,
            pull_back: self.pull_mm / 1000.0,
            shift_limit: self.shift_mm / 1000.0,
            presets: BTreeMap::new(),
            bolt: Some(BoltArea::picked(
                self.bolt.clone(),
                d,
                lib.and_then(|b| b.drawn()[0]),
            )),
            welded: self.welded,
            // The structure file's default minimums, taken by the run.
            defaults: Default::default(),
            bent_plates: true,
            once_per_kind: true,
            // L1 (owner 2026-11-19): bottom up, then out; slightly more
            // plates; what an end bolts onto chosen in the search.
            bottom_up: true,
            more_plates: true,
            onto_search: true,
            // L2 (2026-11-20): plates on their member, clash-aware
            // planning, the tries before an end is left for a clash; L1 (c)
            // off (measured, left out).
            plate_on: true,
            exact_plan: true,
            clash_tries: true,
            direct_first: false,
            // L2's follow-ups and L3 (2026-11-20): a plate stopping short of
            // what it would run into, a bent plate's own bolts slid clear of
            // the holes there, a plate bent by a few degrees.
            short_plates: true,
            slide_holds: true,
            small_bends: true,
            // L3b (2026-11-20): an equal-leg angle's other leg tried for
            // the end bent the wrong way; an end running towards its bend
            // bent away from the flange too.
            bend_facing: true,
            bend_back: true,
            // L5 (2026-11-20): an end of a face going onto a plate bolted
            // directly where pulled back further, or moved by at most the
            // plate's thickness, its bolts land.
            direct_moved: true,
            // L5o (2026-11-20): a flat plate on a member running on through
            // the joint given two provisional bolts of its own onto it.
            hold_plates: true,
            // J6 (2026-11-20): a plate on the angle of a built-up member
            // facing the ends (its angles known), kept where it lays out
            // more.
            built_up_plates: true,
            // L8 (2026-11-21): a joint planned once more with the members
            // the run named but does not lay out where the file draws them.
            as_drawn: true,
            // L8 (2026-11-21): the bolts of a member left as it is that
            // bolts onto an end laid out kept on that end's steel.
            partner_bolts: true,
            // L9 (2026-11-21): a welded plate's flat plate planned with its
            // outline as written (else short, else the ends left); holding
            // bolts on a plate stopping short of the work point where they
            // fit.
            reach_clear: true,
            short_holds: true,
            // L10 (2026-11-21): one of two ends on a face of a leg's flange
            // tried on its other face (half a turn), a diagonal onto the
            // inner face first, kept where it lays out more or as many on
            // fewer plates and its leg lies no farther out than now.
            other_face: true,
            face_pattern: true,
            // L10 (2026-11-21): a joint with an end's bolts drawn that join
            // nothing is not laid out whole (that member by hand, said).
            nothing_joined: true,
            // L11 (2026-11-21): an end going onto a plate moved sideways
            // within the shift limit so that its bolts pass the flange within
            // the longest pull-back — off in a run (measured: alone it lost
            // more ends than it gained); Solve the clashes tries it per joint.
            plate_shift: false,
            // L11: an end on a plate in its own plane lying flat in a
            // flange's plane on its inner side, past the heel, on the back of
            // that flange's flat plate instead (both faces of one plate).
            both_faces: true,
            // L12 (2026-11-21): a chord turned to what its joints say, kept
            // where its joints lay out more — off (measured; the owner
            // decides, a chord's roll being Orient angles').
            chord_turns: false,
            // L13 (2026-11-22): an end whose axis lies in no flange's plane
            // of today's choice nor of a member running through named onto
            // a member ending at the joint in whose flange's plane it lies
            // (a plan diagonal onto a face horizontal), weighed after
            // writing.
            plane_hosts: true,
            // L14 (2026-11-22): where nothing runs through the joint, the
            // base member's own end bolts hold the plate on its flange
            // (the plate's held_by) instead of joining nothing.
            base_holds: true,
            // L15 (2026-11-22): an end across the gap between a built-up
            // member's angles onto a plate between them (the plate's
            // between), as thick as the gap.
            between_plates: true,
            // L16 (2026-11-22): an end planned on the face the joint details
            // then seat it on.
            seat_as_drawn: true,
            // L18 (2026-11-22): a joint's planes of one rank high to low
            // (the program's default).
            planes_high_to_low: ohl_structure_link::LayoutSettings::default().planes_high_to_low,
            // L19 (2026-11-23): the turns Orient angles set from the
            // joints' planes weighed over every joint they reach.
            weigh_turns: true,
            // L20 (2026-11-24): a level plate across two horizontals (the
            // plate's also_bolts onto the second).
            level_plates: true,
            // L20 item 2 (2026-11-24): an end on a plate in its own plane
            // putting its axis up to 40 mm out, the offset recorded for the
            // analysis.
            own_offset_analysed: true,
            // L22 (2026-11-24): a plate's own holding bolts placed clear of
            // what the plan lays out after that plate at the joint.
            holds_clear: true,
            // L25 (2026-11-25): a member with an end whose outstanding leg
            // points into the flange laid out turned by half a turn and a
            // quarter turn, weighed over every joint it reaches (applied
            // where the roll is the program's, else offered).
            flange_half: true,
            flange_quarter: true,
            // L30 (2026-11-26): Solve the clashes after the run at the
            // joints left not whole — the window's tick (off by default).
            solve_left: self.solve_left,
            // L24 (2026-11-25): the detailing practice's values, below.
            ..LayoutSettings::default()
        };
        LayoutSettings {
            practice_refused: self.practice_note.clone(),
            ..ohl_structure_link::with_practice(s, self.practice.as_ref())
        }
    }

    fn key(&self, revision: u64, libs: u64, targets: &BTreeSet<String>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        revision.hash(&mut h);
        libs.hash(&mut h);
        targets.hash(&mut h);
        for v in [self.gap_mm, self.pull_mm, self.shift_mm, self.diameter_mm] {
            v.to_bits().hash(&mut h);
        }
        (
            &self.bolt,
            &self.presets,
            self.welded,
            self.replace,
            self.solve_left,
        )
            .hash(&mut h);
        self.practice
            .as_ref()
            .map(|p| (p.source.word(), p.practice.to_text()))
            .hash(&mut h);
        self.turns.hash(&mut h);
        self.plane_ticks.hash(&mut h);
        self.flange_ticks.hash(&mut h);
        h.finish()
    }
}

impl StructureApp {
    /// Opens the Lay out joints window for `scope`, its settings those of
    /// the last run (the file's record), else the starting values.
    pub(crate) fn open_layout_window(&mut self, scope: Scope) {
        self.layout_window = self.new_layout_window(scope);
    }

    /// The Lay out joints window for `scope` as it opens (the settings of
    /// the last run, else the starting values); also the settings Solve
    /// the clashes runs the layout with.
    pub(crate) fn new_layout_window(&mut self, scope: Scope) -> Option<LayoutWindow> {
        let f = self.file.as_ref()?;
        let record = LayoutRecord::of(f.structure());
        let rec = record.settings.clone();
        let (practice, practice_note) = self.layout_practice(&record);
        let start = practice
            .as_ref()
            .map(|p| p.practice.clone())
            .unwrap_or_default();
        let bolts = self.bolt_library();
        let first = bolts
            .0
            .bolts
            .iter()
            .find(|b| (b.diameter.si() * 1000.0 - BOLT_START_MM).abs() < 0.5)
            .or(bolts.0.bolts.first())
            .map(|b| b.id.clone());
        let (bolt, d) = match rec.as_ref().and_then(|r| r.bolt.clone()) {
            Some((id, d)) => (id, d * 1000.0),
            None => (first, BOLT_START_MM),
        };
        Some(LayoutWindow {
            scope,
            similar: true,
            gap_mm: rec.as_ref().map_or(start.gap, |r| r.gap) * 1000.0,
            pull_mm: rec.as_ref().map_or(start.pull_back, |r| r.pull_back) * 1000.0,
            shift_mm: rec.as_ref().map_or(start.shift, |r| r.shift_limit) * 1000.0,
            bolt,
            diameter_mm: d,
            presets: rec.as_ref().map(|r| r.presets.clone()).unwrap_or_default(),
            welded: rec.as_ref().is_some_and(|r| r.welded),
            replace: false,
            solve_left: false,
            preview: None,
            job: None,
            error: None,
            shown: None,
            views: {
                // A little further out than Joint details: the two views are small.
                let mut v: Box<(
                    crate::joint_panel::JointPanel,
                    crate::joint_panel::JointPanel,
                )> = Box::default();
                v.0.zoom = -0.4;
                v.1.zoom = -0.4;
                v
            },
            usage: None,
            usage_job: None,
            turns: BTreeSet::new(),
            plane_ticks: BTreeMap::new(),
            flange_ticks: BTreeSet::new(),
            practice,
            practice_note,
        })
    }

    /// O44 L24: the detailing practice a run of the layout takes, and why
    /// one given is not used: the structure file's own `[layout_practice]`
    /// over its code plugin's (its member check's method, asked once per
    /// session), value by value, over the default (none). A practice
    /// refused, or a plugin failing, leaves the default, said here and in
    /// the run's notes.
    pub(crate) fn layout_practice(
        &self,
        _record: &LayoutRecord,
    ) -> (Option<ohl_structure_link::RecordedPractice>, Option<String>) {
        let Some(f) = self.file.as_ref() else {
            return (None, None);
        };
        let own = f.structure().layout_practice.clone();
        let (plugin, failed) = match ohl_structure_link::plugin_practice(
            f.text_ref(),
            f.path().and_then(std::path::Path::parent),
        ) {
            Ok(p) => (p, None),
            Err(e) => (None, Some(e)),
        };
        match ohl_structure_link::practice::in_force(own.as_ref(), plugin.as_ref()) {
            Ok(p) => (
                p,
                failed.map(|e| format!("{e}. The code plugin's detailing practice is not used.")),
            ),
            Err(e) => (None, Some(ohl_structure_link::practice::refused_note(&e))),
        }
    }

    /// The layout's settings as the Lay out joints window would run them
    /// now (the last run's, else the starting values), the presets' fields
    /// by class and a leg splice's provisional bolt.
    pub(crate) fn layout_settings_now(
        &mut self,
    ) -> Option<(
        LayoutSettings,
        BTreeMap<String, toml_edit::Table>,
        Option<BoltArea>,
    )> {
        let w = self.new_layout_window(Scope::Tower)?;
        let bolts = self.bolt_library();
        let presets = self.preset_list();
        let mut s = w.settings(&bolts.0);
        let (areas, fields) = ohl_structure_link::presets_by_class(&w.presets, &presets, &bolts.0);
        let leg = areas.get("leg").cloned().or_else(|| s.bolt.clone());
        s.presets = areas;
        Some((s, fields, leg))
    }

    /// The joints the window's scope names (with the similar and
    /// symmetric ones when ticked).
    fn layout_targets(&mut self, w: &LayoutWindow) -> Option<BTreeSet<String>> {
        let f = self.file.as_ref()?;
        let t = f.structure_arc();
        let mut out: BTreeSet<String> = match &w.scope {
            Scope::Joint(j) => BTreeSet::from([j.clone()]),
            Scope::Picked => self
                .selected
                .iter()
                .filter_map(|h| match h {
                    Handle::Joint(j) => Some(j.clone()),
                    _ => None,
                })
                .collect(),
            Scope::Group(g) => t
                .members
                .iter()
                .filter(|m| m.group.as_deref() == Some(g.as_str()))
                .flat_map(|m| m.nodes.iter().cloned())
                .collect(),
            Scope::Tower => t.nodes.iter().map(|n| n.id.clone()).collect(),
        };
        if w.similar && w.scope != Scope::Tower {
            let details = self.joint_details();
            let mut more = BTreeSet::new();
            for j in &out {
                more.extend(ohl_structure_edit::symmetry::partners(&t, j));
                if let Some(Ok(d)) = details.as_deref() {
                    more.extend(
                        ohl_structure_link::similar_joints(d, j)
                            .into_iter()
                            .map(|s| s.joint),
                    );
                }
            }
            out.extend(more);
        }
        Some(out)
    }

    /// The presets as (name, fields).
    fn preset_list(&mut self) -> Vec<(String, toml_edit::Table)> {
        self.presets()
            .into_iter()
            .map(|p| (p.name, p.fields))
            .collect()
    }

    /// The Lay out joints window ([`Self::open_layout_window`]); its
    /// section of the manual: [`LAYOUT_PAGE`].
    pub(crate) fn layout_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.layout_window.take() else {
            return;
        };
        let Some(targets) = self.layout_targets(&w) else {
            return;
        };
        let bolts = self.bolt_library();
        let presets = self.preset_list();
        let (revision, text) = match self.file.as_ref() {
            Some(f) => (f.revision(), f.text()),
            None => return,
        };
        let key = w.key(revision, self.libs_stamp, &targets);
        // The preview: worked out again on its own thread when what it is
        // of changes.
        if let Some((k, rx)) = &w.job {
            match rx.try_recv() {
                Ok(r) => {
                    if *k == key {
                        match r {
                            Ok(p) => {
                                w.preview = Some(p);
                                w.error = None;
                            }
                            Err(e) => w.error = Some(e),
                        }
                    }
                    w.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(_) => w.job = None,
            }
        }
        let fresh = w.preview.as_ref().is_some_and(|p| p.key == key);
        if !fresh && w.job.as_ref().is_none_or(|(k, _)| *k != key) && !targets.is_empty() {
            let mut s = w.settings(&bolts.0);
            let (areas, fields) =
                ohl_structure_link::presets_by_class(&w.presets, &presets, &bolts.0);
            s.presets = areas;
            let lib = self.loads.sections.clone();
            let bolts2 = bolts.clone();
            let replace = w.replace;
            let joints = targets.clone();
            let ticked = w.turns.clone();
            let plane_ticks = w.plane_ticks.clone();
            let flange_ticks = w.flange_ticks.clone();
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let ask = RunAsk {
                    library: &lib,
                    bolts: &bolts2.0,
                    settings: &s,
                    preset_fields: fields,
                    joints: Some(joints.clone()),
                    replace_hand: replace,
                };
                let r = (|| {
                    // (O44 L19) The turns Orient angles set from the joints'
                    // planes weighed once, on the whole tower (each kept only
                    // where every joint it reaches is laid out better); the
                    // rest laid out from the file so settled.
                    let settled = ohl_structure_link::weighed_turns::settle_turns(&text, &ask)?;
                    let weighed = settled.as_ref().map(|(_, r)| r.weighed.clone());
                    let text = settled.map_or(text, |(s, _)| s);
                    // (O44 L25) The turns offered for an outstanding leg
                    // pointing into the flange (the roll the designer's):
                    // those ticked applied, every joint they reach laid
                    // out with them.
                    let flange_offered: Vec<(
                        String,
                        ohl_structure_link::flange_turns::FlangeTurnRow,
                        bool,
                    )> = {
                        let t =
                            <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
                                std::path::Path::new("s.ohls"),
                                &text,
                            )
                            .map_err(|e| e.message)?;
                        ohl_structure_link::flange_turns::offered(&t)
                            .into_iter()
                            .map(|(m, r)| {
                                let on = flange_ticks.contains(&m);
                                (m, r, on)
                            })
                            .collect()
                    };
                    let mut joints = joints.clone();
                    let text = if flange_offered.iter().any(|(_, _, on)| *on) {
                        let t =
                            <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
                                std::path::Path::new("s.ohls"),
                                &text,
                            )
                            .map_err(|e| e.message)?;
                        let on: BTreeSet<String> = flange_offered
                            .iter()
                            .filter(|(_, _, on)| *on)
                            .map(|(m, _, _)| m.clone())
                            .collect();
                        joints.extend(reach_of(&t, on.iter()));
                        ohl_structure_link::flange_turns::apply_offered(&text, &on)?
                    } else {
                        text
                    };
                    // Every joint a turn kept reaches laid out with it.
                    let mut joints = joints.clone();
                    for w in weighed.iter().flatten().filter(|w| w.kept) {
                        joints.extend(w.joints.iter().cloned());
                    }
                    let ask = RunAsk {
                        joints: Some(joints.clone()),
                        ..ask.clone()
                    };
                    let before = ohl_structure_link::joint_details_at(
                        &text,
                        &lib,
                        &bolts2.0,
                        Some(&joints),
                    )?;
                    let t = <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
                        std::path::Path::new("s.ohls"),
                        &text,
                    )
                    .map_err(|e| e.message)?;
                    // (O44 L13) The turns proposed, from the file as it is;
                    // (L18) beside them the leg's and chord's runs as their
                    // joints' planes turn them (on the whole tower: a run
                    // reaches past the joints asked), ticked by default where
                    // the roll is the program's and the joints it reaches are
                    // laid out better with it.
                    let turns = proposed_turns(&before);
                    let record = ohl_structure_link::LayoutRecord::of(&t);
                    let whole = ohl_structure_link::joint_details(&text, &lib, &bolts2.0)?;
                    let offered: Vec<(ohl_structure_link::plane_rolls::PlaneRoll, bool)> =
                        ohl_structure_link::plane_rolls::proposed_runs(&whole, Some(&joints))
                            .into_iter()
                            .map(|r| {
                                let auto = r.members.iter().all(|m| {
                                    record.oriented.contains_key(m)
                                        || record.rolled.contains_key(m)
                                        || t.members.iter().any(|x| {
                                            &x.id == m
                                                && x.roll.is_none()
                                                && x.reference.is_none()
                                                && x.orientation.is_none()
                                        })
                                });
                                (r, auto)
                            })
                            .collect();
                    // The joints every proposal reaches (its members' both
                    // ends): a turn is never judged on one joint.
                    let reach_l11 = |r: &ohl_structure_link::LegRoll| {
                        reach_of(&t, std::iter::once(&r.member).chain(r.partners.iter()))
                    };
                    let mut effects = BTreeMap::new();
                    let mut plane_turns: Vec<(
                        ohl_structure_link::plane_rolls::PlaneRoll,
                        bool,
                        bool,
                    )> = Vec::new();
                    if !turns.is_empty() || !offered.is_empty() {
                        let mut all: BTreeSet<String> = joints.clone();
                        for (r, _) in &turns {
                            all.extend(reach_l11(r));
                        }
                        for (r, _) in &offered {
                            all.extend(reach_of(&t, r.members.iter()));
                        }
                        let wide = RunAsk {
                            joints: Some(all.clone()),
                            ..ask.clone()
                        };
                        let plain = ohl_structure_link::lay_out(&text, &wide)?;
                        // Each kind's proposals applied together, each
                        // counted over the joints it reaches.
                        if !turns.is_empty() {
                            let every: BTreeSet<String> =
                                turns.iter().map(|(r, _)| r.member.clone()).collect();
                            let r1 = ohl_structure_link::lay_out(
                                &with_turns(&text, &turns, &every)?,
                                &wide,
                            )?;
                            for (r, _) in &turns {
                                let at = reach_l11(r);
                                effects.insert(
                                    turn_key(false, &r.member),
                                    (counts_at(&plain, &at), counts_at(&r1, &at)),
                                );
                            }
                        }
                        if !offered.is_empty() {
                            let r2 = ohl_structure_link::lay_out(
                                &with_plane_turns(&text, offered.iter().map(|(r, _)| r))?,
                                &wide,
                            )?;
                            for (r, auto) in offered {
                                let at = reach_of(&t, r.members.iter());
                                effects.insert(
                                    turn_key(true, &r.members[0]),
                                    (counts_at(&plain, &at), counts_at(&r2, &at)),
                                );
                                let helps = helps_at(&plain, &r2, &at);
                                plane_turns.push((r, auto, helps));
                            }
                        }
                    }
                    let planes_on: Vec<&ohl_structure_link::plane_rolls::PlaneRoll> = plane_turns
                        .iter()
                        .filter(|(r, auto, helps)| {
                            plane_ticks
                                .get(&r.members[0])
                                .copied()
                                .unwrap_or(*auto && *helps)
                        })
                        .map(|(r, _, _)| r)
                        .collect();
                    // One turn per member: a piece a plane turn ticked covers
                    // is not turned by the leg-rotation check's too.
                    let covered: BTreeSet<&String> =
                        planes_on.iter().flat_map(|r| r.members.iter()).collect();
                    let ticked: BTreeSet<String> = ticked
                        .into_iter()
                        .filter(|m| {
                            turns.iter().any(|(r, _)| {
                                &r.member == m
                                    && !std::iter::once(&r.member)
                                        .chain(r.partners.iter())
                                        .any(|x| covered.contains(x))
                            })
                        })
                        .collect();
                    let (run, without) = {
                        if ticked.is_empty() && planes_on.is_empty() {
                            (ohl_structure_link::lay_out(&text, &ask)?, None)
                        } else {
                            // What Apply writes: the turns ticked, every joint
                            // they reach laid out with them.
                            let mut at: BTreeSet<String> = joints.clone();
                            for (r, _) in turns.iter().filter(|(r, _)| ticked.contains(&r.member)) {
                                at.extend(reach_l11(r));
                            }
                            for r in &planes_on {
                                at.extend(reach_of(&t, r.members.iter()));
                            }
                            let turned = with_plane_turns(
                                &with_turns(&text, &turns, &ticked)?,
                                planes_on.iter().copied(),
                            )?;
                            let asked = RunAsk {
                                joints: Some(at.clone()),
                                ..ask.clone()
                            };
                            let plain_at = ohl_structure_link::lay_out(&text, &asked)?;
                            (
                                ohl_structure_link::lay_out(&turned, &asked)?,
                                Some(run_counts(&plain_at)),
                            )
                        }
                    };
                    let mut run = run;
                    if let Some(w) = weighed {
                        let mut notes = ohl_structure_link::weighed_turns::notes_of(&w);
                        notes.append(&mut run.notes);
                        run.notes = notes;
                        run.weighed = w;
                    }
                    let after = ohl_structure_link::joint_details_at(
                        &run.text,
                        &lib,
                        &bolts2.0,
                        Some(&joints),
                    )?;
                    Ok(Preview {
                        key,
                        revision,
                        run,
                        before,
                        after,
                        turns,
                        without,
                        plane_turns,
                        effects,
                        flange_offered,
                    })
                })();
                let _ = tx.send(r);
            });
            w.job = Some((key, rx));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.poll_usage(&mut w, ctx);
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let mut usage_asked = false;
        let mut in_line: Option<Vec<ohl_structure_link::InLine>> = None;
        let mut leg_places: Option<LegPlaces> = None;
        let groups: Vec<String> = self
            .file
            .as_ref()
            .map(|f| {
                ohl_structure_edit::groups(f.structure())
                    .into_iter()
                    .map(|g| g.name)
                    .collect()
            })
            .unwrap_or_default();
        let picked = self
            .selected
            .iter()
            .filter(|h| matches!(h, Handle::Joint(_)))
            .count();
        let all = self.file.as_ref().map_or(0, |f| f.structure().nodes.len());
        egui::Window::new("Lay out joints")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                let dark = ui.visuals().dark_mode;
                ohl_ui::tool_window::intro(
                    ui,
                    "Places the member ends of the joints so that they fit: pulled back, moved \
                     sideways only where needed, onto the leg or onto plates.",
                    "Each member end bolting by one leg onto another member's flange is laid flat \
                     (turned only where a joint of it is then laid out and no flat end is \
                     spoilt) and its gauge line put through the work point (concentric); its end is \
                     pulled back until its steel clears the flange's heel and the members placed \
                     before it by the gap. Only where that would need more than the longest \
                     pull-back is it moved sideways, the smallest move that fits, up to the shift \
                     limit (the analysis takes the move as an eccentricity). Where every bolt \
                     lands on the flange it bolts on directly, else all the plane's members onto \
                     one plate. What does not fit is left as it is and listed for a hand \
                     detail, never forced. Bolts are not chosen here: an end without bolts gets \
                     its class's preset or the bolt picked (provisional). Nothing is written \
                     before Apply (one undo step); later edits near a laid-out joint lay it out \
                     again.",
                    LAYOUT_ANCHOR,
                    || crate::help::open_section(LAYOUT_PAGE.0, LAYOUT_PAGE.1),
                );
                egui::ScrollArea::vertical()
                    .id_salt("layout-window-body")
                    .max_height(560.0)
                    .show(ui, |ui| {
                ohl_ui::tool_window::heading(
                    ui,
                    "Joints",
                    "Which joints are laid out. Similar joints have the same member sections, \
                     classes and angles (within 1°); symmetric ones mirror them ([[symmetry]]).",
                );
                if let Scope::Joint(j) = &w.scope {
                    let j = j.clone();
                    ui.radio_value(&mut w.scope, Scope::Joint(j.clone()), format!("Joint {j}"));
                }
                ui.radio_value(
                    &mut w.scope,
                    Scope::Picked,
                    format!("The joints picked ({picked})"),
                );
                ui.horizontal(|ui| {
                    let in_group = matches!(w.scope, Scope::Group(_));
                    if ui.radio(in_group, "A member group's joints").clicked()
                        && !in_group
                        && let Some(g) = groups.first()
                    {
                        w.scope = Scope::Group(g.clone());
                    }
                    if let Scope::Group(g) = &mut w.scope {
                        egui::ComboBox::from_id_salt("layout-group")
                            .selected_text(g.clone())
                            .show_ui(ui, |ui| {
                                for x in &groups {
                                    ui.selectable_value(g, x.clone(), x);
                                }
                            });
                    }
                });
                ui.radio_value(
                    &mut w.scope,
                    Scope::Tower,
                    format!("The whole tower ({all} joints)"),
                );
                ui.add_enabled(
                    w.scope != Scope::Tower,
                    egui::Checkbox::new(&mut w.similar, "With their similar and symmetric joints"),
                );
                ui.label(
                    egui::RichText::new(format!("{} joint(s) laid out", targets.len()))
                        .small()
                        .weak(),
                );
                ohl_ui::tool_window::heading(
                    ui,
                    "Settings",
                    "Not from a standard: the owner gave no values (2026-11-18). The gap's \
                     starting value is invented (10 mm); the pull-back's and the shift limit's \
                     were measured on real towers (see the theory page). Each changes the \
                     layout below at once.",
                );
                egui::Grid::new("layout-settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ohl_ui::tool_window::label(
                            ui,
                            "Gap between steel (mm)",
                            "Between the steel of members that do not bolt to each other, and \
                             from their bolts' nuts. Not from a standard: 10 mm to start with.",
                        );
                        ui.add(
                            egui::DragValue::new(&mut w.gap_mm)
                                .range(0.0..=100.0)
                                .speed(0.5),
                        );
                        ui.end_row();
                        ohl_ui::tool_window::label(
                            ui,
                            "Longest pull-back (mm)",
                            "How far a member's steel may stop from the work point before it is \
                             moved sideways instead (the plate gets long). Not from a standard.",
                        );
                        ui.add(
                            egui::DragValue::new(&mut w.pull_mm)
                                .range(10.0..=2000.0)
                                .speed(1.0),
                        );
                        ui.end_row();
                        ohl_ui::tool_window::label(
                            ui,
                            "Shift limit (mm)",
                            "The largest sideways move of a gauge line off the work point (0: \
                             never moved). The analysis takes a move as an eccentricity (a rigid \
                             end offset of a frame member). Not from a standard.",
                        );
                        ui.add(
                            egui::DragValue::new(&mut w.shift_mm)
                                .range(0.0..=200.0)
                                .speed(0.5),
                        );
                        ui.end_row();
                        ohl_ui::tool_window::label(
                            ui,
                            "Bolt where an end has none",
                            "Two in one gauge line, provisional (marked so) until the code \
                             step chooses the bolts; a tower with provisional bolts stays at \
                             level 3. Their pitch and end distance, and the edge distance a \
                             bolt keeps in the flange where the design code gives none, are \
                             the structure's default minimums (Design › Analysis; default \
                             minimum, assumed, not a code's).",
                        );
                        ui.horizontal(|ui| {
                            let text = w.bolt.clone().unwrap_or_else(|| "typed".into());
                            egui::ComboBox::from_id_salt("layout-bolt")
                                .selected_text(text)
                                .show_ui(ui, |ui| {
                                    for b in &bolts.0.bolts {
                                        ui.selectable_value(&mut w.bolt, Some(b.id.clone()), &b.id);
                                    }
                                    ui.selectable_value(&mut w.bolt, None, "typed");
                                });
                            if w.bolt.is_none() {
                                ui.add(
                                    egui::DragValue::new(&mut w.diameter_mm)
                                        .range(6.0..=48.0)
                                        .suffix(" mm"),
                                );
                            }
                        });
                        ui.end_row();
                        for class in CLASSES {
                            ohl_ui::tool_window::label(
                                ui,
                                &format!("Preset for {class}"),
                                "A connection preset's bolts for an end of this class that has \
                                 none (before the bolt picked); provisional too.",
                            );
                            let now = w.presets.get(class).cloned();
                            let mut pick = now.clone();
                            egui::ComboBox::from_id_salt(format!("layout-preset-{class}"))
                                .selected_text(pick.clone().unwrap_or_else(|| "none".into()))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut pick, None, "none");
                                    for (n, _) in &presets {
                                        ui.selectable_value(&mut pick, Some(n.clone()), n);
                                    }
                                });
                            if pick != now {
                                match pick {
                                    Some(n) => {
                                        w.presets.insert(class.to_owned(), n);
                                    }
                                    None => {
                                        w.presets.remove(class);
                                    }
                                }
                            }
                            ui.end_row();
                        }
                        ohl_ui::tool_window::label(
                            ui,
                            "Plates at a corner",
                            "Members in both faces of a leg: one plate per face, each bolted to \
                             its flange of the leg (usual lattice practice), or the plates welded \
                             together.",
                        );
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut w.welded, false, "Each bolted to its flange");
                            ui.radio_value(&mut w.welded, true, "Welded together");
                        });
                        ui.end_row();
                    });
                ui.checkbox(&mut w.replace, "Replace values given by hand")
                    .on_hover_text(
                        "Off: a cut, shift, roll or what a member bolts onto given by hand is \
                         kept (the layout works round it). On: replaced by the layout's.",
                    );
                ui.checkbox(
                    &mut w.solve_left,
                    "Solve the clashes at the joints left after the run",
                )
                .on_hover_text(
                    "After the run, Solve the clashes at every joint it leaves not whole, each \
                     solution kept only where the joint gets better on every count and every \
                     joint around is no worse. Every turn and move it makes is said (marked) in \
                     the notes; it never confirms a proposal or turns a leg. Slow: about 20 s per \
                     joint left, more on a busy machine — hours on a large tower (measured; see \
                     the user guide).",
                );
                ui.separator();
                // The preview.
                let busy = w.job.is_some();
                if let Some(e) = &w.error {
                    crate::app::error_label(ui, e.clone());
                }
                match w.preview.as_ref().filter(|p| p.key == key) {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(if busy {
                                "Working out the layout on a copy…"
                            } else {
                                "No joint to lay out."
                            });
                        });
                    }
                    Some(p) => {
                        layout_summary(ui, p);
                        layout_turns(ui, p, &mut w.turns, &mut w.plane_ticks);
                        layout_flange_turns(ui, p, &mut w.flange_ticks);
                        in_line = layout_in_line(ui, p);
                        leg_places = layout_leg_splices(ui, p);
                        layout_changes(ui, p);
                        layout_close_ups(ui, &mut w.shown, &mut w.views, p, dark);
                        usage_asked =
                            layout_usage(ui, w.usage.as_ref(), p.key, w.usage_job.is_some());
                    }
                }
                // (L24) The detailing practice in force, below the preview (its
                // settings above stay where they were).
                ui.separator();
                layout_practice_ui(ui, &mut w);
                });
                let ready = w
                    .preview
                    .as_ref()
                    .is_some_and(|p| p.key == key && p.revision == revision);
                let n = w.preview.as_ref().map_or(0, |p| p.run.laid_out().count());
                use ohl_ui::tool_window::{Keys, Pressed, buttons};
                match buttons(
                    ui,
                    &format!("Apply ({n} joints)"),
                    ready && n > 0,
                    Some(if ready {
                        "Nothing to lay out"
                    } else {
                        "The preview is being worked out"
                    }),
                    Keys::Escape,
                ) {
                    Some(Pressed::Action) => apply = true,
                    Some(Pressed::Cancel) => cancel = true,
                    None => {}
                }
            });
        if usage_asked {
            self.start_usage(&mut w, ctx);
        }
        // J9 (D15): pieces in line confirmed one part, one undo step; the
        // preview is worked out again on the file as edited.
        if let Some(list) = in_line
            && let Some(f) = self.file.as_mut()
        {
            let asked = list.len();
            let r = f.edit(&format!("Pieces in line one part ({asked})"), |doc, t| {
                ohl_structure_link::in_line::confirm(doc, t, &list)
            });
            self.message = Some(match r {
                Ok((n, not)) if not.is_empty() => (
                    format!("{n} pair(s) of pieces in line one part (one undo step)"),
                    false,
                ),
                Ok((n, not)) => (
                    format!(
                        "{n} pair(s) of pieces in line one part (one undo step); not: {}",
                        not.join("; ")
                    ),
                    true,
                ),
                Err(e) => (e, true),
            });
        }
        // J7 (the owner's decisions of 2026-11-19): the leg splice places
        // confirmed — the same section one part, a change of section
        // spliced — one undo step; the preview worked out again.
        if let Some((one, splices)) = leg_places {
            // (Provisional bolts where a piece has none: the preset for the
            // class `leg`, else the bolt picked.)
            let bolts = self.bolt_library();
            let presets = self.preset_list();
            let s = w.settings(&bolts.0);
            let (areas, _) = ohl_structure_link::presets_by_class(&w.presets, &presets, &bolts.0);
            let bolt = areas.get("leg").cloned().or(s.bolt);
            let asked = one.len() + splices.len();
            let r = self.file.as_mut().map(|f| {
                f.edit(&format!("Leg splice places ({asked})"), |doc, t| {
                    ohl_structure_link::leg_splices::confirm_places(
                        doc,
                        t,
                        &one,
                        &splices,
                        bolt.as_ref(),
                    )
                })
            });
            self.message = r.map(|r| match r {
                Ok((n, m, not)) => (
                    format!(
                        "{m} leg splice(s) written, {n} pair(s) of leg pieces one part (one undo \
                         step){}",
                        if not.is_empty() {
                            String::new()
                        } else {
                            format!("; not: {}", not.join("; "))
                        }
                    ),
                    !not.is_empty(),
                ),
                Err(e) => (e, true),
            });
        }
        if apply && let Some(p) = w.preview.take() {
            let laid = p.run.laid_out().count();
            let hand = p.run.for_hand().count();
            let plates: usize = p.run.laid_out().map(|o| o.plates.len()).sum();
            let text = p.run.text.clone();
            if let Some(f) = self.file.as_mut() {
                f.edit_alone(&format!("Lay out joints ({laid})"), |doc, _| {
                    if let Ok(d) = text.parse::<toml_edit::DocumentMut>() {
                        *doc = d;
                    }
                });
            }
            self.message = Some((
                format!(
                    "{laid} joint(s) laid out ({plates} plate(s), one undo step){}; clashes at \
                     them {} to {}",
                    if hand > 0 {
                        format!(
                            ", {hand} need a hand detail (Design › Review the joints left… lists \
                             them with why)"
                        )
                    } else {
                        String::new()
                    },
                    p.run.clashes.0,
                    p.run.clashes.1
                ),
                false,
            ));
            return;
        }
        if open && !cancel {
            self.layout_window = Some(w);
        }
    }

    /// Starts the usage before and after for the last run's cases.
    fn start_usage(&mut self, w: &mut LayoutWindow, ctx: &egui::Context) {
        let Some(p) = w.preview.as_ref() else { return };
        let Some(f) = self.file.as_ref() else { return };
        let (before, after, key) = (f.text(), p.run.text.clone(), p.key);
        if let Some(rx) = self.usage_thread(before, after, key) {
            w.usage_job = Some((key, rx));
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    /// The members' largest usage over the last run's cases with the file
    /// `before_text` and `after_text`, worked out on a thread of its own (the
    /// analysis twice); none (said) when there is no run yet.
    pub(crate) fn usage_thread(
        &mut self,
        before_text: String,
        after_text: String,
        key: u64,
    ) -> Option<Receiver<Result<UsageCompare, String>>> {
        let f = self.file.as_ref()?;
        let (table, recipes) = self.ran_with_of();
        let names: Vec<String> = match self.results.results.as_ref() {
            Some(Ok(r)) => r.cases.iter().map(|c| c.name.clone()).collect(),
            _ => Vec::new(),
        };
        if names.is_empty() {
            self.message = Some((
                "no run yet: Results › Run first (the usage before and after takes the last \
                 run's cases)"
                    .into(),
                true,
            ));
            return None;
        }
        let cases: Vec<_> = ohl_structure_link::cases_to_run(&table, &recipes, None)
            .into_iter()
            .filter(|c| names.iter().any(|n| n == &c.name))
            .collect();
        let dir = f
            .path()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf);
        let sections = self.loads.sections.clone();
        let bolts = self.bolt_library();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let usage = |text: &str| -> Result<BTreeMap<String, Option<f64>>, String> {
                let t = <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
                    std::path::Path::new("structure.ohls"),
                    text,
                )
                .map_err(|e| e.message)?;
                let run = ohl_structure_link::analyse_structure(
                    &ohl_structure_link::RunInput {
                        structure: &t,
                        text,
                        dir: dir.as_deref(),
                        sections: &sections,
                        table: &table,
                        recipes: &recipes,
                        cases: &cases,
                        extension: None,
                        bolts: Some(&bolts.0),
                        feet: None,
                    },
                    &ohl_core::budget::Budget::new(),
                )?;
                let mut out: BTreeMap<String, Option<f64>> = BTreeMap::new();
                for c in &run.cases {
                    if let Some(Ok(r)) = &c.fe {
                        for m in &r.members {
                            let e = out.entry(m.id.clone()).or_insert(None);
                            if let Some(u) = m.usage {
                                *e = Some(e.map_or(u, |x: f64| x.max(u)));
                            }
                        }
                    }
                }
                Ok(out)
            };
            let r = (|| {
                let a = usage(&before_text)?;
                let b = usage(&after_text)?;
                let mut rows: Vec<(String, Option<f64>, Option<f64>)> = a
                    .iter()
                    .map(|(m, u)| (m.clone(), *u, b.get(m).copied().flatten()))
                    .collect();
                // The largest change first.
                rows.sort_by(|x, y| {
                    let d = |r: &(String, Option<f64>, Option<f64>)| {
                        (r.2.unwrap_or(0.0) - r.1.unwrap_or(0.0)).abs()
                    };
                    d(y).total_cmp(&d(x)).then(x.0.cmp(&y.0))
                });
                Ok(UsageCompare {
                    key,
                    rows,
                    cases: cases.len(),
                    seconds: start.elapsed().as_secs_f64(),
                })
            })();
            let _ = tx.send(r);
        });
        Some(rx)
    }

    fn poll_usage(&mut self, w: &mut LayoutWindow, ctx: &egui::Context) {
        if let Some(e) = poll_usage_job(&mut w.usage_job, &mut w.usage, ctx) {
            self.message = Some((format!("usage before and after: {e}"), true));
        }
    }

    /// The layout follows later edits: the file's follow set (again when
    /// the libraries or presets change) to [`ohl_structure_link::follow`].
    pub(crate) fn follow_setup(&mut self) {
        let stamp = self.libs_stamp;
        let has = self.file.as_ref().is_some_and(|f| f.has_follow());
        if has && self.follow_stamp == Some(stamp) {
            return;
        }
        if self.file.is_none() {
            return;
        }
        let sections = self.loads.sections.clone();
        let bolts = self.bolt_library();
        let presets = self.preset_list();
        // The file as the edit left it (not read again: quick on a large
        // tower).
        let hook: ohl_structure_edit::AfterEdit = std::sync::Arc::new(move |e| {
            ohl_structure_link::follow_read(
                e.before,
                e.after_text,
                e.after.clone(),
                &sections,
                &bolts.0,
                &presets,
            )
            .map(|f| (f.text, f.message))
        });
        if let Some(f) = self.file.as_mut() {
            f.set_follow(Some(hook));
        }
        self.follow_stamp = Some(stamp);
    }
}

/// O44 L24: the detailing practice the run takes — where it comes from,
/// why one given is not used, what is said with it, and its values
/// (read-only, each with its effect on the results and its time); the
/// practice's gap, pull-back and shift taken into the settings above on
/// request.
fn layout_practice_ui(ui: &mut egui::Ui, w: &mut LayoutWindow) {
    let source = w
        .practice
        .as_ref()
        .map_or_else(|| "the program's default".to_owned(), |p| p.source.label());
    let practice = w
        .practice
        .as_ref()
        .map(|p| p.practice.clone())
        .unwrap_or_default();
    ohl_ui::tool_window::heading(
        ui,
        "Detailing practice",
        "The shop's preferences the layout follows (O44 L24): which kinds of parts it may make \
         and in which order, the faces tried first, the limits, the flatness, the plates' shapes \
         and bend, the picked bolt's pattern (per member class too). This structure file's own \
         [layout_practice] over the code plugin of its member check, value by value, over the \
         program's default (the layout as before L24); each value says where it comes from. \
         OpenOHL checks every value and refuses a practice out of range (said here and in the \
         run's notes); the clash checks, joint checks and the guard that no \
         joint ends worse stay in force whatever it says.",
    );
    let name = if practice.name.is_empty() {
        String::new()
    } else {
        format!(" — \"{}\"", practice.name)
    };
    ui.label(egui::RichText::new(format!("From {source}{name}")).small());
    if let Some(n) = &w.practice_note {
        crate::app::error_label(ui, egui::RichText::new(n.as_str()).small());
    }
    for n in practice.notes() {
        ui.label(egui::RichText::new(n).small().weak());
    }
    egui::CollapsingHeader::new("Its values")
        .id_salt("layout-practice")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("layout-practice-grid")
                .num_columns(3)
                .show(ui, |ui| {
                    for (keys, what, value, effect) in practice.rows() {
                        ohl_ui::tool_window::label(ui, what, effect);
                        ui.label(value);
                        ui.label(
                            egui::RichText::new(
                                w.practice
                                    .as_ref()
                                    .map_or("the default", |p| p.source_of(keys)),
                            )
                            .small()
                            .weak(),
                        );
                        ui.end_row();
                    }
                });
            let start = (
                practice.gap * 1000.0,
                practice.pull_back * 1000.0,
                practice.shift * 1000.0,
            );
            let now = (w.gap_mm, w.pull_mm, w.shift_mm);
            if ((start.0 - now.0).abs() > 1e-6
                || (start.1 - now.1).abs() > 1e-6
                || (start.2 - now.2).abs() > 1e-6)
                && ui
                    .button("Take its gap, pull-back and shift")
                    .on_hover_text(
                        "The settings above are the last run's (kept in the layout record); \
                         this puts the practice's starting values in their place.",
                    )
                    .clicked()
            {
                (w.gap_mm, w.pull_mm, w.shift_mm) = start;
            }
        });
}

/// The numbers of a preview.
fn layout_summary(ui: &mut egui::Ui, p: &Preview) {
    let laid = p.run.laid_out().count();
    let hand = p.run.for_hand().count();
    let plates: usize = p.run.laid_out().map(|o| o.plates.len()).sum();
    let provisional: usize = p
        .run
        .laid_out()
        .flat_map(|o| &o.plan.ends)
        .filter(|e| e.bolts.is_some())
        .count();
    // Member ends laid out of those the layout places (J2: the ends that
    // fit are kept in a joint that is not whole).
    let ends: usize = p.run.laid_out().map(|o| o.plan.ends.len()).sum();
    let all: usize = p
        .run
        .joints
        .iter()
        .map(|o| o.plan.ends.len() + o.plan.hand_members.len())
        .sum();
    // J4: the joints laid out as a joint like them (their plan mapped).
    let like = p.run.laid_out().filter(|o| o.plan.like.is_some()).count();
    ui.label(format!(
        "{laid} joint(s) laid out, {plates} plate(s); {hand} need a hand detail; {ends} of {all} \
         member end(s) laid out; clashes at these joints {} to {}; {provisional} end(s) with \
         provisional bolts. Worked out in {:.2} s (measured).",
        p.run.clashes.0, p.run.clashes.1, p.run.seconds
    ))
    .on_hover_text(format!(
        "{like} of the joints laid out took the plan of a joint like them (the tower's symmetry, \
         a repeated panel: planned once per kind, checked for clashes in their own place)"
    ));
    let shifted = p
        .run
        .laid_out()
        .flat_map(|o| &o.plan.ends)
        .filter(|e| e.shift.is_some_and(|s| s.abs() >= 5e-5))
        .count();
    if shifted > 0 {
        ui.label(
            egui::RichText::new(format!(
                "{shifted} end(s) moved sideways: the analysis takes each move as an \
                 eccentricity where a frame member meets the joint; where only bars meet it, it \
                 is left out — NOT ON THE SAFE SIDE there: its moment is missing, so a usage can \
                 be too low; a frame member at the joint takes it (said per member in the results)"
            ))
            .small()
            .color(ui.visuals().warn_fg_color),
        );
    }
    for n in p.run.notes.iter().take(3) {
        ui.label(egui::RichText::new(n).small().weak());
    }
    if p.run.notes.len() > 3 {
        ui.label(
            egui::RichText::new(format!("… {} more notes", p.run.notes.len() - 3))
                .small()
                .weak(),
        );
    }
}

/// O44 L13 (the owner's answers to L12's questions 4 and 5): the leg and
/// chord pieces the joints asked say are turned, each with a tick (off by
/// default) applying its turn with the run; and (O44 L18, the owner's
/// decision of 2026-11-22) beside them the leg's and chord's runs as their
/// joints' planes turn them, each with its own tick (on by default where
/// the roll is the program's — an import's —, off where given by hand).
/// Each with its measured effect over every joint its members reach; with
/// turns ticked, the run with and without them. One turn per member: a tick
/// clears the other kind's tick of the same pieces.
fn layout_turns(
    ui: &mut egui::Ui,
    p: &Preview,
    ticked: &mut BTreeSet<String>,
    plane_ticks: &mut BTreeMap<String, bool>,
) {
    if p.turns.is_empty() && p.plane_turns.is_empty() {
        return;
    }
    let effect =
        |k: &str| -> String {
            p.effects.get(k).map_or(String::new(), |((w0, e0, p0), (w1, e1, p1))| {
            format!(
                " — at the joints it reaches: whole {w0} to {w1}, member ends {e0} to {e1}, \
                 plates {p0} to {p1}"
            )
        })
        };
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "Legs and chords turned against their joints, two ways side by side: {} from the \
                 ends at their joints, {} from their joints' planes — each applied only when \
                 ticked (with the run, one undo step)",
                p.turns.len(),
                p.plane_turns.len()
            ))
            .small()
            .color(ui.visuals().warn_fg_color),
        );
        ohl_ui::tool_window::question(ui, LEG_ROLL_HELP);
    });
    let plane_on = |r: &ohl_structure_link::plane_rolls::PlaneRoll,
                    default: bool,
                    ticks: &BTreeMap<String, bool>| {
        ticks.get(&r.members[0]).copied().unwrap_or(default)
    };
    for (r, chord) in &p.turns {
        let mut on = ticked.contains(&r.member);
        let words = if *chord {
            format!(
                "Chord (from the ends at its joints): {}",
                r.words().replacen("leg piece", "chord piece", 1).replacen(
                    "one leg",
                    "one chord",
                    1
                )
            )
        } else {
            format!("Leg (from the ends at its joints): {}", r.words())
        };
        let words = format!("{words}{}", effect(&turn_key(false, &r.member)));
        if ui.checkbox(&mut on, words).changed() {
            if on {
                ticked.insert(r.member.clone());
                // (The plane turn of the same pieces cleared.)
                for (x, _, _) in &p.plane_turns {
                    if x.members
                        .iter()
                        .any(|m| m == &r.member || r.partners.contains(m))
                    {
                        plane_ticks.insert(x.members[0].clone(), false);
                    }
                }
            } else {
                ticked.remove(&r.member);
            }
        }
    }
    for (r, auto, helps) in &p.plane_turns {
        let mut on = plane_on(r, *auto && *helps, plane_ticks);
        let words = format!(
            "{} ({}): {}{}",
            if r.kind == ohl_structure_link::plane_rolls::RunKind::Leg {
                "Leg"
            } else {
                "Chord"
            },
            match (*auto, *helps) {
                (true, true) => {
                    "from its joints' planes; its roll the program's and the joints it reaches \
                     laid out better — automatic, untick to keep it"
                }
                (true, false) => {
                    "from its joints' planes; its roll the program's, but the joints it reaches \
                     not laid out better — proposed"
                }
                _ => "from its joints' planes; its roll given by hand — proposed",
            },
            r.words(),
            effect(&turn_key(true, &r.members[0]))
        );
        if ui.checkbox(&mut on, words).changed() {
            plane_ticks.insert(r.members[0].clone(), on);
            if on {
                for (x, _) in &p.turns {
                    if r.members
                        .iter()
                        .any(|m| m == &x.member || x.partners.contains(m))
                    {
                        ticked.remove(&x.member);
                    }
                }
            }
        }
    }
    if let Some((w0, e0, p0)) = p.without {
        let (w1, e1, p1) = run_counts(&p.run);
        ui.label(
            egui::RichText::new(format!(
                "With the turns ticked (every joint they reach laid out with them): {w1} \
                 joint(s) laid out whole, {e1} member end(s), {p1} plate(s); without them: \
                 {w0}, {e0}, {p0} (measured: the layout run both ways)"
            ))
            .small(),
        );
    }
}

/// The turns offered for an outstanding leg pointing into the flange (O44
/// L25): each with its tick (off by default: the roll is the designer's)
/// and its effect as weighed over every joint the member reaches.
fn layout_flange_turns(ui: &mut egui::Ui, p: &Preview, ticks: &mut BTreeSet<String>) {
    if p.flange_offered.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "Members with an outstanding leg pointing into the flange, a turn offered: {} \
                 (their rolls given by hand) — each applied only when ticked (with the run, one \
                 undo step)",
                p.flange_offered.len()
            ))
            .small()
            .color(ui.visuals().warn_fg_color),
        );
        ohl_ui::tool_window::question(ui, FLANGE_TURN_HELP);
    });
    for (m, r, _) in &p.flange_offered {
        let mut on = ticks.contains(m);
        if ui.checkbox(&mut on, r.words.clone()).changed() {
            if on {
                ticks.insert(m.clone());
            } else {
                ticks.remove(m);
            }
        }
    }
}

/// The help of the turns for an outstanding leg pointing into the flange
/// (O44 L25).
pub const FLANGE_TURN_HELP: &str = "A member end whose outstanding leg points into the flange it \
     bolts onto: a plate in its own plane welded to the flat plate would have that leg stand on \
     the flat plate, so the end is left for a hand detail. The layout lays the member out turned \
     by half a turn (the same leg bolted) and by a quarter turn either way (its other leg bolted: \
     on an unequal angle the other width, with its own bolts, gauges and edge distances — only \
     where no leg is given), each weighed over every joint the member reaches (both its ends, \
     after writing: each joint no worse, one better); the one laying out more member ends is \
     kept, on a tie the half turn. Where the member's roll is the program's (an import's) the \
     turn is applied and written in the layout record; where its roll is given by hand it is \
     offered here, never applied unasked — tick it to apply it with the run. The words say its \
     effect as weighed.";

/// The help of the leg pieces turned against their joints (O44 L11).
pub const LEG_ROLL_HELP: &str = "A leg piece, or a chord running on through a joint (an arm's \
     chord, a body's member running on), whose joints say it is turned about its axis: the \
     member ends bolting onto it arrive in planes (the plane through its axis and theirs) turned \
     against its flanges by about the same angle — at least two members, all the same way, \
     within 4° of each other, more than the owner's 2°. The tower's faces through the \
     neighbouring legs (Orient angles) are said beside a leg as a hint only. Never applied \
     silently: tick it to apply its turn with the run (the pieces in line with it turned alike, \
     one leg or chord; one undo step with the layout) — the preview then shows the run with and \
     without the turns ticked. Beside it (O44 L18) the turn from the joints' planes: the whole leg \
     or chord in line turned so that the most ends bolting onto it lie within 2° of its flanges' \
     planes at all its joints, centred in the turns that keep them so; ticked by default where \
     its roll is the program's (Orient angles, the layout: an import's) and the joints it reaches \
     are laid out better with it (at none fewer ends nor as many on more plates), else proposed. \
     One turn per member: ticking one clears the other. Each shows its effect over every \
     joint its members reach (all the proposals of its kind applied together, the joints laid out \
     without and with them): a turn is never judged on one joint. Joint details › Solve the \
     clashes offers the same turns per joint with the joint's solution worked out with them.";

/// The help of the pieces in line (O44 J9).
pub const IN_LINE_HELP: &str = "Two pieces of one section in line through a joint (within 1°, \
     turned alike within 2°), with nothing given there of how they are joined: the layout proposes \
     them one part (continuous) for you to confirm. Until confirmed each piece ends at the joint \
     in the joint checks and in the layout, its steel from its cut (on the safe side) — never a \
     silent default. Confirming writes both ends one part in one undo step (rolls a little \
     apart: both the roll half-way between); the preview is then worked out again. Pieces of a \
     leg with bolts are a leg splice (a command of its own, to come) and are not proposed.";

/// The pieces in line of a preview (O44 J9, D15): those proposed one part,
/// each joint's with a button, and *Confirm all*; those not proposed with
/// why. The pieces to confirm when a button was pressed.
fn layout_in_line(ui: &mut egui::Ui, p: &Preview) -> Option<Vec<ohl_structure_link::InLine>> {
    // (The pieces of a leg with bolts are leg splice places: J7's list.)
    let list: Vec<&ohl_structure_link::InLine> =
        p.run.in_line.iter().filter(|x| !x.splice_bolts).collect();
    if list.is_empty() {
        return None;
    }
    let mut out = None;
    let yes: Vec<&ohl_structure_link::InLine> =
        list.iter().copied().filter(|x| x.proposed()).collect();
    let no: Vec<&ohl_structure_link::InLine> =
        list.iter().copied().filter(|x| !x.proposed()).collect();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.strong(format!(
            "Pieces in line: {} pair(s) proposed one part",
            yes.len()
        ))
        .on_hover_text(IN_LINE_HELP);
        ohl_ui::tool_window::question(ui, IN_LINE_HELP);
        if !yes.is_empty()
            && ui
                .button(format!("Confirm all ({})", yes.len()))
                .on_hover_text("Writes every proposed pair one part (continuous), one undo step")
                .clicked()
        {
            out = Some(yes.iter().map(|x| (*x).clone()).collect());
        }
    });
    egui::CollapsingHeader::new(format!(
        "Each joint ({} proposed, {} not)",
        yes.len(),
        no.len()
    ))
    .id_salt("layout-in-line")
    .show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("layout-in-line-list")
            .max_height(160.0)
            .show(ui, |ui| {
                let mut joints: Vec<&str> = Vec::new();
                for x in &yes {
                    if !joints.contains(&x.joint.as_str()) {
                        joints.push(&x.joint);
                    }
                }
                for j in joints {
                    let here: Vec<&ohl_structure_link::InLine> =
                        yes.iter().copied().filter(|x| x.joint == j).collect();
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("Confirm")
                            .on_hover_text("These pieces one part (one undo step)")
                            .clicked()
                        {
                            out = Some(here.iter().map(|x| (*x).clone()).collect());
                        }
                        let words: Vec<String> = here
                            .iter()
                            .map(|x| match x.also() {
                                Some(a) => format!("{} and {} ({a})", x.member, x.onto),
                                None => format!("{} and {}", x.member, x.onto),
                            })
                            .collect();
                        ui.label(format!("{j}: {} one part?", words.join(", ")));
                    });
                }
                for x in &no {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}: not proposed — {}",
                            x.words(),
                            x.not.as_deref().unwrap_or_default()
                        ))
                        .small()
                        .weak(),
                    );
                }
            });
    });
    out
}

/// The leg splice places to confirm: the pieces of one section one part,
/// the splices.
type LegPlaces = (
    Vec<ohl_structure_link::InLine>,
    Vec<ohl_structure_link::LegSplice>,
);

/// The help of the leg splice places (O44 J7).
pub const LEG_SPLICES_HELP: &str = "Where a leg's pieces meet in line (the owner's rule of \
     2026-11-19): a change of section is a splice — proposed with covers outer and inner on each \
     leg bolted, as thick as the thicker piece's leg (the program's starting value, checked as \
     plates), the import's bolts, else provisional ones; the same section is one part unless \
     marked — proposed one part (its pieces' bolts then not checked there: not on the safe side \
     if a splice of one section is really there; mark it by giving the splice by hand). Never \
     written silently: until confirmed each piece ends at the joint in the joint checks and in \
     the layout (on the safe side). Confirmed, bracing at the joint may bolt through the \
     splice's outer cover. One undo step; the preview is then worked out again.";

/// The leg splice places of a preview (O44 J7): the splices and the pieces
/// of one section proposed, each joint's with a button, and *Confirm all*;
/// those not proposed with why. What to confirm when a button was pressed.
fn layout_leg_splices(ui: &mut egui::Ui, p: &Preview) -> Option<LegPlaces> {
    let one: Vec<&ohl_structure_link::InLine> = p
        .run
        .in_line
        .iter()
        .filter(|x| x.splice_bolts && x.proposed())
        .collect();
    let yes: Vec<&ohl_structure_link::LegSplice> =
        p.run.splices.iter().filter(|s| s.proposed()).collect();
    let no: Vec<&ohl_structure_link::LegSplice> =
        p.run.splices.iter().filter(|s| !s.proposed()).collect();
    if one.is_empty() && p.run.splices.is_empty() {
        return None;
    }
    let mut out = None;
    let all = one.len() + yes.len();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.strong(format!(
            "Leg splices: {} proposed spliced, {} one part",
            yes.len(),
            one.len()
        ))
        .on_hover_text(LEG_SPLICES_HELP);
        ohl_ui::tool_window::question(ui, LEG_SPLICES_HELP);
        if all > 0
            && ui
                .button(format!("Confirm all ({all})"))
                .on_hover_text(
                    "Writes every proposed leg splice and every leg's pieces of one section one \
                     part, one undo step",
                )
                .clicked()
        {
            out = Some((
                one.iter().map(|x| (*x).clone()).collect(),
                yes.iter().map(|x| (*x).clone()).collect(),
            ));
        }
    });
    egui::CollapsingHeader::new(format!("Each leg joint ({all} proposed, {} not)", no.len()))
        .id_salt("layout-leg-splices")
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("layout-leg-splices-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    let mut joints: Vec<&str> = Vec::new();
                    for j in yes
                        .iter()
                        .map(|s| s.joint.as_str())
                        .chain(one.iter().map(|x| x.joint.as_str()))
                    {
                        if !joints.contains(&j) {
                            joints.push(j);
                        }
                    }
                    for j in joints {
                        let s_here: Vec<&ohl_structure_link::LegSplice> =
                            yes.iter().copied().filter(|s| s.joint == j).collect();
                        let o_here: Vec<&ohl_structure_link::InLine> =
                            one.iter().copied().filter(|x| x.joint == j).collect();
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("Confirm")
                                .on_hover_text("This joint's leg splice place (one undo step)")
                                .clicked()
                            {
                                out = Some((
                                    o_here.iter().map(|x| (*x).clone()).collect(),
                                    s_here.iter().map(|x| (*x).clone()).collect(),
                                ));
                            }
                            let mut words: Vec<String> = s_here
                                .iter()
                                .map(|s| format!("{} and {} spliced?", s.member, s.onto))
                                .collect();
                            words.extend(o_here.iter().map(|x| match x.also() {
                                Some(a) => format!(
                                    "{} and {} one part? (the same section; {a})",
                                    x.member, x.onto
                                ),
                                None => format!(
                                    "{} and {} one part? (the same section)",
                                    x.member, x.onto
                                ),
                            }));
                            ui.label(format!("{j}: {}", words.join("; ")));
                        });
                        // What is assumed, wrapped below the row.
                        for s in &s_here {
                            ui.label(
                                egui::RichText::new(format!("Assumed: {}", s.assumed()))
                                    .small()
                                    .weak(),
                            );
                        }
                    }
                    for s in &no {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}: not proposed — {}",
                                s.words(),
                                s.not.as_deref().unwrap_or_default()
                            ))
                            .small()
                            .weak(),
                        );
                    }
                });
        });
    out
}

/// What changes per joint, and the joints left for a hand detail.
fn layout_changes(ui: &mut egui::Ui, p: &Preview) {
    let laid: Vec<&ohl_structure_link::JointOutcome> = p.run.laid_out().collect();
    egui::CollapsingHeader::new(format!("What changes ({} joints)", laid.len()))
        .id_salt("layout-changes")
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("layout-changes-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    for o in &laid {
                        ui.label(change_words(o));
                    }
                });
        });
    let hand: Vec<&ohl_structure_link::JointOutcome> = p.run.for_hand().collect();
    egui::CollapsingHeader::new(format!("Needs a hand detail ({} joints)", hand.len()))
        .id_salt("layout-hand")
        .default_open(!hand.is_empty() && hand.len() <= 5)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("layout-hand-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    for o in &hand {
                        ui.label(
                            egui::RichText::new(hand_words(o)).color(ui.visuals().warn_fg_color),
                        );
                    }
                });
        });
}

/// A joint left for a hand detail in words (J2, 2026-11-19): which members
/// are left — the rest of the joint laid out — and whose place is kept free
/// (every end laid out keeps clear of it), then why: "N2: by hand R1 (its
/// place kept free), the rest laid out — R1: too short …"; a joint left
/// whole: "N3: left whole — …".
pub fn hand_words(o: &ohl_structure_link::JointOutcome) -> String {
    let why = o.hand.join("; ");
    if !o.written {
        return format!("{}: left whole — {why}", o.joint);
    }
    let members: Vec<String> = o
        .plan
        .hand_members
        .iter()
        .map(|m| {
            if o.plan.kept_free.contains(m) {
                format!("{m} (its place kept free)")
            } else {
                m.clone()
            }
        })
        .collect();
    if members.is_empty() {
        return format!("{}: {why}", o.joint);
    }
    format!(
        "{}: by hand {}, the rest laid out — {why}",
        o.joint,
        members.join(", ")
    )
}

/// One joint's changes in words: "N2: D1 steel stops at 34 mm, onto L2;
/// D2 … ; plate G1 7 mm (D1, D2)".
pub fn change_words(o: &ohl_structure_link::JointOutcome) -> String {
    let mut parts: Vec<String> = o
        .plan
        .ends
        .iter()
        .map(|e| {
            let mut w = e.member.clone();
            if let Some(c) = e.cut {
                w.push_str(&format!(" steel stops at {:.0} mm", c * 1000.0));
            }
            if let Some(s) = e.shift.filter(|s| s.abs() >= 5e-5) {
                w.push_str(&format!(", shift {:+.1} mm", s * 1000.0));
            }
            if e.off.abs() > 1e-6 {
                w.push_str(&format!(
                    " (gauge line {:.0} mm off the work point)",
                    e.off.abs() * 1000.0
                ));
            }
            match &e.onto {
                ohl_structure_link::Onto::Member(m) => w.push_str(&format!(", onto {m}")),
                ohl_structure_link::Onto::Plate(k) => {
                    let id = o.plates.get(*k).cloned().unwrap_or_default();
                    w.push_str(&format!(", onto plate {id}"));
                }
            }
            if let Some(b) = &e.bolts {
                w.push_str(&format!(", {} provisional bolt(s)", b.count));
            }
            w
        })
        .collect();
    for (id, pl) in o.plates.iter().zip(&o.plan.plates) {
        parts.push(format!(
            "plate {id} {:.0} mm on {} ({})",
            pl.thickness * 1000.0,
            pl.on,
            pl.members.join(", ")
        ));
    }
    if !o.turned.is_empty() {
        parts.push(format!("turned flat: {}", o.turned.join(", ")));
    }
    format!("{}: {}", o.joint, parts.join("; "))
}

/// Each joint's close-up before and after (turned together by a drag).
fn layout_close_ups(
    ui: &mut egui::Ui,
    shown: &mut Option<String>,
    views: &mut (
        crate::joint_panel::JointPanel,
        crate::joint_panel::JointPanel,
    ),
    p: &Preview,
    dark: bool,
) {
    let laid: Vec<String> = p.run.laid_out().map(|o| o.joint.clone()).collect();
    if laid.is_empty() {
        return;
    }
    let mut pick = shown
        .clone()
        .filter(|j| laid.contains(j))
        .unwrap_or_else(|| laid[0].clone());
    ui.horizontal(|ui| {
        ui.label("Before and after:");
        egui::ComboBox::from_id_salt("layout-shown")
            .selected_text(pick.clone())
            .height(240.0)
            .show_ui(ui, |ui| {
                for j in &laid {
                    ui.selectable_value(&mut pick, j.clone(), j);
                }
            });
    });
    *shown = Some(pick.clone());
    let (Some(a), Some(b)) = (p.before.joint(&pick), p.after.joint(&pick)) else {
        return;
    };
    before_after(ui, views, a, b, p.key, dark);
}

/// A joint's close-up before (`a`) and after (`b`) side by side, turned
/// together by a drag, each with its clashes counted (Lay out joints,
/// Solve the clashes).
pub(crate) fn before_after(
    ui: &mut egui::Ui,
    views: &mut (
        crate::joint_panel::JointPanel,
        crate::joint_panel::JointPanel,
    ),
    a: &ohl_structure_link::JointDetail,
    b: &ohl_structure_link::JointDetail,
    key: u64,
    dark: bool,
) {
    let width = ((ui.available_width() - 8.0) / 2.0).max(120.0);
    let size = egui::vec2(width, 200.0);
    ui.horizontal(|ui| {
        for (k, j) in [(0u64, a), (1u64, b)] {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(if k == 0 { "Before" } else { "After" })
                        .small()
                        .strong(),
                );
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
                if response.dragged() {
                    let d = response.drag_delta();
                    for v in [&mut views.0, &mut views.1] {
                        v.yaw += d.x * 0.01;
                        v.pitch = (v.pitch + d.y * 0.01).clamp(-1.5, 1.5);
                    }
                }
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
                let view = if k == 0 { &mut views.0 } else { &mut views.1 };
                crate::close_up::paint(&painter, rect, j, view, (key, k), dark, 0);
                let clashes = j.clashes.len();
                ui.label(
                    egui::RichText::new(format!("{clashes} clash(es)"))
                        .small()
                        .color(if clashes > 0 {
                            ui.visuals().warn_fg_color
                        } else {
                            ui.visuals().weak_text_color()
                        }),
                );
            });
        }
    });
}

/// A usage job polled: its result into `usage` when done; the error when
/// it failed.
pub(crate) fn poll_usage_job(
    job: &mut Job<UsageCompare>,
    usage: &mut Option<UsageCompare>,
    ctx: &egui::Context,
) -> Option<String> {
    let (_, rx) = job.as_ref()?;
    match rx.try_recv() {
        Ok(Ok(u)) => {
            *usage = Some(u);
            *job = None;
            None
        }
        Ok(Err(e)) => {
            *job = None;
            Some(e)
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            None
        }
        Err(_) => {
            *job = None;
            None
        }
    }
}

/// The usage before and after (asked by a button: the analysis of the last
/// run's cases twice); true when asked.
pub(crate) fn layout_usage(
    ui: &mut egui::Ui,
    u: Option<&UsageCompare>,
    key: u64,
    busy: bool,
) -> bool {
    let mut asked = false;
    ui.horizontal(|ui| {
        ohl_ui::tool_window::label(
            ui,
            "Usage before and after",
            "Each member's largest usage over the last run's cases (Results › Run), as the file \
             is and as laid out: the shifts are rigid end offsets of frame members (moments), \
             the cuts and plates do not enter the members' check. The analysis runs twice.",
        );
        if busy {
            ui.spinner();
        } else if ui.button("Work it out").clicked() {
            asked = true;
        }
    });
    if let Some(u) = u.filter(|u| u.key == key) {
        let changed = u
            .rows
            .iter()
            .filter(|r| (r.2.unwrap_or(0.0) - r.1.unwrap_or(0.0)).abs() > 1e-6)
            .count();
        ui.label(format!(
            "{} case(s), {:.1} s (measured); {changed} member(s) change.",
            u.cases, u.seconds
        ));
        egui::ScrollArea::vertical()
            .id_salt("layout-usage")
            .max_height(120.0)
            .show(ui, |ui| {
                egui::Grid::new("layout-usage-grid")
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Member");
                        ui.strong("Before");
                        ui.strong("After");
                        ui.end_row();
                        let pct = |x: Option<f64>| {
                            x.map_or("—".to_owned(), |v| format!("{:.1} %", v * 100.0))
                        };
                        for r in u.rows.iter().take(40) {
                            ui.label(&r.0);
                            ui.label(pct(r.1));
                            ui.label(pct(r.2));
                            ui.end_row();
                        }
                    });
            });
    }
    asked
}

impl StructureApp {
    /// Joint details' layout box: **Lay out joint**, and what the
    /// automatic layout set at joint `joint` (the values *automatic*, a run
    /// again or a later edit may replace them), the bolts provisional, a
    /// hand detail needed with why.
    pub(crate) fn layout_box(&mut self, ui: &mut egui::Ui, joint: &str) {
        let Some(f) = self.file.as_ref() else { return };
        let rec = LayoutRecord::of(f.structure());
        let mut open = false;
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Lay out joint…")
                .on_hover_text(
                    "Places this joint's member ends so that they fit (pulled back, moved \
                     sideways only where needed, onto the leg or onto plates), with its similar \
                     and symmetric joints: a preview first, Apply one undo step",
                )
                .clicked()
            {
                open = true;
            }
            if rec.joints.contains(joint) {
                let auto: Vec<String> = rec
                    .ends
                    .iter()
                    .filter(|((_, j), f)| j == joint && !f.is_empty())
                    .map(|((m, _), f)| {
                        // (An `onto` given by hand the layout kept, L2's
                        // follow-up: said as yours.)
                        let words: Vec<String> = f
                            .iter()
                            .map(|x| match x.strip_prefix(ohl_structure_link::ONTO_HAND) {
                                Some(h) => format!("onto (yours: {h}, kept)"),
                                None => x.clone(),
                            })
                            .collect();
                        format!("{m}: {}", words.join(", "))
                    })
                    .chain(
                        rec.made
                            .iter()
                            .filter(|(_, (j, _))| j == joint)
                            .map(|(id, _)| format!("plate {id}")),
                    )
                    .collect();
                ui.label(egui::RichText::new("laid out automatically").italics())
                    .on_hover_text(format!(
                        "Automatic (the layout's; a run again or a later edit near this joint \
                         may replace them, a value you change becomes yours): {}",
                        auto.join("; ")
                    ));
            }
            let provisional = rec
                .provisional
                .iter()
                .filter(|(_, j)| j == joint)
                .map(|(m, _)| m.as_str())
                .collect::<Vec<_>>();
            if !provisional.is_empty() {
                ui.label(
                    egui::RichText::new(format!("provisional bolts: {}", provisional.join(", ")))
                        .italics()
                        .color(ui.visuals().warn_fg_color),
                )
                .on_hover_text(
                    "The layout's bolts (a class preset or the bolt picked), not chosen yet: the \
                     tower stays at level 3 for the joint details until none is provisional",
                );
            }
        });
        if let Some(why) = rec.hand.get(joint) {
            ui.label(
                egui::RichText::new(format!("Needs a hand detail: {why}"))
                    .small()
                    .color(ui.visuals().warn_fg_color),
            );
        }
        if open {
            self.open_layout_window(Scope::Joint(joint.to_owned()));
        }
    }
}
