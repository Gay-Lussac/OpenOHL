// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A gusset plate corrected by hand (owner 2026-11-04: "the user should
//! be able to access the gusset plate details if needed to correct some
//! of the joint in case the automatic sizing fails"): in the plate's box
//! (a plate picked in the Detail view) its corners as u, v in its own
//! plane ([`ohl_structure_link::PlateFrame`]), each editable, added or
//! taken away; the outline worked out round the bolts copied into the
//! file to start from, or taken out again; the plate moved square to its
//! plane; its edge distances and what breaks them beside it, its checks
//! of the last run. In the face view its corners are dragged
//! ([`corner_handles`]). Every change is one undo step, written as the
//! plate's 3D `outline` in the same plane.

use std::collections::HashMap;

use eframe::egui;
use ohl_structure_link::{JointDetail, PlateDetail, PlateFrame};

use crate::detail_tools::{DetailTool, FaceFit};
use crate::joint_tools::Act;

/// A plate's check of the last run, for its box: the part, its largest
/// usage over the cases and that case, what governs.
pub(crate) type UsageRow = (String, Option<f64>, String, String);

/// A plate's corner being dragged in the face view.
#[derive(Debug, Clone, PartialEq)]
pub struct CornerDrag {
    /// The plate.
    pub plate: String,
    /// The corner (its place in the outline).
    pub index: usize,
    /// Where the pointer is, in the view's metres.
    pub now: [f64; 2],
}

/// The view's tilt to a plate beyond which its corners are not dragged
/// in it (the plate seen too much edge-on to place a corner well), as the
/// cosine of the angle between the plate's normal and the view's.
const FACING: f64 = 0.7;

/// How much the face of the view and plate `p`'s plane agree: the cosine
/// of the angle between their normals (1: the plate seen square).
fn facing(frame: &PlateFrame, face: &str) -> f64 {
    let [_, _, w] = ohl_structure_link::face_axes(face);
    (frame.n[0] * w[0] + frame.n[1] * w[1] + frame.n[2] * w[2]).abs()
}

/// A length in mm, as the corner table shows it.
fn mm1(v: f64) -> String {
    format!("{:.1}", v * 1000.0)
}

/// The plate's outline section: its corners in its plane, the automatic
/// outline copied or taken out, the plate moved square to its plane, its
/// edge distances and its checks.
pub(crate) fn outline_section(
    ui: &mut egui::Ui,
    j: &JointDetail,
    p: &PlateDetail,
    buffers: &mut HashMap<String, String>,
    usage: &[UsageRow],
) -> Vec<Act> {
    let mut acts = Vec::new();
    let id = p.id.as_str();
    egui::CollapsingHeader::new("Plate details / outline")
        .id_salt(("plate-outline", id))
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(if p.outline_given {
                    "Its outline is given in the file: the corners below are the plate's, edited \
                     by hand (the plate lies in their plane)."
                } else {
                    "Its outline is worked out: round its bolts with the distances below, the legs \
                     bolted and the corner it lies on. Start from it to correct it by hand."
                })
                .small(),
            );
            ui.horizontal_wrapped(|ui| {
                if !p.outline_given {
                    if p.outline.len() >= 3
                        && ui
                            .button("Start from the automatic outline")
                            .on_hover_text(
                                "Copies the outline drawn round the bolts into the file as the \
                                 plate's corners, to be edited here or dragged in the face view. \
                                 From then on the plate keeps these corners (and their plane) \
                                 whatever the bolts do; the edge distances below say when a bolt \
                                 comes too near its edge. One undo step.",
                            )
                            .clicked()
                    {
                        acts.push(Act::Outline {
                            id: id.to_owned(),
                            outline: Some(p.outline.clone()),
                            what: format!("Outline of plate {id}"),
                        });
                    }
                } else if ui
                    .button("Back to automatic")
                    .on_hover_text(
                        "Takes the outline out of the file: the plate is drawn round its bolts \
                         again, keeping the design code's end and edge distances, in the plane of \
                         the members bolted onto it. One undo step.",
                    )
                    .clicked()
                {
                    acts.push(Act::Outline {
                        id: id.to_owned(),
                        outline: None,
                        what: format!("Plate {id} automatic"),
                    });
                }
            });
            match PlateFrame::of(j, p) {
                Some(frame) => acts.extend(corner_table(ui, j, p, &frame, buffers)),
                None => {
                    ui.label(
                        egui::RichText::new(
                            "No plane yet: the plate needs three corners or more, or members \
                             bolted onto it.",
                        )
                        .small()
                        .weak(),
                    );
                }
            }
            edges_and_checks(ui, p, usage);
        });
    acts
}

