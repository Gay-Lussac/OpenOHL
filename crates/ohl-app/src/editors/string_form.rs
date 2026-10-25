// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The strings window: the strings holding a wire point, their command,
//! the window drawn.

use super::*;

impl StringForm {
    /// A new I-string on structure `id`: the next free set, hanging 3 m
    /// from a point of the structure (the first no string hangs from, else
    /// the first; a new point only on a structure without points, 20 m up).
    pub fn new_on(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        let set = s.sets().max().unwrap_or(0) + 1;
        let local = |p| to_local(s.position, s.orientation_deg, p);
        let top = s
            .points
            .iter()
            .find(|p| !s.strings.iter().any(|x| x.top == p.label))
            .or(s.points.first())
            .map_or([0.0, 0.0, 20.0], |p| local(p.position));
        let mut form = Self {
            new: true,
            structure: id.to_owned(),
            set,
            points: s
                .points
                .iter()
                .map(|p| PointRow {
                    description: p.description.clone(),
                    label: p.label.clone(),
                    at: to_local(s.position, s.orientation_deg, p.position),
                })
                .collect(),
            wires: Vec::new(),
            strings: Vec::new(),
            opened: (Vec::new(), Vec::new()),
            overrides_type: false,
            note: None,
            joints: Default::default(),
        };
        form.preset(Preset::I, top, model);
        Some(form)
    }

    /// A new string hanging from point `label` of structure `id`: a
    /// structure point (a new set, its wire point 3 m straight down), or a
    /// wire point (its strings' window, the new string added below it with
    /// the next free set: a TAK).
    pub fn hanging_from(model: &Model, id: &str, label: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        let next_set = s.sets().max().unwrap_or(0) + 1;
        let mut form = if s.wire(label).is_some() {
            Self::of_wire(model, id, label)?
        } else {
            s.point(label)?;
            let mut form = Self::new_on(model, id)?;
            form.wires.clear();
            form.strings.clear();
            form.points.retain(|p| s.point(&p.label).is_some());
            form
        };
        let top = form
            .points
            .iter()
            .find(|p| p.label == label)
            .map(|p| p.at)
            .or_else(|| form.wires.iter().find(|w| w.label == label).map(|w| w.at))?;
        let set = form
            .wires
            .iter()
            .filter_map(|w| w.set)
            .fold(next_set, |a, b| a.max(b + 1));
        if form.new {
            form.set = set;
        }
        let wire = form.free_label(model, &format!("W{set}"));
        form.wires.push(WireRow {
            description: Default::default(),
            label: wire.clone(),
            at: from_polar(top, 0.0, 0.0, 3.0),
            set: Some(set),
            weight: 0.0,
            dead_end: false,
            dead_end_was: false,
        });
        let string = form.free_label(model, &format!("S{set}"));
        form.strings.push(StringRow {
            label: string,
            top: label.to_owned(),
            wire,
            ..Default::default()
        });
        Some(form)
    }

    /// The strings holding wire point `set` of structure `id` (as resolved).
    pub fn of(model: &Model, id: &str, set: u32) -> Option<Self> {
        let s = model.line.structure(id)?;
        Self::of_wire(model, id, &s.wire_at_set(set)?.label)
    }

    /// The strings holding wire point `wire` of structure `id` and those
    /// joined to them (as resolved); its set, else one among them, else 0
    /// (a jumper's helping string: no wire ends there).
    pub fn of_wire(model: &Model, id: &str, wire: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        let c = s.component(wire);
        let set = s
            .wire(wire)
            .and_then(|w| w.set)
            .or_else(|| c.wires.iter().find_map(|&i| s.wires[i].set))
            .unwrap_or(0);
        let local = |p| to_local(s.position, s.orientation_deg, p);
        let wires: Vec<WireRow> = c
            .wires
            .iter()
            .map(|&i| {
                let w = &s.wires[i];
                WireRow {
                    description: w.description.clone(),
                    label: w.label.clone(),
                    at: local(w.position),
                    set: w.set,
                    weight: w.weight,
                    dead_end: model.line.is_dead_end(s, &w.label),
                    dead_end_was: model.line.is_dead_end(s, &w.label),
                }
            })
            .collect();
        let strings: Vec<StringRow> = c
            .strings
            .iter()
            .map(|&i| {
                let x = &s.strings[i];
                StringRow {
                    description: x.description.clone(),
                    label: x.label.clone(),
                    top: x.top.clone(),
                    wire: x.wire.clone(),
                    insulator: x.insulator.clone(),
                    weight: x.weight,
                    compression: x.compression,
                    wind_area: x.wind_area,
                    stiffness: x.stiffness,
                    strain: x.strain,
                    limits: own_limits(model, id, x),
                }
            })
            .collect();
        let overrides_type = s.structure_type.is_some()
            && model.stored_line().is_some_and(|l| {
                l.structure(id).is_some_and(|x| {
                    x.wires
                        .iter()
                        .any(|w| wires.iter().any(|r| r.label == w.label))
                })
            });
        Some(Self {
            new: false,
            structure: id.to_owned(),
            set,
            points: s
                .points
                .iter()
                .map(|p| PointRow {
                    description: p.description.clone(),
                    label: p.label.clone(),
                    at: local(p.position),
                })
                .collect(),
            opened: (
                strings.iter().map(|x| x.label.clone()).collect(),
                wires.iter().map(|w| w.label.clone()).collect(),
            ),
            wires,
            strings,
            overrides_type,
            note: None,
            joints: Default::default(),
        })
    }

    /// The strings holding the wire point of string `label` of structure
    /// `id` (one with a set among them).
    pub fn of_string(model: &Model, id: &str, label: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        Self::of_wire(model, id, &s.string(label)?.wire)
    }

    /// A label like `stem` not taken on the structure nor in the form.
    pub(super) fn free_label(&self, model: &Model, stem: &str) -> String {
        let s = model.line.structure(&self.structure);
        let taken = |l: &str| {
            self.points.iter().any(|p| p.label == l)
                || self.wires.iter().any(|w| w.label == l)
                || self.strings.iter().any(|x| x.label == l)
                || s.is_some_and(|s| s.node(l).is_some() || s.string(l).is_some())
        };
        if !taken(stem) {
            return stem.to_owned();
        }
        (2..)
            .map(|n| format!("{stem}.{n}"))
            .find(|l| !taken(l))
            .expect("a free label")
    }

