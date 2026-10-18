// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Pole panels: in Pole (Body menu) the pole's outside lines (its
//! outline by height, round or polygonal; drawn in 3D as a wireframe),
//! in Pole design (Design menu) what the pole check needs (material,
//! wall or a hollow section of the section library, strength). A `.olsp` pole
//! named by the file is brought into it in one step. Every change is one
//! edit of the file (one undo step; a value dragged is one step).

use std::path::{Path, PathBuf};

use eframe::egui;
use ols_sections::{SectionLibrary, Shape};
use ols_structure_edit::StructureFile;
use ols_structure_edit::pole::{self as pole_edit, POLE_FIELDS};
use ols_structures::pole::POLE_MATERIALS;
use ols_structures::structures::StructureKind;
use ols_structures::{InlinePole, PoleRef, PoleShape, StructureType};

use crate::app::{StructureApp, error_label};

/// The Pole panels' state between frames.
#[derive(Default)]
pub struct PoleState {
    /// The section library next to the file (its folder and its Sections
    /// folder), read once per folder.
    library: Option<(PathBuf, Result<SectionLibrary, String>)>,
}

impl PoleState {
    /// The section library of the folder `dir`, read when first asked.
    fn library(&mut self, dir: &Path) -> &Result<SectionLibrary, String> {
        if self.library.as_ref().is_none_or(|l| l.0 != dir) {
            let mut lib = SectionLibrary::default();
            let mut err = None;
            for d in [dir.to_path_buf(), dir.join("Sections")] {
                if !d.is_dir() {
                    continue;
                }
                match SectionLibrary::load(&d) {
                    Ok(l) => {
                        lib.merge(l);
                    }
                    Err(e) => err = Some(e),
                }
            }
            self.library = Some((dir.to_path_buf(), err.map_or(Ok(lib), Err)));
        }
        &self.library.as_ref().expect("read above").1
    }
}

/// Whether the structure shows the Pole panels: a pole, or a structure of
/// kind pole.
pub fn has_pole(t: &StructureType) -> bool {
    t.pole.is_some() || t.kind == StructureKind::Pole
}

/// While the pointer is pressed on `r` its changes are one undo step.
fn hold(file: &mut StructureFile, r: &egui::Response, what: &str) {
    if r.is_pointer_button_down_on() {
        file.begin(what);
    }
}

/// The `.olsp` file a `pole = "…"` names, next to the structure file: by
/// file name, else the one with that id.
pub(crate) fn pole_file(dir: &Path, name: &str) -> Option<PathBuf> {
    if name.ends_with(".olsp") {
        return Some(dir.join(name));
    }
    ols_core::files::files_with_extension(dir, "olsp")
        .into_iter()
        .find(|p| {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|t| t.parse::<toml::Table>().ok())
                .and_then(|t| t.get("id").and_then(|v| v.as_str()).map(str::to_owned))
                .as_deref()
                == Some(name)
        })
}

