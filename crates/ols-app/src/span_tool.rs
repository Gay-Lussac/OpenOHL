//! Objects on a span: span points, point loads and spread loads (the span
//! tool's window, placed where the span was clicked), and interphase
//! spacers between two span points (the add-string tool's window).

use crate::model::Model;
use eframe::egui;
use ols_line::{Spacer, SpanLoad, SpanPlace, SpanPoint, SpanPointRef, SpreadLoad};
use ols_session::{Command, Kind};

/// What the span tool adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanWhat {
    /// A span point (where a spacer is clamped).
    Point,
    /// A point load (a marker ball, a weight…).
    Load,
    /// A load spread between two places, N/m.
    Spread,
}

/// How places are given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceBy {
    /// A fraction of the span's horizontal length.
    Fraction,
    /// A horizontal distance from its back structure, m.
    Distance,
    /// A station of the line, m.
    Station,
}

/// The span tool's window: what to add on span `span` of section
/// `section`, where.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanForm {
    /// Section id.
    pub section: String,
    /// Span index.
    pub span: usize,
    /// The span's horizontal length, m (as drawn).
    pub horizontal: f64,
    /// Its structures' stations, if they stand on one alignment.
    pub stations: Option<(f64, f64)>,
    /// What is added.
    pub what: SpanWhat,
    /// How its places are written.
    pub by: PlaceBy,
    /// Where (the start of a spread load, the first of regular ones): m
    /// from the back structure.
    pub from: f64,
    /// The end of a spread load, the last of regular ones, m.
    pub to: f64,
    /// Repeated every `spacing` m from `from` to `to` (points, loads).
    pub regular: bool,
    /// The spacing, m.
    pub spacing: f64,
    /// Label (a number is added to repeated ones).
    pub label: String,
    /// A point load's values.
    pub load: SpanLoad,
    /// A spread load, N/m.
    pub per_metre: f64,
}

impl SpanForm {
    /// On span `span` of `section`, at the point `on` of its wire (where
    /// it was clicked; none: its middle).
    pub fn new(
        model: &Model,
        case: usize,
        section: &str,
        span: usize,
        on: Option<[f64; 3]>,
    ) -> Option<Self> {
        let line = &model.line;
        let s = line.sections.iter().find(|s| s.id == section)?;
        let w = s.points.get(span..=span + 1)?;
        let points = &model.shape(section, case)?.spans.get(span)?.points;
        let horizontal = crate::scene::plan_length(points);
        let fraction = on
            .and_then(|p| {
                let (a, b) = (points.first()?, points.last()?);
                let c = [b[0] - a[0], b[1] - a[1]];
                let l2 = c[0] * c[0] + c[1] * c[1];
                (l2 > 0.0)
                    .then(|| (((p[0] - a[0]) * c[0] + (p[1] - a[1]) * c[1]) / l2).clamp(0.0, 1.0))
            })
            .unwrap_or(0.5);
        let from = round(fraction * horizontal, 0.1);
        let label = next_label(
            &s.span_points
                .iter()
                .map(|p| p.label.clone())
                .collect::<Vec<_>>(),
            "P",
        );
        Some(Self {
            section: section.to_owned(),
            span,
            horizontal,
            stations: line.span_stations(&w[0].structure, &w[1].structure),
            what: SpanWhat::Point,
            by: PlaceBy::Distance,
            from,
            to: (from + 10.0).min(horizontal),
            regular: false,
            spacing: 50.0,
            label,
            load: SpanLoad {
                span,
                vertical: 100.0,
                label: "marker ball".into(),
                diameter: 0.6,
                ..SpanLoad::default()
            },
            per_metre: 10.0,
        })
    }

    /// A place `d` m from the back structure, written the chosen way.
    fn place(&self, d: f64) -> SpanPlace {
        let fraction = if self.horizontal > 0.0 {
            (d / self.horizontal).clamp(0.0, 1.0)
        } else {
            0.0
        };
        match (self.by, self.stations) {
            (PlaceBy::Station, Some((a, b))) => SpanPlace {
                fraction,
                station: Some(round(a + fraction * (b - a), 0.001)),
                ..SpanPlace::default()
            },
            (PlaceBy::Distance, _) => SpanPlace {
                fraction,
                distance: Some(d),
                ..SpanPlace::default()
            },
            _ => SpanPlace::at_fraction(fraction),
        }
    }

    /// The places, m from the back structure.
    pub fn distances(&self) -> Vec<f64> {
        if !(self.regular && self.what != SpanWhat::Spread) {
            return vec![self.from];
        }
        let mut out = Vec::new();
        let mut d = self.from;
        while d <= self.to + 1e-6 && out.len() < 1000 && self.spacing > 0.0 {
            out.push(d);
            d += self.spacing;
        }
        out
    }

