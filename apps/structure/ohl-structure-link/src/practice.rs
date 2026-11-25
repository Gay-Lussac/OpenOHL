// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **detailing practice** of the automatic joint layout (roadmap O44
//! L24; approved by the owner on 2026-11-24 with the plugin interface
//! change it needs): the shop's preferences the layout follows, as data —
//! the fabrication order, the face preferences, the limits, the flatness,
//! the plates' shapes and bend radius, the picked bolt's pattern. The
//! engine that builds, checks and validates the layout stays here (one
//! source of truth with the joint details; a plugin never certifies its own
//! result): a practice only chooses among what the engine already does, and
//! the layout's clash checks, joint checks and "no joint worse" guards stay
//! in force whatever it says.
//!
//! **Where it comes from**, first found: the structure's own (the layout
//! record's `[generator.practice]` with `source = "project"`); the code
//! plugin of the structure's member check (its optional
//! `detailing-practice` export, [`plugin_practice`]); else the default
//! ([`DetailingPractice::default`]), which is the layout as it was before
//! L24 exactly — every value the constant the code had.
//!
//! **Checked here, every value** ([`DetailingPractice::parse`]): a key
//! not known, a value out of its range or a fabrication order the engine
//! does not follow is refused with what, why and what to give instead; a
//! refused practice is not used (the default is, said). The ranges keep
//! the layout on the safe side: the flatness never looser than the joint
//! details' 2°, the shift and the welded plate's axis offset never beyond
//! what the estimated eccentricities take as their upper bounds (O45, until
//! J5), a bend never tighter than one plate thickness, no single bolt (J8).
//!
//! **Expected cost**: reading a practice is microseconds; applying it
//! changes what the layout tries, not how long one try takes (leaving a
//! fabrication kind out makes a run a little faster, never slower).

use toml::{Table, Value};

use crate::joint_layout::{
    BEND_RADIUS_RATIO, BEND_SMALL_MAX, BETWEEN_LEAST, BoltArea, GAP_START, LayoutSettings,
    OWN_OUT_MAX, PULL_BACK_START, SHIFT_LIMIT_START,
};
use crate::joints::FLAT_OFF;

/// The format name of a practice's text (a plugin's answer, a file).
pub const FORMAT: &str = "ohl-detailing-practice";
/// The format's version.
pub const VERSION: i64 = 1;

/// The gap between steel a practice may give, m: from twice the clash
/// check's tolerance (a smaller gap the clash check would not see) to the
/// Lay out joints window's largest.
pub const GAP_RANGE: (f64, f64) = (2.0 * crate::clash::CLASH_TOLERANCE, 0.100);
/// The longest pull-back a practice may give, m (the window's range).
pub const PULL_BACK_RANGE: (f64, f64) = (0.010, 2.0);
/// The largest sideways shift a practice may give, m: up to the program's
/// 30 mm — the upper bound the estimated eccentricities take where no plan
/// exists (O45, decided 2026-11-19: until J5 takes larger shifts into the
/// member checks).
pub const SHIFT_RANGE: (f64, f64) = (0.0, SHIFT_LIMIT_START);
/// The flatness a practice may give, degrees: never looser than the joint
/// details' [`FLAT_OFF`] (the owner's 2°, 2026-11-19), at least 1°.
pub const FLATNESS_RANGE: (f64, f64) = (1.0, FLAT_OFF);
/// The largest small bend a practice may give, degrees (its least is the
/// practice's flatness): up to the program's [`BEND_SMALL_MAX`], the range
/// the bent plate is planned for.
pub const SMALL_BEND_MAX: f64 = BEND_SMALL_MAX;
/// The welded plate's axis offset a practice may give, m: up to the
/// program's [`OWN_OUT_MAX`] (what the estimated eccentricities carry).
pub const OWN_OFFSET_RANGE: (f64, f64) = (0.0, OWN_OUT_MAX);
/// A bent plate's inside radius a practice may give, in plate thicknesses:
/// at least one thickness (the code plugins' bend checks check the radius
/// against their code's least; tighter is refused here).
pub const BEND_RADIUS_RANGE: (f64, f64) = (1.0, 10.0);
/// The least gap for a plate between a built-up member's angles a practice
/// may give, in thicknesses of the thickest leg on it: from the program's
/// [`BETWEEN_LEAST`].
pub const BETWEEN_RANGE: (f64, f64) = (BETWEEN_LEAST, 4.0);
/// The bolts of the picked bolt's pattern a practice may give per end.
pub const BOLT_COUNT_RANGE: (u32, u32) = (2, 8);

