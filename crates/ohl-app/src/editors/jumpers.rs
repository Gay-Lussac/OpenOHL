// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The jumper window: the jumper, its clamps and helping strings; its
//! pieces, new jumpers (through strings, between points, from a tap), the
//! jumpers missing.

use super::*;

/// A jumper being changed: the jumper as it will be put.
#[derive(Debug, Clone, PartialEq)]
pub struct JumperForm {
    /// The jumper.
    pub jumper: ohl_line::Jumper,
    /// A support clamp to be removed, waiting for its confirmation.
    pub remove: Option<usize>,
    /// Not in the line yet: put on Apply, nothing on Cancel (a jumper
    /// between two wire ends, its way of being given asked first).
    pub new: bool,
}

/// How a jumper is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JumperBy {
    Length,
    Sag,
    Rigid,
}

impl JumperForm {
    /// Jumper `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let stored = model.stored_line().unwrap_or(&model.line);
        let jumper = stored.jumpers.iter().find(|j| j.id == id)?.clone();
        Some(Self {
            jumper,
            remove: None,
            new: false,
        })
    }

    /// New jumper `jumper`, not yet put: given by its length, offered as
    /// the least it can have, the straight distance between its ends in
    /// still air (rounded up to the cm); sag or rigid chosen in the window.
    pub fn new(model: &Model, mut jumper: ohl_line::Jumper) -> Self {
        let end = |e: &ohl_line::JumperEnd| ohl_line::jumper::approximate_end(&model.line, e, 0);
        if let (Some(a), Some(b)) = (end(&jumper.from), end(&jumper.to)) {
            let chord = (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt();
            jumper.sag = None;
            jumper.rigid = false;
            jumper.length = Some((chord * 100.0).ceil() / 100.0);
        }
        Self {
            jumper,
            remove: None,
            new: true,
        }
    }

    pub(super) fn command(&self) -> Result<(String, Command), String> {
        let j = &self.jumper;
        Ok((
            if self.new {
                format!("added jumper {}", j.id)
            } else {
                format!("changed jumper {}", j.id)
            },
            Command::Put {
                kind: Kind::Jumper,
                id: j.id.clone(),
                value: serde_json::to_value(j).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

pub(super) fn jumper_ui(ui: &mut egui::Ui, f: &mut JumperForm, model: &Model) {
    let j = &mut f.jumper;
    egui::Grid::new("jumper-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("ends");
            ui.label(format!("{} ➡ {}", j.from, j.to));
            ui.end_row();
            ui.label("description");
            ui.add(
                egui::TextEdit::singleline(&mut j.description)
                    .desired_width(240.0)
                    .hint_text("what it is (free text)"),
            );
            ui.end_row();
            ui.label("wire type");
            egui::ComboBox::from_id_salt("jumper-cable")
                .selected_text(&j.cable)
                .height(400.0)
                .show_ui(ui, |ui| {
                    for c in model.library.iter() {
                        ui.selectable_value(&mut j.cable, c.id.clone(), &c.id);
                    }
                });
            ui.end_row();
            if model.library.is_empty() {
                ui.label("");
                empty_wire_library(ui);
                ui.end_row();
            }
            ui.label("given by");
            let mut by = if j.rigid {
                JumperBy::Rigid
            } else if j.length.is_some() {
                JumperBy::Length
            } else {
                JumperBy::Sag
            };
            let before = by;
            ui.horizontal(|ui| {
                ui.radio_value(&mut by, JumperBy::Length, "length")
                    .on_hover_text("Its unstressed (cut) length at its temperature");
                ui.radio_value(&mut by, JumperBy::Sag, "sag")
                    .on_hover_text("Its sag in still air at its temperature: cut to that");
                ui.radio_value(&mut by, JumperBy::Rigid, "rigid")
                    .on_hover_text("A tube or bar: straight between its clamps");
            });
            if by != before {
                match by {
                    JumperBy::Length => {
                        j.rigid = false;
                        j.sag = None;
                        j.length = Some(j.length.unwrap_or(5.0));
                    }
                    JumperBy::Sag => {
                        j.rigid = false;
                        j.length = None;
                        j.sag = Some(j.sag.unwrap_or(1.0));
                    }
                    JumperBy::Rigid => {
                        j.rigid = true;
                        j.length = None;
                        j.sag = None;
                    }
                }
            }
            ui.end_row();
            match by {
                JumperBy::Length => {
                    ui.label("length");
                    if let Some(l) = &mut j.length {
                        value(ui, l, 0.01, " m");
                    }
                    ui.end_row();
                }
                JumperBy::Sag => {
                    ui.label("sag");
                    if let Some(s) = &mut j.sag {
                        value(ui, s, 0.01, " m");
                    }
                    ui.end_row();
                }
                JumperBy::Rigid => {}
            }
            if by != JumperBy::Rigid {
                ui.label("temperature");
                value(ui, &mut j.temperature, 0.5, " °C");
                ui.end_row();
                ui.label("stiffness EI");
                let mut given = j.bending_stiffness.is_some();
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut given, "given").changed() {
                        j.bending_stiffness = given.then_some(0.0);
                    }
                    match &mut j.bending_stiffness {
                        Some(ei) => {
                            value(ui, ei, 1.0, " N·m²").on_hover_text("0: hinged ends");
                        }
                        None => {
                            let mut k = j
                                .stiffness_factor
                                .unwrap_or(model.line.defaults.jumper_stiffness_factor);
                            ui.label("factor");
                            if ui
                                .add(egui::DragValue::new(&mut k).speed(0.01).range(0.01..=1.0))
                                .on_hover_text(
                                    "Share of the wire's solid-bar stiffness (0.2–0.35 usual)",
                                )
                                .changed()
                            {
                                j.stiffness_factor = Some(k);
                            }
                            if let Some(c) = model.library.get(&j.cable) {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:.0} N·m²",
                                        ohl_line::jumper::jumper_bending_stiffness(
                                            j,
                                            c,
                                            &model.line.defaults
                                        )
                                    ))
                                    .weak(),
                                );
                            }
                        }
                    }
                });
                ui.end_row();
                let d = model.line.defaults.jumper_exit_angle_deg;
                for (name, angle) in [
                    ("leaves from at", &mut j.angle_from_deg),
                    ("leaves to at", &mut j.angle_to_deg),
                ] {
                    ui.label(name);
                    let mut a = angle.unwrap_or(d);
                    if value(ui, &mut a, 1.0, "° from its tension string").changed() {
                        *angle = Some(a);
                    }
                    ui.end_row();
                }
            }
            ui.label("bundle");
            ui.horizontal(|ui| {
                let mut own = j.bundle.is_some();
                if ui
                    .checkbox(&mut own, "its own")
                    .on_hover_text("Else as the section at its first end")
                    .changed()
                {
                    j.bundle = own.then(|| model.line.jumper_bundle(j).count.max(1));
                    j.bundle_spacing = own.then(|| model.line.jumper_bundle(j).spacing).flatten();
                }
                if let Some(n) = &mut j.bundle {
                    ui.add(egui::DragValue::new(n).range(1..=8));
                    if *n > 1 {
                        let mut sp = j.bundle_spacing.unwrap_or(0.4);
                        ui.label("spacing");
                        if value(ui, &mut sp, 0.01, " m").changed() || j.bundle_spacing.is_none() {
                            j.bundle_spacing = Some(sp);
                        }
                    } else {
                        j.bundle_spacing = None;
                    }
                } else {
                    let b = model.line.jumper_bundle(j);
                    ui.label(egui::RichText::new(format!("as its section: {}", b.count)).weak());
                }
            });
            ui.end_row();
        });
    if j.support().is_some() {
        ui.label(egui::RichText::new("Its support string is kept (Jumpers table).").small());
    }
    if !j.rigid {
        clamps_ui(ui, j, &mut f.remove, model);
    }
}

