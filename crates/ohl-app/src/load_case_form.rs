// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **Load cases library** as a form (owner 2026-11-03: "design it so
//! that it's easy to understand and use for a human (load cases for
//! structure and load cases library) the library should be where the
//! load cases are saved then on each pylon you should have a load case
//! for structure that can be different per structure and composed of
//! cases from the library"): the file's cases listed on the left (Add,
//! Duplicate, Delete, Rename), the case picked on the right in plain
//! sections — 1 Weather, 2 Wind on the wires, 3 Wires removed and wires
//! broken, 4 Wind on the structures, 5 Ice and snow on the structures,
//! 6 Factors and local conditions, 7 Tags and description, 8 Applies to
//! (2026-11-15: the structures taking the case and how, ticked per
//! structure — the case's own list, [`ohl_line::Line::give_case`]; a
//! change is one command of the project, not part of the file).
//!
//! The form edits the cases ([`ohl_loads::LoadCases`]) and writes the
//! file with `ohl_loads`' own writer ([`ohl_loads::LoadCases::to_toml`],
//! at the lowest version its cases need); the page's **Save** keeps it
//! (in a project one undoable edit, as every library file). A file written
//! from the form loses its comments: **Fields (expert)** edits the text's
//! fields as before.

use std::collections::HashMap;

use eframe::egui;
use ohl_loads::{
    BrokenSpan, BrokenWire, DEFAULT_SNOW_EXPOSURE, DEFAULT_SNOW_SHAPE, DEFAULT_SNOW_THERMAL,
    LoadCase, LoadCases, RemovedWire, StructureIce, WindDirection, WireCondition,
};
use ohl_weather::{WeatherCase, WindSide};

/// The form's state: the cases as edited, and the text they mirror.
#[derive(Debug, Default)]
pub struct Form {
    /// The file's text the cases were read from (or written to).
    mirror: Option<String>,
    /// The cases (none: the file does not read as load cases).
    pub cases: Option<LoadCases>,
    /// Why the file does not read as load cases.
    pub unreadable: Option<String>,
    /// A case being renamed: the name typed.
    renaming: Option<String>,
    /// A rename asked of the project (old name, new name): one command
    /// renaming the case in the file and in the structures' sets
    /// ([`crate::load_case_sets::rename_command`]); taken by the page.
    pub rename: Option<(String, String)>,
    /// Numbers being typed, by field (a number half typed does not read).
    buffers: HashMap<String, String>,
    /// *8 Applies to*: its state.
    applies: AppliesTo,
    /// A change of which structures take a case, asked of the project
    /// (one command, [`crate::load_case_sets::give_command`]); taken by
    /// the page.
    pub give: Option<Give>,
    /// **Add the default cases…** asked: what it adds, shown to confirm.
    adding_defaults: bool,
    /// The default cases were added with one that goes only to some
    /// structures (the 60 N/m ice, at or above 1000 m): its own list to put
    /// in the line (one command, [`crate::default_cases::groups_command`]);
    /// taken by the page.
    pub default_groups: bool,
}

/// Structures made to take a load case (`take`) and not to (`drop`),
/// through the case's own list (owner, 2026-11-15: "manual should always
/// be available where load cases rules are defined … Not per type but
/// per position").
#[derive(Debug, Clone, PartialEq)]
pub struct Give {
    /// The case, by name.
    pub case: String,
    /// The structures to take it.
    pub take: Vec<String>,
    /// The structures not to take it.
    pub drop: Vec<String>,
}

/// The line resolved for *8 Applies to*: (line version, its groups, what
/// each structure takes, the structures' stations).
type Resolved = (
    u64,
    Vec<ohl_line::StructureGroup>,
    ohl_line::CaseSets,
    std::collections::BTreeMap<String, ohl_line::Facts>,
);

/// *8 Applies to*: the ticks being dragged, the line resolved.
#[derive(Debug, Default)]
struct AppliesTo {
    /// The ticks as dragged (case, the structures ticked): asked as one
    /// change when the button is let go.
    pending: Option<(String, Vec<String>)>,
    /// The line resolved (while its version and groups are as they were).
    cache: Option<Resolved>,
}

/// What the form needs from the project.
pub struct FormContext<'a> {
    /// The project's weather cases (the only ones a case may name).
    pub weather: &'a [WeatherCase],
    /// The line (its sections for the removed and broken wires; none: a
    /// file opened alone).
    pub line: Option<&'a ohl_line::Line>,
    /// The file is the project's own: a rename is one command of the
    /// project, renaming the case in the structures' sets too.
    pub project: bool,
    /// The page has changes not saved (a rename of the project waits for
    /// them to be saved or discarded).
    pub dirty: bool,
    /// The file is the load case library the line uses (*8 Applies to*
    /// shows and changes which structures take its cases).
    pub in_use: bool,
    /// The case names of the library as saved (a case not saved yet
    /// cannot be given to structures); none: not read.
    pub known: Option<&'a [String]>,
    /// The structures selected (the Structures table, a view): *Add the
    /// N selected*.
    pub selected: &'a [String],
    /// The line's version (*8 Applies to* resolves the line again when it
    /// changes).
    pub line_version: u64,
    /// Why the line cannot be changed (read-only), if it cannot.
    pub read_only: Option<&'a str>,
}

impl Form {
    /// Reads the cases again when the file's text changed (opened,
    /// discarded, edited in Fields).
    pub fn sync(&mut self, text: &str) {
        if self.mirror.as_deref() == Some(text) {
            return;
        }
        match LoadCases::read_unchecked(text) {
            Ok(c) => {
                self.cases = Some(c);
                self.unreadable = None;
            }
            Err(e) => {
                self.cases = None;
                self.unreadable = Some(e);
            }
        }
        self.mirror = Some(text.to_owned());
        self.buffers.clear();
        self.renaming = None;
    }

    /// The text of the cases as edited (`ohl_loads`' writer), now
    /// mirrored.
    fn written(&mut self) -> Option<String> {
        let text = self.cases.as_ref()?.to_toml();
        self.mirror = Some(text.clone());
        Some(text)
    }
}