/// The corner table, + corner, the plate moved square to its plane.
fn corner_table(
    ui: &mut egui::Ui,
    j: &JointDetail,
    p: &PlateDetail,
    frame: &PlateFrame,
    buffers: &mut HashMap<String, String>,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let id = p.id.as_str();
    let given = p.outline_given;
    let corners = frame.corners(&p.outline);
    let along = p
        .leg
        .as_deref()
        .map(|l| format!("along the leg {l}"))
        .or_else(|| {
            p.members
                .iter()
                .find(|m| j.arrivals.iter().any(|a| &a.member == *m))
                .map(|m| format!("along {m}"))
        })
        .unwrap_or_else(|| "along its first side".to_owned());
    ui.label(
        egui::RichText::new(format!(
            "Corners in the plate's plane, mm: u {along}, v square to it in the plane; 0, 0 the \
             work point laid square onto the plate, which lies {:.1} mm off the plate's plane.",
            frame.work_point * 1000.0
        ))
        .small()
        .weak(),
    );
    let sel_id = egui::Id::new(("plate-corner-selected", id));
    let mut selected: Option<usize> = ui.data(|d| d.get_temp(sel_id)).flatten();
    if selected.is_some_and(|k| k >= corners.len()) {
        selected = None;
    }
    let write = |c: &[[f64; 2]], what: String| Act::Outline {
        id: id.to_owned(),
        outline: Some(frame.outline(c)),
        what,
    };
    egui::Grid::new(("plate-corners", id))
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Corner");
            ui.strong("u, mm");
            ui.strong("v, mm");
            ui.label("");
            ui.end_row();
            for (k, c) in corners.iter().enumerate() {
                if ui
                    .selectable_label(selected == Some(k), format!("{}", k + 1))
                    .on_hover_text("Selected: + corner adds the next corner after this one")
                    .clicked()
                {
                    selected = if selected == Some(k) { None } else { Some(k) };
                }
                for axis in 0..2 {
                    let current = mm1(c[axis]);
                    if !given {
                        ui.label(&current);
                        continue;
                    }
                    let tip = if axis == 0 {
                        "u of this corner, mm (along the leg or first member, in the plate's \
                         plane). Typed and left (Enter, Tab or a click elsewhere): the corner \
                         moves in the plate's plane, one undo step."
                    } else {
                        "v of this corner, mm (square to u, in the plate's plane). Typed and left \
                         (Enter, Tab or a click elsewhere): the corner moves in the plate's \
                         plane, one undo step."
                    };
                    let key = format!("plate-corner:{id}:{k}:{axis}");
                    if let Some(text) =
                        crate::tables::text_cell(ui, buffers, key, &current, "mm", tip)
                    {
                        match crate::joint_tools::typed_mm_signed(&text) {
                            Some(v) => {
                                let mut c2 = corners.clone();
                                c2[k][axis] = v;
                                acts.push(write(&c2, format!("Corner {} of plate {id}", k + 1)));
                            }
                            None => acts.push(Act::Say(
                                format!("corner {}: \"{text}\" is not a length", k + 1),
                                true,
                            )),
                        }
                    }
                }
                if given {
                    let r = ui
                        .add_enabled(corners.len() > 3, egui::Button::new("🗑").small())
                        .on_hover_text(
                            "Takes this corner away (the plate keeps three corners at least). \
                             One undo step.",
                        )
                        .on_disabled_hover_text("A plate keeps three corners at least");
                    if r.clicked() {
                        let mut c2 = corners.clone();
                        c2.remove(k);
                        acts.push(write(&c2, format!("Corner {} of plate {id} away", k + 1)));
                        selected = None;
                    }
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });
    ui.data_mut(|d| d.insert_temp(sel_id, selected));
    if !given {
        return acts;
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("+ corner")
            .on_hover_text(
                "A corner midway between the selected corner and the next one (none selected: \
                 between the last and the first), to be moved. One undo step.",
            )
            .clicked()
        {
            let c2 = ohl_structure_link::insert_corner(&corners, selected);
            acts.push(write(&c2, format!("Corner added to plate {id}")));
            selected = Some(selected.map_or(corners.len(), |k| k + 1));
            ui.data_mut(|d| d.insert_temp(sel_id, selected));
        }
    });
    // The plate moved square to its plane.
    let off_id = egui::Id::new(("plate-offset", id));
    let mut by: f64 = ui.data(|d| d.get_temp(off_id)).unwrap_or(0.0);
    ui.horizontal_wrapped(|ui| {
        ui.label("Plane offset").on_hover_text(
            "Moves every corner of the plate square to its plane (along its normal), e.g. onto \
             the other side of a leg's flange when it sits on the wrong side; the corners in the \
             plane stay where they are",
        );
        ui.scope(|ui| {
            crate::style::field_visuals(ui);
            ui.add(
                egui::DragValue::new(&mut by)
                    .speed(0.5)
                    .range(-500.0..=500.0)
                    .suffix(" mm"),
            )
        })
        .inner
        .on_hover_text(
            "How far to move the plate along its normal, mm (positive: along n, the normal whose \
             largest component points along +x, +y or +z)",
        );
        if ui
            .add_enabled(by != 0.0, egui::Button::new("Move the plate"))
            .on_hover_text(
                "Moves the plate by the offset square to its plane: the work point then lies \
                 that much nearer or farther from it. One undo step.",
            )
            .clicked()
        {
            acts.push(Act::Outline {
                id: id.to_owned(),
                outline: Some(ohl_structure_link::offset_outline(
                    &p.outline,
                    frame.n,
                    by / 1000.0,
                )),
                what: format!("Plate {id} moved {by:+.1} mm"),
            });
            by = 0.0;
        }
    });
    ui.data_mut(|d| d.insert_temp(off_id, by));
    acts
}

