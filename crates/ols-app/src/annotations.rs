// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Section display options (Sections › Display options): what is written
//! next to the wires, chosen for each view on its own — the section's
//! label and each span's sag, so far.

use eframe::egui;

use crate::model::Model;

/// The views that write next to the wires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Plan view.
    Plan = 0,
    /// Profile view.
    Profile = 1,
    /// 3D view.
    ThreeD = 2,
}

impl View {
    /// Every view, in the menu's order.
    pub const ALL: [View; 3] = [View::Plan, View::Profile, View::ThreeD];

    /// Name in the menu.
    pub fn name(self) -> &'static str {
        match self {
            View::Plan => "Plan",
            View::Profile => "Profile",
            View::ThreeD => "3D",
        }
    }
}

/// What is written next to the wires, per view (indexed by [`View`]).
/// One user's choice, remembered with the project's view preferences.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SectionDisplay {
    /// The section's id, once per section (at its middle span).
    #[serde(default)]
    pub label: [bool; 3],
    /// Each span's sag, at mid-span.
    #[serde(default)]
    pub sag: [bool; 3],
    /// The direction of each section and jumper in its circuit (arrows).
    #[serde(default)]
    pub direction: [bool; 3],
    /// What colours the wires: their circuits or their systems.
    #[serde(default)]
    pub coloring: crate::wire_style::Coloring,
    /// Structures: their id.
    #[serde(default)]
    pub structure_label: [bool; 3],
    /// Their points where strings hang: the label.
    #[serde(default)]
    pub point_label: [bool; 3],
    /// Their strings: the label.
    #[serde(default)]
    pub string_label: [bool; 3],
    /// Their wire points: the label and set.
    #[serde(default)]
    pub wire_label: [bool; 3],
    /// Every label shown with the object's description.
    #[serde(default)]
    pub descriptions: [bool; 3],
}

impl SectionDisplay {
    /// The Structures menu's part: labels of the structures, their
    /// points, strings and wire points, and descriptions, per view (plan
    /// and 3D; the profile names its structures always). Returns whether
    /// it changed.
    pub fn structures_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        egui::Grid::new("structure-display").show(ui, |ui| {
            ui.label("");
            for v in [View::Plan, View::ThreeD] {
                ui.label(v.name());
            }
            ui.end_row();
            for (name, ticks) in [
                ("Structure id", &mut self.structure_label),
                ("Points (label)", &mut self.point_label),
                ("Strings (label)", &mut self.string_label),
                ("Wire points (label, set)", &mut self.wire_label),
                ("With descriptions", &mut self.descriptions),
            ] {
                ui.label(name);
                for v in [View::Plan, View::ThreeD] {
                    let on = &mut ticks[v as usize];
                    let r = ui.checkbox(on, "");
                    let checked = *on;
                    r.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Checkbox,
                            true,
                            checked,
                            format!("{name} in {}", v.name()),
                        )
                    });
                    changed |= r.changed();
                }
                ui.end_row();
            }
        });
        changed
    }

    /// The menu: the wires' colouring, a tick per item and view. Returns
    /// whether it changed.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::wire_style::Coloring;
        let mut changed = false;
        ui.label(egui::RichText::new("Wire colours").small().weak());
        changed |= ui
            .radio_value(&mut self.coloring, Coloring::Circuits, "Circuits")
            .on_hover_text(
                "Each section's colour (a circuit's sections share one), else its phase's",
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut self.coloring,
                Coloring::Systems,
                "Systems, phases by line code",
            )
            .on_hover_text(
                "Each system's colour (Systems table); its first phase solid, the second ·–·–, \
                 the third ·––·––; earth wires grey",
            )
            .changed();
        ui.separator();
        egui::Grid::new("section-display").show(ui, |ui| {
            ui.label("");
            for v in View::ALL {
                ui.label(v.name());
            }
            ui.end_row();
            for (name, ticks) in [
                ("Section label", &mut self.label),
                ("Sag", &mut self.sag),
                ("Direction", &mut self.direction),
                ("With descriptions", &mut self.descriptions),
            ] {
                ui.label(name);
                for v in View::ALL {
                    let on = &mut ticks[v as usize];
                    let r = ui.checkbox(on, "");
                    // Named for screen readers (and tests): "Sag in Plan".
                    let checked = *on;
                    r.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Checkbox,
                            true,
                            checked,
                            format!("{name} in {}", v.name()),
                        )
                    });
                    changed |= r.changed();
                }
                ui.end_row();
            }
        });
        changed
    }
}

