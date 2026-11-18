// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Strings table of a placed structure's **Edit** window (the
//! structure screen on a structure without a type, [`crate::placed_editor`];
//! owner, 2026-11-18): "this window should allow the change of string but
//! not of the strings parameter like weight, strength shape and so on".
//!
//! - **Which string** hangs there is chosen: an entry of the project's
//!   insulator library (its `insulator` label). Its **parameters** —
//!   length, kind, weight, wind area, compression, limits — are shown
//!   read-only, "from library entry X"; they are edited in the library
//!   only. A value the structure keeps that differs from its entry is
//!   shown as a warning (nothing is changed).
//! - A string **without an entry** shows its values read-only, "not from
//!   a library entry: choose one, or make one from it", and offers **Make
//!   a library entry from this string** (one command:
//!   [`crate::string_entries`]).
//! - **No Wire points table**: a wire point is where its string puts it —
//!   its attachment point, its length (its entry's) and two angles typed
//!   in the string's row: **from vertical** (0° hanging, 90° level, 180°
//!   up: a post) and **compass** (in the structure's axes: 0° ahead along
//!   the line, clockwise seen from above), its true bearing beside it
//!   (read-only). Typing an angle turns the string about its attachment
//!   point, as a drag does ([`crate::string_drag`]): length kept, what
//!   hangs below follows. The angles of a wire point held by two strings
//!   or more (a V) follow where the legs meet: shown, not typed (said).
//! - Positions are written exactly (not to the millimetre): typed angles
//!   read back the same to 1e-9°.

use eframe::egui;
use ohl_structures::structures::{local_direction, v_meeting};
use ohl_structures::{AssemblyKind, InsulatorAssembly, InsulatorLibrary, StructureType};
use toml_edit::{Array, DocumentMut, Item, Value};

use crate::string_drag::{Net, distance};
use crate::type_workspace::{Geometry, Handle, Request, Sheet, Workspace};

/// Within this, a value the structure keeps is its entry's (lengths, m).
pub const SAME_LENGTH: f64 = 5e-4;

/// A position as written in the Edit window's document: to the
/// millimetre where it is on one (as written before), else exactly (12
/// decimals, trailing zeros dropped).
pub fn exact(p: [f64; 3]) -> Item {
    let mut a = Array::new();
    for v in p {
        a.push(format!("{} m", number(v)));
    }
    Item::Value(Value::Array(a))
}

/// `v` as written: `{:.3}` on the millimetre, else to 12 decimals.
pub fn number(v: f64) -> String {
    let v = if v.abs() < 5e-13 { 0.0 } else { v };
    let mm = (v * 1000.0).round() / 1000.0;
    if (v - mm).abs() < 1e-12 {
        return format!("{mm:.3}");
    }
    let s = format!("{v:.12}");
    let s = s.trim_end_matches('0');
    s.trim_end_matches('.').to_owned()
}

/// The table entry of `kind` with `key` = `label` in `doc`.
fn entry<'a>(
    doc: &'a mut DocumentMut,
    kind: &str,
    key: &str,
    label: &str,
) -> Option<&'a mut toml_edit::Table> {
    doc.get_mut(kind)?
        .as_array_of_tables_mut()?
        .iter_mut()
        .find(|t| t.get(key).and_then(Item::as_str) == Some(label))
}

/// Writes wire point `label` at `at` exactly.
pub fn put_wire(doc: &mut DocumentMut, label: &str, at: [f64; 3]) -> Result<(), String> {
    let w = entry(doc, "wire", "label", label).ok_or_else(|| format!("no wire point {label}"))?;
    w.insert("point", exact(at));
    Ok(())
}

/// The wire points that move when wire point `wire` of `t` (drawn as `g`)
/// is put at `to` by the drag's rule ([`crate::string_drag::drag`]),
/// written exactly; what it did in words.
pub fn turn_wire(
    doc: &mut DocumentMut,
    t: &StructureType,
    g: &Geometry,
    wire: &str,
    to: [f64; 3],
) -> Result<String, String> {
    let net = Net::of_type(t, g);
    let moved = crate::string_drag::drag(&net, &Handle::Wire(wire.to_owned()), to)?;
    for (w, p) in &moved.wires {
        put_wire(doc, w, *p)?;
    }
    Ok(moved.describe())
}