/// Wire points of structure `id` a support clamp may hang on: without a
/// set, on one string or a V from structure points.
pub(super) fn clamp_wires(model: &Model, id: &str) -> Vec<String> {
    model.line.structure(id).map_or(Vec::new(), |st| {
        st.wires
            .iter()
            .filter(|w| ohl_line::clamp_strings(st, &w.label).is_ok())
            .map(|w| w.label.clone())
            .collect()
    })
}

/// A jumper's support clamps: each on a wire point of a structure, where
/// along the jumper, how long; added and removed (a removal confirmed).
pub(super) fn clamps_ui(
    ui: &mut egui::Ui,
    j: &mut ohl_line::Jumper,
    remove: &mut Option<usize>,
    model: &Model,
) {
    ui.separator();
    ui.label("Suspension strings holding it").on_hover_text(
        "Strings of the structure holding the jumper between its tension clamps, \
         like suspension strings on the line: one or two to a wire point without a set. \
         Each holds the jumper in a clamp that swings and turns freely; the jumper runs \
         straight through the clamp. The strings take their weight and wind area.",
    );
    let ends: Vec<String> = {
        let mut v: Vec<String> = j.points().map(|p| p.structure.clone()).collect();
        v.dedup();
        v
    };
    let from = j.from.structure().unwrap_or_default().to_owned();
    // Edited here, put back on the jumper (its points) when changed.
    let mut supports = j.supports();
    let before = supports.clone();
    egui::Grid::new("jumper-clamps")
        .num_columns(5)
        .show(ui, |ui| {
            for (k, c) in supports.iter_mut().enumerate() {
                ui.label(format!("clamp {}", k + 1));
                let id = c.structure.clone().unwrap_or_else(|| from.clone());
                egui::ComboBox::from_id_salt(("clamp-structure", k))
                    .selected_text(&id)
                    .show_ui(ui, |ui| {
                        for s in &ends {
                            if ui.selectable_label(*s == id, s).clicked() {
                                c.structure = (*s != from).then(|| s.clone());
                            }
                        }
                    });
                let id = c.structure.clone().unwrap_or_else(|| from.clone());
                let wires = clamp_wires(model, &id);
                egui::ComboBox::from_id_salt(("clamp-wire", k))
                    .selected_text(&c.wire)
                    .show_ui(ui, |ui| {
                        if wires.is_empty() {
                            ui.label("no wire point without a set on strings");
                        }
                        for w in &wires {
                            ui.selectable_value(&mut c.wire, w.clone(), w);
                        }
                    })
                    .response
                    .on_hover_text("A wire point without a set, on one string or two");
                ui.horizontal(|ui| {
                    ui.label("at");
                    ui.add(
                        egui::DragValue::new(&mut c.at)
                            .speed(0.005)
                            .range(0.01..=0.99)
                            .max_decimals(3),
                    )
                    .on_hover_text(
                        "Its middle, as a share of the jumper's length from its first end",
                    );
                    ui.label("long");
                    ui.add(
                        egui::DragValue::new(&mut c.clamp_length)
                            .speed(0.005)
                            .range(0.01..=1.0)
                            .max_decimals(3)
                            .suffix(" m"),
                    )
                    .on_hover_text("The length of its clamp: the jumper is straight through it");
                });
                if ui.button("Remove…").clicked() {
                    *remove = Some(k);
                }
                ui.end_row();
            }
        });
    if let Some(k) = *remove {
        if k >= supports.len() {
            *remove = None;
        } else {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Remove clamp {} (on {} of {})? Nothing else goes with it; its strings stay \
                     on the structure. Applied as one change.",
                    k + 1,
                    supports[k].wire,
                    supports[k].structure_id(j)
                ));
                if ui.button("Remove").clicked() {
                    supports.remove(k);
                    *remove = None;
                }
                if ui.button("Keep").clicked() {
                    *remove = None;
                }
            });
        }
    }
    if supports != before {
        j.set_supports(supports.clone());
    }
    // The lengths between the clamps: typed, they give the jumper's length
    // and its clamps' places.
    let pieces = jumper_pieces(j);
    if !pieces.is_empty() {
        ui.label("Lengths between the clamps (cut lengths; the jumper's length is their sum)");
        let mut edited = pieces.clone();
        let mut changed = false;
        egui::Grid::new("jumper-pieces")
            .num_columns(2)
            .show(ui, |ui| {
                // Each clamp by its wire point; the ends by structure and set.
                let mut names: Vec<String> = vec![j.from.to_string()];
                names.extend(supports.iter().map(|c| c.wire.clone()));
                names.push(j.to.to_string());
                for (k, piece) in edited.iter_mut().enumerate() {
                    let name = format!("{} to {}", names[k], names[k + 1]);
                    ui.label(name);
                    changed |= value(ui, piece, 0.01, " m").changed();
                    ui.end_row();
                }
                ui.label("total");
                ui.label(format!("{:.3} m", edited.iter().sum::<f64>()));
                ui.end_row();
            });
        if changed {
            for piece in &mut edited {
                *piece = piece.max(0.05);
            }
            set_jumper_pieces(j, &edited);
        }
    }
    let why_not = if j.support().is_some() {
        Some(
            "It has a support string: clamps on suspension strings replace it (remove it in the Jumpers table)",
        )
    } else if j.length.is_none() {
        Some("A jumper held by suspension strings is given by its length")
    } else {
        None
    };
    let add = ui
        .add_enabled(
            why_not.is_none(),
            egui::Button::new("Add a clamp on a suspension string"),
        )
        .on_hover_text(
            "Held by the strings to a wire point without a set of its first end's structure",
        );
    let add = match why_not {
        Some(why) => add.on_disabled_hover_text(why),
        None => add,
    };
    if add.clicked() {
        // After the last clamp, halfway to the end.
        let after = supports.last().map_or(0.0, |c| c.at);
        let mut c = ohl_line::JumperClamp::new(
            clamp_wires(model, &from)
                .first()
                .cloned()
                .unwrap_or_default(),
            0.5 * (after + 1.0),
        );
        if supports.is_empty() {
            c.at = 0.5;
        }
        c.clamp_length = model.line.defaults.jumper_clamp_length;
        j.edit_supports(|cs| cs.push(c));
    }
}

