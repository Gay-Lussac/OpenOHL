// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Getting around the Detail view (owner 2026-11-03): the Edit tool in
//! the 3D view asks before it opens a tower's joint or member there; a
//! member opened from a joint goes back to it (‹ Joint, Escape); by
//! default the Detail view is a tab on top of the 3D view (owner
//! 2026-11-04: two panes), ‹ 3D view and Escape go back to it, its details
//! in front on the left with the text and edit panels; a bolt or plate
//! picked with the Detail view's Edit tool shows its properties at their
//! top — a bolt's with the bolt libraries to change it.

use eframe::egui;
use ohl_structure_edit::Handle;
use ohl_structure_edit::connection_edits::FieldValue;
use ohl_structure_link::JointDetails;

use crate::app::StructureApp;
use crate::joint_panel::Picked;
use crate::joint_tools::Act;
use crate::layout::Panel;
use crate::member_panel::Subject;

/// How to get around the Detail view (Help › Controls), for the
/// navigation scheme in force (View › Navigation).
pub fn controls(mouse: ohl_ui::navigation::Mouse) -> String {
    let close_up = if mouse == ohl_ui::navigation::Mouse::CAD {
        "Close-up (navigation CAD): Shift + middle drag turns, the wheel zooms, a double \
         middle-click turns it back (it has no pan)"
    } else {
        "Close-up: drag to turn, wheel to zoom"
    };
    format!("{CONTROLS_OPEN}{close_up}{CONTROLS_REST}")
}

const CONTROLS_OPEN: &str = "Opened from the 3D view: the Edit tool on a tower's joint or \
     member (asked first, unless the Detail view is open), or its right-click menu, Open in the \
     Detail view.\n";

const CONTROLS_REST: &str = "; Face view: the joint's plane flat.\n\
     Its toolbar
 below the drawing: Edit (a bolt, plate or member clicked: its properties \
     below), Add bolt, Move bolt, Remove bolt, Member end.\n\
     Escape: from a member back to its joint, else what is picked let go, else the Edit tool \
     again, else (over the drawing) back to the 3D view. ‹ Joint: back to the joint. ‹ 3D \
     view: the 3D view in front again (the Detail view a tab behind it).";

impl StructureApp {
    /// The details of what the Detail view shows brought to the front
    /// (on the left by default): Joint details for a joint, Member details
    /// for a member.
    pub(crate) fn focus_details(&mut self) {
        self.focus(match self.joints.subject {
            Subject::Joint => Panel::JointDetails,
            Subject::Member => Panel::MemberDetails,
        });
    }

    /// The joints the member view goes back to: the one it was opened
    /// from first, then the member's ends.
    pub(crate) fn back_joints(&self) -> Vec<String> {
        let mut out: Vec<String> = self.joints.back.iter().cloned().collect();
        if let (Some(f), Some(m)) = (&self.file, &self.joints.member)
            && let Some(m) = f.structure().members.iter().find(|x| &x.id == m)
        {
            for n in &m.nodes {
                if !out.contains(n) {
                    out.push(n.clone());
                }
            }
        }
        out
    }

    /// From the member back to joint `joint` (none: the one it was opened
    /// from, else its first end) in the Detail view, its details below.
    pub(crate) fn detail_back(&mut self, joint: Option<String>) {
        let Some(j) = joint.or_else(|| self.back_joints().into_iter().next()) else {
            return;
        };
        self.joints.back = None;
        self.joints.picked = None;
        self.joints.drag = None;
        if self.joints.joint.as_ref() != Some(&j) {
            self.joints.face = None;
        }
        self.joints.joint = Some(j.clone());
        self.joints.subject = Subject::Joint;
        self.selected = vec![Handle::Joint(j.clone())];
        self.scroll_to = Some(Handle::Joint(j.clone()));
        self.focus(Panel::JointDetails);
        self.message = Some((format!("joint {j} in the Detail view"), false));
    }

