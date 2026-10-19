// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Plan and profile views of a route.
//!
//! Everything is placed by **station** along the route's centreline (its
//! alignment, or its structures) and **offset** from it (right positive):
//!
//! - **Profile**: ground along the centreline (exact TIN profile, a point
//!   at every triangle edge crossed) and optional side profiles at given
//!   offsets, the required-clearance line (ground + clearance), structures,
//!   the route's wires (spans between consecutive structures of the route
//!   along its alignment, spans to a structure of the route away from the
//!   alignment — projected —, and jumpers at its structures) coloured by
//!   phase, and markers.
//! - **Plan**: straightened along the route (station × offset, true
//!   scale), so it lines up with the profile: centreline, corridor edges,
//!   structures, wires; on request each structure's elevation beside the
//!   line ([`Elevations`]).
//!
//! [`build`] collects the drawing data. The layout makes format-free
//! [`model::Drawing`]s from it: [`drawing`] the whole route (or a station
//! range) as one continuous drawing; [`sheets`] split **at structures**
//! on the line (or free alignment points), clear of every other
//! structure drawn ([`sheet_layout`]: where, without drawing them), over
//! sheets of a standard format (A0–A4 or custom) with a title block and
//! logos; what is drawn along the line is limited by the line width
//! ([`RouteDrawing::shown`]); [`profile_model`] and [`plan_model`] in model space. Writers:
//! SVG ([`model::Drawing::svg`]), [`dxf`], [`pdf`], [`png`]; the numbers
//! as CSV and XML ([`profile_csv`], [`profile_xml`]). [`export`] writes a
//! route's files in the formats chosen. Scales: horizontal (along the
//! route, plan and profile), vertical (profile) and plan across the line,
//! each set separately.
//!
//! The sheet layout (title block, margins) is fixed here; a layout plugin
//! with company templates can replace it later.

use ohl_line::jumper::JumperShape;
use ohl_line::route::project;
use ohl_line::solve::SectionShape;
use ohl_line::{AlignmentView, Line, Point, Station};
use ohl_terrain::Terrain;

pub mod dxf;
pub mod export;
mod layout;
pub mod model;
pub mod pdf;
pub mod png;
mod table;

pub use layout::{
    ElevationDrawing, Sheet, SheetLayout, elevations, plan_model, profile_model, sheet_layout,
    sheets,
};
pub use table::{profile_csv, profile_xml};

/// Drawing options.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Horizontal scale denominator (2000 for 1:2000).
    pub horizontal_scale: f64,
    /// Vertical scale denominator of the profile (200 for 1:200).
    pub vertical_scale: f64,
    /// Offsets of side ground profiles, m (right positive).
    pub side_offsets: Vec<f64>,
    /// Required vertical clearance drawn above the ground, m.
    pub clearance: Option<f64>,
    /// Half width of the plan view, m: the **line width** of the drawings
    /// (`--plan-width`, in the desktop app the one line width of the
    /// profile and the drawings). It decides what is drawn along the line
    /// ([`RouteDrawing::shown`]): a structure beside the line (away from
    /// the alignment) is drawn only within it, wires are cut at it. Where
    /// sheets may break does not depend on it (see [`sheet_layout`]).
    pub plan_half_width: f64,
    /// Title lines (line name, weather case…).
    pub title: Vec<String>,
    /// Scale denominator of the plan across the line (default: the
    /// horizontal scale; along the line the plan always uses the horizontal
    /// scale to stay aligned with the profile).
    pub plan_offset_scale: Option<f64>,
    /// Logos for the title block of paged sheets.
    pub logos: Vec<Logo>,
    /// Station range drawn and exported, m (default: the whole route).
    pub stations: Option<[f64; 2]>,
    /// Draw the plan above the profile (else the profile alone).
    pub plan: bool,
    /// Clearance on paper, mm, between a sheet break and what is drawn of
    /// every other structure (its station, attachments and stick
    /// drawing): 0 to [`MAX_BREAK_CLEARANCE_MM`], default
    /// [`BREAK_CLEARANCE_MM`] (see [`check_break_clearance`]).
    pub break_clearance: f64,
    /// Structures' elevations on the plan (off by default).
    pub elevations: Elevations,
}

/// Default clearance on paper, mm, between a sheet break and the drawing
/// of any other structure ([`Options::break_clearance`]).
pub const BREAK_CLEARANCE_MM: f64 = 2.0;

/// Largest sheet break clearance accepted, mm on paper (under a tenth of
/// an A4 sheet's 263 mm of drawing width: more would leave few structures
/// free to break at).
pub const MAX_BREAK_CLEARANCE_MM: f64 = 20.0;

/// The sheet break clearance `mm` (on paper) if it is a number from 0 to
/// [`MAX_BREAK_CLEARANCE_MM`], else why not.
pub fn check_break_clearance(mm: f64) -> Result<f64, String> {
    if (0.0..=MAX_BREAK_CLEARANCE_MM).contains(&mm) {
        Ok(mm)
    } else {
        Err(format!(
            "sheet break clearance {mm} mm: give 0 to {MAX_BREAK_CLEARANCE_MM} mm on paper"
        ))
    }
}

/// Where the structures' elevations are drawn on the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElevationPlace {
    /// Not drawn (the default).
    #[default]
    Off,
    /// Above the line (left of it, looking ahead).
    Above,
    /// Below the line (right of it).
    Below,
}

impl ElevationPlace {
    /// Every place, in menu order.
    pub const ALL: [Self; 3] = [Self::Off, Self::Above, Self::Below];

    /// As written (CLI, settings): `off`, `above`, `below`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Above => "above",
            Self::Below => "below",
        }
    }

    /// From its key (any case).
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.key().eq_ignore_ascii_case(text.trim()))
    }
}

/// The structures' elevations on the plan: each structure seen along the
/// line (its members, its pole's outline, else its stick drawing), true
/// shape at the horizontal scale, its base `offset_mm` on paper from the
/// line, standing at `rotation_deg` to the line (90°: perpendicular to
/// it), its top away from the line; a thin leader from where it stands.
/// Display only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Elevations {
    /// Off, above or below the line.
    pub place: ElevationPlace,
    /// From the line to the elevation's base, mm on paper.
    pub offset_mm: f64,
    /// Angle between the line and the elevation's height, degrees (90:
    /// standing perpendicular to the line).
    pub rotation_deg: f64,
}

impl Default for Elevations {
    fn default() -> Self {
        Self {
            place: ElevationPlace::Off,
            offset_mm: 10.0,
            rotation_deg: 90.0,
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            horizontal_scale: 2000.0,
            vertical_scale: 200.0,
            side_offsets: Vec::new(),
            clearance: None,
            plan_half_width: 40.0,
            title: Vec::new(),
            plan_offset_scale: None,
            logos: Vec::new(),
            stations: None,
            plan: true,
            break_clearance: BREAK_CLEARANCE_MM,
            elevations: Elevations::default(),
        }
    }
}

/// A wire along the route: (station, offset, z) points.
#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    /// Label (section and span, or jumper).
    pub label: String,
    /// Colour `#rrggbb`.
    pub color: String,
    /// Points: station, offset, z.
    pub points: Vec<[f64; 3]>,
    /// The same points in project coordinates (x, y).
    pub xy: Vec<[f64; 2]>,
    /// Dash pattern (SVG `stroke-dasharray`) for other weather cases;
    /// `None`: solid.
    pub dash: Option<String>,
}

const DASHES: [(&str, &str); 4] = [
    ("3 1.5", "dashed"),
    ("0.8 0.8", "dotted"),
    ("4 1 1 1", "dash-dot"),
    ("6 2", "long dashes"),
];

/// Dash pattern of the `k`-th weather case (k ≥ 1; case 0 is solid).
pub fn case_dash(k: usize) -> String {
    DASHES[(k.max(1) - 1) % DASHES.len()].0.to_owned()
}

/// Name of the dash pattern of the `k`-th weather case.
pub fn case_dash_name(k: usize) -> &'static str {
    DASHES[(k.max(1) - 1) % DASHES.len()].1
}