    /// Replaces the strings and wire points by a preset hanging from `top`
    /// (local): new points, wire points and strings, the set(s) from the
    /// form's.
    pub fn preset(&mut self, preset: Preset, top: [f64; 3], model: &Model) {
        let set = self.set;
        // Points the earlier preset added (not on the structure) go.
        let on_structure = model.line.structure(&self.structure);
        self.points
            .retain(|p| on_structure.is_some_and(|s| s.point(&p.label).is_some()));
        self.wires.clear();
        self.strings.clear();
        let at = |d: [f64; 3]| [top[0] + d[0], top[1] + d[1], top[2] + d[2]];
        // A point of the structure already there (within 5 cm) is used;
        // a new one only where there is none.
        let point = |form: &mut Self, stem: &str, d: [f64; 3]| {
            let p = at(d);
            if let Some(q) = form
                .points
                .iter()
                .find(|q| (0..3).all(|k| (q.at[k] - p[k]).abs() < 0.05))
            {
                return q.label.clone();
            }
            let label = form.free_label(model, stem);
            form.points.push(PointRow {
                description: Default::default(),
                label: label.clone(),
                at: at(d),
            });
            label
        };
        let wire = |form: &mut Self, stem: &str, d: [f64; 3], set: Option<u32>| {
            let label = form.free_label(model, stem);
            form.wires.push(WireRow {
                description: Default::default(),
                label: label.clone(),
                at: at(d),
                set,
                weight: 0.0,
                dead_end: false,
                dead_end_was: false,
            });
            label
        };
        let string = |form: &mut Self, stem: &str, top: &str, wire: &str, strain: bool| {
            let label = form.free_label(model, stem);
            form.strings.push(StringRow {
                label,
                top: top.to_owned(),
                wire: wire.to_owned(),
                strain,
                ..Default::default()
            });
        };
        match preset {
            Preset::I => {
                let a = point(self, &format!("A{set}"), [0.0; 3]);
                let w = wire(self, &format!("W{set}"), [0.0, 0.0, -3.0], Some(set));
                string(self, &format!("S{set}"), &a, &w, false);
            }
            Preset::V | Preset::VAndString => {
                let l = point(self, &format!("A{set}-L"), [-2.0, 0.0, 0.0]);
                let r = point(self, &format!("A{set}-R"), [2.0, 0.0, 0.0]);
                let tip = if preset == Preset::V {
                    wire(self, &format!("W{set}"), [0.0, 0.0, -3.0], Some(set))
                } else {
                    wire(self, &format!("V{set}"), [0.0, 0.0, -3.0], None)
                };
                string(self, &format!("S{set}-L"), &l, &tip, false);
                string(self, &format!("S{set}-R"), &r, &tip, false);
                if preset == Preset::VAndString {
                    let w = wire(self, &format!("W{set}"), [0.0, 0.0, -3.5], Some(set));
                    string(self, &format!("S{set}"), &tip, &w, false);
                }
            }
            Preset::Tak => {
                let a = point(self, &format!("A{set}"), [0.0; 3]);
                let n = wire(self, &format!("N{set}"), [0.0, 0.0, -3.0], None);
                let b = wire(self, &format!("W{set}"), [0.0, -3.0, -3.0], Some(set));
                let f = wire(
                    self,
                    &format!("W{}", set + 1),
                    [0.0, 3.0, -3.0],
                    Some(set + 1),
                );
                string(self, &format!("S{set}"), &a, &n, false);
                string(self, &format!("S{set}-B"), &n, &b, true);
                string(self, &format!("S{set}-F"), &n, &f, true);
            }
        }
        // Its strain strings' wire points are dead ends.
        for w in &mut self.wires {
            w.dead_end = self.strings.iter().any(|x| x.wire == w.label && x.strain);
        }
    }

    /// String `k` takes an insulator assembly of the library: its values;
    /// the wire point it makes moves to the assembly's length (same
    /// angles), a strain clamp where the assembly is a tension one.
    pub fn take_from_library(&mut self, k: usize, a: &LibraryString) {
        let Some(x) = self.strings.get_mut(k) else {
            return;
        };
        x.insulator = Some(a.id.clone());
        (x.weight, x.wind_area, x.stiffness) = (a.weight, a.wind_area, Some(a.stiffness));
        let (top, wire) = (x.top.clone(), x.wire.clone());
        let owns = self.strings.iter().position(|y| y.wire == wire) == Some(k);
        let at = self
            .points
            .iter()
            .find(|p| p.label == top)
            .map(|p| p.at)
            .or_else(|| self.wires.iter().find(|w| w.label == top).map(|w| w.at));
        if owns
            && let Some(top) = at
            && let Some(w) = self.wires.iter_mut().find(|w| w.label == wire)
        {
            let (down, dir, _) = polar(top, w.at);
            w.at = from_polar(top, down, dir, a.length);
            if w.set.is_some() {
                w.dead_end = a.strain;
            }
        }
    }

    /// The wire points hanging under wire point `wire` (itself, the wire
    /// points of the strings hanging from it, and so on).
    pub(super) fn chain_below(&self, wire: &str) -> Vec<String> {
        let mut out = vec![wire.to_owned()];
        let mut k = 0;
        while k < out.len() {
            for x in &self.strings {
                if x.top == out[k] && !out.contains(&x.wire) {
                    out.push(x.wire.clone());
                }
            }
            k += 1;
        }
        out
    }

    /// The wire points string `k` may end at (besides a new one of its
    /// own): the form's and the structure's other wire points, but the one
    /// it ends at, the one it hangs from and those hanging below it (a
    /// loop).
    pub fn end_choices(&self, model: &Model, k: usize) -> Vec<String> {
        let Some(x) = self.strings.get(k) else {
            return Vec::new();
        };
        let below = self.chain_below(&x.wire);
        let mut out: Vec<String> = self.wires.iter().map(|w| w.label.clone()).collect();
        if let Some(s) = model.line.structure(&self.structure) {
            for w in &s.wires {
                if !out.contains(&w.label) {
                    out.push(w.label.clone());
                }
            }
        }
        out.retain(|w| {
            *w != x.wire
                && *w != x.top
                && !below.contains(w)
                && !(self.wires.iter().any(|r| r.label == *w)
                    && self.chain_below(w).contains(&x.top))
        });
        out
    }

    /// The wire points string `k` may hang from: the form's and the
    /// structure's others (with or without a set: a TAK's tension strings
    /// hang from a suspension string's wire point), but its own and those
    /// hanging below it (a loop).
    pub fn top_choices(&self, model: &Model, k: usize) -> Vec<String> {
        let Some(x) = self.strings.get(k) else {
            return Vec::new();
        };
        let below = self.chain_below(&x.wire);
        let mut out: Vec<String> = self.wires.iter().map(|w| w.label.clone()).collect();
        if let Some(s) = model.line.structure(&self.structure) {
            for w in &s.wires {
                if !out.contains(&w.label) {
                    out.push(w.label.clone());
                }
            }
        }
        out.retain(|w| !below.contains(w));
        out
    }

