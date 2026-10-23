// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! File › Export drawings: plan and profile sheets and profile exports of
//! the routes (SVG, DXF, PDF, PNG, CSV, XML) at chosen scales and station
//! range, written in the background (a progress bar; the views stay
//! usable). The same files as `ohl drawing`.
//!
//! The window's settings (formats, scales, sheets, line width, break
//! clearance, options, the sheet preview on or off) are kept in the
//! project, per project line ([`SETTING`]): there when the project is
//! opened again; a project without them opens with the defaults. The
//! **line width** is one setting for the drawings and the profile view:
//! what is shown along the line in both. The sheet template, its fields,
//! the revisions and the other versions drawn are kept with them
//! ([`crate::sheet_ui`]); a sheet is previewed as the export draws it.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ohl_drawing::export::{Export, Format};
use ohl_line::jumper::JumperShape;
use ohl_line::solve::SectionShape;
use ohl_line::{AlignmentView, Line};
use ohl_terrain::Terrain;
use serde::{Deserialize, Serialize};

use crate::model::Model;
use crate::sheet_ui::{PluginTemplates, SheetChoice};
use crate::survey_io::Job;

/// The sheet choices other than a paper size ("" = one continuous
/// drawing, "template": the sheet template's, "optimised": the size and
/// orientation chosen from the ticked sizes).
const SPECIAL_PAGES: [&str; 3] = ["", "template", "optimised"];

/// Whether `page` is a sheet choice the window knows.
fn known_page(page: &str) -> bool {
    SPECIAL_PAGES.contains(&page) || ohl_drawing::page_format(page).is_some()
}

/// The orientations the optimiser may choose ([`DrawingsWindow::paper_orientation`]).
const ORIENTATIONS: [(&str, &str); 3] = [
    ("both", "landscape or portrait"),
    ("landscape", "landscape only"),
    ("portrait", "portrait only"),
];

/// The Export drawings window.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawingsWindow {
    /// Route by index; `None`: every route.
    pub route: Option<usize>,
    /// Formats ticked.
    pub formats: Vec<Format>,
    /// Sheet format, or "" for one continuous drawing.
    pub page: String,
    /// The paper sizes offered and the optimiser's choice
    /// ([`ohl_drawing::paper::PAPER_SIZES`] names).
    pub paper_sizes: Vec<String>,
    /// The optimiser may put a route's sheets on different sizes of one
    /// standard.
    pub mixed_sizes: bool,
    /// The optimiser puts every route on the same paper.
    pub same_paper: bool,
    /// The orientations the optimiser may choose: `both`, `landscape`,
    /// `portrait`.
    pub paper_orientation: String,
    /// The optimiser's candidates for the first route, as last compared
    /// (lines of text, or why not).
    pub paper_report: Option<Result<Vec<String>, String>>,
    /// Horizontal scale denominator.
    pub hscale: f64,
    /// Vertical scale denominator.
    pub vscale: f64,
    /// Only a station range.
    pub range: bool,
    /// Its first and last station, m.
    pub from: f64,
    /// Last station, m.
    pub to: f64,
    /// The profile alone.
    pub profile_only: bool,
    /// Required-clearance line above the ground, m.
    pub clearance: Option<f64>,
    /// The other solved weather cases dashed (else the case shown only).
    pub all_cases: bool,
    /// PNG resolution, dpi.
    pub dpi: f64,
    /// Line width: half width of the plan, m — what is shown along the
    /// line in the drawings and in the profile view (one setting).
    pub line_width: f64,
    /// Clearance on paper between a sheet break and every other
    /// structure's drawing, mm.
    pub break_clearance: f64,
    /// The structures' elevations on the plan (off by default).
    pub elevations: ohl_drawing::Elevations,
    /// The isoclearance lines of the last Checks › Vegetation run, drawn
    /// on the plan (layer OHL_ISOCLEARANCE): (clearance, points), the top
    /// line.
    pub isoclearance: Option<Arc<IsoLines>>,
    /// Side ground profiles at these offsets, m (right positive).
    pub side_offsets: Vec<f64>,
    /// Scale of the plan across the line (none: the horizontal scale).
    pub plan_scale: Option<f64>,
    /// Logos of the title block: file and width on paper, mm.
    pub logos: Vec<(PathBuf, f64)>,
    /// The clearance line from the project's ground rule (when there is
    /// one), else [`DrawingsWindow::clearance`].
    pub clearance_rule: bool,
    /// The violations of the last clearance check drawn as markers.
    pub markers_on: bool,
    /// The project's clearance rules for the ground (Checks › Ground
    /// clearances, else the rule file chosen), taken when the window
    /// opens.
    pub ground_rules: Arc<Vec<ohl_clearance::Rule>>,
    /// The violations of the last clearance check (Checks › Clearances):
    /// point and label, taken when the window opens.
    pub markers: Arc<Vec<([f64; 3], String)>>,
    /// The sheet template and what goes with it.
    pub sheet: SheetChoice,
    /// The plugins' templates (read in the background).
    pub plugin_templates: PluginTemplates,
    /// The sheet shown in the window's preview (none: no preview).
    pub sheet_preview: Option<usize>,
    /// The sheet number the preview shows when ticked (kept while off).
    pub preview_sheet: usize,
    /// The route the sheet preview shows, by id (none: the first route
    /// drawn).
    pub preview_route: Option<String>,
    /// Output folder.
    pub out: Option<PathBuf>,
    /// Why it cannot start.
    pub error: Option<String>,
}

impl Default for DrawingsWindow {
    fn default() -> Self {
        Self {
            route: None,
            formats: vec![Format::Pdf],
            page: "A3".into(),
            paper_sizes: ohl_drawing::paper::default_sizes(),
            mixed_sizes: false,
            same_paper: false,
            paper_orientation: "both".into(),
            paper_report: None,
            hscale: 2000.0,
            vscale: 200.0,
            range: false,
            from: 0.0,
            to: 1000.0,
            profile_only: false,
            clearance: None,
            all_cases: false,
            dpi: 150.0,
            line_width: DEFAULT_LINE_WIDTH,
            break_clearance: ohl_drawing::BREAK_CLEARANCE_MM,
            elevations: ohl_drawing::Elevations::default(),
            isoclearance: None,
            side_offsets: Vec::new(),
            plan_scale: None,
            logos: Vec::new(),
            clearance_rule: true,
            markers_on: true,
            ground_rules: Arc::default(),
            markers: Arc::default(),
            sheet: SheetChoice::default(),
            plugin_templates: Arc::default(),
            sheet_preview: None,
            preview_sheet: 1,
            preview_route: None,
            out: None,
            error: None,
        }
    }
}

/// The required vertical clearance to the ground of `rules` for `line`:
/// the largest of its ground rules over the line's voltages, with the
/// rule's name (none: no ground rule with a vertical distance).
pub fn ground_rule_clearance(rules: &[ohl_clearance::Rule], line: &Line) -> Option<(f64, String)> {
    let kvs = ohl_clearance::zone::line_voltages(line);
    rules
        .iter()
        .filter(|r| r.target == ohl_clearance::Target::Ground)
        .filter_map(|r| Some((r.vertical.as_ref()?.largest(&kvs), r.name.clone())))
        .filter(|(d, _)| d.is_finite() && *d > 0.0)
        .max_by(|a, b| a.0.total_cmp(&b.0))
}

/// Isoclearance lines: (clearance m, points on the ground), and the top
/// line (for the colours).
pub type IsoLines = (Vec<(f64, Vec<[f64; 3]>)>, f64);

/// The line width by default, m (the drawings' plan half width).
pub const DEFAULT_LINE_WIDTH: f64 = 40.0;

/// The elevations' base offsets accepted, mm on paper.
pub const ELEVATION_OFFSETS: std::ops::RangeInclusive<f64> = 0.0..=500.0;

/// The line widths accepted, m.
pub const LINE_WIDTHS: std::ops::RangeInclusive<f64> = 1.0..=2000.0;

/// The project setting keeping the drawing settings: one per project
/// line, `app.drawings.<line>` ([`setting_key`]), a [`DrawingSettings`]
/// as JSON. Like the project's other settings it is not undone and
/// follows everyone on a server (a designer changes it).
pub const SETTING: &str = "app.drawings";

/// The key of project line `line`'s drawing settings.
pub fn setting_key(line: &str) -> String {
    format!("{SETTING}.{line}")
}

/// The drawing settings kept in the project ([`SETTING`]): the export
/// window's, without the route, the output folder (this computer's) and
/// the message. Missing values take the defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawingSettings {
    /// Formats ticked (`svg`, `dxf`, `pdf`, `png`, `csv`, `xml`).
    pub formats: Vec<String>,
    /// Sheet format, or "" for one continuous drawing.
    pub page: String,
    /// The paper sizes ticked (offered, and the optimiser's choice).
    pub paper_sizes: Vec<String>,
    /// The optimiser may mix sizes of one standard on a route.
    pub mixed_sizes: bool,
    /// The optimiser puts every route on the same paper.
    pub same_paper: bool,
    /// The orientations the optimiser may choose.
    pub paper_orientation: String,
    /// Horizontal scale denominator.
    pub hscale: f64,
    /// Vertical scale denominator.
    pub vscale: f64,
    /// Only a station range.
    pub range: bool,
    /// Its first station, m.
    pub from: f64,
    /// Its last station, m.
    pub to: f64,
    /// The profile alone.
    pub profile_only: bool,
    /// Required-clearance line above the ground, m.
    pub clearance: Option<f64>,
    /// The other solved weather cases dashed.
    pub all_cases: bool,
    /// PNG resolution, dpi.
    pub dpi: f64,
    /// Line width (half width), m: the drawings' and the profile's.
    pub line_width: f64,
    /// Sheet break clearance on paper, mm.
    pub break_clearance: f64,
    /// The structures' elevations on the plan: `off`, `above` or `below`.
    pub elevations: String,
    /// From the line to the elevations' base, mm on paper.
    pub elevation_offset: f64,
    /// Angle between the line and the elevations' height, degrees.
    pub elevation_rotation: f64,
    /// The sheet preview on.
    pub preview: bool,
    /// Side ground profiles at these offsets, m.
    pub side_offsets: Vec<f64>,
    /// Scale of the plan across the line (none: the horizontal scale).
    pub plan_scale: Option<f64>,
    /// Logos of the title block: file and width, mm.
    pub logos: Vec<LogoSetting>,
    /// The clearance line from the project's ground rule.
    pub clearance_rule: bool,
    /// The last clearance check's violations as markers.
    pub markers: bool,
    /// The sheet template, its fields, the revisions, the versions drawn.
    pub sheet: SheetChoice,
}

/// A logo of the drawing settings: its file and width on paper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogoSetting {
    /// The image file (SVG, PNG or JPEG) on this computer.
    pub file: PathBuf,
    /// Width on paper, mm.
    pub width: f64,
}

impl Default for DrawingSettings {
    fn default() -> Self {
        Self::of(&DrawingsWindow::default(), false)
    }
}