/// The angles of a string from `top` to `wire` (the structure's axes):
/// from vertical (0° hanging) and compass (0° ahead, clockwise from
/// above), degrees.
pub fn angles(top: [f64; 3], wire: [f64; 3]) -> (f64, f64) {
    let (a, d, _) = ohl_structure_edit::polar(top, wire);
    (a, d)
}

/// The true bearing (from north, clockwise) of compass angle `compass`
/// (0° ahead along the line, clockwise) on a structure whose x axis points
/// `orientation_deg` anticlockwise from east, degrees.
pub fn bearing(compass: f64, orientation_deg: f64) -> f64 {
    let b = (compass - orientation_deg).rem_euclid(360.0);
    // North, not 360° (the rounding of a full turn).
    if 360.0 - b < 1e-9 { 0.0 } else { b }
}

/// The string's entry of the library, if it names one there.
fn entry_of<'a>(
    s: &ohl_structures::structures::TypeString,
    insulators: &'a InsulatorLibrary,
) -> Option<&'a InsulatorAssembly> {
    s.insulator.as_deref().and_then(|i| insulators.get(i))
}

/// String `label` of `t` takes library entry `a`: its own parameters go
/// (the entry's are taken), its wire point moves along it to the entry's
/// length (a V: where the legs meet), what hangs below along.
fn choose_entry(
    doc: &mut DocumentMut,
    t: &StructureType,
    g: &Geometry,
    label: &str,
    a: &InsulatorAssembly,
) -> Result<(), String> {
    let s = t
        .strings
        .iter()
        .find(|x| x.label == label)
        .ok_or_else(|| format!("no string {label}"))?;
    let (top, w) = g
        .at(&s.top)
        .zip(g.at(&s.wire))
        .ok_or_else(|| format!("string {label} is not placed"))?;
    let l = a.length.si();
    let legs: Vec<&ohl_structures::structures::TypeString> =
        t.strings.iter().filter(|x| x.wire == s.wire).collect();
    let to = match legs.as_slice() {
        [_] => {
            let d = distance(top, w);
            let u = if d > 1e-12 {
                [0, 1, 2].map(|k| (w[k] - top[k]) / d)
            } else {
                [0.0, 0.0, -1.0]
            };
            [0, 1, 2].map(|k| top[k] + l * u[k])
        }
        [x, y] => {
            let other = if x.label == label { y } else { x };
            let ot = g.at(&other.top).ok_or("the other leg is not placed")?;
            v_meeting(top, l, ot, distance(ot, w), w).ok_or_else(|| {
                format!(
                    "string {label} cannot take {} ({l:.3} m): the legs of the V would not meet",
                    a.id
                )
            })?
        }
        _ => {
            return Err(format!(
                "string {label}: wire point {} is held by {} strings — choose the entries \
                 of strings held by two at most here",
                s.wire,
                legs.len()
            ));
        }
    };
    let e = entry(doc, "string", "label", label).ok_or_else(|| format!("no string {label}"))?;
    e.insert("insulator", toml_edit::value(a.id.clone()));
    for k in [
        "length",
        "weight",
        "wind_area",
        "compression",
        "max_load",
        "max_swing",
        "max_swing_across",
        "max_swing_along",
    ] {
        e.remove(k);
    }
    if a.kind == AssemblyKind::Strain {
        e.insert("strain", toml_edit::value(true));
    } else {
        e.remove("strain");
    }
    // The wire point (and what hangs below) moved by as much.
    let d = [0, 1, 2].map(|k| to[k] - w[k]);
    let net = Net::of_type(t, g);
    let mut below = vec![s.wire.clone()];
    loop {
        let next = net
            .strings
            .iter()
            .find(|x| below.contains(&x.top) && !below.contains(&x.wire));
        match next {
            Some(x) => below.push(x.wire.clone()),
            None => break,
        }
    }
    for b in &below {
        if let Some(p) = g.at(b) {
            put_wire(doc, b, [0, 1, 2].map(|k| p[k] + d[k]))?;
        }
    }
    Ok(())
}

