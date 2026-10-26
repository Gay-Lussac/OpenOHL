// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The section window: cable, circuit, system, bundle, stringing, points;
//! the sections of its circuit changed with it.

use super::*;

/// A section of a circuit that a change of another section's circuit
/// name, phase or system also changes, and what changes on it.
pub type CircuitMate = (String, Vec<&'static str>);

impl SectionForm {
    /// The other sections of its circuit (its conductor path) that its
    /// circuit name, phase or system change too, when it changes them: an
    /// existing section (`original`: its id in the line), the fields it
    /// changes, on the sections that differ. A circuit keeps one name, one
    /// phase and one system.
    pub fn circuit_mates(&self, stored: &Line, original: &str) -> Vec<CircuitMate> {
        if self.new {
            return Vec::new();
        }
        let Some(old) = stored.sections.iter().find(|s| s.id == original) else {
            return Vec::new();
        };
        let circuit = old.circuit_name != self.circuit;
        let phase = old.phase_label != self.phase_label;
        let system = old.system != self.system;
        if !(circuit || phase || system) {
            return Vec::new();
        }
        let path = ohl_line::electrical::conductor_paths(stored)
            .into_iter()
            .find(|p| p.iter().any(|id| id == original))
            .unwrap_or_default();
        stored
            .sections
            .iter()
            .filter(|m| m.id != original && path.contains(&m.id))
            .filter_map(|m| {
                let mut what = Vec::new();
                if circuit && m.circuit_name != self.circuit {
                    what.push("circuit");
                }
                if phase && m.phase_label != self.phase_label {
                    what.push("phase");
                }
                if system && m.system != self.system {
                    what.push("system");
                }
                (!what.is_empty()).then(|| (m.id.clone(), what))
            })
            .collect()
    }

    /// The puts giving the sections of [`Self::circuit_mates`] its circuit
    /// name, phase and system (those it changes), but those in `skip`
    /// (changed by hand alongside).
    pub fn circuit_puts(
        &self,
        stored: &Line,
        original: &str,
        skip: &[String],
    ) -> Result<Vec<(String, Command)>, String> {
        let mut out = Vec::new();
        for (id, what) in self.circuit_mates(stored, original) {
            if skip.contains(&id) {
                continue;
            }
            let Some(mut m) = stored.sections.iter().find(|s| s.id == id).cloned() else {
                continue;
            };
            if what.contains(&"circuit") {
                m.circuit_name = self.circuit.clone();
            }
            if what.contains(&"phase") {
                m.phase_label = self.phase_label.clone();
            }
            if what.contains(&"system") {
                m.system = self.system.clone();
            }
            out.push((
                format!("section {id}: {} as its conductor path", what.join(", ")),
                Command::Put {
                    kind: Kind::Section,
                    id,
                    value: serde_json::to_value(&m).map_err(|e| e.to_string())?,
                    ord: None,
                },
            ));
        }
        Ok(out)
    }

    /// Section `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.sections.iter().find(|s| s.id == id)?;
        Some(Self {
            new: false,
            id: id.to_owned(),
            cable: s.cable.clone(),
            circuit: s.circuit_name.clone(),
            phase_label: s.phase_label.clone(),
            system: s.system.clone(),
            bundle: s.bundle,
            spacing: s.bundle_spacing.unwrap_or(0.4),
            rotation: s.bundle_rotation_deg.unwrap_or(0.0),
            points: s.points.clone(),
            stringing: s.stringing.clone(),
            note: None,
            calculation: s.calculation,
            weather_groups: s.weather_groups.clone(),
            wind_factors: s.wind_factors.clone(),
            color: s.color.clone(),
            description: s.description.clone(),
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
        // The structure point a section point's wire point hangs from (by
        // its strings): the two sides of a tension structure share it.
        let top = |p: &SectionPoint| -> Option<String> {
            let st = line.structure(&p.structure)?;
            let w = st.wire_at_set(p.set)?;
            st.strings_to(&w.label).next().map(|x| x.top.clone())
        };
        let continued = line.sections.iter().find(|s| {
            s.points.iter().any(|p| {
                points.first().is_some_and(|q| {
                    p.structure == q.structure
                        && (p.set == q.set || top(p).is_some_and(|t| top(q) == Some(t)))
                })
            })
        });
        // Like `template` (the section selected), else the one it
        // continues (from the same wire point, or one on the same
        // structure point: the other side of a tension structure), else
        // the first; not continuing one, the circuit's next phase there
        // (2026-10-25: the phase was copied, to be changed each time).
        let like = template
            .and_then(|t| line.sections.iter().find(|s| s.id == t))
            .or(continued)
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
                stringing: Stringing::Tension {
                    horizontal_tension: 20_000.0,
                    temperature: ohl_line::DEFAULT_STRINGING_TEMPERATURE,
                },
                note: None,
                calculation: Default::default(),
                weather_groups: Vec::new(),
                wind_factors: Vec::new(),
                color: None,
                description: String::new(),
            },
        };
        // In the blocks (rolling) by default: a clipped template's
        // condition, or its tension as solved in the shown case — its
        // lengths belong to its own spans.
        if f.stringing.clipped() {
            f.stringing = f.unclipped(model, case);
        }
        match (continued, points.first()) {
            // Its conductor path's circuit and phase.
            (Some(c), _) => {
                f.circuit = c.circuit_name.clone();
                f.phase_label = c.phase_label.clone();
            }
            (None, Some(first)) => {
                f.phase_label = next_phase(line, &f.circuit, &first.structure, &f.phase_label);
            }
            (None, None) => {}
        }
        f.new = true;
        f.id = id;
        f.points = points;
        f.note = None;
        f
    }

    /// The condition to go back to the blocks with: the one it was sagged
    /// to before clipping in, else its tension as solved in case `case`
    /// (first span), at the clipping temperature.
    pub fn unclipped(&self, model: &Model, case: usize) -> Stringing {
        match &self.stringing {
            Stringing::UnstressedLengths {
                sagged: Some(s), ..
            } => (**s).clone(),
            Stringing::UnstressedLengths { temperature, .. } => {
                let h = model
                    .cases
                    .get(case)
                    .and_then(|c| c.shapes.iter().find(|x| x.section == self.id))
                    .and_then(|x| x.spans.first())
                    .map_or(20_000.0, |sp| sp.horizontal_tension);
                Stringing::Tension {
                    horizontal_tension: (h / 10.0).round() * 10.0,
                    temperature: *temperature,
                }
            }
            other => other.clone(),
        }
    }

    /// Clips in: each span's unstressed length at the condition, on the
    /// current geometry.
    pub fn clip_in(&mut self, model: &Model) -> Result<(), String> {
        if self.stringing.clipped() {
            return Ok(());
        }
        let cable = model
            .library
            .get(&self.cable)
            .ok_or_else(|| format!("wire type {} is not in the wire library", self.cable))?;
        let section = self.section(None)?;
        let (lengths, temperature) = ohl_line::solve::clip_lengths(&model.line, &section, cable)
            .map_err(|e| e.to_string())?;
        self.stringing = Stringing::UnstressedLengths {
            lengths,
            temperature,
            sagged: Some(Box::new(self.stringing.clone())),
        };
        Ok(())
    }

    /// Sets new points; a clipped section with another number of spans
    /// goes back to the blocks.
    pub fn set_points(&mut self, model: &Model, case: usize, points: Vec<SectionPoint>) {
        let spans = points.len().saturating_sub(1);
        if let Stringing::UnstressedLengths { lengths, .. } = &self.stringing
            && lengths.len() != spans
        {
            let n = lengths.len();
            self.stringing = self.unclipped(model, case);
            self.note = Some(format!(
                "{spans} spans now instead of {n}: back in the blocks, to be clipped in again"
            ));
        }
        if let Stringing::Sag { span: Some(k), .. } = &mut self.stringing
            && *k >= spans
        {
            *k = spans.saturating_sub(1);
        }
        self.points = points;
    }

    /// The section as typed (span points, spreads and plugin values kept
    /// from `old`, the span points on their spans if the points changed).
    pub fn section(&self, old: Option<&Section>) -> Result<Section, String> {
        let mut s = Section {
            description: self.description.trim().to_owned(),
            id: self.id.trim().to_owned(),
            cable: self.cable.clone(),
            circuit_name: self.circuit.clone(),
            phase_label: self.phase_label.clone(),
            system: self.system.clone(),
            bundle: self.bundle.max(1),
            bundle_spacing: (self.bundle > 1).then_some(self.spacing),
            bundle_rotation_deg: (self.bundle > 1 && self.rotation != 0.0).then_some(self.rotation),
            points: old.map_or_else(|| self.points.clone(), |o| o.points.clone()),
            stringing: self.stringing.clone(),
            span_points: old.map_or_else(Vec::new, |o| o.span_points.clone()),
            spreads: old.map_or_else(Vec::new, |o| o.spreads.clone()),
            color: self.color.clone(),
            calculation: self.calculation,
            weather_groups: self.weather_groups.clone(),
            wind_factors: self.wind_factors.clone(),
            fields: old.map_or_else(Default::default, |o| o.fields.clone()),
        };
        if old.is_some_and(|o| o.points != self.points) {
            s.set_points(self.points.clone())?;
        }
        Ok(s)
    }

    pub(crate) fn command(&self, stored: Option<&Line>) -> Result<(String, Command), String> {
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
        match &self.stringing {
            Stringing::Tension {
                horizontal_tension, ..
            } if *horizontal_tension <= 0.0 => {
                return Err("the tension must be above 0".into());
            }
            Stringing::Sag { sag, .. } if *sag <= 0.0 => {
                return Err("the sag must be above 0".into());
            }
            _ => {}
        }
        let s = self.section(old)?;
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

/// Returns true when "pick points again" is clicked.
/// A sagging condition in words.
pub fn describe(s: &Stringing) -> String {
    match s {
        Stringing::Tension {
            horizontal_tension,
            temperature,
        } => format!("{:.1} kN at {temperature} °C", horizontal_tension / 1000.0),
        Stringing::Sag {
            sag,
            temperature,
            span,
        } => format!(
            "a sag of {sag:.2} m in {} at {temperature} °C",
            span.map_or("the ruling span".into(), |k| format!("span {}", k + 1))
        ),
        Stringing::UnstressedLengths { lengths, .. } => {
            format!("{} unstressed lengths", lengths.len())
        }
    }
}

/// Where a cable is chosen and the wire library is empty: says so, with a
/// button to its page (where cable files are imported or added).
pub fn empty_wire_library(ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "The wire library is empty: no wire type to choose.",
        );
        if ui
            .button(OPEN_WIRE_LIBRARY)
            .on_hover_text(
                "Line › Sections › Wires library: Browse… for a wire library file (.ohlw), e.g. \
                 libraries/cables/acsr-examples.ohlw (typical example values), or add one",
            )
            .clicked()
        {
            crate::menu::request_page(
                ui.ctx(),
                crate::app::Tab::Library(crate::library::LibraryKind::Cables),
            );
        }
    });
}