impl DrawingSettings {
    /// The settings of window `w`, the preview on or not.
    pub fn of(w: &DrawingsWindow, preview: bool) -> Self {
        Self {
            formats: w.formats.iter().map(|f| f.name().to_owned()).collect(),
            page: w.page.clone(),
            paper_sizes: w.paper_sizes.clone(),
            mixed_sizes: w.mixed_sizes,
            same_paper: w.same_paper,
            paper_orientation: w.paper_orientation.clone(),
            hscale: w.hscale,
            vscale: w.vscale,
            range: w.range,
            from: w.from,
            to: w.to,
            profile_only: w.profile_only,
            clearance: w.clearance,
            all_cases: w.all_cases,
            dpi: w.dpi,
            line_width: w.line_width,
            break_clearance: w.break_clearance,
            elevations: w.elevations.place.key().to_owned(),
            elevation_offset: w.elevations.offset_mm,
            elevation_rotation: w.elevations.rotation_deg,
            preview,
            side_offsets: w.side_offsets.clone(),
            plan_scale: w.plan_scale,
            logos: w
                .logos
                .iter()
                .map(|(file, width)| LogoSetting {
                    file: file.clone(),
                    width: *width,
                })
                .collect(),
            clearance_rule: w.clearance_rule,
            markers: w.markers_on,
            sheet: w.sheet.clone(),
        }
    }

    /// Puts them into window `w` (its route, folder and message kept);
    /// a value out of range (a file edited by hand) takes the default.
    pub fn apply(&self, w: &mut DrawingsWindow) {
        let d = DrawingsWindow::default();
        let ok = |v: f64, range: std::ops::RangeInclusive<f64>, default: f64| {
            if range.contains(&v) { v } else { default }
        };
        w.formats = Format::ALL
            .into_iter()
            .filter(|f| self.formats.iter().any(|n| n == f.name()))
            .collect();
        w.page = if known_page(&self.page) {
            self.page.clone()
        } else {
            d.page.clone()
        };
        let sizes: Vec<String> = self
            .paper_sizes
            .iter()
            .filter(|n| ohl_drawing::page_format(n).is_some())
            .cloned()
            .collect();
        w.paper_sizes = if sizes.is_empty() {
            d.paper_sizes
        } else {
            sizes
        };
        w.mixed_sizes = self.mixed_sizes;
        w.same_paper = self.same_paper;
        w.paper_orientation = if ORIENTATIONS
            .iter()
            .any(|(k, _)| *k == self.paper_orientation)
        {
            self.paper_orientation.clone()
        } else {
            d.paper_orientation
        };
        w.hscale = ok(self.hscale, 10.0..=100_000.0, d.hscale);
        w.vscale = ok(self.vscale, 10.0..=100_000.0, d.vscale);
        w.range = self.range;
        w.from = if self.from.is_finite() {
            self.from
        } else {
            d.from
        };
        w.to = if self.to.is_finite() { self.to } else { d.to };
        w.profile_only = self.profile_only;
        w.clearance = self.clearance.filter(|c| c.is_finite() && *c >= 0.0);
        w.all_cases = self.all_cases;
        w.dpi = ok(self.dpi, 10.0..=2400.0, d.dpi);
        w.line_width = ok(self.line_width, LINE_WIDTHS, d.line_width);
        w.break_clearance =
            ohl_drawing::check_break_clearance(self.break_clearance).unwrap_or(d.break_clearance);
        w.elevations = ohl_drawing::Elevations {
            place: ohl_drawing::ElevationPlace::parse(&self.elevations)
                .unwrap_or(d.elevations.place),
            offset_mm: ok(
                self.elevation_offset,
                ELEVATION_OFFSETS,
                d.elevations.offset_mm,
            ),
            rotation_deg: ok(
                self.elevation_rotation,
                -180.0..=180.0,
                d.elevations.rotation_deg,
            ),
            ..d.elevations
        };
        w.side_offsets = self
            .side_offsets
            .iter()
            .copied()
            .filter(|o| o.is_finite() && o.abs() <= *LINE_WIDTHS.end())
            .collect();
        w.plan_scale = self.plan_scale.filter(|s| (10.0..=100_000.0).contains(s));
        w.logos = self
            .logos
            .iter()
            .filter(|l| (1.0..=500.0).contains(&l.width))
            .map(|l| (l.file.clone(), l.width))
            .collect();
        w.clearance_rule = self.clearance_rule;
        w.markers_on = self.markers;
        w.sheet = self.sheet.clone();
        if w.sheet.template.is_empty() {
            w.sheet.template = "default".into();
        }
        w.sheet.corridor = w.sheet.corridor.filter(|c| (0.0..=10_000.0).contains(c));
        w.sheet.elevation_scale = w
            .sheet
            .elevation_scale
            .filter(|s| (10.0..=100_000.0).contains(s));
    }
}

/// The drawings window, the export it started and the sheet preview.
#[derive(Debug, Default)]
pub struct Drawings {
    /// The window, when open.
    pub window: Option<DrawingsWindow>,
    /// The settings of the window when it was last closed (for this
    /// session: the next window starts from them, the preview uses them).
    pub last: DrawingsWindow,
    /// The export running (or finished, until closed).
    pub job: Option<Job>,
    /// Show the export's sheets over the profile and the plan (the sheet
    /// preview; off by default).
    pub preview_on: bool,
    /// The preview made, for what (the settings and the case).
    preview: Option<(PreviewKey, Result<Preview, String>)>,
    /// The settings as last read from or written to the project: the
    /// setting's key and the JSON ([`Drawings::save`] writes them again
    /// when they differ).
    saved: Option<(String, String)>,
    /// The plugins giving sheet templates (name, folder), as last given.
    template_plugins: Vec<(String, PathBuf)>,
    /// Their templates being read.
    template_job: Option<std::sync::mpsc::Receiver<PluginTemplates>>,
    /// The sheet previewed in the window: for what, the image (or why
    /// not), the sheets' count and warnings.
    sheet_image: Option<(SheetKey, Result<SheetImage, String>)>,
    /// Settings changed since the sheet image: what, and since when (it
    /// is drawn again once they settle, [`PREVIEW_SETTLE`]).
    sheet_pending: Option<(SheetKey, std::time::Instant)>,
    /// Likewise for the sheet preview over the views.
    preview_pending: Option<(PreviewKey, std::time::Instant)>,
    /// The sheet being drawn in the background: for what, and where its
    /// pixels come.
    sheet_job: Option<(
        SheetKey,
        std::sync::mpsc::Receiver<Result<SheetPixels, String>>,
    )>,
}

/// A sheet previewed, for the tests: the route's sheet count, the
/// sheet's warnings and its number.
#[cfg(test)]
pub type SheetPreviewed = (usize, Vec<String>, usize);

/// What a sheet preview was made for: the settings (without the sheet
/// and route shown), the case, the route and the sheet.
type SheetKey = (DrawingsWindow, usize, Option<String>, usize);

/// How long the settings must stay unchanged before a preview is drawn
/// again (typing in a field draws once, not at every key).
pub(crate) const PREVIEW_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

/// A sheet drawn for the window.
pub struct SheetImage {
    /// On screen.
    pub texture: egui::TextureHandle,
    /// Its size, mm.
    pub size: [f64; 2],
    /// How many sheets its route has.
    pub sheets: usize,
    /// The sheet shown (1-based; the last when one past it was asked).
    pub number: usize,
    /// Its route.
    pub route: String,
    /// The routes the export draws (to choose from).
    pub routes: Vec<String>,
    /// The sheet's warnings.
    pub warnings: Vec<String>,
}

impl std::fmt::Debug for SheetImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SheetImage")
            .field("size", &self.size)
            .field("sheets", &self.sheets)
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

/// What a preview was made for: the settings that shape the sheets and
/// the weather case shown.
type PreviewKey = (DrawingsWindow, usize);

/// The export's sheets of one alignment, laid out by the export's own
/// code ([`ohl_drawing::sheet_layout`]).
#[derive(Debug, Clone)]
pub struct PreviewRoute {
    /// The alignment (a route of the export).
    pub id: String,
    /// Its centreline (for the plan).
    pub centreline: Vec<[f64; 3]>,
    /// Its sheets: stations, structures at the ends, warnings.
    pub sheets: Vec<ohl_drawing::SheetLayout>,
    /// The structures' elevations the sheets' plan carries (none when
    /// off), in the route's (station, offset).
    pub elevations: Vec<ohl_drawing::ElevationDrawing>,
    /// The lines below the lowest conductor the sheets' profile carries
    /// (none when not asked for), by station and height.
    pub curves: Vec<ohl_drawing::curves::Curve>,
}

/// The export's sheets, as File › Export › Drawings… would write them now.
#[derive(Debug, Clone)]
pub struct Preview {
    /// The sheets' paper in words (none: the export writes one continuous
    /// drawing).
    pub paper: Option<String>,
    /// The sheets of every alignment.
    pub routes: Vec<PreviewRoute>,
    /// The sheets have a plan above the profile.
    pub plan: bool,
    /// The line width (half width of the plan), m.
    pub line_width: f64,
}

impl Preview {
    /// The export's warnings, as the export writes them.
    pub fn warnings(&self) -> Vec<String> {
        self.routes
            .iter()
            .flat_map(|r| {
                r.sheets.iter().flat_map(move |s| {
                    s.warnings
                        .iter()
                        .map(move |w| format!("route {}: sheet {}: {w}", r.id, s.number))
                })
            })
            .collect()
    }

    /// The sheets of alignment `id`.
    pub fn route(&self, id: &str) -> Option<&PreviewRoute> {
        self.routes.iter().find(|r| r.id == id)
    }
}

/// The sheets the export would write with settings `w` for `model`
/// showing case `case`, laid out by the export's own code.
///
/// Expected cost: what the export takes to collect the drawing data
/// (under a second per route), without writing anything.
pub fn preview(model: &Model, case: usize, w: &DrawingsWindow) -> Result<Preview, String> {
    let every = DrawingsWindow {
        route: None,
        ..w.clone()
    };
    let input = Input::prepare(model, case, &every, PathBuf::new())?;
    let options = input.route_options();
    let export = input.common_export(&options)?;
    let mut routes = Vec::new();
    let mut papers = Vec::new();
    if export.paged() {
        for route in &input.routes {
            let d = input.drawing(route)?;
            let sheets = match (&export.optimise, &export.page) {
                (Some(opt), _) => {
                    let best = ohl_drawing::paper::best(&d, &options, opt)
                        .ok_or("no paper size to lay the sheets out on")?;
                    papers.push(format!("{}: {}", route.id, best.sizes()));
                    best.layout
                }
                (None, Some(page)) => ohl_drawing::sheet_layout(&d, &options, page),
                (None, None) => Vec::new(),
            };
            routes.push(PreviewRoute {
                id: route.id.clone(),
                sheets,
                elevations: ohl_drawing::elevations(&d, &options),
                curves: ohl_drawing::curves::curves(
                    &d,
                    &options.conductor_clearance,
                    &options.template.conductor_clearance.circuits,
                    &options.template.conductor_clearance.label,
                )
                .0,
                centreline: d.centreline,
            });
        }
    }
    let paper = match (&export.optimise, &export.page) {
        (Some(_), _) => Some(format!("optimised ({})", papers.join("; "))),
        (None, Some(p)) => Some(p.name.clone()),
        (None, None) => None,
    };
    Ok(Preview {
        paper,
        routes,
        plan: options.plan,
        line_width: options.plan_half_width,
    })
}

/// The key of a section's circuit for [`CircuitCases`]: its system's
/// name, "" for the earth wires (no system).
pub fn circuit_key(section: &ohl_line::Section) -> String {
    section.system.clone().unwrap_or_default()
}

