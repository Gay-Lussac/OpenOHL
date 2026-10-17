//! Objects on the wires: points on spans and jumpers with their object
//! (a body or a clamp, from the library's templates or own values) and
//! spread objects between two points (the span tool's window, placed
//! where the wire was clicked), and interphase spacers between two span
//! points (the add-string tool's window).

use crate::model::Model;
use eframe::egui;
use ols_line::{
    JumperPlace, JumperPoint, LineObject, ObjectKind, ObjectLibrary, Shape, Spacer, SpanPlace,
    SpanPoint, SpanPointRef, SpreadObject, TypeKind,
};
use ols_session::{Command, Kind};

/// What the span tool adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanWhat {
    /// A bare point (a spacer's end, a spread's end).
    Point,
    /// A point with its object (a body or a clamp).
    Load,
    /// A spread object between two new points (spans only).
    Spread,
}

/// How places are given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceBy {
    /// A fraction of the span's horizontal length (a jumper's length).
    Fraction,
    /// A horizontal distance from its back structure (a jumper's `from`
    /// clamp), m.
    Distance,
    /// A station of the line, m (spans).
    Station,
    /// A length along the jumper from `from`, m (jumpers).
    Length,
}

/// The tool's window: what to add on span `span` of section `section`
/// (or on jumper `jumper`), where.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanForm {
    /// Section id (a span).
    pub section: String,
    /// Span index.
    pub span: usize,
    /// On this jumper instead of a span.
    pub jumper: Option<String>,
    /// The span's (the jumper's ends') horizontal length, m (as drawn).
    pub horizontal: f64,
    /// A jumper's length, m.
    pub length: f64,
    /// Its structures' stations, if they stand on one alignment (spans).
    pub stations: Option<(f64, f64)>,
    /// What is added.
    pub what: SpanWhat,
    /// How its places are written.
    pub by: PlaceBy,
    /// Where (the start of a spread, the first of regular ones): m
    /// horizontally from the back structure (from the jumper's `from`).
    pub from: f64,
    /// The end of a spread, the last of regular ones, m.
    pub to: f64,
    /// Repeated every `spacing` m from `from` to `to` (points, objects).
    pub regular: bool,
    /// The spacing, m.
    pub spacing: f64,
    /// Label (a number is added to repeated ones).
    pub label: String,
    /// What the point is, in words.
    pub description: String,
    /// The object put on the point.
    pub object: LineObject,
    /// The advanced settings shown (shape, sizes, drag).
    pub advanced: bool,
    /// A spread's load, N/m.
    pub per_metre: f64,
    /// A spread's library type.
    pub spread_type: Option<String>,
    /// The types to choose from (the project's library).
    pub library: ObjectLibrary,
    /// An existing point changed (its label), instead of points added.
    pub editing: Option<String>,
}

impl SpanForm {
    fn base(model: &Model, horizontal: f64, from: f64, taken: &[String]) -> Self {
        let library = model.object_library();
        // The first body template to start from.
        let object = library
            .types
            .iter()
            .find(|t| t.kind == TypeKind::Body)
            .and_then(|t| t.object())
            .unwrap_or_else(|| LineObject::body(100.0, Shape::Sphere { diameter: 0.6 }));
        Self {
            section: String::new(),
            span: 0,
            jumper: None,
            horizontal,
            length: horizontal,
            stations: None,
            what: SpanWhat::Point,
            by: PlaceBy::Distance,
            from,
            to: (from + 10.0).min(horizontal),
            regular: false,
            spacing: 50.0,
            label: next_label(taken, "P"),
            description: String::new(),
            object,
            advanced: false,
            per_metre: 10.0,
            spread_type: None,
            library,
            editing: None,
        }
    }