/// What a note says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The section's id.
    Label,
    /// A span's sag.
    Sag,
    /// An object's label at a point (a structure, a point, a string, a
    /// wire point).
    Object,
}

/// A note next to a wire: where (line coordinates), what, and the span's
/// ends (to leave out spans too short on screen).
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// Section id.
    pub section: String,
    /// Span index in the section.
    pub span: usize,
    /// Where: the span's mid-point.
    pub at: [f64; 3],
    /// The span's first and last point.
    pub ends: [[f64; 3]; 2],
    /// The span's points (where else to write the note when its
    /// mid-point is out of sight).
    pub points: Vec<[f64; 3]>,
    /// Label or sag.
    pub kind: Kind,
    /// The text.
    pub text: String,
    /// Colour (the section's).
    pub color: [u8; 3],
}

/// The notes of `view` for the sections shown in weather case `case`: a
/// label per section (its first shape shown), a sag per span shown.
pub fn notes(model: &Model, case: usize, view: View) -> Vec<Note> {
    let display = &model.section_display;
    let (label, sag) = (display.label[view as usize], display.sag[view as usize]);
    if !label && !sag {
        return Vec::new();
    }
    let line = &model.line;
    let mut labelled = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for shape in model.shapes_shown(case) {
        let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
            continue;
        };
        let color = crate::scene::rgb(&line.section_color(section));
        let note = |k: usize, kind, text| {
            let span = &shape.spans[k];
            Note {
                section: shape.section.clone(),
                span: k,
                at: span.mid_point,
                ends: [
                    *span.points.first().unwrap_or(&span.mid_point),
                    *span.points.last().unwrap_or(&span.mid_point),
                ],
                points: span.points.clone(),
                kind,
                text,
                color,
            }
        };
        if label && !shape.spans.is_empty() && labelled.insert(shape.section.clone()) {
            out.push(note(
                shape.spans.len() / 2,
                Kind::Label,
                with_description(
                    shape.section.clone(),
                    &section.description,
                    display.descriptions[view as usize],
                ),
            ));
        }
        if sag {
            for (k, s) in shape.spans.iter().enumerate() {
                out.push(note(k, Kind::Sag, format!("{:.2} m", s.sag)));
            }
        }
    }
    out
}

/// `text` and, when asked for and given, ` — description`.
fn with_description(text: String, description: &str, show: bool) -> String {
    if show && !description.trim().is_empty() {
        format!("{text} — {}", description.trim())
    } else {
        text
    }
}

/// The labels of `view` for the structures, their points, strings (where
/// drawn in weather case `case`) and wire points, as chosen in Structures
/// › Display options.
pub fn object_notes(model: &Model, case: usize, view: View) -> Vec<Note> {
    let d = &model.section_display;
    let v = view as usize;
    let (structures, points, strings, wires) = (
        d.structure_label[v],
        d.point_label[v],
        d.string_label[v],
        d.wire_label[v],
    );
    if !(structures || points || strings || wires) {
        return Vec::new();
    }
    let show = d.descriptions[v];
    let pieces = if strings || wires {
        crate::selection::string_pieces(model, case)
    } else {
        Default::default()
    };
    let note = |at: [f64; 3], text: String, color: [u8; 3]| Note {
        section: String::new(),
        span: 0,
        at,
        ends: [at, at],
        points: vec![at],
        kind: Kind::Object,
        text,
        color,
    };
    let mut out = Vec::new();
    for s in &model.line.structures {
        if structures {
            let top = s
                .points
                .iter()
                .map(|p| p.position[2])
                .chain(s.members.iter().flatten().map(|p| p[2]))
                .fold(s.position[2] + 5.0, f64::max);
            out.push(note(
                [s.position[0], s.position[1], top + 2.0],
                with_description(s.id.clone(), &s.description, show),
                [235, 235, 240],
            ));
        }
        if points {
            for p in &s.points {
                out.push(note(
                    p.position,
                    with_description(p.label.clone(), &p.description, show),
                    [255, 160, 80],
                ));
            }
        }
        for x in &s.strings {
            let drawn = pieces.get(&(s.id.clone(), x.label.clone()));
            let (a, b) = match drawn.and_then(|v| v.first()) {
                Some(&(a, b)) => (a, b),
                None => match (s.node(&x.top), s.wire(&x.wire)) {
                    (Some(t), Some(w)) => (t.position(), w.position),
                    _ => continue,
                },
            };
            if strings {
                out.push(note(
                    [0, 1, 2].map(|k| 0.5 * (a[k] + b[k])),
                    with_description(x.label.clone(), &x.description, show),
                    [120, 180, 255],
                ));
            }
            // Its wire point, where drawn (the first string to it).
            if wires
                && let Some(w) = s.wire(&x.wire)
                && s.strings_to(&w.label)
                    .next()
                    .is_some_and(|y| y.label == x.label)
            {
                let text = w
                    .set
                    .map_or(w.label.clone(), |set| format!("{} (set {set})", w.label));
                out.push(note(
                    b,
                    with_description(text, &w.description, show),
                    [90, 200, 255],
                ));
            }
        }
        if wires {
            // Wire points no string reaches (clamped to the structure).
            for w in s
                .wires
                .iter()
                .filter(|w| s.strings_to(&w.label).next().is_none())
            {
                let text = w
                    .set
                    .map_or(w.label.clone(), |set| format!("{} (set {set})", w.label));
                out.push(note(
                    w.position,
                    with_description(text, &w.description, show),
                    [90, 200, 255],
                ));
            }
        }
    }
    out
}

