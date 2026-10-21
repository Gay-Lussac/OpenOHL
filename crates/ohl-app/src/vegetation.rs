// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Vegetation: the danger-tree panel — points of the LiDAR
//! classes chosen (by default those with the role vegetation) near the
//! wires of the weather cases ticked (each windy case blown left and
//! right, along its swing arc and/or at every wind step, with unbalanced
//! ice on the spans near vegetation): grow-in now or after the growth
//! until the next cut, and trees that could fall onto a wire turning about
//! their root ball (heights above the surface of the checks) — per span
//! with the case and position of the worst, and **every** grow-in and
//! fall-in point found ([`ohl_clearance::vegetation`]); optionally the
//! isoclearance lines of the ground ([`ohl_clearance::isoclearance`]).
//! The required clearance and the allowance A are requirements by voltage
//! (each wire checked with its system's). The points are read again from
//! the point cloud in the search corridor, on a background thread.
//!
//! The panel can be docked ([`crate::dockable`]); each setting is
//! explained beside it, with small drawings ([`crate::check_help`]).
//! A design-code plugin's vegetation preset
//! ([`ohl_clearance::rules::VegetationPreset`]) is chosen from a dropdown,
//! previewed, then applied (its weather cases matched to the project's by
//! physics first, then by name, those with none created:
//! [`crate::preset_cases`]). A run is saved in Checks › Performed checks ([`CHECK_ID`])
//! and reopened from there.
//!
//! What it found is drawn on three layers ([`Overlay`]) while the panel
//! is open: every grow-in point and every danger tree (its trunk from base
//! to top) in two colours of the user's, and the isoclearance lines.
//! Closing the panel takes them off the views; reopening it (or a saved
//! run) draws them again.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use eframe::egui;
use ohl_clearance::WindSides;
use ohl_clearance::isoclearance::{Contour, IsoParams};
use ohl_clearance::positions::{PositionOptions, Positions, UnbalancedIce};
use ohl_clearance::rules::{PresetCheck, Requirement, VegetationPreset, WindSteps};
use ohl_clearance::vegetation::{
    DEFAULT_MAX_BASE_GROUND_DISTANCE, DEFAULT_SEARCH_DISTANCE, VegetationCheck, VegetationParams,
    WireVegetation,
};
use ohl_io::codes::RulePresetChecked;
use ohl_session::packing::{Reader, Writer};
use ohl_session::reports::{self, CheckReport, ReportStatus};
use serde_json::{Value, json};

use crate::check_help as help;
use crate::job::{Job, or_dash, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// The id of a saved vegetation run in the library of performed checks.
pub const CHECK_ID: &str = "ohl.vegetation";
/// Its title.
pub const CHECK_TITLE: &str = "Vegetation and danger trees";
/// The version of its parameters and results ([`saved`]).
pub const SAVED_VERSION: u32 = 1;
/// The user guide's page of the panel's settings.
pub const GUIDE: &str = "user-guide/analyses";

/// What a vegetation report is computed from.
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::ALL;

/// How unbalanced ice is checked (none: not).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ice {
    /// Not checked.
    Off,
    /// The case's ice on the span alone.
    OfCase,
    /// More radial ice, mm.
    Thickness,
    /// More ice load per wire, N/m.
    Extra,
    /// A load at mid-span, N.
    Concentrated,
}

impl Ice {
    const ALL: [(Ice, &'static str, &'static str); 5] = [
        (Ice::Off, "off", "off"),
        (Ice::OfCase, "case", "the case's ice on the span"),
        (Ice::Thickness, "thickness", "more radial ice, mm"),
        (Ice::Extra, "extra", "more ice load per wire, N/m"),
        (Ice::Concentrated, "load", "a load at mid-span, N"),
    ];

    fn key(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|x| x.0 == self)
            .map_or("off", |x| x.1)
    }

    fn text(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|x| x.0 == self)
            .map_or("off", |x| x.2)
    }

    fn parse(s: &str) -> Ice {
        Self::ALL
            .iter()
            .find(|x| x.1 == s)
            .map_or(Ice::Off, |x| x.0)
    }
}

/// The panel's settings: what a run uses, saved with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The LiDAR classes checked as vegetation (empty: those with the
    /// role vegetation).
    pub classes: Vec<u8>,
    /// Required clearance c from the wires (by voltage or not).
    pub clearance: Requirement,
    /// Growth per year, m.
    pub growth_per_year: f64,
    /// Years until the next cut.
    pub years: f64,
    /// Buffer b, m.
    pub buffer: f64,
    /// Look for trees falling in.
    pub fall_in: bool,
    /// Root-ball fraction k.
    pub root_ball: f64,
    /// Clearance allowance A of a falling tree (by voltage or not).
    pub allowance: Requirement,
    /// Smallest tree, m.
    pub min_tree_height: f64,
    /// Tallest tree, m.
    pub max_tree_height: f64,
    /// A limit to the distance of a tree base's ground point, m.
    pub base_limit: (bool, f64),
    /// Search distance: half width of the corridor read and drawn, m.
    pub search: f64,
    /// Each span along its swing arc too.
    pub arc: bool,
    /// The wind stepped (on with `stepped`), its step and sides.
    pub steps: WindSteps,
    /// Unbalanced ice and its value (mm, N/m or N).
    pub ice: (Ice, f64),
    /// Worst points kept per span.
    pub keep: usize,
    /// Isoclearance lines asked for, and how.
    pub iso: (bool, IsoParams),
}

impl Default for Settings {
    fn default() -> Self {
        // The defaults: the utility practice (root ball, allowance, base
        // ground point, search distance), as the command line.
        let p = VegetationParams::default();
        Self {
            classes: Vec::new(),
            clearance: Requirement::Fixed(p.clearance),
            growth_per_year: p.growth_per_year,
            years: p.years,
            buffer: p.buffer,
            fall_in: true,
            root_ball: p.root_ball,
            allowance: Requirement::Fixed(p.fall_allowance),
            min_tree_height: p.min_tree_height,
            max_tree_height: p.max_tree_height,
            base_limit: (true, DEFAULT_MAX_BASE_GROUND_DISTANCE),
            search: DEFAULT_SEARCH_DISTANCE,
            arc: false,
            steps: WindSteps::default(),
            ice: (Ice::Off, 5.0),
            keep: p.keep,
            iso: (false, IsoParams::default()),
        }
    }
}

/// A requirement's single distance, or the largest of its rows.
fn largest(r: &Requirement) -> f64 {
    match r {
        Requirement::Fixed(d) => *d,
        Requirement::ByVoltage(rows) => rows.iter().map(|x| x.1).fold(0.0, f64::max),
        Requirement::Formula { base, .. } => *base,
    }
}

impl Settings {
    /// The check's parameters.
    pub fn params(&self) -> VegetationParams {
        let by = |r: &Requirement| r.by_voltage().then(|| r.clone());
        VegetationParams {
            clearance: largest(&self.clearance),
            growth_per_year: self.growth_per_year,
            years: self.years,
            buffer: self.buffer,
            fall_in: self.fall_in,
            root_ball: self.root_ball,
            fall_allowance: largest(&self.allowance),
            min_tree_height: self.min_tree_height,
            max_tree_height: self.max_tree_height,
            max_base_ground_distance: self.base_limit.0.then_some(self.base_limit.1),
            keep: self.keep,
            clearance_by_voltage: by(&self.clearance),
            allowance_by_voltage: by(&self.allowance),
            keep_all: true,
        }
    }

    /// The classes checked: those chosen, else those with the role
    /// vegetation in `scheme`.
    pub fn classes_of(&self, scheme: &ohl_lidar::ClassScheme) -> Vec<u8> {
        if self.classes.is_empty() {
            scheme.codes_with_role("vegetation")
        } else {
            self.classes.clone()
        }
    }

