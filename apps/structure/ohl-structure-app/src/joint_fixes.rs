// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A joint's problems and their fixes (owner, 2026-11-08: "the joint is
//! giving a lot of errors and i would like to have a tool to help resolve
//! errors"). Each problem the joint details find
//! ([`ohl_structure_link::Problem`]) gets the changes that could fix it —
//! the other face, a turn, the gauge or end distance that keeps the
//! code's distances, nesting the other way, a cut worked out — and each is
//! **tried on a copy of the structure** first: offered only when the
//! problems of the joint are fewer after it and none is added elsewhere.
//! A fix is applied as one undo step.

use std::sync::mpsc::{Receiver, channel};

use ohl_structure_edit::connection_edits::{self, FieldValue};
use ohl_structure_link::{Arrival, JointDetails, Problem};
use ohl_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

/// One change of a fix.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A field of a member end's connection given (`None`: not given).
    Field {
        /// The member.
        member: String,
        /// The joint.
        joint: String,
        /// The field.
        field: &'static str,
        /// Its value.
        value: Option<FieldValue>,
    },
    /// A member's roll, radians.
    Roll {
        /// The member.
        member: String,
        /// The roll.
        roll: f64,
    },
    /// A member end's bolts where they are (the leg bolted's, the other
    /// leg's).
    Points {
        /// The member.
        member: String,
        /// The joint.
        joint: String,
        /// On the leg bolted.
        bolted: Vec<[f64; 2]>,
        /// On the other leg.
        other: Vec<[f64; 2]>,
    },
}

impl Change {
    /// Made in `doc`.
    pub fn apply(&self, doc: &mut DocumentMut) {
        match self {
            Change::Field {
                member,
                joint,
                field,
                value,
            } => {
                let _ = connection_edits::set_connection_field(
                    doc,
                    member,
                    joint,
                    field,
                    value.as_ref(),
                );
            }
            Change::Roll { member, roll } => {
                ohl_structure_edit::edits::set_member_roll(doc, member, *roll);
            }
            Change::Points {
                member,
                joint,
                bolted,
                other,
            } => connection_edits::set_bolt_points(doc, member, joint, bolted, Some(other), None),
        }
    }
}

/// A fix: what it says and its changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    /// In words ("Turn it 90°").
    pub label: String,
    /// The changes, one undo step.
    pub changes: Vec<Change>,
}

/// A problem of a member end at a joint and the fixes found for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// The member.
    pub member: String,
    /// The problem in words.
    pub text: String,
    /// The fixes that worked on a copy, each with the joint's problems
    /// before and after.
    pub fixes: Vec<(Fix, usize, usize)>,
}