/// The lengths of a jumper's pieces between its clamps, m: from its
/// first end to the first helping string's clamp, between clamps, and on
/// to its other end (its length shared by the clamps' places along it);
/// empty without a length or helping strings.
pub fn jumper_pieces(j: &ohl_line::Jumper) -> Vec<f64> {
    let Some(length) = j.length else {
        return Vec::new();
    };
    let supports = j.supports();
    if supports.is_empty() {
        return Vec::new();
    }
    let mut at: Vec<f64> = supports.iter().map(|c| c.at).collect();
    at.insert(0, 0.0);
    at.push(1.0);
    at.windows(2).map(|w| (w[1] - w[0]) * length).collect()
}

/// Gives a jumper with helping strings these piece lengths (see
/// [`jumper_pieces`]): its length their sum, its clamps' places from them.
pub fn set_jumper_pieces(j: &mut ohl_line::Jumper, pieces: &[f64]) {
    let mut supports = j.supports();
    if pieces.len() != supports.len() + 1 {
        return;
    }
    let total: f64 = pieces.iter().sum();
    if total <= 0.0 {
        return;
    }
    let mut run = 0.0;
    for (c, piece) in supports.iter_mut().zip(pieces) {
        run += piece;
        c.at = run / total;
    }
    j.set_supports(supports);
    j.length = Some(total);
    j.sag = None;
}

