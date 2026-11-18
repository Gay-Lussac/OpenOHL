// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Drag handles in the line's 3D view for the points of the structure
//! being edited (its structure screen open: the Structure editor of a
//! structure without a type, or its type on the Structure types page with
//! the structure selected).
//!
//! - **Handles**: its points where strings hang (orange), its wire points
//!   (blue, where the tables put them, before stringing) and, for a type
//!   with a body (levels 2 and 3), its joints (grey squares). The pointer
//!   over one highlights it.
//! - **Drag** a point or a wire point to move it. Free, it moves in the
//!   plane facing the camera; **X**, **Y** or **Z** while dragging hold
//!   it on that axis of the structure (x across, to the right looking
//!   ahead; y ahead; z up), **Shift + X / Y / Z** in the plane across that
//!   axis; the same key again frees it. It snaps onto another point,
//!   wire point or joint of the structure near the pointer, else to the
//!   snap grid of the structure screen (default 5 cm, 0: none), in the
//!   directions it may move; **Alt** held: no snapping. Its coordinates
//!   in the structure's axes (and how far it moved) are written beside it.
//!   **Esc** cancels the drag.
//! - **What the strings do** ([`crate::string_drag`], the one rule of
//!   every drag, owner 2026-11-18): a **wire point** dragged turns its
//!   string about its top — the string keeps its length and its top, the
//!   wire point goes where the string reaches nearest the pointer (a V:
//!   round the circle both legs reach; three strings or more: refused,
//!   said); no snapping then, and the readout gives the string's length
//!   and angles from the vertical (across and along the line). A
//!   **point** dragged takes everything hanging from it along by the
//!   same displacement (the strings keep their length and direction); a
//!   V whose other leg hangs from a point not dragged turns about that
//!   point, both lengths kept (refused where they cannot meet). What
//!   hangs below a wire point that moves moves along with it.
//!   **Ctrl (⌘ on a Mac)** held while dragging: lengths free — a wire
//!   point goes where the pointer is, a point moves alone, their strings
//!   change length (the readout and the status bar say so).
//! - Released, **one command** (one undo): a point takes its new place
//!   (as typed in the Attachment points table), the wire points moving
//!   with it theirs. A point, wire point or string that comes from the
//!   structure's type becomes the structure's own, over its type's (as a
//!   string window does), and the status bar says so. The line is solved
//!   again as after any edit.
//! - An attachment point of a typed structure is linked to a joint of its
//!   type (2026-10-21): dragged, its **offset** from that joint changes
//!   (the link stays: a rigid link, drawn on its layer); **released on
//!   another joint** (snapped onto it) it is **relinked** there, at no
//!   offset. A support joint takes no strings: a point released on one
//!   keeps its link (the offset changes).
//! - Joints belong to the body: they are shown but not dragged (the body
//!   is edited in OHL-Structure).

use ohl_line::{Structure, StructurePoint};
use ohl_session::{Command, Kind};
use ohl_structure_edit::Handle;
use ohl_structures::structures::{to_global, to_local};

use crate::editors::StringForm;
use crate::model::Model;

/// Screen distance within which the pointer is on a handle, pixels.
pub const RADIUS: f64 = 9.0;

/// Screen distance within which a dragged handle snaps onto another,
/// pixels.
pub const SNAP_RADIUS: f64 = 8.0;

/// The structure whose handles are shown, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// The structure.
    pub structure: String,
    /// The snap grid, m (the structure screen's; 0: none).
    pub grid: f64,
    /// Why a drag cannot be made now (changes not applied in an editor),
    /// said when one is started.
    pub blocked: Option<String>,
}

/// A handle as drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    /// What it moves.
    pub handle: Handle,
    /// Where it is, project coordinates.
    pub global: [f64; 3],
    /// Where it is in the structure's axes (x across, y ahead, z up from
    /// its base).
    pub local: [f64; 3],
    /// Why it is not dragged (part of the body), if so.
    pub fixed: Option<String>,
}

/// What a drag is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Constraint {
    /// The plane facing the camera.
    #[default]
    Free,
    /// One axis of the structure (0 x, 1 y, 2 z).
    Axis(usize),
    /// The plane across one axis of the structure.
    Plane(usize),
}