/// The weather cases the sheets draw and take the stringing tension
/// from, per circuit key ([`circuit_key`]): model case indices (none:
/// the case shown, for the drawn ones; no stringing band data for the
/// others).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SheetCases {
    /// Drawn, per key.
    pub drawn: std::collections::BTreeMap<String, Option<usize>>,
    /// Stringing, per key.
    pub stringing: std::collections::BTreeMap<String, Option<usize>>,
}

impl SheetCases {
    /// The cases of `choice` (the window's, per circuit) over `template`'s
    /// `[cases]` in `model`; an error names a choice no case matches.
    pub fn of(
        model: &Model,
        template: &ohl_drawing::SheetTemplate,
        choice: &std::collections::BTreeMap<String, crate::sheet_ui::CircuitCases>,
    ) -> Result<Self, String> {
        let cases: Vec<(String, f64, bool)> = model
            .cases
            .iter()
            .map(|c| {
                let w = &c.case;
                (
                    w.name.clone(),
                    w.temperature.celsius(),
                    w.wind_pressure.si() == 0.0
                        && w.ice_thickness.si() == 0.0
                        && w.ice_load.is_none(),
                )
            })
            .collect();
        let pick = |c: Option<&String>, what: &str| -> Result<Option<usize>, String> {
            match c {
                None => Ok(None),
                Some(c) => ohl_drawing::template::CasesSpec::pick(c, &cases)
                    .map(Some)
                    .ok_or_else(|| {
                        format!(
                            "{what}: no weather case \"{c}\" in the project (a case's name, or a \
                             temperature with a still case without ice at it)"
                        )
                    }),
            }
        };
        let mut keys: Vec<String> = model.line.sections.iter().map(circuit_key).collect();
        keys.sort();
        keys.dedup();
        let t = &template.cases;
        let mut out = Self::default();
        for k in keys {
            let own = choice.get(&k);
            let name = if k.is_empty() {
                "earth wires".to_owned()
            } else {
                k.clone()
            };
            let default = if k.is_empty() {
                &t.earth_wires
            } else {
                &t.conductors
            };
            let drawn = own.and_then(|c| c.drawn.as_ref()).or(default.as_ref());
            let stringing = own
                .and_then(|c| c.stringing.as_ref())
                .or(t.stringing.as_ref());
            out.drawn
                .insert(k.clone(), pick(drawn, &format!("{name}, drawn"))?);
            out.stringing
                .insert(k, pick(stringing, &format!("{name}, stringing"))?);
        }
        Ok(out)
    }

    /// The cases to solve for them.
    pub fn needed(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .drawn
            .values()
            .chain(self.stringing.values())
            .flatten()
            .copied()
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Whether any circuit is drawn in a case other than the one shown.
    pub fn mixed(&self) -> bool {
        self.drawn.values().any(Option::is_some)
    }
}

/// What an export needs, taken from the model (sent to the background).
pub struct Input {
    /// The line.
    pub line: Line,
    /// Its structure types' bodies (levels 2 and 3: members drawn on their
    /// own layer).
    pub bodies: Arc<ohl_structures_fe::drawn::Bodies>,
    /// Routes to draw.
    pub routes: Vec<AlignmentView>,
    /// Solved cases: name, sections, jumpers; the first drawn solid.
    pub cases: Vec<(String, Vec<SectionShape>, Vec<JumperShape>)>,
    /// Ground surface.
    pub ground: Option<Arc<Terrain>>,
    /// Drawing options (without the station range: set per route).
    pub options: ohl_drawing::Options,
    /// Station range.
    pub range: Option<[f64; 2]>,
    /// Formats and sheets.
    pub export: Export,
    /// With the optimised layout: one paper for every route.
    pub same_paper: bool,
    /// Isoclearance lines on the plan.
    pub isoclearance: Option<Arc<IsoLines>>,
    /// Violation markers: point, label.
    pub markers: Arc<Vec<([f64; 3], String)>>,
    /// Other versions of the line drawn with it.
    pub versions: Vec<Version>,
    /// LiDAR points loaded of the classes drawn: (x, y, z), class (none
    /// unless the template draws them).
    pub lidar: Arc<Vec<([f64; 3], u8)>>,
    /// The stringing case(s) and the sections solved in them (the
    /// `stringing` band; none: no stringing case chosen).
    pub stringing: Option<(String, Vec<SectionShape>)>,
    /// Output folder.
    pub out: PathBuf,
}

/// Another version of the line drawn with it: a project line in its role,
/// solved in the cases drawn (name, sections, jumpers).
pub struct Version {
    /// The project line.
    pub name: String,
    /// Its role.
    pub role: String,
    /// Its label.
    pub label: String,
    /// Its line.
    pub line: Line,
    /// Solved in each case drawn.
    pub solved: Vec<(String, Vec<SectionShape>, Vec<JumperShape>)>,
}

impl Input {
    /// The export the window asks for, from `model` showing case `case`.
    pub fn new(model: &Model, case: usize, w: &DrawingsWindow) -> Result<Self, String> {
        let out = w.out.clone().ok_or("choose an output folder")?;
        if w.formats.is_empty() {
            return Err("tick a format".into());
        }
        Self::prepare(model, case, w, out)
    }