/// A structure along the route.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// Structure id.
    pub id: String,
    /// Station and offset of its position.
    pub station: f64,
    /// Offset.
    pub offset: f64,
    /// Away from the alignment (not on the run along it, such as a
    /// connection beside the line; the route's rule,
    /// `AlignmentView::away`): a sheet never ends at it, and it is drawn
    /// only within the line width ([`Options::plan_half_width`]).
    pub away: bool,
    /// Position in project coordinates (x, y).
    pub xy: [f64; 2],
    /// Ground (base) height.
    pub base: f64,
    /// Highest attachment height.
    pub top: f64,
    /// Attachment points: station, offset, z.
    pub attachments: Vec<[f64; 3]>,
    /// Stick drawing: polylines of (station, offset, z).
    pub sticks: Vec<Vec<[f64; 3]>>,
    /// Members of a type at level 2 or 3 ([`RouteDrawing::set_members`]):
    /// polylines of (station, offset, z), drawn on their own layer instead
    /// of the symbol and the stick drawing; empty at level 1 (the symbol).
    pub members: Vec<Vec<[f64; 3]>>,
    /// The same members in project coordinates (x, y).
    pub members_xy: Vec<Vec<[f64; 2]>>,
    /// The outline of a pole at level 2 or 3 ([`RouteDrawing::set_poles`]):
    /// its outer outline at each of its heights, bottom to top, closed
    /// polylines of (station, offset, z); empty without one.
    pub pole: Vec<Vec<[f64; 3]>>,
    /// The same outlines in project coordinates (x, y).
    pub pole_xy: Vec<Vec<[f64; 2]>>,
}

impl Mark {
    /// Whether it is drawn with its members or its pole's outline (levels
    /// 2 and 3) instead of the symbol.
    pub fn has_body(&self) -> bool {
        !self.members.is_empty() || !self.pole.is_empty()
    }
}

/// A point of the route's alignment, by station.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentMark {
    /// Point id, e.g. `P3`.
    pub id: String,
    /// Station along the alignment, m.
    pub station: f64,
    /// The structure tied to it; `None`: a free point (an angle point
    /// without a structure), where a sheet may end too.
    pub structure: Option<String>,
}

/// Ground profile runs: (station, z) points; a run ends where there is no
/// ground.
pub type Profile = Vec<Vec<[f64; 2]>>;

/// Drawing data of one route.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteDrawing {
    /// Route id.
    pub route: String,
    /// Route name.
    pub name: String,
    /// Length of the centreline, m.
    pub length: f64,
    /// The centreline in project coordinates.
    pub centreline: Vec<Point>,
    /// Structures' stationing.
    pub stations: Vec<Station>,
    /// Structures.
    pub structures: Vec<Mark>,
    /// Ground along the centreline.
    pub ground: Profile,
    /// Ground along side lines: offset and profile.
    pub sides: Vec<(f64, Profile)>,
    /// Wires.
    pub wires: Vec<Trace>,
    /// Markers: station, offset, z, label.
    pub markers: Vec<([f64; 3], String)>,
    /// The alignment's points, in station order.
    pub alignment: Vec<AlignmentMark>,
}

fn polyline_length(p: &[Point]) -> f64 {
    p.windows(2)
        .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
        .sum()
}

/// Ground profile along the line at `offset` from the centreline.
fn ground_profile(centre: &[Point], offset: f64, terrain: &Terrain) -> Profile {
    let mut runs: Profile = Vec::new();
    let mut start = 0.0;
    let mut last_end: Option<f64> = None;
    for w in centre.windows(2) {
        let len = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        if len == 0.0 {
            continue;
        }
        // The segment shifted sideways (right positive); at bends the side
        // lines are not joined, the station stays the centreline's.
        let (ux, uy) = ((w[1][0] - w[0][0]) / len, (w[1][1] - w[0][1]) / len);
        let shift = |p: &Point| [p[0] + uy * offset, p[1] - ux * offset];
        let (a, b) = (shift(&w[0]), shift(&w[1]));
        for run in terrain.profile(a, b) {
            let pts: Vec<[f64; 2]> = run
                .iter()
                .map(|g| [start + g.t * len, g.point[2]])
                .collect();
            let joins =
                matches!((last_end, pts.first()), (Some(e), Some(p)) if (p[0] - e).abs() < 1e-6);
            match runs.last_mut() {
                Some(r) if joins => r.extend(pts.into_iter().skip(1)),
                _ => runs.push(pts),
            }
            last_end = runs.last().and_then(|r| r.last()).map(|p| p[0]);
        }
        start += len;
    }
    runs
}

/// Collects the drawing data of `route`: structures, wires of its spans
/// and jumpers (from solved `shapes` and `jumpers`), ground profiles from
/// `terrain`, and `markers` (3D points with labels) near the route.
pub fn build(
    line: &Line,
    route: &AlignmentView,
    shapes: &[SectionShape],
    jumpers: &[JumperShape],
    terrain: Option<&Terrain>,
    markers: &[(Point, String)],
    options: &Options,
) -> Result<RouteDrawing, String> {
    let centre = route.centreline(line);
    let stations = route.stations(line).map_err(|e| e.to_string())?;
    let to_route = |p: &Point| {
        let (s, o) = project(&centre, *p);
        [s, o, p[2]]
    };
    let xy = |points: &[Point]| points.iter().map(|p| [p[0], p[1]]).collect();
    let structures = route
        .structures
        .iter()
        .filter_map(|id| line.structure(id))
        .map(|s| {
            let (station, offset) = project(&centre, s.position);
            Mark {
                id: s.id.clone(),
                station,
                offset,
                away: route.is_away(&s.id),
                xy: [s.position[0], s.position[1]],
                base: s.position[2],
                top: s
                    .points
                    .iter()
                    .map(|p| p.position[2])
                    .fold(s.position[2], f64::max),
                attachments: s.points.iter().map(|p| to_route(&p.position)).collect(),
                sticks: s
                    .members
                    .iter()
                    .map(|m| m.iter().map(to_route).collect())
                    .collect(),
                members: Vec::new(),
                members_xy: Vec::new(),
                pole: Vec::new(),
                pole_xy: Vec::new(),
            }
        })
        .collect();
    // Spans of the route: between consecutive structures of its run along
    // the alignment, and those to a structure of the route away from it
    // (a connection beside the line: projected), which do not break the
    // run.
    let on = |s: &str| route.structures.iter().any(|x| x == s);
    let consecutive = |a: &str, b: &str| {
        route.consecutive(a, b) || (on(a) && on(b) && (route.is_away(a) || route.is_away(b)))
    };
    let mut wires = Vec::new();
    for shape in shapes {
        let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
            continue;
        };
        let color = line.section_color(section);
        for (k, span) in shape.spans.iter().enumerate() {
            let (a, b) = (
                &section.points[k].structure,
                &section.points[k + 1].structure,
            );
            if consecutive(a, b) {
                wires.push(Trace {
                    dash: None,
                    label: format!(
                        "{} span {k} ({} {})",
                        section.id, section.circuit_name, section.phase_label
                    ),
                    color: color.clone(),
                    points: span.points.iter().map(to_route).collect(),
                    xy: xy(&span.points),
                });
            }
        }
    }
    for j in jumpers {
        let Some(def) = line.jumpers.iter().find(|d| d.id == j.jumper) else {
            continue;
        };
        if [&def.from, &def.to]
            .into_iter()
            .flat_map(|e| line.end_structures(e))
            .any(|s| route.structures.contains(&s))
        {
            let color = def.color.clone().unwrap_or_else(|| {
                line.jumper_section(def)
                    .map_or(ohl_line::NEUTRAL_COLOR.to_owned(), |s| {
                        line.section_color(s)
                    })
            });
            wires.push(Trace {
                dash: None,
                label: format!("jumper {}", j.jumper),
                color,
                points: j.points.iter().map(to_route).collect(),
                xy: xy(&j.points),
            });
        }
    }
    let ground = terrain.map_or_else(Vec::new, |t| ground_profile(&centre, 0.0, t));
    let sides = terrain.map_or_else(Vec::new, |t| {
        options
            .side_offsets
            .iter()
            .map(|&o| (o, ground_profile(&centre, o, t)))
            .collect()
    });
    let markers = markers
        .iter()
        .map(|(p, label)| (to_route(p), label.clone()))
        .filter(|(q, _)| q[1].abs() <= options.plan_half_width.max(100.0))
        .collect();
    let mut alignment: Vec<AlignmentMark> = route
        .alignment
        .iter()
        .map(|p| AlignmentMark {
            id: p.id.clone(),
            station: project(&centre, p.position).0,
            structure: p.structure.clone(),
        })
        .collect();
    alignment.sort_by(|a, b| a.station.total_cmp(&b.station));
    Ok(RouteDrawing {
        route: route.id.clone(),
        name: route.name.clone(),
        length: polyline_length(&centre),
        alignment,
        centreline: centre,
        stations,
        structures,
        ground,
        sides,
        wires,
        markers,
    })
}

