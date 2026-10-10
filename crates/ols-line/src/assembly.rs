// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What hangs on a structure, defined explicitly: **structure points**
//! (labelled points on the structure), **strings** (straight members from a
//! top to a wire point) and **wire points** (where strings meet and where
//! wires are clamped).
//!
//! - A string's top is a structure point or another string's wire point,
//!   so strings hang from each other: a V-string is two strings sharing a
//!   wire point, a V with a string below adds a third string hanging from
//!   that wire point, a TAK is a suspension string to a wire point and two
//!   tension strings from it.
//! - A wire point with a **set** number is where a section or a jumper
//!   ends ([`crate::SectionPoint`]: structure + set). A wire point no string
//!   reaches is fixed to the structure.
//! - A string carries no compression unless it allows some
//!   ([`InsulatorString::compression`]): it then acts both ways, and the
//!   solve flags it when the compression exceeds that limit.
//!
//! Files written before (line file version 1) gave each string as one
//! attachment (set and phase, insulator point, wire point, V legs); they
//! are read through [`AttachmentV1`] ([`from_v1`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Point, Structure};

/// A labelled point on a structure where strings hang.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructurePoint {
    /// Label, unique among the structure's points and wire points (e.g.
    /// `"A1-L"`).
    pub label: String,
    /// Position, m.
    pub position: Point,
}

/// A wire point: the bottom of one or more strings (or a point fixed to the
/// structure, when no string reaches it), where a wire may be clamped.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WirePoint {
    /// Label, unique among the structure's points and wire points (e.g.
    /// `"W1"`).
    pub label: String,
    /// Position in still air, m (where the strings hang as given; their
    /// lengths are the distances).
    pub position: Point,
    /// Set number: sections and jumpers end here (unique on the
    /// structure). None: strings only meet here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<u32>,
    /// A weight hanging at it, N (a counterweight), on the whole bundle.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub weight: f64,
}

/// One string: a straight member from its top to a wire point. Its length
/// is the distance between them as given.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsulatorString {
    /// Label, unique among the structure's strings.
    pub label: String,
    /// Top: a structure point, or the wire point of another string.
    pub top: String,
    /// Bottom: a wire point.
    pub wire: String,
    /// Insulator assembly id from the insulator library (`.olsi`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insulator: Option<String>,
    /// Weight, N, half at each end.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub weight: f64,
    /// Area the wind blows on, m², half at each end.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub wind_area: f64,
    /// Axial stiffness EA, N (none: [`crate::solve::DEFAULT_INSULATOR_STIFFNESS`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stiffness: Option<f64>,
    /// Compression it may take, N: it then acts both ways and is flagged
    /// beyond this. None: tension only (slack when pushed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<f64>,
    /// A strain (dead-end) string: placed along its conductor and lined up
    /// with it ([`crate::Line::resolve`]); without a weight, not given one
    /// from its position.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strain: bool,
}

fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

/// A node of a structure's string graph: a structure point or a wire
/// point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Node<'a> {
    /// On the structure (fixed).
    Structure(&'a StructurePoint),
    /// A wire point.
    Wire(&'a WirePoint),
}

impl Node<'_> {
    /// Its position.
    pub fn position(&self) -> Point {
        match self {
            Node::Structure(p) => p.position,
            Node::Wire(w) => w.position,
        }
    }
}

/// The strings and wire points joined to a wire point through strings and
/// other wire points (not through the structure): what moves with it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Component {
    /// Indices into the structure's `wires`, the starting one first.
    pub wires: Vec<usize>,
    /// Indices into the structure's `strings`.
    pub strings: Vec<usize>,
}

impl Structure {
    /// A structure at `position` with nothing hanging on it, local x
    /// towards east.
    pub fn new(id: impl Into<String>, position: Point) -> Self {
        Structure {
            id: id.into(),
            position,
            orientation_deg: 0.0,
            structure_type: None,
            extension: None,
            description: String::new(),
            height_adjust: 0.0,
            points: Vec::new(),
            strings: Vec::new(),
            wires: Vec::new(),
            omit: Vec::new(),
            omit_sets: Vec::new(),
            members: Vec::new(),
        }
    }