    /// The export of `w` into `out`, the folder and formats not checked.
    fn prepare(
        model: &Model,
        case: usize,
        w: &DrawingsWindow,
        out: PathBuf,
    ) -> Result<Self, String> {
        let break_clearance = ohl_drawing::check_break_clearance(w.break_clearance)?;
        if !(w.line_width.is_finite() && w.line_width > 0.0) {
            return Err(format!(
                "line width ±{} m: give a positive width",
                w.line_width
            ));
        }
        let all = model.line.alignment_views();
        let routes = match w.route {
            Some(k) => vec![all.get(k).ok_or("no such route")?.clone()],
            None => all,
        };
        if routes.is_empty() {
            return Err("the line has no route".into());
        }
        let template = crate::sheet_ui::resolve(model, &w.sheet.template, &w.plugin_templates)?;
        let mut optimise = None;
        let page = match w.page.as_str() {
            "" => None,
            "template" if template.optimise().is_some() => {
                optimise = template.optimise();
                None
            }
            "template" => Some(
                template
                    .page()
                    .ok_or("the sheet template gives no page format: choose one")?,
            ),
            "optimised" => {
                optimise = Some(w.optimise()?);
                None
            }
            p => Some(ohl_drawing::page_format(p).ok_or("unknown sheet format")?),
        };
        let page = page.map(|p| if w.sheet.portrait { p.portrait() } else { p });
        // The cases per circuit (the window's over the template's): the
        // first drawing's wires each in their circuit's case, and the
        // stringing band's tensions.
        let picks = SheetCases::of(model, &template, &w.sheet.circuit_cases)?;
        let solved_case = |k: usize| -> Result<&crate::model::Solved, String> {
            let c = model.cases.get(k).ok_or("no such weather case")?;
            if c.ready {
                Ok(c)
            } else {
                Err(format!(
                    "weather case \"{}\" is being solved: the sheets follow once it is",
                    c.case.name
                ))
            }
        };
        let key_of = |id: &str| {
            model
                .line
                .sections
                .iter()
                .find(|s| s.id == id)
                .map(circuit_key)
                .unwrap_or_default()
        };
        // A jumper's circuit: that of the section at its first wire end.
        let jumper_key = |id: &str| {
            let j = model.line.jumpers.iter().find(|j| j.id == id)?;
            let ohl_line::JumperEnd::Wire(p) = &j.from else {
                return None;
            };
            model
                .line
                .sections
                .iter()
                .find(|s| s.points.contains(p))
                .map(circuit_key)
        };
        let first_mixed = if picks.mixed() {
            let mut names: Vec<String> = Vec::new();
            let mut shapes = Vec::new();
            let mut swapped = std::collections::BTreeSet::new();
            for s in model.shapes_shown(case) {
                match picks.drawn.get(&key_of(&s.section)).copied().flatten() {
                    Some(_) if swapped.contains(&s.section) => {}
                    Some(k) if k != case => {
                        swapped.insert(s.section.clone());
                        let c = solved_case(k)?;
                        if !names.contains(&c.case.name) {
                            names.push(c.case.name.clone());
                        }
                        shapes.extend(
                            c.drawn()
                                .filter_map(|x| x.shapes.iter().find(|y| y.section == s.section))
                                .cloned(),
                        );
                    }
                    _ => {
                        let n = &model.cases[case].case.name;
                        if !names.contains(n) {
                            names.push(n.clone());
                        }
                        shapes.push(s);
                    }
                }
            }
            let mut jumpers = Vec::new();
            for j in &model.cases[case].jumpers {
                match jumper_key(&j.jumper).and_then(|k| picks.drawn.get(&k).copied().flatten()) {
                    Some(k) if k != case => jumpers.extend(
                        solved_case(k)?
                            .drawn()
                            .filter_map(|x| x.jumpers.iter().find(|y| y.jumper == j.jumper))
                            .cloned(),
                    ),
                    _ => jumpers.push(j.clone()),
                }
            }
            Some((names.join(" / "), shapes, jumpers))
        } else {
            None
        };
        let stringing = if picks.stringing.values().any(Option::is_some) {
            let mut names: Vec<String> = Vec::new();
            let mut shapes = Vec::new();
            for s in &model.line.sections {
                if let Some(k) = picks.stringing.get(&circuit_key(s)).copied().flatten() {
                    let c = solved_case(k)?;
                    if !names.contains(&c.case.name) {
                        names.push(c.case.name.clone());
                    }
                    shapes.extend(c.shapes.iter().find(|y| y.section == s.id).cloned());
                }
            }
            Some((names.join(" / "), shapes))
        } else {
            None
        };
        // The case shown first (solid), then the other solved ones.
        let mut order = vec![case];
        if w.all_cases {
            order.extend((0..model.cases.len()).filter(|&k| k != case && model.cases[k].ready));
        }
        // The case shown as the views show it (each section in its display
        // cases); every other case from its own solution, both wind sides
        // where both are shown.
        let cases: Vec<_> = order
            .into_iter()
            .enumerate()
            .filter_map(|(i, k)| {
                let solved = model.cases.get(k)?;
                if i == 0
                    && let Some(f) = &first_mixed
                {
                    return Some(f.clone());
                }
                let (shapes, jumpers) = if i == 0 {
                    (model.shapes_shown(k), solved.jumpers.clone())
                } else {
                    (
                        solved.drawn().flat_map(|c| c.shapes.clone()).collect(),
                        solved.drawn().flat_map(|c| c.jumpers.clone()).collect(),
                    )
                };
                Some((solved.case.name.clone(), shapes, jumpers))
            })
            .collect();
        let first = cases.first().ok_or("no solved weather case")?;
        let names: Vec<String> = cases
            .iter()
            .enumerate()
            .map(|(k, c)| match k {
                0 => format!("{} (solid)", c.0),
                _ => format!("{} ({})", c.0, ohl_drawing::case_dash_name(k)),
            })
            .collect();
        let title = vec![
            model.line.name.clone(),
            if names.len() > 1 {
                format!("Weather cases: {}", names.join(", "))
            } else {
                format!("Weather case: {}", first.0)
            },
        ];
        let logos = w
            .logos
            .iter()
            .map(|(file, width)| ohl_drawing::Logo::from_file(file, *width))
            .collect::<Result<Vec<_>, _>>()?;
        let clearance = match ground_rule_clearance(&w.ground_rules, &model.line) {
            Some((d, _)) if w.clearance_rule => Some(d),
            _ => w.clearance,
        };
        let s = &w.sheet;
        let conductor_clearance = if s.conductor_clearance || template.conductor_clearance.show {
            ohl_drawing::curves::conductor_clearances(&model.line, &w.ground_rules).0
        } else {
            Vec::new()
        };
        let line_name = model
            .session()
            .map_or(String::new(), |x| x.current_line().to_owned());
        let project = model.sources.file.file_name().map_or(String::new(), |n| {
            n.to_string_lossy()
                .split('.')
                .next()
                .unwrap_or_default()
                .to_owned()
        });
        // Other versions: project lines, solved in the cases drawn (those
        // shown next to the line already solved).
        let mut versions = Vec::new();
        for v in &s.versions {
            let solved: Vec<(String, Vec<SectionShape>, Vec<JumperShape>)> = match model
                .other_lines
                .iter()
                .find(|o| o.name == v.line)
            {
                Some(o) => cases
                    .iter()
                    .filter_map(|(name, _, _)| {
                        let k = model.cases.iter().position(|c| &c.case.name == name)?;
                        let c = o.cases.get(k)?;
                        Some((name.clone(), c.shapes.clone(), c.jumpers.clone()))
                    })
                    .collect(),
                None => {
                    let session = model.session().ok_or("open a project to draw its lines")?;
                    let stored = session.line_named(&v.line).map_err(|e| e.to_string())?;
                    let mut line = match &model.types {
                        Some((st, ins)) => stored.resolve(st, ins).map_err(|e| e.to_string())?,
                        None => stored.clone(),
                    };
                    let weather: Vec<ohl_weather::WeatherCase> =
                        model.cases.iter().map(|c| c.case.clone()).collect();
                    line.set_weather_cases(&weather);
                    cases
                        .iter()
                        .filter_map(|(name, _, _)| {
                            let c = model.cases.iter().find(|c| &c.case.name == name)?;
                            let solved =
                                crate::model::Solved::new(&line, &model.library, c.case.clone());
                            Some((name.clone(), solved.shapes, solved.jumpers))
                        })
                        .collect()
                }
            };
            let line = model
                .other_lines
                .iter()
                .find(|o| o.name == v.line)
                .map(|o| o.line.clone())
                .or_else(|| model.session()?.line_named(&v.line).ok().cloned())
                .ok_or_else(|| format!("no project line \"{}\"", v.line))?;
            versions.push(Version {
                name: v.line.clone(),
                role: v.role.clone(),
                label: if v.label.is_empty() {
                    "{name}".into()
                } else {
                    v.label.clone()
                },
                line,
                solved,
            });
        }
        let mut options = ohl_drawing::Options {
            horizontal_scale: w.hscale,
            vertical_scale: w.vscale,
            clearance,
            title,
            plan: !w.profile_only,
            plan_half_width: w.line_width,
            break_clearance,
            side_offsets: w.side_offsets.clone(),
            plan_offset_scale: w.plan_scale,
            logos,
            fields: s.fields.clone(),
            info: ohl_drawing::SheetInfo {
                project,
                line: if line_name.is_empty() {
                    model.line.name.clone()
                } else {
                    line_name
                },
                crs: model.line.crs.clone(),
                date: ohl_drawing::today(),
                cases: cases
                    .iter()
                    .map(|c| c.0.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                height_datum: model.line.height_datum_label(),
            },
            role: (!s.role.is_empty()).then(|| s.role.clone()),
            corridor: s.corridor,
            conductor_clearance,
            bands: s.bands,
            lidar_classes: s
                .lidar_classes
                .as_deref()
                .map(crate::sheet_ui::parse_classes)
                .transpose()?,
            class_names: model
                .scheme
                .classes
                .iter()
                .map(|c| {
                    (
                        c.code,
                        (
                            c.name.clone(),
                            crate::scene::class_color(&model.scheme, c.code),
                        ),
                    )
                })
                .collect(),
            ..Default::default()
        }
        .with_template(template);
        // The LiDAR points loaded (the project's point budget), of the
        // classes drawn: each route keeps those within its corridor.
        let lidar = if options.draws_lidar() {
            let mut wanted = [false; 256];
            for c in options.lidar_class_list() {
                wanted[c as usize] = true;
            }
            Arc::new(
                model
                    .points
                    .iter()
                    .filter(|(_, c)| wanted[*c as usize])
                    .map(|(q, c)| {
                        (
                            [0, 1, 2].map(|k| model.origin[k] + f64::from(q[k]) / 1000.0),
                            *c,
                        )
                    })
                    .collect(),
            )
        } else {
            Arc::default()
        };
        // The window's elevations over the template's when it sets them.
        if w.elevations.place != ohl_drawing::ElevationPlace::Off {
            options.elevations.place = w.elevations.place;
            options.elevations.offset_mm = w.elevations.offset_mm;
            options.elevations.rotation_deg = w.elevations.rotation_deg;
        }
        if s.elevation_scale.is_some() {
            options.elevations.scale = s.elevation_scale;
        }
        if !s.elevation_labels.is_empty() {
            options.elevations.labels = ohl_drawing::ElevationLabels::parse(&s.elevation_labels)?;
        }
        if let Some(stick) = s.elevation_stick {
            options.elevations.stick = stick;
        }
        Ok(Self {
            line: model.line.clone(),
            bodies: model.bodies.clone(),
            routes,
            ground: model.ground.clone(),
            options,
            versions,
            markers: if w.markers_on {
                w.markers.clone()
            } else {
                Arc::default()
            },
            range: w.range.then_some([w.from.min(w.to), w.from.max(w.to)]),
            export: Export {
                formats: w.formats.clone(),
                page,
                optimise,
                dpi: w.dpi,
            },
            same_paper: w.same_paper,
            cases,
            isoclearance: w.isoclearance.clone(),
            lidar,
            stringing,
            out,
        })
    }

    /// The drawing data of `route`: the first case's wires solid, the
    /// others' dashed.
    pub fn drawing(&self, route: &AlignmentView) -> Result<ohl_drawing::RouteDrawing, String> {
        let (_, shapes, jumpers) = &self.cases[0];
        let mut d = ohl_drawing::build(
            &self.line,
            route,
            shapes,
            jumpers,
            self.ground.as_deref(),
            &self.markers,
            &self.options,
        )?;
        crate::scene::set_drawn_members(&mut d, &self.line, &self.bodies);
        if let Some((name, shapes)) = &self.stringing {
            d.set_stringing(&self.line, shapes, name);
        }
        if !self.lidar.is_empty() {
            d.set_lidar(
                self.lidar.iter().copied(),
                &self.options.lidar_class_list(),
                self.options.lidar_half_width(),
            );
        }
        if let Some(lines) = &self.isoclearance {
            d.set_isoclearance(&lines.0, lines.1);
        }
        d.set_case(&self.cases[0].0);
        for (k, (name, shapes, jumpers)) in self.cases.iter().enumerate().skip(1) {
            let other =
                ohl_drawing::build(&self.line, route, shapes, jumpers, None, &[], &self.options)?;
            for mut w in other.wires {
                w.label = format!("{} — {name}", w.label);
                w.dash = Some(ohl_drawing::case_dash(k));
                w.case = name.clone();
                d.wires.push(w);
            }
        }
        for v in &self.versions {
            for (k, (case, shapes, jumpers)) in v.solved.iter().enumerate() {
                d.add_version(
                    &v.line,
                    shapes,
                    jumpers,
                    &v.name,
                    &v.role,
                    &v.label,
                    self.options.plan_half_width,
                    (case, (k > 0).then(|| ohl_drawing::case_dash(k))),
                );
            }
        }
        Ok(d)
    }

    /// The export, with the optimised layout narrowed to one paper (or
    /// one standard) for every route when [`Self::same_paper`] asks.
    ///
    /// Expected cost: the routes' drawing data and their candidates once
    /// more (seconds for a long line).
    pub fn common_export(&self, options: &ohl_drawing::Options) -> Result<Export, String> {
        let mut export = self.export.clone();
        if self.same_paper
            && self.routes.len() > 1
            && let Some(opt) = &self.export.optimise
        {
            let ds = self
                .routes
                .iter()
                .map(|r| self.drawing(r))
                .collect::<Result<Vec<_>, _>>()?;
            let all: Vec<_> = ds.iter().map(|d| (d, options)).collect();
            export.optimise = Some(ohl_drawing::paper::common_choice(&all, opt));
        }
        Ok(export)
    }

    /// The options of each route's drawings (with the station range).
    pub fn route_options(&self) -> ohl_drawing::Options {
        ohl_drawing::Options {
            stations: self.range,
            ..self.options.clone()
        }
    }
}

/// Writes the drawings of `input`, reporting progress per route.
///
/// Expected cost: under a second per route for SVG, DXF, PDF, CSV and
/// XML; ≈ 0.5 s per sheet for PNG. Exit clause: 30 minutes per route.
pub fn run(input: &Input, report: &dyn Fn(String, f32)) -> Result<String, String> {
    let started = std::time::Instant::now();
    let budget =
        ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(30 * 60));
    let (mut files, mut warnings) = (0, Vec::new());
    let n = input.routes.len();
    let options = input.route_options();
    let export = input.common_export(&options)?;
    for (r, route) in input.routes.iter().enumerate() {
        report(
            format!("route {} ({} of {n})", route.id, r + 1),
            r as f32 / n as f32,
        );
        let d = input.drawing(route)?;
        let written = ohl_drawing::export::write_route(&d, &options, &export, &input.out, &budget)?;
        if let Some(paper) = &written.paper {
            warnings.push(format!("route {}: optimised layout {paper}", route.id));
        }
        files += written.files.len();
        warnings.extend(
            written
                .warnings
                .into_iter()
                .map(|w| format!("route {}: {w}", route.id)),
        );
    }
    report("done".into(), 1.0);
    let mut text = format!(
        "{files} files written to {} in {:.1?}",
        input.out.display(),
        started.elapsed()
    );
    for w in warnings {
        text.push_str("\n⚠ ");
        text.push_str(&w);
    }
    Ok(text)
}

/// The standard paper sizes as ticks, by standard: the sizes offered
/// and those the optimiser may choose (at least one stays ticked).
fn paper_sizes_ui(ui: &mut egui::Ui, ticked: &mut Vec<String>) {
    use ohl_drawing::paper::{PAPER_SIZES, Standard};
    for standard in [
        Standard::IsoA,
        Standard::IsoB,
        Standard::Ansi,
        Standard::Arch,
    ] {
        ui.label(egui::RichText::new(standard.name()).small().strong());
        ui.horizontal_wrapped(|ui| {
            for p in PAPER_SIZES.iter().filter(|p| p.standard == standard) {
                let mut on = ticked.iter().any(|t| t == p.name);
                let r = ui
                    .checkbox(&mut on, p.name)
                    .on_hover_text(format!("{:.0} × {:.0} mm", p.long, p.short));
                if r.changed() {
                    if on {
                        ticked.push(p.name.to_owned());
                    } else if ticked.len() > 1 {
                        ticked.retain(|t| t != p.name);
                    }
                }
            }
        });
    }
}

/// A sheet drawn for the window as pixels (made in the background).
struct SheetPixels {
    /// Its size, mm.
    size: [f64; 2],
    /// How many sheets its route has.
    sheets: usize,
    /// The sheet shown (1-based).
    number: usize,
    /// Its route.
    route: String,
    /// The routes the export draws.
    routes: Vec<String>,
    /// The sheet's warnings.
    warnings: Vec<String>,
    /// Width and height, pixels.
    px: [usize; 2],
    /// RGBA, unmultiplied.
    rgba: Vec<u8>,
}

impl SheetPixels {
    /// On screen.
    fn image(self, ctx: &egui::Context) -> SheetImage {
        let image = egui::ColorImage::from_rgba_unmultiplied(self.px, &self.rgba);
        SheetImage {
            texture: ctx.load_texture("drawings-sheet-preview", image, Default::default()),
            size: self.size,
            sheets: self.sheets,
            number: self.number,
            route: self.route,
            routes: self.routes,
            warnings: self.warnings,
        }
    }
}

/// Sheet `n` of route `route` ([`DrawingsWindow::preview_route`], else the
/// first `input` draws) as pixels (about 1600 pixels wide), with the
/// route's sheet count and the sheet's warnings. A sheet past the route's
/// last shows the last.
///
/// Expected cost: the route's drawing data and layout (well under a
/// second), the sheet drawn (≈ 0.05 s) and rendered (≈ 1 s for a full
/// A1 sheet); run in the background.
fn sheet_pixels(input: &Input, route: Option<&str>, n: usize) -> Result<SheetPixels, String> {
    if !input.export.paged() {
        return Err("one continuous drawing: choose a sheet format to preview a sheet".into());
    }
    let options = input.route_options();
    let export = input.common_export(&options)?;
    let routes: Vec<String> = input.routes.iter().map(|r| r.id.clone()).collect();
    let route = route
        .and_then(|id| input.routes.iter().find(|r| r.id == id))
        .or(input.routes.first())
        .ok_or("no route")?;
    let d = input.drawing(route)?;
    // Laid out, then only the sheet shown drawn.
    let (count, draw): (usize, Box<dyn Fn(usize) -> Option<ohl_drawing::Sheet>>) =
        match (&export.optimise, &export.page) {
            (Some(opt), _) => {
                let best = ohl_drawing::paper::best(&d, &options, opt)
                    .ok_or("no paper size to lay the sheets out on")?;
                let (d, options) = (&d, &options);
                (
                    best.sheets(),
                    Box::new(move |k| ohl_drawing::paper::draw_one(d, options, &best, k)),
                )
            }
            (None, Some(page)) => {
                let count = ohl_drawing::sheet_layout(&d, &options, page).len();
                let (d, options, page) = (&d, &options, page.clone());
                (
                    count,
                    Box::new(move |k| ohl_drawing::sheet(d, options, &page, k)),
                )
            }
            (None, None) => (0, Box::new(|_| None)),
        };
    let number = n.clamp(1, count.max(1));
    let sheet = draw(number).ok_or_else(|| format!("route {} has no sheet", route.id))?;
    let page = sheet.page.clone();
    let dpi = 1600.0 / (page.width / 25.4);
    let (wpx, hpx, rgba) = ohl_drawing::png::render_rgba(&sheet.drawing, dpi, 4.0e6)?;
    Ok(SheetPixels {
        size: [page.width, page.height],
        sheets: count,
        number,
        route: route.id.clone(),
        routes,
        warnings: sheet.warnings,
        px: [wpx as usize, hpx as usize],
        rgba,
    })
}

impl DrawingsWindow {
    /// The optimiser of the window's ticked sizes, orientations and
    /// mixing.
    pub fn optimise(&self) -> Result<ohl_drawing::paper::Optimise, String> {
        let mut o = ohl_drawing::paper::Optimise::from_names(&self.paper_sizes, self.mixed_sizes)?;
        match self.paper_orientation.as_str() {
            "landscape" => o.portrait = false,
            "portrait" => o.landscape = false,
            _ => {}
        }
        Ok(o)
    }