    /// A preset's values put in (those it gives), and its wind steps.
    pub fn apply(&mut self, p: &VegetationPreset, wind: Option<WindSteps>) {
        if !p.classes.is_empty() {
            self.classes = p.classes.clone();
        }
        if let Some(r) = &p.clearance {
            self.clearance = r.clone();
        }
        if let Some(r) = &p.fall_allowance {
            self.allowance = r.clone();
        }
        let set = |to: &mut f64, v: Option<f64>| {
            if let Some(v) = v {
                *to = v;
            }
        };
        set(&mut self.growth_per_year, p.growth_per_year);
        set(&mut self.years, p.years);
        set(&mut self.buffer, p.buffer);
        set(&mut self.root_ball, p.root_ball);
        set(&mut self.min_tree_height, p.min_tree_height);
        set(&mut self.max_tree_height, p.max_tree_height);
        set(&mut self.search, p.search_distance);
        if let Some(d) = p.max_base_ground_distance {
            self.base_limit = (true, d);
        }
        if let Some(f) = p.fall_in {
            self.fall_in = f;
        }
        if let Some(w) = wind {
            // The arc of a `[wind]` table turns the swing arc on; its
            // steps and sides are the steps'.
            self.arc |= w.arc;
            self.steps = WindSteps { arc: false, ..w };
        }
        if let Some(a) = p.swing_arc {
            self.arc = a;
        }
    }

    /// As saved (JSON), with the weather cases' names.
    pub fn to_json(&self, cases: &[String]) -> Value {
        json!({
            "classes": self.classes,
            "cases": cases,
            "clearance": self.clearance.to_toml(),
            "growth_per_year": self.growth_per_year,
            "years": self.years,
            "buffer": self.buffer,
            "fall_in": self.fall_in,
            "root_ball": self.root_ball,
            "fall_allowance": self.allowance.to_toml(),
            "min_tree_height": self.min_tree_height,
            "max_tree_height": self.max_tree_height,
            "max_base_ground_distance": self.base_limit.0.then_some(self.base_limit.1),
            "search_distance": self.search,
            "swing_arc": self.arc,
            "wind_steps": self.steps.stepped,
            "wind_step": self.steps.step,
            "wind_sides": self.steps.sides.as_str(),
            "unbalanced_ice": self.ice.0.key(),
            "unbalanced_ice_value": self.ice.1,
            "keep": self.keep,
            "isoclearance": self.iso.0.then(|| json!({
                "interval": self.iso.1.interval,
                "max_level": self.iso.1.max_level,
                "spacing": self.iso.1.spacing,
            })),
        })
    }

    /// Read back from [`Settings::to_json`]: the settings and the cases'
    /// names.
    pub fn from_json(v: &Value) -> Result<(Self, Vec<String>), String> {
        let d = Self::default();
        let num = |k: &str, or: f64| v.get(k).and_then(Value::as_f64).unwrap_or(or);
        let req = |k: &str, or: &Requirement| -> Result<Requirement, String> {
            match v.get(k).and_then(Value::as_str) {
                Some(t) => Requirement::parse(t).map_err(|e| format!("{k}: {e}")),
                None => Ok(or.clone()),
            }
        };
        let cases = v
            .get("cases")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let base = v.get("max_base_ground_distance").and_then(Value::as_f64);
        let mut iso = IsoParams::default();
        let iso_on = match v.get("isoclearance") {
            Some(Value::Object(o)) => {
                let n = |k: &str, or: f64| o.get(k).and_then(Value::as_f64).unwrap_or(or);
                iso.interval = n("interval", iso.interval);
                iso.max_level = n("max_level", iso.max_level);
                iso.spacing = n("spacing", iso.spacing);
                true
            }
            _ => false,
        };
        Ok((
            Self {
                classes: v
                    .get("classes")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| c.as_u64().and_then(|c| u8::try_from(c).ok()))
                            .collect()
                    })
                    .unwrap_or_default(),
                clearance: req("clearance", &d.clearance)?,
                growth_per_year: num("growth_per_year", d.growth_per_year),
                years: num("years", d.years),
                buffer: num("buffer", d.buffer),
                fall_in: v
                    .get("fall_in")
                    .and_then(Value::as_bool)
                    .unwrap_or(d.fall_in),
                root_ball: num("root_ball", d.root_ball),
                allowance: req("fall_allowance", &d.allowance)?,
                min_tree_height: num("min_tree_height", d.min_tree_height),
                max_tree_height: num("max_tree_height", d.max_tree_height),
                base_limit: (base.is_some(), base.unwrap_or(d.base_limit.1)),
                search: num("search_distance", d.search),
                arc: v.get("swing_arc").and_then(Value::as_bool).unwrap_or(false),
                steps: WindSteps {
                    stepped: v
                        .get("wind_steps")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    step: num("wind_step", d.steps.step),
                    sides: v
                        .get("wind_sides")
                        .and_then(Value::as_str)
                        .and_then(|s| WindSides::parse(s).ok())
                        .unwrap_or(WindSides::Both),
                    arc: false,
                },
                ice: (
                    Ice::parse(
                        v.get("unbalanced_ice")
                            .and_then(Value::as_str)
                            .unwrap_or("off"),
                    ),
                    num("unbalanced_ice_value", d.ice.1),
                ),
                keep: v
                    .get("keep")
                    .and_then(Value::as_u64)
                    .map_or(d.keep, |k| k as usize),
                iso: (iso_on, iso),
            },
            cases,
        ))
    }
}

/// A point found: where, and its case and position ([`Report::labels`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoundPoint {
    /// The point (a danger tree: its top).
    pub point: [f64; 3],
    /// A danger tree's base.
    pub base: Option<[f64; 3]>,
    /// Its case and position.
    pub label: u32,
}

/// A worst point of a span.
#[derive(Debug, Clone, PartialEq)]
pub struct WorstPoint {
    /// The point.
    pub point: [f64; 3],
    /// Its class.
    pub class: u8,
    /// Distance to the nearest wire position's surface, m.
    pub distance: Option<f64>,
    /// Height above the ground, m.
    pub height: Option<f64>,
    /// Fall-in margin, m.
    pub fall_margin: Option<f64>,
    /// The smaller margin, m.
    pub margin: f64,
    /// Its case and position.
    pub label: u32,
}

/// One span's results.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanRow {
    /// Section.
    pub section: String,
    /// Span.
    pub span: usize,
    /// The required clearance applied, m.
    pub required: f64,
    /// Grow-in points.
    pub encroaching: u64,
    /// Fall-in points.
    pub fall_in: u64,
    /// The nearest point: distance, case and position.
    pub nearest: Option<(f64, u32)>,
    /// The worst falling tree: margin, case and position.
    pub worst_fall: Option<(f64, u32)>,
    /// The worst points, worst first.
    pub worst: Vec<WorstPoint>,
}

/// What a run found (or a saved run holds).
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The spans with what was found near each.
    pub spans: Vec<SpanRow>,
    /// Every grow-in point.
    pub grow: Vec<FoundPoint>,
    /// Every danger tree (top and base).
    pub fall: Vec<FoundPoint>,
    /// The cases and positions named (indices from the points and rows).
    pub labels: Vec<String>,
    /// Vegetation points read.
    pub points: u64,
    /// Wire positions checked.
    pub positions: usize,
    /// Notes.
    pub notes: Vec<String>,
    /// The settings of the run.
    pub settings: Settings,
    /// Its weather cases' names.
    pub cases: Vec<String>,
    /// The isoclearance lines (a run, when asked for: not saved).
    pub lines: Option<(Vec<Contour>, f64)>,
    /// The saved report it was read from, if any.
    pub saved: Option<String>,
}