/// A kind of fabrication, in the engine's order (fewer parts, no weld to
/// inspect, first; the owner's order of 2026-11-23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fabrication {
    /// Bolted directly onto a flange.
    Direct,
    /// Onto one flat plate on the flange.
    Plate,
    /// Onto a plate with one bend (round the heel, J2b; by a few degrees,
    /// L3).
    Bend,
    /// Onto a plate with several bends (not built yet: accepted, never
    /// made).
    Bends,
    /// Onto a plate in the member's own plane welded to the flat plate
    /// (step 4).
    Welded,
}

impl Fabrication {
    /// Every kind, in the engine's order.
    pub const ALL: [Self; 5] = [
        Self::Direct,
        Self::Plate,
        Self::Bend,
        Self::Bends,
        Self::Welded,
    ];

    /// Its word in a practice's text.
    pub fn word(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Plate => "plate",
            Self::Bend => "bend",
            Self::Bends => "bends",
            Self::Welded => "welded",
        }
    }

    /// From its word.
    pub fn parse(w: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.word() == w)
    }

    /// In words, for the window.
    pub fn label(self) -> &'static str {
        match self {
            Self::Direct => "bolted directly onto the flange",
            Self::Plate => "one flat plate",
            Self::Bend => "a plate with one bend",
            Self::Bends => "a plate with several bends (not built yet)",
            Self::Welded => "a plate welded in the member's own plane",
        }
    }
}

/// Which face of a leg's flange a face's ends are tried on first (L10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FacePattern {
    /// The photographs' pattern: a diagonal onto the inner face, a
    /// horizontal onto the outer one (the default).
    DiagonalInner,
    /// The later class first.
    LaterClassFirst,
}

impl FacePattern {
    fn word(self) -> &'static str {
        match self {
            Self::DiagonalInner => "diagonal-inner",
            Self::LaterClassFirst => "later-class-first",
        }
    }
}

/// A detailing practice (see the module). Lengths in m, angles in degrees.
#[derive(Debug, Clone, PartialEq)]
pub struct DetailingPractice {
    /// Its name, shown with its source ("" none).
    pub name: String,
    /// The kinds of fabrication the shop makes, in the engine's order
    /// (`direct` and `plate` always; `bend`, `bends`, `welded` optional).
    pub fabrication: Vec<Fabrication>,
    /// Which face of a leg's flange is tried first (L10).
    pub face_pattern: FacePattern,
    /// Members on either side of a leg's flange (L10's other face).
    pub both_sides_of_flange: bool,
    /// Ends on both faces of one plate (L11).
    pub both_faces_of_plate: bool,
    /// The gap between steel that does not bolt together, to start with.
    pub gap: f64,
    /// The longest pull-back before a shift is tried, to start with.
    pub pull_back: f64,
    /// The largest sideways shift, to start with.
    pub shift: f64,
    /// The most an end's leg may lie off its flange and still bolt flat.
    pub flatness_deg: f64,
    /// The most an end's leg may lie off its flange for a plate bent by as
    /// much (L3).
    pub small_bend_deg: f64,
    /// The most a plate welded in its member's own plane may put its axis
    /// farther out of the flange than a flat plate (L20 item 2).
    pub own_plane_offset: f64,
    /// A bent plate's inside radius, in plate thicknesses.
    pub bend_radius: f64,
    /// The least gap for a plate between a built-up member's angles, in
    /// thicknesses of the thickest leg on it (L15).
    pub between_least: f64,
    /// A plate may stop short of the work point (L2's follow-up).
    pub short_plates: bool,
    /// The picked bolt's pattern: bolts per end.
    pub bolt_count: u32,
    /// The picked bolt's pattern: gauge lines (1 only, for now).
    pub bolt_lines: u32,
}

impl Default for DetailingPractice {
    /// The layout as it was before L24, exactly.
    fn default() -> Self {
        Self {
            name: String::new(),
            fabrication: vec![
                Fabrication::Direct,
                Fabrication::Plate,
                Fabrication::Bend,
                Fabrication::Welded,
            ],
            face_pattern: FacePattern::DiagonalInner,
            both_sides_of_flange: true,
            both_faces_of_plate: true,
            gap: GAP_START,
            pull_back: PULL_BACK_START,
            shift: SHIFT_LIMIT_START,
            flatness_deg: FLAT_OFF,
            small_bend_deg: BEND_SMALL_MAX,
            own_plane_offset: OWN_OUT_MAX,
            bend_radius: BEND_RADIUS_RATIO,
            between_least: BETWEEN_LEAST,
            short_plates: true,
            bolt_count: 2,
            bolt_lines: 1,
        }
    }
}

