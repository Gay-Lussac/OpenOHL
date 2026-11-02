// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Joint details panel (owner 2026-11-02, stage 2 of the joint
//! details): a tower's joint — the one selected — with
//!
//! - a **close-up**: the members as their profiles, cut a little beyond
//!   the joint, the bolts on the legs bolted, the plates; turned by
//!   dragging, zoomed by scrolling;
//! - a **face view**: the joint square to a face, as a shop drawing (the
//!   gauge lines, the bolts, the end distance and pitch), saved as SVG or
//!   PDF;
//! - the **connections table**: each member end, where it comes from and
//!   its condition, each value marked given, the member's, worked out,
//!   assumed or not given ([`ohl_structure_link::Source`]).
//!
//! The details are worked out by the link crate
//! ([`ohl_structure_link::joint_details`]) and kept until the file or the
//! libraries change. Not used by the checks yet (stage 4).

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ohl_structure_link::{
    Arrival, Completeness, FaceView, JointDetail, JointDetails, LineKind, Mark, Source, Valued,
};

use crate::app::StructureApp;
use ohl_structure_edit::Handle;

/// The joint details worked out, or why not.
type Details = Arc<Result<JointDetails, String>>;

/// What the drawing shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Drawn {
    /// The close-up in 3D.
    #[default]
    CloseUp,
    /// The face view.
    Face,
}

/// The panel's state.
#[derive(Default)]
pub struct JointPanel {
    /// The details, for (file revision, libraries stamp).
    details: Option<((u64, u64), Details)>,
    /// The joint shown (follows the selection).
    pub joint: Option<String>,
    /// The face of the face view (none: the joint's first).
    pub face: Option<String>,
    /// What is drawn.
    pub drawn: Drawn,
    /// The close-up's turn about z and tilt, radians.
    pub yaw: f32,
    /// Its tilt.
    pub pitch: f32,
    /// Its zoom.
    pub zoom: f32,
}

/// A colour per completeness, for the 3D view (View › Joints by their
/// details) and the panel.
pub fn completeness_color(c: Completeness) -> [u8; 4] {
    match c {
        Completeness::Complete => [80, 190, 110, 255],
        Completeness::PartlyAssumed => [235, 170, 60, 255],
        Completeness::Nothing => [200, 80, 80, 255],
    }
}

impl StructureApp {
    /// The joint details of the file open, worked out once per revision
    /// and libraries.
    pub(crate) fn joint_details(&mut self) -> Option<Arc<Result<JointDetails, String>>> {
        let f = self.file.as_ref()?;
        let key = (f.revision(), self.libs_stamp);
        if let Some((k, d)) = &self.joints.details
            && *k == key
        {
            return Some(d.clone());
        }
        let d = Arc::new(ohl_structure_link::joint_details(
            f.text_ref(),
            &self.loads.sections,
        ));
        self.joints.details = Some((key, d.clone()));
        Some(d)
    }