impl Report {
    /// The report of a run.
    fn of_run(
        positions: &Positions,
        results: &[WireVegetation],
        required: &[f64],
        params: &VegetationParams,
        settings: Settings,
        cases: Vec<String>,
    ) -> Self {
        let mut labels: Vec<String> = Vec::new();
        let mut index: HashMap<usize, u32> = HashMap::new();
        let mut label = |k: Option<usize>| -> u32 {
            let Some(k) = k else {
                return u32::MAX;
            };
            *index.entry(k).or_insert_with(|| {
                labels.push(positions.describe(k));
                (labels.len() - 1) as u32
            })
        };
        let mut spans = Vec::new();
        let (mut grow, mut fall) = (Vec::new(), Vec::new());
        for ((s, r), req) in positions.spans.iter().zip(results).zip(required) {
            for f in &r.all {
                if f.grows {
                    grow.push(FoundPoint {
                        point: f.point,
                        base: None,
                        label: label(f.position),
                    });
                }
                if f.falls {
                    fall.push(FoundPoint {
                        point: f.point,
                        base: f.base,
                        label: label(f.fall_position),
                    });
                }
            }
            if r.encroaching == 0 && r.fall_in == 0 && r.nearest.is_none() {
                continue;
            }
            spans.push(SpanRow {
                section: s.wire.section.clone(),
                span: s.wire.span,
                required: *req,
                encroaching: r.encroaching,
                fall_in: r.fall_in,
                nearest: r.nearest.map(|v| (v.distance, label(v.position))),
                worst_fall: r
                    .worst_fall
                    .and_then(|v| v.fall_margin.map(|m| (m, label(v.fall_position)))),
                worst: r
                    .worst
                    .iter()
                    .map(|v| WorstPoint {
                        point: v.point,
                        class: v.class,
                        distance: v.distance.is_finite().then_some(v.distance),
                        height: v.height,
                        fall_margin: v.fall_margin,
                        margin: v.margin(params),
                        label: if v.fall_margin.is_some_and(|m| m < 0.0) {
                            label(v.fall_position)
                        } else {
                            label(v.position)
                        },
                    })
                    .collect(),
            });
        }
        Self {
            spans,
            grow,
            fall,
            labels,
            points: 0,
            positions: positions.positions.len(),
            notes: Vec::new(),
            settings,
            cases,
            lines: None,
            saved: None,
        }
    }

    /// The case and position of `label`.
    pub fn label(&self, label: u32) -> &str {
        self.labels.get(label as usize).map_or("-", String::as_str)
    }

    /// The worst points of every span, and every point found, as CSV.
    pub fn csv(&self) -> String {
        use std::fmt::Write as _;
        let mut t = String::from(
            "section,span,kind,class,x,y,z,distance_m,height_m,fall_margin_m,margin_m,required_m,case_position\n",
        );
        let f = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.3}"));
        for s in &self.spans {
            for w in &s.worst {
                let _ = writeln!(
                    t,
                    "{},{},worst,{},{:.3},{:.3},{:.3},{},{},{},{:.3},{:.3},{}",
                    ohl_core::text::csv_field(&s.section),
                    s.span,
                    w.class,
                    w.point[0],
                    w.point[1],
                    w.point[2],
                    f(w.distance),
                    f(w.height),
                    f(w.fall_margin),
                    w.margin,
                    s.required,
                    ohl_core::text::csv_field(self.label(w.label)),
                );
            }
        }
        for (kind, list) in [("grow_in", &self.grow), ("fall_in", &self.fall)] {
            for p in list {
                let _ = writeln!(
                    t,
                    ",,{kind},,{:.3},{:.3},{:.3},,{},,,,{}",
                    p.point[0],
                    p.point[1],
                    p.point[2],
                    f(p.base.map(|b| p.point[2] - b[2])),
                    ohl_core::text::csv_field(self.label(p.label)),
                );
            }
        }
        t
    }
}

/// What the views draw of a report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// Every grow-in point.
    pub grow: Vec<[f64; 3]>,
    /// Every danger tree: its top and its base.
    pub fall: Vec<([f64; 3], [f64; 3])>,
    /// Isoclearance lines: clearance, colour, points on the ground.
    pub lines: Vec<(f64, [u8; 3], Vec<[f64; 3]>)>,
    /// The colours of the grow-in points and of the danger trees.
    pub colors: [[u8; 3]; 2],
    /// The report is out of date: drawn dimmed ([`crate::revision`]).
    pub stale: bool,
}

impl Overlay {
    /// Every point of `report` and its lines, in `colors`.
    pub fn of(report: &Report, colors: [[u8; 3]; 2]) -> Self {
        let mut out = Self {
            colors,
            ..Self::default()
        };
        out.grow = report.grow.iter().map(|p| p.point).collect();
        out.fall = report
            .fall
            .iter()
            .map(|p| (p.point, p.base.unwrap_or(p.point)))
            .collect();
        if let Some((lines, top)) = &report.lines {
            for c in lines {
                let hex = ohl_drawing::isoclearance_color(c.level, *top);
                let rgb = crate::scene::rgb(&hex);
                out.lines.push((c.level, rgb, c.points.clone()));
            }
        }
        out
    }
}

/// What the panel asks of the application.
#[derive(Default)]
pub struct Out {
    /// A span to go to.
    pub go: Option<Selection>,
    /// A command to apply (a run saved), with what it does.
    pub apply: Option<(String, ohl_session::Command)>,
    /// A preset to apply (its plugin, the preset): its weather cases
    /// missing from the project created, then selected.
    pub preset: Option<(String, RulePresetChecked)>,
    /// The colours of the two layers chosen.
    pub colors: Option<[[u8; 3]; 2]>,
    /// A section of the user guide to open.
    pub guide: Option<&'static str>,
}

/// The Vegetation panel.
#[derive(Default)]
pub struct VegetationWindow {
    /// Shown (closed: its layers are taken off the views).
    pub open: bool,
    /// Weather cases ticked (indices in the model's cases).
    pub cases: Vec<usize>,
    /// The settings.
    pub settings: Settings,
    job: Job<Report>,
    /// Counts the reports taken (a new one is drawn).
    generation: u64,
    /// What the views draw now: shown, report, colours.
    drawn: Option<(bool, u64, [[u8; 3]; 2])>,
    /// A saved run shown: what changed since it was saved, when it was
    /// read (none: unchanged then, tracked live since).
    saved_stale: Option<String>,
    /// The name to save the run as, and what the last save did.
    save_name: String,
    save_message: Option<(String, bool)>,
    /// A save asked for under a name in use (asked once more).
    overwrite: Option<String>,
    /// The preset chosen (plugin, preset).
    pub preset: Option<(usize, usize)>,
    /// The colour tool open: which layer (0 grow-in, 1 fall-in).
    color_tool: Option<(usize, crate::color_tool::ColorTool)>,
    /// The case names a preset selects, once the model has them.
    pending_cases: Option<Vec<String>>,
}

/// The ground the checks use, in words (a saved run says whether it
/// changed since).
fn ground_key(model: &Model) -> String {
    format!(
        "{}|{}",
        model.ground_source,
        model.check_surface.clone().unwrap_or_default()
    )
}

impl VegetationWindow {
    /// Opens the panel on the case shown (ticked, if nothing is).
    pub fn show(&mut self, case: usize) {
        self.open = true;
        if self.cases.is_empty() {
            self.cases.push(case);
        }
    }

    /// Sets the name a run is saved as (tests).
    #[cfg(test)]
    pub fn set_save_name(&mut self, name: &str) {
        self.save_name = name.to_owned();
    }