/// Where a practice comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PracticeSource {
    /// The program's default.
    Default,
    /// A code plugin's `detailing-practice` export.
    Plugin {
        /// The plugin's name.
        plugin: String,
        /// Its method.
        method: String,
    },
    /// The structure's own (the layout record's).
    Project,
}

impl PracticeSource {
    /// As the record writes it.
    pub fn word(&self) -> String {
        match self {
            Self::Default => "default".to_owned(),
            Self::Plugin { plugin, method } => format!("plugin:{plugin}/{method}"),
            Self::Project => "project".to_owned(),
        }
    }

    /// From the record's word.
    pub fn parse(w: &str) -> Option<Self> {
        match w {
            "default" => Some(Self::Default),
            "project" => Some(Self::Project),
            _ => {
                let (plugin, method) = w.strip_prefix("plugin:")?.split_once('/')?;
                (!plugin.is_empty() && !method.is_empty()).then(|| Self::Plugin {
                    plugin: plugin.to_owned(),
                    method: method.to_owned(),
                })
            }
        }
    }

    /// In words, for the window.
    pub fn label(&self) -> String {
        match self {
            Self::Default => "the program's default".to_owned(),
            Self::Plugin { plugin, method } => {
                format!("the code plugin {plugin} (method {method})")
            }
            Self::Project => "this structure's own".to_owned(),
        }
    }
}

/// A practice and where it comes from: what a run uses and the record
/// keeps.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedPractice {
    /// Where it comes from.
    pub source: PracticeSource,
    /// The practice.
    pub practice: DetailingPractice,
}

/// The keys a practice's text may have, by table.
const TOP_KEYS: [&str; 9] = [
    "format",
    "version",
    "name",
    "source",
    "fabrication",
    "faces",
    "limits",
    "plates",
    "bolts",
];
const FACE_KEYS: [&str; 3] = ["pattern", "both_sides_of_flange", "both_faces_of_plate"];
const LIMIT_KEYS: [&str; 6] = [
    "gap",
    "pull_back",
    "shift",
    "flatness_deg",
    "small_bend_deg",
    "own_plane_offset",
];
const PLATE_KEYS: [&str; 3] = ["bend_radius", "between_least", "short"];
const BOLT_KEYS: [&str; 2] = ["count", "lines"];

impl DetailingPractice {
    /// The practice of a practice file's text (a plugin's answer), checked
    /// ([`Self::from_table`]).
    pub fn parse(text: &str) -> Result<Self, String> {
        let t: Table = toml::from_str(text)
            .map_err(|e| format!("the detailing practice does not read as TOML: {e}"))?;
        Self::from_table(&t)
    }

