// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The windows of the alignment tools: the coordinates of a point placed
//! where no survey point, alignment point or structure is near (E, N and
//! height required), the route(s) a new alignment belongs to (Enter), and
//! the routes dropdown of tick boxes with "+ New route…" used wherever
//! routes are chosen.

use eframe::egui;
use ols_line::Line;

/// What a point typed in the coordinate window is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordFor {
    /// The next point of the alignment being laid out.
    NewAlignment,
    /// A point inserted into the nearest alignment.
    AddAlignment,
}

/// The coordinate window: where nothing to snap to was right-clicked.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordDialog {
    /// What the point is for.
    pub purpose: CoordFor,
    /// East, north, height, as typed.
    pub text: [String; 3],
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
        egui::Window::new("Point coordinates")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "No survey point, alignment point or structure near where you clicked: \
                         give the point's coordinates (all three required).",
                    )
                    .small(),
                );
                egui::Grid::new("coord-dialog")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (k, name) in ["E (m)", "N (m)", "height (m)"].into_iter().enumerate() {
                            ui.label(name);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.text[k]).desired_width(140.0),
                            );
                            ui.end_row();
                        }
                    });
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

/// Enter after laying out a new alignment: the route(s) it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteChoice {
    /// Its points.
    pub points: Vec<ols_line::AlignmentPoint>,
    /// The routes ticked.
    pub routes: Vec<String>,
    /// A new route's name being typed (empty: none).
    pub new_route: String,
}

/// What the route window asks for.
pub enum RouteChoiceAction {
    /// Make the alignment on these routes, a new route first when named.
    Make {
        /// Routes ticked.
        routes: Vec<String>,
        /// A new route to make (and tick).
        new_route: Option<String>,
    },
    /// Back to laying out the points.
    Back,
}

impl RouteChoice {
    /// For `points`: the routes of the alignments they meet (junctions)
    /// ticked, else the line's only route.
    pub fn new(line: &Line, points: Vec<ols_line::AlignmentPoint>) -> Self {
        let mut routes: Vec<String> = Vec::new();
        for p in &points {
            for a in &line.alignments {
                if a.points.iter().any(|q| {
                    (q.position[0] - p.position[0]).hypot(q.position[1] - p.position[1]) < 1e-3
                }) {
                    for r in &a.routes {
                        if !routes.contains(r) {
                            routes.push(r.clone());
                        }
                    }
                }
            }
        }
        if routes.is_empty()
            && let [only] = &line.routes[..]
        {
            routes.push(only.id.clone());
        }
        Self {
            points,
            routes,
            new_route: String::new(),
        }
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, line: &Line) -> Option<RouteChoiceAction> {
        let mut out = None;
        let mut open = true;
        egui::Window::new("New alignment: its routes")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("{} point(s). It belongs to:", self.points.len()));
                for r in &line.routes {
                    let mut on = self.routes.contains(&r.id);
                    let text = if r.name.is_empty() || r.name == r.id {
                        r.id.clone()
                    } else {
                        format!("{} — {}", r.id, r.name)
                    };
                    if ui.checkbox(&mut on, text).changed() {
                        if on {
                            self.routes.push(r.id.clone());
                        } else {
                            self.routes.retain(|x| *x != r.id);
                        }
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("+ New route");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_route)
                            .desired_width(160.0)
                            .hint_text("its name (optional)"),
                    );
                });
                let new = self.new_route.trim();
                let taken = line.routes.iter().any(|r| r.id == new);
                if taken {
                    crate::menu::error_label(
                        ui,
                        egui::RichText::new(format!("there is a route {new} already: tick it"))
                            .small(),
                    );
                }
                let ok = !taken && (!self.routes.is_empty() || !new.is_empty());
                if !ok && !taken {
                    ui.label(
                        egui::RichText::new("Tick a route, or name a new one.")
                            .small()
                            .weak(),
                    );
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(ok, egui::Button::new("Make the alignment"))
                        .clicked()
                    {
                        out = Some(RouteChoiceAction::Make {
                            routes: self.routes.clone(),
                            new_route: (!new.is_empty()).then(|| new.to_owned()),
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