    /// The colours of the joints by their details (View › Joints by their
    /// details): a tower's only.
    pub(crate) fn joint_color_list(&mut self) -> Vec<(String, [u8; 4])> {
        let tower = self
            .file
            .as_ref()
            .is_some_and(|f| ohl_structure_edit::is_tower(f.structure().kind));
        if !self.joint_colors || !tower {
            return Vec::new();
        }
        match self.joint_details().as_deref() {
            Some(Ok(d)) => d
                .joints
                .iter()
                .map(|j| (j.id.clone(), completeness_color(j.completeness)))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The Joint details panel.
    pub(crate) fn joint_panel_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("joint-details-panel")
            .auto_shrink([false, false])
            .show(ui, |ui| self.joint_panel_body(ui));
    }

    fn joint_panel_body(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.file else {
            ui.label("No structure open.");
            return;
        };
        let t = f.structure_arc();
        if !ohl_structure_edit::is_tower(t.kind) {
            ui.label(format!(
                "Joint details are a tower's: this structure is a {}. Geometry › Structure kind \
                 makes it a lattice tower or a portal.",
                kind_words(t.kind)
            ));
            return;
        }
        // The joint selected, else the one shown before, else the first.
        if let Some(Handle::Joint(id)) = self
            .selected
            .iter()
            .rev()
            .find(|h| matches!(h, Handle::Joint(_)))
            && self.joints.joint.as_ref() != Some(id)
        {
            self.joints.joint = Some(id.clone());
            self.joints.face = None;
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
        if details.joints.is_empty() {
            ui.label("No joints yet: build the body first (Body › Generators).");
            return;
        }
        let id = self
            .joints
            .joint
            .clone()
            .filter(|j| details.joint(j).is_some())
            .unwrap_or_else(|| details.joints[0].id.clone());
        let mut chosen = id.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label("Joint");
            egui::ComboBox::from_id_salt("joint-detail-joint")
                .selected_text(&chosen)
                .height(320.0)
                .show_ui(ui, |ui| {
                    for j in &details.joints {
                        let [r, g, b, _] = completeness_color(j.completeness);
                        let text =
                            egui::RichText::new(format!("{} — {}", j.id, j.completeness.words()))
                                .color(egui::Color32::from_rgb(r, g, b));
                        ui.selectable_value(&mut chosen, j.id.clone(), text);
                    }
                });
            if let Some(j) = details.joint(&chosen) {
                let [r, g, b, _] = completeness_color(j.completeness);
                ui.colored_label(
                    egui::Color32::from_rgb(r, g, b),
                    format!(
                        "{} member end(s) · {}",
                        j.arrivals.len(),
                        j.completeness.words()
                    ),
                )
                .on_hover_text(
                    "complete: every member end's condition given, the member's or worked out; \
                     partly assumed: some of it assumed; no connection data: nothing given at \
                     any end",
                );
            }
        });
        if chosen != id {
            self.selected = vec![Handle::Joint(chosen.clone())];
            self.scroll_to = Some(Handle::Joint(chosen.clone()));
            self.joints.face = None;
        }
        self.joints.joint = Some(chosen.clone());
        let Some(j) = details.joint(&chosen) else {
            return;
        };
        ui.label(
            egui::RichText::new(
                "Shown and worked out only: the checks do not take these yet (stage 4 of the \
                 joint details). Values in a connection override the member's design data at \
                 that end.",
            )
            .small()
            .weak(),
        );
        if !details.level3 {
            ui.label(
                "The structure is not a full design yet (level 3: every member with a \
                 cross-section, supports): the members meeting here only. Connections belong to \
                 level 3.",
            );
            arrivals_only(ui, j);
            return;
        }
        let faces = ohl_structure_link::faces_of(j);
        let face = self
            .joints
            .face
            .clone()
            .filter(|f| faces.contains(f))
            .unwrap_or_else(|| faces[0].clone());
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.joints.drawn, Drawn::CloseUp, "Close-up")
                .on_hover_text("The joint in 3D: drag to turn it, scroll to zoom");
            ui.selectable_value(&mut self.joints.drawn, Drawn::Face, "Face view")
                .on_hover_text("The joint square to a face, as a shop drawing");
            if self.joints.drawn == Drawn::Face {
                let mut f = face.clone();
                egui::ComboBox::from_id_salt("joint-detail-face")
                    .selected_text(face_words(&f))
                    .show_ui(ui, |ui| {
                        for x in &faces {
                            ui.selectable_value(&mut f, x.clone(), face_words(x));
                        }
                    });
                self.joints.face = Some(f);
                let view = ohl_structure_link::face_view(j, &face);
                if ui
                    .button("Save as SVG…")
                    .on_hover_text("The face view on an A4 sheet, scaled to fit")
                    .clicked()
                {
                    self.save_face(&view, "svg");
                }
                if ui
                    .button("Save as PDF…")
                    .on_hover_text("The face view on an A4 sheet, scaled to fit")
                    .clicked()
                {
                    self.save_face(&view, "pdf");
                }
            } else if ui.button("Reset view").clicked() {
                self.joints.yaw = 0.0;
                self.joints.pitch = 0.0;
                self.joints.zoom = 0.0;
            }
        });
        let height = 300.0;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        match self.joints.drawn {
            Drawn::CloseUp => {
                if response.dragged() {
                    let d = response.drag_delta();
                    self.joints.yaw += d.x * 0.01;
                    self.joints.pitch = (self.joints.pitch + d.y * 0.01).clamp(-1.5, 1.5);
                }
                if response.hovered() {
                    let s = ui.input(|i| i.smooth_scroll_delta.y);
                    self.joints.zoom = (self.joints.zoom + s * 0.002).clamp(-1.5, 2.5);
                }
                close_up(&painter, rect, j, &self.joints, ui.visuals().dark_mode);
            }
            Drawn::Face => {
                let view = ohl_structure_link::face_view(j, &face);
                paint_face(&painter, rect, &view, ui.visuals().dark_mode);
            }
        }
        ui.add_space(4.0);
        if j.arrivals.iter().any(|a| {
            a.bolts.value.is_some() && a.profile.as_ref().is_some_and(|p| p.angle.is_none())
        }) {
            ui.label(
                egui::RichText::new(
                    "Bolts are drawn on angles only: a member of another cross-section (or a \
                     [[section]] table of values) shows its bolts in the table.",
                )
                .small()
                .weak(),
            );
        }
        legend(ui);
        connections_table(ui, j, &mut self.selected);
        for n in &details.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
    }

    /// Asks where, then writes the face view as `ext` (svg, pdf).
    fn save_face(&mut self, view: &FaceView, ext: &str) {
        let stem = self
            .file
            .as_ref()
            .and_then(|f| f.path())
            .and_then(|p| p.file_stem())
            .map_or("structure".to_owned(), |s| s.to_string_lossy().into_owned());
        let name = format!(
            "{stem}-joint-{}-{}.{ext}",
            view.joint,
            view.face.replace('+', "p").replace('-', "m")
        );
        let dir: Option<PathBuf> = self
            .file
            .as_ref()
            .and_then(|f| f.path())
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        let mut dialog = rfd::FileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(&name);
        if let Some(d) = dir {
            dialog = dialog.set_directory(d);
        }
        let Some(path) = dialog.save_file() else {
            return;
        };
        let bytes = if ext == "pdf" {
            ohl_structure_link::face_pdf(view)
        } else {
            ohl_structure_link::face_svg(view).into_bytes()
        };
        self.message = Some(match std::fs::write(&path, bytes) {
            Ok(()) => (format!("face view written: {}", path.display()), false),
            Err(e) => (format!("{}: {e}", path.display()), true),
        });
    }
}