    /// Structure point by label.
    pub fn point(&self, label: &str) -> Option<&StructurePoint> {
        self.points.iter().find(|p| p.label == label)
    }

    /// Wire point by label.
    pub fn wire(&self, label: &str) -> Option<&WirePoint> {
        self.wires.iter().find(|w| w.label == label)
    }

    /// The wire point with set `set`.
    pub fn wire_at_set(&self, set: u32) -> Option<&WirePoint> {
        self.wires.iter().find(|w| w.set == Some(set))
    }

    /// String by label.
    pub fn string(&self, label: &str) -> Option<&InsulatorString> {
        self.strings.iter().find(|s| s.label == label)
    }

    /// A structure point or wire point by label.
    pub fn node(&self, label: &str) -> Option<Node<'_>> {
        self.point(label)
            .map(Node::Structure)
            .or_else(|| self.wire(label).map(Node::Wire))
    }

    /// The strings ending at wire point `label`.
    pub fn strings_to<'a>(&'a self, label: &'a str) -> impl Iterator<Item = &'a InsulatorString> {
        self.strings.iter().filter(move |s| s.wire == label)
    }

    /// Whether wire point `label` hangs on strings (else it is fixed to the
    /// structure).
    pub fn is_held(&self, label: &str) -> bool {
        self.strings.iter().any(|s| s.wire == label)
    }

    /// A string's length as given: from its top to its wire point.
    pub fn string_length(&self, string: &InsulatorString) -> Option<f64> {
        let a = self.node(&string.top)?.position();
        let b = self.wire(&string.wire)?.position;
        Some(distance(a, b))
    }

    /// The strings and wire points moving with wire point `label` (see
    /// [`Component`]); empty when there is none.
    pub fn component(&self, label: &str) -> Component {
        let Some(start) = self.wires.iter().position(|w| w.label == label) else {
            return Component::default();
        };
        let mut wires = vec![start];
        let mut strings = Vec::new();
        let mut k = 0;
        while k < wires.len() {
            let here = self.wires[wires[k]].label.as_str();
            for (i, s) in self.strings.iter().enumerate() {
                let other = if s.wire == here {
                    s.top.as_str()
                } else if s.top == here {
                    s.wire.as_str()
                } else {
                    continue;
                };
                if !strings.contains(&i) {
                    strings.push(i);
                }
                if let Some(j) = self.wires.iter().position(|w| w.label == other)
                    && !wires.contains(&j)
                {
                    wires.push(j);
                }
            }
            k += 1;
        }
        strings.sort_unstable();
        Component { wires, strings }
    }

    /// The sets of the structure's wire points.
    pub fn sets(&self) -> impl Iterator<Item = u32> + '_ {
        self.wires.iter().filter_map(|w| w.set)
    }

    /// Every point of the structure that moves with it (structure points,
    /// wire points, stick drawing), for moving or transforming it.
    pub fn points_mut(&mut self) -> impl Iterator<Item = &mut Point> {
        self.points
            .iter_mut()
            .map(|p| &mut p.position)
            .chain(self.wires.iter_mut().map(|w| &mut w.position))
            .chain(self.members.iter_mut().flatten())
    }

    /// Checks the strings: labels unique, every top and wire point there,
    /// sets unique, numbers finite, every wire point hanging on strings
    /// reaching the structure.
    pub fn check_strings(&self) -> Result<(), String> {
        let id = &self.id;
        let finite = |p: &Point| p.iter().all(|x| x.is_finite());
        let mut labels = BTreeSet::new();
        for (label, position) in self
            .points
            .iter()
            .map(|p| (&p.label, &p.position))
            .chain(self.wires.iter().map(|w| (&w.label, &w.position)))
        {
            if label.is_empty() {
                return Err(format!("structure {id}: a point without a label"));
            }
            if !labels.insert(label.as_str()) {
                return Err(format!("structure {id}: two points labelled \"{label}\""));
            }
            if !finite(position) {
                return Err(format!("structure {id}: point {label} is not a number"));
            }
        }
        let mut sets = BTreeSet::new();
        if let Some(w) = self
            .wires
            .iter()
            .find(|w| !(w.weight.is_finite() && w.weight >= 0.0))
        {
            return Err(format!(
                "structure {id}: wire point {}: weight must be ≥ 0",
                w.label
            ));
        }
        for w in &self.wires {
            if let Some(set) = w.set
                && !sets.insert(set)
            {
                return Err(format!("structure {id}: two wire points with set {set}"));
            }
        }
        let mut strings = BTreeSet::new();
        for s in &self.strings {
            let bad = |m: &str| Err(format!("structure {id}: string {}: {m}", s.label));
            if s.label.is_empty() {
                return Err(format!("structure {id}: a string without a label"));
            }
            if !strings.insert(s.label.as_str()) {
                return Err(format!(
                    "structure {id}: two strings labelled \"{}\"",
                    s.label
                ));
            }
            if self.node(&s.top).is_none() {
                return bad(&format!("no point \"{}\" for its top", s.top));
            }
            if self.wire(&s.wire).is_none() {
                return bad(&format!("no wire point \"{}\" for its bottom", s.wire));
            }
            if s.top == s.wire {
                return bad("its top is its wire point");
            }
            if self.string_length(s).is_none_or(|l| l < 1e-3) {
                return bad("its top and wire point are less than 1 mm apart");
            }
            if !(s.weight.is_finite()
                && s.weight >= 0.0
                && s.wind_area.is_finite()
                && s.wind_area >= 0.0)
            {
                return bad("weight and wind area must be ≥ 0");
            }
            if s.stiffness.is_some_and(|k| !(k.is_finite() && k > 0.0)) {
                return bad("stiffness must be > 0");
            }
            if s.compression.is_some_and(|c| !(c.is_finite() && c >= 0.0)) {
                return bad("the compression it may take must be ≥ 0");
            }
        }
        // Every wire point on strings must reach the structure through them.
        for w in self.wires.iter().filter(|w| self.is_held(&w.label)) {
            let c = self.component(&w.label);
            let anchored = c
                .strings
                .iter()
                .any(|&i| self.point(&self.strings[i].top).is_some());
            if !anchored {
                return Err(format!(
                    "structure {id}: wire point {} hangs on strings that reach no structure point",
                    w.label
                ));
            }
        }
        Ok(())
    }
}

