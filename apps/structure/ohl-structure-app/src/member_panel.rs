// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Member details panel (owner 2026-11-02: "the same view of the
//! members as for the joints where we could define where the holes and
//! bolts are and that it would directly be shown on the joint side … see
//! live the impacts on the bar's resistance"): a tower's member — the one
//! selected — with
//!
//! - its two ends as rows of the connections table (the same
//!   `[[connection]]` data the joints show and edit: a value set here is
//!   the joint's), the bolt layout form and the presets;
//! - its ASCE 10-97 capacities worked out from them on every change, what
//!   governs, KL/r, what was assumed, and the usage under the last FE
//!   run's forces (the bolts and holes do not change the forces);
//! - in the Detail view (on top of the 3D view), its leg bolted laid flat with
//!   the holes and bolts of both ends; a member with no single leg to
//!   lay flat (a built-up section, an I or H, a tube) from the side
//!   instead, with its cross-section (2026-11-10).

use std::sync::Arc;

use eframe::egui;
use ohl_structure_edit::Handle;
use ohl_structure_link::{FlatLeg, MemberCapacity, MemberEnds};

use crate::app::StructureApp;

/// What the Detail view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Subject {
    /// The joint of Joint details.
    #[default]
    Joint,
    /// The member of Member details.
    Member,
}

/// The capacities worked out, for (file revision, libraries, member).
pub type CapacityCache = Option<((u64, u64, String), Arc<Result<MemberCapacity, String>>)>;

impl StructureApp {
    /// The joint or member shown follows what was selected last (in the
    /// 3D view, a table, a panel's list): a joint the Joint details and
    /// the Detail view, a member the Member details and the Detail view.
    pub(crate) fn follow_selection(&mut self) {
        match self
            .selected
            .iter()
            .rev()
            .find(|h| matches!(h, Handle::Joint(_) | Handle::Member(_)))
        {
            Some(Handle::Joint(id)) => {
                if self.joints.joint.as_ref() != Some(id) {
                    self.joints.joint = Some(id.clone());
                    self.joints.face = None;
                    self.joints.picked = None;
                }
                self.joints.subject = Subject::Joint;
            }
            Some(Handle::Member(id)) => {
                // From a joint: the way back to it (‹ Joint, Escape).
                if self.joints.subject == Subject::Joint {
                    self.joints.back = self.joints.joint.clone();
                    self.joints.picked = None;
                }
                self.joints.member = Some(id.clone());
                self.joints.subject = Subject::Member;
            }
            _ => {}
        }
    }

    /// The capacities of `member`, worked out again when the file, the
    /// libraries or the member change.
    pub(crate) fn member_capacity(&mut self, member: &str) -> Arc<Result<MemberCapacity, String>> {
        let Some(f) = self.file.as_ref() else {
            return Arc::new(Err("no structure open".into()));
        };
        let key = (f.revision(), self.libs_stamp, member.to_owned());
        if let Some((k, c)) = &self.joints.capacity
            && *k == key
        {
            return c.clone();
        }
        let bolts = self.bolt_library();
        let f = self.file.as_ref().expect("checked above");
        let c = Arc::new(ohl_structure_link::member_capacity(
            f.text_ref(),
            &self.loads.sections,
            &bolts.0,
            member,
        ));
        self.joints.capacity = Some((key, c.clone()));
        c
    }

