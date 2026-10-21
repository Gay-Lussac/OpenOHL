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
//! - Released, **one command** (one undo): a point takes its new place
//!   (as typed in the Attachment points table) and takes along, by the
//!   same displacement, the wire points whose strings all hang from it
//!   (and what hangs below them), given by their length and direction or
//!   by their own position alike: their strings keep their length and
//!   direction ([`moving_with`]). A wire point also held by a string from
//!   another point (a V) stays: its strings from the point change length,
//!   and the status bar says so. A wire point moves with what hangs below
//!   it (as typed in its string's window: the strings holding it change
//!   length). A point or wire point that comes from the structure's type
//!   becomes the structure's own, over its type's (as a string window
//!   does), and the status bar says so. The line is solved again as after
//!   any edit.
//! - Joints, and points sitting on a joint or a member, belong to the
//!   body: they are shown but not dragged (the body is edited in
//!   OHL-Structure).

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
    /// it snapped onto.
    pub fn readout(&self) -> Option<String> {
        let d = self.drag.as_ref()?;
        let delta = [0, 1, 2].map(|k| d.at[k] - d.from[k]);
        Some(format!(
            "{} of {}: x {:.3}  y {:.3}  z {:.3} m  (moved {:+.3}, {:+.3}, {:+.3}) — {}{}",
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
                .map_or(String::new(), |s| format!(", on {s}"))
        ))
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
    // Points from the type sitting on the body: not dragged here.
    let on_body = |label: &str| -> Option<String> {
        let (t, _) = typed?;
        // The structure's own point: its own position, free.
        if model
            .stored_line()
            .and_then(|l| l.structure(id))
            .is_some_and(|x| x.point(label).is_some())
        {
            return None;
        }
        let p = t.points.iter().find(|p| p.label == label)?;
        let on = p
            .node
            .as_ref()
            .map(|n| format!("joint {n}"))
            .or_else(|| p.member.as_ref().map(|m| format!("member {m}")))?;
        Some(format!(
            "point {label} sits on {on} of the body of type {}: it moves with the body \
             (Open this type in OHL-Structure…)",
            t.id
        ))
    };
    let mut out: Vec<Spot> = s
        .points
        .iter()
        .map(|p| Spot {
            handle: Handle::Point(p.label.clone()),
            global: p.position,
            local: local(p.position),
            fixed: on_body(&p.label),
        })
        .chain(s.wires.iter().map(|w| Spot {
            handle: Handle::Wire(w.label.clone()),
            global: w.position,
            local: local(w.position),
            fixed: None,
        }))
        .collect();
    // The joints of the type's body, where its stick drawing is.
    if let Some((t, (_, insulators))) = typed
        && let Ok(lift) = t.lift(s.extension.as_deref())
    {
        let base = [
            s.position[0],
            s.position[1],
            s.position[2] + s.height_adjust,
        ];
        let g = ohl_structure_edit::geometry(t, insulators);
        for (j, p, _) in g.joints {
            let global = to_global(base, s.orientation_deg, [p[0], p[1], p[2] + lift]);
            out.push(Spot {
                fixed: Some(format!(
                    "joint {j} is not dragged here: joints are edited in OHL-Structure only \
                     (part of the body of type {}; Open this type in OHL-Structure…)",
                    t.id
                )),
                handle: Handle::Joint(j),
                local: local(global),
                global,
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
/// axes), with what it does in words. A point or wire point from the
/// structure's type becomes the structure's own (the words say so).
pub fn command(
    model: &Model,
    id: &str,
    handle: &Handle,
    at: [f64; 3],
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
    let global = to_global(resolved.position, resolved.orientation_deg, at);
    let mut own: Vec<String> = Vec::new();
    // What the strings hanging from a dragged point did, in words.
    let mut notes = String::new();
    match handle {
        Handle::Point(label) => {
            let r = resolved
                .point(label)
                .ok_or_else(|| format!("{id} has no point {label}"))?;
            match s.points.iter_mut().find(|p| &p.label == label) {
                Some(p) => p.position = global,
                None if s.structure_type.is_some() => {
                    s.points.push(StructurePoint {
                        label: label.clone(),
                        position: global,
                        description: r.description.clone(),
                        ..Default::default()
                    });
                    own.push(format!("point {label}"));
                }
                None => return Err(format!("{id} has no point {label}")),
            }
            let from = r.position;
            let shift = [0, 1, 2].map(|k| global[k] - from[k]);
            let local_from = to_local(resolved.position, resolved.orientation_deg, from);
            let shift_local = [0, 1, 2].map(|k| at[k] - local_from[k]);
            notes = follow(
                model,
                id,
                resolved,
                &mut s,
                label,
                shift,
                shift_local,
                &mut own,
            )?
            .describe();
        }
        Handle::Wire(label) => {
            let from = resolved
                .wire(label)
                .ok_or_else(|| format!("{id} has no wire point {label}"))?
                .position;
            // Its string window: the wire point placed there, what hangs
            // below along; put as the window puts it.
            let mut form = StringForm::of_wire(model, id, label)
                .ok_or_else(|| format!("{id} has no wire point {label}"))?;
            let before: Vec<(String, [f64; 3])> =
                form.wires.iter().map(|w| (w.label.clone(), w.at)).collect();
            form.place_wire(label, at);
            // The points are not touched (only the strings and wire
            // points of the window).
            form.points.clear();
            let wires_before: Vec<String> = s.wires.iter().map(|w| w.label.clone()).collect();
            let strings_before: Vec<String> = s.strings.iter().map(|x| x.label.clone()).collect();
            for w in &form.wires {
                if !wires_before.contains(&w.label) && s.structure_type.is_some() {
                    own.push(format!("wire point {}", w.label));
                }
            }
            for x in &form.strings {
                if !strings_before.contains(&x.label) && s.structure_type.is_some() {
                    own.push(format!("string {}", x.label));
                }
            }
            form.apply_to(&mut s, Some(resolved));
            // Exact positions: the dragged wire point where it was put,
            // those below moved by as much, the others as they were (not
            // turned into the structure's axes and back).
            let shift = [0, 1, 2].map(|k| global[k] - from[k]);
            for w in s.wires.iter_mut() {
                let Some((_, was)) = before.iter().find(|b| b.0 == w.label) else {
                    continue;
                };
                let Some(r) = resolved.wire(&w.label) else {
                    continue;
                };
                let now = form.wires.iter().find(|x| x.label == w.label).map(|x| x.at);
                w.position = if &w.label == label {
                    global
                } else if now.is_some_and(|n| n != *was) {
                    [0, 1, 2].map(|k| r.position[k] + shift[k])
                } else {
                    r.position
                };
            }
            // In the order they were in (new ones last).
            let rank =
                |list: &[String], l: &str| list.iter().position(|x| x == l).unwrap_or(usize::MAX);
            s.wires.sort_by_key(|w| rank(&wires_before, &w.label));
            s.strings.sort_by_key(|x| rank(&strings_before, &x.label));
        }
        Handle::Joint(j) => {
            return Err(format!(
                "joint {j} is not dragged here: joints are edited in OHL-Structure only"
            ));
        }
        Handle::Member(m) => return Err(format!("member {m}: move its joints")),
    }
    let mut what = format!(
        "moved {} of {id} to x {:.3}, y {:.3}, z {:.3} m (in its axes)",
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
    what.push_str(&notes);
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

/// The wire points that move with point `label` of `resolved` (the
/// structure as resolved): those whose strings all hang from it, or from
/// wire points moving with it (what hangs below them), in its order.
pub fn moving_with(resolved: &Structure, label: &str) -> Vec<String> {
    let mut moved: Vec<String> = Vec::new();
    loop {
        let next = resolved.wires.iter().find(|w| {
            !moved.contains(&w.label) && {
                let mut on = resolved.strings_to(&w.label).peekable();
                on.peek().is_some() && on.all(|x| x.top == label || moved.contains(&x.top))
            }
        });
        match next {
            Some(w) => moved.push(w.label.clone()),
            None => break,
        }
    }
    resolved
        .wires
        .iter()
        .filter(|w| moved.contains(&w.label))
        .map(|w| w.label.clone())
        .collect()
}

/// What happened to the wire points hanging from a dragged point.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hanging {
    /// The wire points moved along.
    pub moved: Vec<String>,
    /// The wire points also held from elsewhere (a V): not moved.
    pub stays: Vec<String>,
    /// The strings changing length (from what moved to what did not).
    pub longer: Vec<String>,
}

impl Hanging {
    /// In words, for the status bar (empty: nothing hangs from it).
    pub fn describe(&self) -> String {
        let s = |n: usize| if n > 1 { "s" } else { "" };
        let mut out = String::new();
        if !self.moved.is_empty() {
            out.push_str(&format!(
                "; wire point{} {} moved along (their strings keep their length and direction)",
                s(self.moved.len()),
                self.moved.join(", ")
            ));
        }
        if !self.stays.is_empty() {
            out.push_str(&format!(
                "; wire point{} {} also held by a string from elsewhere: not moved, \
                 string{} {} change{} length",
                s(self.stays.len()),
                self.stays.join(", "),
                s(self.longer.len()),
                self.longer.join(", "),
                if self.longer.len() > 1 { "" } else { "s" },
            ));
        }
        out
    }
}

/// Point `label` of structure `id` moved by `shift` (project axes;
/// `shift_local` in the structure's): `s` (as stored, the point already
/// moved) takes the wire points hanging from it along — given by their
/// length and direction or by their own position alike, their strings
/// keeping their length and direction — put as their string windows put
/// them (those of the type becoming the structure's own, named in
/// `own`). A wire point also held by a string from elsewhere (a V) stays
/// where it is: its strings from what moved change length.
#[allow(clippy::too_many_arguments)]
fn follow(
    model: &Model,
    id: &str,
    resolved: &Structure,
    s: &mut Structure,
    label: &str,
    shift: [f64; 3],
    shift_local: [f64; 3],
    own: &mut Vec<String>,
) -> Result<Hanging, String> {
    let moved = moving_with(resolved, label);
    let moves = |l: &str| l == label || moved.iter().any(|m| m == l);
    let longer: Vec<String> = resolved
        .strings
        .iter()
        .filter(|x| moves(&x.top) && !moves(&x.wire))
        .map(|x| x.label.clone())
        .collect();
    let mut stays: Vec<String> = Vec::new();
    for x in resolved
        .strings
        .iter()
        .filter(|x| longer.contains(&x.label))
    {
        if !stays.contains(&x.wire) {
            stays.push(x.wire.clone());
        }
    }
    let wires_before: Vec<String> = s.wires.iter().map(|w| w.label.clone()).collect();
    let strings_before: Vec<String> = s.strings.iter().map(|x| x.label.clone()).collect();
    // Each string window holding a wire point that moves, put as that
    // window puts it: the moving wire points shifted, the others where
    // they are.
    let mut put_wires: Vec<String> = Vec::new();
    let mut put_strings: Vec<String> = Vec::new();
    for w in &moved {
        if put_wires.contains(w) {
            continue;
        }
        let mut form = StringForm::of_wire(model, id, w)
            .ok_or_else(|| format!("{id} has no wire point {w}"))?;
        for row in form.wires.iter_mut() {
            if moved.contains(&row.label) {
                row.at = [0, 1, 2].map(|k| row.at[k] + shift_local[k]);
            }
        }
        // The points are not touched (the dragged one is put already).
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
    // Exact positions: those moving shifted, the others as they were
    // (not turned into the structure's axes and back).
    for w in s.wires.iter_mut().filter(|w| put_wires.contains(&w.label)) {
        if let Some(r) = resolved.wire(&w.label) {
            w.position = if moved.contains(&w.label) {
                [0, 1, 2].map(|k| r.position[k] + shift[k])
            } else {
                r.position
            };
        }
    }
    for x in s.strings.iter_mut() {
        if longer.contains(&x.label) {
            // Its length and direction taken from where its ends are now.
            x.length = 0.0;
            x.angle_deg = 0.0;
            x.direction_deg = 0.0;
        } else if put_strings.contains(&x.label)
            && let Some(r) = resolved.string(&x.label)
        {
            // Both ends moved alike (or neither): as it was.
            x.length = r.length;
            x.angle_deg = r.angle_deg;
            x.direction_deg = r.direction_deg;
        }
    }
    // In the order they were in (new ones last).
    let rank = |list: &[String], l: &str| list.iter().position(|x| x == l).unwrap_or(usize::MAX);
    s.wires.sort_by_key(|w| rank(&wires_before, &w.label));
    s.strings.sort_by_key(|x| rank(&strings_before, &x.label));
    Ok(Hanging {
        moved,
        stays,
        longer,
    })
}