    /// The command putting the section with what is added, in words.
    pub fn command(&self, model: &Model) -> Result<(String, Command), String> {
        let stored = model.stored_line().unwrap_or(&model.line);
        let mut s = stored
            .sections
            .iter()
            .find(|s| s.id == self.section)
            .ok_or_else(|| format!("section {} is not in the line", self.section))?
            .clone();
        let inside = |d: f64| (-1e-6..=self.horizontal + 1e-6).contains(&d);
        let ds = self.distances();
        if ds.is_empty() {
            return Err("no place: the first is beyond the last".into());
        }
        if let Some(d) = ds.iter().chain([&self.to]).find(|d| !inside(**d))
            && (self.what == SpanWhat::Spread || ds.contains(d))
        {
            return Err(format!(
                "{d} m is off the span (0 to {:.1} m)",
                self.horizontal
            ));
        }
        let many = ds.len() > 1;
        let what = match self.what {
            SpanWhat::Point => {
                let mut taken: Vec<String> =
                    s.span_points.iter().map(|p| p.label.clone()).collect();
                let mut labels = Vec::new();
                for (k, d) in ds.iter().enumerate() {
                    let label = if many {
                        let l = format!("{}{}", self.label, k + 1);
                        if taken.contains(&l) {
                            next_label(&taken, &self.label)
                        } else {
                            l
                        }
                    } else {
                        self.label.clone()
                    };
                    if label.is_empty() || taken.contains(&label) {
                        return Err(format!("the section has a span point {label:?} already"));
                    }
                    taken.push(label.clone());
                    labels.push(label.clone());
                    s.span_points.push(SpanPoint {
                        label,
                        span: self.span,
                        at: self.place(*d),
                        description: String::new(),
                    });
                }
                format!("span points {} on section {}", labels.join(", "), s.id)
            }
            SpanWhat::Load => {
                for (k, d) in ds.iter().enumerate() {
                    let p = self.place(*d);
                    s.loads.push(SpanLoad {
                        span: self.span,
                        fraction: p.fraction,
                        distance: p.distance,
                        station: p.station,
                        label: if many {
                            format!("{} {}", self.load.label, k + 1)
                        } else {
                            self.load.label.clone()
                        },
                        ..self.load.clone()
                    });
                }
                format!("{} point loads on section {}", ds.len(), s.id)
            }
            SpanWhat::Spread => {
                if self.to <= self.from {
                    return Err("a spread load ends after it starts".into());
                }
                s.spread_loads.push(SpreadLoad {
                    span: self.span,
                    start: self.place(self.from),
                    end: self.place(self.to),
                    load: self.per_metre,
                    label: self.label.clone(),
                });
                format!(
                    "a spread load of {} N/m on section {}",
                    self.per_metre, s.id
                )
            }
        };
        Ok((
            format!("added {what}"),
            Command::Put {
                kind: Kind::Section,
                id: s.id.clone(),
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

fn round(x: f64, step: f64) -> f64 {
    (x / step).round() * step
}

/// `prefix` and the first number not in `taken`.
fn next_label(taken: &[String], prefix: &str) -> String {
    (1..)
        .map(|k| format!("{prefix}{k}"))
        .find(|l| !taken.contains(l))
        .expect("a free label")
}

/// A distance (m from the back structure) shown and edited the way
/// places are given.
fn place_value(ui: &mut egui::Ui, f: &SpanForm, d: &mut f64) {
    let h = f.horizontal.max(1e-9);
    match (f.by, f.stations) {
        (PlaceBy::Fraction, _) => {
            let mut x = *d / h;
            if ui
                .add(
                    egui::DragValue::new(&mut x)
                        .speed(0.001)
                        .range(0.0..=1.0)
                        .max_decimals(4),
                )
                .changed()
            {
                *d = x * h;
            }
        }
        (PlaceBy::Station, Some((a, b))) => {
            let (lo, hi) = (a.min(b), a.max(b));
            let mut x = a + *d / h * (b - a);
            if ui
                .add(
                    egui::DragValue::new(&mut x)
                        .speed(0.1)
                        .range(lo..=hi)
                        .max_decimals(3)
                        .prefix("st. ")
                        .suffix(" m"),
                )
                .changed()
                && (b - a).abs() > 1e-9
            {
                *d = (x - a) / (b - a) * h;
            }
        }
        _ => {
            ui.add(
                egui::DragValue::new(d)
                    .speed(0.1)
                    .range(0.0..=f.horizontal)
                    .max_decimals(3)
                    .suffix(" m"),
            );
        }
    }
}

/// The span tool's window.
pub fn span_ui(ui: &mut egui::Ui, f: &mut SpanForm) {
    ui.label(format!(
        "Span {} of section {}: {:.1} m horizontally{}",
        f.span + 1,
        f.section,
        f.horizontal,
        f.stations.map_or(String::new(), |(a, b)| format!(
            ", stations {a:.1} to {b:.1} m"
        ))
    ));
    ui.horizontal(|ui| {
        ui.radio_value(&mut f.what, SpanWhat::Point, "span point")
            .on_hover_text("A point on the wire: a spacer is clamped there (Add string, between two span points)");
        ui.radio_value(&mut f.what, SpanWhat::Load, "point load")
            .on_hover_text("A body on the wire: a marker ball, a weight");
        ui.radio_value(&mut f.what, SpanWhat::Spread, "spread load")
            .on_hover_text("A load along a stretch of the wire, N per metre");
    });
    ui.horizontal(|ui| {
        ui.label("place by");
        ui.radio_value(&mut f.by, PlaceBy::Fraction, "fraction");
        ui.radio_value(&mut f.by, PlaceBy::Distance, "distance");
        ui.add_enabled_ui(f.stations.is_some(), |ui| {
            ui.radio_value(&mut f.by, PlaceBy::Station, "station")
        })
        .response
        .on_disabled_hover_text("The span's structures stand on no common alignment");
    });
    if f.what != SpanWhat::Spread {
        ui.checkbox(&mut f.regular, "regular spacing")
            .on_hover_text("Repeated from the first place to the last, every spacing");
    }
    let mut from = f.from;
    let mut to = f.to;
    egui::Grid::new("span-form").num_columns(2).show(ui, |ui| {
        let two = f.what == SpanWhat::Spread || f.regular;
        ui.label(match (f.what, two) {
            (SpanWhat::Spread, _) => "start",
            (_, true) => "first",
            _ => "at",
        });
        place_value(ui, f, &mut from);
        ui.end_row();
        if two {
            ui.label(if f.what == SpanWhat::Spread {
                "end"
            } else {
                "last"
            });
            place_value(ui, f, &mut to);
            ui.end_row();
        }
        if f.regular && f.what != SpanWhat::Spread {
            ui.label("spacing");
            ui.add(
                egui::DragValue::new(&mut f.spacing)
                    .speed(0.1)
                    .range(0.1..=f64::MAX)
                    .suffix(" m"),
            );
            ui.end_row();
        }
        match f.what {
            SpanWhat::Point => {
                ui.label("label");
                ui.text_edit_singleline(&mut f.label);
                ui.end_row();
            }
            SpanWhat::Load => {
                ui.label("label");
                ui.text_edit_singleline(&mut f.load.label);
                ui.end_row();
                ui.label("weight");
                ui.add(
                    egui::DragValue::new(&mut f.load.vertical)
                        .speed(1.0)
                        .suffix(" N"),
                );
                ui.end_row();
                ui.label("diameter");
                ui.add(
                    egui::DragValue::new(&mut f.load.diameter)
                        .speed(0.01)
                        .range(0.0..=10.0)
                        .suffix(" m"),
                )
                .on_hover_text(
                    "Taken as a sphere: its wind area and its surface for the ice (0: a point)",
                );
                ui.end_row();
            }
            SpanWhat::Spread => {
                ui.label("label");
                ui.text_edit_singleline(&mut f.label);
                ui.end_row();
                ui.label("load");
                ui.add(
                    egui::DragValue::new(&mut f.per_metre)
                        .speed(0.1)
                        .suffix(" N/m"),
                )
                .on_hover_text("Downward, per metre of conductor, on the whole bundle");
                ui.end_row();
            }
        }
    });
    (f.from, f.to) = (from, to);
    if f.regular && f.what != SpanWhat::Spread {
        ui.label(format!("{} places", f.distances().len()));
    }
}

/// The add-string tool's window between two span points: a new spacer.
#[derive(Debug, Clone, PartialEq)]
pub struct SpacerForm {
    /// The spacer.
    pub spacer: Spacer,
    /// Its length given (else as strung).
    pub given_length: bool,
    /// A new one (else spacer `spacer.id` as stored).
    pub new: bool,
}

impl SpacerForm {
    /// A new spacer from `a` to `b`, with the project's default
    /// compression limit.
    pub fn new(model: &Model, a: SpanPointRef, b: SpanPointRef) -> Self {
        let taken: Vec<String> = model.line.spacers.iter().map(|s| s.id.clone()).collect();
        Self {
            spacer: Spacer {
                id: next_label(&taken, "SP"),
                a,
                b,
                compression: Some(model.line.defaults.spacer_compression),
                stiffness: Some(ols_line::solve::DEFAULT_INSULATOR_STIFFNESS),
                ..Spacer::default()
            },
            given_length: false,
            new: true,
        }
    }

    /// Spacer `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let stored = model.stored_line().unwrap_or(&model.line);
        let spacer = stored.spacers.iter().find(|s| s.id == id)?.clone();
        Some(Self {
            given_length: spacer.length.is_some(),
            spacer,
            new: false,
        })
    }

    /// The command adding it, in words.
    pub fn command(&self, model: &Model) -> Result<(String, Command), String> {
        let s = &self.spacer;
        if s.id.is_empty() {
            return Err("a spacer needs an id".into());
        }
        if self.new && model.line.spacers.iter().any(|x| x.id == s.id) {
            return Err(format!("there is a spacer {} already", s.id));
        }
        Ok((
            format!(
                "{} spacer {}",
                if self.new { "added" } else { "changed" },
                s.id
            ),
            Command::Put {
                kind: Kind::Spacer,
                id: s.id.clone(),
                value: serde_json::to_value(s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

/// The spacer window.
pub fn spacer_ui(ui: &mut egui::Ui, f: &mut SpacerForm, model: &Model) {
    let s = &mut f.spacer;
    let end = |e: &SpanPointRef| format!("{} span point {}", e.section, e.point);
    egui::Grid::new("spacer-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("ends");
            ui.label(format!("{} → {}", end(&s.a), end(&s.b)));
            ui.end_row();
            ui.label("id");
            ui.add_enabled(f.new, egui::TextEdit::singleline(&mut s.id));
            ui.end_row();
            ui.label("description");
            ui.add(
                egui::TextEdit::singleline(&mut s.description).hint_text("what it is (free text)"),
            );
            ui.end_row();
            ui.label("insulator");
            let assemblies: Vec<&ols_structures::InsulatorAssembly> = model
                .types
                .as_ref()
                .map(|(_, lib)| lib.iter().collect())
                .unwrap_or_default();
            egui::ComboBox::from_id_salt("spacer-insulator")
                .selected_text(s.insulator.clone().unwrap_or_else(|| "own values".into()))
                .height(400.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(s.insulator.is_none(), "own values")
                        .clicked()
                    {
                        s.insulator = None;
                    }
                    for a in &assemblies {
                        if ui
                            .selectable_label(s.insulator.as_deref() == Some(&a.id), &a.name)
                            .clicked()
                        {
                            s.insulator = Some(a.id.clone());
                            s.weight = a.weight.si();
                            s.wind_area = a.wind_area.si();
                            s.stiffness = Some(a.axial_stiffness.si());
                            s.length = Some(a.length.si());
                            f.given_length = true;
                        }
                    }
                });
            ui.end_row();
            ui.label("length");
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut f.given_length, "given")
                    .on_hover_text(
                        "Else the distance between its ends where the wires hang at stringing",
                    )
                    .changed()
                {
                    s.length = f.given_length.then(|| s.length.unwrap_or(3.0));
                }
                if let Some(l) = &mut s.length {
                    ui.add(
                        egui::DragValue::new(l)
                            .speed(0.01)
                            .range(0.01..=100.0)
                            .suffix(" m"),
                    );
                }
            });
            ui.end_row();
            ui.label("weight");
            ui.add(
                egui::DragValue::new(&mut s.weight)
                    .speed(1.0)
                    .range(0.0..=f64::MAX)
                    .suffix(" N"),
            );
            ui.end_row();
            ui.label("wind area");
            ui.add(
                egui::DragValue::new(&mut s.wind_area)
                    .speed(0.01)
                    .range(0.0..=f64::MAX)
                    .suffix(" m²"),
            );
            ui.end_row();
            ui.label("stiffness EA");
            let mut ea = s
                .stiffness
                .unwrap_or(ols_line::solve::DEFAULT_INSULATOR_STIFFNESS);
            if ui
                .add(
                    egui::DragValue::new(&mut ea)
                        .speed(1e5)
                        .range(1.0..=f64::MAX)
                        .suffix(" N"),
                )
                .changed()
            {
                s.stiffness = Some(ea);
            }
            ui.end_row();
            ui.label("compression");
            ui.horizontal(|ui| {
                let mut takes = s.compression.is_some();
                if ui
                    .checkbox(&mut takes, "takes compression")
                    .on_hover_text("Beyond its limit it is flagged; unticked: tension only")
                    .changed()
                {
                    s.compression = takes.then_some(model.line.defaults.spacer_compression);
                }
                if let Some(c) = &mut s.compression {
                    ui.add(
                        egui::DragValue::new(c)
                            .speed(10.0)
                            .range(0.0..=f64::MAX)
                            .suffix(" N"),
                    );
                }
            });
            ui.end_row();
        });
}