fn kind_words(k: ohl_structures::structures::StructureKind) -> &'static str {
    use ohl_structures::structures::StructureKind as K;
    match k {
        K::Lattice => "lattice tower",
        K::Pole => "pole",
        K::Portal => "portal",
        K::Other => "structure of another kind",
    }
}

fn face_words(f: &str) -> String {
    match f {
        "plan" => "plan (from above)".into(),
        f => format!("face {f}"),
    }
}

/// The colour a value is written in, by where it comes from.
fn source_color(ui: &egui::Ui, s: Source) -> egui::Color32 {
    let dark = ui.visuals().dark_mode;
    match s {
        Source::Given | Source::Member => ui.visuals().text_color(),
        Source::WorkedOut => {
            if dark {
                egui::Color32::from_rgb(120, 180, 255)
            } else {
                egui::Color32::from_rgb(20, 90, 180)
            }
        }
        Source::Assumed => {
            if dark {
                egui::Color32::from_rgb(235, 170, 60)
            } else {
                egui::Color32::from_rgb(170, 100, 0)
            }
        }
        Source::Missing => ui.visuals().weak_text_color(),
    }
}

fn legend(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        for (s, what) in [
            (Source::Given, "given at the member end"),
            (
                Source::Member,
                "the member's design data (both ends alike), in italics",
            ),
            (
                Source::WorkedOut,
                "worked out from the geometry, the bolts or the leg",
            ),
            (
                Source::Assumed,
                "assumed: as the checks assume, or for the drawing only",
            ),
            (Source::Missing, "not given"),
        ] {
            let mut t = egui::RichText::new(s.words())
                .small()
                .color(source_color(ui, s));
            if s == Source::Member {
                t = t.italics();
            }
            ui.label(t).on_hover_text(what);
        }
    });
}

/// A value in its colour, its source on hover.
fn cell<T>(ui: &mut egui::Ui, v: &Valued<T>, text: impl Fn(&T) -> String) {
    let s = v.value.as_ref().map_or("—".to_owned(), text);
    let mut t = egui::RichText::new(s).color(source_color(ui, v.source));
    if v.source == Source::Member {
        t = t.italics();
    }
    ui.label(t).on_hover_text(v.source.words());
}

fn mm(x: &f64) -> String {
    format!("{:.0}", x * 1000.0)
}

/// The members meeting at `j` (below level 3).
fn arrivals_only(ui: &mut egui::Ui, j: &JointDetail) {
    egui::Grid::new("joint-arrivals")
        .striped(true)
        .show(ui, |ui| {
            for h in ["Member", "To joint", "Face", "Length"] {
                ui.strong(h);
            }
            ui.end_row();
            for a in &j.arrivals {
                ui.label(&a.member);
                ui.label(&a.other);
                ui.label(&a.face);
                ui.label(format!("{:.2} m", a.length));
                ui.end_row();
            }
        });
}