/// A number field: the value shown (or the text being typed), the new
/// value when a number was typed. `label` names it (for screen readers
/// and tests).
fn number(
    ui: &mut egui::Ui,
    buffers: &mut HashMap<String, String>,
    key: &str,
    label: &str,
    value: f64,
    width: f32,
) -> Option<f64> {
    let l = ui.label(label);
    let mut text = buffers
        .get(key)
        .cloned()
        .unwrap_or_else(|| format_number(value));
    let r = ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .id_salt(("load-case-number", key))
                .desired_width(width),
        )
        .labelled_by(l.id);
    let mut out = None;
    if r.changed() {
        if let Ok(v) = text.trim().replace(',', ".").parse::<f64>()
            && v.is_finite()
        {
            out = Some(v);
        }
        buffers.insert(key.to_owned(), text);
    } else if !r.has_focus() {
        buffers.remove(key);
    }
    out
}

/// A number as typed back: no trailing zeros.
fn format_number(x: f64) -> String {
    let t = format!("{x:.6}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" { "0".into() } else { t.into() }
}

/// A section's number of spans and its sets.
fn spans_of(s: &ohl_line::Section) -> (usize, Vec<u32>) {
    let mut sets: Vec<u32> = s.points.iter().map(|p| p.set).collect();
    sets.sort_unstable();
    sets.dedup();
    (s.points.len().saturating_sub(1), sets)
}

/// A heading of the form: number and title, a line saying what it is.
fn heading(ui: &mut egui::Ui, n: u32, title: &str, what: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(format!("{n}  {title}")).strong());
    ui.label(egui::RichText::new(what).small().weak());
}

/// The form: the cases on the left, the one picked on the right. Returns
/// the file's new text when an edit was made. `error`: why the file as
/// edited does not check (shown in a box of its own height).
pub fn ui(
    ui: &mut egui::Ui,
    form: &mut Form,
    item: &mut usize,
    cx: &FormContext<'_>,
    error: Option<&str>,
) -> Option<String> {
    let Some(cases) = form.cases.as_mut() else {
        crate::menu::error_label(
            ui,
            format!(
                "The file does not read as load cases: {} — edit it in Fields (expert).",
                form.unreadable.as_deref().unwrap_or("?")
            ),
        );
        return None;
    };
    let mut changed = false;
    let mut request = None;
    let mut give = None;
    let buffers = &mut form.buffers;
    let applies = &mut form.applies;
    egui::Panel::left(egui::Id::new("load-case-form-list"))
        .resizable(true)
        .default_size(210.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Add")
                    .on_hover_text("A new case: the first weather case, every factor 1")
                    .clicked()
                {
                    let taken: Vec<String> = cases.cases.iter().map(|c| c.name.clone()).collect();
                    let name =
                        crate::editors::next_id(taken.iter().map(String::as_str), "new case");
                    let weather = cx
                        .weather
                        .first()
                        .map_or("everyday".to_owned(), |w| w.name.clone());
                    let mut c = LoadCase::from_weather(&weather);
                    c.name = name;
                    c.description.clear();
                    c.tags.clear();
                    cases.cases.push(c);
                    *item = cases.cases.len() - 1;
                    buffers.clear();
                    changed = true;
                }
                if ui
                    .add_enabled(*item < cases.cases.len(), egui::Button::new("Duplicate"))
                    .on_hover_text("A copy of the case picked, under a new name")
                    .clicked()
                {
                    let taken: Vec<String> = cases.cases.iter().map(|c| c.name.clone()).collect();
                    let mut c = cases.cases[*item].clone();
                    c.name = crate::editors::next_id(taken.iter().map(String::as_str), &c.name);
                    cases.cases.insert(*item + 1, c);
                    *item += 1;
                    buffers.clear();
                    changed = true;
                }
                let takers = match (cx.line, cases.cases.get(*item)) {
                    (Some(line), Some(c)) => crate::load_case_sets::takers(line, &c.name),
                    _ => Vec::new(),
                };
                if ui
                    .add_enabled(*item < cases.cases.len(), egui::Button::new("Delete"))
                    .on_hover_text(if takers.is_empty() {
                        "Delete the case picked (no structure group ticks it)".to_owned()
                    } else {
                        format!(
                            "Delete the case picked: group(s) {} tick it; their structures are \
                             then not checked in it (said in the usage and the Structures table)",
                            takers.join(", ")
                        )
                    })
                    .clicked()
                {
                    cases.cases.remove(*item);
                    *item = item.saturating_sub(1);
                    buffers.clear();
                    changed = true;
                }
                if ui
                    .add_enabled(*item < cases.cases.len(), egui::Button::new("Rename"))
                    .on_hover_text(if cx.project {
                        "Give the case picked another name: in the file, in every structure group \
                         ticking it and in its own list (Applies to), one change (undone as one)"
                    } else {
                        "Give the case picked another name (a file outside a project: the \
                         structures' sets naming it are not renamed)"
                    })
                    .clicked()
                {
                    form.renaming = Some(cases.cases[*item].name.clone());
                }
                // OpenOHL's default load cases (owner, 2026-11-15): those
                // missing, by name, after a look at what they are.
                if ui
                    .button(crate::default_cases::ADD_BUTTON)
                    .on_hover_text(
                        "OpenOHL's default load cases for the structures — arbitrary values for \
                         checking (not from a design code): those this file lacks (by name) \
                         are added; a case of the same name is never replaced. Save keeps them \
                         (one change, one undo).",
                    )
                    .clicked()
                {
                    form.adding_defaults = !form.adding_defaults;
                }
            });
            if form.adding_defaults {
                let (missing, kept) = crate::default_cases::missing_load_cases(cases);
                let lacking = crate::default_cases::weather_missing(&missing, cx.weather);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    if missing.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "The default load cases are all there (by name): nothing to add.",
                            )
                            .small(),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(format!(
                                "Adds {} default case(s), arbitrary values for checking (not from \
                                 a design code): {}",
                                missing.len(),
                                missing
                                    .iter()
                                    .map(|c| c.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ))
                            .small(),
                        );
                    }
                    if !kept.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "Already there by name, kept as they are: {}",
                                kept.join(", ")
                            ))
                            .small()
                            .weak(),
                        );
                    }
                    if !lacking.is_empty() {
                        crate::menu::message(
                            ui,
                            egui::RichText::new(format!(
                                "They name weather cases the project does not have: {} — add \
                                 them in Line › Criteria › Weather cases › {}",
                                lacking.join(", "),
                                crate::default_cases::ADD_BUTTON
                            ))
                            .small(),
                            Some(ui.visuals().warn_fg_color),
                        );
                    }
                    ui.horizontal(|ui| {
                        if !missing.is_empty()
                            && ui
                                .button(format!("Add {} case(s)", missing.len()))
                                .clicked()
                        {
                            let first = cases.cases.len();
                            // The 60 N/m ice only at or above 1000 m:
                            // its own list's rule, a command of the line.
                            form.default_groups = missing
                                .iter()
                                .any(|c| c.name == ohl_loads::DEFAULT_HIGH_ICE_CASE);
                            cases.cases.extend(missing.iter().cloned());
                            *item = first;
                            buffers.clear();
                            changed = true;
                            form.adding_defaults = false;
                        }
                        if ui.button("Cancel").clicked() {
                            form.adding_defaults = false;
                        }
                    });
                });
            }
            if let Some(name) = &mut form.renaming {
                ui.horizontal(|ui| {
                    let l = ui.label("new name");
                    ui.add(egui::TextEdit::singleline(name).desired_width(120.0))
                        .labelled_by(l.id);
                });
                let taken = cases
                    .cases
                    .iter()
                    .enumerate()
                    .any(|(k, c)| k != *item && c.name == name.trim());
                let why = if name.trim().is_empty() {
                    Some("a name, please")
                } else if taken {
                    Some("another case has this name")
                } else if cx.project && cx.dirty {
                    Some(
                        "Save or discard the changes first: the rename is one change of the \
                         project, with the structures' sets",
                    )
                } else {
                    None
                };
                // Some(true): renamed, Some(false): cancelled.
                let mut done = None;
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(why.is_none(), egui::Button::new("OK"))
                        .on_disabled_hover_text(why.unwrap_or(""))
                        .clicked()
                    {
                        done = Some(true);
                    } else if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                });
                if done == Some(true)
                    && let Some(c) = cases.cases.get_mut(*item)
                {
                    if cx.project {
                        // One command of the project (the page reads the
                        // file again after it).
                        request = Some((c.name.clone(), name.trim().to_owned()));
                    } else {
                        c.name = name.trim().to_owned();
                        changed = true;
                    }
                }
                if done.is_some() {
                    form.renaming = None;
                }
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("load-case-form-names")
                .show(ui, |ui| {
                    for (k, c) in cases.cases.iter().enumerate() {
                        let bad = c.validate().is_err();
                        let text = if bad {
                            egui::RichText::new(format!("{} ⚠", c.name))
                                .color(ui.visuals().warn_fg_color)
                        } else {
                            egui::RichText::new(&c.name)
                        };
                        if ui
                            .selectable_label(k == *item, text)
                            .on_hover_text(summary(c))
                            .clicked()
                        {
                            *item = k;
                            buffers.clear();
                            form.renaming = None;
                        }
                    }
                });
        });
    egui::CentralPanel::default().show(ui, |ui| {
        if let Some(e) = error {
            // A long error stays in a box of its own height.
            egui::ScrollArea::vertical()
                .id_salt("load-case-form-error")
                .max_height(70.0)
                .show(ui, |ui| {
                    crate::menu::error_label(ui, egui::RichText::new(e).small());
                });
        }
        let Some(case) = cases.cases.get_mut(*item) else {
            ui.label("No case: Add one on the left.");
            return;
        };
        if let Err(e) = case.validate() {
            crate::menu::error_label(ui, egui::RichText::new(e).small());
        }
        egui::ScrollArea::vertical()
            .id_salt("load-case-form-case")
            .auto_shrink(false)
            .show(ui, |ui| {
                changed |= case_ui(ui, case, *item, cx, buffers);
                give = applies_to_ui(ui, &case.name, cx, applies);
            });
    });
    if request.is_some() {
        form.rename = request;
    }
    if give.is_some() {
        form.give = give;
    }
    if changed { form.written() } else { None }
}