impl Constraint {
    /// The axes it may move along.
    pub fn free(self) -> [bool; 3] {
        match self {
            Constraint::Free => [true; 3],
            Constraint::Axis(k) => [0, 1, 2].map(|i| i == k),
            Constraint::Plane(k) => [0, 1, 2].map(|i| i != k),
        }
    }

    /// In words, for the readout.
    pub fn describe(self) -> String {
        const AXES: [&str; 3] = ["x", "y", "z"];
        match self {
            Constraint::Free => "facing you".into(),
            Constraint::Axis(k) => format!("along {}", AXES[k]),
            Constraint::Plane(k) => {
                let other: Vec<&str> = (0..3).filter(|&i| i != k).map(|i| AXES[i]).collect();
                format!("in the {}{} plane", other[0], other[1])
            }
        }
    }

    /// Key `k` (0 X, 1 Y, 2 Z) pressed, with Shift or not: that axis (or
    /// plane); the same again frees it.
    pub fn toggled(self, k: usize, plane: bool) -> Self {
        let want = if plane {
            Constraint::Plane(k)
        } else {
            Constraint::Axis(k)
        };
        if self == want { Constraint::Free } else { want }
    }
}

/// A drag in progress.
#[derive(Debug, Clone, PartialEq)]
pub struct Drag {
    /// The structure.
    pub structure: String,
    /// What is dragged.
    pub handle: Handle,
    /// Where it was, structure's axes.
    pub from: [f64; 3],
    /// Where it is now (snapped), structure's axes.
    pub at: [f64; 3],
    /// What it is held to.
    pub constraint: Constraint,
    /// What it snapped onto (or the grid), in words.
    pub snapped: Option<String>,
    /// The handle it snapped onto (a joint: the point is relinked there
    /// when released).
    pub onto: Option<Handle>,
    /// Lengths free ([`crate::string_drag::FREE_KEY`] held): its strings
    /// change length.
    pub free: bool,
    /// What its strings do, in words (the readout).
    pub note: Option<String>,
    /// Why it cannot move there (said when released; nothing moves).
    pub refused: Option<String>,
}

/// What a drag ended with (taken by the application).
#[derive(Debug, Clone, PartialEq)]
pub enum Done {
    /// Released somewhere else: `handle` of `structure` to `at` (its axes).
    Moved {
        /// The structure.
        structure: String,
        /// What moved.
        handle: Handle,
        /// Where to, structure's axes.
        at: [f64; 3],
        /// The handle it was released on (snapped onto), if any.
        onto: Option<Handle>,
        /// Lengths free: its strings change length.
        free: bool,
    },
    /// A handle that is not dragged, or a drag that cannot be made now.
    Refused(String),
    /// Escape.
    Cancelled,
}

/// State of the handles in the 3D view.
#[derive(Debug, Clone, Default)]
pub struct Handles {
    /// The structure being edited (set by the application each frame).
    pub target: Option<Target>,
    /// The handle under the pointer.
    pub hovered: Option<Handle>,
    /// The drag in progress.
    pub drag: Option<Drag>,
    /// How the last drag ended (taken by the application).
    pub done: Option<Done>,
    /// A cancelled drag: the button is still down (the view does not
    /// turn until it is released).
    pub swallow: bool,
}

impl Handles {
    /// Escape: the drag is given up (nothing changes).
    pub fn cancel(&mut self) {
        if self.drag.take().is_some() {
            self.swallow = true;
            self.done = Some(Done::Cancelled);
        }
    }

    /// The readout of the drag: the handle, where it is in the
    /// structure's axes, how far it moved, what it is held to and what
    /// it snapped onto; below, what its strings do (or why it cannot
    /// move).
    pub fn readout(&self) -> Option<String> {
        let d = self.drag.as_ref()?;
        let delta = [0, 1, 2].map(|k| d.at[k] - d.from[k]);
        Some(format!(
            "{} of {}: x {:.3}  y {:.3}  z {:.3} m  (moved {:+.3}, {:+.3}, {:+.3}) — {}{}{}",
            d.handle.describe(),
            d.structure,
            d.at[0],
            d.at[1],
            d.at[2],
            delta[0],
            delta[1],
            delta[2],
            d.constraint.describe(),
            d.snapped
                .as_ref()
                .map_or(String::new(), |s| format!(", on {s}")),
            d.refused
                .as_ref()
                .or(d.note.as_ref())
                .map_or(String::new(), |n| format!("\n{n}"))
        ))
    }
}

