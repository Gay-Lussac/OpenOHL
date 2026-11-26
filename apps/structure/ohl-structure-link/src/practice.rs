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
//! **Where it comes from** (the owner's design questions of 2026-11-24,
//! built as recommended): the structure file's own `[layout_practice]`
//! table (structure file version 27) over the code plugin's answer (its
//! optional `layout-practice` export, [`plugin_practice`]), **value by
//! value** ([`in_force`]), over the default ([`DetailingPractice::default`]),
//! which is the layout as it was before L24 exactly — every value the
//! constant the code had, so that an empty practice lays out as today.
//! Per member class first ([`ClassPractice`]): the picked bolt's pattern
//! per class, as the connection presets are; per joint kind later.
//!
//! **Checked here, every value** ([`DetailingPractice::parse`]): a key
//! not known, a value out of its range or a fabrication list that is not
//! the five forms once each is refused with what, why and what to give
//! instead; a refused practice is not used (the default is), said in Lay
//! out joints and in the run's notes. The core's checks are no values of a
//! practice (the clash check, the code's distances, holes overlapping, the
//! bend check, the NOT ON THE SAFE SIDE words). The ranges keep
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
pub const FORMAT: &str = "ohl-layout-practice";
/// The format's version: 2 since O44 L29 (2026-11-26), the plate thickness
/// rules (`plates.least_thickness`, `plates.thicknesses`); a practice
/// without them is written at 1, as before, and both are read.
pub const VERSION: i64 = 2;
/// The format's version of a practice without the thickness rules.
pub const VERSION_1: i64 = 1;

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
/// program's `OWN_OUT_MAX` (what the estimated eccentricities carry).
pub const OWN_OFFSET_RANGE: (f64, f64) = (0.0, OWN_OUT_MAX);
/// A bent plate's inside radius a practice may give, in plate thicknesses:
/// at least one thickness (the code plugins' bend checks check the radius
/// against their code's least; tighter is refused here).
pub const BEND_RADIUS_RANGE: (f64, f64) = (1.0, 10.0);
/// The least gap for a plate between a built-up member's angles a practice
/// may give, in thicknesses of the thickest leg on it: from the program's
/// `BETWEEN_LEAST`.
pub const BETWEEN_RANGE: (f64, f64) = (BETWEEN_LEAST, 4.0);
/// The bolts of the picked bolt's pattern a practice may give per end.
pub const BOLT_COUNT_RANGE: (u32, u32) = (2, 8);
/// The least plate thickness a practice may give, m (O44 L29): 0 (none)
/// to 50 mm. Never a way to a thinner plate: a plate is at least as thick
/// as the thickest leg on it whatever the practice says.
pub const LEAST_THICKNESS_RANGE: (f64, f64) = (0.0, 0.050);
/// Each plate thickness a practice may list, m (O44 L29): 2 to 100 mm.
pub const THICKNESS_RANGE: (f64, f64) = (0.002, 0.100);
/// The most plate thicknesses a practice may list (O44 L29).
pub const THICKNESSES_MAX: usize = 30;

/// A form of fabrication, in the default order (fewer parts, no weld to
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
    /// Every form, in the default order.
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
    /// The fabrication order: the five forms, each once, in the shop's
    /// order, each allowed or not (`direct` and `plate` always allowed).
    /// The layout's weighing keeps its rule (more ends, or as many on fewer
    /// plates); the order decides only ties — today between a plate bent by
    /// a few degrees and one welded in the member's own plane.
    pub fabrication: Vec<(Fabrication, bool)>,
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
    /// The least thickness of a plate the layout makes, m (0: none; O44
    /// L29). A plate is never thinner than the thickest leg on it.
    pub least_thickness: f64,
    /// The plate thicknesses the shop makes, m, ascending (empty: any; O44
    /// L29): a plate takes the thinnest at least as thick as the thickest
    /// leg on it and the least thickness.
    pub thicknesses: Vec<f64>,
    /// The picked bolt's pattern: bolts per end.
    pub bolt_count: u32,
    /// The picked bolt's pattern: gauge lines (1 only, for now).
    pub bolt_lines: u32,
    /// Per member class (`leg`, `bracing`, `redundant`, `crossing`; the
    /// connection presets' classes): what differs for that class.
    pub classes: std::collections::BTreeMap<String, ClassPractice>,
}

/// What a practice gives for one member class (per member class first,
/// the owner's design question 5): the picked bolt's pattern. Other values
/// per class are refused (not built yet).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClassPractice {
    /// Bolts per end of the picked bolt, for ends of this class without
    /// bolts or a connection preset (none: the practice's).
    pub bolt_count: Option<u32>,
}

/// The member classes a practice may name (the connection presets').
pub const CLASSES: [&str; 4] = ["leg", "bracing", "redundant", "crossing"];

impl Default for DetailingPractice {
    /// The layout as it was before L24, exactly.
    fn default() -> Self {
        Self {
            name: String::new(),
            fabrication: Fabrication::ALL.iter().map(|f| (*f, true)).collect(),
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
            least_thickness: 0.0,
            thicknesses: Vec::new(),
            bolt_count: 2,
            bolt_lines: 1,
            classes: Default::default(),
        }
    }
}