/// The edge distances the plate keeps, the bolts too near its edge, its
/// checks of the last run.
fn edges_and_checks(ui: &mut egui::Ui, p: &PlateDetail, usage: &[UsageRow]) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new("Edge distances it keeps from its bolts").small());
    for e in &p.edges {
        ui.label(egui::RichText::new(e).small().weak());
    }
    if p.notes.is_empty() {
        if p.outline_given && !p.edges.is_empty() {
            ui.colored_label(
                crate::app::ok_color(ui),
                egui::RichText::new("every bolt onto it keeps these distances from its edge")
                    .small(),
            );
        }
    } else {
        egui::ScrollArea::vertical()
            .id_salt(("plate-notes", p.id.as_str()))
            .max_height(110.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for n in &p.notes {
                    ui.colored_label(ui.visuals().error_fg_color, egui::RichText::new(n).small());
                }
            });
    }
    if !usage.is_empty() {
        ui.label(egui::RichText::new("Its checks, the last run (largest over the cases)").small());
        egui::Grid::new(("plate-box-usage", p.id.as_str()))
            .striped(true)
            .show(ui, |ui| {
                for (part, u, case, governs) in usage {
                    let [r, g, b, _] = crate::results_tab::usage_colour(*u);
                    ui.label(egui::RichText::new(part).small());
                    ui.colored_label(
                        egui::Color32::from_rgb(r, g, b),
                        u.map_or("—".to_owned(), |u| format!("{:.0} %", u * 100.0)),
                    );
                    ui.label(egui::RichText::new(case).small());
                    ui.label(egui::RichText::new(governs).small().weak());
                    ui.end_row();
                }
            });
        ui.label(
            egui::RichText::new(
                "From the last run: an outline changed since is checked again at the next run \
                 (Results › Run).",
            )
            .small()
            .weak(),
        );
    }
}