    /// The optimiser's candidates for the first route the window draws,
    /// best first, as lines of text.
    ///
    /// Expected cost: the route's drawing data and one layout per size,
    /// orientation and (mixed) standard: under a second for most routes,
    /// seconds for a long one.
    fn compare_papers(&self, model: &Model, case: usize) -> Result<Vec<String>, String> {
        let input = Input::prepare(model, case, self, PathBuf::new())?;
        let opt = match &input.export.optimise {
            Some(o) => o.clone(),
            None => self.optimise()?,
        };
        let route = input.routes.first().ok_or("no route")?;
        let d = input.drawing(route)?;
        let options = input.route_options();
        let lines = ohl_drawing::paper::candidates(&d, &options, &opt)
            .iter()
            .map(|c| {
                format!(
                    "{}: {} sheet(s) ({}), {} warning(s), {:.2} m², {:.0} % filled",
                    c.label,
                    c.sheets(),
                    c.sizes(),
                    c.warnings,
                    c.area,
                    c.fill * 100.0
                )
            })
            .collect();
        Ok(lines)
    }

    /// The window; `Some(job)` once started.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &Model,
        case: usize,
        open: &mut bool,
        preview_on: &mut bool,
        action: &mut Option<crate::sheet_ui::Action>,
    ) -> Option<Job> {
        let mut job = None;
        let routes = model.line.alignment_views();
        let offers = crate::sheet_ui::offers(model, &self.plugin_templates);
        let template =
            crate::sheet_ui::resolve(model, &self.sheet.template, &self.plugin_templates);
        let max_height = (ctx.content_rect().height() * 0.85).max(300.0);
        egui::Window::new("Export drawings")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
              egui::ScrollArea::vertical()
                .id_salt("drawings-scroll")
                .max_height(max_height)
                .show(ui, |ui| {
                egui::Grid::new("drawings-grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Template");
                        ui.vertical(|ui| {
                            if let Some(a) =
                                crate::sheet_ui::template_ui(ui, &mut self.sheet, &offers)
                            {
                                *action = Some(a);
                            }
                            if let Err(e) = &template {
                                crate::menu::error_label(ui, e);
                            }
                        });
                        ui.end_row();

                        ui.label("Route");
                        let name = |k: Option<usize>| match k.and_then(|k| routes.get(k)) {
                            Some(r) => format!("{} {}", r.id, r.name),
                            None => "every route".into(),
                        };
                        egui::ComboBox::from_id_salt("drawings-route")
                            .selected_text(name(self.route))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.route, None, name(None));
                                for k in 0..routes.len() {
                                    ui.selectable_value(&mut self.route, Some(k), name(Some(k)));
                                }
                            });
                        ui.end_row();

                        ui.label("Formats");
                        ui.vertical(|ui| {
                            for f in Format::ALL {
                                let mut on = self.formats.contains(&f);
                                if ui
                                    .checkbox(&mut on, f.name().to_uppercase())
                                    .on_hover_text(f.description())
                                    .changed()
                                {
                                    self.formats.retain(|x| *x != f);
                                    if on {
                                        self.formats.push(f);
                                    }
                                }
                            }
                        });
                        ui.end_row();