/// The button of [`empty_wire_library`].
pub const OPEN_WIRE_LIBRARY: &str = "Open the wires library";

pub(super) fn section_ui(
    ui: &mut egui::Ui,
    f: &mut SectionForm,
    model: &Model,
    case: usize,
) -> bool {
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
            ui.label("description");
            ui.add(
                egui::TextEdit::singleline(&mut f.description)
                    .desired_width(240.0)
                    .hint_text("what it is (free text)"),
            );
            ui.end_row();
            ui.label("wire type");
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
            if model.library.is_empty() {
                ui.label("");
                empty_wire_library(ui);
                ui.end_row();
            }
            ui.label("circuit");
            ui.add(egui::TextEdit::singleline(&mut f.circuit).desired_width(200.0));
            ui.end_row();
            ui.label("phase");
            ui.add(egui::TextEdit::singleline(&mut f.phase_label).desired_width(60.0));
            ui.end_row();
            ui.label("calculation");
            egui::ComboBox::from_id_salt("section-calculation")
                .selected_text(f.calculation.label())
                .show_ui(ui, |ui| {
                    for c in ohl_line::condition::Calculation::ALL {
                        ui.selectable_value(&mut f.calculation, c, c.label());
                    }
                })
                .response
                .on_hover_text(
                    "Wire condition (Initial, after Creep, after Load, Max Sag of the two) and \
                     method (RS: ruling span; FE: finite elements)",
                );
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
                crate::style::field(
                    ui,
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
    // Status: in the blocks (rolling) or clipped in.
    let mut clipped = f.stringing.clipped();
    ui.horizontal(|ui| {
        ui.label("status");
        ui.radio_value(&mut clipped, false, "in the blocks")
            .on_hover_text("Unclipped: the wire rolls over the suspension clamps in every weather case — over frictionless sheaves (its tension the same on both sides of each) or with one horizontal tension in all spans, as Line › Criteria › Calculation options say; moving a structure sags it again to its condition");
        ui.radio_value(&mut clipped, true, "clipped in")
            .on_hover_text("Each span keeps its unstressed length: a structure moved changes the spans next to it, the suspension strings swing to balance them");
    });
    if clipped != f.stringing.clipped() {
        if clipped {
            if let Err(e) = f.clip_in(model) {
                f.note = Some(format!("cannot clip in: {e}"));
            } else {
                f.note = None;
            }
        } else {
            f.stringing = f.unclipped(model, case);
            f.note = None;
        }
    }
    let spans = f.points.len().saturating_sub(1);
    match &mut f.stringing {
        Stringing::Tension { .. } | Stringing::Sag { .. } => {
            // The sagging condition: a tension, or a sag.
            let temperature = f.stringing.temperature();
            let mut by_sag = matches!(f.stringing, Stringing::Sag { .. });
            ui.horizontal(|ui| {
                ui.label("sagged to");
                ui.radio_value(&mut by_sag, false, "a tension");
                ui.radio_value(&mut by_sag, true, "a sag");
            });
            if by_sag != matches!(f.stringing, Stringing::Sag { .. }) {
                f.stringing = if by_sag {
                    Stringing::Sag {
                        sag: 10.0,
                        temperature,
                        span: None,
                    }
                } else {
                    Stringing::Tension {
                        horizontal_tension: 20_000.0,
                        temperature,
                    }
                };
            }
            match &mut f.stringing {
                Stringing::Tension {
                    horizontal_tension,
                    temperature,
                } => {
                    let mut kn = *horizontal_tension / 1000.0;
                    ui.horizontal(|ui| {
                        ui.label("horizontal tension");
                        if value(ui, &mut kn, 0.1, " kN").changed() {
                            *horizontal_tension = kn * 1000.0;
                        }
                        ui.label("at");
                        value(ui, temperature, 0.5, " °C");
                    });
                }
                Stringing::Sag {
                    sag,
                    temperature,
                    span,
                } => {
                    ui.horizontal(|ui| {
                        ui.label("sag");
                        value(ui, sag, 0.01, " m");
                        ui.label("at");
                        value(ui, temperature, 0.5, " °C");
                    });
                    ui.horizontal(|ui| {
                        ui.label("in");
                        egui::ComboBox::from_id_salt("sag-span")
                            .selected_text(
                                span.map_or("the ruling span".into(), |k| {
                                    format!("span {}", k + 1)
                                }),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(span, None, "the ruling span");
                                for k in 0..spans {
                                    ui.selectable_value(span, Some(k), format!("span {}", k + 1));
                                }
                            });
                    });
                }
                Stringing::UnstressedLengths { .. } => {}
            }
            ui.label(
                egui::RichText::new(
                    "Still air, strings plumb; the same horizontal tension in every span.",
                )
                .small(),
            );
        }
        Stringing::UnstressedLengths {
            lengths,
            temperature,
            sagged,
        } => {
            ui.horizontal(|ui| {
                ui.label("unstressed lengths at");
                value(ui, temperature, 0.5, " °C");
            });
            egui::Grid::new("section-lengths").show(ui, |ui| {
                for (k, l) in lengths.iter_mut().enumerate() {
                    ui.label(format!("span {}", k + 1));
                    value(ui, l, 0.001, " m");
                    ui.end_row();
                }
            });
            if let Some(c) = sagged {
                ui.label(
                    egui::RichText::new(format!("clipped in after sagging to {}", describe(c)))
                        .small(),
                );
            }
        }
    }
    if let Some(n) = &f.note {
        ui.colored_label(ui.visuals().warn_fg_color, n);
    }
    if !f.new {
        ui.separator();
        crate::sagging_panel::ui(ui, model, &f.id, f.stringing.temperature());
    }
    ui.separator();
    ui.label(format!("{} points:", f.points.len()));
    let text: Vec<String> = f
        .points
        .iter()
        .map(|p| format!("{} set {}", p.structure, p.set))
        .collect();
    ui.label(egui::RichText::new(text.join(" ➡ ")).small());
    ui.button("Pick points again")
        .on_hover_text("With the String wire tool: click the strings in order, Enter")
        .clicked()
}

