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
}

impl SectionDisplay {
    /// The menu: a tick per item and view. Returns whether it changed.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        egui::Grid::new("section-display").show(ui, |ui| {
            ui.label("");
            for v in View::ALL {
                ui.label(v.name());
            }
            ui.end_row();
            for (name, ticks) in [("Section label", &mut self.label), ("Sag", &mut self.sag)] {
                ui.label(name);
                for v in View::ALL {
                    changed |= ui.checkbox(&mut ticks[v as usize], "").changed();
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
                kind,
                text,
                color,
            }
        };
        if label && !shape.spans.is_empty() && labelled.insert(shape.section.clone()) {
            out.push(note(
                shape.spans.len() / 2,
                Kind::Label,
                shape.section.clone(),
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

/// Spans shorter than this on screen, px, get no note (they would only
/// overlap).
pub const MIN_SPAN_PX: f32 = 40.0;

/// Writes notes on a view, leaving out those that would overlap one
/// already written (parallel wires: the first one's note).
#[derive(Default)]
pub struct Placer {
    taken: Vec<egui::Rect>,
}

impl Placer {
    /// Writes a note at screen point `p` (the wire's mid-point): labels
    /// above the wire in the section's colour, sags below it.
    pub fn paint(&mut self, painter: &egui::Painter, p: egui::Pos2, note: &Note) {
        let (offset, align, font, color) = match note.kind {
            Kind::Label => (
                egui::vec2(0.0, -6.0),
                egui::Align2::CENTER_BOTTOM,
                egui::FontId::proportional(12.0),
                egui::Color32::from_rgb(note.color[0], note.color[1], note.color[2]),
            ),
            Kind::Sag => (
                egui::vec2(0.0, 6.0),
                egui::Align2::CENTER_TOP,
                egui::FontId::proportional(11.0),
                egui::Color32::from_gray(220),
            ),
        };
        let galley = painter.layout_no_wrap(note.text.clone(), font, color);
        let rect = align.anchor_size(p + offset, galley.size()).expand(1.5);
        if self.taken.iter().any(|r| r.intersects(rect)) {
            return;
        }
        self.taken.push(rect);
        // A dark halo keeps the text readable over wires and ground.
        painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(140));
        painter.galley(rect.shrink(1.5).min, galley, color);
    }
}