/// The connections table: one row per member end.
fn connections_table(ui: &mut egui::Ui, j: &JointDetail, selected: &mut Vec<Handle>) {
    egui::ScrollArea::both()
        .id_salt("joint-connections")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("joint-connections-grid")
                .striped(true)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    for (h, tip) in [
                        ("Member", "The member (click: select it)"),
                        ("End", "Its first (i) or second (j) joint is this one"),
                        ("Class", "Its class (design data) or group"),
                        ("From", "The face it lies in"),
                        (
                            "Angle",
                            "To the member it bolts onto (or the joint's leg), degrees",
                        ),
                        ("Held", "Bolted, welded, pinned or fixed"),
                        ("Onto", "What it bolts onto: a member or a plate"),
                        ("Leg", "The angle's leg bolted"),
                        ("Bolts", "Count × diameter, grade"),
                        ("Lines", "Gauge lines"),
                        ("Gauge mm", "From the heel along the leg bolted"),
                        ("Pitch mm", "Between bolts along a line"),
                        ("End mm", "From the member's end to the first bolt"),
                        ("Edge mm", "From the bolts to the leg's edge"),
                        ("Eccentric", "Loaded off its axis (one leg bolted)"),
                        (
                            "Restrained",
                            "Held against turning (two bolts or more into a stiffer member)",
                        ),
                        ("e mm", "The eccentricity: the gauge line from the centroid"),
                    ] {
                        ui.strong(h).on_hover_text(tip);
                    }
                    ui.end_row();
                    for a in &j.arrivals {
                        row(ui, a, selected);
                        ui.end_row();
                    }
                });
        });
}

fn row(ui: &mut egui::Ui, a: &Arrival, selected: &mut Vec<Handle>) {
    let mut name = egui::RichText::new(&a.member);
    if a.modelling {
        name = name.weak();
    }
    let r = ui.link(name).on_hover_text(if a.modelling {
        "a modelling member (not design-checked); click: select it"
    } else {
        "click: select it"
    });
    if r.clicked() {
        *selected = vec![Handle::Member(a.member.clone())];
    }
    ui.label(if a.end == 0 { "i" } else { "j" });
    ui.label(
        a.class
            .clone()
            .or_else(|| a.group.clone())
            .unwrap_or_else(|| "—".into()),
    );
    ui.label(&a.face);
    match &a.angle {
        Some((to, deg)) => ui
            .label(format!("{deg:.1}°"))
            .on_hover_text(format!("to {to}")),
        None => ui.label("—"),
    };
    cell(ui, &a.kind, |k| k.as_str().to_owned());
    cell(ui, &a.onto, |o| o.clone());
    cell(ui, &a.leg, |l| l.as_str().to_owned());
    cell(ui, &a.bolts, |b| {
        let mut s = format!("{} × M{:.0}", b.count, b.diameter * 1000.0);
        if let Some(g) = &b.grade {
            s += &format!(" {g}");
        }
        s
    });
    cell(ui, &a.gauge_lines, |n| n.to_string());
    cell(ui, &a.gauges, |g| {
        g.iter().map(mm).collect::<Vec<_>>().join(", ")
    });
    cell(ui, &a.pitch, mm);
    cell(ui, &a.end_distance, mm);
    cell(ui, &a.edge_distance, mm);
    cell(ui, &a.eccentric, |b| {
        if *b { "yes" } else { "no" }.to_owned()
    });
    cell(ui, &a.restrained, |b| {
        if *b { "yes" } else { "no" }.to_owned()
    });
    cell(ui, &a.eccentricity, mm);
    if !a.notes.is_empty() {
        ui.label(egui::RichText::new("note").weak())
            .on_hover_text(a.notes.join("\n"));
    }
}