    /// The last run's answer, and whether one runs.
    #[cfg(test)]
    pub fn result(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    /// The report shown.
    fn report(&self) -> Option<&Report> {
        match &self.job.result {
            Some(Ok(r)) => Some(r),
            _ => None,
        }
    }

    /// The isoclearance lines of the report shown while the panel is
    /// open, for the drawings.
    pub fn isoclearance_lines(&self) -> Option<Arc<crate::drawings::IsoLines>> {
        if !self.open {
            return None;
        }
        let (lines, top) = self.report()?.lines.as_ref()?;
        Some(Arc::new((
            lines.iter().map(|c| (c.level, c.points.clone())).collect(),
            *top,
        )))
    }

    /// What the views should draw, when it changed since the last call:
    /// the report shown while the panel is open (in the model's colours),
    /// nothing once it is closed.
    pub fn overlay_change(&mut self, model: &Model) -> Option<Overlay> {
        let shown = self.open && self.report().is_some();
        let key = (shown, self.generation, model.vegetation_colors);
        if self.drawn == Some(key) {
            return None;
        }
        self.drawn = Some(key);
        Some(match (shown, self.report()) {
            (true, Some(r)) => Overlay::of(r, model.vegetation_colors),
            _ => Overlay::default(),
        })
    }

    fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(spec) = model.cloud.clone() else {
            self.job.result = Some(Err("no point cloud in this model".into()));
            return;
        };
        let classes = self.settings.classes_of(&model.scheme);
        if classes.is_empty() {
            self.job.result = Some(Err(format!(
                "no class chosen, and no class of \"{}\" has the role vegetation",
                model.scheme.name
            )));
            return;
        }
        let cases: Vec<ohl_weather::WeatherCase> = self
            .cases
            .iter()
            .filter_map(|&k| model.cases.get(k).map(|c| c.case.clone()))
            .collect();
        if cases.is_empty() {
            self.job.result = Some(Err("tick a weather case".into()));
            return;
        }
        for (what, r) in [
            ("required clearance", &self.settings.clearance),
            ("allowance A", &self.settings.allowance),
        ] {
            // One distance may be 0 m (no allowance); a table or formula
            // must be complete.
            if r.by_voltage()
                && let Some(e) = r.problem()
            {
                self.job.result = Some(Err(format!("{what}: {e}")));
                return;
            }
        }
        self.saved_stale = None;
        let line = model.line.clone();
        let library = model.library.clone();
        let ground = model.ground_for_checks();
        let settings = self.settings.clone();
        let params = settings.params();
        let options = PositionOptions {
            swing_arc: settings.arc,
            wind_steps: settings.steps.stepped.then_some(settings.steps),
        };
        let ice = match settings.ice.0 {
            Ice::Off => None,
            Ice::OfCase => Some(UnbalancedIce::CaseIce),
            Ice::Thickness => Some(UnbalancedIce::ExtraThickness(settings.ice.1 / 1000.0)),
            Ice::Extra => Some(UnbalancedIce::ExtraLoad(settings.ice.1)),
            Ice::Concentrated => Some(UnbalancedIce::Concentrated(settings.ice.1)),
        };
        let corridor_width = settings.search;
        let iso = settings.iso.0.then_some(settings.iso.1);
        let origin = model.origin;
        let names: Vec<String> = cases.iter().map(|c| c.name.clone()).collect();
        self.job.start_at(ctx, model.revision(), move || {
            // Exit clause: ~1 µs a point and position set; an hour is far
            // beyond what a line needs.
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(3600));
            let mut positions = ohl_clearance::positions::solve_positions(
                &line, &library, &cases, options, &budget,
            )?;
            // The points, as whole millimetres from the model's origin.
            let (file, name) = ohl_lidar::cloud_spec(&spec);
            let source =
                ohl_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
            let reach = params.near_reach().max(if params.fall_in {
                params.fall_reach()
            } else {
                0.0
            });
            let corridor = ohl_lidar::Corridor::along_spans(&line, reach.max(corridor_width));
            let mm = |v: f64, k: usize| ((v - origin[k]) * 1000.0).round() as i32;
            let mut points: Vec<([i32; 3], u8)> = Vec::new();
            ohl_pointcloud::PointSource::visit(
                &source,
                Some(&corridor),
                Some(&classes),
                &budget,
                &mut |x, y, z, c| points.push(([mm(x, 0), mm(y, 1), mm(z, 2)], c)),
            )?;
            let at = |v: i32, k: usize| v as f64 / 1000.0 + origin[k];
            let mut notes = positions.notes.clone();
            if let Some(mode) = ice {
                let spans = ohl_clearance::vegetation::spans_near(
                    &positions,
                    reach,
                    points.iter().map(|(p, _)| [at(p[0], 0), at(p[1], 1)]),
                );
                let solves = ohl_clearance::positions::add_unbalanced_ice(
                    &mut positions,
                    &line,
                    &library,
                    &cases,
                    mode,
                    &spans,
                    &budget,
                )?;
                notes.push(format!(
                    "unbalanced ice: {} of {} spans with vegetation within reach, {solves} solves",
                    spans.len(),
                    positions.spans.len()
                ));
            }
            let positions = Arc::new(positions);
            let mut check =
                VegetationCheck::with_positions(Arc::clone(&positions), &classes, params.clone());
            check.visit_parallel(
                ground.as_deref(),
                &points,
                |(p, c)| (at(p[0], 0), at(p[1], 1), at(p[2], 2), *c),
                &budget,
            )?;
            if params.fall_in && ground.is_none() {
                notes.push("no ground surface: no fall-in check".into());
            } else if check.without_ground > 0 {
                notes.push(format!(
                    "{} points without a tree base (no fall-in check): {} with no ground below them, {} with no ground point within the distance",
                    check.without_ground,
                    check.without_ground - check.ground_too_far,
                    check.ground_too_far
                ));
            }
            let lines = match (iso, ground.as_deref()) {
                (Some(iso), Some(g)) => {
                    let area = ohl_lidar::Corridor::along_spans(&line, corridor_width);
                    let bounds = area.bounds().ok_or("no span to draw the lines along")?;
                    let found = ohl_clearance::isoclearance::isoclearance(
                        &positions,
                        g,
                        bounds,
                        &|x, y| area.contains(x, y),
                        &iso,
                        &budget,
                    )?;
                    Some((found.contours, iso.max_level))
                }
                (Some(_), None) => {
                    notes.push("no ground surface: no isoclearance lines".into());
                    None
                }
                _ => None,
            };
            let read = check.points;
            let required = check.required().to_vec();
            let results = check.finish();
            let mut report =
                Report::of_run(&positions, &results, &required, &params, settings, names);
            report.points = read;
            report.notes = notes;
            report.lines = lines;
            Ok(report)
        });
    }

    /// What the report shown was computed from that changed since, in
    /// words (`None`: nothing, or no report): the line, the LiDAR points,
    /// the ground ([`crate::revision`]); a saved run: since it was saved.
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        self.report()?;
        if let Some(s) = &self.saved_stale {
            return Some(s.clone());
        }
        self.job.stamp.out_of_date(model, USES)
    }

    /// The saved run `r` shown (its settings, cases and results; its
    /// layers drawn), with what changed since it was saved.
    pub fn load_saved(&mut self, r: &CheckReport, model: &Model) -> Result<(), String> {
        let (report, params) = saved::read(r)?;
        // What changed since: the line by its fingerprints, the points by
        // their state, the ground by its source.
        let mut what = Vec::new();
        if let Some(s) = model.session() {
            if let Ok(now) = s.check_state()
                && !reports::changed_since(&r.state, &now).is_empty()
            {
                what.push("the line");
            }
            if params.get("points_state").and_then(Value::as_str) != Some(&s.points_state()) {
                what.push("the LiDAR points");
            }
        }
        if params.get("ground").and_then(Value::as_str) != Some(&ground_key(model)) {
            what.push("the ground");
        }
        self.saved_stale = match what.as_slice() {
            [] => None,
            [one] => Some((*one).to_owned()),
            [first @ .., last] => Some(format!("{} and {last}", first.join(", "))),
        };
        // Unchanged: tracked from now on, as a run.
        self.job.stamp.set(model.revision());
        self.settings = report.settings.clone();
        self.cases = report
            .cases
            .iter()
            .filter_map(|n| model.cases.iter().position(|c| &c.case.name == n))
            .collect();
        self.save_name = r.id.clone();
        self.job.result = Some(Ok(report));
        self.generation += 1;
        self.open = true;
        Ok(())
    }

    /// The run shown as a report named `name` of the performed checks.
    fn to_check_report(&self, model: &Model, name: &str) -> Result<CheckReport, String> {
        let report = self.report().ok_or("run the check first")?;
        let editing = model
            .editing
            .as_ref()
            .map_err(|e| format!("read-only: {e}"))?;
        let s = &editing.session;
        let mut parameters = report.settings.to_json(&report.cases);
        parameters["points_state"] = json!(s.points_state());
        parameters["ground"] = json!(ground_key(model));
        parameters["version"] = json!(SAVED_VERSION);
        let (enc, fall) = report
            .spans
            .iter()
            .fold((0, 0), |(e, f), r| (e + r.encroaching, f + r.fall_in));
        Ok(CheckReport {
            format: reports::REPORT_FILE_FORMAT.into(),
            version: reports::REPORT_FILE_VERSION,
            id: name.to_owned(),
            check: CHECK_ID.into(),
            title: CHECK_TITLE.into(),
            time: s.now().map_err(|e| e.to_string())?,
            user: editing.user.clone(),
            app: format!("OHL-Design {}", env!("CARGO_PKG_VERSION")),
            plugin: None,
            status: ReportStatus::Draft,
            note: s.report(name).map(|o| o.note).unwrap_or_default(),
            state: s.check_state().map_err(|e| e.to_string())?,
            summary: format!(
                "{} vegetation points; grow-in {enc}, fall-in {fall} ({} and {} points listed)",
                report.points,
                report.grow.len(),
                report.fall.len()
            ),
            parameters,
            results: saved::results(report)?,
        })
    }

    /// The panel (a window, or a dock panel); what it asks of the app.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
        palette: &mut crate::color_tool::Palette,
    ) -> Out {
        let was = self.job.running();
        self.job.poll();
        if was && !self.job.running() {
            self.generation += 1;
        }
        if let Some(names) = self.pending_cases.take() {
            let found: Vec<usize> = names
                .iter()
                .filter_map(|n| model.cases.iter().position(|c| &c.case.name == n))
                .collect();
            if found.len() == names.len() {
                self.cases = found;
            } else {
                self.pending_cases = Some(names);
            }
        }
        let mut out = Out::default();
        if !self.open {
            return out;
        }
        let mut open = true;
        let mut again = false;
        let stale = self.out_of_date(model);
        let window = egui::Window::new(CHECK_TITLE)
            .default_width(900.0)
            .default_height(760.0);
        let shown = crate::dockable::show(
            ctx,
            crate::dockable::Panel::Vegetation,
            CHECK_TITLE,
            window,
            Some(&mut open),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("vegetation-panel")
                    .show(ui, |ui| {
                        self.run_ui(ui, model, &mut out);
                        // Out of date: said at the top, Run again beside.
                        if let Some(what) = &stale
                            && self.report().is_some()
                        {
                            again = crate::revision::banner(ui, what, !self.job.running());
                        }
                        self.preset_ui(ui, model, presets, &mut out);
                        ui.separator();
                        egui::CollapsingHeader::new("Settings")
                            .id_salt("vegetation-settings")
                            .default_open(true)
                            .show(ui, |ui| {
                                self.settings_ui(ui, model, &mut out);
                                ui.separator();
                                self.colors_ui(ui, model);
                            });
                        ui.separator();
                        match &self.job.result {
                            None => {
                                ui.label(
                                    "Vegetation near the wires of the cases ticked, now and after \
                                     growth, and trees that could fall onto them turning about \
                                     their root ball.",
                                );
                            }
                            Some(Err(e)) => {
                                crate::menu::error_label(ui, e);
                            }
                            Some(Ok(report)) => {
                                if let Some(go) = report_ui(ui, report) {
                                    out.go = Some(go);
                                }
                            }
                        }
                    });
            },
        );
        if shown == crate::dockable::Shown::Closed {
            open = false;
        }
        self.open = open;
        if again {
            self.start(model, ctx);
        }
        // The colour tool, when open.
        if let Some((k, tool)) = &mut self.color_tool {
            match tool.show(ctx, egui::Id::new(("vegetation-color", *k)), palette) {
                Some(crate::color_tool::Outcome::Pick(c)) => {
                    let mut colors = model.vegetation_colors;
                    colors[*k] = c;
                    out.colors = Some(colors);
                    self.color_tool = None;
                }
                Some(crate::color_tool::Outcome::Cancel) => self.color_tool = None,
                None => {}
            }
        }
        out
    }

    /// The preset dropdown, its preview and Apply.
    fn preset_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
        out: &mut Out,
    ) {
        ui.horizontal(|ui| {
            ui.label("Preset");
            crate::rule_editor::preset_dropdown(
                ui,
                "vegetation-preset",
                presets,
                |p| crate::preset_cases::Part::Check(PresetCheck::Vegetation).lists(&p.rules),
                &mut self.preset,
            );
        });
        let Some((plugin, p)) = self.preset.and_then(|(i, k)| {
            let pp = presets.get(i)?;
            Some((pp.plugin.clone(), pp.presets.as_ref().ok()?.get(k)?.clone()))
        }) else {
            help::explain(
                ui,
                "A design-code plugin may preset the classes, the distances and the wind steps \
                 of this check.",
                "The presets of every design-code plugin turned on (Plugins › Plugin manager), \
                 read with the parameter file of Criteria › Design code. Choose one to see what \
                 it sets; Apply puts its values in, creates its weather cases the project lacks \
                 and ticks them.",
            );
            return;
        };
        let Some(v) = &p.rules.vegetation else {
            return;
        };
        let voltages = ohl_clearance::zone::line_voltages(&model.line);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.strong(format!("{plugin}: {}", p.name));
            if !p.description.is_empty() {
                crate::menu::message(ui, egui::RichText::new(&p.description).small(), None);
            }
            let mut rows: Vec<(String, String)> = Vec::new();
            if !v.classes.is_empty() {
                let names: Vec<String> = v
                    .classes
                    .iter()
                    .map(|c| match model.scheme.class(*c) {
                        Some(d) => format!("{c} {}", d.name),
                        None => format!("{c}"),
                    })
                    .collect();
                rows.push(("classes".into(), names.join(", ")));
            }
            if let Some(r) = &v.clearance {
                rows.push((
                    "required clearance".into(),
                    format!(
                        "{r} — {}",
                        crate::requirement_ui::preview_text(r, &voltages)
                    ),
                ));
            }
            if let Some(r) = &v.fall_allowance {
                rows.push(("allowance A".into(), r.to_string()));
            }
            for (k, x) in [
                ("growth per year, m", v.growth_per_year),
                ("years", v.years),
                ("buffer b, m", v.buffer),
                ("root-ball fraction k", v.root_ball),
                ("smallest tree, m", v.min_tree_height),
                ("tallest tree, m", v.max_tree_height),
                ("tree base to a ground point, m", v.max_base_ground_distance),
                ("search distance, m", v.search_distance),
            ] {
                if let Some(x) = x {
                    rows.push((k.into(), format!("{x}")));
                }
            }
            if let Some(w) = p.rules.wind {
                rows.push((
                    "wind".into(),
                    if w.stepped {
                        format!("stepped by {} Pa, from {}", w.step, w.sides.as_str())
                    } else {
                        "not stepped".into()
                    },
                ));
            }
            if let Some(a) = v.swing_arc {
                rows.push(("swing arc".into(), if a { "yes" } else { "no" }.into()));
            }
            egui::Grid::new("vegetation-preset-preview")
                .num_columns(2)
                .show(ui, |ui| {
                    for (k, x) in rows {
                        ui.label(egui::RichText::new(k).small());
                        ui.label(egui::RichText::new(x).small());
                        ui.end_row();
                    }
                });
            // Only its vegetation part and its own weather cases (owner,
            // 2026-10-21: presets are per check; its rules are not taken),
            // matched to the project's: the same weather used, the others
            // created.
            let matched = crate::preset_cases::for_project(
                model,
                &p,
                crate::preset_cases::Part::Check(PresetCheck::Vegetation),
            );
            match &matched {
                Ok((_, plan)) => {
                    for line in plan.lines() {
                        ui.label(egui::RichText::new(line).small());
                    }
                }
                Err(e) => {
                    ui.label(
                        egui::RichText::new(format!("its weather cases: {e}"))
                            .small()
                            .color(ui.visuals().error_fg_color),
                    );
                }
            }
            if ui
                .button("Apply preset")
                .on_hover_text(
                    "Its values replace the panel's (edit them freely after); your weather \
                     cases of the same weather are used, the others created (one change, \
                     undone by Undo); all of them ticked",
                )
                .clicked()
                && let Ok((adapted, plan)) = matched
            {
                self.settings.apply(v, p.rules.wind);
                if adapted.weather.is_some() {
                    self.pending_cases = Some(plan.names());
                }
                out.preset = Some((plugin, adapted));
            }
        });
    }

    /// A heading with a link to its section of the user guide.
    fn heading(ui: &mut egui::Ui, title: &str, anchor: &'static str, out: &mut Out) {
        if let Some(a) = help::heading(ui, title, anchor) {
            out.guide = Some(a);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, model: &Model, out: &mut Out) {
        let voltages = ohl_clearance::zone::line_voltages(&model.line);
        let num = |ui: &mut egui::Ui, label: &str, v: &mut f64, speed: f64, max: f64| {
            ui.label(label);
            ui.add(egui::DragValue::new(v).speed(speed).range(0.0..=max));
        };
        // Wire positions.
        Self::heading(ui, "Wire positions", "vegetation-wire-positions", out);
        let cases = &mut self.cases;
        let s = &mut self.settings;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                // The cases: a tick each (the list stays open while ticking).
                egui::CollapsingHeader::new(format!("Weather cases ({} ticked)", cases.len()))
                    .id_salt("vegetation-cases")
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(140.0)
                            .show(ui, |ui| {
                                for (k, c) in model.cases.iter().enumerate() {
                                    let mut on = cases.contains(&k);
                                    if ui.checkbox(&mut on, &c.case.name).changed() {
                                        if on {
                                            cases.push(k);
                                            cases.sort_unstable();
                                        } else {
                                            cases.retain(|x| *x != k);
                                        }
                                    }
                                }
                            });
                    });
                help::explain(
                    ui,
                    "Each case ticked: still air as it is, a windy case blown to the left and to \
                     the right.",
                    "The wires are solved in each case ticked; a case with wind is solved with \
                     the wind from the right (blown left) and from the left (blown right).",
                );
                // The wind choice shared with the clearance windows (the
                // arc kept apart, as saved).
                let full = cases
                    .iter()
                    .filter_map(|&k| model.cases.get(k))
                    .map(|c| c.case.wind_pressure.si())
                    .fold(0.0, f64::max);
                let mut wind = WindSteps {
                    arc: s.arc,
                    ..s.steps
                };
                help::wind_choice(ui, "vegetation", &mut wind, false, full);
                s.arc = wind.arc;
                s.steps = WindSteps { arc: false, ..wind };
                ui.horizontal(|ui| {
                    ui.label("unbalanced ice");
                    egui::ComboBox::from_id_salt("vegetation-ice")
                        .selected_text(s.ice.0.text())
                        .show_ui(ui, |ui| {
                            for (k, _, n) in Ice::ALL {
                                ui.selectable_value(&mut s.ice.0, k, n);
                            }
                        });
                    if !matches!(s.ice.0, Ice::Off | Ice::OfCase) {
                        ui.add(
                            egui::DragValue::new(&mut s.ice.1)
                                .speed(1.0)
                                .range(0.0..=1e6),
                        );
                    }
                });
                help::explain(
                    ui,
                    "The span checked carries more ice than its neighbours (spans near \
                     vegetation only).",
                    "Solved with its section on its own, the span in a weather case of its own: \
                     the case's ice on it alone, more radial ice, more ice load per wire, or a \
                     load at mid-span.",
                );
            });
            ui.vertical(|ui| {
                let n = if s.steps.stepped {
                    let full = cases
                        .iter()
                        .filter_map(|&k| model.cases.get(k))
                        .map(|c| c.case.wind_pressure.si())
                        .fold(0.0, f64::max);
                    (full / s.steps.step.max(1.0)).ceil() as usize
                } else {
                    0
                };
                help::wind_positions(ui, s.arc, n);
                help::unbalanced_ice(ui, s.ice.0 != Ice::Off);
            });
        });
        ui.separator();
        // Grow-in.
        Self::heading(ui, "Grow-in", "vegetation-grow-in", out);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("required clearance c");
                    crate::requirement_ui::requirement_ui_stacked(
                        ui,
                        "vegetation-clearance",
                        &mut s.clearance,
                        &voltages,
                    );
                });
                help::explain(
                    ui,
                    "Radially from the conductor's surface; one distance, or by the voltage of \
                     each wire's system.",
                    "A table \"up to kV: distance\" or a base plus an amount per kV checks each \
                     wire with the distance for its own system's nominal voltage (an earthed \
                     wire: 0 kV). The line under it gives it at the line's voltages.",
                );
                ui.horizontal(|ui| {
                    num(ui, "growth per year g", &mut s.growth_per_year, 0.05, 5.0);
                    num(ui, "years", &mut s.years, 0.1, 30.0);
                });
                help::explain(
                    ui,
                    "Growth until the next cut: g × years, in every direction.",
                    "A point closer than c + g·years + b to a wire grows in before the next cut.",
                );
                ui.horizontal(|ui| num(ui, "buffer b, m", &mut s.buffer, 0.05, 20.0));
                help::explain(
                    ui,
                    "Added to the clearance, e.g. the survey's accuracy.",
                    "The buffer b covers what the points do not say exactly: the survey's \
                     accuracy, the wire's position at the survey.",
                );
            });
            help::grow_in(
                ui,
                largest(&s.clearance),
                (s.growth_per_year * s.years).max(0.0),
                s.buffer,
            );
        });
        ui.separator();
        // Fall-in.
        Self::heading(ui, "Fall-in", "vegetation-fall-in", out);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.checkbox(&mut s.fall_in, "trees falling in");
                ui.add_enabled_ui(s.fall_in, |ui| {
                    ui.horizontal(|ui| {
                        num(ui, "root-ball fraction k", &mut s.root_ball, 0.01, 0.5)
                    });
                    help::explain(
                        ui,
                        "The tree turns about the edge of its root ball, k × h from its trunk.",
                        "h: the tree's height grown until the next cut. k = 0: it turns about \
                         its base. The sector it sweeps has radius √(h² + (k·h)²).",
                    );
                    ui.horizontal(|ui| {
                        ui.label("allowance A");
                        crate::requirement_ui::requirement_ui_stacked(
                            ui,
                            "vegetation-allowance",
                            &mut s.allowance,
                            &voltages,
                        );
                    });
                    help::explain(
                        ui,
                        "What a falling tree must keep from the wire (flashover, buffers).",
                        "A tree whose sector comes within A of a wire position is a danger \
                         tree; by voltage like the clearance when it depends on it.",
                    );
                    ui.horizontal(|ui| {
                        num(ui, "smallest tree, m", &mut s.min_tree_height, 0.1, 50.0);
                        num(ui, "tallest tree, m", &mut s.max_tree_height, 0.5, 120.0);
                    });
                    help::explain(
                        ui,
                        "Points lower than the smallest are not trees; bases are searched as far \
                         as the tallest can reach.",
                        "Heights above the surface of the checks (the TIN below the point).",
                    );
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut s.base_limit.0, "tree base to a ground point within m");
                        ui.add_enabled(
                            s.base_limit.0,
                            egui::DragValue::new(&mut s.base_limit.1)
                                .speed(0.1)
                                .range(0.1..=100.0),
                        );
                    });
                    help::explain(
                        ui,
                        "The base is taken from the ground only with a ground point this near in \
                         plan.",
                        "Where the ground surface is only interpolated far from any ground point \
                         (under dense trees), its height is not trusted: no fall-in check there.",
                    );
                });
            });
            ui.vertical(|ui| {
                help::falling_tree(ui, s.root_ball, largest(&s.allowance));
                help::tree_base(ui, s.base_limit.0.then_some(s.base_limit.1));
            });
        });
        ui.separator();
        // Search.
        Self::heading(ui, "Search", "vegetation-search", out);
        ui.horizontal(|ui| {
            ui.label("classes");
            let chosen: BTreeSet<u8> = s.classes.iter().copied().collect();
            let shown = if chosen.is_empty() {
                "by role: vegetation".to_owned()
            } else {
                chosen
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            // A tick menu: it stays open while ticking.
            let r = ui.menu_button(format!("{shown} ⏷"), |ui| {
                ui.set_max_height(360.0);
                if ui
                    .button("By role (vegetation)")
                    .on_hover_text("The classes whose role is vegetation in the class scheme")
                    .clicked()
                {
                    s.classes.clear();
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for c in &model.scheme.classes {
                        let mut on = chosen.contains(&c.code)
                            || (chosen.is_empty() && c.roles.iter().any(|r| r == "vegetation"));
                        ui.horizontal(|ui| {
                            help::class_swatch(ui, &model.scheme, c.code);
                            if ui
                                .checkbox(&mut on, format!("{} {}", c.code, c.name))
                                .changed()
                            {
                                let mut set: BTreeSet<u8> = if s.classes.is_empty() {
                                    model
                                        .scheme
                                        .codes_with_role("vegetation")
                                        .into_iter()
                                        .collect()
                                } else {
                                    s.classes.iter().copied().collect()
                                };
                                if on {
                                    set.insert(c.code);
                                } else {
                                    set.remove(&c.code);
                                }
                                s.classes = set.into_iter().collect();
                            }
                        });
                    }
                });
            });
            r.response
                .on_hover_text("The LiDAR classes checked as vegetation");
        });
        help::explain(
            ui,
            "The LiDAR classes checked as vegetation (default: those whose role is vegetation).",
            "From the project's class scheme (number, name, colour). A design-code preset may \
             set them.",
        );
        ui.horizontal(|ui| {
            num(ui, "search distance ± m", &mut s.search, 1.0, 500.0);
            ui.label("points kept per span");
            ui.add(egui::DragValue::new(&mut s.keep).range(0..=1000));
        });
        help::explain(
            ui,
            "Half width of the corridor read around the spans (and drawn for the lines).",
            "Points farther from every span's chord are not read; the reach of the checks is \
             added when larger. Every grow-in and fall-in point found is listed and drawn; the \
             table keeps the worst of each span.",
        );
        ui.horizontal(|ui| {
            let iso = &mut s.iso;
            ui.checkbox(&mut iso.0, "isoclearance lines");
            ui.add_enabled_ui(iso.0, |ui| {
                num(ui, "every m", &mut iso.1.interval, 0.1, 50.0);
                num(ui, "hide above m", &mut iso.1.max_level, 0.5, 200.0);
                num(ui, "grid m", &mut iso.1.spacing, 0.1, 10.0);
            });
        });
    }

    /// The colours of the two layers.
    fn colors_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        ui.horizontal(|ui| {
            ui.label("Colours:");
            for (k, name) in [(0, "grow-in points"), (1, "danger trees")] {
                let c = model.vegetation_colors[k];
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(18.0, 14.0), egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, 3.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                let label = format!("colour of the {name}");
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label)
                });
                if response
                    .on_hover_text(format!("The colour of the {name} (click to choose)"))
                    .clicked()
                {
                    self.color_tool = Some((
                        k,
                        crate::color_tool::ColorTool::new(format!("Colour of the {name}"), Some(c)),
                    ));
                }
                ui.label(name);
            }
        });
        help::explain(
            ui,
            "Drawn while this panel is open (View › Layers: Vegetation).",
            "Every grow-in point and every danger tree found are drawn in the 3D view, the plan \
             and the profile while the panel is open; closing the panel takes them off the \
             views, reopening it (or a saved run) draws them again. The colours are kept with \
             this project's view settings.",
        );
    }

    /// Run, its status, saving.
    fn run_ui(&mut self, ui: &mut egui::Ui, model: &Model, out: &mut Out) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.job.running(), egui::Button::new("Check"))
                .on_hover_text(
                    "Solve the cases, read the vegetation points again and check every span",
                )
                .clicked()
            {
                self.start(model, ui.ctx());
            }
            self.job.status_ui(ui);
            ui.separator();
            ui.label("save as");
            ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(160.0));
            let ready = self.report().is_some() && model.editing.is_ok();
            let label = if self.overwrite.as_deref() == Some(self.save_name.trim()) {
                "Replace the saved run"
            } else {
                "Save in Performed checks"
            };
            if ui
                .add_enabled(ready, egui::Button::new(label))
                .on_hover_text(
                    "Keep this run (its settings, cases, results and every point found) in \
                     Checks › Performed checks; reopened from there",
                )
                .on_disabled_hover_text("Run the check first (in a project you may edit)")
                .clicked()
            {
                let name = self.save_name.trim().to_owned();
                let exists = model.session().is_some_and(|s| s.report(&name).is_some());
                if name.is_empty() {
                    self.save_message = Some(("give the run a name".into(), true));
                } else if exists && self.overwrite.as_deref() != Some(name.as_str()) {
                    self.overwrite = Some(name.clone());
                    self.save_message = Some((
                        format!("\"{name}\" exists: click again to replace it"),
                        true,
                    ));
                } else {
                    match self.to_check_report(model, &name) {
                        Ok(r) => {
                            out.apply = Some((format!("saved the vegetation run {name}"), r.put()));
                            self.overwrite = None;
                            self.save_message = Some((format!("saved as \"{name}\""), false));
                            if let Some(Ok(rep)) = &mut self.job.result {
                                rep.saved = Some(name);
                            }
                        }
                        Err(e) => self.save_message = Some((e, true)),
                    }
                }
            }
        });
        if let Some((m, error)) = &self.save_message {
            crate::menu::message(
                ui,
                egui::RichText::new(m).small(),
                error.then_some(ui.visuals().warn_fg_color),
            );
        }
    }
}