    /// Escape for the Detail view, when the pointer is over it or it or
    /// its details are in front: from a member back to the joint, else
    /// what was picked let go, else its tool back to Edit. False: nothing
    /// of the Detail view's to undo (Escape then lets go of the 3D view's
    /// tool).
    pub(crate) fn detail_escape(&mut self, ctx: &egui::Context) -> bool {
        let over = self
            .joints
            .view_rect
            .zip(ctx.pointer_hover_pos())
            .is_some_and(|(r, p)| r.contains(p));
        let front = matches!(
            self.focused(),
            Some(Panel::JointView | Panel::JointDetails | Panel::MemberDetails)
        );
        if !(over || front) || !crate::layout::is_shown(&self.dock, Panel::JointView) {
            return false;
        }
        if self.joints.subject == Subject::Member {
            self.detail_back(None);
            return true;
        }
        if self.joints.picked.take().is_some() {
            return true;
        }
        if self.joints.tool != crate::detail_tools::DetailTool::Edit {
            self.joints.tool = crate::detail_tools::DetailTool::Edit;
            self.joints.drag = None;
            return true;
        }
        // Nothing more of its own: back to the 3D view when it is the tab
        // under it — with the pointer over the drawing or the Detail view
        // itself in front (not from its details).
        let view_front = self.focused() == Some(Panel::JointView);
        (over || view_front) && self.back_to_3d()
    }

    /// The 3D view in front again when the Detail view is on top of it (a
    /// tab of one pane, the default); false: they are apart (nothing to
    /// do).
    pub(crate) fn back_to_3d(&mut self) -> bool {
        if !crate::layout::same_leaf(&self.dock, Panel::JointView, Panel::View3d) {
            return false;
        }
        self.focus(Panel::View3d);
        true
    }

    /// Above the Detail view, when it is on top of the 3D view: the way
    /// back to it (Escape too).
    pub(crate) fn detail_header(&mut self, ui: &mut egui::Ui) {
        if !crate::layout::same_leaf(&self.dock, Panel::JointView, Panel::View3d) {
            return;
        }
        if ui
            .small_button("‹ 3D view")
            .on_hover_text(
                "Back to the 3D view (the Detail view stays open behind it, its tab beside the 3D \
                 view's; Escape too)",
            )
            .clicked()
        {
            self.back_to_3d();
        }
    }

    /// `h` (a tower's joint or member) in the Detail view, in front, its
    /// details in front on the left.
    pub fn open_in_detail(&mut self, h: Handle) {
        self.selected = vec![h.clone()];
        self.scroll_to = Some(h.clone());
        self.joints.picked = None;
        match h {
            Handle::Joint(id) => {
                if self.joints.joint.as_ref() != Some(&id) {
                    self.joints.face = None;
                }
                self.joints.joint = Some(id);
                self.joints.subject = Subject::Joint;
                self.focus(Panel::JointDetails);
            }
            Handle::Member(id) => {
                // Opened from the 3D view: back to the joint shown before.
                self.joints.back = self.joints.joint.clone();
                self.joints.member = Some(id);
                self.joints.subject = Subject::Member;
                self.focus(Panel::MemberDetails);
            }
            _ => {}
        }
    }