/// Plate `p`'s outline with corner `index` moved to view point `v` of
/// the face view of `face` (m, from the work point of joint `j`): the
/// point of the plate's plane seen there. None: the plate seen too much
/// edge-on in this view.
pub(crate) fn dragged_outline(
    j: &JointDetail,
    p: &PlateDetail,
    face: &str,
    index: usize,
    v: [f64; 2],
) -> Option<Vec<[f64; 3]>> {
    let frame = PlateFrame::of(j, p)?;
    if facing(&frame, face) < FACING || index >= p.outline.len() {
        return None;
    }
    let [r, u, w] = ohl_structure_link::face_axes(face);
    let o = j.position;
    let at = [
        o[0] + r[0] * v[0] + u[0] * v[1],
        o[1] + r[1] * v[0] + u[1] * v[1],
        o[2] + r[2] * v[0] + u[2] * v[1],
    ];
    let q = frame.meet(at, w)?;
    // Written in the plate's own u, v (exactly in its plane).
    let mut corners = frame.corners(&p.outline);
    corners[index] = frame.to_uv(q);
    Some(frame.outline(&corners))
}

/// In the face view, with Edit and a plate picked whose outline is given:
/// its corners drawn as handles, dragged in the plate's plane (one undo
/// step on release). A plate seen too much edge-on in this face says so
/// instead.
#[allow(clippy::too_many_arguments)]
pub(crate) fn corner_handles(
    j: &JointDetail,
    picked: Option<&str>,
    face: &str,
    fit: &FaceFit,
    response: &egui::Response,
    tool: DetailTool,
    drag: &mut Option<CornerDrag>,
    painter: &egui::Painter,
) -> Vec<Act> {
    let mut acts = Vec::new();
    if tool != DetailTool::Edit {
        *drag = None;
        return acts;
    }
    let Some(p) = picked.and_then(|id| j.plates.iter().find(|p| p.id == id)) else {
        *drag = None;
        return acts;
    };
    if !p.outline_given {
        return acts;
    }
    let Some(frame) = PlateFrame::of(j, p) else {
        return acts;
    };
    let colour = egui::Color32::from_rgb(90, 150, 230);
    if facing(&frame, face) < FACING {
        let deg = facing(&frame, face).acos().to_degrees();
        painter.text(
            response.rect.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!(
                "Plate {}: seen {deg:.0}° off square here — its corners are dragged in a face \
                 more square to it, or typed in its box",
                p.id
            ),
            egui::FontId::proportional(11.0),
            colour,
        );
        return acts;
    }
    let [r, u, _] = ohl_structure_link::face_axes(face);
    let o = j.position;
    let to = |q: [f64; 3]| {
        let d = [q[0] - o[0], q[1] - o[1], q[2] - o[2]];
        [
            d[0] * r[0] + d[1] * r[1] + d[2] * r[2],
            d[0] * u[0] + d[1] * u[1] + d[2] * u[2],
        ]
    };
    let screen: Vec<egui::Pos2> = p.outline.iter().map(|q| fit.at(to(*q))).collect();
    if response.drag_started()
        && let Some(q) = response.interact_pointer_pos()
        && let Some((k, _)) = screen
            .iter()
            .enumerate()
            .map(|(k, s)| (k, s.distance(q)))
            .filter(|(_, d)| *d <= 8.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        *drag = Some(CornerDrag {
            plate: p.id.clone(),
            index: k,
            now: fit.back(q),
        });
    }
    if let Some(d) = drag.as_mut().filter(|d| d.plate == p.id)
        && let Some(q) = response.interact_pointer_pos()
    {
        d.now = fit.back(q);
    }
    // The outline as it would be, while dragged.
    let moved = drag
        .as_ref()
        .filter(|d| d.plate == p.id)
        .and_then(|d| dragged_outline(j, p, face, d.index, d.now));
    if let Some(m) = &moved {
        let pts: Vec<egui::Pos2> = m.iter().map(|q| fit.at(to(*q))).collect();
        painter.add(egui::Shape::closed_line(
            pts,
            egui::Stroke::new(1.5, colour),
        ));
    }
    for (k, s) in screen.iter().enumerate() {
        let hit = response.hover_pos().is_some_and(|h| h.distance(*s) <= 8.0);
        let rect = egui::Rect::from_center_size(*s, egui::vec2(8.0, 8.0));
        painter.rect_filled(rect, 1.0, if hit { colour } else { egui::Color32::WHITE });
        painter.rect_stroke(
            rect,
            1.0,
            egui::Stroke::new(1.5, colour),
            egui::StrokeKind::Middle,
        );
        if hit {
            painter.text(
                *s + egui::vec2(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                format!("corner {} — drag it in the plate's plane", k + 1),
                egui::FontId::proportional(11.0),
                colour,
            );
        }
    }
    if response.drag_stopped()
        && let Some(d) = drag.take()
        && d.plate == p.id
        && let Some(m) = dragged_outline(j, p, face, d.index, d.now)
    {
        acts.push(Act::Outline {
            id: p.id.clone(),
            outline: Some(m),
            what: format!("Corner {} of plate {} dragged", d.index + 1, p.id),
        });
    }
    acts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plate in the plane x = 1 at a joint at the origin, seen from x+.
    fn joint() -> JointDetail {
        JointDetail {
            id: "N".into(),
            position: [0.0; 3],
            arrivals: Vec::new(),
            plates: vec![PlateDetail {
                id: "G".into(),
                thickness: 0.01,
                material: None,
                outline: vec![
                    [1.0, 0.0, 0.0],
                    [1.0, 0.2, 0.0],
                    [1.02, 0.2, 0.3],
                    [1.02, 0.0, 0.3],
                ],
                outline_given: true,
                through: None,
                members: Vec::new(),
                welded_to: Vec::new(),
                assembly: vec!["G".into()],
                welds: Vec::new(),
                leg: None,
                edges: Vec::new(),
                notes: Vec::new(),
            }],
            splices: Vec::new(),
            completeness: ohl_structure_link::Completeness::Complete,
        }
    }

    #[test]
    fn a_dragged_corner_stays_in_the_plate_plane() {
        let j = joint();
        let p = &j.plates[0];
        let n = ohl_structure_link::plane_normal(&p.outline).unwrap();
        // Seen from x+: right is +y, up is +z.
        let m = dragged_outline(&j, p, "x+", 2, [0.25, 0.4]).unwrap();
        assert_eq!(m.len(), 4);
        let q = m[2];
        assert!((q[1] - 0.25).abs() < 1e-9 && (q[2] - 0.4).abs() < 1e-9);
        // In the plane of the others.
        let d = [
            q[0] - p.outline[0][0],
            q[1] - p.outline[0][1],
            q[2] - p.outline[0][2],
        ];
        assert!((d[0] * n[0] + d[1] * n[1] + d[2] * n[2]).abs() < 1e-9);
        // The others unchanged.
        for k in [0, 1, 3] {
            for (a, b) in m[k].iter().zip(&p.outline[k]) {
                assert!((a - b).abs() < 1e-9);
            }
        }
        // Seen from above, the plate is edge-on: no drag.
        assert!(dragged_outline(&j, p, "plan", 2, [0.25, 0.4]).is_none());
    }
}
