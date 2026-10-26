// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The windows of the alignment tools: the coordinates of a point placed
//! where no survey point, alignment point or structure is near (E, N and
//! height required), the route a new alignment belongs to (Enter; a route
//! is offered to be made when there is none), and the routes dropdown of
//! tick boxes with "+ New route…" used wherever routes are chosen.

use eframe::egui;
use ohl_line::Line;

/// What a point typed in the coordinate window is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordFor {
    /// The next point of the alignment being laid out.
    NewAlignment,
    /// A new point (Add alignment point): on no alignment, or put
    /// between the two closest points of the nearest alignment.
    AddAlignment,
    /// A point of the Edit alignment tool's new stretch.
    Reroute,
}

/// The coordinate window: where nothing to snap to was clicked.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordDialog {
    /// What the point is for.
    pub purpose: CoordFor,
    /// East, north, height, as typed.
    pub text: [String; 3],
    /// Add alignment point: put between the two closest points of the
    /// nearest alignment (unticked: on no alignment).
    pub connect: bool,
    /// Tied under this structure (snapped onto it); untied, the
    /// coordinates can be typed.
    pub structure: Option<String>,
}

impl CoordDialog {
    /// For a click at `at` (the ground there, when known): its E and N
    /// offered, the height of the ground there too — to be confirmed.
    pub fn new(purpose: CoordFor, at: Option<[f64; 3]>) -> Self {
        let f = |v: f64| format!("{v:.3}");
        Self {
            purpose,
            text: match at {
                Some(p) => [f(p[0]), f(p[1]), f(p[2])],
                None => Default::default(),
            },
            connect: false,
            structure: None,
        }
    }

    /// The point, or why not (each value required, a number).
    pub fn point(&self) -> Result<[f64; 3], String> {
        let mut out = [0.0; 3];
        for (k, name) in ["E", "N", "height"].into_iter().enumerate() {
            let t = self.text[k].trim().replace('\'', "");
            if t.is_empty() {
                return Err(format!("{name} is required"));
            }
            out[k] = t
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("{name}: \"{t}\" is not a number"))?;
        }
        Ok(out)
    }

    /// The window; the point when applied, `Err(())` when cancelled.
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<Result<[f64; 3], ()>> {
        let mut out = None;
        let mut open = true;
        let add = self.purpose == CoordFor::AddAlignment;
        egui::Window::new(if add {
            "New alignment point"
        } else {
            "Point coordinates"
        })
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(if add {
                    "Its coordinates (all three required; a structure, survey or LiDAR \
                         point clicked gives them)."
                } else {
                    "No survey point, alignment point or structure near where you clicked: \
                         give the point's coordinates (all three required)."
                })
                .small(),
            );
            if let Some(st) = self.structure.clone() {
                let mut tied = true;
                ui.checkbox(&mut tied, format!("tied under {st} (it follows it)"));
                if !tied {
                    self.structure = None;
                }
            }
            let typed = self.structure.is_none();
            egui::Grid::new("coord-dialog")
                .num_columns(2)
                .show(ui, |ui| {
                    for (k, name) in ["E (m)", "N (m)", "height (m)"].into_iter().enumerate() {
                        ui.label(name);
                        ui.add_enabled(
                            typed,
                            egui::TextEdit::singleline(&mut self.text[k]).desired_width(140.0),
                        );
                        ui.end_row();
                    }
                });
            if add {
                ui.checkbox(
                    &mut self.connect,
                    "connect it to the nearest alignment (between its two closest points)",
                );
            }
            let point = self.point();
            if let Err(e) = &point {
                crate::menu::error_label(ui, egui::RichText::new(e).small());
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(point.is_ok(), egui::Button::new("Add the point"))
                    .clicked()
                    && let Ok(p) = point
                {
                    out = Some(Ok(p));
                }
                if ui.button("Cancel").clicked() {
                    out = Some(Err(()));
                }
            });
        });
        if !open {
            out = Some(Err(()));
        }
        out
    }
}

/// Enter after laying out a new alignment: the route it belongs to (one;
/// a new one made first when there is none, or when asked).
#[derive(Debug, Clone, PartialEq)]
pub struct RouteChoice {
    /// Its points: existing ones by id (shared), new ones with an empty id.
    pub points: Vec<ohl_line::AlignmentPoint>,
    /// The route chosen (empty: none yet).
    pub route: String,
    /// A new route's name being typed (empty: none).
    pub new_route: String,
}

/// What the route window asks for.
pub enum RouteChoiceAction {
    /// Make the alignment on this route, made first when `new`.
    Make {
        /// The route.
        route: String,
        /// It is a new route to make.
        new: bool,
    },
    /// Back to laying out the points.
    Back,
}