                        ui.label("Sheets");
                        let template_page = template
                            .as_ref()
                            .ok()
                            .and_then(|t| t.page())
                            .map(|p| format!("the template's: {}", p.name));
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let fixed = |p: &str| format!("{p}, split at structures");
                                egui::ComboBox::from_id_salt("drawings-page")
                                    .selected_text(match self.page.as_str() {
                                        "" => "one continuous drawing".to_owned(),
                                        "template" => template_page
                                            .clone()
                                            .unwrap_or("the template's (none: choose one)".into()),
                                        "optimised" => "optimised from the ticked sizes".to_owned(),
                                        p => p.to_owned(),
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.page,
                                            String::new(),
                                            "one continuous drawing",
                                        );
                                        if let Some(t) = &template_page {
                                            ui.selectable_value(
                                                &mut self.page,
                                                "template".into(),
                                                fixed(t),
                                            );
                                        }
                                        ui.selectable_value(
                                            &mut self.page,
                                            "optimised".into(),
                                            "optimised: size and orientation from the ticked sizes",
                                        );
                                        for p in self.paper_sizes.clone() {
                                            ui.selectable_value(&mut self.page, p.clone(), fixed(&p));
                                        }
                                    });
                                if self.page != "optimised" {
                                    ui.add_enabled(
                                        !self.page.is_empty(),
                                        egui::Checkbox::new(&mut self.sheet.portrait, "portrait"),
                                    );
                                }
                            });
                            if self.page == "optimised" {
                                ui.horizontal(|ui| {
                                    egui::ComboBox::from_id_salt("drawings-orientation")
                                        .selected_text(
                                            ORIENTATIONS
                                                .iter()
                                                .find(|(k, _)| *k == self.paper_orientation)
                                                .map_or("landscape or portrait", |(_, t)| *t),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (k, t) in ORIENTATIONS {
                                                ui.selectable_value(
                                                    &mut self.paper_orientation,
                                                    k.to_owned(),
                                                    t,
                                                );
                                            }
                                        });
                                    ui.checkbox(&mut self.mixed_sizes, "different sizes allowed")
                                        .on_hover_text(
                                            "A route's sheets may take different sizes of one \
                                             standard (e.g. A3 and A4), to use less paper",
                                        );
                                    ui.add_enabled(
                                        self.route.is_none() && routes.len() > 1,
                                        egui::Checkbox::new(
                                            &mut self.same_paper,
                                            "same paper on every route",
                                        ),
                                    );
                                });
                                ui.label(
                                    egui::RichText::new(
                                        "Chosen per route: the fewest warnings, then the least \
                                         paper (fuller sheets), then the fewest sheets. The \
                                         scales stay as set.",
                                    )
                                    .small()
                                    .weak(),
                                );
                                ui.horizontal(|ui| {
                                    if ui
                                        .button("Compare sizes")
                                        .on_hover_text(
                                            "Every candidate for the first route drawn, best first",
                                        )
                                        .clicked()
                                    {
                                        self.paper_report = Some(self.compare_papers(model, case));
                                    }
                                    if self.paper_report.is_some() && ui.button("Hide").clicked() {
                                        self.paper_report = None;
                                    }
                                });
                                match &self.paper_report {
                                    Some(Ok(lines)) => {
                                        egui::ScrollArea::vertical()
                                            .id_salt("drawings-paper-report")
                                            .max_height(120.0)
                                            .show(ui, |ui| {
                                                for l in lines {
                                                    ui.label(egui::RichText::new(l).small());
                                                }
                                            });
                                    }
                                    Some(Err(e)) => {
                                        crate::menu::error_label(ui, e);
                                    }
                                    None => {}
                                }
                            }
                            egui::CollapsingHeader::new(format!(
                                "Paper sizes ({} ticked)",
                                self.paper_sizes.len()
                            ))
                            .id_salt("drawings-paper-sizes")
                            .show(ui, |ui| {
                                paper_sizes_ui(ui, &mut self.paper_sizes);
                            });
                        });
                        ui.end_row();

                        ui.label("Scales");
                        ui.horizontal(|ui| {
                            ui.label("horizontal 1:");
                            ui.add(
                                egui::DragValue::new(&mut self.hscale)
                                    .range(10.0..=100_000.0)
                                    .speed(10.0),
                            );
                            ui.label("vertical 1:");
                            ui.add(
                                egui::DragValue::new(&mut self.vscale)
                                    .range(10.0..=100_000.0)
                                    .speed(10.0),
                            );
                        });
                        ui.end_row();

                        ui.label("Line width");
                        ui.horizontal(|ui| {
                            ui.label("±");
                            ui.add(
                                egui::DragValue::new(&mut self.line_width)
                                    .range(LINE_WIDTHS)
                                    .suffix(" m")
                                    .speed(1.0),
                            )
                            .on_hover_text(LINE_WIDTH_HELP);
                        });
                        ui.end_row();

                        ui.label("Sheet breaks");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.break_clearance)
                                    .range(0.0..=ohl_drawing::MAX_BREAK_CLEARANCE_MM)
                                    .suffix(" mm")
                                    .speed(0.1)
                                    .max_decimals(1),
                            )
                            .on_hover_text(
                                "Clearance on paper between a sheet break and what is drawn \
                                 of every other structure (0 to 20 mm)",
                            );
                            ui.label("on paper clear of other structures");
                        });
                        ui.end_row();

                        ui.label("Stations");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.range, "from");
                            ui.add_enabled(
                                self.range,
                                egui::DragValue::new(&mut self.from).suffix(" m").speed(5.0),
                            );
                            ui.label("to");
                            ui.add_enabled(
                                self.range,
                                egui::DragValue::new(&mut self.to).suffix(" m").speed(5.0),
                            );
                        });
                        ui.end_row();

                        ui.label("Clearance line");
                        ui.horizontal(|ui| {
                            let mut on = self.clearance.is_some();
                            let rule = ground_rule_clearance(&self.ground_rules, &model.line);
                            let from_rule = self.clearance_rule && rule.is_some();
                            ui.add_enabled(!from_rule, egui::Checkbox::new(&mut on, "ground +"));
                            let mut c = self.clearance.unwrap_or(7.0);
                            ui.add_enabled(
                                on && !from_rule,
                                egui::DragValue::new(&mut c)
                                    .range(0.0..=100.0)
                                    .suffix(" m")
                                    .speed(0.1),
                            );
                            self.clearance = on.then_some(c);
                            if let Some((d, name)) = rule {
                                ui.checkbox(
                                    &mut self.clearance_rule,
                                    format!("from the ground rule \"{name}\" ({d} m)"),
                                )
                                .on_hover_text(
                                    "The project's clearance rule for the ground (Checks › \
                                     Ground clearances, else the rule file chosen): its largest \
                                     vertical distance over the line's voltages",
                                );
                            }
                        });
                        ui.end_row();

                        ui.label("Side profiles");
                        ui.horizontal_wrapped(|ui| {
                            let mut gone = None;
                            for (k, o) in self.side_offsets.iter_mut().enumerate() {
                                ui.add(
                                    egui::DragValue::new(o)
                                        .range(-2000.0..=2000.0)
                                        .suffix(" m")
                                        .speed(0.5),
                                )
                                .on_hover_text("Offset from the axis, right positive");
                                if ui.small_button("×").on_hover_text("Leave it out").clicked() {
                                    gone = Some(k);
                                }
                            }
                            if let Some(k) = gone {
                                self.side_offsets.remove(k);
                            }
                            if ui
                                .small_button("+")
                                .on_hover_text("A ground profile beside the axis (dashed)")
                                .clicked()
                            {
                                let next = self.side_offsets.last().map_or(10.0, |o| -o);
                                self.side_offsets.push(next);
                            }
                        });
                        ui.end_row();

                        ui.label("Plan across");
                        ui.horizontal(|ui| {
                            let mut on = self.plan_scale.is_some();
                            ui.checkbox(&mut on, "own scale 1:")
                                .on_hover_text(
                                    "The plan across the line at its own scale (along it the \
                                     horizontal scale, to line up with the profile)",
                                );
                            let mut k = self.plan_scale.unwrap_or(self.hscale / 2.0);
                            ui.add_enabled(
                                on,
                                egui::DragValue::new(&mut k)
                                    .range(10.0..=100_000.0)
                                    .speed(10.0),
                            );
                            self.plan_scale = on.then_some(k);
                        });
                        ui.end_row();

                        ui.label("Logos");
                        ui.vertical(|ui| {
                            let mut gone = None;
                            for (k, (file, width)) in self.logos.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        file.file_name()
                                            .map_or(String::new(), |n| {
                                                n.to_string_lossy().into_owned()
                                            }),
                                    )
                                    .on_hover_text(file.display().to_string());
                                    ui.add(
                                        egui::DragValue::new(width)
                                            .range(1.0..=500.0)
                                            .suffix(" mm")
                                            .speed(0.5),
                                    );
                                    if ui.small_button("×").clicked() {
                                        gone = Some(k);
                                    }
                                });
                            }
                            if let Some(k) = gone {
                                self.logos.remove(k);
                            }
                            if ui
                                .button("Add a logo…")
                                .on_hover_text("SVG, PNG or JPEG, in the title block")
                                .clicked()
                                && let Some(file) = rfd::FileDialog::new()
                                    .add_filter("Images", &["svg", "png", "jpg", "jpeg"])
                                    .pick_file()
                            {
                                self.logos.push((file, 30.0));
                            }
                        });
                        ui.end_row();

                        ui.label("Elevations");
                        ui.horizontal(|ui| {
                            let e = &mut self.elevations;
                            egui::ComboBox::from_id_salt("drawings-elevations")
                                .selected_text(e.place.key())
                                .show_ui(ui, |ui| {
                                    for p in ohl_drawing::ElevationPlace::ALL {
                                        ui.selectable_value(&mut e.place, p, p.key());
                                    }
                                })
                                .response
                                .on_hover_text(
                                    "Each structure seen along the line on the plan, at the \
                                     horizontal scale, a leader from where it stands \
                                     (layer OHL_ELEVATIONS)",
                                );
                            let on = e.place != ohl_drawing::ElevationPlace::Off;
                            ui.label("base");
                            ui.add_enabled(
                                on,
                                egui::DragValue::new(&mut e.offset_mm)
                                    .range(ELEVATION_OFFSETS)
                                    .suffix(" mm")
                                    .speed(0.5),
                            )
                            .on_hover_text("From the line to the elevation's base, on paper");
                            ui.label("at");
                            ui.add_enabled(
                                on,
                                egui::DragValue::new(&mut e.rotation_deg)
                                    .range(-180.0..=180.0)
                                    .suffix("°")
                                    .speed(1.0),
                            )
                            .on_hover_text(
                                "Angle between the line and the elevation's height \
                                 (90°: standing perpendicular to the line)",
                            );
                        });
                        ui.end_row();

                        ui.label("PNG");
                        ui.add(
                            egui::DragValue::new(&mut self.dpi)
                                .range(30.0..=600.0)
                                .suffix(" dpi"),
                        );
                        ui.end_row();
                    });
                if let Ok(t) = &template {
                    sheet_sections(ui, &mut self.sheet, t, model);
                }
                ui.horizontal(|ui| {
                    let mut on = self.sheet_preview.is_some();
                    ui.checkbox(&mut on, "Preview sheet")
                        .on_hover_text(
                            "One sheet as the export draws it, in a window of its own (its \
                             route and sheet chosen there too)",
                        );
                    let mut n = self.sheet_preview.unwrap_or(self.preview_sheet).max(1);
                    ui.add(egui::DragValue::new(&mut n).range(1..=999).prefix("sheet "));
                    self.preview_sheet = n;
                    self.sheet_preview = on.then_some(n);
                });
                ui.checkbox(&mut self.profile_only, "the profile alone (no plan)");
                ui.checkbox(
                    &mut self.all_cases,
                    "the other solved weather cases too (dashed)",
                );
                if !self.markers.is_empty() {
                    ui.checkbox(
                        &mut self.markers_on,
                        format!(
                            "the violations of the last clearance check ({}) as markers",
                            self.markers.len()
                        ),
                    );
                }
                ui.checkbox(preview_on, PREVIEW_TICK)
                    .on_hover_text(PREVIEW_HELP);
                ui.horizontal(|ui| {
                    if ui.button("Folder…").clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        self.out = Some(dir);
                    }
                    ui.label(
                        self.out
                            .as_ref()
                            .map_or("no folder chosen".into(), |p| p.display().to_string()),
                    );
                });
                ui.label(
                    egui::RichText::new(
                        "DXF: R12 sheets in mm, plus the plan in project coordinates and the \
                         profile (station × exaggerated height) in metres. PDF: a page per \
                         sheet. CSV/XML: the profile's numbers.",
                    )
                    .small()
                    .weak(),
                );
                if let Some(lines) = &self.isoclearance {
                    let mut keep = true;
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Isoclearance lines of Checks › Vegetation: {} on the plan (OHL_ISOCLEARANCE)",
                            lines.0.len()
                        ));
                        if ui.button("Leave them out").clicked() {
                            keep = false;
                        }
                    });
                    if !keep {
                        self.isoclearance = None;
                    }
                }
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
                }
                if ui.button("Export").clicked() {
                    match Input::new(model, case, self) {
                        Ok(input) => {
                            self.error = None;
                            job = Some(Job::start(
                                "Exporting drawings".into(),
                                false,
                                ctx,
                                move |report| run(&input, report),
                            ));
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                });
            });
        job
    }
}

/// The sheet template's sections of the export window: its values and
/// Per circuit (each system, and the earth wires) the weather case its
/// wires are drawn in and that of its stringing tension (the
/// `stringing` band): the template's (`[cases]`) or the case shown
/// unless chosen.
fn circuit_cases_ui(
    ui: &mut egui::Ui,
    s: &mut SheetChoice,
    t: &ohl_drawing::SheetTemplate,
    model: &Model,
) {
    let mut keys: Vec<String> = model.line.sections.iter().map(circuit_key).collect();
    keys.sort();
    keys.dedup();
    let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
    ui.label(
        egui::RichText::new(
            "A case's name, else the template's (a temperature: the still case at it), else the \
             case shown. The stringing tension is written by a \"stringing\" band row.",
        )
        .small()
        .weak(),
    );
    egui::Grid::new("drawings-circuit-cases-grid")
        .num_columns(3)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            ui.strong("Circuit");
            ui.strong("Drawn");
            ui.strong("Stringing");
            ui.end_row();
            for k in keys {
                let label = if k.is_empty() {
                    "earth wires".to_owned()
                } else {
                    k.clone()
                };
                ui.label(&label);
                let entry = s.circuit_cases.entry(k.clone()).or_default();
                let default_drawn = if k.is_empty() {
                    &t.cases.earth_wires
                } else {
                    &t.cases.conductors
                };
                for (id, value, default, none) in [
                    ("drawn", &mut entry.drawn, default_drawn, "the case shown"),
                    (
                        "stringing",
                        &mut entry.stringing,
                        &t.cases.stringing,
                        "none",
                    ),
                ] {
                    let fallback = match default {
                        Some(d) => format!("the template's: {d}"),
                        None => none.to_owned(),
                    };
                    egui::ComboBox::from_id_salt(("drawings-circuit-case", &k, id))
                        .width(200.0)
                        .selected_text(value.clone().unwrap_or_else(|| fallback.clone()))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(value, None, &fallback);
                            for n in &names {
                                ui.selectable_value(value, Some(n.clone()), n);
                            }
                        });
                }
                ui.end_row();
            }
        });
    // Rows left at the defaults are not kept.
    s.circuit_cases
        .retain(|_, c| c.drawn.is_some() || c.stringing.is_some());
}