/// The drag `d` of structure `s` (as resolved), the pointer putting it at
/// `at` (its axes, snapped): where it goes by the strings' rule
/// ([`crate::string_drag`]) — a wire point with its lengths kept where its
/// strings reach (not snapped) — with what its strings do in words, or
/// why it cannot move (it then stays where it was).
pub fn hold(s: &Structure, d: &mut Drag, at: [f64; 3]) {
    use crate::string_drag as sd;
    let net = sd::Net::of_structure(s);
    let global = |p| to_global(s.position, s.orientation_deg, p);
    let local = |p| to_local(s.position, s.orientation_deg, p);
    d.at = at;
    d.refused = None;
    d.note = None;
    let free = format!("lengths free ({} let go: lengths kept)", sd::FREE_KEY);
    match &d.handle {
        Handle::Wire(w) if !d.free => {
            if net.strings.iter().any(|x| &x.wire == w) {
                // Not snapped: where its strings reach.
                d.snapped = None;
                d.onto = None;
            }
            match sd::drag(&net, &d.handle, global(at), false) {
                Ok(m) => {
                    d.at = local(m.at);
                    let words = sd::strings_words(&net, w, m.at, local);
                    if !words.is_empty() {
                        d.note = Some(format!(
                            "lengths kept: {words} ({} held: lengths free)",
                            sd::FREE_KEY
                        ));
                    }
                }
                Err(e) => {
                    d.at = d.from;
                    d.refused = Some(e);
                }
            }
        }
        Handle::Wire(w) => {
            let words = sd::strings_words(&net, w, global(at), local);
            d.note = Some(format!("{free}: {words}"));
        }
        Handle::Point(_) if d.free => d.note = Some(format!("{free}: the point moves alone")),
        Handle::Point(_) => match sd::drag(&net, &d.handle, global(at), false) {
            Ok(m) => {
                let words = m.describe();
                if let Some(w) = words.strip_prefix("; ") {
                    d.note = Some(w.replace("; ", "\n"));
                }
            }
            Err(e) => {
                d.at = d.from;
                d.refused = Some(e);
            }
        },
        _ => {}
    }
}

/// Direction of the structure's axis `k` in project coordinates.
pub fn axis(orientation_deg: f64, k: usize) -> [f64; 3] {
    let mut e = [0.0; 3];
    e[k] = 1.0;
    to_global([0.0; 3], orientation_deg, e)
}

/// The handles of structure `id` (as resolved).
pub fn spots(model: &Model, id: &str) -> Vec<Spot> {
    let Some(s) = model.line.structure(id) else {
        return Vec::new();
    };
    let local = |p: [f64; 3]| to_local(s.position, s.orientation_deg, p);
    let typed = s
        .structure_type
        .as_deref()
        .and_then(|t| Some((model.types.as_ref()?.0.get(t)?, model.types.as_ref()?)));
    // Every attachment point is dragged: its offset from its joint
    // changes (or it is relinked to the joint it is released on).
    let mut out: Vec<Spot> = s
        .points
        .iter()
        .map(|p| Spot {
            handle: Handle::Point(p.label.clone()),
            global: p.position,
            local: local(p.position),
            fixed: None,
        })
        .chain(s.wires.iter().map(|w| Spot {
            handle: Handle::Wire(w.label.clone()),
            global: w.position,
            local: local(w.position),
            fixed: None,
        }))
        .collect();
    // The joints of the type (its body's), where the type is placed; a
    // point released on an attach joint is linked there.
    if let Some((t, _)) = typed {
        for j in ohl_line::links::placed_joints(t, s) {
            if j.joint.source == ohl_structures::JointSource::Point {
                continue;
            }
            out.push(Spot {
                fixed: Some(format!(
                    "joint {} is not dragged here: joints are edited in OHL-Structure only \
                     (part of the body of type {}; Open this type in OHL-Structure…)",
                    j.joint.id, t.id
                )),
                handle: Handle::Joint(j.joint.id),
                local: local(j.position),
                global: j.position,
            });
        }
    }
    out
}