fn distance(a: Point, b: Point) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The set number of a version 1 (set, phase): the set itself for phase 1
/// (the usual case: one wire per set), else `set + 1000 · (phase + 1)` —
/// unique as long as sets stay below 1000, and the same everywhere it is
/// met (structures, sections, jumpers, project history).
pub fn set_from_v1(set: u32, phase: u32) -> u32 {
    ols_structures::structures::set_from_v1(set, phase)
}

/// An attachment as line files of version 1 gave it: one string (or a
/// V-string, or a wire fixed to the structure) per set and phase.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentV1 {
    /// Set number.
    pub set: u32,
    /// Phase number within the set.
    pub phase: u32,
    /// Label.
    #[serde(default)]
    pub label: String,
    /// Top of the string, m.
    pub insulator_point: Point,
    /// Clamp, m.
    pub wire_point: Point,
    /// Insulator weight, N.
    #[serde(default)]
    pub insulator_weight: f64,
    /// Insulator assembly id.
    #[serde(default)]
    pub insulator: Option<String>,
    /// Wind area, m².
    #[serde(default)]
    pub insulator_wind_area: f64,
    /// V-string leg tops.
    #[serde(default)]
    pub legs: Option<[Point; 2]>,
    /// Label of the top.
    #[serde(default)]
    pub top_label: String,
    /// Label of the bottom.
    #[serde(default)]
    pub bottom_label: String,
    /// Strain string.
    #[serde(default)]
    pub strain_member: bool,
}

/// The default labels of a version 1 attachment's top (`A<set>-<phase>`),
/// wire point (`W<set>-<phase>`) and string (`S<set>-<phase>`).
pub fn labels_v1(set: u32, phase: u32) -> [String; 3] {
    ["A", "W", "S"].map(|p| format!("{p}{set}-{phase}"))
}