/// revisions, the versions of the line, its other options.
fn sheet_sections(
    ui: &mut egui::Ui,
    s: &mut SheetChoice,
    t: &ohl_drawing::SheetTemplate,
    model: &Model,
) {
    egui::CollapsingHeader::new("Values of the template (title block, band rows)")
        .id_salt("drawings-values")
        .show(ui, |ui| crate::sheet_ui::fields_ui(ui, t, &mut s.fields));
    egui::CollapsingHeader::new("Weather cases per circuit (drawn, stringing tension)")
        .id_salt("drawings-circuit-cases")
        .show(ui, |ui| circuit_cases_ui(ui, s, t, model));
    if t.revisions.place != "off" {
        egui::CollapsingHeader::new(format!("Revisions ({})", s.fields.revisions.len()))
            .id_salt("drawings-revisions")
            .show(ui, |ui| {
                crate::sheet_ui::revisions_ui(
                    ui,
                    &mut s.fields,
                    crate::sheet_ui::last_commit(model),
                )
            });
    }
    egui::CollapsingHeader::new("Versions of the line, roles")
        .id_salt("drawings-versions")
        .show(ui, |ui| {
            let roles = crate::sheet_ui::roles(t);
            ui.horizontal(|ui| {
                ui.label("This line");
                egui::ComboBox::from_id_salt("drawings-own-role")
                    .selected_text(if s.role.is_empty() {
                        "as it is"
                    } else {
                        &s.role
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut s.role, String::new(), "as it is");
                        for r in &roles {
                            ui.selectable_value(&mut s.role, r.clone(), r);
                        }
                    })
                    .response
                    .on_hover_text("Its style: the template's [roles] (e.g. new thick)");
            });
            let current = model
                .session()
                .map_or(String::new(), |x| x.current_line().to_owned());
            let lines: Vec<String> = model
                .session()
                .and_then(|x| x.lines().ok())
                .unwrap_or_default()
                .into_iter()
                .map(|l| l.name)
                .filter(|n| *n != current)
                .collect();
            crate::sheet_ui::versions_ui(ui, s, &lines, &roles);
        });
    egui::CollapsingHeader::new("More of the template")
        .id_salt("drawings-more")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut on = s.corridor.is_some();
                let shown = s.corridor.or(t.corridor.width.map(|w| w.0)).unwrap_or(25.0);
                ui.checkbox(&mut on, "corridor ±")
                    .on_hover_text("Its edges in the plan and the key map (default: the template's)");
                let mut w = shown;
                ui.add_enabled(on, egui::DragValue::new(&mut w).range(0.0..=10_000.0).suffix(" m"));
                s.corridor = on.then_some(w);
            });
            ui.horizontal(|ui| {
                let mut own = s.lidar_classes.is_some();
                let shown = t.lidar.profile || t.lidar.plan;
                ui.add_enabled(
                    shown,
                    egui::Checkbox::new(&mut own, "LiDAR classes"),
                )
                .on_hover_text(
                    "The LiDAR points loaded within the corridor (else the line width), as dots \
                     where the template draws them ([lidar] profile, plan); these classes instead \
                     of the template's",
                );
                let mut text = s.lidar_classes.clone().unwrap_or_else(|| {
                    t.lidar
                        .classes
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                ui.add_enabled(
                    shown && own,
                    egui::TextEdit::singleline(&mut text).desired_width(120.0),
                );
                s.lidar_classes = own.then_some(text);
                if !shown {
                    ui.weak("(the template draws none)");
                }
            });
            ui.checkbox(
                &mut s.conductor_clearance,
                "lines below the lowest conductor (the project's ground rules)",
            )
            .on_hover_text(
                "For each circuit its lowest conductor lowered by the ground clearance its rule                  requires (by voltage; the rule's weather cases, else the largest sag drawn)",
            );
            ui.add_enabled(
                !t.bands.is_empty(),
                egui::Checkbox::new(&mut s.bands, "the template's band rows"),
            );
            ui.horizontal(|ui| {
                let mut on = s.elevation_scale.is_some();
                ui.checkbox(&mut on, "elevations at 1:");
                let mut k = s.elevation_scale.or(t.elevations.scale).unwrap_or(1000.0);
                ui.add_enabled(on, egui::DragValue::new(&mut k).range(10.0..=100_000.0).speed(10.0));
                s.elevation_scale = on.then_some(k);
            });
            ui.horizontal(|ui| {
                ui.label("elevation labels:");
                for (key, text) in [
                    ("number", "number"),
                    ("type", "type"),
                    ("height", "height"),
                    ("string_length", "vertical string length"),
                    ("dimensions", "heights to and between the arms"),
                    ("arm_heights", "arm heights"),
                ] {
                    let mut on = s.elevation_labels.iter().any(|l| l == key);
                    if ui.checkbox(&mut on, text).changed() {
                        s.elevation_labels.retain(|l| l != key);
                        if on {
                            s.elevation_labels.push(key.to_owned());
                        }
                    }
                }
            });
            let mut stick = s.elevation_stick.or(t.elevations.stick).unwrap_or(false);
            if ui
                .checkbox(&mut stick, "elevations as stick figures")
                .on_hover_text(
                    "The stick figure even where the full structure (members, a pole's \
                     outline) exists: easier to read at a small scale",
                )
                .changed()
            {
                s.elevation_stick = Some(stick);
            }
        });
}

impl Drawings {
    /// The drawing settings kept in `model`'s project for the project
    /// line worked on (the defaults when there are none).
    pub fn loaded(model: &Model) -> Self {
        let mut d = Self::default();
        d.load(model);
        d
    }

    /// Reads the drawing settings of the project line worked on from the
    /// project (after opening it, switching lines, or another user
    /// changing them); a project without them gets the defaults.
    pub fn load(&mut self, model: &Model) {
        let Some(session) = model.session() else {
            return;
        };
        let key = setting_key(session.current_line());
        let stored: DrawingSettings = session
            .setting(&key)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        stored.apply(&mut self.last);
        if let Some(w) = &mut self.window {
            stored.apply(w);
        }
        self.preview_on = stored.preview;
        self.preview = None;
        self.saved = serde_json::to_string(&self.stored())
            .ok()
            .map(|json| (key, json));
    }

    /// The settings as they are kept in the project.
    pub fn stored(&self) -> DrawingSettings {
        DrawingSettings::of(self.settings(), self.preview_on)
    }

    /// Writes the drawing settings into the project when they changed
    /// (each frame; not while a value is being dragged or typed). A
    /// project this user cannot change keeps them for the session.
    pub fn save(&mut self, model: &mut Model, ctx: &egui::Context) {
        let Some(line) = model.session().map(|s| s.current_line().to_owned()) else {
            return;
        };
        let key = setting_key(&line);
        if self.saved.as_ref().is_some_and(|(k, _)| *k != key) {
            // Another project line worked on: its own settings.
            self.load(model);
            return;
        }
        let Ok(json) = serde_json::to_string(&self.stored()) else {
            return;
        };
        if self.saved.as_ref().is_some_and(|(_, j)| *j == json)
            || ctx.input(|i| i.pointer.any_down())
            || ctx.memory(|m| m.focused().is_some())
        {
            return;
        }
        let op = ohl_server::Op::SetSetting {
            key: key.clone(),
            value: Some(json.clone()),
        };
        // (Read-only for this user: kept for the session.)
        let _ = model.project_op("drawing settings", op);
        self.saved = Some((key, json));
    }

    /// The line width, m: the drawings' and the profile view's.
    pub fn line_width(&self) -> f64 {
        self.settings().line_width
    }

    /// Sets the line width (from the profile view too).
    pub fn set_line_width(&mut self, width: f64) {
        let w = width.clamp(*LINE_WIDTHS.start(), *LINE_WIDTHS.end());
        if let Some(win) = &mut self.window {
            win.line_width = w;
        }
        self.last.line_width = w;
    }

    /// The window (if open) and the job's window, each frame.
    pub fn show(&mut self, ctx: &egui::Context, model: &mut Model, case: usize) {
        // The weather cases the sheets draw per circuit, solved when they
        // are not yet (the export and the preview wait for them).
        if self.window.is_some() || self.preview_on {
            let s = self.settings().sheet.clone();
            if let Ok(t) =
                crate::sheet_ui::resolve(model, &s.template, &self.settings().plugin_templates)
                && let Ok(p) = SheetCases::of(model, &t, &s.circuit_cases)
            {
                model.want(p.needed(), ctx);
            }
        }
        // The sheet preview over the views is drawn again once the
        // settings settle: a frame then.
        if self.preview_pending.is_some() {
            ctx.request_repaint_after(PREVIEW_SETTLE);
        }
        // The plugins' templates, once read.
        if let Some(rx) = &self.template_job
            && let Ok(list) = rx.try_recv()
        {
            self.template_job = None;
            self.last.plugin_templates = list.clone();
            if let Some(w) = &mut self.window {
                w.plugin_templates = list;
            }
        }
        let mut action = None;
        if let Some(w) = &mut self.window {
            let mut open = true;
            if let Some(job) = w.ui(
                ctx,
                model,
                case,
                &mut open,
                &mut self.preview_on,
                &mut action,
            ) {
                self.job = Some(job);
            }
            if !open {
                self.last = self.window.take().unwrap_or_default();
                self.last.error = None;
            }
        }
        if let Some(a) = action {
            let message = self.template_action(model, a);
            if let Some(w) = &mut self.window {
                w.error = message.err();
            }
        }
        self.sheet_window(ctx, model, case);
        if let Some(job) = &mut self.job
            && crate::survey_io::job_window(ctx, job)
        {
            self.job = None;
        }
    }

    /// Loads a template file into the project (or uses it from the file
    /// without one), or writes the template chosen to a file.
    fn template_action(
        &mut self,
        model: &mut Model,
        action: crate::sheet_ui::Action,
    ) -> Result<(), String> {
        let Some(w) = &mut self.window else {
            return Ok(());
        };
        match action {
            crate::sheet_ui::Action::Load(path) => {
                let t = ohl_drawing::SheetTemplate::load(&path)?;
                let stem = path
                    .file_stem()
                    .map_or("template".to_owned(), |s| s.to_string_lossy().into_owned());
                let op = ohl_server::Op::PutLibrary {
                    kind: crate::sheet_ui::KIND.into(),
                    name: format!("{stem}.{}", ohl_drawing::template::EXTENSION),
                    text: t.embedded(),
                };
                w.sheet.template = match model.project_op("sheet template", op) {
                    Ok(_) => format!("project:{stem}"),
                    // No project (or read-only): used from its file.
                    Err(_) => format!("file:{}", path.display()),
                };
                Ok(())
            }
            crate::sheet_ui::Action::SaveAs(path) => {
                let t = crate::sheet_ui::resolve(model, &w.sheet.template, &w.plugin_templates)?;
                std::fs::write(&path, t.embedded()).map_err(|e| format!("{}: {e}", path.display()))
            }
        }
    }

