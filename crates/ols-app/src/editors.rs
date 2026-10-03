// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The editor windows of the tools: a structure (new or existing: id,
//! type, extension, orientation, position), a string (insulator and wire
//! points in the structure's own axes, weight, insulator, strain), a
//! section (cable, circuit, system, bundle, stringing, points). Applying
//! one makes one session command ([`Form::command`]).

use eframe::egui;
use ols_line::{Attachment, Line, Section, SectionPoint, Stringing, Structure};
use ols_session::{Command, Kind};
use ols_structures::structures::{to_global, to_local};

use crate::model::Model;

/// `like` with its trailing number increased until no id in `taken` has
/// it (`T7` → `T8`; `S` → `S-1`).
pub fn next_id<'a>(taken: impl Iterator<Item = &'a str> + Clone, like: &str) -> String {
    let digits = like.len() - like.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (stem, num) = like.split_at(like.len() - digits);
    let width = num.len();
    let (stem, mut n) = match num.parse::<u64>() {
        Ok(n) => (stem.to_owned(), n + 1),
        Err(_) => (format!("{like}-"), 1),
    };
    loop {
        let id = format!("{stem}{n:0width$}");
        if !taken.clone().any(|t| t == id) {
            return id;
        }
        n += 1;
    }
}

/// A structure being added or changed.
#[derive(Debug, Clone, PartialEq)]
pub struct StructureForm {
    /// Not in the line yet.
    pub new: bool,
    /// Id (editable when new).
    pub id: String,
    /// Structure type.
    pub structure_type: Option<String>,
    /// Height extension of the type.
    pub extension: Option<String>,
    /// Orientation of the local x axis, degrees from east.
    pub orientation_deg: f64,
    /// Position of the base.
    pub position: [f64; 3],
    /// The base follows the ground.
    pub on_ground: bool,
}

/// A string being added or changed, its points in the structure's axes
/// (across: to the right looking ahead, along: ahead, up from the base).
#[derive(Debug, Clone, PartialEq)]
pub struct StringForm {
    /// Not on the structure yet.
    pub new: bool,
    /// Structure.
    pub structure: String,
    /// Set.
    pub set: u32,
    /// Phase.
    pub phase: u32,
    /// Label.
    pub label: String,
    /// Insulator (structure) point, local.
    pub top: [f64; 3],
    /// Wire (clamp) point, local.
    pub bottom: [f64; 3],
    /// V-string legs, local.
    pub legs: Option<[[f64; 3]; 2]>,
    /// Insulator weight, N.
    pub weight: f64,
    /// Insulator assembly from the library.
    pub insulator: Option<String>,
    /// Solved as a strain string.
    pub strain: bool,
    /// Labels of the top and the bottom (kept).
    pub top_label: String,
    /// Bottom label.
    pub bottom_label: String,
    /// The structure is typed and this string overrides its type's.
    pub overrides_type: bool,
}

/// How a section is strung, as typed.
#[derive(Debug, Clone, PartialEq)]
pub enum StringingForm {
    /// Horizontal tension, kN, at a temperature, °C.
    Tension {
        /// kN.
        h_kn: f64,
        /// °C.
        temperature: f64,
    },
    /// Unstressed length of each span, m, at a temperature.
    Lengths {
        /// m.
        lengths: Vec<f64>,
        /// °C.
        temperature: f64,
    },
}

/// A section being added or changed.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionForm {
    /// Not in the line yet.
    pub new: bool,
    /// Id (editable when new).
    pub id: String,
    /// Cable id.
    pub cable: String,
    /// Circuit.
    pub circuit: String,
    /// Phase label.
    pub phase_label: String,
    /// Electrical system.
    pub system: Option<String>,
    /// Subconductors.
    pub bundle: u32,
    /// Subconductor spacing, m.
    pub spacing: f64,
    /// Bundle rotation, degrees.
    pub rotation: f64,
    /// Attachments in line order.
    pub points: Vec<SectionPoint>,
    /// Stringing.
    pub stringing: StringingForm,
    /// Why the stringing was changed (points changed with lengths given).
    pub note: Option<String>,
}

