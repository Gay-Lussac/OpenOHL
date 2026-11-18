// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What a dragged attachment point or wire point does to the strings
//! (owner, 2026-11-18): the one rule of every drag of OHL-Design — the
//! line's 3D view ([`crate::handles`]) and the structure screen
//! ([`crate::type_workspace`], also in a placed structure's Edit window,
//! [`crate::placed_editor`]).
//!
//! - **A wire point dragged turns its string about its top**: the string
//!   keeps its length and its top; the wire point goes where the string
//!   reaches nearest the pointer (on the sphere of the string's length
//!   about its top). Held by two strings from different tops (a V), it
//!   goes round the circle both lengths reach; held by three or more
//!   (tops not in one line) it cannot move without changing a length,
//!   and the drag says so and moves nothing.
//! - **An attachment point dragged takes along everything hanging from
//!   it**, by the same displacement: its strings keep their length and
//!   direction.
//! - **What hangs below** a wire point that moves (a chain, as typed in
//!   the string window) moves with it by the same displacement: the
//!   strings below keep their lengths and directions.
//! - A wire point held also by a string from a top that does not move (a
//!   V whose other leg hangs from a point not dragged) **turns about that
//!   top**: every string keeps its length, the wire point goes where the
//!   lengths meet nearest to where the moving legs take it; where they
//!   cannot meet the drag is refused with the reason.
//! - **Lengths free** ([`FREE_KEY`] held while dragging): a length
//!   changes on purpose — a wire point goes where the pointer is (its
//!   strings change length, what hangs below moves along), an attachment
//!   point moves alone (its strings change length). A string's length is
//!   also typed in the string window.

use std::collections::BTreeMap;

use ohl_structure_edit::Handle;

/// The key held while dragging for lengths free, in words.
pub const FREE_KEY: &str = "Ctrl (⌘ on a Mac)";

/// Within this, two tops are one, m.
const SAME_TOP: f64 = 1e-9;

/// Within this, a length is kept (a check of the solution), m.
const KEPT: f64 = 1e-6;

/// A string between two labelled nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Leg {
    /// The string's label.
    pub label: String,
    /// Its top: an attachment point or a wire point.
    pub top: String,
    /// Its wire point.
    pub wire: String,
}

/// The points, wire points and strings of a structure, in any one set of
/// axes (all positions in the same).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Net {
    /// Attachment points: label, position.
    pub points: Vec<(String, [f64; 3])>,
    /// Wire points: label, position.
    pub wires: Vec<(String, [f64; 3])>,
    /// The strings.
    pub strings: Vec<Leg>,
}

impl Net {
    /// Structure `s` of the line as it is (project coordinates).
    pub fn of_structure(s: &ohl_line::Structure) -> Self {
        Net {
            points: s
                .points
                .iter()
                .map(|p| (p.label.clone(), p.position))
                .collect(),
            wires: s
                .wires
                .iter()
                .map(|w| (w.label.clone(), w.position))
                .collect(),
            strings: s
                .strings
                .iter()
                .map(|x| Leg {
                    label: x.label.clone(),
                    top: x.top.clone(),
                    wire: x.wire.clone(),
                })
                .collect(),
        }
    }

    /// A structure type as drawn ([`ohl_structure_edit::Geometry`], its
    /// axes) with the strings of `t`.
    pub fn of_type(t: &ohl_structures::StructureType, g: &ohl_structure_edit::Geometry) -> Self {
        Net {
            points: g.points.clone(),
            wires: g.wires.iter().map(|w| (w.0.clone(), w.1)).collect(),
            strings: t
                .strings
                .iter()
                .map(|x| Leg {
                    label: x.label.clone(),
                    top: x.top.clone(),
                    wire: x.wire.clone(),
                })
                .collect(),
        }
    }

    /// Where point or wire point `label` is.
    pub fn at(&self, label: &str) -> Option<[f64; 3]> {
        self.points
            .iter()
            .chain(&self.wires)
            .find(|p| p.0 == label)
            .map(|p| p.1)
    }

    /// The strings ending at wire point `wire`.
    fn legs_to<'a>(&'a self, wire: &'a str) -> impl Iterator<Item = &'a Leg> + 'a {
        self.strings.iter().filter(move |x| x.wire == wire)
    }
}