    /// The Edit tool's question (a tower's joint or member clicked in the
    /// 3D view): open it in the Detail view, or stay (the Inspector).
    pub(crate) fn detail_question(&mut self, ctx: &egui::Context) {
        let Some(h) = self.joints.ask_open.clone() else {
            return;
        };
        let what = match &h {
            Handle::Joint(id) => format!("joint {id}"),
            Handle::Member(id) => format!("member {id}"),
            other => other.describe(),
        };
        let mut open = true;
        let mut answer = None;
        egui::Window::new("Open in the Detail view?")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .default_pos(ctx.content_rect().center() - egui::vec2(170.0, 60.0))
            .show(ctx, |ui| {
                crate::help::tool_intro(
                    ui,
                    &format!(
                        "Open {what} in the Detail view, or stay with its fields in the Inspector."
                    ),
                    "The Detail view draws a tower's joint with its bolts and plates, its details \
                     on the left, and has its own tools (Add bolt, Move bolt, Member end…). \
                     Stay: the object stays selected, its fields in the Inspector. With the \
                     Detail view open already, Edit shows it there without asking.",
                    &crate::tools::Tool::Edit.guide(),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button(egui::RichText::new("Open the Detail view").strong())
                        .clicked()
                    {
                        answer = Some(true);
                    }
                    if ui
                        .button("Stay")
                        .on_hover_text("Escape: its fields in the Inspector only")
                        .clicked()
                    {
                        answer = Some(false);
                    }
                });
            });
        match answer {
            Some(true) => {
                self.joints.ask_open = None;
                self.open_in_detail(h);
                self.message = Some((format!("{what} in the Detail view"), false));
            }
            Some(false) => self.joints.ask_open = None,
            None if !open => self.joints.ask_open = None,
            None => {}
        }
    }

    /// What the Edit tool picked in the Detail view (a bolt, a plate): its
    /// properties, at the top of the details.
    pub(crate) fn picked_box(&mut self, ui: &mut egui::Ui, details: &JointDetails) {
        let Some(p) = self.joints.picked.clone() else {
            return;
        };
        let acts = match &p {
            Picked::Bolt {
                member,
                joint,
                index,
            } => self.bolt_box(ui, details, member, joint, *index),
            Picked::Plate(id) => {
                let t = self.file.as_ref().map(|f| f.structure_arc());
                let usage = self.plate_usage_rows(id);
                match t {
                    Some(t) => plate_box(ui, &t, details, id, &mut self.buffers, &usage),
                    None => None,
                }
            }
        };
        match acts {
            Some(acts) => self.apply_joint_acts(acts, details),
            None => self.joints.picked = None,
        }
        ui.add_space(4.0);
    }

    /// Plate `id`'s checks of the last run: per part its largest usage
    /// over the cases, that case and what governs (none: no run).
    fn plate_usage_rows(&self, id: &str) -> Vec<crate::plate_outline_ui::UsageRow> {
        let Some(run) = self.results.run() else {
            return Vec::new();
        };
        let mut rows: Vec<crate::plate_outline_ui::UsageRow> = Vec::new();
        for c in &run.cases {
            let Some(Ok(r)) = c.fe.as_ref() else {
                continue;
            };
            for x in r.plates.iter().filter(|x| x.plate == id) {
                let part = x.part.words();
                match rows.iter_mut().find(|row| row.0 == part) {
                    Some(row) => {
                        if let Some(u) = x.usage
                            && row
                                .1
                                .is_none_or(|b| ohl_structure_link::usage_order(u, b).is_gt())
                        {
                            *row = (part, Some(u), c.name.clone(), x.governs.clone());
                        }
                    }
                    None => rows.push((part, x.usage, c.name.clone(), x.governs.clone())),
                }
            }
        }
        rows
    }

    /// A bolt's properties, its kind changed from the bolt libraries; none:
    /// closed (or gone).
    fn bolt_box(
        &mut self,
        ui: &mut egui::Ui,
        details: &JointDetails,
        member: &str,
        joint: &str,
        index: usize,
    ) -> Option<Vec<Act>> {
        let a = details
            .joint(joint)?
            .arrivals
            .iter()
            .find(|a| a.member == member)?;
        let b = a.bolts.value.clone()?;
        let place = *a.bolt_flat.get(index)?;
        let lib = self.bolt_library();
        let rules = self
            .bolt_rules_of(&[(member.to_owned(), joint.to_owned())])
            .remove(&(member.to_owned(), joint.to_owned()));
        let mut acts = Vec::new();
        let mut close = false;
        let mm = |v: f64| format!("{:.1} mm", v * 1000.0);
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!(
                    "Bolt {} of {} — {member} at {joint}",
                    index + 1,
                    a.bolt_flat.len()
                ));
                if ui.small_button("Close").clicked() {
                    close = true;
                }
            });
            ui.label(format!(
                "{} along the member from the work point, {} across the leg from the heel",
                mm(place[0]),
                mm(place[1])
            ));
            ui.label(format!(
                "M{:.0} · hole {} · grade {} ({})",
                b.diameter * 1000.0,
                b.hole.map_or("not given".into(), mm),
                b.grade.clone().unwrap_or_else(|| "not given".into()),
                a.bolts.source.words()
            ));
            // What it clamps (2026-11-05: through both legs of a lap joint,
            // the leg and the covers of a splice).
            ui.label(format!(
                "Clamps {} + {} = {} (its leg{}, then {}){}",
                mm(a.grip[0]),
                mm(a.grip[1]),
                mm(a.grip[0] + a.grip[1]),
                if a.grip[0]
                    > a.profile
                        .as_ref()
                        .and_then(|p| p.angle)
                        .map_or(f64::MAX, |g| g.t)
                        + 1e-9
                {
                    " and the inner covers"
                } else {
                    ""
                },
                a.onto.value.as_deref().unwrap_or("what it bolts onto"),
                match &a.planes {
                    (v, _) if v.value.is_some_and(|n| n >= 2) => {
                        format!(" · {} shear planes", v.value.unwrap_or(1))
                    }
                    _ => String::new(),
                }
            ))
            .on_hover_text(
                "The bolt's grip: the thickness of the leg bolted (and a splice's inner covers \
                 on it), then of what it bolts onto (the other member's leg, a plate, a \
                 splice's outer covers)",
            );
            // The bolt libraries: a member end's bolts are of one kind.
            let mut chosen = b.bolt.clone();
            ui.horizontal_wrapped(|ui| {
                ui.label("Bolt of the library");
                egui::ComboBox::from_id_salt(("picked-bolt-library", member, joint))
                    .selected_text(chosen.clone().unwrap_or_else(|| "typed here".into()))
                    .height(320.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, None, "typed here");
                        for x in &lib.0.bolts {
                            ui.selectable_value(&mut chosen, Some(x.id.clone()), x.title())
                                .on_hover_text(&x.id);
                        }
                        if lib.0.bolts.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "no bolt library (File › Import › Bolt libraries)",
                                )
                                .weak(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(
                        "The bolts of this member end are all of one kind: the bolt chosen is \
                         theirs (the bolt libraries: Bolts/ beside the structure). One undo step.",
                    );
            });
            if chosen != b.bolt {
                let value = match &chosen {
                    Some(id) => FieldValue::Bolts {
                        count: b.count,
                        bolt: Some(id.clone()),
                        diameter: None,
                        hole: None,
                        grade: None,
                    },
                    None => FieldValue::Bolts {
                        count: b.count,
                        bolt: None,
                        diameter: Some(b.diameter),
                        hole: b.hole,
                        grade: b.grade.clone(),
                    },
                };
                acts.push(Act::Field {
                    member: member.to_owned(),
                    joint: joint.to_owned(),
                    field: "bolts",
                    value: Some(value),
                });
            }
            // The library bolt's own values.
            if let Some(x) = b.bolt.as_deref().and_then(|id| lib.0.get(id)) {
                egui::Grid::new(("picked-bolt-values", member, joint))
                    .striped(true)
                    .show(ui, |ui| {
                        let mut row = |k: &str, v: Option<String>| {
                            if let Some(v) = v {
                                ui.label(k);
                                ui.label(v);
                                ui.end_row();
                            }
                        };
                        let kn = |f: f64| format!("{:.1} kN", f / 1000.0);
                        let mpa = |s: f64| format!("{:.0} MPa", s / 1e6);
                        row("Library bolt", Some(format!("{} ({})", x.title(), x.id)));
                        row("Diameter", Some(mm(x.diameter.si())));
                        row("Hole", x.hole.map(|h| mm(h.si())));
                        row("Grade", x.grade.clone());
                        row("Ultimate strength", x.ultimate.map(|s| mpa(s.si())));
                        row("Yield strength", x.yield_strength.map(|s| mpa(s.si())));
                        row(
                            "Shear per plane, threads in",
                            x.shear_capacity.map(|f| kn(f.si())),
                        );
                        row(
                            "Shear per plane, threads out",
                            x.shear_capacity_shank.map(|f| kn(f.si())),
                        );
                        row("Tension", x.tension_capacity.map(|f| kn(f.si())));
                        row("Grip", x.grip.map(|g| mm(g.si())));
                        row("Mass", x.mass_kg.map(|m| format!("{m:.3} kg")));
                        row("Description", x.description.clone());
                    });
            } else if let Some(id) = &b.bolt {
                crate::app::error_label(ui, format!("bolt {id}: in no bolt library"));
            }
            // The design code's distances at this bolt.
            match &rules {
                Some(r) => {
                    let mine: Vec<String> =
                        ohl_structure_link::breaches(r, &a.bolt_flat, a.cut.value.unwrap_or(0.0))
                            .into_iter()
                            .filter(|x| x.bolt == index)
                            .map(|x| x.what)
                            .collect();
                    if mine.is_empty() {
                        ui.colored_label(
                            crate::app::ok_color(ui),
                            egui::RichText::new(format!("keeps {}", r.words())).small(),
                        );
                    } else {
                        crate::app::error_label(ui, mine.join("; "));
                    }
                }
                None => {
                    ui.label(
                        egui::RichText::new(
                            "The capacity method has no minimum bolt distances (the simple check).",
                        )
                        .small()
                        .weak(),
                    );
                }
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Bolt layout…")
                    .on_hover_text("All the bolts of this member end in one go")
                    .clicked()
                {
                    acts.push(Act::OpenLayout(member.to_owned(), joint.to_owned()));
                }
                if ui
                    .button("Remove this bolt")
                    .on_hover_text("The end's bolts become their places, this one left out")
                    .clicked()
                {
                    let mut pts = a.bolt_flat.clone();
                    pts.remove(index);
                    let n = pts.len() as u32;
                    acts.push(Act::BoltPoints(
                        member.to_owned(),
                        joint.to_owned(),
                        pts,
                        Some(FieldValue::Bolts {
                            count: n,
                            bolt: b.bolt.clone(),
                            diameter: b.bolt.is_none().then_some(b.diameter),
                            hole: if b.bolt.is_none() { b.hole } else { None },
                            grade: if b.bolt.is_none() {
                                b.grade.clone()
                            } else {
                                None
                            },
                        }),
                    ));
                    close = true;
                }
            });
        });
        (!close).then_some(acts)
    }
}