    /// The Member details panel.
    pub(crate) fn member_panel_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("member-details-panel")
            .auto_shrink([false, false])
            .show(ui, |ui| self.member_panel_body(ui));
    }

    fn member_panel_body(&mut self, ui: &mut egui::Ui) {
        self.follow_selection();
        let Some(f) = &self.file else {
            ui.label("No structure open.");
            return;
        };
        let t = f.structure_arc();
        if !ohl_structure_edit::is_tower(t.kind) {
            ui.label(
                "Member details are a tower's (Geometry › Structure kind: a lattice tower or a \
                 portal).",
            );
            return;
        }
        if t.members.is_empty() {
            ui.label("No members yet: build the body first (Body › Generators).");
            return;
        }
        let Some(details) = self.joint_details() else {
            return;
        };
        let details = match details.as_ref() {
            Ok(d) => d,
            Err(e) => {
                crate::app::error_label(ui, format!("The joints could not be read: {e}"));
                return;
            }
        };
        let id = self
            .joints
            .member
            .clone()
            .filter(|m| t.members.iter().any(|x| &x.id == m))
            .unwrap_or_else(|| t.members[0].id.clone());
        let mut chosen = id.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label("Member");
            egui::ComboBox::from_id_salt("member-detail-member")
                .selected_text(&chosen)
                .height(320.0)
                .show_ui(ui, |ui| {
                    for m in &t.members {
                        ui.selectable_value(&mut chosen, m.id.clone(), &m.id);
                    }
                });
        });
        if chosen != id {
            self.selected = vec![Handle::Member(chosen.clone())];
            self.scroll_to = Some(Handle::Member(chosen.clone()));
            // The Detail view follows (2026-11-10): the member chosen
            // here, with the way back to the joint it showed.
            if self.joints.subject == Subject::Joint {
                self.joints.back = self.joints.joint.clone();
                self.joints.picked = None;
            }
            self.joints.subject = Subject::Member;
        }
        self.joints.member = Some(chosen.clone());
        let Some(m) = ohl_structure_link::member_ends(details, &chosen) else {
            ui.label(format!("Member {chosen}: its joints are not found."));
            return;
        };
        ui.label(format!(
            "{} · {:.3} m · from {} to {}",
            m.section
                .clone()
                .unwrap_or_else(|| "no cross-section".into()),
            m.length,
            m.ends[0].as_ref().map_or("—", |e| e.0.as_str()),
            m.ends[1].as_ref().map_or("—", |e| e.0.as_str()),
        ));
        if !details.level3 {
            ui.label(
                "The structure is not a full design yet (level 3): connections and capacities \
                 belong to level 3.",
            );
            return;
        }
        // Its roll, the correction angle about its axis (2026-11-04).
        let ends = crate::roll_ui::flat_ends(details, &chosen);
        let roll = crate::roll_ui::roll_of(&t, &chosen);
        if let Some(r) = crate::roll_ui::roll_control(ui, &chosen, roll, &ends) {
            self.apply_joint_acts(
                vec![crate::joint_tools::Act::Roll(chosen.clone(), r)],
                details,
            );
        }
        // What the Detail view's Edit tool picked (a bolt).
        self.picked_box(ui, details);
        self.capacity_box(ui, &chosen);
        ui.add_space(4.0);
        self.joint_forms(ui);
        let presets = self.presets();
        let rows: Vec<(
            &ohl_structure_link::JointDetail,
            &ohl_structure_link::Arrival,
        )> = m
            .ends
            .iter()
            .flatten()
            .filter_map(|(j, a)| details.joint(j).map(|jd| (jd, a)))
            .collect();
        let acts = crate::joint_tools::rows_table(
            ui,
            &t,
            details,
            &rows,
            true,
            &mut self.buffers,
            &presets,
        );
        ui.label(
            egui::RichText::new(
                "These are the member's two ends as their joints show them: a value set here is \
                 the joint's too. Each change is one undo step; the capacities above follow it.",
            )
            .small()
            .weak(),
        );
        self.apply_joint_acts(acts, details);
    }

    /// The capacities, what governs, KL/r, the usage under the last run's
    /// forces.
    fn capacity_box(&mut self, ui: &mut egui::Ui, member: &str) {
        let c = self.member_capacity(member);
        // The largest tension and compression of the last run, and their
        // cases.
        let mut most_t: Option<(f64, String)> = None;
        let mut most_c: Option<(f64, String)> = None;
        if let Some(run) = self.results.run() {
            for case in &run.cases {
                let Some(Ok(r)) = &case.fe else { continue };
                if let Some(x) = r.members.iter().find(|x| x.id == member) {
                    if x.axial > 0.0 && most_t.as_ref().is_none_or(|t| x.axial > t.0) {
                        most_t = Some((x.axial, case.name.clone()));
                    }
                    if x.axial < 0.0 && most_c.as_ref().is_none_or(|t| -x.axial > t.0) {
                        most_c = Some((-x.axial, case.name.clone()));
                    }
                }
            }
        }
        // The aid shows the member with its joint details; the run may
        // take the simple form (owner 2026-11-11): said.
        let simple = self.file.as_ref().is_some_and(|f| {
            let t = f.structure();
            !t.connections.is_empty() && !t.joint_details_taken()
        });
        // A plugin's method: its resistance comes with a run, and no
        // other code's is shown in its place.
        if let Err(e) = c.as_ref()
            && e.starts_with(ohl_structure_link::RESISTANCE_FROM_PLUGIN)
        {
            ui.group(|ui| {
                ui.strong("Resistance");
                ui.label(e);
            });
            return;
        }
        ui.group(|ui| {
            ui.strong("Resistance (ASCE 10-97, worked out live)");
            if simple {
                ui.label(
                    egui::RichText::new(
                        "With this member's joint details, as an aid to their design. The \
                         calculation (Results) checks the members in the simple form \u{2014} \
                         joint details not taken \u{2014} so its capacity and usage can differ.",
                    )
                    .weak(),
                );
            }
            match c.as_ref() {
                Err(e) => {
                    crate::app::error_label(ui, format!("Not worked out: {e}"));
                }
                Ok(c) => {
                    egui::Grid::new("member-capacity")
                        .num_columns(4)
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["", "Capacity", "Governs", "Usage (last run)"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for (what, cap, most) in [
                                ("Tension", &c.tension, &most_t),
                                ("Compression", &c.compression, &most_c),
                            ] {
                                ui.label(what);
                                match cap {
                                    Some((v, governs)) => {
                                        ui.label(format!("{:.1} kN", v / 1000.0));
                                        ui.label(governs);
                                        match most {
                                            Some((force, case)) if *v > 0.0 => {
                                                let u = force / v;
                                                let color = if u > 1.0 {
                                                    ui.visuals().error_fg_color
                                                } else if u > 0.9 {
                                                    egui::Color32::from_rgb(235, 170, 60)
                                                } else {
                                                    ui.visuals().text_color()
                                                };
                                                ui.colored_label(
                                                    color,
                                                    format!("{:.0} %", u * 100.0),
                                                )
                                                .on_hover_text(format!(
                                                    "{:.1} kN in case {case} (the last FE run; \
                                                     the bolts and holes do not change the \
                                                     forces)",
                                                    force / 1000.0
                                                ));
                                            }
                                            _ => {
                                                ui.label(egui::RichText::new("no run").weak())
                                                    .on_hover_text(
                                                        "Results › Run: the forces to set against \
                                                     the capacity",
                                                    );
                                            }
                                        }
                                    }
                                    None => {
                                        ui.label("—");
                                        ui.label(
                                            egui::RichText::new(
                                                "not worked out (no yield strength?)",
                                            )
                                            .weak(),
                                        );
                                        ui.label("");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    if let Some(k) = c.kl_r {
                        ui.label(format!("KL/r {k:.0}"));
                    }
                    if let Some(l) = &c.limit {
                        crate::app::error_label(ui, l.clone());
                    }
                    for j in &c.joints {
                        ui.label(egui::RichText::new(j).small());
                    }
                    if !c.assumed.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("Assumed: {}", c.assumed.join("; ")))
                                .small()
                                .weak(),
                        );
                    }
                }
            }
        });
    }
}