/// A value of the string read-only: the entry's (or its own), with a
/// warning where its own differs from its entry's.
fn value_cell(
    ui: &mut egui::Ui,
    own: Option<f64>,
    from_entry: Option<f64>,
    unit: &str,
    decimals: usize,
    entry: Option<&str>,
    same: f64,
) {
    let shown = own.or(from_entry);
    let text = shown.map_or("—".to_owned(), |v| format!("{v:.decimals$} {unit}"));
    match (own, from_entry, entry) {
        (Some(o), Some(e), Some(id)) if (o - e).abs() > same => {
            ui.colored_label(ui.visuals().warn_fg_color, format!("⚠ {text}"))
                .on_hover_text(format!(
                    "{o:.decimals$} {unit} kept on the structure differs from library entry \
                     {id}'s {e:.decimals$} {unit} (nothing changed: choose the entry again, or \
                     edit the entry in the library)"
                ));
        }
        (_, _, Some(id)) => {
            ui.label(text).on_hover_text(format!(
                "From library entry {id} (edited in the library only)"
            ));
        }
        (_, _, None) => {
            ui.label(text).on_hover_text(
                "Not from a library entry: choose one, or make one from this string",
            );
        }
    }
}

impl Workspace {
    /// The Strings table of the Edit window (module docs): the entry
    /// chosen, its parameters read-only, the two angles typed;
    /// `orientation_deg`: the structure's (the true bearing).
    pub(crate) fn library_strings_sheet(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut DocumentMut,
        t: &StructureType,
        g: &Geometry,
        insulators: &InsulatorLibrary,
        orientation_deg: f64,
    ) -> bool {
        let mut changed = false;
        let assemblies: Vec<&InsulatorAssembly> = insulators
            .iter()
            .filter(|a| a.kind != AssemblyKind::VString)
            .collect();
        let tops: Vec<&str> = t
            .points
            .iter()
            .map(|p| p.label.as_str())
            .chain(t.wires.iter().map(|w| w.label.as_str()))
            .collect();
        // What differs from its entry, said above the table.
        let mut differ: Vec<String> = Vec::new();
        for s in &t.strings {
            let Some(a) = entry_of(s, insulators) else {
                continue;
            };
            let drawn = g.at(&s.top).zip(g.at(&s.wire)).map(|(p, q)| distance(p, q));
            if let Some(l) = drawn
                && (l - a.length.si()).abs() > SAME_LENGTH
            {
                differ.push(format!(
                    "{}: length {l:.3} m differs from library entry {}'s {:.3} m",
                    s.label,
                    a.id,
                    a.length.si()
                ));
            }
            if let Some(w) = s.weight.map(|w| w.si())
                && (w - a.weight.si()).abs() > 1e-9
            {
                differ.push(format!(
                    "{}: weight {w:.1} N differs from library entry {}'s {:.1} N",
                    s.label,
                    a.id,
                    a.weight.si()
                ));
            }
        }
        if !differ.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "⚠ {} value(s) kept on the structure differ from their library entry \
                     (nothing changed): {}",
                    differ.len(),
                    differ.join("; ")
                ),
            );
        }
        // A new string: an entry of the library, from the point picked.
        let from = self
            .selected
            .iter()
            .find_map(|h| match h {
                Handle::Point(l) | Handle::Wire(l) => Some(l.clone()),
                _ => None,
            })
            .or_else(|| t.points.first().map(|p| p.label.clone()));
        let mut add: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(from.is_some() && !assemblies.is_empty(), |ui| {
                ui.menu_button(NEW_STRING, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for a in &assemblies {
                                if ui
                                    .selectable_label(false, format!("{} — {}", a.id, a.name))
                                    .clicked()
                                {
                                    add = Some(a.id.clone());
                                    ui.close();
                                }
                            }
                        });
                })
                .response
                .on_hover_text(format!(
                    "A string of the library hanging from {} (the point picked), to a new \
                         wire point",
                    from.as_deref().unwrap_or("—")
                ));
            });
            ui.label(
                egui::RichText::new(
                    "A string's parameters come from its library entry (edited in the \
                     library); here, choose the entry and set the string's two angles.",
                )
                .weak(),
            );
            if ui
                .button("Insulator library…")
                .on_hover_text("The insulator assemblies: import, export, edit, delete")
                .clicked()
            {
                self.request = Some(Request::OpenInsulators);
            }
        });
        if let (Some(id), Some(from)) = (add, &from)
            && let Some(a) = insulators.get(&id)
        {
            match ohl_structure_edit::new_string(doc, t, from, Some(a), a.length.si(), None) {
                Ok((l, w)) => {
                    self.message = Some((format!("string {l} ({id}) from {from} to {w}"), false));
                    self.selected = vec![Handle::Wire(w)];
                    changed = true;
                }
                Err(e) => self.message = Some((e, true)),
            }
        }
        let mut changes: Vec<Change> = Vec::new();
        let focus = self.focus.clone();
        egui::Grid::new("placed-strings").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("string", "Its label: a click picks it in the view"),
                ("hangs from", "An attachment point, or another string's wire point"),
                ("wire point", "Where it ends: where its length and angles put it"),
                ("library entry", "The insulator library entry it is: its length, kind, weight, wind area and limits"),
                ("kind", "Suspension or strain: its entry's"),
                ("length", "Its entry's: changed only by choosing another entry"),
                ("from vertical", "Up/down angle from hanging straight down: 0° hanging, 90° level, 180° straight up (a post)"),
                ("compass", "Horizontal direction in the structure's axes: 0° ahead along the line, clockwise seen from above"),
                ("bearing", "The true bearing of the compass angle: from north, clockwise (read-only)"),
                ("weight", "Its entry's"),
                ("wind area", "Its entry's"),
                ("compression", "The push it may take (none: tension only)"),
                ("limits", "Rated load and swings: its entry's (kept, not checked by OpenOHL)"),
                ("counterweight", "A weight hanging at its wire point"),
                ("description", "What it is, in words"),
                ("", ""),
            ] {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            for s in &t.strings {
                let a = entry_of(s, insulators);
                let entry_id = a.map(|a| a.id.as_str());
                let wire = Handle::Wire(s.wire.clone());
                let r = ui.selectable_label(self.selected.contains(&wire), &s.label);
                if r.clicked() {
                    self.selected = vec![wire.clone()];
                    self.redraw();
                }
                let focused = focus.as_ref() == Some(&(Sheet::Strings, s.label.clone()));
                if focused {
                    r.scroll_to_me(Some(egui::Align::Center));
                }
                // Hangs from.
                let mut top = s.top.clone();
                egui::ComboBox::from_id_salt(("placed-string-top", &s.label))
                    .selected_text(&top)
                    .show_ui(ui, |ui| {
                        for l in tops.iter().filter(|l| **l != s.wire) {
                            ui.selectable_value(&mut top, (*l).to_owned(), *l);
                        }
                    });
                if top != s.top {
                    changes.push(Change::Top(s.label.clone(), top));
                }
                ui.label(&s.wire);
                // The entry.
                let mut pick = s.insulator.clone();
                egui::ComboBox::from_id_salt(("placed-string-entry", &s.label))
                    .selected_text(match (entry_id, &s.insulator) {
                        (Some(id), _) => id.to_owned(),
                        (None, Some(id)) => format!("{id} (not in the library)"),
                        (None, None) => "— none".to_owned(),
                    })
                    .show_ui(ui, |ui| {
                        for x in &assemblies {
                            ui.selectable_value(&mut pick, Some(x.id.clone()), &x.id)
                                .on_hover_text(format!(
                                    "{}: {:?}, {:.3} m, {:.0} N",
                                    x.name,
                                    x.kind,
                                    x.length.si(),
                                    x.weight.si()
                                ));
                        }
                    });
                if pick != s.insulator
                    && let Some(id) = pick
                {
                    changes.push(Change::Entry(s.label.clone(), id));
                }
                ui.label(if s.strain { "strain" } else { "suspension" })
                    .on_hover_text(entry_id.map_or(
                        "Not from a library entry: choose one, or make one from this string"
                            .to_owned(),
                        |id| format!("From library entry {id}"),
                    ));
                let ends = g.at(&s.top).zip(g.at(&s.wire));
                let drawn = ends.map(|(p, q)| distance(p, q));
                value_cell(ui, drawn, a.map(|a| a.length.si()), "m", 3, entry_id, SAME_LENGTH);
                // The two angles: typed for a string alone at its wire
                // point; a V's follow where its legs meet.
                let legs = t.strings.iter().filter(|x| x.wire == s.wire).count();
                let (down, compass) = ends.map_or((0.0, 0.0), |(p, q)| angles(p, q));
                if legs == 1 && ends.is_some() {
                    let (mut dn, mut dr) = (down, compass);
                    let r1 = crate::style::field(
                        ui,
                        egui::DragValue::new(&mut dn)
                            .range(0.0..=180.0)
                            .speed(0.5)
                            .min_decimals(4).max_decimals(4)
                            .suffix("°"),
                    )
                    .on_hover_text("From vertical: 0° = hanging, 90° = level, 180° = straight up");
                    if focused {
                        r1.request_focus();
                        r1.scroll_to_me(Some(egui::Align::Center));
                    }
                    let r2 = crate::style::field(
                        ui,
                        egui::DragValue::new(&mut dr)
                            .range(0.0..=360.0)
                            .speed(0.5)
                            .min_decimals(4).max_decimals(4)
                            .suffix("°"),
                    )
                    .on_hover_text("Compass: 0° = ahead along the line, clockwise seen from above");
                    // An angle typed (not the one shown, to 4 decimals,
                    // read back as the field is left: the string would
                    // turn by its rounding).
                    let typed = |r: &egui::Response, a: f64, b: f64| {
                        r.changed() && (a - b).abs() >= 5e-5
                    };
                    if typed(&r1, dn, down) || typed(&r2, dr, compass) {
                        changes.push(Change::Angles(s.label.clone(), dn, dr.rem_euclid(360.0)));
                    }
                } else {
                    let why = format!(
                        "One of {legs} strings to wire point {}: its angles follow where they \
                         meet (their lengths) — drag {} to turn them",
                        s.wire, s.wire
                    );
                    ui.label(format!("{down:.2}° (V)")).on_hover_text(&why);
                    ui.label(format!("{compass:.2}° (V)")).on_hover_text(&why);
                }
                ui.label(format!("{:.2}°", bearing(compass, orientation_deg)))
                    .on_hover_text("True bearing: from north, clockwise (read-only)");
                value_cell(
                    ui,
                    s.weight.map(|w| w.si()),
                    a.map(|a| a.weight.si()),
                    "N",
                    1,
                    entry_id,
                    1e-9,
                );
                value_cell(
                    ui,
                    s.wind_area.map(|w| w.si()),
                    a.map(|a| a.wind_area.si()),
                    "m²",
                    3,
                    entry_id,
                    1e-12,
                );
                ui.label(
                    s.compression
                        .map(|c| c.si())
                        .or(a.and_then(|a| a.compression_load.map(|c| c.si())))
                        .map_or("— (tension only)".to_owned(), |c| format!("{c:.0} N")),
                );
                let l = s.limits().or(a.map(|a| a.limits()).unwrap_or_default());
                let mut words: Vec<String> = Vec::new();
                if let Some(v) = l.max_load {
                    words.push(format!("{v:.0} N"));
                }
                for (n, v) in [
                    ("swing", l.max_swing_deg),
                    ("across", l.max_swing_across_deg),
                    ("along", l.max_swing_along_deg),
                ] {
                    if let Some(v) = v {
                        words.push(format!("{n} {v:.1}°"));
                    }
                }
                ui.label(if words.is_empty() {
                    "—".to_owned()
                } else {
                    words.join(", ")
                });
                // The counterweight at its wire point (on its first
                // string's row).
                let first = t.strings.iter().find(|x| x.wire == s.wire).map(|x| &x.label);
                if first == Some(&s.label) {
                    let mut cw = t
                        .wires
                        .iter()
                        .find(|w| w.label == s.wire)
                        .and_then(|w| w.weight.map(|f| f.si()))
                        .unwrap_or(0.0);
                    if ui
                        .add(
                            egui::DragValue::new(&mut cw)
                                .range(0.0..=1e6)
                                .speed(10.0)
                                .max_decimals(0)
                                .suffix(" N"),
                        )
                        .changed()
                    {
                        changes.push(Change::Counterweight(s.wire.clone(), cw));
                    }
                } else {
                    ui.label("");
                }
                let mut d = s.description.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut d).desired_width(140.0))
                    .changed()
                {
                    changes.push(Change::Description(s.label.clone(), d));
                }
                ui.horizontal(|ui| {
                    if entry_id.is_none() {
                        ui.label(
                            egui::RichText::new(
                                "not from a library entry: choose one, or make one from it",
                            )
                            .small()
                            .weak(),
                        );
                        if ui
                            .small_button(MAKE_ENTRY)
                            .on_hover_text(
                                "Adds an entry to the project's insulator library with this \
                                 string's values exactly, and points the string at it (one \
                                 command, one undo; the results do not change)",
                            )
                            .clicked()
                        {
                            self.request = Some(Request::MakeStringEntry(s.label.clone()));
                        }
                    }
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Delete this string")
                        .clicked()
                    {
                        changes.push(Change::Delete(s.label.clone()));
                    }
                });
                ui.end_row();
            }
        });
        if focus.as_ref().is_some_and(|f| f.0 == Sheet::Strings) {
            self.focus = None;
        }
        for c in changes {
            let result = match c {
                Change::Top(l, top) => ohl_structure_edit::set_string(
                    doc,
                    t,
                    g,
                    insulators,
                    &l,
                    ohl_structure_edit::StringChange::Top(top),
                )
                .map(|()| format!("string {l} hangs from another point")),
                Change::Entry(l, id) => match insulators.get(&id) {
                    Some(a) => choose_entry(doc, t, g, &l, a)
                        .map(|()| format!("string {l} is library entry {id} now")),
                    None => Err(format!("no library entry {id}")),
                },
                Change::Angles(l, dn, dr) => {
                    let s = t.strings.iter().find(|x| x.label == l);
                    match s.and_then(|s| Some((s, g.at(&s.top)?, g.at(&s.wire)?))) {
                        Some((s, top, w)) => {
                            let len = distance(top, w);
                            let u = local_direction(dn, dr);
                            let to = [0, 1, 2].map(|k| top[k] + len * u[k]);
                            turn_wire(doc, t, g, &s.wire, to).map(|words| {
                                format!(
                                    "string {l}: {dn:.2}° from vertical, compass {dr:.2}° \
                                     (length {len:.3} m kept){words}"
                                )
                            })
                        }
                        None => Err(format!("string {l} is not placed")),
                    }
                }
                Change::Counterweight(w, cw) => {
                    ohl_structure_edit::set_counterweight(doc, &w, cw);
                    Ok(format!("counterweight {cw:.0} N at wire point {w}"))
                }
                Change::Description(l, d) => ohl_structure_edit::set_string(
                    doc,
                    t,
                    g,
                    insulators,
                    &l,
                    ohl_structure_edit::StringChange::Description(d),
                )
                .map(|()| String::new()),
                Change::Delete(l) => {
                    ohl_structure_edit::delete_string(doc, t, g, &l);
                    Ok(format!("deleted string {l}"))
                }
            };
            match result {
                Ok(words) => {
                    changed = true;
                    if !words.is_empty() {
                        self.message = Some((words, false));
                    }
                }
                Err(e) => self.message = Some((e, true)),
            }
        }
        if t.strings.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No string yet: pick an attachment point, then Add string (or the \
                     Add string tool).",
                )
                .weak(),
            );
        }
        changed
    }
}