/// Version 1 attachments as structure points, strings and wire points:
/// - a wire point within 1 cm of its insulator point: a wire point fixed
///   to the structure, no string;
/// - two legs: two structure points `<top>a`, `<top>b` and two strings
///   `<string>a`, `<string>b` sharing the wire point, half the weight and
///   wind area each;
/// - else one structure point and one string.
///
/// Labels: the attachment's top label, else [`labels_v1`]; its bottom
/// label, else its label, else [`labels_v1`] for the wire point; a top
/// label met again at the same place is the same point (strings sharing
/// it), any other label met again gets a suffix (`.2`, …); the set is
/// [`set_from_v1`].
pub fn from_v1(
    attachments: &[AttachmentV1],
) -> (Vec<StructurePoint>, Vec<InsulatorString>, Vec<WirePoint>) {
    let mut points: Vec<StructurePoint> = Vec::new();
    let mut strings: Vec<InsulatorString> = Vec::new();
    let mut wires: Vec<WirePoint> = Vec::new();
    let mut used: BTreeMap<String, Point> = BTreeMap::new();
    // A free label for a point at `at`: the same label for the same place
    // (structure points only), else with a suffix.
    let mut take = |label: String, at: Point, share: bool| -> (String, bool) {
        let mut candidate = label.clone();
        let mut n = 1;
        loop {
            match used.get(&candidate) {
                None => {
                    used.insert(candidate.clone(), at);
                    return (candidate, true);
                }
                Some(p) if share && distance(*p, at) < 1e-6 => return (candidate, false),
                Some(_) => {
                    n += 1;
                    candidate = format!("{label}.{n}");
                }
            }
        }
    };
    for a in attachments {
        let [top_default, wire_default, string_label] = labels_v1(a.set, a.phase);
        let pick = |given: &str, default: String| {
            if given.is_empty() {
                default
            } else {
                given.to_owned()
            }
        };
        let wire_label = if a.bottom_label.is_empty() {
            pick(&a.label, wire_default)
        } else {
            a.bottom_label.clone()
        };
        let (wire, _) = take(wire_label, a.wire_point, false);
        wires.push(WirePoint {
            label: wire.clone(),
            position: a.wire_point,
            set: Some(set_from_v1(a.set, a.phase)),
            weight: 0.0,
        });
        let top = pick(&a.top_label, top_default);
        let string = |label: String, top: String, share: f64| InsulatorString {
            label,
            top,
            wire: wire.clone(),
            insulator: a.insulator.clone(),
            weight: a.insulator_weight * share,
            wind_area: a.insulator_wind_area * share,
            stiffness: None,
            compression: None,
            strain: a.strain_member,
        };
        match &a.legs {
            Some(legs) => {
                for (leg, suffix) in legs.iter().zip(["a", "b"]) {
                    let (label, new) = take(format!("{top}{suffix}"), *leg, true);
                    if new {
                        points.push(StructurePoint {
                            label: label.clone(),
                            position: *leg,
                        });
                    }
                    strings.push(string(format!("{string_label}{suffix}"), label, 0.5));
                }
            }
            None if distance(a.insulator_point, a.wire_point) <= 0.01 => {}
            None => {
                let (label, new) = take(top, a.insulator_point, true);
                if new {
                    points.push(StructurePoint {
                        label: label.clone(),
                        position: a.insulator_point,
                    });
                }
                strings.push(string(string_label, label, 1.0));
            }
        }
    }
    // String labels unique too.
    let mut seen = BTreeSet::new();
    for s in &mut strings {
        let base = s.label.clone();
        let mut n = 1;
        while !seen.insert(s.label.clone()) {
            n += 1;
            s.label = format!("{base}.{n}");
        }
    }
    (points, strings, wires)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(set: u32, phase: u32, ins: Point, wire: Point) -> AttachmentV1 {
        AttachmentV1 {
            set,
            phase,
            label: String::new(),
            insulator_point: ins,
            wire_point: wire,
            insulator_weight: 400.0,
            insulator: None,
            insulator_wind_area: 0.2,
            legs: None,
            top_label: String::new(),
            bottom_label: String::new(),
            strain_member: false,
        }
    }

    fn structure(attachments: &[AttachmentV1]) -> Structure {
        let (points, strings, wires) = from_v1(attachments);
        Structure {
            id: "T".into(),
            position: [0.0; 3],
            orientation_deg: 0.0,
            structure_type: None,
            extension: None,
            description: String::new(),
            height_adjust: 0.0,
            points,
            strings,
            wires,
            omit: Vec::new(),
            omit_sets: Vec::new(),
            members: Vec::new(),
        }
    }

    /// Every kind of version 1 attachment becomes points, strings and wire
    /// points; phases other than 1 get their own sets.
    #[test]
    fn version_1_attachments_become_strings() {
        let mut v = attachment(2, 1, [0.0, 0.0, 30.0], [0.0, 0.0, 27.0]);
        v.legs = Some([[-2.0, 0.0, 30.0], [2.0, 0.0, 30.0]]);
        let s = structure(&[
            attachment(1, 1, [0.0, 0.0, 20.0], [0.0, 0.0, 17.0]),
            attachment(1, 2, [5.0, 0.0, 20.0], [5.0, 0.0, 20.005]),
            v,
        ]);
        s.check_strings().unwrap();
        // The I-string.
        let w = s.wire_at_set(1).unwrap();
        assert_eq!(w.label, "W1-1");
        let i: Vec<_> = s.strings_to("W1-1").collect();
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].top, "A1-1");
        assert!((s.string_length(i[0]).unwrap() - 3.0).abs() < 1e-12);
        assert_eq!(i[0].weight, 400.0);
        // Fixed: a wire point no string reaches, set 1 + 1000·3.
        let fixed = s.wire_at_set(3001).unwrap();
        assert!(!s.is_held(&fixed.label));
        // The V: two strings, half each, sharing the wire point.
        let legs: Vec<_> = s.strings_to("W2-1").collect();
        assert_eq!(legs.len(), 2);
        assert!(legs.iter().all(|l| l.weight == 200.0 && l.wind_area == 0.1));
        assert_eq!(s.component("W2-1").strings.len(), 2);
        assert_eq!(set_from_v1(7, 1), 7);
        assert_eq!(set_from_v1(7, 2), 3007);
        assert_eq!(set_from_v1(7, 0), 1007);
    }

    /// A V with a string below and a TAK: components reach through wire
    /// points, not through the structure; a string floating off the
    /// structure is refused.
    #[test]
    fn strings_hang_from_strings() {
        let p = |label: &str, position: Point| StructurePoint {
            label: label.into(),
            position,
        };
        let w = |label: &str, position: Point, set: Option<u32>| WirePoint {
            label: label.into(),
            position,
            set,
            weight: 0.0,
        };
        let s = |label: &str, top: &str, wire: &str| InsulatorString {
            label: label.into(),
            top: top.into(),
            wire: wire.into(),
            ..Default::default()
        };
        let mut t = structure(&[]);
        t.points = vec![
            p("L", [-2.0, 0.0, 30.0]),
            p("R", [2.0, 0.0, 30.0]),
            p("K", [0.0, 0.0, 40.0]),
        ];
        t.wires = vec![
            w("V", [0.0, 0.0, 27.0], None),
            w("C", [0.0, 0.0, 26.9], Some(1)),
            w("N", [0.0, 0.0, 36.0], None),
            w("B", [0.0, -5.0, 36.0], Some(2)),
            w("F", [0.0, 5.0, 36.0], Some(3)),
        ];
        t.strings = vec![
            s("l", "L", "V"),
            s("r", "R", "V"),
            s("i", "V", "C"),
            s("k", "K", "N"),
            s("b", "N", "B"),
            s("f", "N", "F"),
        ];
        t.check_strings().unwrap();
        let v = t.component("C");
        assert_eq!(v.strings, vec![0, 1, 2]);
        assert_eq!(v.wires.len(), 2);
        // The TAK: both sets move together.
        let tak = t.component("B");
        assert_eq!(tak.strings, vec![3, 4, 5]);
        assert_eq!(tak.wires.len(), 3);
        // Cut the TAK off the structure.
        t.strings.retain(|s| s.label != "k");
        assert!(
            t.check_strings()
                .unwrap_err()
                .contains("reach no structure point")
        );
    }
}
