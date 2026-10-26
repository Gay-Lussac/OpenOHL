// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Body › Generators: the generators of the file (Edit, Rebuild,
//! Remove…, Rebuild all) and a form per generator kind, whose result is
//! drawn in the view before it is applied. Apply is one undoable edit.

use eframe::egui;
use ohl_structure_edit::body::generators;
use ohl_structure_edit::{
    ArmShape, Bracing, CopyFace, Crossarm, Face, Generator, Kind, Panel, Panels, Peak, Plan,
    Redundants, Report, apply, generate, next_id, rebuild, rebuild_all, remove_generator,
};
use ohl_structures::StructureType;

use crate::app::{StructureApp, error_label};

/// A generator being edited.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    /// Its id.
    pub id: String,
    /// A generator of the file (its id stays).
    pub existing: bool,
    /// What it builds, with its inputs.
    pub kind: Kind,
}

/// The kinds offered, in order: a key and a name.
const KINDS: [(&str, &str); 4] = [
    ("panels", "legs and panels"),
    ("crossarm", "crossarms"),
    ("peak", "peak"),
    ("copy_face", "copy a face"),
];

/// Off the structure's axis by more than this, m: a point an arm reaches
/// (on it: a peak's, an earth wire's).
const OFF_AXIS: f64 = 0.1;

/// Whether point `p` is off the structure's axis (an arm's).
fn off_axis(p: &ohl_structures::TypePoint) -> bool {
    let [x, y, _] = p.position();
    x.si().hypot(y.si()) > OFF_AXIS
}

/// The levels of the points arms reach above the feet, bottom to top
/// (those within 0.5 m of each other one level).
fn arm_levels(t: &StructureType) -> Vec<f64> {
    let foot = t.foot_level();
    let mut zs: Vec<f64> = t
        .points
        .iter()
        .filter(|p| off_axis(p))
        .map(|p| p.position()[2].si())
        .filter(|z| *z > foot + 1.0)
        .collect();
    zs.sort_by(f64::total_cmp);
    let mut levels: Vec<f64> = Vec::new();
    for z in zs {
        if levels.last().is_none_or(|l| z - l > 0.5) {
            levels.push(z);
        }
    }
    levels
}