/// A case in a few words, for the list's hover.
fn summary(c: &LoadCase) -> String {
    let mut out = format!(
        "weather {}, wires {}, factors V {} T {} L {}, φ {}",
        c.weather,
        c.condition.words(),
        c.vertical,
        c.transverse,
        c.longitudinal,
        c.strength
    );
    for r in &c.removed {
        out.push_str(&format!("; {}", r.describe()));
    }
    for b in c.broken_wires() {
        out.push_str(&format!("; {}", b.describe()));
    }
    if c.use_local {
        out.push_str("; local conditions");
    }
    out
}

/// The case's sections; true when changed.
fn case_ui(
    ui: &mut egui::Ui,
    case: &mut LoadCase,
    item: usize,
    cx: &FormContext<'_>,
    buffers: &mut HashMap<String, String>,
) -> bool {
    let mut changed = false;
    let key = |what: &str| format!("{item}:{what}");
    ui.label(egui::RichText::new(&case.name).heading());
    // 1 Weather.
    heading(
        ui,
        1,
        "Weather",
        "The weather case of the project this load case takes (temperature, wind, ice on the \
         wires), and the wires' condition.",
    );
    let weather = cx.weather.iter().find(|w| w.name == case.weather);
    ui.horizontal(|ui| {
        let l = ui.label("Weather case");
        egui::ComboBox::from_id_salt("load-case-weather")
            .selected_text(if weather.is_some() {
                case.weather.clone()
            } else {
                format!("{} (not in the project)", case.weather)
            })
            .width(220.0)
            .show_ui(ui, |ui| {
                for w in cx.weather {
                    if ui
                        .selectable_label(w.name == case.weather, &w.name)
                        .on_hover_text(w.summary())
                        .clicked()
                        && w.name != case.weather
                    {
                        case.weather = w.name.clone();
                        changed = true;
                    }
                }
            })
            .response
            .labelled_by(l.id)
            .on_hover_text(
                "Only the project's weather cases (Line › Criteria › Weather cases): a load \
                 case naming another is refused when the loads are computed",
            );
    });
    if weather.is_none() {
        crate::menu::error_label(
            ui,
            egui::RichText::new(format!(
                "\"{}\" is not a weather case of the project: pick one",
                case.weather
            ))
            .small(),
        );
    }
    ui.horizontal(|ui| {
        ui.label("Wires");
        egui::ComboBox::from_id_salt("load-case-condition")
            .selected_text(case.condition.words())
            .width(260.0)
            .show_ui(ui, |ui| {
                for c in WireCondition::ALL {
                    if ui
                        .selectable_label(c == case.condition, c.words())
                        .clicked()
                        && c != case.condition
                    {
                        case.condition = c;
                        changed = true;
                    }
                }
            })
            .response
            .on_hover_text(
                "The wires' state: as strung, after creep, after the heaviest load, or the \
                 one that sags more (the line's Creep-Stretch settings)",
            );
    });
    // 2 Wind on the wires: only when the weather case has wind.
    let windy = weather.is_some_and(|w| w.wind_pressure.si() > 0.0);
    if windy {
        let side = weather.map_or(WindSide::Right, |w| w.wind_from);
        heading(
            ui,
            2,
            "Wind on the wires",
            "Which side the wind on the wires and strings comes from, looking ahead along the \
             line.",
        );
        ui.horizontal(|ui| {
            for (label, value) in [
                (
                    format!("as the weather case (from the {})", side.as_str()),
                    None,
                ),
                ("from the left".to_owned(), Some(WindSide::Left)),
                ("from the right".to_owned(), Some(WindSide::Right)),
            ] {
                if ui
                    .radio(case.wind_from == value, label)
                    .on_hover_text("No extra time: one solve per section as always")
                    .clicked()
                    && case.wind_from != value
                {
                    case.wind_from = value;
                    changed = true;
                }
            }
        });
    } else if case.wind_from.is_some() {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(
                    "The wind side given has no effect: this weather case has no wind.",
                )
                .small()
                .color(ui.visuals().warn_fg_color),
            );
            if ui.small_button("Clear it").clicked() {
                case.wind_from = None;
                changed = true;
            }
        });
    }
    // 3 Wires removed and wires broken.
    heading(
        ui,
        3,
        "Wires removed and wires broken",
        "Removed: a wire not strung yet (a construction state, e.g. a tension tower with some \
         phases missing) — every span of that section is left out, it puts no load anywhere. \
         Broken: a real break of a wire that is there — that section is solved with the wire \
         of the span gone, the strings at its ends swing to the spans left.",
    );
    changed |= wires_ui(ui, case, cx);
    // 4 Wind on the structures.
    heading(
        ui,
        4,
        "Wind on the structures",
        "The wind on the structures' own bodies (wind bands, members, poles). The wires keep \
         the weather case's wind.",
    );
    changed |= structure_wind_ui(ui, case, item, weather, buffers);
    // 5 Ice and snow on the structures.
    heading(
        ui,
        5,
        "Ice and snow on the structures",
        "Weight on the structures' members (ice all round each member, snow on its plan) and on \
         poles (ice); the wires' ice is the weather case's.",
    );
    changed |= ice_snow_ui(ui, case, item, weather, buffers);
    // 6 Factors and local conditions.
    heading(
        ui,
        6,
        "Factors",
        "Load factors on the vertical, transverse and longitudinal loads, and the strength \
         (resistance) factor φ of the capacity checks.",
    );
    ui.horizontal_wrapped(|ui| {
        for (what, label, value) in [
            ("vertical", "vertical ×", &mut case.vertical),
            ("transverse", "transverse ×", &mut case.transverse),
            ("longitudinal", "longitudinal ×", &mut case.longitudinal),
            ("strength", "strength φ", &mut case.strength),
        ] {
            if let Some(v) = number(ui, buffers, &key(what), label, *value, 50.0)
                && v != *value
            {
                *value = v;
                changed = true;
            }
        }
    });
    if ui
        .checkbox(&mut case.use_local, "Use local conditions")
        .on_hover_text(
            "Takes each structure's local conditions (Structures table): the wires solved with \
             their spans' wind and ice for strength (from the structures' wind_static and \
             wire_ice), the wind on each structure's body times its wind_static. The same as \
             ticking Use local conditions in Checks › Structure loads, for this case always. \
             Cost: the sections solved once more (on the line with the local conditions) when \
             a structure gives some.",
        )
        .changed()
    {
        changed = true;
    }
    // 7 Tags and description.
    heading(
        ui,
        7,
        "Tags and description",
        "What the case stands for (a code clause, a limit state); tags separated by commas.",
    );
    ui.horizontal(|ui| {
        let l = ui.label("Tags");
        let mut tags = buffers
            .get(&key("tags"))
            .cloned()
            .unwrap_or_else(|| case.tags.join(", "));
        let r = ui
            .add(egui::TextEdit::singleline(&mut tags).desired_width(260.0))
            .labelled_by(l.id);
        if r.changed() {
            let new: Vec<String> = tags
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect();
            buffers.insert(key("tags"), tags);
            if new != case.tags {
                case.tags = new;
                changed = true;
            }
        } else if !r.has_focus() {
            buffers.remove(&key("tags"));
        }
    });
    let l = ui.label("Description");
    if ui
        .add(
            egui::TextEdit::multiline(&mut case.description)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        )
        .labelled_by(l.id)
        .changed()
    {
        changed = true;
    }
    changed
}