/// The member's leg bolted, laid flat, painted into `rect`: the whole
/// length at one scale when its holes show (else its two ends enlarged
/// with a break between), the holes with their bolts, the end distance,
/// pitch and gauge of each end. A member without a leg to lay flat (a
/// built-up section, an I or H, a tube: `leg` none) is drawn from the
/// side instead ([`paint_elevation`], 2026-11-10). Returns the pieces of
/// steel drawn (0: a member without a cross-section, its caption only).
pub fn paint_member(
    painter: &egui::Painter,
    rect: egui::Rect,
    m: &MemberEnds,
    leg: Option<&FlatLeg>,
    dark: bool,
) -> usize {
    let ink = if dark {
        egui::Color32::from_gray(225)
    } else {
        egui::Color32::from_gray(20)
    };
    let grey = egui::Color32::from_gray(140);
    let title = |text: String| {
        painter.text(
            rect.left_bottom() + egui::vec2(6.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            text,
            egui::FontId::proportional(11.0),
            ink,
        );
    };
    let Some(leg) = leg else {
        return match ohl_structure_link::elevation(m) {
            Some(e) => paint_elevation(painter, rect, m, &e, dark),
            None => {
                title(format!(
                    "member {}: no cross-section yet, so nothing to draw (give it one in the \
                     Members table)",
                    m.member
                ));
                0
            }
        };
    };
    // The same fit as the tools ([`crate::detail_tools::MemberFit`]).
    let fit = crate::detail_tools::MemberFit::new(rect, leg);
    let k = fit.scale();
    let broken = fit.broken();
    let _ = k;
    let x_of = |along: f64| fit.x_of(along);
    let y_of = |across: f64| fit.y_of(across);
    let outline = egui::Stroke::new(1.6, ink);
    let pos = |x: f32, y: f32| egui::pos2(x, y);
    // The steel from where it stops at each end (its cut from the work
    // point), the work points as dashed lines.
    let (c0, c1) = (leg.cuts[0], leg.length - leg.cuts[1]);
    let xb = x_of(leg.length / 2.0);
    let pieces: Vec<(f32, f32)> = if broken {
        vec![(x_of(c0), xb - 6.0), (xb + 6.0, x_of(c1))]
    } else {
        vec![(x_of(c0), x_of(c1))]
    };
    let faint = egui::Stroke::new(1.0, grey);
    for (xa, xz) in &pieces {
        // The other leg, folded out below the heel (2026-11-08: a bolt
        // dragged there goes onto it).
        if leg.other_width > 0.0 {
            painter.rect_stroke(
                egui::Rect::from_two_pos(pos(*xa, y_of(0.0)), pos(*xz, y_of(-leg.other_width))),
                0.0,
                faint,
                egui::StrokeKind::Middle,
            );
        }
        painter.rect_stroke(
            egui::Rect::from_two_pos(pos(*xa, y_of(0.0)), pos(*xz, y_of(leg.width))),
            0.0,
            outline,
            egui::StrokeKind::Middle,
        );
        // The heel's edge (across 0) heavier: the corner of the angle.
        painter.line_segment(
            [pos(*xa, y_of(0.0)), pos(*xz, y_of(0.0))],
            egui::Stroke::new(2.4, ink),
        );
    }
    if broken {
        for dx in [-6.0, 6.0] {
            painter.line_segment(
                [
                    pos(xb + dx - 4.0, y_of(-leg.other_width - 0.01)),
                    pos(xb + dx + 4.0, y_of(leg.width + 0.01)),
                ],
                egui::Stroke::new(1.0, grey),
            );
        }
        painter.text(
            pos(xb, y_of(leg.width) - 10.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{:.3} m", leg.length),
            egui::FontId::proportional(11.0),
            grey,
        );
    }
    let blue = egui::Color32::from_rgb(90, 150, 230);
    for (end, along) in [(0, 0.0), (1, leg.length)] {
        let x = x_of(along);
        painter.add(egui::Shape::dashed_line(
            &[
                pos(x, y_of(-leg.other_width - 0.02)),
                pos(x, y_of(leg.width + 0.02)),
            ],
            egui::Stroke::new(1.0, blue),
            5.0,
            3.0,
        ));
        if leg.cuts[end] > 0.0 {
            painter.text(
                pos(x, y_of(leg.width + 0.02) - 2.0),
                if end == 0 {
                    egui::Align2::LEFT_BOTTOM
                } else {
                    egui::Align2::RIGHT_BOTTOM
                },
                format!("stops {:.0} mm", leg.cuts[end] * 1000.0),
                egui::FontId::proportional(10.5),
                blue,
            );
        }
    }
    for h in &leg.holes {
        let c = pos(
            x_of(h.along),
            y_of(if h.other { -h.across } else { h.across }),
        );
        let color = if h.assumed {
            egui::Color32::from_rgb(235, 170, 60)
        } else {
            egui::Color32::from_rgb(210, 60, 60)
        };
        painter.circle_stroke(
            c,
            ((h.hole / 2.0) * k) as f32,
            egui::Stroke::new(1.2, color),
        );
        painter.circle_filled(c, ((h.bolt / 2.0) * k * 0.6) as f32, color);
    }
    // Each end's layout written beside it.
    for (end, l) in leg.layout.iter().enumerate() {
        let Some([e, p, g]) = l else { continue };
        let (x, align) = if end == 0 {
            (x_of(0.0), egui::Align2::LEFT_TOP)
        } else {
            (x_of(leg.length), egui::Align2::RIGHT_TOP)
        };
        painter.text(
            pos(x, y_of(-leg.other_width) + 6.0),
            align,
            format!(
                "{}: e {:.0} · p {:.0} · g {:.0} mm",
                m.ends[end].as_ref().map_or("—", |x| x.0.as_str()),
                e * 1000.0,
                p * 1000.0,
                g * 1000.0
            ),
            egui::FontId::proportional(11.0),
            grey,
        );
    }
    title(format!(
        "member {} — its leg bolted laid flat ({:.0} mm) above the heel, the other leg ({:.0} mm) \
         folded out below it; holes and bolts of both ends",
        m.member,
        leg.width * 1000.0,
        leg.other_width * 1000.0
    ));
    1
}

/// A member without a leg to lay flat, painted into `rect` from the side
/// (owner 2026-11-10: "the member view doesn't work" — it was empty for
/// these): each piece of its steel (a built-up section's parts, the gap
/// between them open) seen along its section's z, from where it stops at
/// one end to where it stops at the other (its two ends enlarged with a
/// break between when it is long), its work line and work points, and on
/// the right its cross-section at the same scale. Returns the pieces
/// drawn.
pub fn paint_elevation(
    painter: &egui::Painter,
    rect: egui::Rect,
    m: &MemberEnds,
    e: &ohl_structure_link::Elevation,
    dark: bool,
) -> usize {
    let ink = if dark {
        egui::Color32::from_gray(225)
    } else {
        egui::Color32::from_gray(20)
    };
    let grey = egui::Color32::from_gray(140);
    let blue = egui::Color32::from_rgb(90, 150, 230);
    let all = || e.section.iter().flatten();
    let lo = |k: usize| all().map(|q| q[k]).fold(f64::INFINITY, f64::min);
    let hi = |k: usize| all().map(|q| q[k]).fold(f64::NEG_INFINITY, f64::max);
    if e.section.is_empty() || !lo(0).is_finite() {
        return 0;
    }
    let (y0, y1, z0, z1) = (lo(0), hi(0), lo(1), hi(1));
    let (wide, deep) = ((y1 - y0).max(1e-3), (z1 - z0).max(1e-3));
    // Each end out to a little past where its steel stops, at least
    // 0.25 m, at most half the member.
    let reach = |k: usize| (e.cuts[k].max(0.0) + 0.1).max(0.25).min(e.length / 2.0);
    let (r0, r1) = (reach(0), reach(1));
    let broken = r0 + r1 < e.length * 0.999;
    let shown = if broken { r0 + r1 + 0.08 } else { e.length };
    // The side view, a space, the cross-section; the caption below.
    let (margin, space) = (30.0, 40.0);
    let k = ((f64::from(rect.width()) - 2.0 * margin - space) / (shown + wide))
        .min((f64::from(rect.height()) - 2.0 * margin - 40.0) / wide.max(deep))
        .max(1.0);
    let left = f64::from(rect.center().x) - ((shown + wide) * k + space) / 2.0;
    let mid = f64::from(rect.center().y) - 10.0;
    let x_of = |along: f64| -> f32 {
        let s = if !broken || along <= r0 {
            along
        } else if along >= e.length - r1 {
            along - (e.length - r1) + r0 + 0.08
        } else {
            r0 + 0.04
        };
        (left + s * k) as f32
    };
    // Across (the section's y) up the drawing, about the two views' middle.
    let y_of = |across: f64| (mid - (across - (y0 + y1) / 2.0) * k) as f32;
    let pos = |x: f32, y: f32| egui::pos2(x, y);
    let outline = egui::Stroke::new(1.6, ink);
    let (c0, c1) = (e.cuts[0], e.length - e.cuts[1]);
    let xb = x_of(e.length / 2.0);
    let pieces: Vec<(f32, f32)> = if broken {
        vec![(x_of(c0), xb - 6.0), (xb + 6.0, x_of(c1))]
    } else {
        vec![(x_of(c0), x_of(c1))]
    };
    for band in &e.bands {
        for (xa, xz) in &pieces {
            painter.rect_stroke(
                egui::Rect::from_two_pos(pos(*xa, y_of(band[0])), pos(*xz, y_of(band[1]))),
                0.0,
                outline,
                egui::StrokeKind::Middle,
            );
        }
    }
    // Its work line (the member's axis), through the break.
    painter.add(egui::Shape::dashed_line(
        &[pos(x_of(0.0), y_of(0.0)), pos(x_of(e.length), y_of(0.0))],
        egui::Stroke::new(1.0, grey),
        9.0,
        4.0,
    ));
    if broken {
        for dx in [-6.0, 6.0] {
            painter.line_segment(
                [
                    pos(xb + dx - 4.0, y_of(y0) + 6.0),
                    pos(xb + dx + 4.0, y_of(y1) - 6.0),
                ],
                egui::Stroke::new(1.0, grey),
            );
        }
        painter.text(
            pos(xb, y_of(y1) - 12.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{:.3} m", e.length),
            egui::FontId::proportional(11.0),
            grey,
        );
    }
    // The work points, and where the steel stops short of them.
    for (end, along) in [(0, 0.0), (1, e.length)] {
        let x = x_of(along);
        painter.add(egui::Shape::dashed_line(
            &[pos(x, y_of(y0) + 10.0), pos(x, y_of(y1) - 10.0)],
            egui::Stroke::new(1.0, blue),
            5.0,
            3.0,
        ));
        let joint = m.ends[end].as_ref().map_or("—", |x| x.0.as_str());
        let text = if e.cuts[end] > 0.0 {
            format!("{joint}: stops {:.0} mm", e.cuts[end] * 1000.0)
        } else {
            joint.to_owned()
        };
        painter.text(
            pos(x, y_of(y1) - 12.0),
            if end == 0 {
                egui::Align2::LEFT_BOTTOM
            } else {
                egui::Align2::RIGHT_BOTTOM
            },
            text,
            egui::FontId::proportional(10.5),
            blue,
        );
    }
    // The cross-section on the right, at the same scale: z up.
    let sx = left + shown * k + space;
    let section_at = |q: &[f64; 2]| {
        pos(
            (sx + (q[0] - y0) * k) as f32,
            (mid + ((z0 + z1) / 2.0 - q[1]) * k) as f32,
        )
    };
    for o in &e.section {
        painter.add(egui::Shape::closed_line(
            o.iter().map(section_at).collect(),
            outline,
        ));
    }
    painter.text(
        pos(
            (sx + wide * k / 2.0) as f32,
            (mid + deep * k / 2.0) as f32 + 6.0,
        ),
        egui::Align2::CENTER_TOP,
        "cross-section",
        egui::FontId::proportional(10.5),
        grey,
    );
    let section = m.section.as_deref().unwrap_or("its section");
    let what = match (e.shaped, e.section.len()) {
        (false, _) => "a section of values only, drawn as a square of its area".to_owned(),
        (true, 1) => "seen from the side".to_owned(),
        (true, n) => format!("seen from the side, its {n} parts each drawn on its own"),
    };
    // The caption at the top, wrapped to the view (the tools sit at its
    // bottom).
    let caption = painter.layout(
        format!(
            "member {} — {section}: {what}, with where its steel stops at each end; its \
             cross-section on the right. Holes and bolts laid flat, and the tools that place \
             them, are for single angles only.",
            m.member
        ),
        egui::FontId::proportional(11.0),
        ink,
        rect.width() - 12.0,
    );
    painter.galley(rect.left_top() + egui::vec2(6.0, 6.0), caption, ink);
    e.bands.len()
}