    /// The window of existing point `label` of section `section`'s spans
    /// (`on_jumper`: of jumper `section`), as in `case`: its place, label,
    /// description and object.
    pub fn edit_point(
        model: &Model,
        case: usize,
        host: &str,
        on_jumper: bool,
        label: &str,
    ) -> Option<Self> {
        let line = &model.line;
        let (mut f, at_span, at_jumper, description, object) = if on_jumper {
            let j = line.jumpers.iter().find(|j| j.id == host)?;
            let p = j.points.iter().find(|p| p.label == label)?;
            let f = Self::on_jumper(model, case, host, None)?;
            (f, None, Some(p.at), p.description.clone(), p.object.clone())
        } else {
            let s = line.sections.iter().find(|s| s.id == host)?;
            let p = s.span_points.iter().find(|p| p.label == label)?;
            let f = Self::new(model, case, host, p.span, None)?;
            // A station as the fraction it is in this span.
            let resolved = line.section_with_fractions(s);
            let at = resolved
                .span_points
                .iter()
                .find(|q| q.label == label)
                .map_or(p.at, |q| q.at);
            let mut f = f;
            f.by = match p.at {
                SpanPlace::Fraction(_) => PlaceBy::Fraction,
                SpanPlace::Distance(_) => PlaceBy::Distance,
                SpanPlace::Station(_) if f.stations.is_some() => PlaceBy::Station,
                SpanPlace::Station(_) => PlaceBy::Fraction,
            };
            (f, Some(at), None, p.description.clone(), p.object.clone())
        };
        f.from = match (at_span, at_jumper) {
            (Some(at), _) => at.fraction_of(f.horizontal) * f.horizontal,
            (None, Some(at)) => {
                f.by = match at {
                    JumperPlace::Fraction(_) => PlaceBy::Fraction,
                    JumperPlace::Length(_) => PlaceBy::Length,
                    JumperPlace::Distance(_) => PlaceBy::Distance,
                };
                ols_line::jumper::place_fraction(&at, f.length, f.horizontal) * f.horizontal
            }
            _ => f.from,
        };
        f.to = f.from;
        f.label = label.to_owned();
        f.description = description;
        f.what = if object.is_some() {
            SpanWhat::Load
        } else {
            SpanWhat::Point
        };
        if let Some(o) = object {
            f.advanced = o.type_id.is_none();
            f.object = o;
        }
        f.editing = Some(label.to_owned());
        Some(f)
    }