/// *8 Applies to*: the help (the short line, the longer text behind the
/// (?)).
pub const APPLIES_HELP: (&str, &str) = (
    "Which structures take this case: ticked. Tick or untick them here, per structure, whatever \
     its type.",
    "Each row: the structure, its station on its alignment, its type, and how it gets the case \
     — through a structure group (picked there, or by a rule of it: type, line angle, station, \
     dead end), in no group (every case), or picked here. Ticking a structure that does not take \
     the case picks it in the case's own list: it takes the case on top of its groups' cases. \
     Unticking one picked here takes it out; unticking one that gets the case through a group's \
     rule, a group it is picked in, or by being in no group excludes it here: exactly this case \
     leaves it, nothing else changes — or change the rule in Line › Structures › Load cases per \
     group. An exclusion is shown here, in the groups window (the own list) and in the \
     Structures table. Press and drag across rows to give them all one state, shift-click for a \
     range; All, None and Invert act on the rows shown (the search filters them). Each change is \
     one step of Edit › Undo. Nothing is solved again: the case is solved once for the line.",
);

/// The structures selected that do (`true`) or do not take the case, of
/// those in the line.
fn selected_split(
    line: &ohl_line::Line,
    selected: &[String],
    takers: &[String],
) -> [Vec<String>; 2] {
    let there: Vec<String> = selected
        .iter()
        .filter(|s| line.structure(s).is_some())
        .cloned()
        .collect();
    let (held, new): (Vec<String>, Vec<String>) =
        there.into_iter().partition(|s| takers.contains(s));
    [new, held]
}

