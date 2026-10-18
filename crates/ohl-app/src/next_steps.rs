// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The **Next steps** page (View › Next steps, Help › Next steps): what a
//! line still needs, in the order of the work, computed from the project
//! each time it is drawn — route, alignment, ground, structure types,
//! structures, strings, cables, wires, jumpers, weather cases, load cases,
//! clearance rules. Each step is a link to where it is done (a tool held,
//! a page or a window opened). Nothing is stored: a step is done when the
//! project has it.

use eframe::egui;

use crate::app::{Ready, Tab};
use crate::library::LibraryKind;
use crate::model::Model;
use crate::tools::Tool;

/// Where a step's link leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Go {
    /// The new project's first-steps window (the route).
    FirstSteps,
    /// A tool held (with how to use it in the status line).
    Hold(Tool),
    /// A library page.
    Library(LibraryKind),
    /// Terrain › Terrain surfaces.
    Surfaces,
    /// Structures › New pylon from a template.
    PylonBuilder,
    /// Structures › Place on alignment points.
    Placement,
    /// Checks › Ground clearances › Vertical clearances.
    VerticalClearances,
}

/// One step of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// What the line needs.
    pub title: &'static str,
    /// The project has it.
    pub done: bool,
    /// What the project has, or how it is done.
    pub detail: String,
    /// The link.
    pub go: Go,
    /// The link's text.
    pub link: &'static str,
    /// Only some lines need it (jumpers: strain structures).
    pub optional: bool,
}

/// The steps for `model`; `rules`: the project has clearance rules.
pub fn steps(model: &Model, rules: bool) -> Vec<Step> {
    let line = &model.line;
    let count = |n: usize, one: &str, many: &str| match n {
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    };
    let types = model.types.as_ref().map_or(0, |(s, _)| s.len());
    let strings = line
        .structures
        .iter()
        .filter(|s| !s.wires.is_empty())
        .count();
    let mut out = vec![
        Step {
            title: "A route",
            done: !line.routes.is_empty(),
            detail: if line.routes.is_empty() {
                "the path of the line: its alignments, structures and wires belong to it".into()
            } else {
                count(line.routes.len(), "route", "routes")
            },
            go: Go::FirstSteps,
            link: "Name the route…",
            optional: false,
        },
        Step {
            title: "An alignment",
            done: !line.alignments.is_empty(),
            detail: if line.alignments.is_empty() {
                "right-click its points in order with New alignment, Enter to make it".into()
            } else {
                count(line.alignments.len(), "alignment", "alignments")
            },
            go: Go::Hold(Tool::NewAlignment),
            link: "Hold New alignment",
            optional: false,
        },
        Step {
            title: "The ground",
            done: model.ground.is_some(),
            detail: if model.ground.is_some() {
                "a terrain surface is the ground".into()
            } else {
                "a terrain surface: import survey points (CSV), LandXML or OBJ".into()
            },
            go: Go::Surfaces,
            link: "Terrain surfaces…",
            optional: false,
        },
        Step {
            title: "Structure types",
            done: types > 0,
            detail: if types > 0 {
                count(types, "structure type", "structure types")
            } else {
                "build one from a pylon template (suspension and strain)".into()
            },
            go: Go::PylonBuilder,
            link: "New pylon from a template…",
            optional: false,
        },
        Step {
            title: "Structures on the line",
            done: !line.structures.is_empty(),
            detail: if line.structures.is_empty() {
                "a type on the alignment's points, or Add structure on the ground".into()
            } else {
                count(line.structures.len(), "structure", "structures")
            },
            go: if line.alignments.is_empty() {
                Go::Hold(Tool::AddStructure)
            } else {
                Go::Placement
            },
            link: if line.alignments.is_empty() {
                "Hold Add structure"
            } else {
                "Place on alignment points…"
            },
            optional: false,
        },
        Step {
            title: "Strings on the structures",
            done: strings > 0,
            detail: if strings > 0 {
                format!(
                    "{strings} of {} structures have wire points",
                    line.structures.len()
                )
            } else {
                "insulator strings (a type brings its own; Add string on a structure point)".into()
            },
            go: Go::Hold(Tool::AddString),
            link: "Hold Add string",
            optional: false,
        },
        Step {
            title: "Cables",
            done: !model.library.is_empty(),
            detail: if model.library.is_empty() {
                "the wire library is empty: add or import a cable file (.ohlw)".into()
            } else {
                count(model.library.len(), "cable", "cables")
            },
            go: Go::Library(LibraryKind::Cables),
            link: "Open the wires library",
            optional: false,
        },
        Step {
            title: "Wires strung",
            done: !line.sections.is_empty(),
            detail: if line.sections.is_empty() {
                "right-click the strings in line order with String wire, Enter".into()
            } else {
                count(line.sections.len(), "section", "sections")
            },
            go: Go::Hold(Tool::StringWire),
            link: "Hold String wire",
            optional: false,
        },
        Step {
            title: "Jumpers",
            done: !line.jumpers.is_empty(),
            detail: if line.jumpers.is_empty() {
                "where sections end on a strain structure: right-click it with Add jumper".into()
            } else {
                count(line.jumpers.len(), "jumper", "jumpers")
            },
            go: Go::Hold(Tool::AddJumper),
            link: "Hold Add jumper",
            optional: true,
        },
        Step {
            title: "Weather cases",
            done: model.sources.weather.is_some(),
            detail: if model.sources.weather.is_some() {
                count(model.cases.len(), "weather case", "weather cases")
            } else {
                "only still air at 15 °C: add a weather case file (.ohlwc)".into()
            },
            go: Go::Library(LibraryKind::WeatherCases),
            link: "Open the weather cases library",
            optional: false,
        },
        Step {
            title: "Load cases",
            done: model.sources.load_cases.is_some(),
            detail: if model.sources.load_cases.is_some() {
                "for the structure loads (Checks › Compute structure loads)".into()
            } else {
                "none: the structure loads need a load case file (.ohllc)".into()
            },
            go: Go::Library(LibraryKind::LoadCases),
            link: "Open the load cases library",
            optional: false,
        },
        Step {
            title: "Clearance rules",
            done: rules,
            detail: if rules {
                "the checks have rules".into()
            } else {
                "the distances to keep to the ground: a first rule to set".into()
            },
            go: Go::VerticalClearances,
            link: "Vertical clearances…",
            optional: false,
        },
    ];
    // Nothing to do before there is a line to edit.
    if model.can_edit().is_err() {
        for s in &mut out {
            s.detail = format!("{} (read-only)", s.detail);
        }
    }
    out
}