/// Where a practice comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PracticeSource {
    /// The program's default.
    Default,
    /// A code plugin's `layout-practice` export.
    Plugin {
        /// The plugin's name.
        plugin: String,
        /// Its method.
        method: String,
    },
    /// The structure file's own `[layout_practice]`.
    Project,
    /// The structure file's own over a code plugin's, value by value.
    ProjectOverPlugin {
        /// The plugin's name.
        plugin: String,
        /// Its method.
        method: String,
    },
}

impl PracticeSource {
    /// As the record writes it.
    pub fn word(&self) -> String {
        match self {
            Self::Default => "default".to_owned(),
            Self::Plugin { plugin, method } => format!("plugin:{plugin}/{method}"),
            Self::Project => "project".to_owned(),
            Self::ProjectOverPlugin { plugin, method } => {
                format!("project over plugin:{plugin}/{method}")
            }
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
            Self::Project => "this structure file's own [layout_practice]".to_owned(),
            Self::ProjectOverPlugin { plugin, method } => format!(
                "this structure file's own [layout_practice] over the code plugin {plugin} \
                 (method {method}), value by value"
            ),
        }
    }
}

/// A code plugin's practice as it answered it (its table, checked alone):
/// what a run records, so that an edit's follow and the estimated
/// eccentricities take the same.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginAnswer {
    /// The plugin's name.
    pub plugin: String,
    /// Its method.
    pub method: String,
    /// Its practice's table.
    pub table: Table,
}

/// The practice in force and where it comes from: what a run uses.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedPractice {
    /// Where it comes from.
    pub source: PracticeSource,
    /// The practice in force (the file's over the plugin's over the
    /// default, value by value), checked.
    pub practice: DetailingPractice,
    /// The structure file's own table, as written (none: none).
    pub own: Option<Table>,
    /// The code plugin's answer (none: none), recorded by a run.
    pub plugin: Option<PluginAnswer>,
}