/// The close-up: the members as prisms of their profile, the bolts, the
/// plates, painted far to near.
fn close_up(
    painter: &egui::Painter,
    rect: egui::Rect,
    j: &JointDetail,
    s: &JointPanel,
    dark: bool,
) {
    let reach = ohl_structure_link::reach(j);
    let (cy, sy) = (f64::from(s.yaw).cos(), f64::from(s.yaw).sin());
    let (cp, sp) = (
        f64::from(s.pitch + 0.35).cos(),
        f64::from(s.pitch + 0.35).sin(),
    );
    // Seen from the front (−y), turned and tilted: x right, z up.
    let view = |p: [f64; 3]| {
        let d = [
            p[0] - j.position[0],
            p[1] - j.position[1],
            p[2] - j.position[2],
        ];
        let x = d[0] * cy + d[1] * sy;
        let y = -d[0] * sy + d[1] * cy;
        let up = d[2] * cp - y * sp;
        let depth = d[2] * sp + y * cp;
        [x, up, depth]
    };
    let k = f64::from(rect.width().min(rect.height())) / (2.4 * reach) * f64::from(s.zoom).exp();
    let at = |q: [f64; 3]| rect.center() + egui::vec2((q[0] * k) as f32, (-q[1] * k) as f32);
    let light = [0.35, 0.55, 0.75];
    let mut faces: Vec<(f64, Vec<egui::Pos2>, egui::Color32)> = Vec::new();
    let steel = if dark {
        [150.0, 165.0, 185.0]
    } else {
        [120.0, 135.0, 155.0]
    };
    for a in &j.arrivals {
        let length = a.length.min(reach);
        let Some(p) = &a.profile else {
            continue;
        };
        let corner = |sd: f64, q: [f64; 2]| {
            let x: [f64; 3] = std::array::from_fn(|i| {
                j.position[i] + a.direction[i] * sd + p.axes[1][i] * q[0] + p.axes[2][i] * q[1]
            });
            view(x)
        };
        let near: Vec<[f64; 3]> = p.outline.iter().map(|q| corner(0.0, *q)).collect();
        let far: Vec<[f64; 3]> = p.outline.iter().map(|q| corner(length, *q)).collect();
        let n = near.len();
        let mut add = |poly: Vec<[f64; 3]>| {
            let e1 = sub(poly[1], poly[0]);
            let e2 = sub(poly[poly.len() - 1], poly[0]);
            let mut nrm = cross(e1, e2);
            let l = dot(nrm, nrm).sqrt();
            if l < 1e-12 {
                return;
            }
            nrm = [nrm[0] / l, nrm[1] / l, nrm[2] / l];
            let shade = 0.45 + 0.55 * dot(nrm, light).abs();
            let c = egui::Color32::from_rgb(
                (steel[0] * shade) as u8,
                (steel[1] * shade) as u8,
                (steel[2] * shade) as u8,
            );
            let depth = poly.iter().map(|q| q[2]).sum::<f64>() / poly.len() as f64;
            faces.push((depth, poly.iter().map(|q| at(*q)).collect(), c));
        };
        for i in 0..n {
            let i2 = (i + 1) % n;
            add(vec![near[i], near[i2], far[i2], far[i]]);
        }
        add(near.clone());
        add(far.clone());
    }
    for pl in &j.plates {
        if pl.outline.len() < 3 {
            continue;
        }
        let poly: Vec<[f64; 3]> = pl.outline.iter().map(|q| view(*q)).collect();
        let depth = poly.iter().map(|q| q[2]).sum::<f64>() / poly.len() as f64;
        faces.push((
            depth,
            poly.iter().map(|q| at(*q)).collect(),
            egui::Color32::from_rgba_unmultiplied(70, 140, 210, 150),
        ));
    }
    // Far first (the larger depth is nearer the viewer: drawn last).
    faces.sort_by(|a, b| a.0.total_cmp(&b.0));
    let edge = egui::Stroke::new(0.6, egui::Color32::from_black_alpha(140));
    for (_, pts, c) in faces {
        // Concave faces (an angle's ends) as a mesh fan from the first
        // corner is wrong; draw them as outlines filled by triangles of
        // the convex parts: an L of six corners splits into two rectangles.
        if pts.len() == 6 {
            for quad in [[0, 1, 2, 3], [0, 3, 4, 5]] {
                painter.add(egui::Shape::convex_polygon(
                    quad.iter().map(|i| pts[*i]).collect(),
                    c,
                    egui::Stroke::NONE,
                ));
            }
            painter.add(egui::Shape::closed_line(pts, edge));
        } else if pts.len() > 6 {
            painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.2, c)));
        } else {
            painter.add(egui::Shape::convex_polygon(pts, c, edge));
        }
    }
    // Members without a profile: lines.
    for a in j.arrivals.iter().filter(|a| a.profile.is_none()) {
        let far = add3(j.position, scale3(a.direction, a.length.min(reach)));
        painter.line_segment(
            [at(view(j.position)), at(view(far))],
            egui::Stroke::new(2.0, egui::Color32::GRAY),
        );
    }
    // The bolts: their shanks through the leg, their heads.
    for a in &j.arrivals {
        let Some(b) = &a.bolts.value else { continue };
        let assumed = a.gauges.source == Source::Assumed || a.pitch.source == Source::Assumed;
        let c = if assumed {
            egui::Color32::from_rgb(160, 160, 160)
        } else {
            egui::Color32::from_rgb(210, 50, 50)
        };
        for p in &a.bolt_points {
            let top = add3(*p, scale3(a.bolt_axis, 0.02));
            let bottom = add3(*p, scale3(a.bolt_axis, -0.02));
            let w = ((b.diameter * k) as f32).max(2.0);
            painter.line_segment([at(view(bottom)), at(view(top))], egui::Stroke::new(w, c));
            painter.circle_filled(at(view(top)), w * 0.8, c);
        }
    }
    // The labels at the members' cut ends.
    let text = if dark {
        egui::Color32::from_gray(220)
    } else {
        egui::Color32::from_gray(30)
    };
    for a in &j.arrivals {
        let end = add3(j.position, scale3(a.direction, a.length.min(reach) + 0.03));
        painter.text(
            at(view(end)),
            egui::Align2::CENTER_CENTER,
            &a.member,
            egui::FontId::proportional(12.0),
            text,
        );
    }
    painter.circle_filled(at(view(j.position)), 3.0, text);
    painter.text(
        rect.left_bottom() + egui::vec2(6.0, -6.0),
        egui::Align2::LEFT_BOTTOM,
        format!("joint {} — drag to turn, scroll to zoom", j.id),
        egui::FontId::proportional(11.0),
        text,
    );
}