    /// String `k` hangs from wire point `top` of the form or the structure
    /// ([`Self::move_top`]); one outside the form is taken in first with
    /// what moves with it.
    pub fn hang_from(&mut self, model: &Model, k: usize, top: &str) -> Result<(), String> {
        let in_form =
            self.points.iter().any(|p| p.label == top) || self.wires.iter().any(|w| w.label == top);
        if !in_form {
            self.absorb(model, top);
        }
        self.move_top(k, top)
    }

    /// Takes wire point `wire` of the structure (as resolved) into the
    /// form with the strings and wire points joined to it, to be edited
    /// and applied with the rest.
    pub(super) fn absorb(&mut self, model: &Model, wire: &str) {
        let Some(s) = model.line.structure(&self.structure) else {
            return;
        };
        let c = s.component(wire);
        let local = |p| to_local(s.position, s.orientation_deg, p);
        for &i in &c.wires {
            let w = &s.wires[i];
            if self.wires.iter().any(|r| r.label == w.label) {
                continue;
            }
            let dead_end = model.line.is_dead_end(s, &w.label);
            self.wires.push(WireRow {
                description: w.description.clone(),
                label: w.label.clone(),
                at: local(w.position),
                set: w.set,
                weight: w.weight,
                dead_end,
                dead_end_was: dead_end,
            });
            self.opened.1.push(w.label.clone());
        }
        for &i in &c.strings {
            let x = &s.strings[i];
            if self.strings.iter().any(|y| y.label == x.label) {
                continue;
            }
            self.strings.push(StringRow {
                description: x.description.clone(),
                label: x.label.clone(),
                top: x.top.clone(),
                wire: x.wire.clone(),
                insulator: x.insulator.clone(),
                weight: x.weight,
                compression: x.compression,
                wind_area: x.wind_area,
                stiffness: x.stiffness,
                strain: x.strain,
                limits: own_limits(model, &self.structure, x),
            });
            self.opened.0.push(x.label.clone());
        }
    }

    /// String `k` ends at wire point `wire` (of the form or the structure)
    /// instead, keeping its length: where one string ends there already,
    /// the two make a V and the point goes where both lengths meet; where
    /// none does, it goes to the string's end; what hangs below moves
    /// along. The wire point it alone held goes: what hung from it hangs
    /// from the new end (lengths kept), its set going along where the new
    /// end has none. Refused (nothing changes) where the legs cannot meet,
    /// where two strings end there already, where it would hang below
    /// itself, or where both wire points have a set.
    pub fn end_at(&mut self, model: &Model, k: usize, wire: &str) -> Result<(), String> {
        let at = |f: &Self, l: &str| {
            f.points
                .iter()
                .find(|p| p.label == l)
                .map(|p| p.at)
                .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
        };
        let dist =
            |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
        let Some(x) = self.strings.get(k).cloned() else {
            return Ok(());
        };
        if !self.end_choices(model, k).iter().any(|w| w == wire) {
            return Err(format!(
                "{} cannot end at {wire}: it hangs from it, ends there already or {wire} hangs below it",
                x.label
            ));
        }
        let before = self.clone();
        let result = (|| {
            if !self.wires.iter().any(|w| w.label == wire) {
                self.absorb(model, wire);
            }
            // Its wire point now: held by it alone, what hangs from it is
            // hung from the new end instead (moving along, lengths kept),
            // its set going along where the new end has none.
            let alone = !self
                .strings
                .iter()
                .enumerate()
                .any(|(i, y)| i != k && y.wire == x.wire);
            let hanging: Vec<String> = self
                .strings
                .iter()
                .filter(|y| y.top == x.wire)
                .map(|y| y.label.clone())
                .collect();
            let old_set = self
                .wires
                .iter()
                .find(|w| w.label == x.wire)
                .and_then(|w| w.set);
            let new_set = self
                .wires
                .iter()
                .find(|w| w.label == wire)
                .and_then(|w| w.set);
            if alone && let (Some(a), Some(b)) = (old_set, new_set) {
                return Err(format!(
                    "{} cannot end at {wire}: {} (set {a}) would go and {wire} has a set already \
                     ({b}) — take one of the sets away first",
                    x.label, x.wire
                ));
            }
            let (Some(top), Some(old), Some(w)) =
                (at(self, &x.top), at(self, &x.wire), at(self, wire))
            else {
                self.strings[k].wire = wire.to_owned();
                return Ok(());
            };
            let length = dist(top, old);
            let legs: Vec<usize> = (0..self.strings.len())
                .filter(|&i| i != k && self.strings[i].wire == wire)
                .collect();
            let target = match legs[..] {
                [] => {
                    let l = dist(top, w);
                    let u = if l > 1e-9 {
                        [0, 1, 2].map(|i| (w[i] - top[i]) / l)
                    } else {
                        [0.0, 0.0, -1.0]
                    };
                    [0, 1, 2].map(|i| top[i] + length * u[i])
                }
                [o] => {
                    let other = &self.strings[o];
                    let ot = at(self, &other.top).unwrap_or(w);
                    let lo = dist(ot, w);
                    let apart = dist(top, ot);
                    ohl_structures::structures::v_meeting(top, length, ot, lo, w).ok_or_else(|| {
                        format!(
                            "{} cannot end at {wire}: it is {length:.3} m long and {} {lo:.3} m, but \
                             their tops ({} and {}) are {apart:.3} m apart — the two legs cannot meet",
                            x.label, other.label, x.top, other.top
                        )
                    })?
                }
                _ => {
                    return Err(format!(
                        "{} cannot end at {wire}: two strings meet there already",
                        x.label
                    ));
                }
            };
            self.place_wire(wire, target);
            if alone {
                // What hangs from the old end: below the new one, as it was
                // below the old (moved by as much).
                let d = [0, 1, 2].map(|i| target[i] - old[i]);
                let below: Vec<String> = self
                    .chain_below(&x.wire)
                    .into_iter()
                    .filter(|w| *w != x.wire)
                    .collect();
                for r in self.wires.iter_mut().filter(|r| below.contains(&r.label)) {
                    r.at = [0, 1, 2].map(|i| r.at[i] + d[i]);
                }
                for y in self
                    .strings
                    .iter_mut()
                    .filter(|y| hanging.contains(&y.label))
                {
                    y.top = wire.to_owned();
                }
                if new_set.is_none()
                    && let Some(set) = old_set
                    && let Some(w) = self.wires.iter_mut().find(|w| w.label == wire)
                {
                    w.set = Some(set);
                }
            }
            self.strings[k].wire = wire.to_owned();
            self.drop_unused_wires();
            Ok(())
        })();
        if result.is_err() {
            *self = before;
        }
        result
    }

