// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Body › Extensions: the height extensions, each with its `dz`, its own
//! joints and members (the piece below the feet: added by hand, or built
//! in one go with legs and bracing) and its leg extensions. The 3D view
//! shows the structure standing on the extension picked (its piece below
//! the feet, legs lowered, as the finite-element model has them). Every
//! change is one edit of the file (one undo step).

use eframe::egui;
use ohl_structure_edit::extension_edits::{self as ext, PieceBracing};
use ohl_structure_edit::{Handle, StructureFile, edits};
use ohl_structures::StructureType;
use ohl_structures::extensions::LEGS;

use crate::app::StructureApp;

/// The Extensions panel's state between frames.
#[derive(Default)]
pub struct ExtensionsState {
    /// The extension the 3D view stands the structure on (none: as
    /// written).
    pub shown: Option<String>,
    /// The extension being edited.
    pub picked: Option<String>,
    /// Bracing of a piece built in one go.
    pub bracing: PieceBracing,
    /// Horizontals at the bottom of the piece's faces.
    pub horizontals: bool,
    /// The leg chosen for a new leg extension.
    pub leg: String,
    /// Its `dz`, m.
    pub leg_dz: f64,
}

/// While the pointer is pressed on `r` its changes are one undo step.
fn hold(file: &mut StructureFile, r: &egui::Response, what: &str) {
    if r.is_pointer_button_down_on() {
        file.begin(what);
    }
}