    /// The practice of table `t` (a practice file's, or the record's
    /// `practice`), every value checked. A key missing takes the default's
    /// value; an unknown key, a value of the wrong kind or out of its range,
    /// or a fabrication order the engine does not follow is refused — every
    /// refusal said, each with why and what to give instead.
    pub fn from_table(t: &Table) -> Result<Self, String> {
        let mut p = Self::default();
        let mut errs: Vec<String> = Vec::new();
        unknown(t, &TOP_KEYS, "", &mut errs);
        if let Some(f) = t.get("format")
            && f.as_str() != Some(FORMAT)
        {
            errs.push(format!("format: \"{FORMAT}\" expected"));
        }
        if let Some(v) = t.get("version")
            && v.as_integer() != Some(VERSION)
        {
            errs.push(format!(
                "version: {VERSION} expected (this program reads version {VERSION})"
            ));
        }
        if let Some(n) = t.get("name") {
            match n.as_str() {
                Some(s) => p.name = s.to_owned(),
                None => errs.push("name: a text expected".to_owned()),
            }
        }
        if let Some(f) = t.get("fabrication") {
            match f.as_array() {
                Some(a) => {
                    let mut kinds = Vec::new();
                    for v in a {
                        match v.as_str().and_then(Fabrication::parse) {
                            Some(k) => kinds.push(k),
                            None => errs.push(format!(
                                "fabrication: {v} is not a kind the layout knows (give \
                                 \"direct\", \"plate\", \"bend\", \"bends\", \"welded\")"
                            )),
                        }
                    }
                    p.fabrication = kinds;
                }
                None => errs.push("fabrication: a list of kinds expected".to_owned()),
            }
        }
        let table = |k: &str, errs: &mut Vec<String>| -> Option<Table> {
            match t.get(k) {
                None => None,
                Some(Value::Table(x)) => Some(x.clone()),
                Some(_) => {
                    errs.push(format!("{k}: a table expected"));
                    None
                }
            }
        };
        if let Some(f) = table("faces", &mut errs) {
            unknown(&f, &FACE_KEYS, "faces.", &mut errs);
            if let Some(v) = f.get("pattern") {
                match v.as_str() {
                    Some("diagonal-inner") => p.face_pattern = FacePattern::DiagonalInner,
                    Some("later-class-first") => p.face_pattern = FacePattern::LaterClassFirst,
                    _ => errs.push(
                        "faces.pattern: \"diagonal-inner\" or \"later-class-first\" expected"
                            .to_owned(),
                    ),
                }
            }
            flag(
                &f,
                "both_sides_of_flange",
                "faces.",
                &mut p.both_sides_of_flange,
                &mut errs,
            );
            flag(
                &f,
                "both_faces_of_plate",
                "faces.",
                &mut p.both_faces_of_plate,
                &mut errs,
            );
        }
        if let Some(l) = table("limits", &mut errs) {
            unknown(&l, &LIMIT_KEYS, "limits.", &mut errs);
            length(&l, "gap", "limits.", &mut p.gap, &mut errs);
            length(&l, "pull_back", "limits.", &mut p.pull_back, &mut errs);
            length(&l, "shift", "limits.", &mut p.shift, &mut errs);
            degrees(
                &l,
                "flatness_deg",
                "limits.",
                &mut p.flatness_deg,
                &mut errs,
            );
            degrees(
                &l,
                "small_bend_deg",
                "limits.",
                &mut p.small_bend_deg,
                &mut errs,
            );
            length(
                &l,
                "own_plane_offset",
                "limits.",
                &mut p.own_plane_offset,
                &mut errs,
            );
        }
        if let Some(x) = table("plates", &mut errs) {
            unknown(&x, &PLATE_KEYS, "plates.", &mut errs);
            number(&x, "bend_radius", "plates.", &mut p.bend_radius, &mut errs);
            number(
                &x,
                "between_least",
                "plates.",
                &mut p.between_least,
                &mut errs,
            );
            flag(&x, "short", "plates.", &mut p.short_plates, &mut errs);
        }
        if let Some(b) = table("bolts", &mut errs) {
            unknown(&b, &BOLT_KEYS, "bolts.", &mut errs);
            count(&b, "count", "bolts.", &mut p.bolt_count, &mut errs);
            count(&b, "lines", "bolts.", &mut p.bolt_lines, &mut errs);
        }
        if errs.is_empty() {
            errs = p.refusals();
        }
        if errs.is_empty() {
            Ok(p)
        } else {
            Err(format!(
                "the detailing practice is refused: {}",
                errs.join("; ")
            ))
        }
    }