/// The face view painted into `rect`, fitted.
fn paint_face(painter: &egui::Painter, rect: egui::Rect, v: &FaceView, dark: bool) {
    let [x0, y0, x1, y1] = v.bounds;
    let margin = 24.0;
    let k = ((f64::from(rect.width()) - 2.0 * margin) / (x1 - x0).max(1e-6))
        .min((f64::from(rect.height()) - 2.0 * margin) / (y1 - y0).max(1e-6));
    let cx = (x0 + x1) / 2.0;
    let cy = (y0 + y1) / 2.0;
    let at = |p: [f64; 2]| {
        rect.center() + egui::vec2(((p[0] - cx) * k) as f32, (-(p[1] - cy) * k) as f32)
    };
    let ink = if dark {
        egui::Color32::from_gray(225)
    } else {
        egui::Color32::from_gray(20)
    };
    let grey = egui::Color32::from_gray(140);
    for m in &v.marks {
        match m {
            Mark::Line {
                points,
                closed,
                kind,
            } => {
                let stroke = match kind {
                    LineKind::Member => egui::Stroke::new(1.6, ink),
                    LineKind::Edge => egui::Stroke::new(0.8, ink),
                    LineKind::Gauge | LineKind::Dimension => egui::Stroke::new(0.7, grey),
                    LineKind::Plate => {
                        egui::Stroke::new(1.4, egui::Color32::from_rgb(70, 140, 210))
                    }
                };
                let pts: Vec<egui::Pos2> = points.iter().map(|p| at(*p)).collect();
                if *kind == LineKind::Gauge && pts.len() == 2 {
                    painter.add(egui::Shape::dashed_line(&pts, stroke, 6.0, 3.0));
                } else if *closed {
                    painter.add(egui::Shape::closed_line(pts, stroke));
                } else {
                    painter.add(egui::Shape::line(pts, stroke));
                }
            }
            Mark::Bolt {
                at: c,
                radius,
                side,
                assumed,
            } => {
                let color = if *assumed {
                    grey
                } else {
                    egui::Color32::from_rgb(210, 50, 50)
                };
                let r = ((radius * k) as f32).max(2.0);
                match side {
                    None => {
                        painter.circle_stroke(at(*c), r, egui::Stroke::new(1.4, color));
                    }
                    Some(e) => {
                        painter.line_segment([at(*c), at(*e)], egui::Stroke::new(2.0 * r, color));
                    }
                }
            }
            Mark::Text { at: p, text, label } => {
                painter.text(
                    at(*p),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(if *label { 12.0 } else { 10.5 }),
                    if *label { ink } else { grey },
                );
            }
        }
    }
    painter.text(
        rect.left_bottom() + egui::vec2(6.0, -6.0),
        egui::Align2::LEFT_BOTTOM,
        &v.title,
        egui::FontId::proportional(11.0),
        ink,
    );
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale3(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