/// The keys a practice's text may have, by table.
const TOP_KEYS: [&str; 9] = [
    "format",
    "version",
    "name",
    "fabrication",
    "faces",
    "limits",
    "plates",
    "bolts",
    "classes",
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
const PLATE_KEYS: [&str; 5] = [
    "bend_radius",
    "between_least",
    "short",
    "least_thickness",
    "thicknesses",
];
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
            && !matches!(v.as_integer(), Some(VERSION_1 | VERSION))
        {
            errs.push(format!(
                "version: {VERSION_1} or {VERSION} expected (this program reads versions \
                 {VERSION_1} and {VERSION})"
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
                    let mut forms = Vec::new();
                    for v in a {
                        // A form allowed ("bend"), or { form = "bend",
                        // allowed = false }.
                        let (word, allowed) = match v {
                            Value::String(w) => (Some(w.as_str()), Some(true)),
                            Value::Table(x) => (
                                x.get("form").and_then(Value::as_str),
                                x.get("allowed").map_or(Some(true), Value::as_bool),
                            ),
                            _ => (None, None),
                        };
                        if let Value::Table(x) = v
                            && x.keys().any(|k| k != "form" && k != "allowed")
                        {
                            errs.push(format!(
                                "fabrication: {v} — an entry has only `form` and `allowed`"
                            ));
                        }
                        match (word.and_then(Fabrication::parse), allowed) {
                            (Some(k), Some(on)) => forms.push((k, on)),
                            _ => errs.push(format!(
                                "fabrication: {v} is not a form the layout knows (give \
                                 \"direct\", \"plate\", \"bend\", \"bends\", \"welded\", or \
                                 {{ form = \"bend\", allowed = false }})"
                            )),
                        }
                    }
                    p.fabrication = forms;
                }
                None => errs.push("fabrication: a list of the five forms expected".to_owned()),
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
            length(
                &x,
                "least_thickness",
                "plates.",
                &mut p.least_thickness,
                &mut errs,
            );
            if let Some(v) = x.get("thicknesses") {
                match v.as_array() {
                    Some(a) => {
                        let mut list = Vec::new();
                        for (i, v) in a.iter().enumerate() {
                            let mut one = f64::NAN;
                            let mut t1 = Table::new();
                            t1.insert(i.to_string(), v.clone());
                            length(
                                &t1,
                                &i.to_string(),
                                "plates.thicknesses.",
                                &mut one,
                                &mut errs,
                            );
                            list.push(one);
                        }
                        list.sort_by(f64::total_cmp);
                        p.thicknesses = list;
                    }
                    None => errs.push(
                        "plates.thicknesses: a list of lengths expected ([\"6 mm\", \"8 mm\"])"
                            .to_owned(),
                    ),
                }
            }
        }
        if let Some(b) = table("bolts", &mut errs) {
            unknown(&b, &BOLT_KEYS, "bolts.", &mut errs);
            count(&b, "count", "bolts.", &mut p.bolt_count, &mut errs);
            count(&b, "lines", "bolts.", &mut p.bolt_lines, &mut errs);
        }
        if let Some(c) = table("classes", &mut errs) {
            for (name, v) in &c {
                let at = format!("classes.{name}");
                if !CLASSES.contains(&name.as_str()) {
                    errs.push(format!(
                        "{at}: not a member class (give {})",
                        CLASSES.join(", ")
                    ));
                    continue;
                }
                let Some(x) = v.as_table() else {
                    errs.push(format!("{at}: a table expected"));
                    continue;
                };
                for k in x.keys().filter(|k| *k != "bolts") {
                    errs.push(format!(
                        "{at}.{k}: not per member class yet — per member class only the \
                         picked bolt's pattern ([{at}.bolts] count) for now; give it for the \
                         whole structure"
                    ));
                }
                let mut cp = ClassPractice::default();
                if let Some(b) = x.get("bolts") {
                    match b.as_table() {
                        Some(b) => {
                            let at_b = format!("{at}.bolts.");
                            unknown(b, &["count"], &at_b, &mut errs);
                            let mut n = 0;
                            count(b, "count", &at_b, &mut n, &mut errs);
                            if b.contains_key("count") {
                                cp.bolt_count = Some(n);
                            }
                        }
                        None => errs.push(format!("{at}.bolts: a table expected")),
                    }
                }
                p.classes.insert(name.clone(), cp);
            }
        }
        if errs.is_empty() {
            errs = p.refusals();
        }
        if errs.is_empty() {
            Ok(p)
        } else {
            Err(errs.join("; "))
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
            match f.iter().filter(|x| x.0 == k).count() {
                1 => {}
                0 => errs.push(format!(
                    "fabrication: \"{}\" missing — give the five forms, each once, in the \
                     shop's order (a form not made: {{ form = \"{}\", allowed = false }})",
                    k.word(),
                    k.word()
                )),
                _ => errs.push(format!("fabrication: \"{}\" given twice", k.word())),
            }
        }
        for k in [Fabrication::Direct, Fabrication::Plate] {
            if f.iter().any(|x| x.0 == k && !x.1) {
                errs.push(format!(
                    "fabrication: \"{}\" not allowed — the layout's first two steps are \
                     bolting directly and one flat plate, and it has no way without them (a \
                     joint it cannot lay out so is left for a hand detail): allow \"direct\" \
                     and \"plate\"",
                    k.word()
                ));
            }
        }
        for (c, x) in &self.classes {
            if let Some(n) = x.bolt_count
                && !(BOLT_COUNT_RANGE.0..=BOLT_COUNT_RANGE.1).contains(&n)
            {
                errs.push(format!(
                    "classes.{c}.bolts.count {n}: give {} to {} — one bolt per end is the \
                     codes' question (J8: where the code's text allows it), not a practice's",
                    BOLT_COUNT_RANGE.0, BOLT_COUNT_RANGE.1
                ));
            }
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
        // (L29) The thickness rules: only thicker, never a way to a plate
        // thinner than the thickest leg on it (the layout takes the larger).
        range(
            &mut errs,
            self.least_thickness,
            LEAST_THICKNESS_RANGE,
            &format!(
                "plates.least_thickness {}: give 0 (none) to {} — the plates the layout makes are \
                 at least as thick as the thickest leg on them whatever is given; a thicker least \
                 is a hand detail's",
                crate::mm_text(self.least_thickness),
                crate::mm_text(LEAST_THICKNESS_RANGE.1)
            ),
        );
        if self.thicknesses.len() > THICKNESSES_MAX {
            errs.push(format!(
                "plates.thicknesses: {} given — give at most {THICKNESSES_MAX}",
                self.thicknesses.len()
            ));
        }
        for t in &self.thicknesses {
            range(
                &mut errs,
                *t,
                THICKNESS_RANGE,
                &format!(
                    "plates.thicknesses {}: give each {} to {}",
                    crate::mm_text(*t),
                    crate::mm_text(THICKNESS_RANGE.0),
                    crate::mm_text(THICKNESS_RANGE.1)
                ),
            );
        }
        if self
            .thicknesses
            .windows(2)
            .any(|w| (w[1] - w[0]).abs() < 0.5e-4)
        {
            errs.push(
                "plates.thicknesses: a thickness given twice (to a tenth of a millimetre) — give \
                 each once"
                    .to_owned(),
            );
        }
        if let Some(top) = self.thicknesses.last()
            && self.least_thickness > top + 1e-9
        {
            errs.push(format!(
                "plates.least_thickness {}: thicker than the thickest plate listed ({}) — no plate \
                 could be made; give a least within the list, or a thicker plate in it",
                crate::mm_text(self.least_thickness),
                crate::mm_text(*top)
            ));
        }
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

    /// The thickness rules in words, for the window.
    pub fn thickness_text(&self) -> String {
        let least = if self.least_thickness > 0.0 {
            format!("at least {}", crate::mm_text(self.least_thickness))
        } else {
            "the thickest leg on it".to_owned()
        };
        if self.thicknesses.is_empty() {
            least
        } else {
            format!(
                "{least}, in {}",
                self.thicknesses
                    .iter()
                    .map(|t| crate::mm_text(*t))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }

    /// It gives a thickness rule (a least thickness or a list; O44 L29).
    pub fn has_thickness_rules(&self) -> bool {
        self.least_thickness > 0.0 || !self.thicknesses.is_empty()
    }

    /// Makes `form` (it is allowed).
    pub fn makes(&self, form: Fabrication) -> bool {
        self.fabrication.iter().any(|x| x.0 == form && x.1)
    }

    /// The place of `form` in the fabrication order (0 first).
    pub fn place(&self, form: Fabrication) -> usize {
        self.fabrication
            .iter()
            .position(|x| x.0 == form)
            .unwrap_or(usize::MAX)
    }

    /// What is said with the practice (accepted, not acted on).
    pub fn notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.makes(Fabrication::Welded) {
            out.push(
                "no welding: no plate welded in a member's own plane, and the plates of a joint \
                 never welded together; ends that need one are left for a hand detail"
                    .to_owned(),
            );
        }
        if !self.makes(Fabrication::Bend) {
            out.push(
                "no bent plates: an end off its flange only on a welded plate (where allowed), \
                 else left for a hand detail"
                    .to_owned(),
            );
        }
        if self.has_thickness_rules() {
            out.push(THICKER_NOTE.to_owned());
        }
        if self.place(Fabrication::Bends) != 3 || !self.makes(Fabrication::Bends) {
            out.push(
                "\"bends\" (plates with several bends) is not built yet: its place and whether \
                 it is allowed change nothing today"
                    .to_owned(),
            );
        }
        out
    }

    /// The engine's settings `s` with this practice: the forms it does not
    /// allow turned off, its values in place of the program's, the order
    /// where the engine breaks a tie by form. It only restricts what `s`
    /// turns on — a setting off in `s` stays off — so the default practice
    /// leaves `s` exactly as it is. The gap, pull-back and shift are
    /// starting values (the window's own, recorded with a run), not set
    /// here.
    pub fn apply(&self, s: &mut LayoutSettings) {
        let bend = self.makes(Fabrication::Bend);
        s.bent_plates &= bend;
        s.small_bends &= bend;
        s.bend_facing &= bend;
        s.bend_back &= bend;
        let welded = self.makes(Fabrication::Welded);
        s.own_plane_welded &= welded;
        s.welded &= welded;
        s.weld_before_bend = self.place(Fabrication::Welded) < self.place(Fabrication::Bend);
        s.face_pattern &= self.face_pattern == FacePattern::DiagonalInner;
        s.other_face &= self.both_sides_of_flange;
        s.both_faces &= self.both_faces_of_plate;
        s.short_plates &= self.short_plates;
        s.flat_off = self.flatness_deg;
        s.bend_small_max = self.small_bend_deg;
        s.own_out_max = self.own_plane_offset;
        s.bend_radius_ratio = self.bend_radius;
        s.between_least = self.between_least;
        s.plate_least = self.least_thickness;
        s.plate_thicknesses.clone_from(&self.thicknesses);
        if let Some(b) = s.bolt.as_mut()
            && b.preset.is_none()
            && b.lines == 1
            && b.count == 2
            && self.bolt_count != 2
        {
            *b = BoltArea::picked_count(b.bolt.clone(), b.diameter, b.flats, self.bolt_count);
        }
        s.class_bolts = self
            .classes
            .iter()
            .filter_map(|(c, x)| x.bolt_count.map(|n| (c.clone(), n)))
            .collect();
    }

    /// The practice as a practice table (every value written).
    pub fn to_table(&self) -> Table {
        let mut t = Table::new();
        t.insert("format".into(), Value::from(FORMAT));
        t.insert(
            "version".into(),
            Value::from(if self.has_thickness_rules() {
                VERSION
            } else {
                VERSION_1
            }),
        );
        if !self.name.is_empty() {
            t.insert("name".into(), Value::from(self.name.as_str()));
        }
        t.insert(
            "fabrication".into(),
            Value::Array(
                self.fabrication
                    .iter()
                    .map(|(f, on)| {
                        if *on {
                            Value::from(f.word())
                        } else {
                            let mut x = Table::new();
                            x.insert("form".into(), Value::from(f.word()));
                            x.insert("allowed".into(), Value::from(false));
                            Value::Table(x)
                        }
                    })
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
        if self.least_thickness > 0.0 {
            plates.insert(
                "least_thickness".into(),
                Value::from(crate::mm_text(self.least_thickness)),
            );
        }
        if !self.thicknesses.is_empty() {
            plates.insert(
                "thicknesses".into(),
                Value::Array(
                    self.thicknesses
                        .iter()
                        .map(|t| Value::from(crate::mm_text(*t)))
                        .collect(),
                ),
            );
        }
        t.insert("plates".into(), Value::Table(plates));
        let mut bolts = Table::new();
        bolts.insert("count".into(), Value::from(i64::from(self.bolt_count)));
        bolts.insert("lines".into(), Value::from(i64::from(self.bolt_lines)));
        t.insert("bolts".into(), Value::Table(bolts));
        let mut classes = Table::new();
        for (c, x) in &self.classes {
            let mut ct = Table::new();
            if let Some(n) = x.bolt_count {
                let mut b = Table::new();
                b.insert("count".into(), Value::from(i64::from(n)));
                ct.insert("bolts".into(), Value::Table(b));
            }
            classes.insert(c.clone(), Value::Table(ct));
        }
        if !classes.is_empty() {
            t.insert("classes".into(), Value::Table(classes));
        }
        t
    }

    /// The practice as a practice table's text.
    pub fn to_text(&self) -> String {
        toml::to_string(&self.to_table()).unwrap_or_default()
    }

    /// Its values for the window, each with the keys it is read from (to
    /// say where each value comes from), and its effect on the results and
    /// on the time: (keys, what, value, effect).
    pub fn rows(&self) -> Vec<(&'static [&'static str], &'static str, String, &'static str)> {
        let yes = |b: bool| if b { "yes" } else { "no" }.to_owned();
        vec![
            (
                &["fabrication"],
                "Fabrication order",
                self.fabrication
                    .iter()
                    .map(|(f, on)| {
                        if *on {
                            f.word().to_owned()
                        } else {
                            format!("({} not allowed)", f.word())
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" → "),
                "the forms an end may go onto; a form not allowed is never made (an end \
                 needing it is left for a hand detail, a run a little faster); the order \
                 decides only ties — today between a plate bent by a few degrees and one \
                 welded in the member's own plane",
            ),
            (
                &["faces.pattern"],
                "Faces of a leg's flange",
                match self.face_pattern {
                    FacePattern::DiagonalInner => "diagonal onto the inner face first".to_owned(),
                    FacePattern::LaterClassFirst => "the later class first".to_owned(),
                },
                "which of a face's ends is tried on the flange's other face first (L10); no \
                 change in time",
            ),
            (
                &["faces.both_sides_of_flange"],
                "Members on either side of a flange",
                yes(self.both_sides_of_flange),
                "no: no end is turned onto a flange's other face (L10) — fewer ends laid out \
                 where two meet on one face; a run a little faster",
            ),
            (
                &["faces.both_faces_of_plate"],
                "Both faces of one plate",
                yes(self.both_faces_of_plate),
                "no: an end on the inner side of a flange's plane stays on its own welded \
                 plate (L11); no change in time",
            ),
            (
                &["limits.gap", "limits.pull_back", "limits.shift"],
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
                &["limits.flatness_deg"],
                "Flatness",
                format!("{}°", self.flatness_deg),
                "the most an end's leg may lie off its flange and still bolt flat; stricter: \
                 more ends on bent or welded plates (the joint details keep 2°); no change in \
                 time",
            ),
            (
                &["limits.small_bend_deg"],
                "Small bend, at most",
                format!("{}°", self.small_bend_deg),
                "the most a plate is bent by a few degrees (L3) before a welded plate; smaller: \
                 more welded plates; no change in time",
            ),
            (
                &["limits.own_plane_offset"],
                "Welded plate's axis offset, at most",
                crate::mm_text(self.own_plane_offset),
                "the most a plate in a member's own plane may put its axis out beyond a plate's \
                 thickness (L20, carried by the estimated eccentricities); smaller: more hand \
                 details, smaller eccentricities; no change in time",
            ),
            (
                &["plates.bend_radius"],
                "Bend's inside radius",
                format!("{} × the plate's thickness", self.bend_radius),
                "a bent plate's inside radius; larger: the bend reaches farther (a few ends \
                 less room); the bend checks take it; no change in time",
            ),
            (
                &["plates.between_least"],
                "Plate between angles, least gap",
                format!("{} × the thickest leg", self.between_least),
                "the least gap of a built-up member a plate between its angles is laid out in \
                 (L15); larger: more such ends by hand; no change in time",
            ),
            (
                &["plates.short"],
                "Plates may stop short of the work point",
                yes(self.short_plates),
                "no: every plate reaches round the work point, or the ends on it are left for \
                 a hand detail where it would run into something; no change in time",
            ),
            (
                &["plates.least_thickness", "plates.thicknesses"],
                "Plate thickness",
                self.thickness_text(),
                "the plates the layout makes: as thick as the thickest leg on them, raised to \
                 the least and to the next thickness listed (never thinner); thicker plates put \
                 their members' axes farther out (NOT ON THE SAFE SIDE, said) and may leave more \
                 ends where they run into something; a leg thicker than every listed plate: \
                 its ends by hand; no change in time",
            ),
            (
                &["bolts.count", "bolts.lines"],
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
            (
                &["classes"],
                "Per member class",
                if self.classes.is_empty() {
                    "none".to_owned()
                } else {
                    self.classes
                        .iter()
                        .map(|(c, x)| {
                            x.bolt_count
                                .map_or(format!("{c}: —"), |n| format!("{c}: {n} bolts"))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                },
                "the picked bolt's pattern for ends of that class (as the connection presets \
                 are given per class); no change in time",
            ),
        ]
    }
}

/// The words of a joint's note for a plate the thickness rules made
/// thicker than the thickest leg on it (O44 L29), counted for the run's
/// note.
pub const THICKER_MARK: &str = "thick by the detailing practice's plate thicknesses";

/// The run's note for `n` plates the thickness rules made thicker than the
/// thickest leg on them (O44 L29); none for none.
pub fn thicker_run_note(n: usize) -> Option<String> {
    (n > 0).then(|| {
        format!(
            "{n} plate(s) made thicker than the thickest leg on them by the detailing practice's \
             plate thicknesses (each said in its joint's notes). {THICKER_NOTE}."
        )
    })
}

/// What is said with a practice's thickness rules (O44 L29), in Lay out
/// joints and in the run's notes where a plate is made thicker than the
/// thickest leg on it: the three parts the owner's rule asks of every such
/// mark (what is left out, why it can be unsafe, what makes it safe).
pub const THICKER_NOTE: &str = "NOT ON THE SAFE SIDE where a plate is thicker than the thickest \
     leg on it (the practice's plate thicknesses): left out — the analysis does not take an end's \
     seat out of the flange on a gusset plate (one plate's thickness, as for every plate; O45 D), \
     and a thicker plate puts its members' axes farther out by the extra thickness; why it can be \
     unsafe — that offset is a moment at the member's end the member check does not see, so its \
     usage can be too low; what makes it safe — the member checks' rules for an angle bolted by \
     one leg take its end's eccentricity as far as the code says, a thickness list close to the \
     legs' thicknesses keeps the extra small (said per plate in the joint's notes), and an \
     eccentricity written by hand at such an end is taken by the analysis";

/// The thickness of a plate on legs at most `legs` thick (m) under the
/// thickness rules `least` and `list` (m, ascending; O44 L29): the thinnest
/// listed at least as thick as `legs` and `least` (no list: the larger of
/// the two). Never thinner than `legs`. Err: no listed thickness is that
/// thick — why, the plate's ends left for a hand detail. With no rule
/// (`least` 0, `list` empty) exactly `legs`, as before L29.
pub fn plate_thickness(legs: f64, least: f64, list: &[f64]) -> Result<f64, String> {
    let need = if least > legs { least } else { legs };
    if list.is_empty() {
        return Ok(need);
    }
    list.iter()
        .copied()
        .find(|t| *t + 1e-9 >= need)
        .ok_or_else(|| {
            format!(
                "a plate on it would be {} thick (the thickest leg on it{}), thicker than every \
                 plate thickness the detailing practice lists (the thickest {}): lay it out by \
                 hand, or list a thicker plate",
                crate::mm_text(need),
                if least > legs {
                    ", raised to the practice's least"
                } else {
                    ""
                },
                crate::mm_text(list.last().copied().unwrap_or(0.0))
            )
        })
}

impl LayoutSettings {
    /// The thickness of a plate the layout makes on legs at most `legs`
    /// thick (m), under the practice's thickness rules ([`plate_thickness`]
    /// of [`LayoutSettings::plate_least`] and
    /// [`LayoutSettings::plate_thicknesses`]); Err: none listed that thick.
    pub fn plate_thickness(&self, legs: f64) -> Result<f64, String> {
        plate_thickness(legs, self.plate_least, &self.plate_thicknesses)
    }

    /// [`Self::plate_thickness`] where the planner only estimates the plate
    /// (its clearances before the plate is chosen): where none is listed
    /// that thick, `legs` (the plate is then refused where it is made).
    pub fn plate_planned(&self, legs: f64) -> f64 {
        self.plate_thickness(legs).unwrap_or(legs)
    }
}

impl DetailingPractice {
    /// What of `self` differs from `under` (the code plugin's practice, or
    /// the default), as the structure file's own `[layout_practice]` table
    /// (O44 L29's editor): a sub-table key by key, a list whole, the name;
    /// with `format` and `version`. None: nothing differs (no table — the
    /// plugin's or the default practice in force as it is).
    pub fn own_over(&self, under: &Self) -> Option<Table> {
        fn diff(a: &Table, b: &Table, out: &mut Table) {
            for (k, v) in a {
                if k == "format" || k == "version" {
                    continue;
                }
                match (v, b.get(k)) {
                    (Value::Table(x), Some(Value::Table(y))) if k != "classes" => {
                        let mut sub = Table::new();
                        diff(x, y, &mut sub);
                        if !sub.is_empty() {
                            out.insert(k.clone(), Value::Table(sub));
                        }
                    }
                    (v, Some(w)) if v == w => {}
                    (v, _) => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        let mut out = Table::new();
        let (a, b) = (self.to_table(), under.to_table());
        diff(&a, &b, &mut out);
        // Removed from `self` but given in `under` (a name, a class, a
        // thickness rule): said by its value in `self` — the classes whole,
        // the plates' rules by their empty values.
        if b.get("name").is_some() && a.get("name") != b.get("name") {
            out.insert("name".into(), Value::from(self.name.as_str()));
        }
        if b.get("classes").is_some() && a.get("classes") != b.get("classes") {
            out.insert(
                "classes".into(),
                a.get("classes")
                    .cloned()
                    .unwrap_or_else(|| Value::Table(Table::new())),
            );
        }
        let plates = |t: &Table, k: &str| {
            t.get("plates")
                .and_then(Value::as_table)
                .and_then(|p| p.get(k))
                .cloned()
        };
        for (k, empty) in [
            ("least_thickness", Value::from("0 mm")),
            ("thicknesses", Value::Array(Vec::new())),
        ] {
            if plates(&b, k).is_some() && plates(&a, k).is_none() {
                if let Some(Value::Table(p)) = out.get_mut("plates") {
                    p.insert(k.into(), empty);
                } else {
                    let mut p = Table::new();
                    p.insert(k.into(), empty);
                    out.insert("plates".into(), Value::Table(p));
                }
            }
        }
        if out.is_empty() {
            return None;
        }
        let mut t = Table::new();
        t.insert("format".into(), Value::from(FORMAT));
        t.insert(
            "version".into(),
            Value::from(
                if gives(&out, "plates.least_thickness") || gives(&out, "plates.thicknesses") {
                    VERSION
                } else {
                    VERSION_1
                },
            ),
        );
        t.extend(out);
        Some(t)
    }

    /// Where the Lay out joints window's own values for this run (`gap`,
    /// `pull_back`, `shift`, m) go beyond this practice's (O44 L29, the
    /// owner's L24 question 4: they stay the user's choice per run, said
    /// where they exceed the practice's): a smaller gap, a longer
    /// pull-back, a larger shift, each in words; also a gap the clash check
    /// would not see. Empty: none.
    pub fn beyond(&self, gap: f64, pull_back: f64, shift: f64) -> Vec<String> {
        let mm = crate::mm_text;
        let mut out = Vec::new();
        if gap + 1e-9 < self.gap {
            out.push(format!(
                "gap {} — less than the practice's {} (less room between steel than the shop asks)",
                mm(gap),
                mm(self.gap)
            ));
        }
        if gap + 1e-9 < GAP_RANGE.0 {
            out.push(format!(
                "gap {} — below {}, the least the clash check sees (steel this close is not found \
                 clashing)",
                mm(gap),
                mm(GAP_RANGE.0)
            ));
        }
        if pull_back > self.pull_back + 1e-9 {
            out.push(format!(
                "pull-back {} — longer than the practice's {} (longer plates)",
                mm(pull_back),
                mm(self.pull_back)
            ));
        }
        if shift > self.shift + 1e-9 {
            out.push(format!(
                "shift {} — larger than the practice's {} (larger eccentricities; the analysis \
                 takes the moves as written, and the estimated eccentricities this run's shift as \
                 their upper bound)",
                mm(shift),
                mm(self.shift)
            ));
        }
        out
    }
}

/// The run's note where settings `s`'s own gap, pull-back and shift go
/// beyond the detailing practice given with them (O44 L29; none: none, or
/// no practice given — the program's default is no shop's practice).
pub fn beyond_note(s: &LayoutSettings) -> Option<String> {
    let p = s.practice.as_ref()?;
    let b = p.practice.beyond(s.gap, s.pull_back, s.shift_limit);
    (!b.is_empty()).then(|| {
        format!(
            "This run's own settings go beyond the detailing practice in force ({}): {} — the \
             user's choice for this run (kept in the layout record); the practice is unchanged.",
            p.source.label(),
            b.join("; ")
        )
    })
}

/// The structure file `doc` with its own detailing practice `own` (none:
/// its `[layout_practice]` taken out): the editor's change (O44 L29), the
/// file raised to version 27 (`ohl_structures::structures::LAYOUT_PRACTICE_VERSION`),
/// or 30 where the table gives a plate thickness rule
/// (`LAYOUT_PRACTICE_THICKNESS_VERSION`) — never lowered.
pub fn write_own(doc: &mut toml_edit::DocumentMut, own: Option<&Table>) -> Result<(), String> {
    use ohl_structures::structures::{LAYOUT_PRACTICE_THICKNESS_VERSION, LAYOUT_PRACTICE_VERSION};
    match own {
        None => {
            doc.remove("layout_practice");
        }
        Some(t) => {
            DetailingPractice::from_table(t)
                .map_err(|e| format!("the detailing practice is refused: {e}"))?;
            // Written as its own sections at the file's end (a table taken
            // from another document keeps that document's positions and
            // could land between an array's entry and its sub-tables).
            doc.remove("layout_practice");
            let mut wrap = Table::new();
            wrap.insert("layout_practice".into(), Value::Table(t.clone()));
            let text = toml::to_string(&wrap).map_err(|e| e.to_string())?;
            let mut all = doc.to_string();
            if !all.ends_with('\n') {
                all.push('\n');
            }
            all.push('\n');
            all.push_str(&text);
            *doc = all
                .parse()
                .map_err(|e| format!("the detailing practice does not write: {e}"))?;
            let thick = gives(t, "plates.least_thickness") || gives(t, "plates.thicknesses");
            ohl_structure_edit::edits::raise_version(
                doc,
                i64::from(if thick {
                    LAYOUT_PRACTICE_THICKNESS_VERSION
                } else {
                    LAYOUT_PRACTICE_VERSION
                }),
            );
        }
    }
    Ok(())
}

/// Table `over` laid over table `under`, value by value: a sub-table key by
/// key (so that the structure's `[layout_practice.limits] shift` replaces
/// only the plugin's shift), every other value whole (a list as one value).
pub fn merged(under: &Table, over: &Table) -> Table {
    let mut out = under.clone();
    for (k, v) in over {
        match (out.get_mut(k), v) {
            (Some(Value::Table(a)), Value::Table(b)) => {
                *a = merged(a, b);
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// Whether table `t` gives the value at `path` ("limits.shift").
pub fn gives(t: &Table, path: &str) -> bool {
    let mut at = t;
    let mut keys = path.split('.').peekable();
    while let Some(k) = keys.next() {
        match at.get(k) {
            None => return false,
            Some(Value::Table(x)) if keys.peek().is_some() => at = x,
            Some(_) => return keys.peek().is_none(),
        }
    }
    true
}

impl RecordedPractice {
    /// Where the value read from `keys` comes from, in words: "this file",
    /// "the code plugin", "the default".
    pub fn source_of(&self, keys: &[&str]) -> &'static str {
        if self
            .own
            .as_ref()
            .is_some_and(|t| keys.iter().any(|k| gives(t, k)))
        {
            "this file"
        } else if self
            .plugin
            .as_ref()
            .is_some_and(|p| keys.iter().any(|k| gives(&p.table, k)))
        {
            "the code plugin"
        } else {
            "the default"
        }
    }
}

/// The practice in force (the owner's design question 1, built as
/// recommended): the structure file's own table `own` over the code
/// plugin's answer `plugin`, value by value, over the default; checked as
/// a whole. `Ok(None)`: neither (the default). Err: refused — whose table,
/// what, why and what to give instead; the default practice then applies,
/// said in Lay out joints and in the run's notes.
pub fn in_force(
    own: Option<&Table>,
    plugin: Option<&PluginAnswer>,
) -> Result<Option<RecordedPractice>, String> {
    if own.is_none() && plugin.is_none() {
        return Ok(None);
    }
    if let Some(p) = plugin {
        DetailingPractice::from_table(&p.table).map_err(|e| {
            format!(
                "the detailing practice of the code plugin {} (method {}) is refused: {e}",
                p.plugin, p.method
            )
        })?;
    }
    let empty = Table::new();
    let under = plugin.map_or(&empty, |p| &p.table);
    let all = merged(under, own.unwrap_or(&empty));
    let practice = DetailingPractice::from_table(&all)
        .map_err(|e| format!("this structure file's [layout_practice] is refused: {e}"))?;
    let source = match (own, plugin) {
        (Some(_), Some(p)) => PracticeSource::ProjectOverPlugin {
            plugin: p.plugin.clone(),
            method: p.method.clone(),
        },
        (None, Some(p)) => PracticeSource::Plugin {
            plugin: p.plugin.clone(),
            method: p.method.clone(),
        },
        _ => PracticeSource::Project,
    };
    Ok(Some(RecordedPractice {
        source,
        practice,
        own: own.cloned(),
        plugin: plugin.cloned(),
    }))
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

/// [`in_force`] of what was given (its result or its refusal): the settings
/// `s` with it applied, a refusal kept for the run's notes
/// ([`LayoutSettings::practice_refused`]).
pub fn with_given(
    s: LayoutSettings,
    given: &Result<Option<RecordedPractice>, String>,
) -> LayoutSettings {
    match given {
        Ok(p) => with_practice(s, p.as_ref()),
        Err(e) => LayoutSettings {
            practice_refused: Some(refused_note(e)),
            ..with_practice(s, None)
        },
    }
}

/// The run's note for a refused practice.
pub fn refused_note(why: &str) -> String {
    format!("{why}. The default detailing practice was used (the layout as before L24).")
}

/// The detailing practice the code plugin of structure file `text` (its
/// member check's method; the bundled default where it names none) gives
/// (2026-11-25, O44 L24), as it answered it. `Ok(None)`: the plugin gives
/// none. Err: the plugin failed or its answer does not read as TOML — why,
/// to be said; the default practice applies. `dir`: the structure's folder
/// (its `Plugins/`, the parameter file).
///
/// Expected cost: the structure file read (no model built) and one plugin
/// call per plugin, method and parameter file in a session (kept).
pub fn plugin_practice(
    text: &str,
    dir: Option<&std::path::Path>,
) -> Result<Option<PluginAnswer>, String> {
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
    let Some(given) = crate::plugin_check::joints::layout_practice(&p)? else {
        return Ok(None);
    };
    let table: Table = toml::from_str(&given).map_err(|e| {
        format!(
            "the detailing practice of the code plugin {} (method {}) does not read as TOML: {e}",
            p.plugin, p.method
        )
    })?;
    Ok(Some(PluginAnswer {
        plugin: p.plugin,
        method: p.method,
        table,
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