    /// What is refused in this practice's values (empty: none): each value
    /// out of its range, said with why and what to give instead, and a
    /// fabrication order the engine does not follow.
    pub fn refusals(&self) -> Vec<String> {
        let mut errs = Vec::new();
        let mm = |v: f64| format!("{:.0} mm", v * 1000.0);
        let range = |errs: &mut Vec<String>, v: f64, (lo, hi): (f64, f64), what: &str| {
            if !(v.is_finite() && v >= lo - 1e-12 && v <= hi + 1e-12) {
                errs.push(what.to_owned());
            }
        };
        // The fabrication order.
        let f = &self.fabrication;
        for k in Fabrication::ALL {
            if f.iter().filter(|x| **x == k).count() > 1 {
                errs.push(format!("fabrication: \"{}\" given twice", k.word()));
            }
        }
        for k in [Fabrication::Direct, Fabrication::Plate] {
            if !f.contains(&k) {
                errs.push(format!(
                    "fabrication: \"{}\" left out — the layout's first two steps are bolting \
                     directly and one flat plate, and it has no way without them (a joint it \
                     cannot lay out so is left for a hand detail): give \"direct\" and \"plate\"",
                    k.word()
                ));
            }
        }
        if f.windows(2).any(|w| w[0] > w[1]) {
            errs.push(
                "fabrication: the kinds out of the engine's order — the layout tries them as \
                 direct, plate, bend, bends, welded (fewer parts, no weld to inspect, first) \
                 and does not follow another order yet: give them in that order, leaving out \
                 those the shop does not make"
                    .to_owned(),
            );
        }
        range(
            &mut errs,
            self.gap,
            GAP_RANGE,
            &format!(
                "limits.gap {}: give {} to {} (below, the clash check's tolerance would not see \
                 the gap)",
                mm(self.gap),
                mm(GAP_RANGE.0),
                mm(GAP_RANGE.1)
            ),
        );
        range(
            &mut errs,
            self.pull_back,
            PULL_BACK_RANGE,
            &format!(
                "limits.pull_back {}: give {} to {}",
                mm(self.pull_back),
                mm(PULL_BACK_RANGE.0),
                mm(PULL_BACK_RANGE.1)
            ),
        );
        range(
            &mut errs,
            self.shift,
            SHIFT_RANGE,
            &format!(
                "limits.shift {}: give 0 to {} — a shift is an eccentricity, and the estimated \
                 eccentricities take {} as their upper bound where no plan exists (larger \
                 shifts wait on J5: the moment checked instead of the end refused)",
                mm(self.shift),
                mm(SHIFT_RANGE.1),
                mm(SHIFT_RANGE.1)
            ),
        );
        range(
            &mut errs,
            self.flatness_deg,
            FLATNESS_RANGE,
            &format!(
                "limits.flatness_deg {}°: give {}° to {}° — the joint details take an end as \
                 lying flat up to {}°; a looser practice would bolt ends flat that they see off \
                 their flange (a forced fit, its locked-in bending in no check)",
                self.flatness_deg, FLATNESS_RANGE.0, FLATNESS_RANGE.1, FLAT_OFF
            ),
        );
        let bend_lo = if self.flatness_deg.is_finite() {
            self.flatness_deg
        } else {
            FLATNESS_RANGE.0
        };
        range(
            &mut errs,
            self.small_bend_deg,
            (bend_lo, SMALL_BEND_MAX),
            &format!(
                "limits.small_bend_deg {}°: give the flatness ({bend_lo}°) to {SMALL_BEND_MAX}° — \
                 the bent plate is planned for a member's leg twisted a few degrees; beyond, \
                 a welded plate (step 4)",
                self.small_bend_deg
            ),
        );
        range(
            &mut errs,
            self.own_plane_offset,
            OWN_OFFSET_RANGE,
            &format!(
                "limits.own_plane_offset {}: give 0 to {} — the most the estimated \
                 eccentricities carry into the analysis (larger offsets wait on J5)",
                mm(self.own_plane_offset),
                mm(OWN_OFFSET_RANGE.1)
            ),
        );
        range(
            &mut errs,
            self.bend_radius,
            BEND_RADIUS_RANGE,
            &format!(
                "plates.bend_radius {}: give {} to {} plate thicknesses — a tighter bend may \
                 crack the plate (the code plugins' bend checks check the radius against their \
                 code)",
                self.bend_radius, BEND_RADIUS_RANGE.0, BEND_RADIUS_RANGE.1
            ),
        );
        range(
            &mut errs,
            self.between_least,
            BETWEEN_RANGE,
            &format!(
                "plates.between_least {}: give {} to {} thicknesses of the thickest leg on the \
                 plate — a thinner plate between a built-up member's angles is left for a hand \
                 detail",
                self.between_least, BETWEEN_RANGE.0, BETWEEN_RANGE.1
            ),
        );
        if !(BOLT_COUNT_RANGE.0..=BOLT_COUNT_RANGE.1).contains(&self.bolt_count) {
            errs.push(format!(
                "bolts.count {}: give {} to {} — one bolt per end is the codes' question (J8: \
                 where the code's text allows it), not a practice's",
                self.bolt_count, BOLT_COUNT_RANGE.0, BOLT_COUNT_RANGE.1
            ));
        }
        if self.bolt_lines != 1 {
            errs.push(format!(
                "bolts.lines {}: give 1 — the picked bolt in two gauge lines waits on the codes' \
                 bolt choice (a connection preset per class gives two lines)",
                self.bolt_lines
            ));
        }
        errs
    }

    /// The practice is the default's (its name aside).
    pub fn is_default(&self) -> bool {
        Self {
            name: String::new(),
            ..self.clone()
        } == Self::default()
    }

    /// Makes `kind`.
    pub fn makes(&self, kind: Fabrication) -> bool {
        self.fabrication.contains(&kind)
    }