/// *8 Applies to* of case `name` (owner, 2026-11-15: "manual should
/// always be available where load cases rules are defined and on which
/// pylon they apply with a list of them. Not per type but per
/// position"): the structures taking it and how, a tick per structure of
/// the line (a filter, press-and-drag, shift for a range, All / None /
/// Invert) and *Add / Remove the N selected*; a change asked of the
/// project as one command ([`Give`]).
fn applies_to_ui(
    ui: &mut egui::Ui,
    name: &str,
    cx: &FormContext<'_>,
    st: &mut AppliesTo,
) -> Option<Give> {
    heading(
        ui,
        8,
        "Applies to",
        "The structures of the line that take this case and how each gets it; tick or untick \
         them here — per structure, whatever its type.",
    );
    let Some(line) = cx.line else {
        ui.label(
            egui::RichText::new("No line open: no structures to give the case to.")
                .small()
                .weak(),
        );
        return None;
    };
    if !cx.in_use {
        ui.label(
            egui::RichText::new(
                "This file is not the load case library the line uses: which structures take a \
                 case is shown and changed for the library in use (Use for the model).",
            )
            .small()
            .weak(),
        );
        return None;
    }
    let fresh = st
        .cache
        .as_ref()
        .is_some_and(|(v, g, _, _)| *v == cx.line_version && *g == line.structure_groups);
    if !fresh {
        st.cache = Some((
            cx.line_version,
            line.structure_groups.clone(),
            line.case_sets(),
            line.structure_facts_with_stations(),
        ));
    }
    let (_, _, sets, facts) = st.cache.as_ref().expect("set above");
    let takers = sets.takers(name);
    let n = line.structures.len();
    ui.horizontal_wrapped(|ui| {
        if takers.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "No structure takes this case: it gives no loads and no check anywhere.",
            );
        } else {
            ui.label(format!(
                "Taken by {} of {n} structure(s): {}",
                takers.len(),
                first_ids(&takers)
            ));
        }
        crate::check_help::question_both(ui, APPLIES_HELP.0, APPLIES_HELP.1);
    });
    let saved = cx.known.is_none_or(|k| k.iter().any(|c| c == name));
    if let Some(why) = cx.read_only {
        crate::menu::error_label(ui, egui::RichText::new(format!("read-only: {why}")).small());
    }
    if !saved {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            egui::RichText::new(format!(
                "Save the file first: \"{name}\" is not in the line's library yet (structures \
                 take a case by its saved name)."
            ))
            .small(),
        );
    }
    let editable = cx.read_only.is_none() && saved;
    if st.pending.as_ref().is_some_and(|(c, _)| c != name) {
        st.pending = None;
    }
    let alignment_name = |id: &str| {
        line.alignments.iter().find(|a| a.id == id).map_or_else(
            || id.to_owned(),
            |a| {
                if a.name.is_empty() {
                    a.id.clone()
                } else {
                    a.name.clone()
                }
            },
        )
    };
    let items: Vec<crate::tick_list::Item> = line
        .structures
        .iter()
        .map(|s| {
            let at = facts
                .get(&s.id)
                .and_then(|f| f.stations.first())
                .map_or("on no alignment".to_owned(), |(a, st)| {
                    format!("{} m on {}", format_station(*st), alignment_name(a))
                });
            let ty = s
                .structure_type
                .as_deref()
                .map_or("no type".to_owned(), |t| format!("type {t}"));
            let how = crate::load_case_sets::why_words(line, &sets.why(&s.id, name));
            crate::tick_list::Item::new(s.id.clone(), format!("{} · {at} · {ty} — {how}", s.id))
        })
        .collect();
    let mut chosen = Some(
        st.pending
            .as_ref()
            .map_or_else(|| takers.clone(), |p| p.1.clone()),
    );
    let changed = ui
        .add_enabled_ui(editable, |ui| {
            crate::tick_list::tick_list(ui, ("load-case-applies", name), &items, &mut chosen, 220.0)
        })
        .inner;
    // The list reads the pointer itself: greyed, a press changes nothing.
    if changed && editable {
        let ids = chosen.unwrap_or_else(|| line.structures.iter().map(|s| s.id.clone()).collect());
        st.pending = Some((name.to_owned(), ids));
    }
    let mut out = None;
    // Asked when the button is let go: a drag across rows is one change.
    if !ui.input(|i| i.pointer.primary_down())
        && let Some((case, ids)) = st.pending.take()
    {
        let take: Vec<String> = ids
            .iter()
            .filter(|s| !takers.contains(s))
            .cloned()
            .collect();
        let drop: Vec<String> = takers
            .iter()
            .filter(|s| !ids.contains(s))
            .cloned()
            .collect();
        if !take.is_empty() || !drop.is_empty() {
            out = Some(Give { case, take, drop });
        }
    }
    let [new, held] = selected_split(line, cx.selected, &takers);
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !new.is_empty(),
                    egui::Button::new(format!("Add the {} selected", new.len())),
                )
                .on_hover_text(format!(
                    "Give the case to {} (selected in the Structures table or a view)",
                    new.join(", ")
                ))
                .on_disabled_hover_text(
                    "Select structures that do not take it in the Structures table or in a view \
                     first (Shift or Cmd/Ctrl + click for several)",
                )
                .clicked()
            {
                out = Some(Give {
                    case: name.to_owned(),
                    take: new.clone(),
                    drop: Vec::new(),
                });
            }
            if !held.is_empty()
                && ui
                    .button(format!("Remove the {} selected", held.len()))
                    .on_hover_text(format!(
                        "{} not to take the case (taken out of its own list, or excluded here \
                         when a group gives it)",
                        held.join(", ")
                    ))
                    .clicked()
            {
                out = Some(Give {
                    case: name.to_owned(),
                    take: Vec::new(),
                    drop: held.clone(),
                });
            }
        });
    });
    // The case's own list in words: where else it is seen.
    if let Some(k) = sets.own_list(name) {
        let picked = sets.members(k);
        let excluded = sets.excluded(k);
        ui.label(
            egui::RichText::new(format!(
                "Its own list: {} picked here{}{} — also in Line › Structures › Load cases per \
                 group as \"{name} (own list)\".",
                picked.len(),
                if picked.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", first_ids(picked))
                },
                if excluded.is_empty() {
                    String::new()
                } else {
                    format!(", {} excluded ({})", excluded.len(), first_ids(excluded))
                }
            ))
            .small(),
        );
    }
    out
}