/// Where the pointer's ray (`a` + s `d`, project coordinates) puts a
/// handle dragged from `from` (the structure's axes) under `constraint`;
/// `facing` is the normal of the plane facing the camera. `None` where
/// the ray runs along the plane (or the axis).
pub fn drag_to(
    s: &Structure,
    from: [f64; 3],
    constraint: Constraint,
    facing: [f64; 3],
    a: [f64; 3],
    d: [f64; 3],
) -> Option<[f64; 3]> {
    let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let o = to_global(s.position, s.orientation_deg, from);
    let plane = |n: [f64; 3]| -> Option<[f64; 3]> {
        let dn = dot(d, n);
        if dn.abs() < 1e-9 {
            return None;
        }
        let t = dot([0, 1, 2].map(|k| o[k] - a[k]), n) / dn;
        let p = [0, 1, 2].map(|k| a[k] + t * d[k]);
        Some(to_local(s.position, s.orientation_deg, p))
    };
    match constraint {
        Constraint::Free => plane(facing),
        Constraint::Plane(k) => {
            let mut p = plane(axis(s.orientation_deg, k))?;
            p[k] = from[k];
            Some(p)
        }
        Constraint::Axis(k) => {
            // The point of the axis line nearest to the ray.
            let e = axis(s.orientation_deg, k);
            let w = [0, 1, 2].map(|i| o[i] - a[i]);
            let (b, dd, ee) = (dot(e, d), dot(d, d), dot(e, e));
            let den = ee * dd - b * b;
            if den.abs() < 1e-9 {
                return None;
            }
            let t = (b * dot(w, d) - dd * dot(w, e)) / den;
            let mut p = from;
            p[k] = from[k] + t;
            Some(p)
        }
    }
}

/// `at` snapped in the directions `constraint` frees: onto `onto` (a
/// handle near the pointer: its coordinates there), else to the grid
/// (`grid` > 0). With what it snapped onto, in words.
pub fn snap(
    at: [f64; 3],
    constraint: Constraint,
    grid: f64,
    onto: Option<&Spot>,
) -> ([f64; 3], Option<String>) {
    let free = constraint.free();
    let mut p = at;
    if let Some(o) = onto {
        for k in 0..3 {
            if free[k] {
                p[k] = o.local[k];
            }
        }
        return (p, Some(o.handle.describe()));
    }
    if grid > 0.0 {
        for k in 0..3 {
            if free[k] {
                p[k] = (p[k] / grid).round() * grid;
                // No "-0.000".
                if p[k] == 0.0 {
                    p[k] = 0.0;
                }
            }
        }
        return (p, Some(format!("the {grid} m grid")));
    }
    (p, None)
}

/// The command a drag makes: `handle` of structure `id` to `at` (its
/// axes), the lengths kept, with what it does in words. A point or wire
/// point from the structure's type becomes the structure's own (the
/// words say so).
#[cfg(test)]
pub fn command(
    model: &Model,
    id: &str,
    handle: &Handle,
    at: [f64; 3],
) -> Result<(String, Command), String> {
    command_drag(model, id, handle, at, None, false)
}

/// [`command_drag`] with the lengths kept.
#[cfg(test)]
pub fn command_onto(
    model: &Model,
    id: &str,
    handle: &Handle,
    at: [f64; 3],
    onto: Option<&Handle>,
) -> Result<(String, Command), String> {
    command_drag(model, id, handle, at, onto, false)
}

