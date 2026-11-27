// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Detailing practice** (O44 L29, the owner's L24 question 5 of
//! 2026-11-25: "a small editor following the tool-window pattern"):
//! Design › Detailing practice… (or *Edit…* under Lay out joints' practice)
//! opens one tool window that edits the structure file's own
//! `[layout_practice]` — the shop's preferences the automatic joint layout
//! follows — over the code plugin's practice or the program's default,
//! value by value: only what differs from those is written. Every value is
//! checked as the layout checks it (refused values said, Save greyed); each
//! field says its effect on the results and on the time, measured. **Save**
//! writes it as one undo step (structure file version 27, or 30 with a
//! plate thickness rule); the joints laid out stay as they are until the
//! next run of Lay out joints.

use eframe::egui;
use ohl_structure_link::practice::{
    self, CLASSES, DetailingPractice, Fabrication, FacePattern, PluginAnswer,
};

use crate::app::StructureApp;

/// The manual's page of the window (page, anchor).
pub const PRACTICE_PAGE: (&str, &str) = ("user-guide/detailing-practice", PRACTICE_ANCHOR);
const PRACTICE_ANCHOR: &str = "detailing-practice";

/// The Detailing practice window.
pub struct PracticeWindow {
    /// The practice as edited (in force when the window opened).
    pub edit: DetailingPractice,
    /// What the file's own table is laid over: the code plugin's practice,
    /// else the default.
    pub under: DetailingPractice,
    /// Where `under` comes from, in words.
    pub under_from: String,
    /// The plate thicknesses as typed (mm, separated by commas).
    pub thicknesses: String,
    /// Why the practice given is not used, or the plugin failed (said).
    pub note: Option<String>,
    /// Why the last Save was refused.
    pub error: Option<String>,
}

impl PracticeWindow {
    /// The thicknesses typed, read (m); Err: why not.
    fn typed_thicknesses(&self) -> Result<Vec<f64>, String> {
        let mut out = Vec::new();
        for w in self.thicknesses.split([',', ';']) {
            let w = w.trim().trim_end_matches("mm").trim();
            if w.is_empty() {
                continue;
            }
            let v: f64 = w
                .parse()
                .map_err(|_| format!("plate thicknesses: \"{w}\" is not a number of mm"))?;
            out.push(v / 1000.0);
        }
        out.sort_by(f64::total_cmp);
        Ok(out)
    }

    /// The practice as edited with the thicknesses typed, and what is
    /// refused in it (empty: none).
    pub fn checked(&self) -> (DetailingPractice, Vec<String>) {
        let mut p = self.edit.clone();
        match self.typed_thicknesses() {
            Ok(t) => {
                p.thicknesses = t;
                let r = p.refusals();
                (p, r)
            }
            Err(e) => (p, vec![e]),
        }
    }
}