/// A station as the rows show it (metres, one decimal).
fn format_station(m: f64) -> String {
    format!("{m:.1}")
}

/// The first ids of `ids` (ten at most, then how many more).
fn first_ids(ids: &[String]) -> String {
    let mut s = ids
        .iter()
        .take(10)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if ids.len() > 10 {
        s.push_str(&format!(" and {} more", ids.len() - 10));
    }
    s
}

/// Section 3: the line's sections, a tick to remove each; per section its
/// spans to break (or every span one at a time) and the set checked.
fn wires_ui(ui: &mut egui::Ui, case: &mut LoadCase, cx: &FormContext<'_>) -> bool {
    let mut changed = false;
    // The sections: the line's, then those the case names that are not
    // in it.
    let mut ids: Vec<String> = cx
        .line
        .map(|l| l.sections.iter().map(|s| s.id.clone()).collect())
        .unwrap_or_default();
    for id in case
        .removed
        .iter()
        .map(|r| r.section.clone())
        .chain(case.broken_wires().map(|b| b.section.clone()))
    {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        ui.label(
            egui::RichText::new("No line open: no sections to pick (Fields (expert) names them).")
                .small()
                .weak(),
        );
        return false;
    }
    let mut broken: Vec<BrokenWire> = case.broken_wires().cloned().collect();
    egui::ScrollArea::vertical()
        .id_salt("load-case-form-wires")
        .max_height(260.0)
        .show(ui, |ui| {
            for id in &ids {
                let section = cx
                    .line
                    .and_then(|l| l.sections.iter().find(|s| &s.id == id));
                let (n, sets) = section.map_or((0, Vec::new()), spans_of);
                let what = match section {
                    Some(s) if !s.phase_label.is_empty() => {
                        format!("{id} (phase {}, {n} span(s))", s.phase_label)
                    }
                    Some(_) => format!("{id} ({n} span(s))"),
                    None => format!("{id} (not in the line)"),
                };
                let removed = case.removed.iter().any(|r| &r.section == id);
                let breaks: Vec<BrokenSpan> = broken
                    .iter()
                    .filter(|b| &b.section == id)
                    .map(|b| b.span)
                    .collect();
                let header = egui::CollapsingHeader::new(what)
                    .id_salt(("load-case-section", id))
                    .default_open(removed || !breaks.is_empty() || ids.len() <= 4);
                header.show(ui, |ui| {
                    let mut on = removed;
                    if ui
                        .add_enabled(
                            breaks.is_empty(),
                            egui::Checkbox::new(&mut on, format!("{id} removed (not strung)")),
                        )
                        .on_hover_text(
                            "Every span of this section left out of the solve: no loads from \
                             it on any structure (the jumpers and spacers on it too). No extra \
                             time.",
                        )
                        .on_disabled_hover_text("A broken wire of it is ticked below")
                        .changed()
                    {
                        if on {
                            case.removed.push(RemovedWire {
                                section: id.clone(),
                                set: None,
                            });
                        } else {
                            case.removed.retain(|r| &r.section != id);
                        }
                        changed = true;
                    }
                    ui.add_enabled_ui(!removed, |ui| {
                        let mut every = breaks.contains(&BrokenSpan::Every);
                        if ui
                            .checkbox(&mut every, format!("{id} every span broken, one at a time"))
                            .on_hover_text(format!(
                                "Each span of {id} broken on its own, the worst of each force \
                                 kept (the structures' finite-element checks take each break \
                                 on its own). Cost: one solve of the section per span ({n})."
                            ))
                            .changed()
                        {
                            broken.retain(|b| &b.section != id);
                            if every {
                                let set = None;
                                broken.push(BrokenWire {
                                    section: id.clone(),
                                    span: BrokenSpan::Every,
                                    set,
                                    subconductors: None,
                                });
                            }
                            changed = true;
                        }
                        for k in 1..=n {
                            let ends = section.map_or(String::new(), |s| {
                                format!(
                                    " ({}–{})",
                                    s.points[k - 1].structure,
                                    s.points[k].structure
                                )
                            });
                            let mut on = breaks.contains(&BrokenSpan::One(k));
                            if ui
                                .add_enabled(
                                    !every,
                                    egui::Checkbox::new(
                                        &mut on,
                                        format!("{id} span {k}{ends} broken"),
                                    ),
                                )
                                .on_hover_text(
                                    "The wire of this span broken; several ticked break \
                                     together (one solve of the section, the static state after \
                                     the breaks). Cost: one more solve of the section.",
                                )
                                .changed()
                            {
                                if on {
                                    // A span ticked takes what the
                                    // section's other breaks say (their
                                    // set, their subconductors: the
                                    // file's, no field here yet).
                                    let like = broken.iter().find(|b| &b.section == id);
                                    let set = like.and_then(|b| b.set);
                                    let subconductors = like.and_then(|b| b.subconductors);
                                    broken.push(BrokenWire {
                                        section: id.clone(),
                                        span: BrokenSpan::One(k),
                                        set,
                                        subconductors,
                                    });
                                    broken.sort_by_key(|b| {
                                        (
                                            ids.iter().position(|x| *x == b.section),
                                            match b.span {
                                                BrokenSpan::One(k) => k,
                                                BrokenSpan::Every => 0,
                                            },
                                        )
                                    });
                                } else {
                                    broken.retain(|b| {
                                        !(&b.section == id && b.span == BrokenSpan::One(k))
                                    });
                                }
                                changed = true;
                            }
                        }
                        // The set checked (optional): the wire meant.
                        if !breaks.is_empty() && sets.len() > 1 {
                            let now = broken.iter().find(|b| &b.section == id).and_then(|b| b.set);
                            ui.horizontal(|ui| {
                                ui.label("set checked");
                                egui::ComboBox::from_id_salt(("load-case-set", id))
                                    .selected_text(now.map_or("—".into(), |s| s.to_string()))
                                    .show_ui(ui, |ui| {
                                        for s in std::iter::once(None)
                                            .chain(sets.iter().copied().map(Some))
                                        {
                                            if ui
                                                .selectable_label(
                                                    s == now,
                                                    s.map_or("—".into(), |s| s.to_string()),
                                                )
                                                .clicked()
                                                && s != now
                                            {
                                                for b in
                                                    broken.iter_mut().filter(|b| &b.section == id)
                                                {
                                                    b.set = s;
                                                }
                                                changed = true;
                                            }
                                        }
                                    })
                                    .response
                                    .on_hover_text(
                                        "A check that the broken wire is the one meant: a span's \
                                         ends must be of this set (—: not checked)",
                                    );
                            });
                        }
                    });
                });
            }
        });
    if changed {
        case.set_broken(broken);
    }
    changed
}