/// An editor window's content.
#[derive(Debug, Clone, PartialEq)]
pub enum Form {
    /// A structure.
    Structure(StructureForm),
    /// A string.
    String(StringForm),
    /// A section.
    Section(SectionForm),
}

/// What the window asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Apply the form.
    Apply,
    /// Close without a change.
    Cancel,
    /// A typed structure's string: back to the type's.
    ResetString,
    /// Pick the section's points again with the String wire tool.
    PickPoints,
}

impl StructureForm {
    /// A new structure at `at`: the type and orientation of the nearest
    /// structure, the id after the last one's.
    pub fn new_at(model: &Model, at: [f64; 3]) -> Self {
        let line = &model.line;
        let nearest = line.structures.iter().min_by(|a, b| {
            let d = |s: &&Structure| (s.position[0] - at[0]).hypot(s.position[1] - at[1]);
            d(a).total_cmp(&d(b))
        });
        let like = line.structures.last().map_or("T0", |s| s.id.as_str());
        let id = next_id(line.structures.iter().map(|s| s.id.as_str()), like);
        let structure_type = nearest
            .and_then(|s| s.structure_type.clone())
            .or_else(|| model.types.as_ref()?.0.iter().next().map(|t| t.id.clone()));
        Self {
            new: true,
            id,
            structure_type,
            extension: None,
            orientation_deg: nearest.map_or(0.0, |s| s.orientation_deg),
            position: at,
            on_ground: model.ground_at(at[0], at[1]).is_some(),
        }
    }