/// The inputs a new generator of kind `key` starts with, from `t`: arms
/// to the points off the axis, a peak to those on it (the earth wires),
/// the body up to the highest arm.
fn default_kind(t: &StructureType, key: &str) -> Kind {
    let free = |p: &&ohl_structures::TypePoint| p.node.is_none() && p.member.is_none();
    match key {
        "crossarm" => Kind::Crossarm(Crossarm {
            points: t
                .points
                .iter()
                .filter(free)
                .filter(|p| off_axis(p))
                .map(|p| p.label.clone())
                .collect(),
            shape: ArmShape::Triangular,
            one_arm: false,
        }),
        "peak" => Kind::Peak(Peak {
            points: t
                .points
                .iter()
                .filter(free)
                .filter(|p| !off_axis(p) && p.position()[2].si() > t.foot_level() + 1.0)
                .map(|p| p.label.clone())
                .collect(),
            height: 4.0,
        }),
        "copy_face" => Kind::CopyFace(CopyFace {
            body: generators(t)
                .unwrap_or_default()
                .into_iter()
                .find(|g| matches!(g.kind, Kind::Panels(_)))
                .map_or_else(|| "body".into(), |g| g.id),
            face: Face::YMinus,
        }),
        _ => {
            // Four panels up to the lowest arm (the waist), then a panel
            // to each arm level above it at the waist's width; without
            // arms, four panels to the height.
            let foot = t.foot_level();
            let levels = arm_levels(t);
            let waist = [2.0, 2.0];
            let lowest = levels
                .first()
                .copied()
                .unwrap_or_else(|| t.height.map_or(foot + 30.0, |h| h.si()));
            let mut panels: Vec<Panel> = (1..=4)
                .map(|k| Panel {
                    top: round3(foot + (lowest - foot) * f64::from(k) / 4.0),
                    bracing: Bracing::X,
                    horizontal: true,
                    ..Default::default()
                })
                .collect();
            if levels.len() > 1 {
                if let Some(p) = panels.last_mut() {
                    p.width = Some(waist);
                }
                for z in &levels[1..] {
                    panels.push(Panel {
                        top: round3(*z),
                        bracing: Bracing::X,
                        horizontal: true,
                        width: Some(waist),
                        ..Default::default()
                    });
                }
            }
            Kind::Panels(Panels {
                bottom: None,
                feet: [6.0, 6.0],
                waist,
                panels,
                ..Default::default()
            })
        }
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// A free generator id like `like`.
fn free_id(t: &StructureType, like: &str) -> String {
    let taken: Vec<String> = generators(t)
        .unwrap_or_default()
        .into_iter()
        .map(|g| g.id)
        .collect();
    if taken.iter().any(|x| x == like) {
        next_id(taken.iter().map(String::as_str), like)
    } else {
        like.to_owned()
    }
}

fn length(ui: &mut egui::Ui, v: &mut f64) -> egui::Response {
    ui.add(
        egui::DragValue::new(v)
            .speed(0.05)
            .max_decimals(3)
            .suffix(" m"),
    )
}

/// Checkboxes for the points of `t` (the labels ticked are in `chosen`).
fn points_ui(ui: &mut egui::Ui, t: &StructureType, chosen: &mut Vec<String>) {
    ui.horizontal_wrapped(|ui| {
        for p in &t.points {
            let mut on = chosen.contains(&p.label);
            if ui.checkbox(&mut on, &p.label).changed() {
                if on {
                    chosen.push(p.label.clone());
                } else {
                    chosen.retain(|l| l != &p.label);
                }
            }
        }
        if t.points.is_empty() {
            ui.label(egui::RichText::new("no point yet (Geometry)").weak());
        }
    });
}

/// The inputs of `kind`.
fn kind_ui(ui: &mut egui::Ui, t: &StructureType, kind: &mut Kind) {
    match kind {
        Kind::Panels(p) => {
            ui.horizontal_wrapped(|ui| {
                ui.label("feet: across");
                length(ui, &mut p.feet[0]).on_hover_text("Width at the feet across the line (x)");
                ui.label("along");
                length(ui, &mut p.feet[1]).on_hover_text("Width at the feet along the line (y)");
                ui.separator();
                ui.label("waist: across");
                length(ui, &mut p.waist[0]).on_hover_text("Width at the top of the last panel");
                ui.label("along");
                length(ui, &mut p.waist[1]);
                ui.separator();
                let mut own = p.bottom.is_some();
                if ui
                    .checkbox(&mut own, "feet at")
                    .on_hover_text("The level of the feet (else the file's foot level)")
                    .changed()
                {
                    p.bottom = own.then(|| t.foot_level());
                }
                if let Some(b) = &mut p.bottom {
                    length(ui, b);
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("plan bracing where the slope changes and at the crossarms' levels:");
                plan_combo(ui, "plan-auto", &mut p.plan_auto);
            })
            .response
            .on_hover_text(
                "At the levels where a panel's width changes the slope, and at the two levels \
                 each crossarm generator of the file starts from (the body is built again by \
                 itself when a crossarm is added). A panel's own plan bracing goes first.",
            );
            let mut drop = None;
            egui::Grid::new("panels-form").striped(true).show(ui, |ui| {
                for (h, tip) in [
                    ("panel", ""),
                    ("top", "Level of the panel's top (z)"),
                    ("width", "The body's width at the panel's top, across and along (none: on the line between the widths below and above; the last panel's is the waist). The slope can change there."),
                    ("bracing", "single diagonal, X, K (to the middle of the top), diamond, Warren (alternating)"),
                    ("horizontals", "Horizontals at the panel's top (K and diamond write those they need)"),
                    ("redundants", "Secondary members in the faces: none, one level (the main diagonals cut at their middle or crossing, braced to the legs' middles) or two (the halves cut again)"),
                    ("plan", "Plan bracing at the panel's top: X (corner to corner) or diamond (between the horizontals' middles, with a cross to the centre)"),
                    ("", ""),
                ] {
                    crate::style::header(ui, h, tip);
                }
                ui.end_row();
                let n = p.panels.len();
                for (k, panel) in p.panels.iter_mut().enumerate() {
                    ui.label((k + 1).to_string());
                    length(ui, &mut panel.top);
                    if k + 1 < n {
                        ui.horizontal(|ui| {
                            let mut own = panel.width.is_some();
                            if ui.checkbox(&mut own, "").changed() {
                                panel.width = own.then_some([2.0, 2.0]);
                            }
                            if let Some(w) = &mut panel.width {
                                length(ui, &mut w[0]).on_hover_text("across");
                                length(ui, &mut w[1]).on_hover_text("along");
                            }
                        });
                    } else {
                        ui.label("waist");
                    }
                    egui::ComboBox::from_id_salt(("bracing", k))
                        .selected_text(panel.bracing.key())
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            for b in Bracing::ALL {
                                ui.selectable_value(&mut panel.bracing, b, b.key());
                            }
                        });
                    ui.checkbox(&mut panel.horizontal, "");
                    egui::ComboBox::from_id_salt(("redundants", k))
                        .selected_text(panel.redundants.key())
                        .width(60.0)
                        .show_ui(ui, |ui| {
                            for r in Redundants::ALL {
                                ui.selectable_value(&mut panel.redundants, r, r.key());
                            }
                        });
                    plan_combo(ui, ("plan", k), &mut panel.plan);
                    if ui
                        .add_enabled(n > 1, egui::Button::new("×"))
                        .on_hover_text("Remove this panel")
                        .clicked()
                    {
                        drop = Some(k);
                    }
                    ui.end_row();
                }
            });
            if let Some(k) = drop {
                p.panels.remove(k);
            }
            if ui
                .button("+ Panel")
                .on_hover_text("A panel on top, as high as the one below")
                .clicked()
            {
                let last = p.panels.last().cloned();
                let below = p
                    .panels
                    .iter()
                    .rev()
                    .nth(1)
                    .map_or(p.bottom.unwrap_or_else(|| t.foot_level()), |x| x.top);
                let mut panel = last.unwrap_or(Panel {
                    top: below,
                    bracing: Bracing::X,
                    horizontal: true,
                    ..Default::default()
                });
                panel.top += (panel.top - below).max(1.0);
                panel.width = None;
                p.panels.push(panel);
            }
        }
        Kind::Crossarm(c) => {
            ui.horizontal(|ui| {
                ui.label("shape");
                for s in [ArmShape::Triangular, ArmShape::Rectangular] {
                    ui.selectable_value(&mut c.shape, s, s.key()).on_hover_text(match s {
                        ArmShape::Triangular => "Four chords from the body face to one tip at the point",
                        ArmShape::Rectangular => "A bottom tip at the point, a top tip above it, a vertical and diagonals",
                    });
                }
            });
            ui.checkbox(&mut c.one_arm, "several points on one arm")
                .on_hover_text(
                    "The points on one side that start from the same body joints make one arm: \
                     the outermost at its tip, the others each on a section along it",
                );
            ui.label(if c.one_arm {
                "the points ticked, one arm per side and level, from the body face:"
            } else {
                "an arm to each point ticked, from the body face on its side:"
            });
            points_ui(ui, t, &mut c.points);
        }
        Kind::Peak(pk) => {
            ui.label("a tip at each earth-wire point ticked (none: one tip at the centre):");
            points_ui(ui, t, &mut pk.points);
            if pk.points.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("height above the top");
                    length(ui, &mut pk.height);
                });
            }
        }
        Kind::CopyFace(cf) => {
            let bodies: Vec<String> = generators(t)
                .unwrap_or_default()
                .into_iter()
                .filter(|g| matches!(g.kind, Kind::Panels(_)))
                .map(|g| g.id)
                .collect();
            ui.horizontal(|ui| {
                ui.label("face of");
                egui::ComboBox::from_id_salt("copy-body")
                    .selected_text(&cf.body)
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for b in &bodies {
                            ui.selectable_value(&mut cf.body, b.clone(), b);
                        }
                    });
                for f in Face::ALL {
                    ui.selectable_value(&mut cf.face, f, f.key());
                }
                ui.label("copied to the opposite face");
            });
            if bodies.is_empty() {
                ui.label(egui::RichText::new("no legs-and-panels generator yet").weak());
            }
        }
    }
}