    /// The window of the sheet previewed (when asked): drawn by the
    /// export's own code, again when a setting or the case changes (once
    /// the settings have stayed unchanged for [`PREVIEW_SETTLE`]; another
    /// sheet or route at once). The window chooses the route and steps
    /// through the sheets.
    fn sheet_window(&mut self, ctx: &egui::Context, model: &Model, case: usize) {
        let Some(w) = &mut self.window else {
            self.sheet_image = None;
            self.sheet_pending = None;
            self.sheet_job = None;
            return;
        };
        let Some(n) = w.sheet_preview else {
            self.sheet_image = None;
            self.sheet_pending = None;
            self.sheet_job = None;
            return;
        };
        let settings = DrawingsWindow {
            formats: Vec::new(),
            dpi: 0.0,
            out: None,
            error: None,
            sheet_preview: None,
            preview_sheet: 1,
            preview_route: None,
            paper_report: None,
            ..w.clone()
        };
        let key: SheetKey = (settings, case, w.preview_route.clone(), n);
        let current = self.sheet_image.as_ref().map(|(k, _)| k);
        if current != Some(&key) {
            // Only the sheet or the route: at once; other settings: once
            // they settle.
            let navigation = current.is_some_and(|k| k.0 == key.0 && k.1 == key.1);
            let settled = match &self.sheet_pending {
                Some((k, since)) if *k == key => since.elapsed() >= PREVIEW_SETTLE,
                _ => false,
            };
            let running = self.sheet_job.as_ref().is_some_and(|(k, _)| *k == key);
            if running {
            } else if current.is_none() || navigation || settled {
                // Prepared here (the model's data, cheap), drawn and
                // rendered in the background.
                self.sheet_pending = None;
                match Input::prepare(model, case, &key.0, PathBuf::new()) {
                    Err(e) => self.sheet_image = Some((key, Err(e))),
                    Ok(input) => {
                        let (tx, rx) = std::sync::mpsc::channel();
                        let (route, ctx2) = (key.2.clone(), ctx.clone());
                        std::thread::spawn(move || {
                            let _ = tx.send(sheet_pixels(&input, route.as_deref(), n));
                            ctx2.request_repaint();
                        });
                        self.sheet_job = Some((key, rx));
                    }
                }
            } else {
                if self.sheet_pending.as_ref().is_none_or(|(k, _)| *k != key) {
                    self.sheet_pending = Some((key, std::time::Instant::now()));
                }
                ctx.request_repaint_after(PREVIEW_SETTLE);
            }
        }
        // The sheet drawn in the background, once there.
        if let Some((k, rx)) = &self.sheet_job {
            match rx.try_recv() {
                Ok(made) => {
                    let k = k.clone();
                    self.sheet_image = Some((k, made.map(|p| p.image(ctx))));
                    self.sheet_job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.sheet_job = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let waiting = self.sheet_pending.is_some() || self.sheet_job.is_some();
        let mut open = true;
        let (mut number, mut route) = (n, w.preview_route.clone());
        egui::Window::new("Sheet preview")
            .open(&mut open)
            .default_size([900.0, 680.0])
            .show(ctx, |ui| {
                let image = self.sheet_image.as_ref().and_then(|(_, r)| r.as_ref().ok());
                ui.horizontal(|ui| {
                    if let Some(img) = image
                        && img.routes.len() > 1
                    {
                        ui.label("Route");
                        egui::ComboBox::from_id_salt("sheet-preview-route")
                            .selected_text(img.route.clone())
                            .show_ui(ui, |ui| {
                                for r in &img.routes {
                                    if ui.selectable_label(*r == img.route, r).clicked() {
                                        route = Some(r.clone());
                                        number = 1;
                                    }
                                }
                            });
                    }
                    let count = image.map_or(1, |i| i.sheets.max(1));
                    let shown = image.map_or(n, |i| i.number);
                    if ui.add_enabled(shown > 1, egui::Button::new("◀")).clicked() {
                        number = shown - 1;
                    }
                    ui.label(format!("Sheet {shown} of {count}"));
                    if ui
                        .add_enabled(shown < count, egui::Button::new("▶"))
                        .clicked()
                    {
                        number = shown + 1;
                    }
                    if waiting {
                        ui.label(egui::RichText::new("updating…").small().weak());
                    }
                });
                match self.sheet_image.as_ref().map(|(_, r)| r) {
                    Some(Ok(img)) => {
                        ui.label(
                            egui::RichText::new(format!(
                                "{:.0} × {:.0} mm, as the export draws it",
                                img.size[0], img.size[1]
                            ))
                            .small(),
                        );
                        egui::ScrollArea::vertical()
                            .id_salt("sheet-preview-warnings")
                            .max_height(70.0)
                            .show(ui, |ui| {
                                for x in &img.warnings {
                                    ui.label(egui::RichText::new(format!("⚠ {x}")).small());
                                }
                            });
                        let avail = ui.available_width().max(200.0);
                        let k = avail / img.size[0] as f32;
                        egui::ScrollArea::both()
                            .id_salt("sheet-preview-image")
                            .show(ui, |ui| {
                                ui.image(egui::load::SizedTexture::new(
                                    img.texture.id(),
                                    [avail, img.size[1] as f32 * k],
                                ));
                            });
                    }
                    Some(Err(e)) => {
                        crate::menu::error_label(ui, e);
                    }
                    None => {}
                }
            });
        if !open {
            w.sheet_preview = None;
        } else if number != n || route != w.preview_route {
            w.sheet_preview = Some(number);
            w.preview_sheet = number;
            w.preview_route = route;
        }
    }

    /// The sheet previewed in the window, if any: its route's sheet count
    /// and its warnings, or why it could not be drawn.
    /// (The sheets' count, the warnings, the sheet shown.)
    #[cfg(test)]
    pub fn sheet_previewed(&self) -> Option<Result<SheetPreviewed, String>> {
        self.sheet_image.as_ref().map(|(_, r)| match r {
            Ok(i) => Ok((i.sheets, i.warnings.clone(), i.number)),
            Err(e) => Err(e.clone()),
        })
    }

    /// Whether a sheet preview is being drawn (in the background).
    #[cfg(test)]
    pub fn sheet_drawing(&self) -> bool {
        self.sheet_job.is_some() || self.sheet_pending.is_some()
    }

    /// Reads the sheet templates of the plugins `dirs` (name, folder) in
    /// the background when they changed.
    pub fn set_template_plugins(&mut self, dirs: Vec<(String, PathBuf)>) {
        if dirs == self.template_plugins {
            return;
        }
        self.template_plugins = dirs.clone();
        if dirs.is_empty() {
            self.last.plugin_templates = Arc::default();
            if let Some(w) = &mut self.window {
                w.plugin_templates = Arc::default();
            }
            return;
        }
        self.template_job = Some(crate::sheet_ui::read_plugins(dirs));
    }

    /// Opens the window (with the settings it had when last closed).
    pub fn open(&mut self) {
        if self.window.is_none() {
            self.window = Some(self.last.clone());
        }
    }

    /// Gives the drawings the project's ground rules and the last
    /// clearance check's violations (when the window opens).
    pub fn set_checks(
        &mut self,
        ground_rules: Vec<ohl_clearance::Rule>,
        markers: Vec<([f64; 3], String)>,
    ) {
        let (rules, markers) = (Arc::new(ground_rules), Arc::new(markers));
        self.last.ground_rules = rules.clone();
        self.last.markers = markers.clone();
        if let Some(w) = &mut self.window {
            w.ground_rules = rules;
            w.markers = markers;
        }
    }

    /// Gives the drawings the isoclearance lines of a vegetation run (none:
    /// without them).
    pub fn set_isoclearance(&mut self, lines: Option<Arc<IsoLines>>) {
        self.last.isoclearance = lines.clone();
        if let Some(w) = &mut self.window {
            w.isoclearance = lines;
        }
    }

    /// The export settings: the window's, or those it had when closed.
    pub fn settings(&self) -> &DrawingsWindow {
        self.window.as_ref().unwrap_or(&self.last)
    }

    /// Forgets the preview (after the line or the solved cases changed).
    pub fn invalidate_preview(&mut self) {
        self.preview = None;
    }

    /// The sheet preview when it is on: made again when the settings or
    /// the case change (and after [`Self::invalidate_preview`]).
    pub fn preview(&mut self, model: &Model, case: usize) -> Option<&Result<Preview, String>> {
        if !self.preview_on {
            return None;
        }
        // Only what shapes the sheets (not the folder, formats, message,
        // the sheet previewed, the fields' values written on them).
        let s = self.settings();
        let key: PreviewKey = (
            DrawingsWindow {
                route: None,
                formats: Vec::new(),
                dpi: 0.0,
                out: None,
                error: None,
                paper_report: None,
                sheet_preview: None,
                preview_sheet: 1,
                preview_route: None,
                sheet: crate::sheet_ui::SheetChoice {
                    fields: Default::default(),
                    ..s.sheet.clone()
                },
                ..s.clone()
            },
            case,
        );
        if self.preview.as_ref().is_none_or(|(k, _)| *k != key) {
            // Drawn at once the first time, else once the settings settle.
            let settled = match &self.preview_pending {
                Some((k, since)) if *k == key => since.elapsed() >= PREVIEW_SETTLE,
                _ => false,
            };
            if self.preview.is_none() || settled {
                let made = preview(model, case, &key.0);
                self.preview = Some((key, made));
                self.preview_pending = None;
            } else if self.preview_pending.as_ref().is_none_or(|(k, _)| *k != key) {
                self.preview_pending = Some((key, std::time::Instant::now()));
            }
        }
        self.preview.as_ref().map(|(_, p)| p)
    }

    /// The sheet preview's options, in a menu of ticks (it stays open on
    /// a tick); its button opens the drawing settings.
    pub fn preview_menu(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.preview_on, PREVIEW_TICK)
            .on_hover_text(PREVIEW_HELP);
        let s = self.settings();
        let text = if s.page.is_empty() {
            "Export settings: one continuous drawing (no sheets). Choose a sheet format in \
             the drawing settings."
                .to_owned()
        } else {
            format!(
                "Export settings: {} at 1:{} / 1:{}{}, line width ±{} m, breaks {} mm clear \
                 of other structures{}",
                s.page,
                s.hscale,
                s.vscale,
                if s.profile_only {
                    ", the profile alone"
                } else {
                    ", plan and profile"
                },
                s.line_width,
                s.break_clearance,
                if s.range {
                    format!(", stations {} to {}", s.from, s.to)
                } else {
                    String::new()
                }
            )
        };
        ui.label(egui::RichText::new(text).small().weak());
        if ui.button("Drawing settings…").clicked() {
            self.open();
            ui.close();
        }
    }
}

/// What the line width is (the export window and the profile view).
pub const LINE_WIDTH_HELP: &str = "How far left and right of the alignment the line is shown, \
     in the drawings (the plan's half width) and in the profile view (one setting, kept with the \
     project line): the ground at both edges, the survey and LiDAR points and the 3D drawings \
     within it projected on the profile; structures beside the line (connections) only within \
     it, wires cut at it. Where sheets end does not depend on it";

/// The sheet preview's tick.
pub const PREVIEW_TICK: &str = "Show the export's sheets over the profile and the plan";

/// What the sheet preview shows.
const PREVIEW_HELP: &str = "Sheet edges (match lines), sheet numbers and the export's warnings \
     over the continuous profile and the plan, laid out by the export itself with the settings \
     of File › Export › Drawings…; nothing is computed differently";