    /// The change of the point being edited: its place, description and
    /// object put on the host, a new label followed by what names it
    /// ([`ols_line::Line::relabel_point`]).
    fn edit_command(&self, model: &Model, old: &str) -> Result<(String, Command), String> {
        let stored = model.stored_line().unwrap_or(&model.line);
        if !(-1e-6..=self.horizontal + 1e-6).contains(&self.from) {
            return Err(format!(
                "{} m is off the wire (0 to {:.1} m)",
                self.from, self.horizontal
            ));
        }
        if let Some(o) = self.object() {
            o.validate()?;
        }
        let mut line = stored.clone();
        let host = self.jumper.clone().unwrap_or_else(|| self.section.clone());
        match &self.jumper {
            Some(id) => {
                let p = line
                    .jumpers
                    .iter_mut()
                    .find(|j| &j.id == id)
                    .and_then(|j| j.points.iter_mut().find(|p| p.label == old))
                    .ok_or_else(|| format!("jumper {id} has no point {old}"))?;
                p.at = self.jumper_place(self.from);
                p.description = self.description.clone();
                // A support's hold stays with its clamp.
                let hold = p.object.as_ref().and_then(|o| o.hold.clone());
                p.object = self.object().map(|mut o| {
                    if o.kind == ols_line::ObjectKind::Clamp {
                        o.hold = hold;
                    }
                    o
                });
            }
            None => {
                let p = line
                    .sections
                    .iter_mut()
                    .find(|s| s.id == self.section)
                    .and_then(|s| s.span_points.iter_mut().find(|p| p.label == old))
                    .ok_or_else(|| format!("section {} has no point {old}", self.section))?;
                p.at = self.place(self.from);
                p.description = self.description.clone();
                p.object = self.object();
            }
        }
        let label = self.label.trim();
        if label != old {
            line.relabel_point(self.jumper.is_some(), &host, old, label)?;
        }
        // What changed: the host, and what names a relabelled point.
        let mut commands = Vec::new();
        for (a, b) in stored.sections.iter().zip(&line.sections) {
            if a != b {
                commands.push(put(Kind::Section, &b.id, b)?);
            }
        }
        for (a, b) in stored.jumpers.iter().zip(&line.jumpers) {
            if a != b {
                commands.push(put(Kind::Jumper, &b.id, b)?);
            }
        }
        for (a, b) in stored.spacers.iter().zip(&line.spacers) {
            if a != b {
                commands.push(put(Kind::Spacer, &b.id, b)?);
            }
        }
        if commands.is_empty() {
            return Err("nothing changed".into());
        }
        Ok((
            format!("changed point {old} of {host}"),
            match commands.len() {
                1 => commands.remove(0),
                _ => Command::Batch { commands },
            },
        ))
    }

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
        let from = round(chord_share(points, on) * horizontal, 0.1);
        let taken: Vec<String> = s.span_points.iter().map(|p| p.label.clone()).collect();
        Some(Self {
            section: section.to_owned(),
            span,
            stations: line.span_stations(&w[0].structure, &w[1].structure),
            ..Self::base(model, horizontal, from, &taken)
        })
    }

    /// On jumper `jumper`, at the point `on` of it (where it was
    /// clicked; none: its middle).
    pub fn on_jumper(
        model: &Model,
        case: usize,
        jumper: &str,
        on: Option<[f64; 3]>,
    ) -> Option<Self> {
        let j = model.line.jumpers.iter().find(|j| j.id == jumper)?;
        let shape = model.jumper_shape(jumper, case)?;
        let horizontal = crate::scene::plan_length(&shape.points);
        let from = round(chord_share(&shape.points, on) * horizontal, 0.1);
        let taken: Vec<String> = j.points.iter().map(|p| p.label.clone()).collect();
        let mut f = Self {
            jumper: Some(jumper.to_owned()),
            length: shape.unstressed_length,
            by: PlaceBy::Fraction,
            ..Self::base(model, horizontal, from, &taken)
        };
        f.from = from;
        Some(f)
    }

    /// A span place `d` m from the back structure, written the chosen
    /// way.
    fn place(&self, d: f64) -> SpanPlace {
        let fraction = if self.horizontal > 0.0 {
            (d / self.horizontal).clamp(0.0, 1.0)
        } else {
            0.0
        };
        match (self.by, self.stations) {
            (PlaceBy::Station, Some((a, b))) => {
                SpanPlace::Station(round(a + fraction * (b - a), 0.001))
            }
            (PlaceBy::Distance, _) => SpanPlace::Distance(d),
            _ => SpanPlace::at_fraction(fraction),
        }
    }

    /// A jumper place `d` m (horizontally) from its `from` clamp.
    fn jumper_place(&self, d: f64) -> JumperPlace {
        let fraction = if self.horizontal > 0.0 {
            (d / self.horizontal).clamp(0.0, 1.0)
        } else {
            0.0
        };
        match self.by {
            PlaceBy::Distance => JumperPlace::Distance(round(d, 0.001)),
            PlaceBy::Length => JumperPlace::Length(round(fraction * self.length, 0.001)),
            _ => JumperPlace::Fraction(fraction),
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

    /// The labels of the points to add (`self.label`, numbered when
    /// several), free on the host.
    fn labels(&self, taken: &[String], n: usize) -> Result<Vec<String>, String> {
        let mut taken = taken.to_vec();
        let mut out = Vec::new();
        for k in 0..n {
            let label = if n > 1 {
                let l = format!("{}{}", self.label, k + 1);
                if taken.contains(&l) {
                    next_label(&taken, &self.label)
                } else {
                    l
                }
            } else {
                self.label.clone()
            };
            if label.trim().is_empty() || taken.contains(&label) {
                return Err(format!("there is a point {label:?} already"));
            }
            taken.push(label.clone());
            out.push(label);
        }
        Ok(out)
    }

    /// The object to put (none: a bare point).
    fn object(&self) -> Option<LineObject> {
        (self.what == SpanWhat::Load).then(|| self.object.clone())
    }

    /// The command putting the section (or the jumper) with what is
    /// added, in words.
    pub fn command(&self, model: &Model) -> Result<(String, Command), String> {
        if let Some(old) = &self.editing {
            return self.edit_command(model, old);
        }
        let stored = model.stored_line().unwrap_or(&model.line);
        let inside = |d: f64| (-1e-6..=self.horizontal + 1e-6).contains(&d);
        let ds = self.distances();
        if ds.is_empty() {
            return Err("no place: the first is beyond the last".into());
        }
        if let Some(d) = ds.iter().chain([&self.to]).find(|d| !inside(**d))
            && (self.what == SpanWhat::Spread || ds.contains(d))
        {
            return Err(format!(
                "{d} m is off the wire (0 to {:.1} m)",
                self.horizontal
            ));
        }
        if let Some(o) = self.object() {
            o.validate()?;
        }
        let what = if self.what == SpanWhat::Load {
            format!(
                "{} {}",
                self.object.kind.as_str(),
                self.object.type_id.clone().unwrap_or_default()
            )
        } else {
            "point".to_owned()
        };
        if let Some(id) = &self.jumper {
            if self.what == SpanWhat::Spread {
                return Err("spread objects lie on spans".into());
            }
            let mut j = stored
                .jumpers
                .iter()
                .find(|j| &j.id == id)
                .ok_or_else(|| format!("jumper {id} is not in the line"))?
                .clone();
            let taken: Vec<String> = j.points.iter().map(|p| p.label.clone()).collect();
            let labels = self.labels(&taken, ds.len())?;
            for (label, d) in labels.iter().zip(&ds) {
                j.points.push(JumperPoint {
                    label: label.clone(),
                    at: self.jumper_place(*d),
                    description: self.description.clone(),
                    object: self.object(),
                });
            }
            return Ok((
                format!("added {} ({what}) on jumper {}", labels.join(", "), j.id),
                Command::Put {
                    kind: Kind::Jumper,
                    id: j.id.clone(),
                    value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                    ord: None,
                },
            ));
        }
        let mut s = stored
            .sections
            .iter()
            .find(|s| s.id == self.section)
            .ok_or_else(|| format!("section {} is not in the line", self.section))?
            .clone();
        let taken: Vec<String> = s.span_points.iter().map(|p| p.label.clone()).collect();
        let done = if self.what == SpanWhat::Spread {
            if self.to <= self.from {
                return Err("a spread ends after it starts".into());
            }
            let ends = self.labels(&taken, 1)?;
            let a = ends[0].clone();
            let mut more = taken.clone();
            more.push(a.clone());
            let b = next_label(&more, &format!("{a}-"));
            for (label, d) in [(&a, self.from), (&b, self.to)] {
                s.span_points.push(SpanPoint {
                    label: label.clone(),
                    span: self.span,
                    at: self.place(d),
                    description: String::new(),
                    object: None,
                });
            }
            s.spreads.push(SpreadObject {
                label: self.description.clone(),
                from: a.clone(),
                to: b.clone(),
                type_id: self.spread_type.clone(),
                load: self.per_metre,
            });
            format!(
                "a spread of {} N/m from {a} to {b} on section {}",
                self.per_metre, s.id
            )
        } else {
            let labels = self.labels(&taken, ds.len())?;
            for (label, d) in labels.iter().zip(&ds) {
                s.span_points.push(SpanPoint {
                    label: label.clone(),
                    span: self.span,
                    at: self.place(*d),
                    description: self.description.clone(),
                    object: self.object(),
                });
            }
            format!("{} ({what}) on section {}", labels.join(", "), s.id)
        };
        Ok((
            format!("added {done}"),
            Command::Put {
                kind: Kind::Section,
                id: s.id.clone(),
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            },
        ))
    }
}

/// The share of a polyline's plan chord at the point `on` of it (none:
/// its middle).
fn chord_share(points: &[[f64; 3]], on: Option<[f64; 3]>) -> f64 {
    on.and_then(|p| {
        let (a, b) = (points.first()?, points.last()?);
        let c = [b[0] - a[0], b[1] - a[1]];
        let l2 = c[0] * c[0] + c[1] * c[1];
        (l2 > 0.0).then(|| (((p[0] - a[0]) * c[0] + (p[1] - a[1]) * c[1]) / l2).clamp(0.0, 1.0))
    })
    .unwrap_or(0.5)
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
    let drag = |ui: &mut egui::Ui,
                x: &mut f64,
                speed: f64,
                range: std::ops::RangeInclusive<f64>,
                prefix: &str,
                suffix: &str| {
        ui.add(
            egui::DragValue::new(x)
                .speed(speed)
                .range(range)
                .max_decimals(4)
                .prefix(prefix)
                .suffix(suffix),
        )
        .changed()
    };
    match (f.by, f.stations) {
        (PlaceBy::Fraction, _) => {
            let mut x = *d / h;
            if drag(ui, &mut x, 0.001, 0.0..=1.0, "", "") {
                *d = x * h;
            }
        }
        (PlaceBy::Length, _) => {
            let mut x = *d / h * f.length;
            if drag(ui, &mut x, 0.01, 0.0..=f.length, "", " m along") && f.length > 0.0 {
                *d = x / f.length * h;
            }
        }
        (PlaceBy::Station, Some((a, b))) => {
            let (lo, hi) = (a.min(b), a.max(b));
            let mut x = a + *d / h * (b - a);
            if drag(ui, &mut x, 0.1, lo..=hi, "st. ", " m") && (b - a).abs() > 1e-9 {
                *d = (x - a) / (b - a) * h;
            }
        }
        _ => {
            drag(ui, d, 0.1, 0.0..=f.horizontal, "", " m");
        }
    }
}

/// An object's values: a template of `library` or its own, the advanced
/// settings (kind, shape and sizes, drag, clamp length) when `advanced`.
pub fn object_ui(
    ui: &mut egui::Ui,
    id: &str,
    o: &mut LineObject,
    library: &ObjectLibrary,
    advanced: &mut bool,
) {
    egui::Grid::new((id, "object"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("template");
            let shown = o
                .type_id
                .as_ref()
                .and_then(|t| library.get(t))
                .map_or_else(|| "own values".to_owned(), |t| t.name.clone());
            egui::ComboBox::from_id_salt((id, "template"))
                .selected_text(shown)
                .height(400.0)
                .show_ui(ui, |ui| {
                    for t in library.types.iter().filter(|t| t.kind != TypeKind::Spread) {
                        if ui
                            .selectable_label(o.type_id.as_deref() == Some(t.id.as_str()), &t.name)
                            .on_hover_text(&t.description)
                            .clicked()
                            && let Some(x) = t.object()
                        {
                            *o = x;
                        }
                    }
                });
            ui.end_row();
            ui.label("weight");
            if ui
                .add(
                    egui::DragValue::new(&mut o.weight)
                        .speed(1.0)
                        .range(0.0..=f64::MAX)
                        .suffix(" N"),
                )
                .on_hover_text("The whole bundle's")
                .changed()
            {
                o.type_id = None;
            }
            ui.end_row();
        });
    ui.checkbox(advanced, "advanced: kind, shape and sizes, drag")
        .on_hover_text("More than the templates give: change the object's geometry");
    if !*advanced {
        return;
    }
    let before = o.clone();
    egui::Grid::new((id, "advanced"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("kind");
            ui.horizontal(|ui| {
                ui.radio_value(&mut o.kind, ObjectKind::Body, "body")
                    .on_hover_text("A body hanging on the wire: a ball, a light, a weight");
                ui.radio_value(&mut o.kind, ObjectKind::Clamp, "clamp")
                    .on_hover_text("Holds a jumper (a tap) or a spacer: a hinge, forces only");
            });
            ui.end_row();
            ui.label("shape");
            let names = ["point", "sphere", "cylinder", "box", "areas"];
            let mut k = names
                .iter()
                .position(|n| *n == o.shape.as_str())
                .unwrap_or(0);
            egui::ComboBox::from_id_salt((id, "shape"))
                .selected_text(names[k])
                .show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut k, i, *n);
                    }
                });
            if names[k] != o.shape.as_str() {
                o.shape = match k {
                    1 => Shape::Sphere { diameter: 0.6 },
                    2 => Shape::Cylinder {
                        diameter: 0.25,
                        length: 0.6,
                        axis: ols_line::Axis::Along,
                    },
                    3 => Shape::Box {
                        along: 0.3,
                        across: 0.2,
                        height: 0.4,
                    },
                    4 => Shape::Areas {
                        wind_area: 0.1,
                        ice_area: 0.2,
                    },
                    _ => Shape::Point,
                };
            }
            ui.end_row();
            let size = |ui: &mut egui::Ui, name: &str, x: &mut f64, unit: &str| {
                ui.label(name);
                ui.add(
                    egui::DragValue::new(x)
                        .speed(0.01)
                        .range(0.0..=100.0)
                        .suffix(unit),
                );
                ui.end_row();
            };
            match &mut o.shape {
                Shape::Point => {}
                Shape::Sphere { diameter } => size(ui, "diameter", diameter, " m"),
                Shape::Cylinder {
                    diameter,
                    length,
                    axis,
                } => {
                    size(ui, "diameter", diameter, " m");
                    size(ui, "length", length, " m");
                    ui.label("axis");
                    ui.horizontal(|ui| {
                        ui.radio_value(axis, ols_line::Axis::Along, "along the wire");
                        ui.radio_value(axis, ols_line::Axis::Across, "across the line");
                        ui.radio_value(axis, ols_line::Axis::Vertical, "vertical");
                    });
                    ui.end_row();
                }
                Shape::Box {
                    along,
                    across,
                    height,
                } => {
                    size(ui, "along", along, " m");
                    size(ui, "across", across, " m");
                    size(ui, "height", height, " m");
                }
                Shape::Areas {
                    wind_area,
                    ice_area,
                } => {
                    size(ui, "wind area", wind_area, " m²");
                    size(ui, "ice surface", ice_area, " m²");
                }
            }
            ui.label("drag");
            let mut drag = o.drag.unwrap_or(1.0);
            if ui
                .add(
                    egui::DragValue::new(&mut drag)
                        .speed(0.01)
                        .range(0.01..=5.0),
                )
                .on_hover_text("Drag coefficient (1: none given)")
                .changed()
            {
                o.drag = (drag != 1.0).then_some(drag);
            }
            ui.end_row();
            if o.kind == ObjectKind::Clamp {
                ui.label("clamp length");
                let mut l = o.clamp_length.unwrap_or(0.0);
                if ui
                    .add(
                        egui::DragValue::new(&mut l)
                            .speed(0.01)
                            .range(0.0..=5.0)
                            .suffix(" m"),
                    )
                    .changed()
                {
                    o.clamp_length = (l > 0.0).then_some(l);
                }
                ui.end_row();
            }
        });
    // Changed from its template: its own values now.
    if *o != before {
        o.type_id = None;
    }
}