impl StructureApp {
    /// Body › Pole: shape, embedment, the outline rows, + Height; a named
    /// `.olsp` brought in.
    pub(crate) fn pole_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        let dir = f
            .path()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."))
            .to_path_buf();
        match &t.pole {
            Some(PoleRef::File(name)) => {
                ui.label(format!(
                    "The pole is the file \"{name}\" next to this one (level 3, checked by the \
                     pole check). Bring it into this file to edit its outline and design here."
                ));
                if ui
                    .button("Bring it into this file")
                    .on_hover_text(
                        "Its diameters become a two-height outline from the feet, its other \
                         fields are copied: the pole check gives the same numbers",
                    )
                    .clicked()
                {
                    let read = pole_file(&dir, name)
                        .ok_or_else(|| format!("no .olsp file \"{name}\" in {}", dir.display()))
                        .and_then(|p| {
                            std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))
                        });
                    self.message = Some(
                        match read.and_then(|text| {
                            f.edit("bring the pole in", |doc, t| {
                                pole_edit::bring_in(doc, t, &text)
                            })
                        }) {
                            Ok(()) => ("the pole is now described in this file".into(), false),
                            Err(e) => (e, true),
                        },
                    );
                }
                return;
            }
            None => {
                ui.label(
                    egui::RichText::new(
                        "No pole yet: its outline (diameters by height) makes level 2.",
                    )
                    .weak(),
                );
                if ui
                    .button("Describe the pole")
                    .on_hover_text("An outline from the feet to the top, to be adjusted")
                    .clicked()
                {
                    let r = f.edit("pole outline", pole_edit::add_outline_row);
                    if let Err(e) = r {
                        self.message = Some((e, true));
                    }
                }
                return;
            }
            Some(PoleRef::Inline(_)) => {}
        }
        let p: &InlinePole = t.pole.as_ref().and_then(PoleRef::inline).expect("inline");
        ui.horizontal_wrapped(|ui| {
            ui.label("cross-section").on_hover_text(
                "Round, or a regular polygon (its diameters across flats, as poles are made)",
            );
            let mut shape = p.shape;
            egui::ComboBox::from_id_salt("pole-shape")
                .selected_text(shape.key())
                .width(90.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut shape, PoleShape::Round, "round");
                    ui.selectable_value(&mut shape, PoleShape::Polygon, "polygon");
                });
            let mut sides = p.sides.unwrap_or(12);
            let r = ui.add_enabled(
                shape == PoleShape::Polygon,
                egui::DragValue::new(&mut sides)
                    .range(3..=64)
                    .suffix(" sides"),
            );
            if shape != p.shape || r.changed() {
                f.edit("pole shape", |doc, _| {
                    pole_edit::set_shape(doc, shape, sides)
                });
            }
            ui.separator();
            ui.label("embedment")
                .on_hover_text("Depth in the ground: the pole's length is its height plus this");
            let mut e = p.embedment.map_or(0.0, |e| e.si());
            let r = ui.add(
                egui::DragValue::new(&mut e)
                    .speed(0.05)
                    .range(0.0..=20.0)
                    .max_decimals(3)
                    .suffix(" m"),
            );
            hold(f, &r, "pole embedment");
            if r.changed() {
                let text = if e > 0.0 {
                    format!("{e:.3} m")
                } else {
                    String::new()
                };
                let _ = f.edit("pole embedment", |doc, _| {
                    pole_edit::set_field(doc, "embedment", &text)
                });
            }
        });
        ui.separator();
        ui.strong("Outline").on_hover_text(
            "The pole's outside lines, from the feet (the ground line) up: a diameter at each \
             height, linear between them",
        );
        let rings = p.rings();
        let mut drop: Option<usize> = None;
        let mut change: Option<(usize, f64, f64)> = None;
        egui::Grid::new("pole-outline")
            .striped(true)
            .show(ui, |ui| {
                for h in ["", "z", "diameter", "circumference", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for (k, &(z, d)) in rings.iter().enumerate() {
                    ui.label(format!("{}", k + 1));
                    let (mut z2, mut mm) = (z, d * 1000.0);
                    let a = ui.add(
                        egui::DragValue::new(&mut z2)
                            .speed(0.05)
                            .max_decimals(3)
                            .suffix(" m"),
                    );
                    hold(f, &a, "pole row");
                    let b = ui.add(
                        egui::DragValue::new(&mut mm)
                            .speed(1.0)
                            .range(1.0..=10_000.0)
                            .max_decimals(1)
                            .suffix(" mm"),
                    );
                    hold(f, &b, "pole row");
                    if a.changed() || b.changed() {
                        change = Some((k, z2, mm / 1000.0));
                    }
                    let around = match p.polygon_sides() {
                        Some(n) => f64::from(n) * d * (std::f64::consts::PI / f64::from(n)).tan(),
                        None => std::f64::consts::PI * d,
                    };
                    ui.label(format!("{:.0} mm", around * 1000.0));
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Delete this height")
                        .clicked()
                    {
                        drop = Some(k);
                    }
                    ui.end_row();
                }
            });
        if let Some((k, z, d)) = change
            && let Err(e) = f.edit("pole row", |doc, t| {
                pole_edit::set_outline_row(doc, t, k, z, d)
            })
        {
            self.message = Some((e, true));
        }
        if let Some(k) = drop {
            f.edit("delete pole height", |doc, _| {
                pole_edit::remove_outline_row(doc, k)
            });
        }
        ui.horizontal(|ui| {
            if ui
                .button("+ Height")
                .on_hover_text("A height half-way up the top step (or the top, with one height)")
                .clicked()
                && let Err(e) = f.edit("add pole height", |doc, t| {
                    pole_edit::add_outline_row(doc, t).map(drop_usize)
                })
            {
                self.message = Some((e, true));
            }
            if ui
                .button("Remove the pole")
                .on_hover_text("Its outline and design (Undo brings them back)")
                .clicked()
            {
                f.edit("remove the pole", |doc, _| pole_edit::remove_pole(doc));
            }
        });
        if !p.is_3d() {
            ui.label(egui::RichText::new("Diameters at two heights at least make level 2.").weak());
        }
    }

    /// Design › Pole: material, wall or a section of the section library,
    /// strength and the other fields of the pole check; what level 3
    /// still lacks.
    pub(crate) fn pole_design_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        let Some(p) = t.pole.as_ref().and_then(PoleRef::inline) else {
            ui.label(match &t.pole {
                Some(PoleRef::File(name)) => format!(
                    "The pole is the file \"{name}\": its design is there (Body › Pole brings \
                     it into this file)."
                ),
                _ => "No pole outline yet (Body › Pole).".into(),
            });
            return;
        };
        let dir = f
            .path()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let doc_pole = |doc: &toml_edit::DocumentMut, k: &str| -> String {
            doc.get("pole")
                .and_then(|p| p.get(k))
                .map(|v| match v.as_str() {
                    Some(s) => s.to_owned(),
                    None => v.to_string().trim().to_owned(),
                })
                .unwrap_or_default()
        };
        egui::Grid::new("pole-design").striped(true).show(ui, |ui| {
            // Material: one of four.
            ui.strong("material");
            let now = p.material.clone().unwrap_or_default();
            let mut chosen = now.clone();
            egui::ComboBox::from_id_salt("pole-material")
                .selected_text(if chosen.is_empty() { "—" } else { &chosen })
                .width(110.0)
                .show_ui(ui, |ui| {
                    for m in POLE_MATERIALS {
                        ui.selectable_value(&mut chosen, m.to_owned(), m);
                    }
                });
            if chosen != now {
                let _ = f.edit("pole material", |doc, _| {
                    pole_edit::set_field(doc, "material", &chosen)
                });
            }
            ui.end_row();
            // A hollow section of the section library, when one fits.
            ui.strong("section").on_hover_text(
                "A hollow section of the section library next to the file, of the pole's shape \
                 (polygonal with its sides, or circular): its wall, and its material's \
                 elasticity and density when not typed",
            );
            let fits: Vec<(String, String)> = match self.pole.library(&dir) {
                Ok(lib) => lib
                    .sections
                    .iter()
                    .filter_map(|s| match (&s.shape, p.polygon_sides()) {
                        (Shape::PolygonalHollow { sides, d, t }, Some(n)) if *sides == n => {
                            Some((s.id.clone(), format!("{} × {:.1} mm", mm(*d), t * 1000.0)))
                        }
                        (Shape::CircularHollow { d, t }, None) => {
                            Some((s.id.clone(), format!("{} × {:.1} mm", mm(*d), t * 1000.0)))
                        }
                        _ => None,
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
            let now = p.section.clone().unwrap_or_default();
            let mut chosen = now.clone();
            ui.add_enabled_ui(!fits.is_empty() || !now.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("pole-section")
                    .selected_text(if chosen.is_empty() {
                        "— (typed)"
                    } else {
                        &chosen
                    })
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, String::new(), "— (wall typed)");
                        for (id, what) in &fits {
                            ui.selectable_value(&mut chosen, id.clone(), format!("{id}  {what}"));
                        }
                    })
                    .response
                    .on_disabled_hover_text(
                        "No hollow section of the pole's shape in a section library (.olssec) \
                         next to the file: type the wall",
                    );
            });
            if chosen != now {
                let _ = f.edit("pole section", |doc, _| {
                    pole_edit::set_field(doc, "section", &chosen)
                });
            }
            ui.end_row();
            // Solid: no hollow (wall = "solid"), level 3's wall.
            ui.strong("solid").on_hover_text(
                "A solid pole (wall = \"solid\"): no hollow. Ticked, it stands for the wall \
                 thickness; unticked, the wall is not given until typed or a section is chosen",
            );
            let now = pole_edit::is_solid(f.doc());
            let mut solid = now;
            ui.checkbox(&mut solid, "no hollow");
            if solid != now {
                let what = if solid {
                    "pole solid"
                } else {
                    "pole not solid"
                };
                if let Err(e) = f.edit(what, |doc, _| pole_edit::set_solid(doc, solid)) {
                    self.message = Some((e, true));
                }
            }
            ui.end_row();
            for (field, tip) in POLE_FIELDS {
                if matches!(field, "material" | "section" | "embedment") {
                    continue;
                }
                ui.strong(field.replace('_', " ")).on_hover_text(tip);
                let key = format!("pole:{field}");
                let current = doc_pole(f.doc(), field);
                let buf = self.buffers.entry(key.clone()).or_insert(current.clone());
                let r = ui.add(
                    egui::TextEdit::singleline(buf)
                        .desired_width(150.0)
                        .hint_text(tip),
                );
                if r.lost_focus() && *buf != current {
                    let typed = buf.clone();
                    match f.edit(&format!("pole {field}"), |doc, _| {
                        pole_edit::set_field(doc, field, &typed)
                    }) {
                        Ok(()) => {
                            self.buffers.remove(&key);
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                } else if !r.has_focus() && *buf != current {
                    *buf = current;
                }
                ui.end_row();
            }
        });
        if !p.capacity.is_empty() {
            ui.label(format!(
                "{} capacity row(s) by height ([[pole.capacity]], edited in the file)",
                p.capacity.len()
            ));
        }
        let lacks = p.missing_for_design();
        if lacks.is_empty() {
            ui.colored_label(
                egui::Color32::LIGHT_GREEN,
                "The pole check can read the pole (level 3).",
            );
        } else {
            for l in lacks {
                error_label(ui, l);
            }
        }
        if let Err(e) = self.pole.library(&dir) {
            error_label(ui, format!("section library not read: {e}"));
        }
    }
}

/// A width in millimetres, as shown.
fn mm(v: f64) -> String {
    format!("{:.0}", v * 1000.0)
}

/// An edit's row dropped (the button only adds).
fn drop_usize(_: usize) {}