/// How the structures' wind pressure is given.
#[derive(Clone, Copy, PartialEq)]
enum Pressure {
    Off,
    Weather,
    Own,
}

/// Section 4.
fn structure_wind_ui(
    ui: &mut egui::Ui,
    case: &mut LoadCase,
    item: usize,
    weather: Option<&WeatherCase>,
    buffers: &mut HashMap<String, String>,
) -> bool {
    let mut changed = false;
    let key = |what: &str| format!("{item}:{what}");
    let mut own = case.structure.unwrap_or_default();
    let mode = match own.wind_pressure {
        Some(0.0) => Pressure::Off,
        Some(_) => Pressure::Own,
        None => Pressure::Weather,
    };
    let theirs = weather.map_or(0.0, |w| w.wind_pressure.si());
    ui.horizontal_wrapped(|ui| {
        for (label, m, hover) in [
            ("off", Pressure::Off, "No wind on the structures' bodies"),
            (
                "the weather case's",
                Pressure::Weather,
                "The weather case's pressure on the bodies too",
            ),
            (
                "own pressure",
                Pressure::Own,
                "A pressure of this case for the bodies only (a code treating the wind on \
                 towers apart)",
            ),
        ] {
            let text = if m == Pressure::Weather {
                format!("{label} ({theirs:.0} Pa)")
            } else {
                label.to_owned()
            };
            if ui.radio(mode == m, text).on_hover_text(hover).clicked() && mode != m {
                own.wind_pressure = match m {
                    Pressure::Off => Some(0.0),
                    Pressure::Weather => None,
                    Pressure::Own => Some(if theirs > 0.0 { theirs } else { 500.0 }),
                };
                changed = true;
            }
        }
        if mode == Pressure::Own
            && let Some(v) = number(
                ui,
                buffers,
                &key("pressure"),
                "Pa",
                own.wind_pressure.unwrap_or(0.0),
                60.0,
            )
            && v >= 0.0
        {
            own.wind_pressure = Some(v);
            changed = true;
        }
    });
    if mode != Pressure::Off {
        let side = case
            .wind_from
            .or(weather.map(|w| w.wind_from))
            .unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            ui.label("Direction");
            let d = case.on_structure.direction;
            let angle = match d {
                Some(WindDirection::Angle(a)) => Some(a),
                _ => None,
            };
            for (label, value) in [
                (format!("as the wires (from the {})", side.as_str()), None),
                ("from the left".to_owned(), Some(WindDirection::Left)),
                ("from the right".to_owned(), Some(WindDirection::Right)),
                ("from ahead".to_owned(), Some(WindDirection::Ahead)),
                ("from behind".to_owned(), Some(WindDirection::Back)),
            ] {
                if ui.selectable_label(d == value, label).clicked() && d != value {
                    case.on_structure.direction = value;
                    changed = true;
                }
            }
            if ui
                .selectable_label(angle.is_some(), "at an angle")
                .on_hover_text(
                    "The angle the wind comes from: 0° from the right, 90° from ahead, 180° \
                     from the left, 270° from behind. A wind band's area is taken all along the \
                     direction (an approximation, see the theory: Structure wind)",
                )
                .clicked()
                && angle.is_none()
            {
                case.on_structure.direction =
                    Some(WindDirection::Angle(d.map_or(0.0, WindDirection::degrees)));
                changed = true;
            }
            if let Some(a) = angle
                && let Some(v) = number(ui, buffers, &key("angle"), "angle °", a, 50.0)
                && v != a
            {
                case.on_structure.direction = Some(WindDirection::Angle(v));
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            let mut has = own.wind_factor.is_some();
            if ui
                .checkbox(&mut has, "own factor")
                .on_hover_text(format!(
                    "A factor on the bodies' pressure of this case (unticked: the transverse \
                     factor, {})",
                    case.transverse
                ))
                .changed()
            {
                own.wind_factor = has.then_some(case.transverse);
                changed = true;
            }
            if let Some(f) = own.wind_factor
                && let Some(v) = number(ui, buffers, &key("wind factor"), "×", f, 50.0)
                && v != f
            {
                own.wind_factor = Some(v);
                changed = true;
            }
        });
    }
    if changed {
        case.structure = (!own.is_empty()).then_some(own);
    }
    changed
}

/// How the structures' ice is given.
#[derive(Clone, Copy, PartialEq)]
enum Ice {
    /// Not given: the local ice where the case takes the local conditions
    /// and its weather case has ice, else none.
    ByWeather,
    None,
    Local,
    Thickness,
    /// The weather case's ice on the wires (version 6).
    Weather,
}