impl RouteChoice {
    /// For `points`: the route worked on, else the route of the
    /// alignments its existing points are on (one route), else the
    /// line's only route; with no route at all, a new one is proposed.
    pub fn new(line: &Line, points: Vec<ohl_line::AlignmentPoint>, work: Option<&str>) -> Self {
        let mut routes: Vec<String> = points
            .iter()
            .filter(|p| !p.id.is_empty())
            .flat_map(|p| line.point_routes(&p.id))
            .collect();
        routes.sort();
        routes.dedup();
        let route = match (work, &routes[..], &line.routes[..]) {
            (Some(w), _, _) if line.routes.iter().any(|r| r.id == w) => w.to_owned(),
            (_, [one], _) => one.clone(),
            (_, _, [only]) => only.id.clone(),
            _ => String::new(),
        };
        let new_route = if line.routes.is_empty() {
            crate::editors::free_route_id(line)
        } else {
            String::new()
        };
        Self {
            points,
            route,
            new_route,
        }
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, line: &Line) -> Option<RouteChoiceAction> {
        let mut out = None;
        let mut open = true;
        egui::Window::new("New alignment: its route")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let shared = self.points.iter().filter(|p| !p.id.is_empty()).count();
                ui.label(format!(
                    "{} point(s){}. An alignment belongs to one route.",
                    self.points.len(),
                    if shared > 0 {
                        format!(", {shared} of them existing (shared)")
                    } else {
                        String::new()
                    }
                ));
                if line.routes.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "There is no route yet: a line is a route first, then its \
                             alignments. Create one for this alignment:",
                        )
                        .small(),
                    );
                } else {
                    for r in &line.routes {
                        let text = if r.name.is_empty() || r.name == r.id {
                            r.id.clone()
                        } else {
                            format!("{} — {}", r.id, r.name)
                        };
                        if ui
                            .radio(self.route == r.id && self.new_route.trim().is_empty(), text)
                            .clicked()
                        {
                            self.route = r.id.clone();
                            self.new_route.clear();
                        }
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("+ New route");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_route)
                            .desired_width(160.0)
                            .hint_text("its name"),
                    );
                });
                let new = self.new_route.trim();
                let taken = line.routes.iter().any(|r| r.id == new);
                if taken {
                    crate::menu::error_label(
                        ui,
                        egui::RichText::new(format!("there is a route {new} already: choose it"))
                            .small(),
                    );
                }
                let known = line.routes.iter().any(|r| r.id == self.route);
                let ok = !taken && (!new.is_empty() || known);
                if !ok && !taken {
                    ui.label(
                        egui::RichText::new("Choose its route, or name a new one.")
                            .small()
                            .weak(),
                    );
                }
                ui.horizontal(|ui| {
                    let label = if new.is_empty() {
                        "Make the alignment"
                    } else {
                        "Create the route and the alignment"
                    };
                    if ui.add_enabled(ok, egui::Button::new(label)).clicked() {
                        out = Some(if new.is_empty() {
                            RouteChoiceAction::Make {
                                route: self.route.clone(),
                                new: false,
                            }
                        } else {
                            RouteChoiceAction::Make {
                                route: new.to_owned(),
                                new: true,
                            }
                        });
                    }
                    if ui
                        .button("Back")
                        .on_hover_text("Go on laying out its points")
                        .clicked()
                    {
                        out = Some(RouteChoiceAction::Back);
                    }
                });
            });
        if !open {
            out = Some(RouteChoiceAction::Back);
        }
        out
    }
}

/// The routes dropdown: a tick box per route of `line` and "+ New
/// route…" (its name typed in `new_route`; Enter or the button gives it).
/// Whether a tick changed; a new route asked for in `make`.
pub fn route_ticks(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    line: &Line,
    routes: &mut Vec<String>,
    new_route: &mut String,
    make: &mut Option<String>,
) -> bool {
    let mut changed = false;
    let text = if routes.is_empty() {
        "(none)".to_owned()
    } else {
        routes.join(", ")
    };
    // Ticks keep it open.
    let config = egui::containers::menu::MenuConfig::new()
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    egui::containers::menu::MenuButton::new(text)
        .config(config)
        .ui(ui, |ui| {
            for r in &line.routes {
                let mut on = routes.contains(&r.id);
                if ui.checkbox(&mut on, &r.id).changed() {
                    if on {
                        routes.push(r.id.clone());
                    } else {
                        routes.retain(|x| *x != r.id);
                    }
                    changed = true;
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(new_route)
                        .id_salt(("new-route", &id))
                        .desired_width(120.0)
                        .hint_text("+ New route…"),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let name = new_route.trim().to_owned();
                let ok = !name.is_empty() && !line.routes.iter().any(|x| x.id == name);
                if (ui.add_enabled(ok, egui::Button::new("Add")).clicked() || (enter && ok)) && ok {
                    *make = Some(name.clone());
                    if !routes.contains(&name) {
                        routes.push(name);
                    }
                    new_route.clear();
                    changed = true;
                }
            });
        });
    changed
}