fn thicknesses_text(t: &[f64]) -> String {
    t.iter()
        .map(|v| {
            let s = ohl_structure_link::mm_text(*v);
            s.trim_end_matches(" mm").to_owned()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl StructureApp {
    /// Opens the Detailing practice window: the practice in force (the
    /// file's own over the code plugin's, value by value, over the
    /// default), edited over the plugin's or the default.
    pub(crate) fn open_practice_window(&mut self) {
        let Some(f) = self.file.as_ref() else {
            return;
        };
        let (plugin, failed): (Option<PluginAnswer>, Option<String>) =
            match ohl_structure_link::plugin_practice(
                f.text_ref(),
                f.path().and_then(std::path::Path::parent),
            ) {
                Ok(p) => (p, None),
                Err(e) => (None, Some(e)),
            };
        let (under, under_from, mut note) = match &plugin {
            Some(p) => match DetailingPractice::from_table(&p.table) {
                Ok(x) => (
                    x,
                    format!("the code plugin {} (method {})", p.plugin, p.method),
                    None,
                ),
                Err(e) => (
                    DetailingPractice::default(),
                    "the program's default".to_owned(),
                    Some(format!(
                        "The code plugin's detailing practice is refused: {e}. The default is \
                         used under this file's own."
                    )),
                ),
            },
            None => (
                DetailingPractice::default(),
                "the program's default".to_owned(),
                failed.map(|e| format!("{e}. The code plugin's detailing practice is not used.")),
            ),
        };
        let own = f.structure().layout_practice.clone();
        let edit = match practice::in_force(own.as_ref(), plugin.as_ref()) {
            Ok(Some(p)) => p.practice,
            Ok(None) => under.clone(),
            Err(e) => {
                note = Some(format!(
                    "{e}. Shown here: {under_from}'s values; Save replaces this file's own table."
                ));
                under.clone()
            }
        };
        self.practice_window = Some(PracticeWindow {
            thicknesses: thicknesses_text(&edit.thicknesses),
            edit,
            under,
            under_from,
            note,
            error: None,
        });
    }

    /// The Detailing practice window ([`Self::open_practice_window`]); its
    /// page of the manual: [`PRACTICE_PAGE`].
    pub(crate) fn practice_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.practice_window.take() else {
            return;
        };
        if self.file.is_none() {
            return;
        }
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        // Each value's effect on the results and on the time (measured):
        // the same words Lay out joints shows.
        let effects = w.edit.rows();
        let effect = |key: &str| -> &'static str {
            effects
                .iter()
                .find(|r| r.0.contains(&key))
                .map_or("", |r| r.3)
        };
        egui::Window::new("Detailing practice")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(600.0)
            .default_height(640.0)
            .show(ctx, |ui| {
                ohl_ui::tool_window::intro(
                    ui,
                    "Sets the shop's preferences the automatic joint layout follows for this \
                     tower — which parts it may make, the faces, the limits, the plates and \
                     the picked bolt — written in the structure file.",
                    "A detailing practice holds what a fabricator or utility does differently \
                     (O44 L24): the forms of fabrication and their order, the faces tried first, \
                     the limits, the flatness, the plates' bend, thickness and shape, the \
                     picked bolt's pattern (per member class too). It only chooses among what \
                     the layout does already: the clash check, the joint checks after writing \
                     and the guard that no joint ends worse stay in force whatever it says. \
                     This window writes the file's own [layout_practice]: only the values that \
                     differ from the code plugin's practice or the program's default (laid \
                     under it, value by value). Each value is checked as the layout checks it; \
                     a value out of range is said in red and Save waits. The joints laid out \
                     stay as they are until Lay out joints runs again. One undo step.",
                    PRACTICE_ANCHOR,
                    || crate::help::open_section(PRACTICE_PAGE.0, PRACTICE_PAGE.1),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Laid over {}; the values given differently here are written in this \
                         file.",
                        w.under_from
                    ))
                    .small(),
                );
                if let Some(n) = &w.note {
                    crate::app::error_label(ui, egui::RichText::new(n.as_str()).small());
                }
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 330.0).clamp(240.0, 900.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| practice_fields(ui, &mut w, &effect));
                let (checked, refused) = w.checked();
                // Always visible: what is refused, and what is not on the
                // safe side.
                for r in &refused {
                    crate::app::error_label(ui, egui::RichText::new(r.as_str()).small());
                }
                if checked.has_thickness_rules() {
                    ui.label(
                        egui::RichText::new(practice::THICKER_NOTE)
                            .small()
                            .color(ui.visuals().warn_fg_color),
                    );
                }
                for n in checked.notes() {
                    if n != practice::THICKER_NOTE {
                        ui.label(egui::RichText::new(n).small().weak());
                    }
                }
                if let Some(e) = &w.error {
                    crate::app::error_label(ui, e.as_str());
                }
                let own = checked.own_over(&w.under);
                let what = match &own {
                    Some(_) => "Save in the structure file",
                    None => "Save (nothing of its own)",
                };
                use ohl_ui::tool_window::{Keys, Pressed, buttons};
                match buttons(
                    ui,
                    what,
                    refused.is_empty(),
                    Some("A value is refused (said in red above)"),
                    Keys::Escape,
                ) {
                    Some(Pressed::Action) => save = true,
                    Some(Pressed::Cancel) => cancel = true,
                    None => {}
                }
            });
        if save {
            let (checked, _) = w.checked();
            let own = checked.own_over(&w.under);
            let r = self.file.as_mut().map(|f| {
                f.edit_alone("Detailing practice", |doc, _| {
                    practice::write_own(doc, own.as_ref())
                })
            });
            match r {
                Some(Ok(())) => {
                    self.message = Some((
                        match &own {
                            Some(_) => "detailing practice saved in the structure file (one undo \
                                        step); the joints laid out stay as they are until Lay \
                                        out joints runs again"
                                .to_owned(),
                            None => format!(
                                "detailing practice: this file gives none of its own now — {} \
                                 in force (one undo step)",
                                w.under_from
                            ),
                        },
                        false,
                    ));
                    // Lay out joints, if open, takes it at once.
                    if self.layout_window.is_some() {
                        let record = ohl_structure_link::LayoutRecord::of(
                            self.file.as_ref().expect("open").structure(),
                        );
                        let (p, n) = self.layout_practice(&record);
                        if let Some(lw) = self.layout_window.as_mut() {
                            lw.practice = p;
                            lw.practice_note = n;
                        }
                    }
                    return;
                }
                Some(Err(e)) => w.error = Some(e),
                None => return,
            }
        }
        if open && !cancel {
            self.practice_window = Some(w);
        }
    }
}