impl StructureApp {
    /// Body › Extensions.
    pub(crate) fn extensions_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        let st = &mut self.ext;
        if st.leg.is_empty() {
            st.leg = "A".into();
            st.leg_dz = 1.0;
        }
        if st.picked.as_ref().is_none_or(|n| t.extension(n).is_none()) {
            st.picked = t.extensions.first().map(|e| e.name.clone());
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("extension").on_hover_text(
                "Height extensions: below the feet; placed, everything above them rises by dz",
            );
            for e in &t.extensions {
                let on = st.picked.as_deref() == Some(e.name.as_str());
                let what = if e.changes_model() {
                    format!("{} ({} m, body)", e.name, e.dz.si())
                } else {
                    format!("{} ({} m)", e.name, e.dz.si())
                };
                if ui.selectable_label(on, what).clicked() {
                    st.picked = Some(e.name.clone());
                }
            }
            if ui
                .button("+ Extension")
                .on_hover_text("A new extension (+3 m, +6 m…)")
                .clicked()
            {
                let next = (1..)
                    .map(|k| format!("+{}", 3 * k))
                    .find(|n| t.extension(n).is_none())
                    .expect("a free name");
                let dz: f64 = next[1..].parse().unwrap_or(3.0);
                f.edit("add extension", |doc, _| {
                    edits::set_extension(doc, &next, dz)
                });
                st.picked = Some(next);
            }
        });
        let Some(name) = st.picked.clone() else {
            ui.label(
                egui::RichText::new(
                    "No extension yet. At levels 1 and 2 a name and a dz are enough; at level 3 \
                     each gets its piece below the feet (its own joints and members).",
                )
                .weak(),
            );
            return;
        };
        let Some(e) = t.extension(&name).cloned() else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            let mut show = st.shown.as_deref() == Some(name.as_str());
            if ui
                .checkbox(&mut show, "show in 3D")
                .on_hover_text(
                    "The structure standing on this extension: its piece below the feet, legs \
                     lowered (as the finite-element model has them)",
                )
                .changed()
            {
                st.shown = show.then(|| name.clone());
            }
            ui.label("dz");
            let mut dz = e.dz.si();
            let r = ui.add(
                egui::DragValue::new(&mut dz)
                    .speed(0.05)
                    .range(0.0..=60.0)
                    .max_decimals(3)
                    .suffix(" m"),
            );
            hold(f, &r, "extension");
            if r.changed() {
                f.edit("extension", |doc, _| edits::set_extension(doc, &name, dz));
            }
            if ui
                .small_button("🗑 Delete extension")
                .on_hover_text("This extension with its piece and legs (Undo brings it back)")
                .clicked()
            {
                f.edit("delete extension", |doc, _| {
                    edits::remove_extension(doc, &name)
                });
                st.picked = None;
            }
        });
        ui.separator();
        // The piece in one go.
        ui.horizontal_wrapped(|ui| {
            ui.strong("piece").on_hover_text(
                "Under every foot of the body (its supported joints, else its lowest) a joint dz \
                 lower along the foot's leg, with the foot's support; a leg member to the foot \
                 and the faces braced. Replaces the extension's own joints and members",
            );
            egui::ComboBox::from_id_salt("piece-bracing")
                .selected_text(st.bracing.name())
                .width(120.0)
                .show_ui(ui, |ui| {
                    for b in PieceBracing::ALL {
                        ui.selectable_value(&mut st.bracing, b, b.name());
                    }
                });
            ui.checkbox(&mut st.horizontals, "horizontals");
            if ui.button("Build the piece").clicked() {
                let (bracing, horizontals) = (st.bracing, st.horizontals);
                self.message = Some(
                    match f.edit(&format!("piece of extension {name}"), |doc, t| {
                        ext::build_piece(doc, t, &name, bracing, horizontals)
                    }) {
                        Ok(p) => {
                            st.shown = Some(name.clone());
                            (
                                format!(
                                    "extension {name}: {} joint(s) and {} member(s){}",
                                    p.joints,
                                    p.members,
                                    if p.replaced > 0 {
                                        format!(" (replacing {})", p.replaced)
                                    } else {
                                        String::new()
                                    }
                                ),
                                false,
                            )
                        }
                        Err(e) => (e, true),
                    },
                );
            }
        });
        // By hand: a joint, a member between two picked joints.
        let picked: Vec<String> = self
            .selected
            .iter()
            .filter_map(|h| match h {
                Handle::Joint(id) => Some(id.clone()),
                _ => None,
            })
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.strong("by hand");
            if ui
                .add(crate::icons::Icon::AddJoint.button(ui, "Joint"))
                .on_hover_text(
                    "A joint at the bottom of the piece, under the centre (then move it)",
                )
                .clicked()
            {
                let at = [0.0, 0.0, t.foot_level() - e.dz.si()];
                match f.edit(&format!("joint of extension {name}"), |doc, t| {
                    ext::add_joint(doc, t, &name, at)
                }) {
                    Ok(id) => self.selected = vec![Handle::Joint(id)],
                    Err(e) => self.message = Some((e, true)),
                }
            }
            let two = picked.len() == 2;
            if ui
                .add_enabled(
                    two,
                    crate::icons::Icon::AddMember.button(ui, "Member between the 2 picked joints"),
                )
                .on_disabled_hover_text(
                    "Pick two joints (Shift + click), one of this extension at least (show it in \
                     3D)",
                )
                .clicked()
            {
                match f.edit(&format!("member of extension {name}"), |doc, t| {
                    ext::add_member(doc, t, &name, &picked[0], &picked[1])
                }) {
                    Ok(id) => self.selected = vec![Handle::Member(id)],
                    Err(e) => self.message = Some((e, true)),
                }
            }
        });
        // Leg extensions.
        let st = &mut self.ext;
        ui.strong("legs").on_hover_text(
            "One leg longer than the others: its support goes dz lower along its leg member. \
             A: x > 0, y > 0; B: x < 0, y > 0; C: x < 0, y < 0; D: x > 0, y < 0 — or a joint id",
        );
        let mut drop: Option<String> = None;
        egui::Grid::new("ext-legs").striped(true).show(ui, |ui| {
            for l in &e.legs {
                ui.label(format!("leg {}", l.leg));
                let mut dz = l.dz.si();
                let r = ui.add(
                    egui::DragValue::new(&mut dz)
                        .speed(0.05)
                        .range(0.001..=20.0)
                        .max_decimals(3)
                        .suffix(" m"),
                );
                hold(f, &r, "leg");
                if r.changed() {
                    let _ = f.edit("leg", |doc, _| ext::set_leg(doc, &name, &l.leg, Some(dz)));
                }
                if ui
                    .small_button("🗑")
                    .on_hover_text("As long as the others")
                    .clicked()
                {
                    drop = Some(l.leg.clone());
                }
                ui.end_row();
            }
        });
        if let Some(l) = drop {
            let _ = f.edit("leg", |doc, _| ext::set_leg(doc, &name, &l, None));
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("leg-name")
                .selected_text(format!("leg {}", st.leg))
                .width(80.0)
                .show_ui(ui, |ui| {
                    for (l, _) in LEGS {
                        ui.selectable_value(&mut st.leg, l.to_owned(), format!("leg {l}"));
                    }
                });
            ui.add(
                egui::DragValue::new(&mut st.leg_dz)
                    .speed(0.05)
                    .range(0.001..=20.0)
                    .max_decimals(3)
                    .suffix(" m"),
            );
            if ui.button("+ Leg").clicked() {
                let (leg, dz) = (st.leg.clone(), st.leg_dz);
                if let Err(e) = f.edit("leg", |doc, _| ext::set_leg(doc, &name, &leg, Some(dz))) {
                    self.message = Some((e, true));
                }
            }
        });
        ui.separator();
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.strong(format!("joints ({})", e.nodes.len()));
            let mut drop: Option<String> = None;
            egui::Grid::new("ext-joints").striped(true).show(ui, |ui| {
                for h in ["joint", "x", "y", "z", "support", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for n in &e.nodes {
                    let h = Handle::Joint(n.id.clone());
                    if ui
                        .selectable_label(self.selected.contains(&h), &n.id)
                        .clicked()
                    {
                        self.selected = vec![h];
                    }
                    for v in n.point {
                        ui.label(format!("{:.3}", v.si()));
                    }
                    let now = n
                        .support
                        .as_ref()
                        .and_then(|v| v.as_str())
                        .unwrap_or("—")
                        .to_owned();
                    let mut chosen = now.clone();
                    egui::ComboBox::from_id_salt(("ext-support", &n.id))
                        .selected_text(&chosen)
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for s in ["—", "pinned", "fixed"] {
                                ui.selectable_value(&mut chosen, s.to_owned(), s);
                            }
                        });
                    if chosen != now {
                        let _ = f.edit("support", |doc, _| {
                            ext::set_field(
                                doc,
                                &name,
                                "node",
                                &n.id,
                                "support",
                                (chosen != "—").then_some(chosen.as_str()),
                            )
                        });
                    }
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Delete this joint and its members")
                        .clicked()
                    {
                        drop = Some(n.id.clone());
                    }
                    ui.end_row();
                }
            });
            if let Some(id) = drop {
                let gone = f.edit(&format!("delete joint {id}"), |doc, t| {
                    ext::remove_joint(doc, t, &name, &id)
                });
                if !gone.is_empty() {
                    self.message = Some((
                        format!("joint {id} deleted with member(s) {}", gone.join(", ")),
                        false,
                    ));
                }
            }
            let ui = &mut cols[1];
            ui.strong(format!("members ({})", e.members.len()));
            let mut drop: Option<String> = None;
            egui::Grid::new("ext-members").striped(true).show(ui, |ui| {
                for h in ["member", "from", "to", "section", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for m in &e.members {
                    let h = Handle::Member(m.id.clone());
                    if ui
                        .selectable_label(self.selected.contains(&h), &m.id)
                        .clicked()
                    {
                        self.selected = vec![h];
                    }
                    ui.label(&m.nodes[0]);
                    ui.label(&m.nodes[1]);
                    let key = format!("ext-section:{name}:{}", m.id);
                    let current = m.section.clone().unwrap_or_default();
                    let buf = self.buffers.entry(key.clone()).or_insert(current.clone());
                    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(90.0));
                    if r.lost_focus() && *buf != current {
                        let typed = buf.trim().to_owned();
                        let _ = f.edit("section", |doc, _| {
                            ext::set_field(
                                doc,
                                &name,
                                "member",
                                &m.id,
                                "section",
                                (!typed.is_empty()).then_some(typed.as_str()),
                            )
                        });
                        self.buffers.remove(&key);
                    } else if !r.has_focus() && *buf != current {
                        *buf = current;
                    }
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Delete this member")
                        .clicked()
                    {
                        drop = Some(m.id.clone());
                    }
                    ui.end_row();
                }
            });
            if let Some(id) = drop {
                f.edit(&format!("delete member {id}"), |doc, _| {
                    ext::remove_member(doc, &name, &id)
                });
            }
        });
        if let Err(err) = t.extended(Some(&name)) {
            crate::app::error_label(ui, err);
        }
    }
}