/// The problems of joint `joint` in `details`, in words, by member.
pub fn problems(details: &JointDetails, joint: &str) -> Vec<(String, Problem, String)> {
    details
        .joint(joint)
        .map(|j| {
            j.arrivals
                .iter()
                .flat_map(|a| {
                    a.problems
                        .iter()
                        .map(|(p, t)| (a.member.clone(), p.clone(), t.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// How many problems the structure has: at `joint` and in all.
fn count(details: &JointDetails, joint: &str) -> (usize, usize) {
    let all = details
        .joints
        .iter()
        .map(|j| j.arrivals.iter().map(|a| a.problems.len()).sum::<usize>())
        .sum();
    (problems(details, joint).len(), all)
}

/// The degrees of a turn, in words.
fn deg(turn: f64) -> String {
    format!("{:.0}°", turn.to_degrees())
}

/// The fixes to try for problem `p` of `a` (member end at `joint`) of
/// structure `t`.
pub fn candidates(t: &StructureType, joint: &str, a: &Arrival, p: &Problem) -> Vec<Fix> {
    let m = a.member.clone();
    let field = |member: &str, f: &'static str, v: Option<FieldValue>| Change::Field {
        member: member.to_owned(),
        joint: joint.to_owned(),
        field: f,
        value: v,
    };
    let word = |w: &str| Some(FieldValue::Word(w.to_owned()));
    let roll = crate::roll_ui::roll_of(t, &m);
    let turn = |by: f64| Fix {
        label: format!("Turn {m} {}", deg(by)),
        changes: vec![Change::Roll {
            member: m.clone(),
            roll: roll + by,
        }],
    };
    let quarter = std::f64::consts::FRAC_PI_2;
    let given_points = t
        .connection(&m, joint)
        .is_some_and(|c| !c.bolt_points.is_empty() || !c.other_leg_points.is_empty());
    let lap = |face: Option<&str>| Fix {
        label: match face {
            None => format!("Lap {m} on the outside instead of nesting"),
            Some(f) => format!("Lap {m} on the {f} face instead of nesting"),
        },
        changes: vec![field(&m, "face", face.and_then(word))],
    };
    match p {
        Problem::NotNestable { .. } => vec![lap(None), lap(Some("inside"))],
        Problem::NestTurned { .. } => vec![
            turn(quarter),
            turn(-quarter),
            turn(2.0 * quarter),
            lap(None),
        ],
        Problem::NestTooWide { host } => vec![
            Fix {
                label: format!("Nest {host} in {m} instead"),
                changes: vec![
                    field(&m, "face", None),
                    field(host, "onto", word(&m)),
                    field(host, "face", word("nested")),
                ],
            },
            lap(None),
        ],
        Problem::NestEdge { gauge_max, .. } => {
            // Down to the millimetre, the bolts no further out than it.
            let g = (gauge_max * 1000.0).floor() / 1000.0;
            if g <= 0.0 {
                return vec![lap(None)];
            }
            let fix = if given_points {
                Change::Points {
                    member: m.clone(),
                    joint: joint.to_owned(),
                    bolted: a.bolt_flat.iter().map(|q| [q[0], q[1].min(g)]).collect(),
                    other: a.other_flat.iter().map(|q| [q[0], q[1].min(g)]).collect(),
                }
            } else {
                let gauges: Vec<f64> = a
                    .gauges
                    .value
                    .clone()
                    .unwrap_or_else(|| vec![g])
                    .iter()
                    .map(|x| x.min(g))
                    .collect();
                field(&m, "gauges", Some(FieldValue::Lengths(gauges)))
            };
            vec![
                Fix {
                    label: format!(
                        "Gauge {:.0} mm (the largest keeping both edges)",
                        g * 1000.0
                    ),
                    changes: vec![fix],
                },
                lap(None),
            ]
        }
        Problem::NestEnd { short, .. } => {
            let by = (short * 1000.0).ceil() / 1000.0;
            let fix = if given_points {
                Change::Points {
                    member: m.clone(),
                    joint: joint.to_owned(),
                    bolted: a.bolt_flat.iter().map(|q| [q[0] + by, q[1]]).collect(),
                    other: a.other_flat.iter().map(|q| [q[0] + by, q[1]]).collect(),
                }
            } else {
                let e = a.end_distance.value.unwrap_or(0.0) + by;
                field(&m, "end_distance", Some(FieldValue::Length(e)))
            };
            vec![Fix {
                label: format!("Bolts {:.0} mm further from the end", by * 1000.0),
                changes: vec![fix],
            }]
        }
        Problem::HostCut { host } => vec![Fix {
            label: format!("Where {host}'s steel stops: worked out"),
            changes: vec![field(host, "cut", None)],
        }],
        Problem::ThroughLeg { .. } => {
            let other = match a.sits.value {
                Some(ohl_structures::connections::Face::Inside) => "outside",
                _ => "inside",
            };
            vec![
                Fix {
                    label: format!("{m} on the {other} face"),
                    changes: vec![field(&m, "face", word(other))],
                },
                turn(2.0 * quarter),
                turn(quarter),
                turn(-quarter),
            ]
        }
        Problem::NotFlat { turn: by, .. } => vec![
            Fix {
                label: format!("Turn {m} {} (lays it flat)", deg(*by)),
                changes: vec![Change::Roll {
                    member: m.clone(),
                    roll: roll + by,
                }],
            },
            Fix {
                label: format!(
                    "Turn {m} {} (flat, its outstanding leg the other way)",
                    deg(by + 2.0 * quarter)
                ),
                changes: vec![Change::Roll {
                    member: m.clone(),
                    roll: roll + by + 2.0 * quarter,
                }],
            },
        ],
    }
}

/// `text` with `fix` made.
pub fn applied(text: &str, fix: &Fix) -> Option<String> {
    let mut doc: DocumentMut = text.parse().ok()?;
    for c in &fix.changes {
        c.apply(&mut doc);
    }
    Some(doc.to_string())
}

/// Every problem of `joint` with the fixes that work: each candidate
/// tried on a copy of `text`, kept when the joint's problems are fewer
/// after it and the structure's no more than before less those.
pub fn find(
    text: &str,
    sections: &ohl_sections::SectionLibrary,
    bolts: &ohl_structures::bolts::BoltLibrary,
    joint: &str,
) -> Vec<Found> {
    let Ok(details) = ohl_structure_link::joint_details(text, sections, bolts) else {
        return Vec::new();
    };
    let Ok(t) = StructureType::parse(std::path::Path::new("structure.ohls"), text) else {
        return Vec::new();
    };
    let (here, all) = count(&details, joint);
    let mut out: Vec<Found> = Vec::new();
    let mut tried: Vec<(Fix, Option<(usize, usize)>)> = Vec::new();
    for (member, p, words) in problems(&details, joint) {
        let Some(a) = details
            .joint(joint)
            .and_then(|j| j.arrivals.iter().find(|a| a.member == member))
        else {
            continue;
        };
        let mut fixes = Vec::new();
        for fix in candidates(&t, joint, a, &p) {
            // A fix found for an earlier problem: its trial again.
            let after = match tried.iter().find(|x| x.0 == fix) {
                Some(x) => x.1,
                None => {
                    let r = applied(text, &fix)
                        .and_then(|x| ohl_structure_link::joint_details(&x, sections, bolts).ok())
                        .map(|d| count(&d, joint));
                    tried.push((fix.clone(), r));
                    r
                }
            };
            // Fewer here, and elsewhere no more than before (a − h ≤
            // all − here).
            if let Some((h, a)) = after
                && h < here
                && a + here <= all + h
            {
                fixes.push((fix, here, h));
            }
        }
        // The one leaving fewest first.
        fixes.sort_by_key(|f| f.2);
        out.push(Found {
            member,
            text: words,
            fixes,
        });
    }
    out
}

/// What the fixes are found for: the file's revision, the libraries'
/// stamp, the joint.
pub type FixKey = (u64, u64, String);

/// What a search needs: the structure's text and its libraries.
pub type FixInput = (
    String,
    ohl_sections::SectionLibrary,
    ohl_structures::bolts::BoltLibrary,
);

/// The fixes of a joint, found in the background.
#[derive(Debug, Default)]
pub struct Finder {
    /// The fixes found, and for what.
    pub found: Option<(FixKey, Vec<Found>)>,
    /// The search under way, and for what.
    pending: Option<(FixKey, Receiver<Vec<Found>>)>,
}

impl Finder {
    /// Whether a search for `key` must start (neither found nor under
    /// way): its input is then needed.
    pub fn needs(&mut self, key: &FixKey) -> bool {
        if let Some((k, rx)) = &self.pending
            && let Ok(found) = rx.try_recv()
        {
            self.found = Some((k.clone(), found));
            self.pending = None;
        }
        !self.found.as_ref().is_some_and(|(k, _)| k == key)
            && !self.pending.as_ref().is_some_and(|(k, _)| k == key)
    }

    /// The problems and fixes of `key`: found, else searched for in the
    /// background with `input` (none until then; `ctx` asked to draw
    /// again when they are).
    pub fn get(
        &mut self,
        key: FixKey,
        input: Option<FixInput>,
        ctx: &eframe::egui::Context,
    ) -> Option<&[Found]> {
        if let Some((k, rx)) = &self.pending {
            if let Ok(found) = rx.try_recv() {
                self.found = Some((k.clone(), found));
                self.pending = None;
            } else if *k == key {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return None;
            }
        }
        if self.found.as_ref().is_some_and(|(k, _)| *k == key) {
            return self.found.as_ref().map(|(_, f)| f.as_slice());
        }
        let (text, sections, bolts) = input?;
        // Tests draw frames without waiting: found at once there.
        if cfg!(test) {
            let found = find(&text, &sections, &bolts, &key.2);
            self.found = Some((key, found));
            return self.found.as_ref().map(|(_, f)| f.as_slice());
        }
        let (tx, rx) = channel();
        let joint = key.2.clone();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(find(&text, &sections, &bolts, &joint));
            c.request_repaint();
        });
        self.pending = Some((key, rx));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example sections and a 45 × 5 angle to nest in the 61 × 7 legs.
    fn nest_library() -> ohl_sections::SectionLibrary {
        let mut lib = ohl_structure_link::section_library(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap();
        lib.merge(
            ohl_sections::SectionLibrary::parse(
                "format = \"ohl-sections\"\nversion = 1\nname = \"nest\"\n\
                 default_material = \"steel-example\"\n[[section]]\nid = \"EA-45x5\"\n\
                 name = \"Equal angle 45 × 5 (invented)\"\nshape = \"equal_angle\"\n\
                 h = \"45 mm\"\nt = \"5 mm\"\nr = \"6 mm\"\n",
            )
            .unwrap(),
        );
        lib
    }

    /// The angles tower with L2 a 45 × 5 nested in L1 at N2, its two bolts
    /// `gauge` from the heel, the first `end` from the joint.
    fn nested_leg(gauge: &str, end: &str) -> String {
        crate::joint_ui_tests::ANGLES.replacen(
            "id = \"L2\"\nnodes = [\"N2\", \"N3\"]\nsection = \"EA-61x7\"",
            "id = \"L2\"\nnodes = [\"N2\", \"N3\"]\nsection = \"EA-45x5\"",
            1,
        ) + &format!(
            "\n[[connection]]\nmember = \"L2\"\njoint = \"N2\"\nonto = \"L1\"\n\
             face = \"nested\"\nleg = \"long\"\n\
             bolts = {{ count = 2, diameter = \"12 mm\", grade = \"8.8\" }}\n\
             gauges = [\"{gauge}\"]\ngauge_lines = 1\npitch = \"40 mm\"\n\
             end_distance = \"{end}\"\n"
        )
    }

    /// Bolts too near the edge, then too near the end, of an angle nested
    /// in the leg: the gauge, then the end distance offered, each tried
    /// and clearing that problem.
    #[test]
    fn the_gauge_and_end_distance_fixes_clear_their_problems() {
        let lib = nest_library();
        let bolts = Default::default();
        assert!(
            find(&nested_leg("25 mm", "25 mm"), &lib, &bolts, "N2")
                .iter()
                .all(|f| f.member != "L2"),
            "it fits as given"
        );
        for (text, label) in [
            (nested_leg("40 mm", "25 mm"), "Gauge"),
            (nested_leg("25 mm", "5 mm"), "further from the end"),
        ] {
            let found = find(&text, &lib, &bolts, "N2");
            let l2 = found.iter().find(|f| f.member == "L2").expect("a problem");
            let (fix, _, after) = l2
                .fixes
                .iter()
                .find(|f| f.0.label.contains(label))
                .unwrap_or_else(|| panic!("{label}: {found:?}"));
            assert_eq!(*after, 0, "{fix:?}");
            let fixed = applied(&text, fix).unwrap();
            assert!(find(&fixed, &lib, &bolts, "N2").is_empty());
        }
    }

    /// A diagonal nested in the leg it is not in line with: the lap
    /// fixes are offered (they leave the joint without problems), and
    /// applied the problem is gone.
    #[test]
    fn a_nested_member_not_in_line_is_fixed_by_lapping_it() {
        let lib = ohl_structure_link::section_library(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap();
        let text = crate::joint_ui_tests::ANGLES.replace(
            "member = \"D1\"\njoint = \"N2\"\nonto = \"L1\"",
            "member = \"D1\"\njoint = \"N2\"\nonto = \"L1\"\nface = \"nested\"",
        );
        let bolts = Default::default();
        let found = find(&text, &lib, &bolts, "N2");
        let d1 = found
            .iter()
            .find(|f| f.member == "D1")
            .expect("a problem of D1");
        assert!(d1.text.contains("not in line"), "{}", d1.text);
        assert!(!d1.fixes.is_empty(), "{d1:?}");
        let (fix, before, after) = &d1.fixes[0];
        assert!(after < before);
        let fixed = applied(&text, fix).unwrap();
        let again = find(&fixed, &lib, &bolts, "N2");
        assert!(!again.iter().any(|f| f.member == "D1"), "{again:?}");
    }
}