impl RouteDrawing {
    /// What is drawn of it along the line with line width `width` (m, the
    /// half width [`Options::plan_half_width`]): every structure of the run
    /// along the alignment; a structure beside the line (away from the
    /// alignment, such as a connection) only when its offset is within
    /// the width; wires cut where they leave the width (the crossing
    /// interpolated); markers within it. The plan and the profile of the
    /// drawings and sheets show this; the profile's CSV and XML keep
    /// everything.
    pub fn shown(&self, width: f64) -> RouteDrawing {
        // (NaN counts as outside.)
        let within = |offset: f64| offset.abs() <= width;
        let mut d = self.clone();
        d.structures.retain(|m| !m.away || within(m.offset));
        d.markers.retain(|(p, _)| within(p[1]));
        d.wires = self
            .wires
            .iter()
            .flat_map(|w| cut_trace(w, width))
            .collect();
        d
    }

    /// Gives each structure the members `of` it returns (polylines in
    /// project coordinates, as the structure is placed; none: a type at
    /// level 1, drawn with its symbol), along the route.
    pub fn set_members(&mut self, of: impl Fn(&str) -> Option<Vec<Vec<Point>>>) {
        let centre = self.centreline.clone();
        for m in &mut self.structures {
            let lines = of(&m.id).unwrap_or_default();
            (m.members, m.members_xy) = along(&centre, &lines);
        }
    }

    /// Gives each structure the outline of its pole `of` returns (its
    /// outer outline at each height, bottom to top, closed polylines in
    /// project coordinates as the structure is placed; none: no pole at
    /// level 2 or 3), along the route. Drawn on the members' layer instead
    /// of the symbol: the silhouette by height in the profile, the outline
    /// at the ground line and at the top in the plan.
    pub fn set_poles(&mut self, of: impl Fn(&str) -> Option<Vec<Vec<Point>>>) {
        let centre = self.centreline.clone();
        for m in &mut self.structures {
            let rings = of(&m.id).unwrap_or_default();
            (m.pole, m.pole_xy) = along(&centre, &rings);
        }
    }
}

/// Polylines in project coordinates along route `centre`: (station,
/// offset, z), and (x, y).
#[allow(clippy::type_complexity)]
fn along(centre: &[Point], lines: &[Vec<Point>]) -> (Vec<Vec<[f64; 3]>>, Vec<Vec<[f64; 2]>>) {
    (
        lines
            .iter()
            .map(|l| {
                l.iter()
                    .map(|p| {
                        let (s, o) = project(centre, *p);
                        [s, o, p[2]]
                    })
                    .collect()
            })
            .collect(),
        lines
            .iter()
            .map(|l| l.iter().map(|p| [p[0], p[1]]).collect())
            .collect(),
    )
}

/// The runs of `w` within `width` of the alignment, each ending where the
/// wire crosses the edge of the width (interpolated linearly between its
/// points).
fn cut_trace(w: &Trace, width: f64) -> Vec<Trace> {
    let n = w.points.len();
    let inside = |k: usize| w.points[k][1].abs() <= width;
    if (0..n).all(inside) {
        return vec![w.clone()];
    }
    let has_xy = w.xy.len() == n;
    // The point of segment k → k + 1 at offset `edge`.
    let at_edge = |k: usize, edge: f64| -> ([f64; 3], Option<[f64; 2]>) {
        let (a, b) = (w.points[k], w.points[k + 1]);
        let t = ((edge - a[1]) / (b[1] - a[1])).clamp(0.0, 1.0);
        let p = [0, 1, 2].map(|i| a[i] + t * (b[i] - a[i]));
        let xy = has_xy.then(|| [0, 1].map(|i| w.xy[k][i] + t * (w.xy[k + 1][i] - w.xy[k][i])));
        (p, xy)
    };
    // Where segment k → k + 1 crosses the edge on the side of point `out`.
    let crossing = |k: usize, out: usize| at_edge(k, w.points[out][1].signum() * width);
    let mut runs: Vec<Trace> = Vec::new();
    let mut push = |points: Vec<[f64; 3]>, xy: Vec<[f64; 2]>| {
        if points.len() > 1 {
            runs.push(Trace {
                label: w.label.clone(),
                color: w.color.clone(),
                points,
                xy,
                dash: w.dash.clone(),
            });
        }
    };
    // The run within the width so far: points, project coordinates.
    type Run = (Vec<[f64; 3]>, Vec<[f64; 2]>);
    let mut run: Option<Run> = None;
    for k in 0..n {
        if inside(k) {
            let (points, xy) = run.get_or_insert_with(|| {
                let mut start = (Vec::new(), Vec::new());
                if k > 0 {
                    // Entered the width between k − 1 and k.
                    let (p, q) = crossing(k - 1, k - 1);
                    start.0.push(p);
                    start.1.extend(q);
                }
                start
            });
            points.push(w.points[k]);
            if has_xy {
                xy.push(w.xy[k]);
            }
        } else if let Some((mut points, mut xy)) = run.take() {
            // Left it between k − 1 and k.
            let (p, q) = crossing(k - 1, k);
            points.push(p);
            xy.extend(q);
            push(points, xy);
        } else if k > 0 && !inside(k - 1) && w.points[k - 1][1] * w.points[k][1] < 0.0 {
            // Across it from one side to the other between two points.
            let (p0, q0) = crossing(k - 1, k - 1);
            let (p1, q1) = crossing(k - 1, k);
            push(vec![p0, p1], q0.into_iter().chain(q1).collect());
        }
    }
    if let Some((points, xy)) = run {
        push(points, xy);
    }
    runs
}

/// Station as `km+m`, e.g. `1+250.0`.
pub fn format_station(s: f64) -> String {
    // Rounded to what is shown first: 1999.96 is 2+000.0, not 1+1000.0.
    let s = (s * 10.0).round() / 10.0;
    let km = (s / 1000.0).floor();
    format!("{}+{:05.1}", km as i64, s - km * 1000.0)
}

/// A station from metres (`1250.5`) or `km+m` (`1+250.5`).
pub fn parse_station(text: &str) -> Option<f64> {
    let t = text.trim();
    let s = match t.split_once('+') {
        Some((km, m)) if !km.is_empty() => {
            km.trim().parse::<f64>().ok()? * 1000.0 + m.trim().parse::<f64>().ok()?
        }
        _ => t.parse().ok()?,
    };
    s.is_finite().then_some(s)
}

/// A scale denominator from `1:2000` or `2000` (positive).
pub fn parse_scale(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_prefix("1:").unwrap_or(t);
    let v: f64 = t.trim().parse().ok()?;
    (v.is_finite() && v > 0.0).then_some(v)
}

pub(crate) fn nice_step(range: f64, target: f64) -> f64 {
    let raw = range / target;
    let p = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * p)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * p)
}

/// A page format, mm (landscape).
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// Name, e.g. `A1`.
    pub name: String,
    /// Width, mm.
    pub width: f64,
    /// Height, mm.
    pub height: f64,
}

/// ISO 216 formats A0–A4 in landscape, or a custom `WIDTHxHEIGHT` in mm.
pub fn page_format(name: &str) -> Option<Page> {
    let (w, h) = match name.to_ascii_uppercase().as_str() {
        "A0" => (1189.0, 841.0),
        "A1" => (841.0, 594.0),
        "A2" => (594.0, 420.0),
        "A3" => (420.0, 297.0),
        "A4" => (297.0, 210.0),
        other => {
            let (w, h) = other.split_once('X')?;
            (w.trim().parse().ok()?, h.trim().parse().ok()?)
        }
    };
    (w > 50.0 && h > 50.0).then(|| Page {
        name: name.to_owned(),
        width: w,
        height: h,
    })
}