/// The span tool's window.
pub fn span_ui(ui: &mut egui::Ui, f: &mut SpanForm) {
    match &f.jumper {
        Some(j) => ui.label(format!(
            "Jumper {j}: {:.1} m long, its ends {:.1} m apart horizontally",
            f.length, f.horizontal
        )),
        None => ui.label(format!(
            "Span {} of section {}: {:.1} m horizontally{}",
            f.span + 1,
            f.section,
            f.horizontal,
            f.stations.map_or(String::new(), |(a, b)| format!(
                ", stations {a:.1} to {b:.1} m"
            ))
        )),
    };
    let on_jumper = f.jumper.is_some();
    let editing = f.editing.is_some();
    ui.horizontal(|ui| {
        ui.radio_value(&mut f.what, SpanWhat::Point, "point")
            .on_hover_text("A bare point: a spacer's end, a spread's end (a clamp can be put on it later)");
        ui.radio_value(&mut f.what, SpanWhat::Load, "object")
            .on_hover_text("A point with its object: a body (a marker ball, a light, a weight) or a clamp (a tap for a jumper, a spacer's end)");
        if !editing {
            ui.add_enabled_ui(!on_jumper, |ui| {
                ui.radio_value(&mut f.what, SpanWhat::Spread, "spread")
            })
            .response
            .on_hover_text(
                "A load along a stretch of the wire, N per metre, between two new points",
            )
            .on_disabled_hover_text("Spread objects lie on spans");
        }
    });
    ui.horizontal(|ui| {
        ui.label("place by");
        ui.radio_value(&mut f.by, PlaceBy::Fraction, "fraction");
        ui.radio_value(&mut f.by, PlaceBy::Distance, "distance");
        if on_jumper {
            ui.radio_value(&mut f.by, PlaceBy::Length, "length along it");
        } else {
            ui.add_enabled_ui(f.stations.is_some(), |ui| {
                ui.radio_value(&mut f.by, PlaceBy::Station, "station")
            })
            .response
            .on_disabled_hover_text("The span's structures stand on no common alignment");
        }
    });
    if f.what != SpanWhat::Spread && !editing {
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
        ui.label("label");
        ui.text_edit_singleline(&mut f.label)
            .on_hover_text("The point's label (a spread: its first point's)");
        ui.end_row();
        ui.label("description");
        ui.text_edit_singleline(&mut f.description);
        ui.end_row();
        if f.what == SpanWhat::Spread {
            ui.label("template");
            let shown = f
                .spread_type
                .as_ref()
                .and_then(|t| f.library.get(t))
                .map_or_else(|| "own values".to_owned(), |t| t.name.clone());
            let mut chosen = None;
            egui::ComboBox::from_id_salt("spread-template")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for t in f
                        .library
                        .types
                        .iter()
                        .filter(|t| t.kind == TypeKind::Spread)
                    {
                        if ui
                            .selectable_label(
                                f.spread_type.as_deref() == Some(t.id.as_str()),
                                &t.name,
                            )
                            .clicked()
                        {
                            chosen = Some((t.id.clone(), t.load.unwrap_or(0.0)));
                        }
                    }
                });
            if let Some((id, load)) = chosen {
                f.spread_type = Some(id);
                f.per_metre = load;
            }
            ui.end_row();
            ui.label("load");
            if ui
                .add(
                    egui::DragValue::new(&mut f.per_metre)
                        .speed(0.1)
                        .suffix(" N/m"),
                )
                .on_hover_text("Downward, per metre of conductor, on the whole bundle")
                .changed()
            {
                f.spread_type = None;
            }
            ui.end_row();
        }
    });
    (f.from, f.to) = (from, to);
    if f.what == SpanWhat::Load {
        let library = f.library.clone();
        object_ui(ui, "span-form", &mut f.object, &library, &mut f.advanced);
    }
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
                stiffness: Some(model.line.defaults.insulator_stiffness),
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
        let put = Command::Put {
            kind: Kind::Spacer,
            id: s.id.clone(),
            value: serde_json::to_value(s).map_err(|e| e.to_string())?,
            ord: None,
        };
        let what = format!(
            "{} spacer {}",
            if self.new { "added" } else { "changed" },
            s.id
        );
        // Its ends are clamps: a bare end point gets the spacer clamp
        // template (one object for spacers and jumpers).
        let clamp = model
            .object_library()
            .get("spacer-clamp")
            .and_then(ols_line::ObjectType::object)
            .unwrap_or_else(|| LineObject::clamp(0.0));
        let stored = model.stored_line().unwrap_or(&model.line);
        let mut sections: Vec<ols_line::Section> = Vec::new();
        for end in [&s.a, &s.b] {
            let Some(sec) = stored.sections.iter().find(|x| x.id == end.section) else {
                continue;
            };
            let mut sec = sections
                .iter()
                .position(|x| x.id == sec.id)
                .map_or_else(|| sec.clone(), |k| sections.remove(k));
            if let Some(p) = sec
                .span_points
                .iter_mut()
                .find(|p| p.label == end.point && p.object.is_none())
            {
                p.object = Some(clamp.clone());
            }
            sections.push(sec);
        }
        let mut commands = Vec::new();
        for sec in sections {
            if stored.sections.contains(&sec) {
                continue;
            }
            commands.push(Command::Put {
                kind: Kind::Section,
                id: sec.id.clone(),
                value: serde_json::to_value(&sec).map_err(|e| e.to_string())?,
                ord: None,
            });
        }
        if commands.is_empty() {
            return Ok((what, put));
        }
        commands.push(put);
        Ok((
            format!("{what}, its ends clamped"),
            Command::Batch { commands },
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
                .unwrap_or(model.line.defaults.insulator_stiffness);
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

/// The put of `value` as object `kind`/`id`.
fn put(kind: Kind, id: &str, value: &impl serde::Serialize) -> Result<Command, String> {
    Ok(Command::Put {
        kind,
        id: id.to_owned(),
        value: serde_json::to_value(value).map_err(|e| e.to_string())?,
        ord: None,
    })
}