    /// What is said with the practice (accepted, not acted on).
    pub fn notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.makes(Fabrication::Bends) {
            out.push(
                "\"bends\" (plates with several bends) is accepted but not built yet: the layout \
                 makes none"
                    .to_owned(),
            );
        }
        if !self.makes(Fabrication::Welded) {
            out.push(
                "no welding: no plate welded in a member's own plane, and the plates of a joint \
                 never welded together; ends that need one are left for a hand detail"
                    .to_owned(),
            );
        }
        out
    }

    /// The engine's settings `s` with this practice: the kinds it leaves
    /// out turned off, its values in place of the program's. It only
    /// restricts what `s` turns on — a setting off in `s` stays off — so the
    /// default practice leaves `s` exactly as it is. The gap, pull-back and
    /// shift are starting values (the window's own, recorded with a run),
    /// not set here.
    pub fn apply(&self, s: &mut LayoutSettings) {
        let bend = self.makes(Fabrication::Bend);
        s.bent_plates &= bend;
        s.small_bends &= bend;
        s.bend_facing &= bend;
        s.bend_back &= bend;
        let welded = self.makes(Fabrication::Welded);
        s.own_plane_welded &= welded;
        s.welded &= welded;
        s.face_pattern &= self.face_pattern == FacePattern::DiagonalInner;
        s.other_face &= self.both_sides_of_flange;
        s.both_faces &= self.both_faces_of_plate;
        s.short_plates &= self.short_plates;
        s.flat_off = self.flatness_deg;
        s.bend_small_max = self.small_bend_deg;
        s.own_out_max = self.own_plane_offset;
        s.bend_radius_ratio = self.bend_radius;
        s.between_least = self.between_least;
        if let Some(b) = s.bolt.as_mut()
            && b.preset.is_none()
            && b.lines == 1
            && b.count == 2
            && self.bolt_count != 2
        {
            *b = BoltArea::picked_count(b.bolt.clone(), b.diameter, b.flats, self.bolt_count);
        }
    }

    /// The practice as a practice file's table (every value written).
    pub fn to_table(&self) -> Table {
        let mut t = Table::new();
        t.insert("format".into(), Value::from(FORMAT));
        t.insert("version".into(), Value::from(VERSION));
        if !self.name.is_empty() {
            t.insert("name".into(), Value::from(self.name.as_str()));
        }
        t.insert(
            "fabrication".into(),
            Value::Array(
                self.fabrication
                    .iter()
                    .map(|f| Value::from(f.word()))
                    .collect(),
            ),
        );
        let mut faces = Table::new();
        faces.insert("pattern".into(), Value::from(self.face_pattern.word()));
        faces.insert(
            "both_sides_of_flange".into(),
            Value::from(self.both_sides_of_flange),
        );
        faces.insert(
            "both_faces_of_plate".into(),
            Value::from(self.both_faces_of_plate),
        );
        t.insert("faces".into(), Value::Table(faces));
        let mut limits = Table::new();
        limits.insert("gap".into(), Value::from(crate::mm_text(self.gap)));
        limits.insert(
            "pull_back".into(),
            Value::from(crate::mm_text(self.pull_back)),
        );
        limits.insert("shift".into(), Value::from(crate::mm_text(self.shift)));
        limits.insert("flatness_deg".into(), Value::from(self.flatness_deg));
        limits.insert("small_bend_deg".into(), Value::from(self.small_bend_deg));
        limits.insert(
            "own_plane_offset".into(),
            Value::from(crate::mm_text(self.own_plane_offset)),
        );
        t.insert("limits".into(), Value::Table(limits));
        let mut plates = Table::new();
        plates.insert("bend_radius".into(), Value::from(self.bend_radius));
        plates.insert("between_least".into(), Value::from(self.between_least));
        plates.insert("short".into(), Value::from(self.short_plates));
        t.insert("plates".into(), Value::Table(plates));
        let mut bolts = Table::new();
        bolts.insert("count".into(), Value::from(i64::from(self.bolt_count)));
        bolts.insert("lines".into(), Value::from(i64::from(self.bolt_lines)));
        t.insert("bolts".into(), Value::Table(bolts));
        t
    }

    /// The practice as a practice file's text.
    pub fn to_text(&self) -> String {
        toml::to_string(&self.to_table()).unwrap_or_default()
    }

    /// Its values for the window, each with its effect on the results and
    /// on the time: (what, value, effect).
    pub fn rows(&self) -> Vec<(&'static str, String, &'static str)> {
        let yes = |b: bool| if b { "yes" } else { "no" }.to_owned();
        vec![
            (
                "Fabrication order",
                self.fabrication
                    .iter()
                    .map(|f| f.word())
                    .collect::<Vec<_>>()
                    .join(" → "),
                "the kinds of parts an end may go onto, tried in this order; a kind left out \
                 is never made, and an end needing it is left for a hand detail (a run a \
                 little faster, never slower)",
            ),
            (
                "Faces of a leg's flange",
                match self.face_pattern {
                    FacePattern::DiagonalInner => "diagonal onto the inner face first".to_owned(),
                    FacePattern::LaterClassFirst => "the later class first".to_owned(),
                },
                "which of a face's ends is tried on the flange's other face first (L10); no \
                 change in time",
            ),
            (
                "Members on either side of a flange",
                yes(self.both_sides_of_flange),
                "no: no end is turned onto a flange's other face (L10) — fewer ends laid out \
                 where two meet on one face; a run a little faster",
            ),
            (
                "Both faces of one plate",
                yes(self.both_faces_of_plate),
                "no: an end on the inner side of a flange's plane stays on its own welded \
                 plate (L11); no change in time",
            ),
            (
                "Gap, pull-back, shift (to start with)",
                format!(
                    "{}, {}, {}",
                    crate::mm_text(self.gap),
                    crate::mm_text(self.pull_back),
                    crate::mm_text(self.shift)
                ),
                "the window's starting values; a smaller shift leaves more ends for a hand \
                 detail and lowers the estimated eccentricities' upper bound; no change in time",
            ),
            (
                "Flatness",
                format!("{}°", self.flatness_deg),
                "the most an end's leg may lie off its flange and still bolt flat; stricter: \
                 more ends on bent or welded plates (the joint details keep 2°); no change in \
                 time",
            ),
            (
                "Small bend, at most",
                format!("{}°", self.small_bend_deg),
                "the most a plate is bent by a few degrees (L3) before a welded plate; smaller: \
                 more welded plates; no change in time",
            ),
            (
                "Welded plate's axis offset, at most",
                crate::mm_text(self.own_plane_offset),
                "the most a plate in a member's own plane may put its axis out beyond a plate's \
                 thickness (L20, carried by the estimated eccentricities); smaller: more hand \
                 details, smaller eccentricities; no change in time",
            ),
            (
                "Bend's inside radius",
                format!("{} × the plate's thickness", self.bend_radius),
                "a bent plate's inside radius; larger: the bend reaches farther (a few ends \
                 less room); the bend checks take it; no change in time",
            ),
            (
                "Plate between angles, least gap",
                format!("{} × the thickest leg", self.between_least),
                "the least gap of a built-up member a plate between its angles is laid out in \
                 (L15); larger: more such ends by hand; no change in time",
            ),
            (
                "Plates may stop short of the work point",
                yes(self.short_plates),
                "no: every plate reaches round the work point, or the ends on it are left for \
                 a hand detail where it would run into something; no change in time",
            ),
            (
                "Picked bolt's pattern",
                format!(
                    "{} in {} line{}",
                    self.bolt_count,
                    self.bolt_lines,
                    if self.bolt_lines == 1 { "" } else { "s" }
                ),
                "the provisional bolts of an end without bolts or a class preset; more bolts: \
                 longer ends and plates, more pulled back; no change in time",
            ),
        ]
    }
}