/// A logo for the title block.
#[derive(Debug, Clone, PartialEq)]
pub struct Logo {
    /// Image as a data URI.
    pub href: String,
    /// Width on the sheet, mm (height follows the image's proportions).
    pub width: f64,
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

impl Logo {
    /// A logo from an SVG, PNG or JPEG file, `width` mm wide.
    pub fn from_file(path: &std::path::Path, width: f64) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mime = match ext.as_str() {
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            _ => {
                return Err(format!(
                    "{}: logos must be SVG, PNG or JPEG",
                    path.display()
                ));
            }
        };
        Ok(Self {
            href: format!("data:{mime};base64,{}", base64(&bytes)),
            width,
        })
    }
}

/// The whole route (or the options' station range) as one continuous plan
/// and profile SVG (units mm at the given scales).
pub fn svg(d: &RouteDrawing, o: &Options) -> String {
    drawing(d, o).svg()
}

/// The whole route (or the options' station range) as one continuous
/// drawing (units mm at the given scales).
pub fn drawing(d: &RouteDrawing, o: &Options) -> model::Drawing {
    layout::continuous(d, o).0
}

/// What [`drawing`] found to warn about: elevations that overlap each
/// other or reach into the profile (kept, drawn anyway).
pub fn warnings(d: &RouteDrawing, o: &Options) -> Vec<String> {
    layout::continuous(d, o).2
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ohl_core::units::Temperature;
    use ohl_line::AlignmentPoint;
    use ohl_line::route::point_at;
    use ohl_weather::WeatherCase;
    use std::path::Path;

    pub(crate) fn solved() -> (Line, Vec<SectionShape>, Vec<JumperShape>, Terrain) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let line = Line::load(&root.join("synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("../../libraries/cables")).unwrap();
        let case = WeatherCase::still("t", Temperature::from_celsius(15.0));
        let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let jumpers: Vec<_> = ohl_line::jumper::solve_jumpers(&line, &library, &case, &shapes)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let terrain = Terrain::load(&root.join("synthetic-survey.csv")).unwrap();
        (line, shapes, jumpers, terrain)
    }

    /// The main route of the synthetic line, drawn with `options`.
    pub(crate) fn main_route(options: &Options) -> RouteDrawing {
        let (line, shapes, jumpers, terrain) = solved();
        let routes = line.alignment_views();
        build(
            &line,
            &routes[0],
            &shapes,
            &jumpers,
            Some(&terrain),
            &[],
            options,
        )
        .unwrap()
    }

    #[test]
    fn a_connection_beside_the_line_keeps_the_spans_along_it() {
        // A connection 80 m right of the main alignment between T1 and T2
        // (on the route because it gives it), a short span from T1 down to
        // it: T1–T2 stays a span of the profile, the span to the
        // connection is drawn too, projected.
        let (mut line, mut shapes, jumpers, _) = solved();
        let mut c = line.structure("T1").unwrap().clone();
        c.id = "C1".into();
        c.position = [150.0, -80.0, 490.0];
        c.points.clear();
        c.members.clear();
        c.routes = vec!["main".into()];
        line.structures.push(c);
        let s1 = line.sections.iter().find(|s| s.id == "S1").unwrap();
        let mut sc = s1.clone();
        sc.id = "SC".into();
        sc.points.truncate(2);
        sc.points[1].structure = "C1".into();
        line.sections.push(sc);
        let mut shape = shapes.iter().find(|s| s.section == "S1").unwrap().clone();
        shape.section = "SC".into();
        shape.spans.truncate(1);
        let from = shape.spans[0].points[0];
        shape.spans[0].points = (0..=20)
            .map(|k| {
                let t = f64::from(k) / 20.0;
                [
                    from[0] + t * (150.0 - from[0]),
                    from[1] + t * (-80.0 - from[1]),
                    from[2] + t * (500.0 - from[2]) - 4.0 * t * (1.0 - t),
                ]
            })
            .collect();
        shapes.push(shape);
        let views = line.alignment_views();
        let main = &views[0];
        assert_eq!(main.structures[..3], ["T1", "C1", "T2"]);
        assert_eq!(main.away, ["C1"]);
        assert!(main.consecutive("T1", "T2") && !main.consecutive("T1", "C1"));
        let d = build(
            &line,
            main,
            &shapes,
            &jumpers,
            None,
            &[],
            &Options::default(),
        )
        .unwrap();
        // Every span of S1 along the route, T1–T2 too.
        let s1: Vec<&Trace> = d
            .wires
            .iter()
            .filter(|w| w.label.starts_with("S1"))
            .collect();
        assert_eq!(s1.len(), 4);
        assert!(
            s1.iter().any(|w| w.label.starts_with("S1 span 0 ")
                && w.points[0][0].abs() < 10.0
                && (w.points.last().unwrap()[0] - 310.0).abs() < 5.0),
            "{:?}",
            s1.iter()
                .map(|w| (w.label.clone(), w.points[0], *w.points.last().unwrap()))
                .collect::<Vec<_>>()
        );
        // The connection: at its station, 80 m right; its span ends there.
        let m = d.structures.iter().find(|m| m.id == "C1").unwrap();
        assert!((m.station - 150.0).abs() < 1e-6 && (m.offset - 80.0).abs() < 1e-6);
        let to_c: Vec<&Trace> = d
            .wires
            .iter()
            .filter(|w| w.label.starts_with("SC"))
            .collect();
        assert_eq!(to_c.len(), 1);
        let end = to_c[0].points.last().unwrap();
        assert!((end[0] - 150.0).abs() < 1e-6 && (end[1] - 80.0).abs() < 1e-6);
    }

    /// A square tower 4 m wide and 30 m tall at T2 (its four legs and a
    /// crossarm 12 m across), in project coordinates.
    fn tower(line: &Line, id: &str) -> Option<Vec<Vec<Point>>> {
        let p = line.structure(id)?.position;
        let at = |x: f64, y: f64, z: f64| [p[0] + x, p[1] + y, p[2] + z];
        let mut lines: Vec<Vec<Point>> = [(-2.0, -2.0), (2.0, -2.0), (2.0, 2.0), (-2.0, 2.0)]
            .iter()
            .map(|&(x, y)| vec![at(x, y, 0.0), at(0.3 * x, 0.3 * y, 30.0)])
            .collect();
        lines.push(vec![at(-6.0, 0.0, 30.0), at(6.0, 0.0, 30.0)]);
        Some(lines)
    }

    #[test]
    fn members_replace_the_symbol_on_their_own_layer() {
        use crate::model::{Item, Layer};
        let (line, shapes, jumpers, terrain) = solved();
        let options = Options::default();
        let routes = line.alignment_views();
        let plain = build(
            &line,
            &routes[0],
            &shapes,
            &jumpers,
            Some(&terrain),
            &[],
            &options,
        )
        .unwrap();
        // Level 1 everywhere (no members given): the drawings as before.
        let mut level1 = plain.clone();
        level1.set_members(|_| None);
        assert_eq!(level1, plain);
        assert!(!svg(&plain, &options).contains("OHL_MEMBERS"));
        // T2 with members: drawn on OHL_MEMBERS in the plan and the
        // profile, its symbol (the mast line and the plan square) gone;
        // the others unchanged.
        let mut d = plain.clone();
        d.set_members(|id| (id == "T2").then(|| tower(&line, id)).flatten());
        let t2 = d.structures.iter().find(|m| m.id == "T2").unwrap();
        assert_eq!(t2.members.len(), 5);
        // Along the route: a leg's foot 2 m from T2 at its height.
        let foot = t2.members[0][0];
        assert!((foot[2] - t2.base).abs() < 1e-9);
        assert!(((foot[0] - t2.station).hypot(foot[1] - t2.offset) - 8f64.sqrt()).abs() < 1e-6);
        let flat = |dr: &model::Drawing| -> Vec<model::Item> {
            let mut out = Vec::new();
            let mut stack: Vec<Item> = dr.items.clone();
            while let Some(i) = stack.pop() {
                match i {
                    Item::Clip(c) => stack.extend(c.items),
                    other => out.push(other),
                }
            }
            out
        };
        let paths = |dr: &model::Drawing, layer: Layer| {
            flat(dr)
                .into_iter()
                .filter(|i| matches!(i, Item::Path(p) if p.layer == layer))
                .count()
        };
        let before = drawing(&plain, &options);
        let after = drawing(&d, &options);
        // Five members in the plan and five in the profile.
        assert_eq!(paths(&before, Layer::Members), 0);
        assert_eq!(paths(&after, Layer::Members), 10);
        // T2's stick drawing (its mast and arm in the example) and its
        // symbols (the plan square, the profile mast) left out.
        let sticks = 2 * t2.sticks.len() + 2;
        assert_eq!(
            paths(&after, Layer::Structures),
            paths(&before, Layer::Structures) - sticks
        );
        let text = after.svg();
        assert_eq!(text.matches(r#"class="OHL_MEMBERS""#).count(), 10);
        // DXF: the layer in the table and the members' entities on it.
        let dxf = crate::dxf::write(&after);
        let pairs = crate::dxf::read_pairs(&dxf).unwrap();
        let on: usize = pairs
            .windows(2)
            .filter(|w| w[0].0 == 0 && w[1] == (8, "OHL_MEMBERS".to_owned()))
            .count();
        assert!(on >= 10, "{on} entities on OHL_MEMBERS");
        // The plan in project coordinates: the members' footprint.
        let plan = plan_model(&d, &options);
        let feet: Vec<[f64; 2]> = plan
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Path(p) if p.layer == Layer::Members => Some(p.points[0]),
                _ => None,
            })
            .collect();
        let p = line.structure("T2").unwrap().position;
        assert_eq!(feet.len(), 5);
        assert!(
            (feet[0][0] - (p[0] - 2.0)).abs() < 1e-9 && (feet[0][1] - (p[1] - 2.0)).abs() < 1e-9
        );
    }

    /// A round pole at `id`: 700 mm at the ground, 300 mm at 27 m (a
    /// square of four corners stands for its outline: corners on the
    /// project axes), in project coordinates, closed.
    fn pole(line: &Line, id: &str) -> Option<Vec<Vec<Point>>> {
        let p = line.structure(id)?.position;
        Some(
            [(0.0, 0.35), (27.0, 0.15)]
                .iter()
                .map(|&(z, r)| {
                    [(r, 0.0), (0.0, r), (-r, 0.0), (0.0, -r), (r, 0.0)]
                        .iter()
                        .map(|&(x, y)| [p[0] + x, p[1] + y, p[2] + z])
                        .collect()
                })
                .collect(),
        )
    }

    /// Every item of a drawing, clips opened.
    fn flat_items(dr: &model::Drawing) -> Vec<model::Item> {
        let mut out = Vec::new();
        let mut stack: Vec<model::Item> = dr.items.clone();
        while let Some(i) = stack.pop() {
            match i {
                model::Item::Clip(c) => stack.extend(c.items),
                other => out.push(other),
            }
        }
        out
    }

    fn paths_on(dr: &model::Drawing, layer: model::Layer) -> Vec<model::Path> {
        flat_items(dr)
            .into_iter()
            .filter_map(|i| match i {
                model::Item::Path(p) if p.layer == layer => Some(p),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_pole_is_drawn_with_its_outline() {
        use crate::model::Layer;
        let (line, shapes, jumpers, terrain) = solved();
        let options = Options::default();
        let routes = line.alignment_views();
        let plain = build(
            &line,
            &routes[0],
            &shapes,
            &jumpers,
            Some(&terrain),
            &[],
            &options,
        )
        .unwrap();
        let mut d = plain.clone();
        d.set_poles(|id| (id == "T3").then(|| pole(&line, id)).flatten());
        let t3 = d.structures.iter().find(|m| m.id == "T3").unwrap();
        assert!(t3.has_body() && t3.members.is_empty());
        assert_eq!(t3.pole.len(), 2);
        let sheet = drawing(&d, &options);
        let on = paths_on(&sheet, Layer::Members);
        // The profile's silhouette (closed, 4 points: both sides at both
        // heights) and the plan's outline at the ground and the top.
        let profile: Vec<_> = on.iter().filter(|p| p.points.len() == 4).collect();
        assert_eq!(on.len(), 3);
        assert!(on.iter().all(|p| p.closed));
        // The silhouette's width at the ground: the outline's extent along
        // the route, at most 0.7 m, at 1:2000 (0.5 mm per m).
        let s = &profile
            .iter()
            .find(|p| p.title.as_deref() == Some("T3"))
            .unwrap();
        let wide = (s.points[0][0] - s.points[3][0]).abs();
        assert!(wide > 0.5 * 0.49 && wide <= 0.5 * 0.7 + 1e-9, "{wide}");
        // Its symbols gone (the profile mast and the plan square): two
        // structure paths fewer.
        let before = drawing(&plain, &options);
        assert_eq!(
            paths_on(&sheet, Layer::Structures).len(),
            paths_on(&before, Layer::Structures).len() - 2
        );
        // In project coordinates: the two outlines.
        let plan = plan_model(&d, &options);
        let rings: Vec<_> = plan
            .items
            .iter()
            .filter(|i| matches!(i, model::Item::Path(p) if p.layer == Layer::Members))
            .collect();
        assert_eq!(rings.len(), 2);
    }

    /// The elevations on the plan: every structure seen along the line at
    /// the horizontal scale, standing perpendicular to the line from its
    /// base 10 mm above it, a leader from where it stands; on their own
    /// layer, overlaps and the profile warned about.
    #[test]
    fn elevations_stand_beside_the_line() {
        use crate::model::Layer;
        let (line, ..) = solved();
        let mut options = Options::default();
        let mut d = main_route(&options);
        d.set_members(|id| (id == "T2").then(|| tower(&line, id)).flatten());
        // Off by default: nothing on the layer.
        assert!(paths_on(&drawing(&d, &options), Layer::Elevations).is_empty());
        options.elevations.place = ElevationPlace::Above;
        let sheet = drawing(&d, &options);
        let on = paths_on(&sheet, Layer::Elevations);
        let t2: Vec<_> = on
            .iter()
            .filter(|p| p.title.as_deref() == Some("T2 elevation"))
            .collect();
        assert_eq!(t2.len(), 5);
        // The leg's foot 2 m right of the axis, 30 m up: at 1:2000 1 mm
        // across, 15 mm up from the base, which is 10 mm above the line.
        let m = d.structures.iter().find(|m| m.id == "T2").unwrap();
        let (_, f, warnings) = layout::continuous(&d, &options);
        assert!(warnings.is_empty(), "{warnings:?}");
        let base = [
            f.left + (m.station - f.s0) * f.h,
            f.plan_top + f.plan_h / 2.0 - 10.0,
        ];
        let ys: Vec<f64> = t2
            .iter()
            .flat_map(|p| p.points.iter().map(|q| q[1]))
            .collect();
        let lowest = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let highest = ys.iter().copied().fold(f64::INFINITY, f64::min);
        assert!((lowest - base[1]).abs() < 0.5, "{lowest} {base:?}");
        assert!((lowest - highest - 15.0).abs() < 1e-6, "{lowest} {highest}");
        // Every structure has one; leaders dashed.
        for s in &d.structures {
            let title = format!("{} elevation", s.id);
            assert!(
                on.iter().any(|p| p.title.as_deref() == Some(&title)),
                "{title}"
            );
        }
        assert!(on.iter().any(|p| p.title.is_none() && p.pen.dash.is_some()));
        // In every format's layer: DXF and SVG.
        assert!(crate::dxf::write(&sheet).contains("OHL_ELEVATIONS"));
        assert!(sheet.svg().contains(r#"class="OHL_ELEVATIONS""#));
        // Below, far down: into the profile, kept, warned about.
        options.elevations = Elevations {
            place: ElevationPlace::Below,
            offset_mm: 25.0,
            rotation_deg: 90.0,
        };
        let w = warnings_of(&d, &options);
        assert!(
            w.iter().any(|w| w.contains("reaches into the profile")),
            "{w:?}"
        );
        assert!(
            paths_on(&drawing(&d, &options), Layer::Elevations)
                .iter()
                .any(|p| p.title.as_deref() == Some("T2 elevation"))
        );
        // A structure 3 m from T2 (a connection beside it): their
        // elevations overlap, warned about (no warning above without it).
        options.elevations.place = ElevationPlace::Above;
        assert!(warnings_of(&d, &options).is_empty());
        let mut close = d.clone();
        let mut twin = m.clone();
        twin.id = "T2b".into();
        twin.station += 3.0;
        close.structures.push(twin);
        let w = warnings_of(&close, &options);
        assert!(w.iter().any(|w| w.contains("T2 and T2b overlap")), "{w:?}");
        // On sheets too.
        let s = sheets(&close, &options, &page_format("A3").unwrap());
        assert!(
            s.iter()
                .any(|s| s.warnings.iter().any(|w| w.contains("overlap")))
        );
        // Lying along the line (0°): its height along the route.
        options.elevations.rotation_deg = 0.0;
        let on = paths_on(&drawing(&d, &options), Layer::Elevations);
        let xs: Vec<f64> = on
            .iter()
            .filter(|p| p.title.as_deref() == Some("T2 elevation"))
            .flat_map(|p| p.points.iter().map(|q| q[0]))
            .collect();
        let span = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - xs.iter().copied().fold(f64::INFINITY, f64::min);
        assert!((span - 15.0).abs() < 1e-6, "{span}");
    }

    fn warnings_of(d: &RouteDrawing, o: &Options) -> Vec<String> {
        super::warnings(d, o)
    }

    #[test]
    fn stations_round_before_the_kilometre() {
        assert_eq!(format_station(1999.96), "2+000.0");
        assert_eq!(format_station(1250.04), "1+250.0");
        assert_eq!(format_station(12.3), "0+012.3");
    }

    #[test]
    fn stations_and_scales_parse() {
        assert_eq!(parse_station("1+250.5"), Some(1250.5));
        assert_eq!(parse_station("310"), Some(310.0));
        assert_eq!(parse_station("-5"), Some(-5.0));
        assert_eq!(parse_station("x"), None);
        assert_eq!(parse_scale("1:2000"), Some(2000.0));
        assert_eq!(parse_scale("200"), Some(200.0));
        assert_eq!(parse_scale("1:0"), None);
    }

    #[test]
    fn route_drawings_hold_their_spans_and_ground() {
        let (line, shapes, jumpers, terrain) = solved();
        let options = Options {
            side_offsets: vec![-10.0, 10.0],
            clearance: Some(7.0),
            ..Options::default()
        };
        let routes = line.alignment_views();
        let main = build(
            &line,
            &routes[0],
            &shapes,
            &jumpers,
            Some(&terrain),
            &[],
            &options,
        )
        .unwrap();
        // Main route T1–T5: 4 spans each of S1 and S2, plus the jumper at T3.
        assert_eq!(
            main.wires
                .iter()
                .filter(|w| w.label.starts_with("S1"))
                .count(),
            4
        );
        assert_eq!(
            main.wires
                .iter()
                .filter(|w| w.label.starts_with("S2"))
                .count(),
            4
        );
        assert!(main.wires.iter().any(|w| w.label.starts_with("jumper")));
        assert!(main.wires.iter().all(|w| !w.label.starts_with("S3")));
        assert!(main.wires.iter().all(|w| w.xy.len() == w.points.len()));
        // Structures at their stations; T1 at 0.
        assert_eq!(main.structures.len(), 5);
        assert!(main.structures[0].station.abs() < 1e-9);
        // Ground covers the whole route, one run, on the surface.
        assert_eq!(main.ground.len(), 1);
        let run = &main.ground[0];
        assert!(run[0][0].abs() < 1e-6 && (run.last().unwrap()[0] - main.length).abs() < 1e-6);
        let centre = routes[0].centreline(&line);
        for p in run.iter().step_by(17) {
            let q = point_at(&centre, p[0], 0.0);
            assert!((terrain.height(q[0], q[1]).unwrap() - p[1]).abs() < 1e-6);
        }
        assert_eq!(main.sides.len(), 2);
        // Wires are above the ground everywhere at 15 °C (6.4 m minimum).
        let svg_text = svg(&main, &options);
        assert!(svg_text.starts_with("<svg") && svg_text.contains("ground + 7 m"));
        assert!(svg_text.contains("0+310.0")); // T2's station label
        let csv = profile_csv(&main, &options);
        assert!(csv.lines().count() > 500);
        // Branch route: only S3.
        let branch = build(
            &line,
            &routes[1],
            &shapes,
            &jumpers,
            Some(&terrain),
            &[],
            &options,
        )
        .unwrap();
        assert!(
            branch
                .wires
                .iter()
                .filter(|w| !w.label.starts_with("jumper"))
                .all(|w| w.label.starts_with("S3"))
        );
        assert_eq!(
            branch
                .wires
                .iter()
                .filter(|w| w.label.starts_with("S3"))
                .count(),
            2
        );
    }

    #[test]
    fn sheets_split_at_structures() {
        let options = Options {
            clearance: Some(7.0),
            plan_offset_scale: Some(1000.0),
            title: vec!["Synthetic".into()],
            logos: vec![Logo {
                href: "data:image/svg+xml;base64,PHN2Zy8+".into(),
                width: 20.0,
            }],
            ..Options::default()
        };
        let d = main_route(&options);
        // A3 at 1:2000 holds 386 mm = 772 m: two spans per sheet (spans of
        // 310–330 m) — T1–T3 and T3–T5, the second starting at the match
        // structure T3. At 1:500 vertically the profile fits.
        let a3 = page_format("A3").unwrap();
        let fits = Options {
            vertical_scale: 500.0,
            ..options.clone()
        };
        let sheets = sheets(&d, &fits, &a3);
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[0].structures, ("T1".to_owned(), "T3".to_owned()));
        assert_eq!(sheets[1].structures, ("T3".to_owned(), "T5".to_owned()));
        assert!((sheets[0].to - sheets[1].from).abs() < 1e-9);
        assert!(
            sheets.iter().all(|s| s.warnings.is_empty()),
            "{:?}",
            sheets[0].warnings
        );
        let first = sheets[0].svg();
        assert!(first.contains(r#"width="420.0mm""#) && first.contains("Sheet 1 / 2"));
        assert!(first.contains("data:image/svg+xml;base64,PHN2Zy8+"));
        assert!(first.contains("plan across 1:1000"));
        // A1 holds the whole route.
        assert_eq!(
            super::sheets(&d, &fits, &page_format("a1").unwrap()).len(),
            1
        );
        // A4: one span per sheet; at 1:200 the profile (40 m → 200 mm) does
        // not fit the sheet's height — warned.
        let a4 = page_format("A4").unwrap();
        let s = super::sheets(&d, &options, &a4);
        assert_eq!(s.len(), 4);
        assert!(
            s[0].warnings.iter().any(|w| w.contains("height")),
            "{:?}",
            s[0].warnings
        );
        // 1:500 horizontally on A4 (131 m): every span is too long — warned,
        // still drawn.
        let big = Options {
            horizontal_scale: 500.0,
            ..fits.clone()
        };
        let s = super::sheets(&d, &big, &a4);
        assert_eq!(s.len(), 4);
        assert!(s[0].warnings[0].contains("longer than the sheet"));
        // A station range: the profile alone from 0+200 to 0+900 on A4
        // (526 m per sheet) starts and ends at the range, split at T3
        // (0+640).
        let range = Options {
            stations: Some([200.0, 900.0]),
            plan: false,
            ..fits
        };
        let s = super::sheets(&d, &range, &a4);
        assert_eq!(
            s.len(),
            2,
            "{:?}",
            s.iter().map(|s| &s.structures).collect::<Vec<_>>()
        );
        assert_eq!(s[0].structures.0, "0+200.0");
        assert_eq!(s[1].structures.1, "0+900.0");
        assert!((s[0].from - 200.0).abs() < 1e-9 && (s[1].to - 900.0).abs() < 1e-9);
        let svg = s[0].svg();
        assert!(!svg.contains(">Plan<") && svg.contains("— profile"));
    }

    /// The main route of the synthetic line with an invented structure
    /// `C1` on the route but away from its alignment, at `station` and
    /// `offset` (more than 5 m: away), no points nor stick drawing.
    fn main_with_away(station: f64, offset: f64) -> RouteDrawing {
        let (mut line, shapes, jumpers, terrain) = solved();
        let centre = line.alignment_views()[0].centreline(&line);
        let xy = point_at(&centre, station, offset);
        let mut c = line.structure("T1").unwrap().clone();
        c.id = "C1".into();
        c.position = [xy[0], xy[1], 490.0];
        c.points.clear();
        c.members.clear();
        c.routes = vec!["main".into()];
        line.structures.push(c);
        let views = line.alignment_views();
        assert_eq!(views[0].away, ["C1"]);
        build(
            &line,
            &views[0],
            &shapes,
            &jumpers,
            Some(&terrain),
            &[],
            &Options::default(),
        )
        .unwrap()
    }

    fn ends(s: &[Sheet]) -> Vec<(&str, &str)> {
        s.iter()
            .map(|s| (s.structures.0.as_str(), s.structures.1.as_str()))
            .collect()
    }

    fn warnings(s: &[Sheet]) -> Vec<&Vec<String>> {
        s.iter().map(|s| &s.warnings).collect()
    }

    #[test]
    fn sheets_never_end_at_or_near_a_connection_beside_the_line() {
        // Main route: T1 0, T2 310, T3 640, T4 940, T5 1260 m (T3 on the
        // alignment point P2; T2 and T4 on the alignment between its
        // points). A3 at 1:2000 holds (420 − 34) mm × 2 m/mm = 772 m; the
        // clearance of 2 mm on paper is 4 m. Profile at 1:500 so the
        // height fits.
        let o = Options {
            vertical_scale: 500.0,
            ..Options::default()
        };
        let a3 = page_format("A3").unwrap();
        let plain = sheets(&main_route(&o), &o, &a3);
        assert_eq!(ends(&plain), [("T1", "T3"), ("T3", "T5")]);

        // C1 at 0+700 beside the line is the last structure within 772 m
        // of T1, so a greedy break would fall at C1 — a match line through
        // the span T3–T4. A connection never ends a sheet, wherever it
        // stands beside the line: within the line width (±40 m, drawn)
        // left or right, or outside it. The break stays at T3 and the
        // sheets are those without C1.
        for offset in [10.0, 20.0, 39.9, -39.9, 60.0] {
            let s = sheets(&main_with_away(700.0, offset), &o, &a3);
            assert_eq!(ends(&s), [("T1", "T3"), ("T3", "T5")], "{offset}");
            assert!(
                s.iter().all(|s| s.warnings.is_empty()),
                "{offset}: {:?}",
                warnings(&s)
            );
            assert_eq!(
                s.iter().map(|s| (s.from, s.to)).collect::<Vec<_>>(),
                plain.iter().map(|s| (s.from, s.to)).collect::<Vec<_>>()
            );
        }

        // C1 20 m beside the line, 2 m past T3 (inside the 4 m): drawn
        // (within the line width), so a break at T3 would cut C1's
        // drawing; it moves back to T2 (the last free structure within
        // reach); from T2 (reach to 1082 m) T3 and C1 are skipped, T4
        // (940) fits; T4–T5 ends the route. No warnings: every sheet fits.
        let s = sheets(&main_with_away(642.0, 20.0), &o, &a3);
        assert_eq!(ends(&s), [("T1", "T2"), ("T2", "T4"), ("T4", "T5")]);
        assert!(
            s.iter().all(|s| s.warnings.is_empty()),
            "{:?}",
            warnings(&s)
        );
        let c1 = 642.0;
        for sheet in &s {
            for at in [sheet.from, sheet.to] {
                assert!((at - c1).abs() >= 4.0, "break at {at}");
            }
        }
        // 60 m beside it, outside the line width: not drawn, so it keeps
        // no break away: the sheets are those without C1.
        let s = sheets(&main_with_away(642.0, 60.0), &o, &a3);
        assert_eq!(ends(&s), [("T1", "T3"), ("T3", "T5")]);

        // No free break fits: A4 holds (297 − 34) × 2 = 526 m; C1 2 m
        // past T2 blocks T2 and T3 (640) lies beyond the reach. The sheet
        // runs on to T3 (640 m) and says so; the others are as before.
        let a4 = page_format("A4").unwrap();
        let s = sheets(&main_with_away(312.0, 20.0), &o, &a4);
        assert_eq!(ends(&s), [("T1", "T3"), ("T3", "T4"), ("T4", "T5")]);
        let w = &s[0].warnings;
        assert!(
            w.iter().any(|w| w.starts_with("no free sheet break")
                && w.contains(
                    "C1 stands beside the line, 20.0 m right of the alignment (not on it)"
                )
                && w.contains("T2 is within 4.0 m of C1")
                && w.contains("runs on to T3 (640 m)")),
            "{w:?}"
        );
        assert!(!w.iter().any(|w| w.contains("is longer than")), "{w:?}");
        // (On A4 the profile is taller than the sheet: those are warned.)
        let others = |s: &Sheet| {
            s.warnings
                .iter()
                .filter(|w| !w.contains("of height"))
                .count()
        };
        assert!(s[1..].iter().all(|s| others(s) == 0), "{:?}", warnings(&s));

        // A station range keeps its ends even beside C1.
        let range = Options {
            stations: Some([100.0, 1100.0]),
            ..o.clone()
        };
        let s = sheets(&main_with_away(642.0, 20.0), &range, &a3);
        assert_eq!(s[0].structures.0, "0+100.0");
        assert_eq!(s.last().unwrap().structures.1, "1+100.0");
        assert!(
            s.iter()
                .all(|s| s.structures.1 != "T3" && s.structures.1 != "C1")
        );
    }

    /// The main route of the synthetic line with an invented free
    /// alignment point `P9` (no structure) on its alignment at `station`
    /// (on the straight between P1 and P2, or P2 and P3: the stations do
    /// not change).
    fn main_with_free_point(station: f64) -> RouteDrawing {
        let (mut line, shapes, jumpers, terrain) = solved();
        let centre = line.alignment_views()[0].centreline(&line);
        let xy = point_at(&centre, station, 0.0);
        line.alignment_points
            .push(AlignmentPoint::free("P9", [xy[0], xy[1], 500.0]));
        let points = &mut line.alignments[0].points;
        let p2 = points.iter().position(|p| p == "P2").unwrap();
        points.insert(if station < 640.0 { p2 } else { p2 + 1 }, "P9".into());
        let views = line.alignment_views();
        build(
            &line,
            &views[0],
            &shapes,
            &jumpers,
            Some(&terrain),
            &[],
            &Options::default(),
        )
        .unwrap()
    }

    #[test]
    fn a_sheet_ends_at_a_structure_on_the_line_or_a_free_alignment_point() {
        let o = Options {
            vertical_scale: 500.0,
            ..Options::default()
        };
        let a3 = page_format("A3").unwrap();
        // The alignment's points: P1 (0, at T1, free), P2 (640, T3), P3
        // (the end, at T5, free). A free point at either end of the
        // structures is no break of its own: the sheets start at T1 and
        // end at T5.
        let d = main_route(&o);
        let points: Vec<(&str, Option<&str>)> = d
            .alignment
            .iter()
            .map(|p| (p.id.as_str(), p.structure.as_deref()))
            .collect();
        assert_eq!(points, [("P1", None), ("P2", Some("T3")), ("P3", None)]);
        // T3 stands on the alignment point P2: the first sheet ends there.
        assert_eq!(ends(&sheets(&d, &o, &a3)), [("T1", "T3"), ("T3", "T5")]);
        // A free alignment point (an angle point without a pylon) at 0+700
        // is the last place within the 772 m of T1, 60 m from T3 and
        // 240 m from T4: the sheet ends there, the next one starts there
        // (700 + 772 = 1472 m holds T5 at 1260).
        let s = sheets(&main_with_free_point(700.0), &o, &a3);
        assert_eq!(ends(&s), [("T1", "P9"), ("P9", "T5")]);
        assert!((s[0].to - 700.0).abs() < 1e-6, "{}", s[0].to);
        assert!(
            s.iter().all(|s| s.warnings.is_empty()),
            "{:?}",
            warnings(&s)
        );
        // 3 m past T3 it is within the 4 m of T3's drawing: no break there;
        // T3 ends the sheet (a free point blocks no structure).
        let s = sheets(&main_with_free_point(643.0), &o, &a3);
        assert_eq!(ends(&s), [("T1", "T3"), ("T3", "T5")]);
        // On A4 (526 m from T1) the free point at 0+500 ends the first
        // sheet instead of T2 (310); from it T4 (940) fits, then T5.
        let a4 = page_format("A4").unwrap();
        let s = sheets(&main_with_free_point(500.0), &o, &a4);
        assert_eq!(ends(&s), [("T1", "P9"), ("P9", "T4"), ("T4", "T5")]);
        // Its reason when blocked is named in a run-on warning: C1 20 m
        // beside the line at 0+312 blocks T2, the free point at 0+315
        // likewise; T3 lies beyond the 526 m.
        let mut d = main_with_free_point(315.0);
        let with_c1 = main_with_away(312.0, 20.0);
        d.structures = with_c1.structures;
        let s = sheets(&d, &o, &a4);
        assert_eq!(s[0].structures, ("T1".to_owned(), "T3".to_owned()));
        let w = &s[0].warnings;
        assert!(
            w.iter().any(|w| w.contains("P9 is within 4.0 m of C1")
                && w.contains("T2 is within 4.0 m of C1")),
            "{w:?}"
        );
    }

    #[test]
    fn the_line_width_decides_what_is_drawn() {
        // A connection beside the line is drawn within the line width
        // (±40 m by default), not outside it; the structures of the run
        // always are.
        let ids = |d: &RouteDrawing| -> Vec<String> {
            d.structures.iter().map(|m| m.id.clone()).collect()
        };
        // The synthetic example has no structure beside the line and no
        // wire beyond ±40 m: every route is drawn whole, as before (its
        // free alignment points P1 and P3 lie at its first and last
        // structure: no breaks of their own, its sheets unchanged).
        let (line, shapes, jumpers, terrain) = solved();
        for route in line.alignment_views() {
            let o = Options::default();
            let d = build(&line, &route, &shapes, &jumpers, Some(&terrain), &[], &o).unwrap();
            assert_eq!(d.shown(o.plan_half_width), d, "{}", route.id);
        }
        let d = main_with_away(700.0, 39.9);
        assert!(ids(&d.shown(40.0)).contains(&"C1".to_owned()));
        assert!(!ids(&d.shown(30.0)).contains(&"C1".to_owned()));
        let d = main_with_away(700.0, -40.1);
        assert!(!ids(&d.shown(40.0)).contains(&"C1".to_owned()));
        // (Everything is kept in the drawing data itself.)
        assert!(ids(&d).contains(&"C1".to_owned()));
        assert_eq!(
            ids(&d.shown(0.5)),
            ["T1", "T2", "T3", "T4", "T5"].map(String::from)
        );
        // Wires are cut where they leave the width, the crossing
        // interpolated: a wire out to 60 m and back at width 40 gives two
        // runs ending at ±40 m, one across the width from −60 to +60 m
        // its part from −40 to +40 m.
        let trace = |pts: &[[f64; 3]]| Trace {
            label: "w".into(),
            color: "#000000".into(),
            points: pts.to_vec(),
            xy: pts.iter().map(|p| [p[0], p[1]]).collect(),
            dash: None,
        };
        let w = trace(&[
            [0.0, 0.0, 10.0],
            [10.0, 20.0, 12.0],
            [20.0, 60.0, 16.0],
            [30.0, 20.0, 12.0],
            [40.0, 0.0, 10.0],
        ]);
        let runs = cut_trace(&w, 40.0);
        assert_eq!(runs.len(), 2);
        // 20 → 60 m between stations 10 and 20: 40 m at 15, z 14.
        assert_eq!(
            runs[0].points,
            [[0.0, 0.0, 10.0], [10.0, 20.0, 12.0], [15.0, 40.0, 14.0]]
        );
        assert_eq!(runs[0].xy.last(), Some(&[15.0, 40.0]));
        assert_eq!(
            runs[1].points,
            [[25.0, 40.0, 14.0], [30.0, 20.0, 12.0], [40.0, 0.0, 10.0]]
        );
        let across = cut_trace(&trace(&[[0.0, -60.0, 0.0], [12.0, 60.0, 12.0]]), 40.0);
        assert_eq!(across.len(), 1);
        assert_eq!(across[0].points, [[2.0, -40.0, 2.0], [10.0, 40.0, 10.0]]);
        assert!(cut_trace(&trace(&[[0.0, 50.0, 0.0], [1.0, 60.0, 0.0]]), 40.0).is_empty());
        assert_eq!(cut_trace(&w, 60.0), std::slice::from_ref(&w));
    }

    #[test]
    fn the_break_clearance_is_a_setting() {
        let o = Options {
            vertical_scale: 500.0,
            ..Options::default()
        };
        let a3 = page_format("A3").unwrap();
        // C1 (beside the line, within the line width) 2 m past T3 (640).
        // At the default 2 mm on paper (4 m at 1:2000) T3 is blocked:
        // T1–T2, T2–T4, T4–T5.
        let d = main_with_away(642.0, 20.0);
        assert_eq!(o.break_clearance, BREAK_CLEARANCE_MM);
        assert_eq!(
            ends(&sheets(&d, &o, &a3)),
            [("T1", "T2"), ("T2", "T4"), ("T4", "T5")]
        );
        // At 0.5 mm (1 m) the 2 m between T3 and C1 are enough.
        let close = Options {
            break_clearance: 0.5,
            ..o.clone()
        };
        assert_eq!(ends(&sheets(&d, &close, &a3)), [("T1", "T3"), ("T3", "T5")]);
        // At 1.5 mm (3 m) not.
        let one = Options {
            break_clearance: 1.5,
            ..o.clone()
        };
        assert_eq!(
            ends(&sheets(&d, &one, &a3)),
            [("T1", "T2"), ("T2", "T4"), ("T4", "T5")]
        );
        // Checked: 0 to 20 mm.
        assert_eq!(check_break_clearance(0.0), Ok(0.0));
        assert_eq!(check_break_clearance(MAX_BREAK_CLEARANCE_MM), Ok(20.0));
        for bad in [-0.1, 20.5, f64::NAN, f64::INFINITY] {
            assert!(check_break_clearance(bad).is_err(), "{bad}");
        }
        // The export refuses an invalid one (before writing anything).
        let dir =
            std::env::temp_dir().join(format!("ohl-drawing-clearance-{}", std::process::id()));
        let bad = Options {
            break_clearance: -1.0,
            ..o.clone()
        };
        let e = export::Export {
            page: Some(a3.clone()),
            ..Default::default()
        };
        let err =
            export::write_route(&d, &bad, &e, &dir, &ohl_core::budget::Budget::new()).unwrap_err();
        assert!(err.contains("0 to 20 mm"), "{err}");
        assert!(!dir.exists());
    }

    #[test]
    fn the_sheet_layout_is_that_of_the_sheets() {
        let o = Options {
            vertical_scale: 500.0,
            ..Options::default()
        };
        let a4 = page_format("A4").unwrap();
        let d = main_with_away(312.0, 20.0);
        let layout = sheet_layout(&d, &o, &a4);
        let drawn = sheets(&d, &o, &a4);
        assert_eq!(layout.len(), drawn.len());
        for (l, s) in layout.iter().zip(&drawn) {
            assert_eq!(
                (l.number, l.from, l.to, &l.structures, &l.warnings),
                (s.number, s.from, s.to, &s.structures, &s.warnings)
            );
            assert!((l.reach - 526.0).abs() < 1e-9, "{}", l.reach);
        }
        assert!(layout[0].warnings[0].starts_with("no free sheet break"));
    }

    #[test]
    fn page_formats_and_base64() {
        assert_eq!(page_format("A1").unwrap().width, 841.0);
        assert_eq!(page_format("1000x600").unwrap().height, 600.0);
        assert!(page_format("B7").is_none());
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn stations_format() {
        assert_eq!(format_station(0.0), "0+000.0");
        assert_eq!(format_station(1250.58), "1+250.6");
        assert_eq!(nice_step(1300.0, 13.0), 100.0);
    }
}