/// What a drag does.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Moved {
    /// Where the dragged point or wire point goes.
    pub at: [f64; 3],
    /// The wire points that move (the dragged one too), where to, in the
    /// order of the structure.
    pub wires: Vec<(String, [f64; 3])>,
    /// The wire points moved along by the same displacement (not the
    /// dragged one).
    pub along: Vec<String>,
    /// The wire points turned about a top that does not move (a V), their
    /// strings keeping their lengths.
    pub turned: Vec<String>,
    /// The strings changing length (lengths free only).
    pub longer: Vec<String>,
}

impl Moved {
    /// Where wire point `label` goes, if it moves.
    pub fn wire(&self, label: &str) -> Option<[f64; 3]> {
        self.wires.iter().find(|w| w.0 == label).map(|w| w.1)
    }

    /// In words, for the status bar ("; …" each; empty: nothing hangs on
    /// it).
    pub fn describe(&self) -> String {
        let s = |n: usize| if n > 1 { "s" } else { "" };
        let mut out = String::new();
        if !self.along.is_empty() {
            out.push_str(&format!(
                "; wire point{} {} moved along (their strings keep their length and direction)",
                s(self.along.len()),
                self.along.join(", ")
            ));
        }
        if !self.turned.is_empty() {
            out.push_str(&format!(
                "; wire point{} {} also held by a string from a point not moved: turned \
                 about it, every string keeping its length",
                s(self.turned.len()),
                self.turned.join(", ")
            ));
        }
        if !self.longer.is_empty() {
            out.push_str(&format!(
                "; lengths free: string{} {} change{} length",
                s(self.longer.len()),
                self.longer.join(", "),
                if self.longer.len() > 1 { "" } else { "s" },
            ));
        }
        out
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    a.map(|v| v * k)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The distance from `a` to `b`.
pub fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    norm(sub(b, a))
}

/// Spheres (centre, radius) with the same centre made one (`None`: two
/// radii about one centre that differ).
fn distinct(spheres: &[([f64; 3], f64)]) -> Option<Vec<([f64; 3], f64)>> {
    let mut out: Vec<([f64; 3], f64)> = Vec::new();
    for &(c, r) in spheres {
        match out.iter().find(|o| distance(o.0, c) < SAME_TOP) {
            Some(o) if (o.1 - r).abs() > KEPT => return None,
            Some(_) => {}
            None => out.push((c, r)),
        }
    }
    Some(out)
}

/// Whether the centres all lie on one line.
fn in_line(spheres: &[([f64; 3], f64)]) -> bool {
    if spheres.len() < 3 {
        return true;
    }
    let (a, b) = (spheres[0].0, spheres[1].0);
    let u = sub(b, a);
    let lu = norm(u);
    spheres[2..]
        .iter()
        .all(|s| norm(cross(u, sub(s.0, a))) / lu < SAME_TOP.max(1e-9 * lu))
}

/// The point at the distances given from the centres given (spheres:
/// centre, radius) nearest `toward`; `current`, where it is, sets the
/// side where `toward` gives none. None: they do not meet.
pub fn meet(spheres: &[([f64; 3], f64)], toward: [f64; 3], current: [f64; 3]) -> Option<[f64; 3]> {
    let s = distinct(spheres)?;
    let p = match s.len() {
        0 => return Some(toward),
        1 => {
            let (c, r) = s[0];
            let mut v = sub(toward, c);
            if norm(v) < SAME_TOP {
                v = sub(current, c);
            }
            let n = norm(v);
            if n < SAME_TOP {
                return Some(current);
            }
            add(c, scale(v, r / n))
        }
        _ if in_line(&s) => {
            let (a, ra) = s[0];
            let (b, rb) = s[1];
            // On the axis: the side where it is.
            let u = sub(b, a);
            let off = |q: [f64; 3]| {
                let w = sub(q, a);
                norm(sub(w, scale(u, dot(w, u) / dot(u, u))))
            };
            let toward = if off(toward) < SAME_TOP {
                current
            } else {
                toward
            };
            ohl_structures::structures::v_meeting(a, ra, b, rb, toward)?
        }
        _ => {
            // Three centres not in one line: two points at most.
            let (p1, r1) = s[0];
            let (p2, r2) = s[1];
            let k = (2..s.len()).find(|&k| !in_line(&[s[0], s[1], s[k]]))?;
            let (p3, r3) = s[k];
            let d = distance(p1, p2);
            let ex = scale(sub(p2, p1), 1.0 / d);
            let i = dot(ex, sub(p3, p1));
            let ey0 = sub(sub(p3, p1), scale(ex, i));
            let ey = scale(ey0, 1.0 / norm(ey0));
            let ez = cross(ex, ey);
            let j = dot(ey, sub(p3, p1));
            let x = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
            let y = (r1 * r1 - r3 * r3 + i * i + j * j) / (2.0 * j) - i / j * x;
            let z2 = r1 * r1 - x * x - y * y;
            if z2 < -KEPT * r1.max(1.0) {
                return None;
            }
            let z = z2.max(0.0).sqrt();
            let base = add(p1, add(scale(ex, x), scale(ey, y)));
            let (a, b) = (add(base, scale(ez, z)), add(base, scale(ez, -z)));
            if distance(a, toward) <= distance(b, toward) {
                a
            } else {
                b
            }
        }
    };
    // Every length as asked (a third centre in line, a fourth…).
    s.iter()
        .all(|&(c, r)| (distance(c, p) - r).abs() < KEPT)
        .then_some(p)
}

/// Where dragged wire point `wire` goes with its strings' lengths kept,
/// the pointer at `to`: on its string's sphere, a V's circle; refused
/// where three or more strings from tops not in one line hold it.
pub fn on_strings(net: &Net, wire: &str, to: [f64; 3]) -> Result<[f64; 3], String> {
    let current = net
        .at(wire)
        .ok_or_else(|| format!("no wire point {wire}"))?;
    let legs: Vec<&Leg> = net.legs_to(wire).collect();
    let spheres: Vec<([f64; 3], f64)> = legs
        .iter()
        .filter_map(|x| {
            let top = net.at(&x.top)?;
            Some((top, distance(top, current)))
        })
        .collect();
    let s = distinct(&spheres).unwrap_or_default();
    if s.len() >= 3 && !in_line(&s) {
        let names: Vec<&str> = legs.iter().map(|x| x.label.as_str()).collect();
        return Err(format!(
            "wire point {wire} is held by {} strings ({}): it cannot move without changing a \
             length ({FREE_KEY} held while dragging: lengths free)",
            legs.len(),
            names.join(", ")
        ));
    }
    meet(&spheres, to, current)
        .ok_or_else(|| format!("wire point {wire}: its strings cannot keep their lengths there"))
}

/// The drag of `handle` (an attachment point or a wire point of `net`) to
/// `to` (the same axes), the lengths kept (`free`: lengths free): where
/// it goes and which wire points move where (module docs). An error says
/// why it cannot move.
pub fn drag(net: &Net, handle: &Handle, to: [f64; 3], free: bool) -> Result<Moved, String> {
    let (grabbed, at) = match handle {
        Handle::Point(l) => {
            net.points
                .iter()
                .find(|p| &p.0 == l)
                .ok_or_else(|| format!("no point {l}"))?;
            (l.as_str(), to)
        }
        Handle::Wire(l) => {
            let at = if free {
                net.at(l).ok_or_else(|| format!("no wire point {l}"))?;
                to
            } else {
                on_strings(net, l, to)?
            };
            (l.as_str(), at)
        }
        Handle::Joint(j) => return Err(format!("joint {j}: not dragged here")),
        Handle::Member(m) => return Err(format!("member {m}: move its joints")),
    };
    let mut out = Moved {
        at,
        ..Default::default()
    };
    let wire_dragged = matches!(handle, Handle::Wire(_));
    if free {
        // Its strings change length.
        out.longer = net
            .strings
            .iter()
            .filter(|x| {
                if wire_dragged {
                    x.wire == grabbed
                } else {
                    x.top == grabbed
                }
            })
            .map(|x| x.label.clone())
            .collect();
        if !wire_dragged {
            // A point moves alone.
            return Ok(out);
        }
    }
    let mut new: BTreeMap<String, [f64; 3]> = BTreeMap::new();
    new.insert(grabbed.to_owned(), at);
    // The wire points below the dragged one (through strings).
    let mut below: Vec<String> = Vec::new();
    loop {
        let next = net.strings.iter().find(|x| {
            (x.top == grabbed || below.contains(&x.top))
                && x.wire != grabbed
                && !below.contains(&x.wire)
        });
        match next {
            Some(x) => below.push(x.wire.clone()),
            None => break,
        }
    }
    // Placed once every top of theirs is.
    let mut left = below.clone();
    while !left.is_empty() {
        let ready = left.iter().position(|w| {
            net.legs_to(w)
                .all(|x| !left.contains(&x.top) || x.top == *w)
        });
        let Some(k) = ready else {
            return Err(format!(
                "wire points {} hang from each other in a loop",
                left.join(", ")
            ));
        };
        let w = left.remove(k);
        let Some(was) = net.at(&w) else { continue };
        // Each string: its top before and after.
        let tops: Vec<([f64; 3], [f64; 3])> = net
            .legs_to(&w)
            .filter_map(|x| {
                let a = net.at(&x.top)?;
                Some((a, new.get(&x.top).copied().unwrap_or(a)))
            })
            .collect();
        let shifts: Vec<[f64; 3]> = tops.iter().map(|(a, b)| sub(*b, *a)).collect();
        let first = shifts.first().copied().unwrap_or([0.0; 3]);
        let alike = shifts.iter().all(|d| distance(*d, first) < 1e-12);
        let p = if alike {
            out.along.push(w.clone());
            add(was, first)
        } else {
            // Turned about the tops that stay: every length kept, nearest
            // where the moving tops take it.
            let moving: Vec<&[f64; 3]> = shifts.iter().filter(|d| norm(**d) > 0.0).collect();
            let mean = scale(
                moving.iter().fold([0.0; 3], |a, d| add(a, **d)),
                1.0 / moving.len().max(1) as f64,
            );
            let spheres: Vec<([f64; 3], f64)> =
                tops.iter().map(|(a, b)| (*b, distance(*a, was))).collect();
            let names: Vec<String> = net.legs_to(&w).map(|x| x.label.clone()).collect();
            let p = meet(&spheres, add(was, mean), was).ok_or_else(|| {
                format!(
                    "wire point {w}: strings {} cannot keep their lengths — their tops would \
                     be too far apart (or too close); {FREE_KEY} held: lengths free",
                    names.join(", ")
                )
            })?;
            out.turned.push(w.clone());
            p
        };
        new.insert(w, p);
    }
    out.wires = net
        .wires
        .iter()
        .filter_map(|(l, _)| Some((l.clone(), *new.get(l)?)))
        .collect();
    let keep = |l: &String| out.wires.iter().any(|w| &w.0 == l);
    out.along.retain(keep);
    out.turned.retain(keep);
    Ok(out)
}

/// A string from `top` to `wire` (structure axes: x across, y ahead, z
/// up) in words: its length, its angle from the vertical, across and
/// along the line.
pub fn swing_words(label: &str, top: [f64; 3], wire: [f64; 3]) -> String {
    let v = sub(wire, top);
    let l = norm(v);
    let total = if l > 0.0 {
        (-v[2] / l).clamp(-1.0, 1.0).acos().to_degrees()
    } else {
        0.0
    };
    format!(
        "{label} {l:.3} m, {total:.1}° from the vertical (across {:+.1}°, along {:+.1}°)",
        v[0].atan2(-v[2]).to_degrees(),
        v[1].atan2(-v[2]).to_degrees()
    )
}

/// The strings to wire point `wire` of `net` in words ([`swing_words`]),
/// the wire point at `at`; `local` turns a position into the
/// structure's axes.
pub fn strings_words(
    net: &Net,
    wire: &str,
    at: [f64; 3],
    local: impl Fn([f64; 3]) -> [f64; 3],
) -> String {
    net.legs_to(wire)
        .filter_map(|x| Some(swing_words(&x.label, local(net.at(&x.top)?), local(at))))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg(label: &str, top: &str, wire: &str) -> Leg {
        Leg {
            label: label.into(),
            top: top.into(),
            wire: wire.into(),
        }
    }

    fn net(
        points: &[(&str, [f64; 3])],
        wires: &[(&str, [f64; 3])],
        legs: &[(&str, &str, &str)],
    ) -> Net {
        Net {
            points: points.iter().map(|p| (p.0.into(), p.1)).collect(),
            wires: wires.iter().map(|p| (p.0.into(), p.1)).collect(),
            strings: legs.iter().map(|x| leg(x.0, x.1, x.2)).collect(),
        }
    }

    fn wire(l: &str) -> Handle {
        Handle::Wire(l.into())
    }

    /// An I string: its wire point dragged far away keeps the length (to
    /// 1e-9 m) and the top; it points towards the pointer.
    #[test]
    fn a_wire_point_turns_about_its_top() {
        let n = net(
            &[("A1", [2.0, 1.0, 30.0])],
            &[("W1", [2.0, 1.0, 26.5])],
            &[("S1", "A1", "W1")],
        );
        let to = [40.0, -25.0, 3.0];
        let m = drag(&n, &wire("W1"), to, false).unwrap();
        let top = [2.0, 1.0, 30.0];
        assert!((distance(top, m.at) - 3.5).abs() < 1e-9, "{m:?}");
        let (u, v) = (sub(m.at, top), sub(to, top));
        let cos = dot(u, v) / norm(u) / norm(v);
        assert!((cos - 1.0).abs() < 1e-12, "{cos}");
        assert_eq!(m.wires, [("W1".to_owned(), m.at)]);
        assert!(m.longer.is_empty() && m.along.is_empty());
        // The pointer on the top: where it is.
        let m = drag(&n, &wire("W1"), top, false).unwrap();
        assert_eq!(m.at, [2.0, 1.0, 26.5]);
        // Lengths free: where the pointer is, the string longer.
        let m = drag(&n, &wire("W1"), to, true).unwrap();
        assert_eq!(m.at, to);
        assert_eq!(m.longer, ["S1"]);
        assert!(
            m.describe()
                .contains("lengths free: string S1 changes length")
        );
    }

    /// A V: dragged anywhere its wire point stays where both legs reach
    /// (both lengths to 1e-9 m).
    #[test]
    fn a_v_keeps_both_lengths() {
        let (a, b) = ([-1.0, 0.0, 30.0], [1.0, 0.0, 30.0]);
        let w = [0.0, 0.0, 27.0];
        let n = net(
            &[("A1", a), ("A2", b)],
            &[("W1", w)],
            &[("S1", "A1", "W1"), ("S2", "A2", "W1")],
        );
        let (la, lb) = (distance(a, w), distance(b, w));
        for to in [[5.0, 3.0, 20.0], [0.0, -4.0, 30.0], [0.0, 0.0, 10.0]] {
            let m = drag(&n, &wire("W1"), to, false).unwrap();
            assert!((distance(a, m.at) - la).abs() < 1e-9, "{to:?} {m:?}");
            assert!((distance(b, m.at) - lb).abs() < 1e-9, "{to:?} {m:?}");
        }
        // Swung ahead: y > 0, still in the plane x = 0.
        let m = drag(&n, &wire("W1"), [0.0, 3.0, 28.0], false).unwrap();
        assert!(m.at[0].abs() < 1e-12 && m.at[1] > 1.0, "{m:?}");
    }

    /// Three strings from tops not in one line hold it: refused, said;
    /// nothing moves. In one line: a circle still.
    #[test]
    fn three_strings_refuse() {
        let w = [0.0, 0.0, 27.0];
        let n = net(
            &[
                ("A1", [-1.0, 0.0, 30.0]),
                ("A2", [1.0, 0.0, 30.0]),
                ("A3", [0.0, 1.0, 30.0]),
            ],
            &[("W1", w)],
            &[("S1", "A1", "W1"), ("S2", "A2", "W1"), ("S3", "A3", "W1")],
        );
        let e = drag(&n, &wire("W1"), [3.0, 3.0, 25.0], false).unwrap_err();
        assert!(
            e.contains("held by 3 strings (S1, S2, S3): it cannot move without changing a length"),
            "{e}"
        );
        // Lengths free: it moves.
        assert!(drag(&n, &wire("W1"), [3.0, 3.0, 25.0], true).is_ok());
        let n = net(
            &[
                ("A1", [-1.0, 0.0, 30.0]),
                ("A2", [1.0, 0.0, 30.0]),
                ("A3", [3.0, 0.0, 30.0]),
            ],
            &[("W1", w)],
            &[("S1", "A1", "W1"), ("S2", "A2", "W1"), ("S3", "A3", "W1")],
        );
        let m = drag(&n, &wire("W1"), [0.0, 3.0, 28.0], false).unwrap();
        for a in [[-1.0, 0.0, 30.0], [1.0, 0.0, 30.0], [3.0, 0.0, 30.0]] {
            assert!((distance(a, m.at) - distance(a, w)).abs() < 1e-9);
        }
    }

    /// A chain: W2 hangs from W1; W1 dragged turns about A1, W2 follows
    /// by the same displacement (S2 keeps its length and direction); the
    /// upper wire point W1 is the top W2 turns about when W2 is dragged.
    #[test]
    fn a_chain_follows() {
        let n = net(
            &[("A1", [0.0, 0.0, 30.0])],
            &[("W1", [0.0, 0.0, 28.0]), ("W2", [0.0, 0.0, 25.0])],
            &[("S1", "A1", "W1"), ("S2", "W1", "W2")],
        );
        let m = drag(&n, &wire("W1"), [2.0, 0.0, 28.0], false).unwrap();
        let w2 = m.wire("W2").unwrap();
        let d = sub(m.at, [0.0, 0.0, 28.0]);
        assert!(distance(w2, add([0.0, 0.0, 25.0], d)) < 1e-12);
        assert!((distance([0.0, 0.0, 30.0], m.at) - 2.0).abs() < 1e-9);
        assert_eq!(m.along, ["W2"]);
        let m = drag(&n, &wire("W2"), [5.0, 0.0, 28.0], false).unwrap();
        assert!((distance([0.0, 0.0, 28.0], m.at) - 3.0).abs() < 1e-9);
        assert_eq!(m.wires.len(), 1);
    }

    /// An attachment point dragged: every wire point hanging from it (and
    /// below) moves by the same vector; a V whose other leg hangs from a
    /// point not dragged turns about that point, both lengths kept; with
    /// lengths free the point moves alone.
    #[test]
    fn a_point_takes_what_hangs_along() {
        let n = net(
            &[
                ("A1", [0.0, 0.0, 30.0]),
                ("A2", [4.0, 0.0, 30.0]),
                ("A3", [6.0, 0.0, 30.0]),
            ],
            &[
                ("W1", [0.0, 0.0, 27.0]),
                ("W1b", [0.0, 0.0, 25.0]),
                ("W2", [5.0, 0.0, 27.0]),
                ("W9", [9.0, 0.0, 27.0]),
            ],
            &[
                ("S1", "A1", "W1"),
                ("S1b", "W1", "W1b"),
                ("S2", "A2", "W2"),
                ("S2b", "A3", "W2"),
                ("S9", "A3", "W9"),
            ],
        );
        let to = [0.5, 1.0, 31.0];
        let m = drag(&n, &Handle::Point("A1".into()), to, false).unwrap();
        assert_eq!(m.at, to);
        let d = [0.5, 1.0, 1.0];
        assert_eq!(m.wire("W1"), Some(add([0.0, 0.0, 27.0], d)));
        assert_eq!(m.wire("W1b"), Some(add([0.0, 0.0, 25.0], d)));
        assert_eq!(m.wires.len(), 2);
        assert_eq!(m.along, ["W1", "W1b"]);
        // A V: A2 up 0.5 m, W2 turns about A3.
        let w2 = [5.0, 0.0, 27.0];
        let to = [4.0, 0.0, 30.5];
        let m = drag(&n, &Handle::Point("A2".into()), to, false).unwrap();
        let p = m.wire("W2").unwrap();
        assert!((distance(to, p) - distance([4.0, 0.0, 30.0], w2)).abs() < 1e-9);
        assert!((distance([6.0, 0.0, 30.0], p) - distance([6.0, 0.0, 30.0], w2)).abs() < 1e-9);
        assert_eq!(m.turned, ["W2"]);
        assert!(m.describe().contains("turned about it"), "{}", m.describe());
        // Too far: the legs cannot meet.
        let e = drag(&n, &Handle::Point("A2".into()), [-10.0, 0.0, 30.0], false).unwrap_err();
        assert!(e.contains("cannot keep their lengths"), "{e}");
        // Lengths free: the point alone.
        let m = drag(&n, &Handle::Point("A1".into()), [0.5, 1.0, 31.0], true).unwrap();
        assert!(m.wires.is_empty());
        assert_eq!(m.longer, ["S1"]);
    }

    /// The readout of a string: its length and angles from the vertical.
    #[test]
    fn swing_in_words() {
        let t = swing_words("S1", [0.0, 0.0, 30.0], [1.0, 0.0, 29.0]);
        assert_eq!(
            t,
            "S1 1.414 m, 45.0° from the vertical (across +45.0°, along +0.0°)"
        );
    }
}