impl RecordedPractice {
    /// The record's `practice` table: the practice's with its source.
    pub fn to_table(&self) -> Table {
        let mut t = self.practice.to_table();
        t.insert("source".into(), Value::from(self.source.word()));
        t
    }

    /// From the record's `practice` table: its source (none or not read:
    /// the structure's own) and the practice, checked.
    pub fn from_table(t: &Table) -> Result<Self, String> {
        let source = match t.get("source") {
            None => PracticeSource::Project,
            Some(v) => v.as_str().and_then(PracticeSource::parse).ok_or_else(|| {
                format!(
                    "the detailing practice is refused: source {v} is not \"project\", \
                         \"default\" or \"plugin:<plugin>/<method>\""
                )
            })?,
        };
        Ok(Self {
            source,
            practice: DetailingPractice::from_table(t)?,
        })
    }
}

/// The settings `s` with practice `p` applied ([`DetailingPractice::apply`])
/// and kept with them for the record; none: `s` as it is (the default).
pub fn with_practice(mut s: LayoutSettings, p: Option<&RecordedPractice>) -> LayoutSettings {
    if let Some(p) = p {
        p.practice.apply(&mut s);
    }
    s.practice = p.cloned();
    s
}

/// The detailing practice the code plugin of structure file `text` (its
/// member check's method; the bundled default where it names none) gives,
/// read and checked (2026-11-25, O44 L24). `Ok(None)`: the plugin gives
/// none (the default practice applies). Err: the plugin failed or its
/// practice is refused — why, to be said; the default practice applies.
/// `dir`: the structure's folder (its `Plugins/`, the parameter file).
///
/// Expected cost: the structure file read (no model built) and one plugin
/// call per plugin, method and parameter file in a session (kept).
pub fn plugin_practice(
    text: &str,
    dir: Option<&std::path::Path>,
) -> Result<Option<RecordedPractice>, String> {
    use ohl_structures::LibraryItem;
    use ohl_structures_fe::model::{DEFAULT_METHOD, PluginMethod};
    let t = ohl_structures::StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| format!("the structure does not read: {}", e.message))?;
    let capacity = t
        .analysis
        .as_ref()
        .and_then(|a| a.get("capacity"))
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_METHOD)
        .to_owned();
    let mut p = match PluginMethod::parse(&capacity) {
        None => return Ok(None),
        Some(p) => p.map_err(|e| format!("analysis: {e}"))?,
    };
    crate::plugin_check::roots(dir);
    p.parameters = crate::plugin_check::parameters(&p.plugin, dir)?;
    let Some(given) = crate::plugin_check::joints::detailing_practice(&p)? else {
        return Ok(None);
    };
    let practice = DetailingPractice::parse(&given)
        .map_err(|e| format!("{} (from the code plugin {}): {e}", p.method, p.plugin))?;
    Ok(Some(RecordedPractice {
        source: PracticeSource::Plugin {
            plugin: p.plugin,
            method: p.method,
        },
        practice,
    }))
}