    /// Moves wire point `wire` to `at`, and what hangs below it along
    /// with it (their strings keep their lengths).
    pub fn place_wire(&mut self, wire: &str, at: [f64; 3]) {
        let Some(w) = self.wires.iter().find(|w| w.label == wire).map(|w| w.at) else {
            return;
        };
        let d = [0, 1, 2].map(|i| at[i] - w[i]);
        let below = self.chain_below(wire);
        for r in self.wires.iter_mut().filter(|r| below.contains(&r.label)) {
            r.at = [0, 1, 2].map(|i| r.at[i] + d[i]);
        }
    }

    /// String `k` takes length `length`, the other strings keeping theirs:
    /// its wire point moves along it (an I), or where it and the other leg
    /// meet (a V); what hangs below moves along. Where no such point
    /// exists, nothing changes and the reason is returned.
    pub fn set_length(&mut self, k: usize, length: f64) -> Result<(), String> {
        let at = |f: &Self, l: &str| {
            f.points
                .iter()
                .find(|p| p.label == l)
                .map(|p| p.at)
                .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
        };
        let dist =
            |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
        let Some(x) = self.strings.get(k).cloned() else {
            return Ok(());
        };
        if !(length.is_finite() && length > 0.0) {
            return Err(format!("{}: a length must be more than 0", x.label));
        }
        let (Some(top), Some(w)) = (at(self, &x.top), at(self, &x.wire)) else {
            return Ok(());
        };
        let legs: Vec<usize> = (0..self.strings.len())
            .filter(|&i| i != k && self.strings[i].wire == x.wire)
            .collect();
        let target = match legs.first() {
            None => {
                let l = dist(top, w);
                let u = if l > 1e-9 {
                    [0, 1, 2].map(|i| (w[i] - top[i]) / l)
                } else {
                    [0.0, 0.0, -1.0]
                };
                [0, 1, 2].map(|i| top[i] + length * u[i])
            }
            Some(&o) => {
                let other = &self.strings[o];
                let ot = at(self, &other.top).unwrap_or(w);
                let lo = dist(ot, w);
                let apart = dist(top, ot);
                let p = ohl_structures::structures::v_meeting(top, length, ot, lo, w).ok_or_else(|| {
                    format!(
                        "{} cannot be {length:.3} m long: with {} at {lo:.3} m the two legs of the V \
                         cannot meet — their tops ({} and {}) are {apart:.3} m apart, so the legs \
                         must be at least {apart:.3} m together and differ by at most {apart:.3} m",
                        x.label, other.label, x.top, other.top
                    )
                })?;
                for &i in &legs[1..] {
                    let y = &self.strings[i];
                    if let Some(yt) = at(self, &y.top)
                        && (dist(yt, p) - dist(yt, w)).abs() > 1e-3
                    {
                        return Err(format!(
                            "{} cannot be {length:.3} m long: {} would change its length",
                            x.label, y.label
                        ));
                    }
                }
                p
            }
        };
        self.place_wire(&x.wire, target);
        Ok(())
    }

    /// String `k` hangs from `top` instead, keeping its length: its wire
    /// point moves with it (and what hangs below, unchanged). A V's leg
    /// keeps its length and the other leg's: its wire point goes where
    /// both meet; where they cannot, nothing changes and the reason is
    /// returned.
    pub fn move_top(&mut self, k: usize, top: &str) -> Result<(), String> {
        let at = |f: &Self, l: &str| {
            f.points
                .iter()
                .find(|p| p.label == l)
                .map(|p| p.at)
                .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
        };
        let dist =
            |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
        let Some(x) = self.strings.get(k).cloned() else {
            return Ok(());
        };
        let below = self.chain_below(&x.wire);
        if below.iter().any(|w| w == top) {
            return Err(format!(
                "{} cannot hang from {top}: {top} hangs below it",
                x.label
            ));
        }
        let (Some(old), Some(new), Some(w)) = (at(self, &x.top), at(self, top), at(self, &x.wire))
        else {
            self.strings[k].top = top.to_owned();
            return Ok(());
        };
        let length = dist(old, w);
        let legs: Vec<usize> = (0..self.strings.len())
            .filter(|&i| i != k && self.strings[i].wire == x.wire)
            .collect();
        let target = match legs.first() {
            None => [0, 1, 2].map(|i| w[i] + new[i] - old[i]),
            Some(&o) => {
                let other = &self.strings[o];
                let ot = at(self, &other.top).unwrap_or(w);
                let lo = dist(ot, w);
                let p = ohl_structures::structures::v_meeting(new, length, ot, lo, w).ok_or_else(
                    || {
                        format!(
                            "{} cannot hang from {top}: it is {length:.2} m long and {} {lo:.2} m, \
                         but {top} is {:.2} m from {} — the two legs of the V cannot meet",
                            x.label,
                            other.label,
                            dist(new, ot),
                            other.top
                        )
                    },
                )?;
                // A third leg (or more) must keep its length too.
                for &i in &legs[1..] {
                    let y = &self.strings[i];
                    if let Some(yt) = at(self, &y.top)
                        && (dist(yt, p) - dist(yt, w)).abs() > 1e-3
                    {
                        return Err(format!(
                            "{} cannot hang from {top}: {} would change its length",
                            x.label, y.label
                        ));
                    }
                }
                p
            }
        };
        self.place_wire(&x.wire, target);
        self.strings[k].top = top.to_owned();
        Ok(())
    }

    /// Takes away the wire points no string ends at any more, with the
    /// strings hanging from them (and so on).
    pub fn drop_unused_wires(&mut self) {
        loop {
            let unused: Vec<String> = self
                .wires
                .iter()
                .filter(|w| !self.strings.iter().any(|x| x.wire == w.label))
                .map(|w| w.label.clone())
                .collect();
            if unused.is_empty() {
                return;
            }
            self.wires.retain(|w| !unused.contains(&w.label));
            self.strings.retain(|x| !unused.contains(&x.top));
        }
    }

    /// A string's length, m (from its rows' positions).
    pub fn length(&self, x: &StringRow) -> Option<f64> {
        let at = |l: &str| {
            self.points
                .iter()
                .find(|p| p.label == l)
                .map(|p| p.at)
                .or_else(|| self.wires.iter().find(|w| w.label == l).map(|w| w.at))
        };
        let (a, b) = (at(&x.top)?, at(&x.wire)?);
        Some((0..3).map(|k| (b[k] - a[k]).powi(2)).sum::<f64>().sqrt())
    }