/// The window's numbered sections.
fn practice_fields(
    ui: &mut egui::Ui,
    w: &mut PracticeWindow,
    effect: &dyn Fn(&str) -> &'static str,
) {
    use ohl_ui::tool_window::{heading, label};
    let p = &mut w.edit;
    ui.horizontal(|ui| {
        label(
            ui,
            "Name",
            "Shown with the practice in Lay out joints and the run's notes; no effect on the \
             layout.",
        );
        ui.text_edit_singleline(&mut p.name);
    });
    heading(ui, "1. What the shop makes", effect("fabrication"));
    let n = p.fabrication.len();
    let mut swap: Option<(usize, usize)> = None;
    egui::Grid::new("practice-fabrication")
        .num_columns(3)
        .show(ui, |ui| {
            for (i, (form, on)) in p.fabrication.iter_mut().enumerate() {
                let fixed = matches!(form, Fabrication::Direct | Fabrication::Plate);
                ui.add_enabled(
                    !fixed,
                    egui::Checkbox::new(on, format!("{}. {}", i + 1, form.label())),
                )
                .on_disabled_hover_text(
                    "Always allowed: the layout's first two steps; it has no way without them",
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(i > 0, egui::Button::new("⏶").small())
                        .on_hover_text("Earlier in the order (decides only ties)")
                        .clicked()
                    {
                        swap = Some((i - 1, i));
                    }
                    if ui
                        .add_enabled(i + 1 < n, egui::Button::new("⏷").small())
                        .on_hover_text("Later in the order (decides only ties)")
                        .clicked()
                    {
                        swap = Some((i, i + 1));
                    }
                });
                ui.end_row();
            }
        });
    if let Some((a, b)) = swap {
        p.fabrication.swap(a, b);
    }
    heading(ui, "2. Faces", effect("faces.pattern"));
    egui::Grid::new("practice-faces")
        .num_columns(2)
        .show(ui, |ui| {
            label(ui, "Tried first on a leg's flange", effect("faces.pattern"));
            egui::ComboBox::from_id_salt("practice-pattern")
                .selected_text(match p.face_pattern {
                    FacePattern::DiagonalInner => "a diagonal onto the inner face",
                    FacePattern::LaterClassFirst => "the later class",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut p.face_pattern,
                        FacePattern::DiagonalInner,
                        "a diagonal onto the inner face",
                    );
                    ui.selectable_value(
                        &mut p.face_pattern,
                        FacePattern::LaterClassFirst,
                        "the later class",
                    );
                });
            ui.end_row();
            label(
                ui,
                "Members on either side of a flange",
                effect("faces.both_sides_of_flange"),
            );
            ui.checkbox(&mut p.both_sides_of_flange, "");
            ui.end_row();
            label(
                ui,
                "Ends on both faces of one plate",
                effect("faces.both_faces_of_plate"),
            );
            ui.checkbox(&mut p.both_faces_of_plate, "");
            ui.end_row();
        });
    heading(ui, "3. Limits", effect("limits.gap"));
    egui::Grid::new("practice-limits")
        .num_columns(2)
        .show(ui, |ui| {
            let mm = |ui: &mut egui::Ui, v: &mut f64, lo: f64, hi: f64| {
                let mut x = *v * 1000.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut x)
                            .range(lo * 1000.0..=hi * 1000.0)
                            .speed(0.5),
                    )
                    .changed()
                {
                    *v = x / 1000.0;
                }
            };
            label(ui, "Gap between steel (mm)", effect("limits.gap"));
            mm(ui, &mut p.gap, practice::GAP_RANGE.0, practice::GAP_RANGE.1);
            ui.end_row();
            label(ui, "Longest pull-back (mm)", effect("limits.pull_back"));
            mm(
                ui,
                &mut p.pull_back,
                practice::PULL_BACK_RANGE.0,
                practice::PULL_BACK_RANGE.1,
            );
            ui.end_row();
            label(ui, "Shift limit (mm)", effect("limits.shift"));
            mm(
                ui,
                &mut p.shift,
                practice::SHIFT_RANGE.0,
                practice::SHIFT_RANGE.1,
            );
            ui.end_row();
            label(ui, "Flatness (°)", effect("limits.flatness_deg"));
            ui.add(
                egui::DragValue::new(&mut p.flatness_deg)
                    .range(practice::FLATNESS_RANGE.0..=practice::FLATNESS_RANGE.1)
                    .speed(0.05),
            );
            ui.end_row();
            label(
                ui,
                "Small bend, at most (°)",
                effect("limits.small_bend_deg"),
            );
            ui.add(
                egui::DragValue::new(&mut p.small_bend_deg)
                    .range(p.flatness_deg..=practice::SMALL_BEND_MAX)
                    .speed(0.5),
            );
            ui.end_row();
            label(
                ui,
                "Welded plate's axis offset, at most (mm)",
                effect("limits.own_plane_offset"),
            );
            mm(
                ui,
                &mut p.own_plane_offset,
                practice::OWN_OFFSET_RANGE.0,
                practice::OWN_OFFSET_RANGE.1,
            );
            ui.end_row();
        });
    heading(ui, "4. Plates", effect("plates.least_thickness"));
    egui::Grid::new("practice-plates")
        .num_columns(2)
        .show(ui, |ui| {
            label(
                ui,
                "Bend's inside radius (× thickness)",
                effect("plates.bend_radius"),
            );
            ui.add(
                egui::DragValue::new(&mut p.bend_radius)
                    .range(practice::BEND_RADIUS_RANGE.0..=practice::BEND_RADIUS_RANGE.1)
                    .speed(0.1),
            );
            ui.end_row();
            label(
                ui,
                "Plate between angles, least gap (× thickest leg)",
                effect("plates.between_least"),
            );
            ui.add(
                egui::DragValue::new(&mut p.between_least)
                    .range(practice::BETWEEN_RANGE.0..=practice::BETWEEN_RANGE.1)
                    .speed(0.05),
            );
            ui.end_row();
            label(
                ui,
                "Plates may stop short of the work point",
                effect("plates.short"),
            );
            ui.checkbox(&mut p.short_plates, "");
            ui.end_row();
            label(
                ui,
                "Least plate thickness (mm, 0: none)",
                effect("plates.least_thickness"),
            );
            let mut x = p.least_thickness * 1000.0;
            if ui
                .add(
                    egui::DragValue::new(&mut x)
                        .range(0.0..=practice::LEAST_THICKNESS_RANGE.1 * 1000.0)
                        .speed(0.5),
                )
                .changed()
            {
                p.least_thickness = x / 1000.0;
            }
            ui.end_row();
            label(
                ui,
                "Plate thicknesses made (mm, empty: any)",
                effect("plates.thicknesses"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut w.thicknesses)
                    .hint_text("e.g. 6, 8, 10, 12, 15, 20")
                    .desired_width(220.0),
            );
            ui.end_row();
        });
    let p = &mut w.edit;
    heading(ui, "5. The picked bolt", effect("bolts.count"));
    egui::Grid::new("practice-bolts")
        .num_columns(2)
        .show(ui, |ui| {
            label(ui, "Bolts per end, in one line", effect("bolts.count"));
            ui.add(
                egui::DragValue::new(&mut p.bolt_count)
                    .range(practice::BOLT_COUNT_RANGE.0..=practice::BOLT_COUNT_RANGE.1),
            );
            ui.end_row();
            for class in CLASSES {
                label(ui, &format!("… for {class}"), effect("classes"));
                ui.horizontal(|ui| {
                    let now = p.classes.get(class).and_then(|c| c.bolt_count);
                    let mut on = now.is_some();
                    let mut n = now.unwrap_or(p.bolt_count);
                    let a = ui.checkbox(&mut on, "own").changed();
                    let b = ui
                        .add_enabled(
                            on,
                            egui::DragValue::new(&mut n)
                                .range(practice::BOLT_COUNT_RANGE.0..=practice::BOLT_COUNT_RANGE.1),
                        )
                        .changed();
                    if a || b {
                        if on {
                            p.classes.entry(class.to_owned()).or_default().bolt_count = Some(n);
                        } else {
                            p.classes.remove(class);
                        }
                    }
                });
                ui.end_row();
            }
        });
}