/// The phase a new section of circuit `circuit` starting at `structure`
/// is given: the first of the circuit's phase labels (in the order its
/// sections give them) not yet strung at that structure; all strung, the
/// label after the last one used there when it ends in a number (`L2` →
/// `L3`); else `like` (the template's).
fn next_phase(line: &ohl_line::Line, circuit: &str, structure: &str, like: &str) -> String {
    if circuit.is_empty() {
        return like.to_owned();
    }
    let of_circuit = || line.sections.iter().filter(|s| s.circuit_name == circuit);
    let mut labels: Vec<&str> = Vec::new();
    for s in of_circuit() {
        if !s.phase_label.is_empty() && !labels.contains(&s.phase_label.as_str()) {
            labels.push(&s.phase_label);
        }
    }
    let here: Vec<&str> = of_circuit()
        .filter(|s| s.points.iter().any(|p| p.structure == structure))
        .map(|s| s.phase_label.as_str())
        .collect();
    if let Some(free) = labels.iter().find(|l| !here.contains(l)) {
        return (*free).to_owned();
    }
    // Every known label strung there: the next number after the last.
    let last = here.last().copied().unwrap_or(like);
    let digits = last.len() - last.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 {
        let (stem, n) = last.split_at(last.len() - digits);
        if let Ok(n) = n.parse::<u32>() {
            return format!("{stem}{}", n + 1);
        }
    }
    like.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new section not continuing one: the circuit's first phase label
    /// not strung at its first structure; every label strung there, the
    /// next number; no circuit, the template's.
    #[test]
    fn a_new_section_takes_the_next_phase() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut line =
            ohl_line::Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json"))
                .unwrap();
        let first = line.sections[0].clone();
        let at = first.points[0].structure.clone();
        let circuit = first.circuit_name.clone();
        let here: Vec<String> = line
            .sections
            .iter()
            .filter(|s| s.circuit_name == circuit && s.points.iter().any(|p| p.structure == at))
            .map(|s| s.phase_label.clone())
            .collect();
        // Only one phase there: another of the circuit's labels comes.
        line.sections.retain(|s| {
            !(s.circuit_name == circuit && s.points.iter().any(|p| p.structure == at))
                || s.id == first.id
        });
        let next = next_phase(&line, &circuit, &at, &first.phase_label);
        assert_ne!(next, first.phase_label, "{here:?}");
        // Every label of the circuit there: the number after the last.
        let mut s2 = first.clone();
        s2.id = "X2".into();
        s2.phase_label = "L2".into();
        let mut s3 = first.clone();
        s3.id = "X3".into();
        s3.phase_label = "L3".into();
        line.sections.retain(|s| s.circuit_name != circuit);
        line.sections.extend([first.clone(), s2, s3]);
        assert_eq!(next_phase(&line, &circuit, &at, "L1"), "L4");
        assert_eq!(next_phase(&line, "", &at, "L1"), "L1");
    }
}