/// The choice adding a string of the library.
pub const NEW_STRING: &str = "+ String from the library…";

/// The button making a library entry from a string without one.
pub const MAKE_ENTRY: &str = "Make a library entry from this string";

/// A change asked in the table (made after it is drawn).
enum Change {
    Top(String, String),
    Entry(String, String),
    Angles(String, f64, f64),
    Counterweight(String, f64),
    Description(String, String),
    Delete(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Written on the millimetre as before, else exactly; read back equal.
    #[test]
    fn numbers_are_written_exactly() {
        assert_eq!(number(30.0), "30.000");
        assert_eq!(number(-1.495), "-1.495");
        assert_eq!(number(1e-14), "0.000");
        let v = 26.123456789012;
        assert_eq!(number(v), "26.123456789012");
        let back: f64 = number(1.0 / 3.0).parse().unwrap();
        assert!((back - 1.0 / 3.0).abs() < 1e-12);
    }

    /// The bearing: on a structure whose x axis points east, ahead is
    /// north; on one turned by −85.24°, ahead is 85.24°.
    #[test]
    fn the_true_bearing() {
        assert_eq!(bearing(0.0, 0.0), 0.0);
        assert_eq!(bearing(90.0, 0.0), 90.0);
        assert!((bearing(0.0, -85.24) - 85.24).abs() < 1e-12);
        assert!((bearing(350.0, 20.0) - 330.0).abs() < 1e-12);
        assert_eq!(bearing(274.76, -85.24), 0.0);
    }
}