/// Spans shorter than this on screen, px, get no note (they would only
/// overlap).
pub const MIN_SPAN_PX: f32 = 40.0;

/// Where to write a note in a perspective view: at the span's mid-point
/// when it is in sight, else at the point of the wire in sight nearest the
/// view's centre (close to a wire, its middle may be behind the camera or
/// off the side). `None`: no part in sight, or the span too short on
/// screen. `screen` gives a point's screen position, `None` behind the
/// camera.
pub fn anchor(
    note: &Note,
    rect: egui::Rect,
    screen: impl Fn([f64; 3]) -> Option<egui::Pos2>,
) -> Option<egui::Pos2> {
    // An object's label: where it is, when in sight.
    if note.kind == Kind::Object {
        return screen(note.at).filter(|p| rect.contains(*p));
    }
    // Too short on screen only when both ends are in front of the camera.
    if let (Some(a), Some(b)) = (screen(note.ends[0]), screen(note.ends[1]))
        && a.distance(b) < MIN_SPAN_PX
    {
        return None;
    }
    if let Some(p) = screen(note.at).filter(|p| rect.contains(*p)) {
        return Some(p);
    }
    let centre = rect.center();
    note.points
        .iter()
        .filter_map(|p| screen(*p))
        .filter(|p| rect.shrink(20.0).contains(*p))
        .min_by(|a, b| a.distance(centre).total_cmp(&b.distance(centre)))
}

/// Writes notes on a view, leaving out those that would overlap one
/// already written (parallel wires: the first one's note).
#[derive(Default)]
pub struct Placer {
    taken: Vec<egui::Rect>,
}

impl Placer {
    /// Writes a note at screen point `p` (on the wire), in the section's
    /// colour: labels above the wire, sags below it.
    pub fn paint(&mut self, painter: &egui::Painter, p: egui::Pos2, note: &Note) {
        let color = egui::Color32::from_rgb(note.color[0], note.color[1], note.color[2]);
        let (offset, align, font) = match note.kind {
            Kind::Label => (
                egui::vec2(0.0, -5.0),
                egui::Align2::CENTER_BOTTOM,
                egui::FontId::proportional(12.0),
            ),
            Kind::Sag => (
                egui::vec2(0.0, 5.0),
                egui::Align2::CENTER_TOP,
                egui::FontId::proportional(11.0),
            ),
            Kind::Object => (
                egui::vec2(6.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                egui::FontId::proportional(11.0),
            ),
        };
        let galley = painter.layout_no_wrap(note.text.clone(), font, color);
        let rect = align.anchor_size(p + offset, galley.size());
        if self.taken.iter().any(|r| r.intersects(rect.expand(1.0))) {
            return;
        }
        self.taken.push(rect);
        painter.galley(rect.min, galley, color);
    }
}