/// The results: totals, exports, a table of the spans with findings, the
/// worst points.
fn report_ui(ui: &mut egui::Ui, report: &Report) -> Option<Selection> {
    let mut go = None;
    if let Some(s) = &report.saved {
        ui.label(
            egui::RichText::new(format!("saved run \"{s}\""))
                .small()
                .weak(),
        );
    }
    egui::ScrollArea::vertical()
        .id_salt("vegetation-notes")
        .max_height(60.0)
        .show(ui, |ui| {
            for n in &report.notes {
                ui.label(egui::RichText::new(n).small().weak());
            }
        });
    ui.horizontal(|ui| {
        let (enc, fall) = report
            .spans
            .iter()
            .fold((0, 0), |(e, f), r| (e + r.encroaching, f + r.fall_in));
        ui.label(format!(
            "{} vegetation points, {} positions; grow-in {enc}, fall-in {fall} — {} grow-in \
             points and {} danger trees drawn",
            report.points,
            report.positions,
            report.grow.len(),
            report.fall.len()
        ));
        if ui.button("Export CSV…").clicked() {
            save_csv("danger-trees.csv", &report.csv());
        }
        if let Some((lines, _)) = &report.lines
            && ui
                .button(format!("Export {} lines (CSV)…", lines.len()))
                .clicked()
        {
            save_csv(
                "isoclearance.csv",
                &ohl_clearance::isoclearance::to_csv(lines),
            );
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("vegetation-rows")
        .max_height(320.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("vegetation-rows")
                .striped(true)
                .spacing([14.0, 3.0])
                .show(ui, |ui| {
                    for h in [
                        "span",
                        "c m",
                        "grow-in",
                        "fall-in",
                        "nearest m",
                        "case, position",
                        "fall margin m",
                        "case, position",
                    ] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for r in report
                        .spans
                        .iter()
                        .filter(|r| r.encroaching > 0 || r.fall_in > 0)
                    {
                        if ui
                            .link(format!("{} span {}", r.section, r.span))
                            .on_hover_text("Go to this span")
                            .clicked()
                        {
                            go = Some(Selection::Span {
                                section: r.section.clone(),
                                span: r.span,
                            });
                        }
                        let error = ui.visuals().error_fg_color;
                        let red = |n: u64| {
                            let t = egui::RichText::new(n.to_string()).monospace();
                            if n > 0 { t.color(error) } else { t }
                        };
                        ui.label(format!("{:.2}", r.required));
                        ui.label(red(r.encroaching));
                        ui.label(red(r.fall_in));
                        ui.label(or_dash(r.nearest.map(|v| v.0), 2));
                        ui.label(r.nearest.map_or("-", |v| report.label(v.1)));
                        ui.label(or_dash(r.worst_fall.map(|v| v.0), 2));
                        ui.label(r.worst_fall.map_or("-", |v| report.label(v.1)));
                        ui.end_row();
                    }
                });
        });
    let worst: usize = report.spans.iter().map(|r| r.worst.len()).sum();
    egui::CollapsingHeader::new(format!("Worst points ({worst})"))
        .id_salt("vegetation-worst")
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("vegetation-worst-rows")
                .max_height(240.0)
                .show(ui, |ui| {
                    egui::Grid::new("vegetation-worst-grid")
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["span", "class", "x, y, z", "margin m", "case, position"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for r in &report.spans {
                                for w in &r.worst {
                                    ui.label(format!("{} span {}", r.section, r.span));
                                    ui.label(w.class.to_string());
                                    ui.label(format!(
                                        "{:.2}, {:.2}, {:.2}",
                                        w.point[0], w.point[1], w.point[2]
                                    ));
                                    ui.label(format!("{:.2}", w.margin));
                                    ui.label(report.label(w.label));
                                    ui.end_row();
                                }
                            }
                        });
                });
        });
    go
}

