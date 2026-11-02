// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Joint details panel and the Joint view pane (owner 2026-11-02,
//! stage 2 of the joint details; the view a pane of its own beside the 3D
//! view: "make it so that the joint view opens on the tower window as a
//! new pane … it's easier to not fill the view with screens"): a tower's
//! joint — the one selected — with
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
//! libraries change. The end conditions shown are the check's
//! (`ohl_structures_fe::member_ends`, stage 4.1 and 4.2).

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ohl_structure_link::{
    Completeness, FaceView, JointDetail, JointDetails, LineKind, Mark, Source,
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
    /// What the Detail view shows: the joint or the member
    /// ([`crate::member_panel`]).
    pub subject: crate::member_panel::Subject,
    /// The member of Member details.
    pub member: Option<String>,
    /// Its capacities worked out.
    pub capacity: crate::member_panel::CapacityCache,
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
        let bolts = self.bolt_library();
        let f = self.file.as_ref()?;
        let d = Arc::new(ohl_structure_link::joint_details(
            f.text_ref(),
            &self.loads.sections,
            &bolts.0,
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
        self.follow_selection();
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
                "The ASCE 10-97 check takes from this table each end's eccentric and restrained \
                 (as shown) and its bolts, holes and leg bolted for the bolt shear, bearing and \
                 net section, the weaker end governing; not yet the distances, the plates' own \
                 checks or the eccentricity. An end without a connection is checked from the \
                 member's design data, as before. Values in a connection override the member's \
                 design data at that end.",
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
        self.joint_forms(ui);
        legend(ui);
        let t = self.file.as_ref().map(|f| f.structure_arc());
        let Some(t) = t else {
            return;
        };
        let mut acts = crate::joint_tools::joint_bar(ui, details, j, &mut self.buffers);
        let presets = self.presets();
        acts.extend(crate::joint_tools::table(
            ui,
            &t,
            details,
            j,
            &mut self.buffers,
            &presets,
        ));
        if let Some(e) = self.presets_error() {
            crate::app::error_label(ui, format!("Connection presets: {e}"));
        }
        ui.label(
            egui::RichText::new(
                "Type or pick a value to give it at this member end; right-click a value (or \
                 pick \"not given here\") to go back to the member's or what is worked out. \
                 Each change is one undo step.",
            )
            .small()
            .weak(),
        );
        self.apply_joint_acts(acts, details);
        for n in &details.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
    }

    /// The Joint view pane (beside the 3D view): the joint shown in the
    /// Joint details panel as a close-up or a face view.
    pub(crate) fn joint_view_ui(&mut self, ui: &mut egui::Ui) {
        // Also when the Joint details panel is not the tab in front.
        self.follow_selection();
        let Some(f) = &self.file else {
            return;
        };
        if !ohl_structure_edit::is_tower(f.structure().kind) {
            ui.label("A tower's joints are drawn here (Geometry › Structure kind).");
            return;
        }
        let Some(details) = self.joint_details() else {
            return;
        };
        let Ok(details) = details.as_ref() else {
            ui.label("The joints could not be read (Design › Joint details says why).");
            return;
        };
        if self.joints.subject == crate::member_panel::Subject::Member
            && let Some(m) = self
                .joints
                .member
                .as_deref()
                .and_then(|id| ohl_structure_link::member_ends(details, id))
        {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Member {}", m.member));
                ui.label(
                    egui::RichText::new("(click a joint to see it here)")
                        .small()
                        .weak(),
                );
            });
            let leg = ohl_structure_link::flat_leg(&m);
            let size = ui.available_size().max(egui::vec2(120.0, 120.0));
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            crate::member_panel::paint_member(
                &painter,
                rect,
                &m,
                leg.as_ref(),
                ui.visuals().dark_mode,
            );
            return;
        }
        let id = self
            .joints
            .joint
            .clone()
            .filter(|j| details.joint(j).is_some())
            .or_else(|| details.joints.first().map(|j| j.id.clone()));
        let Some(j) = id.as_deref().and_then(|id| details.joint(id)) else {
            ui.label("No joints yet: build the body first (Body › Generators).");
            return;
        };
        if !details.level3 {
            ui.label(format!(
                "Joint {}: drawn at level 3 (every member with a cross-section, supports).",
                j.id
            ));
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
            ui.strong(format!("Joint {}", j.id));
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
        let size = ui.available_size().max(egui::vec2(120.0, 120.0));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
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

use crate::joint_tools::source_color;

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

/// How a face of the close-up is filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fill {
    /// Convex: filled as it is.
    Convex,
    /// An angle's end: an L, filled as two rectangles.
    Angle,
    /// Another concave end: outlined.
    Outline,
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
    // Depth, corners, colour, and whether the face is convex (an angle's
    // or a channel's end is not: drawn by its parts or outlined).
    let mut faces: Vec<(f64, Vec<egui::Pos2>, egui::Color32, Fill)> = Vec::new();
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
                j.position[i]
                    + a.offset[i]
                    + a.direction[i] * sd
                    + p.axes[1][i] * q[0]
                    + p.axes[2][i] * q[1]
            });
            view(x)
        };
        let near: Vec<[f64; 3]> = p.outline.iter().map(|q| corner(0.0, *q)).collect();
        let far: Vec<[f64; 3]> = p.outline.iter().map(|q| corner(length, *q)).collect();
        let n = near.len();
        let cap = if p.angle.is_some() {
            Fill::Angle
        } else if p.outline.len() == 8 {
            Fill::Outline
        } else {
            Fill::Convex
        };
        let mut add = |poly: Vec<[f64; 3]>, fill: Fill| {
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
            faces.push((depth, poly.iter().map(|q| at(*q)).collect(), c, fill));
        };
        for i in 0..n {
            let i2 = (i + 1) % n;
            add(vec![near[i], near[i2], far[i2], far[i]], Fill::Convex);
        }
        add(near.clone(), cap);
        add(far.clone(), cap);
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
            Fill::Convex,
        ));
    }
    // The bolts: head, shank and nut, shaded as the members (zinc; orange
    // where their layout is partly assumed).
    for a in &j.arrivals {
        let Some(b) = &a.bolts.value else { continue };
        let assumed = a.gauges.source == Source::Assumed || a.pitch.source == Source::Assumed;
        let base = if assumed {
            [235.0, 170.0, 60.0]
        } else {
            [200.0, 202.0, 196.0]
        };
        for p in &a.bolt_points {
            for (poly, _) in
                ohl_structure_link::bolt_faces(*p, a.bolt_axis, b.diameter, a.grip, b.drawn)
            {
                let poly: Vec<[f64; 3]> = poly.iter().map(|q| view(*q)).collect();
                let e1 = sub(poly[1], poly[0]);
                let e2 = sub(poly[poly.len() - 1], poly[0]);
                let n = cross(e1, e2);
                let l = dot(n, n).sqrt();
                if l < 1e-15 {
                    continue;
                }
                let shade = 0.45 + 0.55 * (dot(n, light) / l).abs();
                let c = egui::Color32::from_rgb(
                    (base[0] * shade) as u8,
                    (base[1] * shade) as u8,
                    (base[2] * shade) as u8,
                );
                let depth = poly.iter().map(|q| q[2]).sum::<f64>() / poly.len() as f64;
                faces.push((
                    depth,
                    poly.iter().map(|q| at(*q)).collect(),
                    c,
                    Fill::Convex,
                ));
            }
        }
    }
    // Far first (the larger depth is nearer the viewer: drawn last).
    faces.sort_by(|a, b| a.0.total_cmp(&b.0));
    let edge = egui::Stroke::new(0.6, egui::Color32::from_black_alpha(140));
    for (_, pts, c, fill) in faces {
        match fill {
            // An angle's end: an L of six corners, two rectangles.
            Fill::Angle if pts.len() == 6 => {
                for quad in [[0, 1, 2, 3], [0, 3, 4, 5]] {
                    painter.add(egui::Shape::convex_polygon(
                        quad.iter().map(|i| pts[*i]).collect(),
                        c,
                        egui::Stroke::NONE,
                    ));
                }
                painter.add(egui::Shape::closed_line(pts, edge));
            }
            Fill::Angle | Fill::Outline => {
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.2, c)));
            }
            Fill::Convex => {
                painter.add(egui::Shape::convex_polygon(pts, c, edge));
            }
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
            Mark::Bolt { parts, assumed, .. } => {
                let color = if *assumed {
                    egui::Color32::from_rgb(235, 170, 60)
                } else {
                    egui::Color32::from_rgb(210, 60, 60)
                };
                let fill = if dark {
                    egui::Color32::from_gray(45)
                } else {
                    egui::Color32::from_gray(225)
                };
                for part in parts {
                    let pts: Vec<egui::Pos2> = part.iter().map(|p| at(*p)).collect();
                    if pts.len() >= 3 {
                        painter.add(egui::Shape::convex_polygon(
                            pts,
                            fill,
                            egui::Stroke::new(1.0, color),
                        ));
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