/// A choice of plan bracing.
fn plan_combo(ui: &mut egui::Ui, salt: impl egui::AsIdSalt, plan: &mut Plan) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(plan.key())
        .width(70.0)
        .show_ui(ui, |ui| {
            for p in Plan::ALL {
                ui.selectable_value(plan, p, p.key());
            }
        });
}

/// What was clicked in the Generators table.
enum Action {
    RebuildAll,
    Rebuild(String),
    Remove(String),
    Apply(Generator),
}

/// What a build did, in a few words.
fn counts(r: &Report) -> String {
    let mut s = format!("{} joint(s), {} member(s)", r.joints, r.members);
    if r.removed > 0 {
        s.push_str(&format!(", {} removed", r.removed));
    }
    s
}

impl StructureApp {
    /// Body › Generators.
    pub(crate) fn generators_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let gens = match generators(t) {
            Ok(g) => g,
            Err(e) => {
                error_label(ui, e);
                return;
            }
        };
        let mut action = None;
        // The generators of the file.
        ui.horizontal_wrapped(|ui| {
            ui.label("New:");
            for (key, name) in KINDS {
                if ui.button(name).clicked() {
                    let like = match key {
                        "panels" => "body",
                        "crossarm" => "arm1",
                        "copy_face" => "face1",
                        _ => "peak",
                    };
                    self.body.form = Some(Form {
                        id: free_id(t, like),
                        existing: false,
                        kind: default_kind(t, key),
                    });
                }
            }
            ui.separator();
            if ui
                .add_enabled(!gens.is_empty(), egui::Button::new("Rebuild all"))
                .on_hover_text(
                    "Every generator built again from its inputs, in order (hand edits stay)",
                )
                .clicked()
            {
                action = Some(Action::RebuildAll);
            }
        });
        if !gens.is_empty() {
            egui::Grid::new("generators").striped(true).show(ui, |ui| {
                for h in ["generator", "kind", "joints", "members", ""] {
                    crate::style::header(ui, h, "");
                }
                ui.end_row();
                for g in &gens {
                    let mine = |m: &Option<String>| m.as_deref() == Some(g.id.as_str());
                    ui.label(&g.id);
                    ui.label(g.kind.name());
                    ui.label(
                        t.nodes
                            .iter()
                            .filter(|n| mine(&n.made_by))
                            .count()
                            .to_string(),
                    );
                    ui.label(
                        t.members
                            .iter()
                            .filter(|m| mine(&m.made_by))
                            .count()
                            .to_string(),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .button("Edit")
                            .on_hover_text("Change its inputs, see the result, apply")
                            .clicked()
                        {
                            self.body.form = Some(Form {
                                id: g.id.clone(),
                                existing: true,
                                kind: g.kind.clone(),
                            });
                        }
                        if ui
                            .button("Rebuild")
                            .on_hover_text(
                                "Build it again: what it made is replaced, hand edits stay",
                            )
                            .clicked()
                        {
                            action = Some(Action::Rebuild(g.id.clone()));
                        }
                        if ui
                            .button("Remove…")
                            .on_hover_text("Remove it and what it made (asks first)")
                            .clicked()
                        {
                            self.body.confirm_remove = Some(g.id.clone());
                        }
                    });
                    ui.end_row();
                }
            });
        }
        if let Some(id) = self.body.confirm_remove.clone() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(format!(
                    "Remove generator {id} and the joints and members it made? What a hand-made \
                     member or a point still uses stays, as made by hand."
                ));
                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() {
                        action = Some(Action::Remove(id.clone()));
                        self.body.confirm_remove = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.body.confirm_remove = None;
                    }
                });
            });
        }
        if let Some(g) = self.form_ui(ui, t, &gens) {
            action = Some(Action::Apply(g));
        }
        if let Some(a) = action {
            self.act(a);
        }
        if let Some((summary, kept)) = &self.body.report {
            ui.separator();
            ui.label(summary);
            if !kept.is_empty() {
                ui.label(format!(
                    "kept, as made by hand (something else uses them): {}",
                    kept.len()
                ));
                egui::ScrollArea::vertical()
                    .id_salt("kept")
                    .max_height(80.0)
                    .show(ui, |ui| {
                        for k in kept {
                            ui.label(k);
                        }
                    });
            }
        }
        if gens.is_empty() && self.body.form.is_none() {
            ui.label(
                egui::RichText::new(
                    "No generator yet: New › legs and panels builds the body from its widths and \
                     panel levels; crossarms and a peak follow on it.",
                )
                .weak(),
            );
        }
    }

    /// Does what was clicked: one undoable edit each.
    fn act(&mut self, a: Action) {
        let Some(f) = &mut self.file else {
            return;
        };
        let r = match &a {
            Action::RebuildAll => f.edit("rebuild all", rebuild_all),
            Action::Rebuild(id) => f.edit(&format!("rebuild {id}"), |doc, t| rebuild(doc, t, id)),
            Action::Remove(id) => Ok(f.edit(&format!("remove generator {id}"), |doc, t| {
                remove_generator(doc, t, id)
            })),
            Action::Apply(g) => f.edit(&format!("generator {}", g.id), |doc, t| apply(doc, t, g)),
        };
        match r {
            Ok(r) => {
                let what = match &a {
                    Action::Remove(id) => format!("removed {id}"),
                    _ => format!("built: {}", counts(&r)),
                };
                self.message = Some((what.clone(), false));
                self.body.report = Some((what, r.kept));
                // A crossarm added: a body bracing in plan at the arms'
                // levels (`plan_auto`) built again, so it knows the new
                // level (its own undo step).
                if let Action::Apply(g) = &a
                    && matches!(g.kind, Kind::Crossarm(_))
                {
                    let bodies: Vec<String> = generators(f.structure())
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|b| matches!(&b.kind, Kind::Panels(p) if p.plan_auto != Plan::None))
                        .map(|b| b.id)
                        .collect();
                    for id in bodies {
                        if let Err(e) = f.edit(
                            &format!("rebuild {id} (plan bracing at the arms)"),
                            |doc, t| rebuild(doc, t, &id),
                        ) {
                            self.message = Some((e, true));
                            return;
                        }
                    }
                }
                match &a {
                    Action::Apply(_) => self.body.form = None,
                    Action::Remove(id) if self.body.form.as_ref().is_some_and(|x| &x.id == id) => {
                        self.body.form = None;
                    }
                    _ => {}
                }
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    /// The generator being edited: its inputs, the preview; the generator
    /// to apply when Apply is clicked.
    fn form_ui(
        &mut self,
        ui: &mut egui::Ui,
        t: &StructureType,
        gens: &[Generator],
    ) -> Option<Generator> {
        let mut form = self.body.form.clone()?;
        let mut close = false;
        let mut apply_it = None;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(form.kind.name());
                ui.label("id");
                ui.add_enabled(
                    !form.existing,
                    egui::TextEdit::singleline(&mut form.id).desired_width(90.0),
                )
                .on_hover_text(
                    "The made_by of what it builds; its joints and members are named after it",
                );
            });
            kind_ui(ui, t, &mut form.kind);
            let g = Generator {
                id: form.id.trim().to_owned(),
                kind: form.kind.clone(),
            };
            let taken = !form.existing && gens.iter().any(|x| x.id == g.id);
            let built = if g.id.is_empty() {
                Err("give it an id".to_owned())
            } else if taken {
                Err(format!("generator {} exists already: another id", g.id))
            } else {
                generate(t, &g)
            };
            match &built {
                Ok(b) => {
                    self.body.preview = b.segments(t);
                    ui.label(
                        egui::RichText::new(format!(
                            "preview (in the view): {} new joint(s), {} member(s)",
                            b.joints.len(),
                            b.members.len()
                        ))
                        .color(egui::Color32::from_rgb(80, 230, 200)),
                    );
                }
                Err(e) => error_label(ui, e.as_str()),
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(built.is_ok(), egui::Button::new("Apply"))
                    .on_hover_text(
                        "Write it into the file (one undo step); a rebuild replaces only what it made",
                    )
                    .clicked()
                {
                    apply_it = Some(g.clone());
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
        self.body.form = (!close).then_some(form);
        if close {
            self.body.preview.clear();
        }
        apply_it
    }
}