/// The command of a drag released at `at` on `onto` (snapped onto it),
/// the strings following by their rule ([`crate::string_drag`]; `free`:
/// lengths free): an attachment point of a typed structure released on a
/// joint its type lets it be linked to is relinked there (no offset);
/// otherwise a linked point keeps its joint, its offset the distance from
/// it.
pub fn command_drag(
    model: &Model,
    id: &str,
    handle: &Handle,
    at: [f64; 3],
    onto: Option<&Handle>,
    free: bool,
) -> Result<(String, Command), String> {
    model.can_edit()?;
    let stored = model
        .stored_line()
        .ok_or("read-only: open a project to edit")?;
    let resolved = model
        .line
        .structure(id)
        .ok_or_else(|| format!("no structure {id}"))?;
    let mut s = stored
        .structure(id)
        .ok_or_else(|| format!("no structure {id}"))?
        .clone();
    if let Some(Spot {
        fixed: Some(why), ..
    }) = spots(model, id).into_iter().find(|x| &x.handle == handle)
    {
        return Err(why);
    }
    let mut global = to_global(resolved.position, resolved.orientation_deg, at);
    let mut own: Vec<String> = Vec::new();
    // Its link: (joint, offset), the joint released on, in words.
    let mut link_words = String::new();
    match handle {
        Handle::Point(label) => {
            let r = resolved
                .point(label)
                .ok_or_else(|| format!("{id} has no point {label}"))?;
            // A typed structure's point: linked to the joint it is
            // released on, else to its own at the new offset.
            let joints = crate::link_tool::joints(model, id);
            // Released on an attach joint (only those take a link).
            let link: Option<(String, [f64; 3])> = match onto {
                Some(Handle::Joint(j)) if joints.iter().any(|x| &x.id == j && x.attach) => {
                    let x = joints.iter().find(|x| &x.id == j).expect("found");
                    global = x.at;
                    link_words = format!("; relinked to joint {j} (no offset)");
                    Some((j.clone(), [0.0; 3]))
                }
                _ if s.structure_type.is_some() && r.is_linked() => {
                    let j = r.joint.clone().expect("linked");
                    match joints.iter().find(|x| x.id == j) {
                        Some(x) => {
                            let o =
                                crate::link_tool::offset(resolved.orientation_deg, x.at, global);
                            link_words =
                                format!("; on joint {j}, {}", crate::link_tool::offset_words(o));
                            Some((j, o))
                        }
                        None => None,
                    }
                }
                _ => None,
            };
            let (joint, offset) = match link {
                Some((j, o)) => (Some(j), o),
                None => (None, [0.0; 3]),
            };
            let unlinked = s.structure_type.is_some() && joint.is_none() && !r.is_linked();
            match s.points.iter_mut().find(|p| &p.label == label) {
                Some(p) => {
                    p.position = global;
                    p.joint = joint;
                    p.offset = offset;
                    p.unlinked = unlinked;
                }
                None if s.structure_type.is_some() => {
                    s.points.push(StructurePoint {
                        label: label.clone(),
                        position: global,
                        description: r.description.clone(),
                        joint,
                        offset,
                        unlinked,
                    });
                    own.push(format!("point {label}"));
                }
                None => return Err(format!("{id} has no point {label}")),
            }
        }
        Handle::Wire(label) => {
            resolved
                .wire(label)
                .ok_or_else(|| format!("{id} has no wire point {label}"))?;
        }
        Handle::Joint(j) => {
            return Err(format!(
                "joint {j} is not dragged here: joints are edited in OHL-Structure only"
            ));
        }
        Handle::Member(m) => return Err(format!("member {m}: move its joints")),
    }
    // The strings' rule: where the wire points go.
    let net = crate::string_drag::Net::of_structure(resolved);
    let moved = crate::string_drag::drag(&net, handle, global, free)?;
    if let Handle::Wire(_) = handle {
        global = moved.at;
    }
    put_wires(
        model, id, resolved, &mut s, handle, global, &moved, &mut own,
    )?;
    let at = to_local(resolved.position, resolved.orientation_deg, global);
    let mut what = format!(
        "moved {} of {id} to x {:.3}, y {:.3}, z {:.3} m (in its axes){link_words}",
        handle.describe(),
        at[0],
        at[1],
        at[2]
    );
    if !own.is_empty() {
        what.push_str(&format!(
            "; {} now the structure's own, over its type's",
            own.join(", ")
        ));
    }
    what.push_str(&moved.describe());
    Ok((
        what,
        Command::Put {
            kind: Kind::Structure,
            id: id.to_owned(),
            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
            ord: None,
        },
    ))
}