/// What the application does with the panel's [`Out`].
pub fn handle(r: &mut crate::app::Ready, out: Out) {
    if let Some(target) = out.go {
        r.go_to(target);
    }
    if let Some(colors) = out.colors {
        r.model.vegetation_colors = colors;
    }
    if let Some((what, command)) = out.apply {
        r.perform(crate::edit::Action::Command { what, command });
    }
    if let Some(anchor) = out.guide {
        r.status = Some(match crate::help::open_section(GUIDE, anchor) {
            Ok(m) => (format!("user guide opened: {m}"), false),
            Err(e) => (e, true),
        });
    }
    if let Some((plugin, p)) = out.preset {
        let mut text = format!("preset {plugin} {} applied to the vegetation check", p.name);
        if let Some(wc) = &p.weather {
            match crate::rule_editor::preset_cases(r, &plugin, &p.name, &p.weather_text, wc) {
                Ok(m) if !m.is_empty() => text = format!("{text}; {m}"),
                Ok(_) => {}
                Err(e) => text = format!("{text}; weather cases: {e}"),
            }
        }
        r.status = Some((text, false));
    }
}

/// The saved form of a run ([`CHECK_ID`] in the performed checks): its
/// settings and cases in the parameters, its per-span results, worst
/// points and every point found (packed) in the results.
pub mod saved {
    use super::*;