/// Whether the project has clearance rules: a rule file of its own, or
/// a ground clearance window's rules.
pub fn has_rules(r: &Ready) -> bool {
    use ohl_clearance::zone::Measure;
    let set = |w: &crate::clearance_zones::ZoneWindow| {
        w.rules.iter().any(|x| !crate::clearance_zones::unset(x))
    };
    !r.clearances.files.is_empty()
        || set(&r.vertical_clearances)
        || set(&r.radial_clearances)
        || [Measure::Vertical, Measure::Radial]
            .into_iter()
            .any(|m| crate::clearance_zones::ZoneWindow::project_file(&r.model, m).is_file())
}

/// Follows a step's link.
pub fn go(r: &mut Ready, go: Go) {
    match go {
        Go::FirstSteps => {
            if r.model.line.routes.is_empty() {
                r.first_steps = Some(("route 1".into(), String::new()));
            } else {
                r.open_page = Some((Tab::Table(crate::tables::TableKind::Routes), None));
            }
        }
        Go::Hold(t) => {
            r.hotbar.hold(t);
            r.status = Some((format!("{} held: {}", t.name(), t.help()), false));
            r.open_page = Some((Tab::View3d, None));
        }
        Go::Library(k) => r.open_page = Some((Tab::Library(k), None)),
        Go::Surfaces => r.surfaces.show(&r.model),
        Go::PylonBuilder => {
            r.pylon_builder = Some(crate::pylon_builder::PylonBuilder::new(&r.model));
        }
        Go::Placement => {
            r.placement = Some(crate::pylon_builder::Placement::new(&r.model));
        }
        Go::VerticalClearances => r.vertical_clearances.show(&r.model),
    }
}

/// The page.
pub fn ui(ui: &mut egui::Ui, r: &mut Ready) {
    let list = steps(&r.model, has_rules(r));
    let next = list.iter().position(|s| !s.done && !s.optional);
    ui.label(
        egui::RichText::new(
            "What the line still needs, in the order of the work. Each link goes where the \
             step is done.",
        )
        .small()
        .weak(),
    );
    ui.add_space(4.0);
    let mut chosen = None;
    egui::Grid::new("next-steps")
        .num_columns(3)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            for (k, s) in list.iter().enumerate() {
                let (mark, color) = if s.done {
                    ("✔", crate::menu::ok_color(ui))
                } else {
                    ("○", ui.visuals().weak_text_color())
                };
                ui.colored_label(color, mark);
                let mut title = egui::RichText::new(s.title);
                if Some(k) == next {
                    title = title.strong();
                }
                if s.optional {
                    title = title.italics();
                }
                ui.vertical(|ui| {
                    ui.label(title);
                    ui.label(egui::RichText::new(&s.detail).small().weak());
                });
                let link = if Some(k) == next {
                    egui::RichText::new(format!("Next: {}", s.link)).strong()
                } else {
                    egui::RichText::new(s.link)
                };
                if ui.link(link).clicked() {
                    chosen = Some(s.go);
                }
                ui.end_row();
            }
        });
    if next.is_none() {
        ui.add_space(6.0);
        ui.label("Every step is done: check the line (Checks menu) and save it.");
    }
    if let Some(g) = chosen {
        go(r, g);
    }
}