    /// The form put on structure `s` (as stored): its strings and wire
    /// points as the structure's own (replacing those it was opened with;
    /// for a typed structure, over the type's), its points where new or
    /// moved from `resolved` (the structure as resolved).
    pub fn apply_to(&self, s: &mut Structure, resolved: Option<&Structure>) {
        let global = |p| to_global(s.position, s.orientation_deg, p);
        for p in &self.points {
            let position = global(p.at);
            let same = resolved
                .and_then(|r| r.point(&p.label))
                .is_some_and(|q| (0..3).all(|k| (q.position[k] - position[k]).abs() < 1e-6));
            if same && s.point(&p.label).is_none() {
                continue;
            }
            // A new point of a typed structure: linked to the attach joint
            // chosen, its distance from it the offset; a linked point moved
            // stays on its joint, its offset following (owner decision
            // 2026-10-21).
            let was = resolved.and_then(|r| r.point(&p.label));
            let (joint, offset) = match (self.joints.get(&p.label), was) {
                (Some((j, at)), _) if s.structure_type.is_some() => (
                    Some(j.clone()),
                    [0, 1, 2].map(|k| {
                        let v = p.at[k] - at[k];
                        if v.abs() < ohl_structures::SAME_JOINT {
                            0.0
                        } else {
                            v
                        }
                    }),
                ),
                (_, Some(q)) if s.structure_type.is_some() && q.is_linked() => {
                    let m = ohl_line::links::moved(q, s.orientation_deg, position);
                    (m.joint, m.offset)
                }
                _ => (None, [0.0; 3]),
            };
            s.points.retain(|q| q.label != p.label);
            s.points.push(StructurePoint {
                description: p.description.clone(),
                label: p.label.clone(),
                position,
                joint,
                offset,
                unlinked: false,
            });
        }
        // The strings' own volumes (not edited here: the String library
        // and the line file) kept.
        let own_shapes: std::collections::BTreeMap<String, Vec<ohl_structures::StringShape>> = s
            .strings
            .iter()
            .filter(|x| !x.shapes.is_empty())
            .map(|x| (x.label.clone(), x.shapes.clone()))
            .collect();
        let (strings, wires) = &self.opened;
        s.strings.retain(|x| !strings.contains(&x.label));
        s.wires.retain(|w| !wires.contains(&w.label));
        for w in &self.wires {
            s.wires.retain(|v| v.label != w.label);
            s.wires.push(WirePoint {
                description: w.description.clone(),
                label: w.label.clone(),
                position: global(w.at),
                set: w.set,
                weight: w.weight,
            });
        }
        for x in &self.strings {
            // A string to a set: a tension string where the set is a dead
            // end, else a suspension one.
            let strain = match self.wires.iter().find(|w| w.label == x.wire) {
                Some(w) if w.set.is_some() && w.dead_end != w.dead_end_was => w.dead_end,
                _ => x.strain,
            };
            s.strings.retain(|y| y.label != x.label);
            s.strings.push(InsulatorString {
                description: x.description.clone(),
                label: x.label.clone(),
                top: x.top.clone(),
                wire: x.wire.clone(),
                insulator: x.insulator.clone(),
                weight: x.weight,
                compression: x.compression,
                wind_area: x.wind_area,
                stiffness: x.stiffness,
                strain,
                max_load: x.limits.max_load,
                max_swing_deg: x.limits.max_swing_deg,
                max_swing_across_deg: x.limits.max_swing_across_deg,
                max_swing_along_deg: x.limits.max_swing_along_deg,
                shapes: own_shapes.get(&x.label).cloned().unwrap_or_default(),
                // From its wire point as placed here.
                ..Default::default()
            });
        }
        if s.structure_type.is_some() {
            let now: Vec<&str> = self
                .strings
                .iter()
                .map(|x| x.label.as_str())
                .chain(self.wires.iter().map(|w| w.label.as_str()))
                .collect();
            s.omit.retain(|l| !now.contains(&l.as_str()));
            s.omit_sets
                .retain(|set| !self.wires.iter().any(|w| w.set == Some(*set)));
            // Those taken away from the type's stay out.
            for l in strings.iter().chain(wires) {
                if !now.contains(&l.as_str()) && !s.omit.contains(l) {
                    s.omit.push(l.clone());
                }
            }
        }
    }