/// A new jumper from `from` to `to` through clamps on suspension strings
/// `via` (structure, wire point without a set), in that order: given by its
/// length, each piece between consecutive clamps 3 % longer than the
/// straight distance between them (to be typed in its window).
pub fn new_jumper_through(
    model: &Model,
    from: &SectionPoint,
    to: &SectionPoint,
    via: &[(String, String)],
) -> Result<ohl_line::Jumper, String> {
    let line = &model.line;
    let mut j = new_jumper(model, from, to, &[])?;
    let mut at: Vec<[f64; 3]> = Vec::with_capacity(via.len() + 2);
    at.push(line.wire_point(from).ok_or("no first end")?.position);
    for (s, w) in via {
        let p = line
            .structure(s)
            .and_then(|s| s.wire(w))
            .ok_or_else(|| format!("no wire point {w} on {s}"))?;
        at.push(p.position);
    }
    at.push(line.wire_point(to).ok_or("no other end")?.position);
    let pieces: Vec<f64> = at
        .windows(2)
        .map(|w| {
            1.03 * (0..3)
                .map(|k| (w[1][k] - w[0][k]).powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .map(|l: f64| l.max(0.05))
        .collect();
    // (Placed in order: their places follow from the pieces.)
    let n = via.len() as f64;
    j.set_supports(
        via.iter()
            .enumerate()
            .map(|(k, (s, w))| {
                let mut c = ohl_line::JumperClamp::new(w.clone(), (k as f64 + 1.0) / (n + 1.0));
                c.clamp_length = model.line.defaults.jumper_clamp_length;
                if *s != from.structure {
                    c.structure = Some(s.clone());
                }
                c
            })
            .collect(),
    );
    set_jumper_pieces(&mut j, &pieces);
    Ok(j)
}

/// A new jumper from `from` to `to`: the cable of the section at `from`,
/// a sag of 15 % of the distance (0.3–3 m) at 15 °C, the next free id.
pub fn new_jumper(
    model: &Model,
    from: &SectionPoint,
    to: &SectionPoint,
    taken: &[String],
) -> Result<ohl_line::Jumper, String> {
    let line = &model.line;
    if from == to {
        return Err("a jumper joins two different strings".into());
    }
    let (a, b) = (
        line.wire_point(from)
            .ok_or_else(|| format!("no wire point {} set {}", from.structure, from.set))?,
        line.wire_point(to)
            .ok_or_else(|| format!("no wire point {} set {}", to.structure, to.set))?,
    );
    let d = [0, 1, 2].map(|k| b.position[k] - a.position[k]);
    let chord = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let cable = line
        .sections
        .iter()
        .find(|s| s.points.contains(from))
        .or_else(|| line.sections.iter().find(|s| s.points.contains(to)))
        .map(|s| s.cable.clone())
        .or_else(|| model.library.iter().next().map(|c| c.id.clone()))
        .ok_or("no wire type for the jumper")?;
    let like = line.jumpers.last().map_or("J0", |j| j.id.as_str());
    let id = next_id(
        line.jumpers
            .iter()
            .map(|j| j.id.as_str())
            .chain(taken.iter().map(String::as_str)),
        like,
    );
    serde_json::from_value(serde_json::json!({
        "id": id,
        "cable": cable,
        "from": from,
        "to": to,
        "sag": ((0.15 * chord).clamp(0.3, 3.0) * 100.0).round() / 100.0,
        "temperature": ohl_line::DEFAULT_STRINGING_TEMPERATURE,
    }))
    .map_err(|e| e.to_string())
}

/// A new jumper between two ends, one of them at least a tap (a clamp on
/// a span point or a jumper point; the other a section end or a tap):
/// its cable the section's it hangs on, given by its length, 3 % over the
/// still-air distance between its ends.
pub fn new_tap_jumper(
    model: &Model,
    from: ohl_line::JumperEnd,
    to: ohl_line::JumperEnd,
) -> Result<ohl_line::Jumper, String> {
    let line = &model.line;
    if from == to {
        return Err("a jumper joins two different points".into());
    }
    let end = |e: &ohl_line::JumperEnd| {
        ohl_line::jumper::approximate_end(line, e, 0).ok_or_else(|| match e.as_tap() {
            Some(t) => format!("no place for the tap {}", t.text()),
            None => format!("no wire point {e}"),
        })
    };
    let (a, b) = (end(&from)?, end(&to)?);
    let chord = (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt();
    if chord < 0.1 {
        return Err("its ends are less than 0.1 m apart".into());
    }
    let section = |e: &ohl_line::JumperEnd| match e {
        ohl_line::JumperEnd::Tap { tap } => line.tap_section(tap, 0),
        ohl_line::JumperEnd::Wire(p) => line.sections.iter().find(|s| s.points.contains(p)),
    };
    let cable = section(&from)
        .or_else(|| section(&to))
        .map(|s| s.cable.clone())
        .or_else(|| model.library.iter().next().map(|c| c.id.clone()))
        .ok_or("no wire type for the jumper")?;
    let like = line.jumpers.last().map_or("J0", |j| j.id.as_str());
    let id = next_id(line.jumpers.iter().map(|j| j.id.as_str()), like);
    let j: ohl_line::Jumper = serde_json::from_value(serde_json::json!({
        "id": id,
        "cable": cable,
        "from": from,
        "to": to,
        "length": ((1.03 * chord) * 100.0).round() / 100.0,
        "temperature": ohl_line::DEFAULT_STRINGING_TEMPERATURE,
    }))
    .map_err(|e| e.to_string())?;
    Ok(j)
}

/// The jumpers missing at structure `id`: between the ends of two
/// sections there (their first or last point) of the same circuit and
/// phase, where no jumper is yet. Also what could not be paired.
pub fn missing_jumpers(model: &Model, id: &str) -> (Vec<ohl_line::Jumper>, Vec<String>) {
    let line = &model.line;
    let jumpered = |p: &SectionPoint| line.jumpers.iter().any(|j| &j.from == p || &j.to == p);
    // (circuit, phase) → the free section ends at this structure.
    let mut ends: std::collections::BTreeMap<(String, String), Vec<(String, SectionPoint)>> =
        std::collections::BTreeMap::new();
    for s in &line.sections {
        let (Some(first), Some(last)) = (s.points.first(), s.points.last()) else {
            continue;
        };
        for p in [first, last] {
            if p.structure == id && !jumpered(p) {
                ends.entry((s.circuit_name.clone(), s.phase_label.clone()))
                    .or_default()
                    .push((s.id.clone(), p.clone()));
            }
        }
    }
    let mut out = Vec::new();
    let mut unpaired = Vec::new();
    for ((circuit, phase), list) in ends {
        let mut list = list;
        list.dedup_by(|a, b| a.1 == b.1);
        if list.len() == 2 && list[0].0 != list[1].0 {
            let taken: Vec<String> = out
                .iter()
                .map(|j: &ohl_line::Jumper| j.id.clone())
                .collect();
            match new_jumper(model, &list[0].1, &list[1].1, &taken) {
                Ok(j) => out.push(j),
                Err(e) => unpaired.push(e),
            }
        } else if list.len() > 1 {
            unpaired.push(format!(
                "{circuit} {phase}: {} section ends — join them string by string",
                list.len()
            ));
        }
    }
    (out, unpaired)
}