/// `s` (as stored, a dragged point already put) takes the wire points of
/// `moved` where they go — put as their string windows put them (those of
/// the type becoming the structure's own, named in `own`), with the
/// strings changing length (lengths free) — at their exact places: those
/// moving where `moved` puts them, the others as they were (not turned
/// into the structure's axes and back). A string whose ends moved alike
/// keeps its length and direction as given; the others take theirs from
/// where their ends are.
#[allow(clippy::too_many_arguments)]
fn put_wires(
    model: &Model,
    id: &str,
    resolved: &Structure,
    s: &mut Structure,
    handle: &Handle,
    at: [f64; 3],
    moved: &crate::string_drag::Moved,
    own: &mut Vec<String>,
) -> Result<(), String> {
    // The wire points whose string windows are put: those moving, and
    // those at the end of a string changing length.
    let mut ends: Vec<String> = moved.wires.iter().map(|w| w.0.clone()).collect();
    for x in &moved.longer {
        if let Some(x) = resolved.string(x)
            && !ends.contains(&x.wire)
        {
            ends.push(x.wire.clone());
        }
    }
    let wires_before: Vec<String> = s.wires.iter().map(|w| w.label.clone()).collect();
    let strings_before: Vec<String> = s.strings.iter().map(|x| x.label.clone()).collect();
    let mut put_wires: Vec<String> = Vec::new();
    let mut put_strings: Vec<String> = Vec::new();
    for w in &ends {
        if put_wires.contains(w) {
            continue;
        }
        let mut form = StringForm::of_wire(model, id, w)
            .ok_or_else(|| format!("{id} has no wire point {w}"))?;
        // The points are not touched (a dragged one is put already).
        form.points.clear();
        if s.structure_type.is_some() {
            for r in &form.wires {
                if !wires_before.contains(&r.label) {
                    own.push(format!("wire point {}", r.label));
                }
            }
            for x in &form.strings {
                if !strings_before.contains(&x.label) {
                    own.push(format!("string {}", x.label));
                }
            }
        }
        put_wires.extend(form.wires.iter().map(|r| r.label.clone()));
        put_strings.extend(form.strings.iter().map(|x| x.label.clone()));
        form.apply_to(s, Some(resolved));
    }
    for w in s.wires.iter_mut().filter(|w| put_wires.contains(&w.label)) {
        if let Some(r) = resolved.wire(&w.label) {
            w.position = moved.wire(&w.label).unwrap_or(r.position);
        }
    }
    // Where each node is, before and after.
    let dragged = match handle {
        Handle::Point(l) | Handle::Wire(l) => Some(l.as_str()),
        _ => None,
    };
    let shift = |l: &str| -> [f64; 3] {
        let Some(was) = resolved.node(l).map(|n| n.position()) else {
            return [0.0; 3];
        };
        let now = if dragged == Some(l) {
            at
        } else {
            moved.wire(l).unwrap_or(was)
        };
        [0, 1, 2].map(|k| now[k] - was[k])
    };
    for x in s.strings.iter_mut() {
        let Some(r) = resolved.string(&x.label) else {
            continue;
        };
        let (a, b) = (shift(&r.top), shift(&r.wire));
        let alike = (0..3).all(|k| (a[k] - b[k]).abs() < 1e-12);
        if alike && put_strings.contains(&x.label) {
            // Both ends moved alike (or neither): as it was.
            x.length = r.length;
            x.angle_deg = r.angle_deg;
            x.direction_deg = r.direction_deg;
        } else if !alike {
            // Turned (or longer, lengths free): its length and direction
            // taken from where its ends are now.
            x.length = 0.0;
            x.angle_deg = 0.0;
            x.direction_deg = 0.0;
        }
    }
    // In the order they were in (new ones last).
    let rank = |list: &[String], l: &str| list.iter().position(|x| x == l).unwrap_or(usize::MAX);
    s.wires.sort_by_key(|w| rank(&wires_before, &w.label));
    s.strings.sort_by_key(|x| rank(&strings_before, &x.label));
    Ok(())
}