    pub(super) fn command(
        &self,
        stored: Option<&Line>,
        resolved: &Line,
        reset: bool,
    ) -> Result<(String, Command), String> {
        let mut s = stored
            .and_then(|l| l.structure(&self.structure))
            .ok_or_else(|| format!("no structure {}", self.structure))?
            .clone();
        if reset {
            let (strings, wires) = &self.opened;
            s.strings.retain(|x| !strings.contains(&x.label));
            s.wires.retain(|w| !wires.contains(&w.label));
            s.omit
                .retain(|l| !strings.contains(l) && !wires.contains(l));
            s.omit_sets.retain(|set| *set != self.set);
        } else {
            self.apply_to(&mut s, resolved.structure(&self.structure));
        }
        let what = if reset {
            format!("set {} of {} back to its type", self.set, self.structure)
        } else {
            format!(
                "{} the strings of set {} of {}",
                if self.new { "added" } else { "changed" },
                self.set,
                self.structure
            )
        };
        Ok((
            what,
            Command::Put {
                kind: Kind::Structure,
                id: s.id.clone(),
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

/// Returns true when "back to the type" is clicked.
pub(super) fn string_ui(ui: &mut egui::Ui, f: &mut StringForm, model: &Model) -> bool {
    // Directions in plan as the project counts them.
    let ahead = model.line.display.ahead_deg;
    let mut reset = false;
    if f.new {
        ui.horizontal(|ui| {
            ui.label("set");
            ui.add(egui::DragValue::new(&mut f.set).range(1..=9999));
            ui.label("shape");
            let top = f
                .strings
                .first()
                .and_then(|x| f.points.iter().find(|p| p.label == x.top))
                .map_or([0.0, 0.0, 20.0], |p| p.at);
            let mut chosen = None;
            egui::ComboBox::from_id_salt("string-preset")
                .selected_text("replace with…")
                .show_ui(ui, |ui| {
                    for p in Preset::ALL {
                        if ui.selectable_label(false, p.name()).clicked() {
                            chosen = Some(p);
                        }
                    }
                });
            if let Some(p) = chosen {
                f.preset(p, top, model);
            }
        });
    }
    let help = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).small().weak());
    };
    let structure = model.line.structure(&f.structure);
    // Points of the structure strings may hang from (with the new ones of
    // the form), and where each is (local).
    let on_structure: Vec<String> = structure
        .map(|s| s.points.iter().map(|p| p.label.clone()).collect())
        .unwrap_or_default();
    let new_points: Vec<usize> = (0..f.points.len())
        .filter(|&k| !on_structure.contains(&f.points[k].label))
        .collect();
    let point_names: Vec<String> = on_structure
        .iter()
        .cloned()
        .chain(new_points.iter().map(|&k| f.points[k].label.clone()))
        .collect();
    egui::ScrollArea::vertical()
        .max_height(460.0)
        .show(ui, |ui| {
            ui.strong("Strings");
            help(
                ui,
                "A string is one straight insulating member: from the point it hangs from (on the structure, \
                 or the bottom of another string) to its wire point, which it makes: placed from \
                 its top by an angle from straight down, a direction and its length (where it is \
                 before the wire is strung; once strung it is drawn where the calculation puts \
                 it). With a set number a wire is clamped at the wire point (sections and jumpers \
                 end on it); without one, other strings may hang from it. A second leg of a V \
                 ends at another string's wire point.",
            );
            // (name, a V's leg, what the string takes).
            let library: Vec<(String, bool, LibraryString)> = model
                .types
                .as_ref()
                .map(|(_, lib)| {
                    lib.iter()
                        .map(|a| {
                            use ohl_structures::AssemblyKind as K;
                            let v = a.kind == K::VString;
                            let share = if v { 0.5 } else { 1.0 };
                            (
                                a.name.clone(),
                                v,
                                LibraryString {
                                    id: a.id.clone(),
                                    length: a.length.si(),
                                    weight: a.weight.si() * share,
                                    wind_area: a.wind_area.si() * share,
                                    stiffness: a.axial_stiffness.si(),
                                    strain: a.kind == K::Strain,
                                },
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let tops: Vec<Option<[f64; 3]>> = f
                .strings
                .iter()
                .map(|x| {
                    f.points
                        .iter()
                        .find(|p| p.label == x.top)
                        .map(|p| p.at)
                        .or_else(|| f.wires.iter().find(|w| w.label == x.top).map(|w| w.at))
                })
                .collect();
            let lengths: Vec<Option<f64>> = f.strings.iter().map(|x| f.length(x)).collect();
            // The string that makes each wire point: the first to it.
            let owner = |f: &StringForm, wire: &str| f.strings.iter().position(|x| x.wire == wire);
            let mut remove = None;
            for (k, length) in lengths.iter().enumerate() {
                let owns = owner(f, &f.strings[k].wire) == Some(k);
                let others: Vec<String> = f.top_choices(model, k);
                let ends = f.end_choices(model, k);
                let mut ends_at: Option<Option<String>> = None;
                let mut retop: Option<String> = None;
                // A length typed (kept: what it moves follows) or, for a
                // string making its wire point alone, angles typed (its
                // wire point there, what hangs below along).
                let mut want_len: Option<f64> = None;
                let mut want_at: Option<[f64; 3]> = None;
                let mut pick: Option<usize> = None;
                // A wire point renamed: (old, new) for its strings.
                let mut rename: Option<(String, String)> = None;
                // Names its wire point may not take (its own current one
                // aside).
                let wire_now = f.strings[k].wire.clone();
                let structure = model.line.structure(&f.structure);
                let names: Vec<String> = f
                    .points
                    .iter()
                    .map(|p| p.label.clone())
                    .chain(f.wires.iter().map(|w| w.label.clone()))
                    .chain(f.strings.iter().map(|x| x.label.clone()))
                    .chain(structure.into_iter().flat_map(|s| {
                        s.points
                            .iter()
                            .map(|p| p.label.clone())
                            .chain(s.wires.iter().map(|w| w.label.clone()))
                            .chain(s.strings.iter().map(|x| x.label.clone()))
                    }))
                    .filter(|l| *l != wire_now)
                    .collect();
                let clash = |text: &str, own: &str| text != own && names.iter().any(|l| l == text);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    let x = &mut f.strings[k];
                    ui.horizontal(|ui| {
                        ui.label("string");
                        ui.add(egui::TextEdit::singleline(&mut x.label).desired_width(110.0));
                        match length {
                            Some(l) => ui.label(format!("{l:.3} m long")),
                            None => ui.label("—"),
                        };
                        if ui
                            .small_button("×")
                            .on_hover_text("Take this string away, with the wire point it makes")
                            .clicked()
                        {
                            remove = Some(k);
                        }
                    });
                    if !library.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label("library label");
                            egui::ComboBox::from_id_salt(("string-library", k))
                                .selected_text(x.insulator.as_deref().unwrap_or("(given here)"))
                                .height(300.0)
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(x.insulator.is_none(), "(given here)").clicked() {
                                        x.insulator = None;
                                    }
                                    for (n, a) in library.iter().enumerate() {
                                        let text = format!(
                                            "{} — {}, {:.2} m, {:.0} N{}",
                                            a.2.id,
                                            a.0,
                                            a.2.length,
                                            a.2.weight,
                                            if a.2.strain { ", tension" } else if a.1 { ", one of two strings" } else { "" }
                                        );
                                        if ui
                                            .selectable_label(x.insulator.as_deref() == Some(a.2.id.as_str()), text)
                                            .clicked()
                                        {
                                            pick = Some(n);
                                        }
                                    }
                                })
                                .response
                                .on_hover_text(
                                    "Its entry in the string library (Structures › String library): its \
                                     length, weight, wind area and stiffness are taken; a tension assembly \
                                     makes a dead end. Given here: a label is given when applied",
                                );
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("description");
                        ui.add(
                            egui::TextEdit::singleline(&mut x.description)
                                .desired_width(260.0)
                                .hint_text("what it is (free text)"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("hangs from")
                            .on_hover_text("The point of the structure it is attached to");
                        egui::ComboBox::from_id_salt(("string-top", k))
                            .selected_text(&x.top)
                            .height(300.0)
                            .show_ui(ui, |ui| {
                                ui.label(egui::RichText::new("points of the structure").small().weak());
                                for t in &point_names {
                                    if ui.selectable_label(x.top == *t, t).clicked() && x.top != *t {
                                        retop = Some(t.clone());
                                    }
                                }
                                ui.separator();
                                ui.label(
                                    egui::RichText::new(
                                        "the bottom of another string (with or without a set)",
                                    )
                                    .small()
                                    .weak(),
                                );
                                for w in &others {
                                    if ui.selectable_label(x.top == *w, w).clicked() && x.top != *w {
                                        retop = Some(w.clone());
                                    }
                                }
                            });
                        ui.label("ends at").on_hover_text(
                            "Its own wire point (made by it), or another wire point of the structure: \
                             where a string ends already, two strings end at one wire point (both keep their lengths); \
                             strings chain (a string below two)",
                        );
                        let shown = if owns {
                            format!("its own wire point ({})", x.wire)
                        } else {
                            format!("{}'s wire point", x.wire)
                        };
                        egui::ComboBox::from_id_salt(("string-ends", k))
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(owns, "its own wire point").clicked() && !owns {
                                    ends_at = Some(None);
                                }
                                for w in &ends {
                                    if ui.selectable_label(false, format!("the wire point {w}")).clicked() {
                                        ends_at = Some(Some(w.clone()));
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("weight");
                        value(ui, &mut x.weight, 1.0, " N")
                            .on_hover_text("Its own weight, half at each end");
                        ui.label("wind area");
                        value(ui, &mut x.wind_area, 0.01, " m²").on_hover_text(
                            "Area the wind blows on (0: its assembly's, if any); half at each end",
                        );
                        let mut both = x.compression.is_some();
                        if ui
                            .checkbox(&mut both, "may push, up to")
                            .on_hover_text(
                                "It acts both ways (e.g. one of two strings to a wire point, designed to push); beyond this it \
                                 is flagged. Unticked: tension only, slack when pushed",
                            )
                            .changed()
                        {
                            x.compression = both.then_some(1000.0);
                        }
                        if let Some(c) = &mut x.compression {
                            value(ui, c, 10.0, " N");
                            *c = c.max(0.0);
                        }
                    });
                    limits_ui(ui, &mut x.limits);
                    // A V's leg (strings meeting at one wire point): its
                    // length only; where the legs meet follows from both.
                    let legs = f.strings.iter().filter(|y| y.wire == f.strings[k].wire).count();
                    if legs > 1
                        && let (Some(top), Some(length)) = (tops[k], *length)
                    {
                        let mut len = length;
                        ui.horizontal(|ui| {
                            ui.label("length");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut len)
                                        .range(0.01..=50.0)
                                        .speed(0.01)
                                        .max_decimals(3)
                                        .suffix(" m"),
                                )
                                .on_hover_text(
                                    "Its length: the other leg keeps its own; the point where they meet \
                                     moves (and what hangs below it along)",
                                )
                                .changed()
                            {
                                want_len = Some(len);
                            }
                            if let Some(w) = f.wires.iter().find(|w| w.label == f.strings[k].wire) {
                                let (down, dir, _) = polar(top, w.at);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "down/up {down:.1}°, direction {:.1}° (from where the legs meet)",
                                        (dir + ahead).rem_euclid(360.0)
                                    ))
                                    .small()
                                    .weak(),
                                );
                            }
                        });
                    }
                    // The wire point it makes.
                    if owns
                        && let Some(top) = tops[k]
                        && let Some(w) = f.wires.iter_mut().find(|w| w.label == f.strings[k].wire)
                    {
                        // Shown in the project's convention (ahead at its
                        // angle, clockwise seen from above).
                        let (mut down, dir, mut len) = polar(top, w.at);
                        let mut dir = (dir + ahead).rem_euclid(360.0);
                        let mut changed = false;
                        // A V's tip: from its legs' lengths (above).
                        if legs == 1 {
                        ui.horizontal(|ui| {
                            ui.label("down/up");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut down)
                                        .range(0.0..=180.0)
                                        .speed(0.5)
                                        .max_decimals(2)
                                        .suffix("°"),
                                )
                                .on_hover_text("Angle from straight down: 0° down, 90° level, 180° straight up")
                                .changed();
                            ui.label("direction");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut dir)
                                        .range(0.0..=360.0)
                                        .speed(0.5)
                                        .max_decimals(2)
                                        .suffix("°"),
                                )
                                .on_hover_text(
                                    format!(
                                        "Direction in plan, clockwise seen from above: ahead along the line {:.0}°, \
                                         to the right {:.0}°, back {:.0}°, to the left {:.0}° \
                                         (Structures › Angles around structures)",
                                        ahead,
                                        (ahead + 90.0) % 360.0,
                                        (ahead + 180.0) % 360.0,
                                        (ahead + 270.0) % 360.0
                                    ),
                                )
                                .changed();
                            ui.label("length");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut len)
                                        .range(0.01..=50.0)
                                        .speed(0.01)
                                        .max_decimals(3)
                                        .suffix(" m"),
                                )
                                .on_hover_text("Its length: from its top to its wire point")
                                .changed();
                        });
                        }
                        if changed {
                            want_at =
                                Some(from_polar(top, down, (dir - ahead).rem_euclid(360.0), len.max(0.01)));
                        }
                        ui.horizontal(|ui| {
                            ui.label("wire point");
                            // Typed into a copy: its strings follow each
                            // valid name; one taken or empty waits (shown
                            // red) and is dropped when the field is left.
                            let id = ui.id().with(("wire-label", k));
                            let (own, mut text) = ui
                                .data_mut(|d| d.get_temp::<(String, String)>(id))
                                .unwrap_or_else(|| (w.label.clone(), w.label.clone()));
                            let bad = if text.trim().is_empty() {
                                Some("give it a name")
                            } else if clash(&text, &own) {
                                Some("taken: another point, wire point or string has this name")
                            } else {
                                None
                            };
                            let mut edit = egui::TextEdit::singleline(&mut text).desired_width(90.0);
                            if bad.is_some() {
                                edit = edit.text_color(ui.visuals().error_fg_color);
                            }
                            let r = ui.add(edit);
                            if let Some(why) = bad {
                                r.clone().on_hover_text(why);
                            }
                            if r.changed() {
                                let ok = !text.trim().is_empty() && !clash(&text, &own);
                                if ok && text != w.label {
                                    rename = Some((w.label.clone(), text.clone()));
                                    w.label = text.clone();
                                }
                                ui.data_mut(|d| d.insert_temp(id, (own, text)));
                            } else if !r.has_focus() {
                                ui.data_mut(|d| d.remove::<(String, String)>(id));
                            }
                            let mut has = w.set.is_some();
                            if ui
                                .checkbox(&mut has, "set")
                                .on_hover_text(
                                    "A wire is clamped here: sections and jumpers end on its set number. \
                                     Without: other strings may hang from it",
                                )
                                .changed()
                            {
                                w.set = has.then_some(f.set.max(1));
                            }
                            if let Some(set) = &mut w.set {
                                ui.add(egui::DragValue::new(set).range(1..=9999));
                                ui.label("clamp");
                                egui::ComboBox::from_id_salt(("wire-dead-end", k))
                                    .selected_text(if w.dead_end { "strain" } else { "suspension" })
                                    .width(90.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut w.dead_end, false, "suspension");
                                        ui.selectable_value(&mut w.dead_end, true, "strain");
                                    })
                                    .response
                                    .on_hover_text(
                                        "Suspension: the wire runs through, its strings hang. Strain: \
                                         a dead end, its strings are tension strings lined up with \
                                         their conductor",
                                    );
                            }
                            ui.label("counterweight");
                            value(ui, &mut w.weight, 10.0, " N")
                                .on_hover_text("A weight hanging at the clamp (0: none)");
                            w.weight = w.weight.max(0.0);
                        });
                        ui.horizontal(|ui| {
                            ui.label("wire point description");
                            ui.add(
                                egui::TextEdit::singleline(&mut w.description)
                                    .desired_width(220.0)
                                    .hint_text("what it is (free text)"),
                            );
                        });
                    }
                });
                // Its strings follow a wire point renamed.
                if let Some((old, new)) = rename {
                    for x in &mut f.strings {
                        if x.wire == old {
                            x.wire.clone_from(&new);
                        }
                        if x.top == old {
                            x.top.clone_from(&new);
                        }
                    }
                }
                // The library's values (and the wire point at its length).
                if let Some(n) = pick {
                    let a = library[n].2.clone();
                    f.take_from_library(k, &a);
                }
                // Hung from another point: its length kept.
                if let Some(top) = retop {
                    f.note = f.hang_from(model, k, &top).err();
                }
                if let Some(len) = want_len {
                    f.note = f.set_length(k, len).err();
                }
                if let Some(at) = want_at {
                    let wire = f.strings[k].wire.clone();
                    f.place_wire(&wire, at);
                    f.note = None;
                }
                match ends_at {
                    Some(Some(w)) => {
                        f.note = f.end_at(model, k, &w).err();
                    }
                    Some(None) => {
                        let label = f.free_label(model, "W");
                        let top = tops[k].unwrap_or([0.0, 0.0, 20.0]);
                        f.wires.push(WireRow { description: Default::default(),
                            label: label.clone(),
                            at: from_polar(top, 0.0, 0.0, lengths[k].unwrap_or(3.0)),
                            set: None,
                            weight: 0.0,
                            dead_end: false,
                            dead_end_was: false,
                        });
                        f.strings[k].wire = label;
                    }
                    None => {}
                }
            }
            if let Some(k) = remove {
                f.strings.remove(k);
                f.drop_unused_wires();
            }
            if let Some(note) = &f.note {
                crate::menu::error_label(ui, egui::RichText::new(note).small());
            }
            // Wire points no string reaches: the wire clamped straight to
            // the structure.
            for w in f.wires.iter_mut() {
                if f.strings.iter().any(|x| x.wire == w.label) {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.label(format!("wire point {} — clamped straight to the structure", w.label));
                    for (i, name) in ["across", "along", "up"].iter().enumerate() {
                        ui.label(*name);
                        value(ui, &mut w.at[i], 0.05, " m");
                    }
                });
            }
            if ui
                .small_button("+ string")
                .on_hover_text("Another string with its own wire point, 3 m straight down (to set)")
                .clicked()
                && let Some(top) = point_names.first().cloned()
            {
                let label = f.free_label(model, "S");
                let wire = f.free_label(model, "W");
                let at = f
                    .points
                    .iter()
                    .find(|p| p.label == top)
                    .map_or([0.0, 0.0, 20.0], |p| p.at);
                f.wires.push(WireRow { description: Default::default(),
                    label: wire.clone(),
                    at: from_polar(at, 0.0, 0.0, 3.0),
                    set: None,
                    weight: 0.0,
                    dead_end: false,
                    dead_end_was: false,
                });
                f.strings.push(StringRow {
                    label,
                    top,
                    wire,
                    ..Default::default()
                });
            }
            ui.separator();
            ui.strong("New points on the structure");
            help(
                ui,
                "A string hangs from a point of the structure; the structure's points are listed \
                 under \"hangs from\". Add one here where none fits (across: to the right looking \
                 ahead, along: ahead, up: from the base).",
            );
            let mut drop = None;
            // A typed structure: each new point on one of its type's attach
            // joints (only those are offered), at an offset from it.
            let attach = crate::link_tool::attach_joints(model, &f.structure);
            for &k in &new_points {
                let p = &mut f.points[k];
                if !attach.is_empty() && !f.joints.contains_key(&p.label) {
                    // The nearest attach joint, the distance kept as the
                    // offset (shown: change it here).
                    if let Some(j) = attach.iter().min_by(|a, b| {
                        let d = |q: &[f64; 3]| (0..3).map(|i| (q[i] - p.at[i]).powi(2)).sum::<f64>();
                        d(&a.1).total_cmp(&d(&b.1))
                    }) {
                        f.joints.insert(p.label.clone(), j.clone());
                    }
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.label).desired_width(90.0));
                    if !attach.is_empty() {
                        let mut chosen = f.joints.get(&p.label).map(|j| j.0.clone()).unwrap_or_default();
                        egui::ComboBox::from_id_salt(("new-point-joint", k))
                            .selected_text(format!("on {chosen}"))
                            .width(90.0)
                            .show_ui(ui, |ui| {
                                for (j, _) in &attach {
                                    ui.selectable_value(&mut chosen, j.clone(), j);
                                }
                            })
                            .response
                            .on_hover_text(
                                "The attach joint of the structure's type it is linked to (only \
                                 attach joints take strings); chosen, the point is put on it",
                            );
                        if f.joints.get(&p.label).map(|j| &j.0) != Some(&chosen)
                            && let Some(j) = attach.iter().find(|j| j.0 == chosen)
                        {
                            p.at = j.1;
                            f.joints.insert(p.label.clone(), j.clone());
                        }
                    }
                    for (i, name) in ["across", "along", "up"].iter().enumerate() {
                        ui.label(*name);
                        value(ui, &mut p.at[i], 0.05, " m");
                    }
                    if let Some((_, at)) = f.joints.get(&p.label) {
                        let o = [0, 1, 2].map(|i| p.at[i] - at[i]);
                        ui.label(
                            egui::RichText::new(format!(
                                "offset {:.3} m",
                                (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt()
                            ))
                            .small(),
                        )
                        .on_hover_text(crate::link_tool::offset_words(o));
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut p.description)
                            .desired_width(140.0)
                            .hint_text("description"),
                    );
                    if ui.small_button("×").clicked() {
                        drop = Some(k);
                    }
                });
            }
            if let Some(k) = drop {
                let label = f.points[k].label.clone();
                if !f.strings.iter().any(|x| x.top == label) {
                    f.points.remove(k);
                }
            }
            if ui.small_button("+ point").clicked() {
                let label = f.free_label(model, "A");
                let at = f.points.last().map_or([0.0, 0.0, 20.0], |p| p.at);
                f.points.push(PointRow { description: Default::default(), label, at });
            }
        });
    if f.overrides_type {
        ui.label(
            egui::RichText::new("These strings are the structure's own, over its type's.").small(),
        );
        reset = ui.button("Back to the type's strings").clicked();
    }
    reset
}