    /// Structure `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        Some(Self {
            new: false,
            id: id.to_owned(),
            structure_type: s.structure_type.clone(),
            extension: s.extension.clone(),
            orientation_deg: s.orientation_deg,
            position: s.position,
            on_ground: false,
        })
    }

    fn command(&self, stored: Option<&Line>) -> Result<(String, Command), String> {
        let id = self.id.trim().to_owned();
        if id.is_empty() {
            return Err("the structure needs an id".into());
        }
        let s = if self.new {
            if stored.is_some_and(|l| l.structure(&id).is_some()) {
                return Err(format!("structure {id} exists already"));
            }
            Structure {
                id: id.clone(),
                position: self.position,
                orientation_deg: self.orientation_deg,
                structure_type: self.structure_type.clone(),
                extension: self.extension.clone(),
                attachments: Vec::new(),
                omit: Vec::new(),
                members: Vec::new(),
            }
        } else {
            let old = stored
                .and_then(|l| l.structure(&id))
                .ok_or_else(|| format!("no structure {id}"))?;
            let mut s = old.moved_to(self.position);
            if s.orientation_deg != self.orientation_deg {
                // Explicit strings turn with it about its base.
                let turn = self.orientation_deg - s.orientation_deg;
                for a in &mut s.attachments {
                    let rot = |p: [f64; 3]| {
                        to_global(
                            s.position,
                            self.orientation_deg,
                            to_local(s.position, self.orientation_deg - turn, p),
                        )
                    };
                    a.insulator_point = rot(a.insulator_point);
                    a.wire_point = rot(a.wire_point);
                    if let Some(l) = &mut a.legs {
                        *l = l.map(rot);
                    }
                }
                s.orientation_deg = self.orientation_deg;
            }
            if s.structure_type != self.structure_type {
                // The type's stick drawing comes with the new type.
                s.members.clear();
                s.omit.clear();
            }
            s.structure_type = self.structure_type.clone();
            s.extension = self.extension.clone();
            s
        };
        let what = if self.new {
            format!("added structure {id}")
        } else {
            format!("changed structure {id}")
        };
        Ok((
            what,
            Command::Put {
                kind: Kind::Structure,
                id,
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

impl StringForm {
    /// A new string on structure `id`: the next free set, hanging 3 m from
    /// the top of the structure.
    pub fn new_on(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        let set = s.attachments.iter().map(|a| a.set).max().unwrap_or(0) + 1;
        let top = s
            .attachments
            .iter()
            .map(|a| a.insulator_point[2])
            .fold(s.position[2] + 20.0, f64::max)
            - s.position[2];
        Some(Self {
            new: true,
            structure: id.to_owned(),
            set,
            phase: 1,
            label: String::new(),
            top: [0.0, 0.0, top],
            bottom: [0.0, 0.0, top - 3.0],
            legs: None,
            weight: 0.0,
            insulator: None,
            strain: false,
            top_label: String::new(),
            bottom_label: String::new(),
            overrides_type: false,
        })
    }

    /// String `set`:`phase` of structure `id` (as resolved).
    pub fn of(model: &Model, id: &str, set: u32, phase: u32) -> Option<Self> {
        let s = model.line.structure(id)?;
        let a = s
            .attachments
            .iter()
            .find(|a| (a.set, a.phase) == (set, phase))?;
        let local = |p| to_local(s.position, s.orientation_deg, p);
        let overrides_type = s.structure_type.is_some()
            && model.stored_line().is_some_and(|l| {
                l.structure(id).is_some_and(|x| {
                    x.attachments
                        .iter()
                        .any(|a| (a.set, a.phase) == (set, phase))
                })
            });
        Some(Self {
            new: false,
            structure: id.to_owned(),
            set,
            phase,
            label: a.label.clone(),
            top: local(a.insulator_point),
            bottom: local(a.wire_point),
            legs: a.legs.map(|l| l.map(local)),
            weight: a.insulator_weight,
            insulator: a.insulator.clone(),
            strain: a.strain_member,
            top_label: a.top_label.clone(),
            bottom_label: a.bottom_label.clone(),
            overrides_type,
        })
    }

    /// String length, m.
    pub fn length(&self) -> f64 {
        let d = [0, 1, 2].map(|k| self.bottom[k] - self.top[k]);
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    fn command(&self, stored: Option<&Line>, reset: bool) -> Result<(String, Command), String> {
        let mut s = stored
            .and_then(|l| l.structure(&self.structure))
            .ok_or_else(|| format!("no structure {}", self.structure))?
            .clone();
        let key = (self.set, self.phase);
        let global = |p| to_global(s.position, s.orientation_deg, p);
        let a = Attachment {
            set: self.set,
            phase: self.phase,
            label: self.label.clone(),
            insulator_point: global(self.top),
            wire_point: global(self.bottom),
            insulator_weight: self.weight,
            insulator: self.insulator.clone(),
            legs: self.legs.map(|l| l.map(global)),
            top_label: self.top_label.clone(),
            bottom_label: self.bottom_label.clone(),
            strain_member: self.strain,
        };
        s.attachments.retain(|x| (x.set, x.phase) != key);
        s.omit.retain(|k| *k != [self.set, self.phase]);
        let what = if reset {
            format!(
                "string {}:{} of {} back to its type",
                self.set, self.phase, self.structure
            )
        } else {
            s.attachments.push(a);
            s.attachments.sort_by_key(|a| (a.set, a.phase));
            format!(
                "{} string {}:{} of {}",
                if self.new { "added" } else { "changed" },
                self.set,
                self.phase,
                self.structure
            )
        };
        Ok((
            what,
            Command::Put {
                kind: Kind::Structure,
                id: s.id.clone(),
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

impl SectionForm {
    /// Section `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.sections.iter().find(|s| s.id == id)?;
        Some(Self {
            new: false,
            id: id.to_owned(),
            cable: s.cable.clone(),
            circuit: s.circuit.clone(),
            phase_label: s.phase_label.clone(),
            system: s.system.clone(),
            bundle: s.bundle,
            spacing: s.bundle_spacing.unwrap_or(0.4),
            rotation: s.bundle_rotation_deg.unwrap_or(0.0),
            points: s.points.clone(),
            stringing: match &s.stringing {
                Stringing::Tension {
                    horizontal_tension,
                    temperature,
                } => StringingForm::Tension {
                    h_kn: horizontal_tension / 1000.0,
                    temperature: *temperature,
                },
                Stringing::UnstressedLengths {
                    lengths,
                    temperature,
                } => StringingForm::Lengths {
                    lengths: lengths.clone(),
                    temperature: *temperature,
                },
            },
            note: None,
        })
    }

    /// A new section through `points`, like `template` (a section's id:
    /// cable, circuit, phase, system, bundle; its tension in case `case`).
    pub fn new_through(
        model: &Model,
        case: usize,
        points: Vec<SectionPoint>,
        template: Option<&str>,
    ) -> Self {
        let line = &model.line;
        let like = template
            .and_then(|t| line.sections.iter().find(|s| s.id == t))
            .or_else(|| {
                line.sections.iter().find(|s| {
                    s.points.iter().any(|p| {
                        points
                            .first()
                            .is_some_and(|q| p.structure == q.structure && p.set == q.set)
                    })
                })
            })
            .or_else(|| line.sections.first());
        let id = next_id(
            line.sections.iter().map(|s| s.id.as_str()),
            line.sections.last().map_or("S0", |s| s.id.as_str()),
        );
        let mut f = match like.and_then(|s| Self::of(model, &s.id)) {
            Some(f) => f,
            None => Self {
                new: true,
                id: String::new(),
                cable: model
                    .library
                    .iter()
                    .next()
                    .map_or(String::new(), |c| c.id.clone()),
                circuit: String::new(),
                phase_label: String::new(),
                system: None,
                bundle: 1,
                spacing: 0.4,
                rotation: 0.0,
                points: Vec::new(),
                stringing: StringingForm::Tension {
                    h_kn: 20.0,
                    temperature: 15.0,
                },
                note: None,
            },
        };
        // The template's tension (as solved in the shown case) for the new
        // spans: lengths belong to its own spans.
        if let StringingForm::Lengths { temperature, .. } = f.stringing {
            let h = like
                .and_then(|s| {
                    model
                        .cases
                        .get(case)?
                        .shapes
                        .iter()
                        .find(|x| x.section == s.id)
                })
                .and_then(|x| x.spans.first())
                .map_or(20.0, |sp| sp.horizontal_tension / 1000.0);
            f.stringing = StringingForm::Tension {
                h_kn: (h * 10.0).round() / 10.0,
                temperature,
            };
        }
        f.new = true;
        f.id = id;
        f.points = points;
        f.note = None;
        f
    }

    /// Sets new points; unstressed lengths for other spans become the
    /// tension of the first span solved in case `case`.
    pub fn set_points(&mut self, model: &Model, case: usize, points: Vec<SectionPoint>) {
        let spans = points.len().saturating_sub(1);
        if let StringingForm::Lengths {
            lengths,
            temperature,
        } = &self.stringing
            && lengths.len() != spans
        {
            let h = model
                .cases
                .get(case)
                .and_then(|c| c.shapes.iter().find(|x| x.section == self.id))
                .and_then(|x| x.spans.first())
                .map_or(20.0, |sp| sp.horizontal_tension / 1000.0);
            self.note = Some(format!(
                "{} spans now: strung by tension ({h:.1} kN as solved) instead of {} unstressed lengths",
                spans,
                lengths.len()
            ));
            self.stringing = StringingForm::Tension {
                h_kn: (h * 10.0).round() / 10.0,
                temperature: *temperature,
            };
        }
        self.points = points;
    }

    fn command(&self, stored: Option<&Line>) -> Result<(String, Command), String> {
        let id = self.id.trim().to_owned();
        if id.is_empty() {
            return Err("the section needs an id".into());
        }
        if self.points.len() < 2 {
            return Err("a section needs two points or more".into());
        }
        let old = stored.and_then(|l| l.sections.iter().find(|s| s.id == id));
        if self.new && old.is_some() {
            return Err(format!("section {id} exists already"));
        }
        let stringing = match &self.stringing {
            StringingForm::Tension { h_kn, temperature } => {
                if *h_kn <= 0.0 {
                    return Err("the tension must be above 0".into());
                }
                Stringing::Tension {
                    horizontal_tension: h_kn * 1000.0,
                    temperature: *temperature,
                }
            }
            StringingForm::Lengths {
                lengths,
                temperature,
            } => Stringing::UnstressedLengths {
                lengths: lengths.clone(),
                temperature: *temperature,
            },
        };
        let s = Section {
            id: id.clone(),
            cable: self.cable.clone(),
            circuit: self.circuit.clone(),
            phase_label: self.phase_label.clone(),
            system: self.system.clone(),
            bundle: self.bundle.max(1),
            bundle_spacing: (self.bundle > 1).then_some(self.spacing),
            bundle_rotation_deg: (self.bundle > 1 && self.rotation != 0.0).then_some(self.rotation),
            points: self.points.clone(),
            stringing,
            loads: old.map_or_else(Vec::new, |o| o.loads.clone()),
            color: old.and_then(|o| o.color.clone()),
        };
        Ok((
            if self.new {
                format!("strung section {id}")
            } else {
                format!("changed section {id}")
            },
            Command::Put {
                kind: Kind::Section,
                id,
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

impl Form {
    /// Window title.
    pub fn title(&self) -> String {
        match self {
            Form::Structure(f) if f.new => "Add structure".into(),
            Form::Structure(f) => format!("Structure {}", f.id),
            Form::String(f) if f.new => format!("Add string to {}", f.structure),
            Form::String(f) => format!("String {}:{} of {}", f.set, f.phase, f.structure),
            Form::Section(f) if f.new => "String wire: new section".into(),
            Form::Section(f) => format!("Section {}", f.id),
        }
    }

    /// The command applying the form (or resetting a string to its type),
    /// with what it does in words.
    pub fn command(&self, model: &Model, reset: bool) -> Result<(String, Command), String> {
        let stored = model.stored_line();
        match self {
            Form::Structure(f) => f.command(stored),
            Form::String(f) => {
                if f.new
                    && model.line.structure(&f.structure).is_some_and(|s| {
                        s.attachments
                            .iter()
                            .any(|a| (a.set, a.phase) == (f.set, f.phase))
                    })
                {
                    return Err(format!(
                        "{} has a string {}:{} already",
                        f.structure, f.set, f.phase
                    ));
                }
                f.command(stored, reset)
            }
            Form::Section(f) => f.command(stored),
        }
    }

    /// The window's content; what was asked for.
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<Outcome> {
        let mut out = None;
        match self {
            Form::Structure(f) => structure_ui(ui, f, model),
            Form::String(f) => {
                if string_ui(ui, f, model) {
                    out = Some(Outcome::ResetString);
                }
            }
            Form::Section(f) => {
                if section_ui(ui, f, model) {
                    out = Some(Outcome::PickPoints);
                }
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Apply").on_hover_text("Enter").clicked() {
                out = Some(Outcome::Apply);
            }
            if ui.button("Cancel").on_hover_text("Escape").clicked() {
                out = Some(Outcome::Cancel);
            }
        });
        out
    }
}

fn value(ui: &mut egui::Ui, v: &mut f64, speed: f64, suffix: &str) -> egui::Response {
    ui.add(
        egui::DragValue::new(v)
            .speed(speed)
            .max_decimals(3)
            .suffix(suffix),
    )
}

fn xyz(ui: &mut egui::Ui, label: &str, p: &mut [f64; 3], names: [&str; 3]) {
    ui.label(label);
    ui.horizontal(|ui| {
        for k in 0..3 {
            ui.label(names[k]);
            value(ui, &mut p[k], 0.05, " m");
        }
    });
    ui.end_row();
}

fn structure_ui(ui: &mut egui::Ui, f: &mut StructureForm, model: &Model) {
    egui::Grid::new("structure-form").num_columns(2).show(ui, |ui| {
        ui.label("id");
        ui.add_enabled(f.new, egui::TextEdit::singleline(&mut f.id).desired_width(140.0));
        ui.end_row();
        ui.label("type");
        let types: Vec<&ols_structures::StructureType> = model
            .types
            .as_ref()
            .map(|(s, _)| s.iter().collect())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("structure-type")
            .selected_text(f.structure_type.as_deref().unwrap_or("(none: strings given)"))
            .height(400.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.structure_type, None, "(none: strings given)");
                for t in &types {
                    ui.selectable_value(&mut f.structure_type, Some(t.id.clone()), &t.id);
                }
            });
        ui.end_row();
        let extensions: Vec<String> = f
            .structure_type
            .as_deref()
            .and_then(|id| types.iter().find(|t| t.id == id))
            .map(|t| t.extensions.iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default();
        if !extensions.is_empty() {
            ui.label("extension");
            egui::ComboBox::from_id_salt("structure-extension")
                .selected_text(f.extension.as_deref().unwrap_or("(base)"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.extension, None, "(base)");
                    for e in &extensions {
                        ui.selectable_value(&mut f.extension, Some(e.clone()), e);
                    }
                });
            ui.end_row();
        }
        ui.label("orientation");
        value(ui, &mut f.orientation_deg, 0.1, "°").on_hover_text(
            "Direction of the cross-arms (local x, to the right looking ahead), counter-clockwise from east",
        );
        ui.end_row();
        for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
            ui.label(name);
            ui.add_enabled_ui(k < 2 || !f.on_ground, |ui| {
                value(ui, &mut f.position[k], 0.1, " m");
            });
            ui.end_row();
        }
    });
    ui.checkbox(&mut f.on_ground, "base on the ground");
    if f.on_ground
        && let Some(z) = model.ground_at(f.position[0], f.position[1])
    {
        f.position[2] = z;
    }
    if f.new && f.structure_type.is_none() {
        ui.label(
            egui::RichText::new("Without a type, add its strings with the Add string tool.")
                .small(),
        );
    }
}

/// Returns true when "back to the type" is clicked.
fn string_ui(ui: &mut egui::Ui, f: &mut StringForm, model: &Model) -> bool {
    let mut reset = false;
    let axes = ["across", "along", "up"];
    egui::Grid::new("string-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("set : phase");
            ui.add_enabled_ui(f.new, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut f.set).range(1..=999));
                    ui.label(":");
                    ui.add(egui::DragValue::new(&mut f.phase).range(1..=99));
                });
            });
            ui.end_row();
            ui.label("label");
            ui.add(egui::TextEdit::singleline(&mut f.label).desired_width(140.0));
            ui.end_row();
            xyz(ui, "insulator point", &mut f.top, axes);
            xyz(ui, "wire point", &mut f.bottom, axes);
            if let Some(legs) = &mut f.legs {
                xyz(ui, "V leg 1", &mut legs[0], axes);
                xyz(ui, "V leg 2", &mut legs[1], axes);
            }
            ui.label("length");
            let mut len = f.length();
            if value(ui, &mut len, 0.01, " m")
                .on_hover_text("Changes the wire point: straight below the insulator point")
                .changed()
            {
                f.bottom = [f.top[0], f.top[1], f.top[2] - len.max(0.0)];
            }
            ui.end_row();
            ui.label("weight");
            value(ui, &mut f.weight, 1.0, " N");
            ui.end_row();
            let insulators: Vec<String> = model
                .types
                .as_ref()
                .map(|(_, i)| i.iter().map(|a| a.id.clone()).collect())
                .unwrap_or_default();
            if !insulators.is_empty() || f.insulator.is_some() {
                ui.label("insulator");
                egui::ComboBox::from_id_salt("string-insulator")
                    .selected_text(f.insulator.as_deref().unwrap_or("(none)"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.insulator, None, "(none)");
                        for i in &insulators {
                            ui.selectable_value(&mut f.insulator, Some(i.clone()), i);
                        }
                    });
                ui.end_row();
            }
        });
    ui.checkbox(&mut f.strain, "strain string (free along its conductor)");
    ui.label(
        egui::RichText::new("In the structure's axes: across (to the right looking ahead), along (ahead), up from its base.")
            .small(),
    );
    if f.overrides_type {
        ui.label(egui::RichText::new("This string overrides the structure type's.").small());
        reset = ui.button("Back to the type's string").clicked();
    }
    reset
}

/// Returns true when "pick points again" is clicked.
fn section_ui(ui: &mut egui::Ui, f: &mut SectionForm, model: &Model) -> bool {
    let line = &model.line;
    egui::Grid::new("section-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("id");
            ui.add_enabled(
                f.new,
                egui::TextEdit::singleline(&mut f.id).desired_width(140.0),
            );
            ui.end_row();
            ui.label("cable");
            egui::ComboBox::from_id_salt("section-cable")
                .selected_text(&f.cable)
                .width(240.0)
                .height(400.0)
                .show_ui(ui, |ui| {
                    for c in model.library.iter() {
                        ui.selectable_value(&mut f.cable, c.id.clone(), &c.id)
                            .on_hover_text(&c.name);
                    }
                });
            ui.end_row();
            ui.label("circuit");
            ui.add(egui::TextEdit::singleline(&mut f.circuit).desired_width(200.0));
            ui.end_row();
            ui.label("phase");
            ui.add(egui::TextEdit::singleline(&mut f.phase_label).desired_width(60.0));
            ui.end_row();
            ui.label("system");
            egui::ComboBox::from_id_salt("section-system")
                .selected_text(
                    f.system
                        .as_deref()
                        .unwrap_or("(from the jumpers, or earthed)"),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.system, None, "(from the jumpers, or earthed)");
                    for s in &line.systems {
                        ui.selectable_value(&mut f.system, Some(s.id.clone()), &s.id);
                    }
                });
            ui.end_row();
            ui.label("bundle");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut f.bundle)
                        .range(1..=8)
                        .suffix(" wires"),
                );
                if f.bundle > 1 {
                    value(ui, &mut f.spacing, 0.01, " m apart");
                    value(ui, &mut f.rotation, 1.0, "°");
                }
            });
            ui.end_row();
        });
    ui.separator();
    let mut lengths = matches!(f.stringing, StringingForm::Lengths { .. });
    ui.horizontal(|ui| {
        ui.label("strung by");
        let spans = f.points.len().saturating_sub(1);
        let was = lengths;
        ui.radio_value(&mut lengths, false, "tension");
        ui.radio_value(&mut lengths, true, "unstressed lengths");
        if lengths != was {
            let temperature = match &f.stringing {
                StringingForm::Tension { temperature, .. }
                | StringingForm::Lengths { temperature, .. } => *temperature,
            };
            f.stringing = if lengths {
                // The solved lengths are not kept: start from the spans'
                // chords.
                let chords: Vec<f64> = f
                    .points
                    .windows(2)
                    .map(|w| {
                        let p = |q: &SectionPoint| {
                            line.attachment(q).map_or([0.0; 3], |a| a.wire_point)
                        };
                        let (a, b) = (p(&w[0]), p(&w[1]));
                        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2))
                            .sqrt()
                    })
                    .collect();
                debug_assert_eq!(chords.len(), spans);
                StringingForm::Lengths {
                    lengths: chords,
                    temperature,
                }
            } else {
                StringingForm::Tension {
                    h_kn: 20.0,
                    temperature,
                }
            };
        }
    });
    match &mut f.stringing {
        StringingForm::Tension { h_kn, temperature } => {
            ui.horizontal(|ui| {
                ui.label("horizontal tension");
                value(ui, h_kn, 0.1, " kN");
                ui.label("at");
                value(ui, temperature, 0.5, " °C");
            });
        }
        StringingForm::Lengths {
            lengths,
            temperature,
        } => {
            ui.horizontal(|ui| {
                ui.label("at");
                value(ui, temperature, 0.5, " °C");
            });
            egui::Grid::new("section-lengths").show(ui, |ui| {
                for (k, l) in lengths.iter_mut().enumerate() {
                    ui.label(format!("span {}", k + 1));
                    value(ui, l, 0.001, " m");
                    ui.end_row();
                }
            });
        }
    }
    if let Some(n) = &f.note {
        ui.colored_label(egui::Color32::YELLOW, n);
    }
    ui.separator();
    ui.label(format!("{} points:", f.points.len()));
    let text: Vec<String> = f
        .points
        .iter()
        .map(|p| format!("{} {}:{}", p.structure, p.set, p.phase))
        .collect();
    ui.label(egui::RichText::new(text.join(" → ")).small());
    ui.button("Pick points again")
        .on_hover_text("With the String wire tool: right-click the strings in order, Enter")
        .clicked()
}