/// Keys of `t` not in `known`, refused.
fn unknown(t: &Table, known: &[&str], prefix: &str, errs: &mut Vec<String>) {
    for k in t.keys() {
        if !known.contains(&k.as_str()) {
            errs.push(format!(
                "{prefix}{k}: not a key of a detailing practice (known: {})",
                known.join(", ")
            ));
        }
    }
}

fn flag(t: &Table, k: &str, prefix: &str, out: &mut bool, errs: &mut Vec<String>) {
    if let Some(v) = t.get(k) {
        match v.as_bool() {
            Some(b) => *out = b,
            None => errs.push(format!("{prefix}{k}: true or false expected")),
        }
    }
}

fn number(t: &Table, k: &str, prefix: &str, out: &mut f64, errs: &mut Vec<String>) {
    if let Some(v) = t.get(k) {
        match v.as_float().or_else(|| v.as_integer().map(|i| i as f64)) {
            Some(x) => *out = x,
            None => errs.push(format!("{prefix}{k}: a number expected")),
        }
    }
}

fn count(t: &Table, k: &str, prefix: &str, out: &mut u32, errs: &mut Vec<String>) {
    if let Some(v) = t.get(k) {
        match v.as_integer().and_then(|i| u32::try_from(i).ok()) {
            Some(x) => *out = x,
            None => errs.push(format!("{prefix}{k}: a whole number expected")),
        }
    }
}

fn length(t: &Table, k: &str, prefix: &str, out: &mut f64, errs: &mut Vec<String>) {
    if let Some(v) = t.get(k) {
        let x = match v {
            Value::String(s) => s.parse::<ohl_core::units::Length>().ok().map(|l| l.si()),
            Value::Float(f) => Some(*f),
            Value::Integer(i) => Some(*i as f64),
            _ => None,
        };
        match x {
            Some(x) => *out = x,
            None => errs.push(format!(
                "{prefix}{k}: a length expected (\"30 mm\", or a number in metres)"
            )),
        }
    }
}

fn degrees(t: &Table, k: &str, prefix: &str, out: &mut f64, errs: &mut Vec<String>) {
    if let Some(v) = t.get(k) {
        let x = match v {
            Value::String(s) => s
                .parse::<ohl_core::units::Angle>()
                .ok()
                .map(|a| a.degrees()),
            Value::Float(f) => Some(*f),
            Value::Integer(i) => Some(*i as f64),
            _ => None,
        };
        match x {
            Some(x) => *out = x,
            None => errs.push(format!(
                "{prefix}{k}: an angle expected (a number in degrees, or \"0.03 rad\")"
            )),
        }
    }
}