/// A plate's properties: the plates it is welded to, its thickness and
/// material, what it lies on, the edge distances it keeps from its bolts
/// and what breaks them.
fn plate_box(
    ui: &mut egui::Ui,
    t: &ohl_structures::StructureType,
    details: &JointDetails,
    id: &str,
    buffers: &mut std::collections::HashMap<String, String>,
    usage: &[crate::plate_outline_ui::UsageRow],
) -> Option<Vec<Act>> {
    let (j, p) = details
        .joints
        .iter()
        .find_map(|j| j.plates.iter().find(|p| p.id == id).map(|p| (j, p)))?;
    let mut acts = Vec::new();
    let mut close = false;
    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("Plate {id} at joint {}", j.id));
            if ui.small_button("Close").clicked() {
                close = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("welded to");
            acts.extend(crate::joint_tools::welded_to_menu(ui, j, p));
            if p.assembly.len() > 1 {
                ui.label(format!("one welded plate: {}", p.assembly.join(" + ")))
                    .on_hover_text(
                        "Several plates welded together so that every member bolting at the \
                         joint reaches one; the seams drawn where they meet",
                    );
            }
            ui.label("thickness");
            let mut t = p.thickness * 1000.0;
            let r = ui.scope(|ui| {
                crate::style::field_visuals(ui);
                ui.add(
                    egui::DragValue::new(&mut t)
                        .range(1.0..=200.0)
                        .speed(0.5)
                        .suffix(" mm"),
                )
            });
            if r.inner.changed() && t > 0.0 {
                acts.push(Act::Plate(id.to_owned(), Some(t / 1000.0), None));
            }
        });
        // Its welds with their size and length (2026-11-04).
        acts.extend(crate::joint_tools::weld_rows(ui, t, j, Some(id), buffers));
        ui.label(format!(
            "{} · lies on {} · members onto it: {}",
            p.material.as_deref().unwrap_or("material not given"),
            p.leg
                .as_deref()
                .map_or("no angle leg (through the work point)".into(), |l| format!(
                    "the leg {l}"
                )),
            if p.members.is_empty() && p.outline.len() < 3 {
                format!(
                    "none: {} (set a member end's Onto to {id})",
                    ohl_structure_link::NO_OUTLINE
                )
            } else if p.members.is_empty() {
                "none yet (a member's Onto)".to_owned()
            } else {
                p.members.join(", ")
            }
        ));
        // Its outline corrected by hand, its edges and checks
        // (2026-11-04).
        acts.extend(crate::plate_outline_ui::outline_section(
            ui, j, p, buffers, usage,
        ));
    });
    (!close).then_some(acts)
}