/// Section 5.
fn ice_snow_ui(
    ui: &mut egui::Ui,
    case: &mut LoadCase,
    item: usize,
    weather: Option<&WeatherCase>,
    buffers: &mut HashMap<String, String>,
) -> bool {
    let mut changed = false;
    let key = |what: &str| format!("{item}:{what}");
    // The local ice a case taking the local conditions puts on the
    // structures when its weather case has ice (owner 2026-11-04).
    let local_ice = case.use_local && weather.is_some_and(WeatherCase::has_ice);
    let o = &mut case.on_structure;
    let mode = match o.ice {
        None => Ice::ByWeather,
        Some(StructureIce::NoIce) => Ice::None,
        Some(StructureIce::Local) => Ice::Local,
        Some(StructureIce::Thickness(_)) => Ice::Thickness,
        Some(StructureIce::Weather) => Ice::Weather,
    };
    ui.horizontal_wrapped(|ui| {
        ui.label("Ice");
        for (label, m, hover) in [
            (
                "default",
                Ice::ByWeather,
                "Each structure's local ice when the case takes the local conditions (its Use \
                 local conditions, or a check's switch) and its weather case has ice; else no \
                 ice on the structures",
            ),
            (
                "none",
                Ice::None,
                "No ice on the structures, not even their local ice when the case takes the \
                 local conditions",
            ),
            (
                "the structures' local ice",
                Ice::Local,
                "Each structure's own ice thickness (Structures table › local conditions: \
                 ice on the structure); none where it gives none",
            ),
            (
                "the wires' ice",
                Ice::Weather,
                "The weather case's ice thickness on the wires, on every structure taking this \
                 case (with a factor below to make it more or less); refused when the weather \
                 case gives its ice as a load per metre of wire",
            ),
            (
                "a thickness",
                Ice::Thickness,
                "The same radial thickness on every structure taking this case",
            ),
        ] {
            if ui.radio(mode == m, label).on_hover_text(hover).clicked() && mode != m {
                o.ice = match m {
                    Ice::ByWeather => None,
                    Ice::None => Some(StructureIce::NoIce),
                    Ice::Local => Some(StructureIce::Local),
                    Ice::Thickness => Some(StructureIce::Thickness(0.01)),
                    Ice::Weather => Some(StructureIce::Weather),
                };
                if m == Ice::None || (m == Ice::ByWeather && !local_ice) {
                    o.ice_density = None;
                    o.ice_factor = None;
                }
                changed = true;
            }
        }
        if let Some(StructureIce::Thickness(t)) = o.ice
            && let Some(v) = number(ui, buffers, &key("ice"), "mm", t * 1000.0, 50.0)
            && v >= 0.0
        {
            o.ice = Some(StructureIce::Thickness(v / 1000.0));
            changed = true;
        }
    });
    let iced = matches!(
        o.ice,
        Some(StructureIce::Local | StructureIce::Thickness(_) | StructureIce::Weather)
    ) || (o.ice.is_none()
        && (local_ice || o.ice_density.is_some() || o.ice_factor.is_some()));
    if iced {
        let wires = weather.map_or(String::new(), |w| {
            format!(" (the wires': {:.0} mm)", w.ice_thickness.si() * 1000.0)
        });
        ui.horizontal(|ui| {
            let mut has = o.ice_factor.is_some();
            if ui
                .checkbox(&mut has, "factor on the thickness")
                .on_hover_text(format!(
                    "The structures' ice thickness, whichever it is above, times this factor \
                     (unticked: 1) — e.g. 0.5 with the wires' ice: half the wires' thickness on \
                     the bodies{wires}. Its weight is then times the vertical factor as before."
                ))
                .changed()
            {
                o.ice_factor = has.then_some(1.0);
                changed = true;
            }
            if let Some(f) = o.ice_factor
                && let Some(v) = number(ui, buffers, &key("ice factor"), "×", f, 50.0)
                && v != f
                && v >= 0.0
            {
                o.ice_factor = Some(v);
                changed = true;
            }
            if let (Some(StructureIce::Weather), Some(w)) = (o.ice, weather) {
                let t = w.ice_thickness.si() * o.ice_factor.unwrap_or(1.0) * 1000.0;
                ui.label(egui::RichText::new(format!("= {t:.1} mm on the structures")).small());
            }
        });
    }
    if iced && !(o.ice.is_none() && !local_ice && o.ice_density.is_none()) {
        let by_weather = o.ice == Some(StructureIce::Weather);
        let theirs = weather.and_then(|w| w.ice_density).map(|d| d.si());
        ui.horizontal(|ui| {
            let mut has = o.ice_density.is_some();
            if ui
                .checkbox(&mut has, "own density")
                .on_hover_text(match theirs {
                    Some(d) => format!("Unticked: the weather case's, {d:.0} kg/m³"),
                    None if by_weather => "Unticked: the wires' — glaze ice, the weather case \
                                           giving no density"
                        .to_owned(),
                    None => "Unticked: the weather case's — it gives none, so a density is \
                             needed here (none is made up)"
                        .to_owned(),
                })
                .changed()
            {
                o.ice_density = has.then_some(theirs.unwrap_or(900.0));
                changed = true;
            }
            match o.ice_density {
                Some(d) => {
                    if let Some(v) = number(ui, buffers, &key("density"), "kg/m³", d, 60.0)
                        && v != d
                    {
                        o.ice_density = Some(v);
                        changed = true;
                    }
                }
                None if theirs.is_none() && !by_weather => {
                    crate::menu::error_label(
                        ui,
                        egui::RichText::new("the weather case gives no density: tick own density")
                            .small(),
                    );
                }
                None => {}
            }
        });
    }
    let sia = "SIA 261:2014: μ the shape coefficient, 0.8 where the snow cannot slide off \
               (5.3.4) — taken for a structure's horizontal surfaces, an assumption the \
               engineer is to confirm for a pylon; C_e the exposure coefficient, 1.0 for normal \
               exposure (5.2.4); C_T the thermal coefficient, 1.0 (5.2.5). q = μ·C_e·C_T·s_k on \
               each member's plan, s_k each structure's snow on the ground (Structures table › \
               local conditions).";
    ui.horizontal_wrapped(|ui| {
        if ui
            .checkbox(&mut o.snow, "Snow on the structures")
            .on_hover_text(sia)
            .changed()
        {
            if !o.snow {
                o.snow_shape = None;
                o.snow_exposure = None;
                o.snow_thermal = None;
            }
            changed = true;
        }
        if o.snow {
            for (what, label, default, value) in [
                ("mu", "μ", DEFAULT_SNOW_SHAPE, &mut o.snow_shape),
                ("ce", "C_e", DEFAULT_SNOW_EXPOSURE, &mut o.snow_exposure),
                ("ct", "C_T", DEFAULT_SNOW_THERMAL, &mut o.snow_thermal),
            ] {
                let now = value.unwrap_or(default);
                if let Some(v) = number(ui, buffers, &key(what), label, now, 40.0)
                    && v != now
                    && v >= 0.0
                {
                    *value = (v != default).then_some(v);
                    changed = true;
                }
            }
            ui.label("(about)").on_hover_text(sia);
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_back_plainly() {
        assert_eq!(format_number(1.0), "1");
        assert_eq!(format_number(1.35), "1.35");
        assert_eq!(format_number(-0.0), "0");
    }
}