    /// Points packed ([`ohl_session::packing`]): their count, x, y, z in
    /// mm (each the difference to the one before), a danger tree's base
    /// height likewise, then each label.
    fn pack_points(list: &[FoundPoint], with_base: bool) -> Result<String, String> {
        let mm = |v: f64| (v * 1000.0).round() as i64;
        let mut w = Writer::default();
        w.u(list.len() as u64);
        for k in 0..3 {
            w.deltas(list.iter().map(|p| mm(p.point[k])));
        }
        if with_base {
            w.deltas(list.iter().map(|p| mm(p.base.unwrap_or(p.point)[2])));
        }
        for p in list {
            w.u(u64::from(p.label));
        }
        ohl_session::packing::pack(&w.0)
    }

    fn unpack_points(text: &str, with_base: bool) -> Result<Vec<FoundPoint>, String> {
        let raw = ohl_session::packing::unpack(text, 1 << 31)?;
        let mut r = Reader(&raw);
        let n = usize::try_from(r.u()?).map_err(|e| e.to_string())?;
        if n > raw.len() {
            return Err(format!("{n} points in {} bytes", raw.len()));
        }
        let m = |v: i64| v as f64 / 1000.0;
        let xs = r.deltas(n)?;
        let ys = r.deltas(n)?;
        let zs = r.deltas(n)?;
        let bases = if with_base { Some(r.deltas(n)?) } else { None };
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let point = [m(xs[i]), m(ys[i]), m(zs[i])];
            out.push(FoundPoint {
                point,
                base: bases.as_ref().map(|b| [point[0], point[1], m(b[i])]),
                label: u32::try_from(r.u()?).map_err(|e| e.to_string())?,
            });
        }
        if !r.done() {
            return Err("bytes left over".into());
        }
        Ok(out)
    }

    /// The results of a report.
    pub fn results(report: &Report) -> Result<Value, String> {
        let point = |p: [f64; 3]| json!([p[0], p[1], p[2]]);
        let spans: Vec<Value> = report
            .spans
            .iter()
            .map(|s| {
                json!({
                    "section": s.section,
                    "span": s.span,
                    "required": s.required,
                    "encroaching": s.encroaching,
                    "fall_in": s.fall_in,
                    "nearest": s.nearest.map(|v| json!({"distance": v.0, "label": v.1})),
                    "worst_fall": s.worst_fall.map(|v| json!({"margin": v.0, "label": v.1})),
                    "worst": s.worst.iter().map(|w| json!({
                        "point": point(w.point),
                        "class": w.class,
                        "distance": w.distance,
                        "height": w.height,
                        "fall_margin": w.fall_margin,
                        "margin": w.margin,
                        "label": w.label,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(json!({
            "version": SAVED_VERSION,
            "points": report.points,
            "positions": report.positions,
            "notes": report.notes,
            "labels": report.labels,
            "spans": spans,
            "grow_in": pack_points(&report.grow, false)?,
            "fall_in": pack_points(&report.fall, true)?,
        }))
    }

    /// The report and the parameters of a saved run.
    pub fn read(r: &CheckReport) -> Result<(Report, Value), String> {
        if r.check != CHECK_ID {
            return Err(format!("\"{}\" is not a vegetation run", r.id));
        }
        let v = &r.results;
        let version = v.get("version").and_then(Value::as_u64).unwrap_or(1);
        if version > u64::from(SAVED_VERSION) {
            return Err(format!(
                "saved run \"{}\": version {version} of a newer program",
                r.id
            ));
        }
        let (settings, cases) = Settings::from_json(&r.parameters)?;
        let f = |x: &Value| x.as_f64();
        let p3 = |x: &Value| -> Option<[f64; 3]> {
            let a = x.as_array()?;
            Some([
                a.first()?.as_f64()?,
                a.get(1)?.as_f64()?,
                a.get(2)?.as_f64()?,
            ])
        };
        let label = |x: &Value| x.get("label").and_then(Value::as_u64).unwrap_or(u64::MAX) as u32;
        let strings = |k: &str| -> Vec<String> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|x| x.as_str().unwrap_or_default().to_owned())
                        .collect()
                })
                .unwrap_or_default()
        };
        let spans = v
            .get("spans")
            .and_then(Value::as_array)
            .ok_or("no spans")?
            .iter()
            .map(|s| SpanRow {
                section: s["section"].as_str().unwrap_or_default().to_owned(),
                span: s["span"].as_u64().unwrap_or(0) as usize,
                required: s["required"].as_f64().unwrap_or(0.0),
                encroaching: s["encroaching"].as_u64().unwrap_or(0),
                fall_in: s["fall_in"].as_u64().unwrap_or(0),
                nearest: s
                    .get("nearest")
                    .filter(|x| !x.is_null())
                    .map(|x| (x["distance"].as_f64().unwrap_or(f64::NAN), label(x))),
                worst_fall: s
                    .get("worst_fall")
                    .filter(|x| !x.is_null())
                    .map(|x| (x["margin"].as_f64().unwrap_or(f64::NAN), label(x))),
                worst: s["worst"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|w| {
                                Some(WorstPoint {
                                    point: p3(&w["point"])?,
                                    class: w["class"].as_u64().unwrap_or(0) as u8,
                                    distance: f(&w["distance"]),
                                    height: f(&w["height"]),
                                    fall_margin: f(&w["fall_margin"]),
                                    margin: w["margin"].as_f64().unwrap_or(f64::NAN),
                                    label: label(w),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();
        let points = |k: &str, base: bool| -> Result<Vec<FoundPoint>, String> {
            match v.get(k).and_then(Value::as_str) {
                Some(t) => unpack_points(t, base).map_err(|e| format!("{k}: {e}")),
                None => Ok(Vec::new()),
            }
        };
        let report = Report {
            spans,
            grow: points("grow_in", false)?,
            fall: points("fall_in", true)?,
            labels: strings("labels"),
            points: v.get("points").and_then(Value::as_u64).unwrap_or(0),
            positions: v.get("positions").and_then(Value::as_u64).unwrap_or(0) as usize,
            notes: strings("notes"),
            settings,
            cases,
            lines: None,
            saved: Some(r.id.clone()),
        };
        Ok((report, r.parameters.clone()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Every point and label read back to the millimetre.
        #[test]
        fn points_pack_and_read_back() {
            let grow: Vec<FoundPoint> = (0..2000)
                .map(|i| FoundPoint {
                    point: [
                        2_600_000.0 + f64::from(i) * 0.731,
                        1_200_000.0 - f64::from(i % 17),
                        400.123 + f64::from(i % 5),
                    ],
                    base: None,
                    label: (i % 7) as u32,
                })
                .collect();
            let text = pack_points(&grow, false).unwrap();
            assert!(text.len() < 12 * grow.len(), "{}", text.len());
            let back = unpack_points(&text, false).unwrap();
            for (a, b) in back.iter().zip(&grow) {
                for k in 0..3 {
                    assert!((a.point[k] - b.point[k]).abs() < 0.0005);
                }
                assert_eq!(a.label, b.label);
            }
            let fall: Vec<FoundPoint> = grow
                .iter()
                .map(|p| FoundPoint {
                    base: Some([p.point[0], p.point[1], p.point[2] - 15.5]),
                    ..*p
                })
                .collect();
            let text = pack_points(&fall, true).unwrap();
            let back = unpack_points(&text, true).unwrap();
            assert_eq!(back.len(), fall.len());
            assert!(back.iter().all(|p| p.base.is_some()));
        }
    }
}
