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
//!   the route's wires (spans between consecutive structures of the route,
//!   and jumpers at its structures) coloured by phase, and markers.
//! - **Plan**: straightened along the route (station × offset, true
//!   scale), so it lines up with the profile: centreline, corridor edges,
//!   structures, wires.
//!
//! [`build`] collects the drawing data; [`svg`] draws the whole route as
//! one continuous drawing; [`sheets`] splits it **at structures** over
//! sheets of a standard format (A0–A4 or custom) with a title block and
//! logos; [`profile_csv`] writes the ground and wire profiles. Scales:
//! horizontal (along the route, plan and profile), vertical (profile) and
//! plan across the line, each set separately.
//!
//! The sheet layout (title block, margins) is fixed here; a layout plugin
//! with company templates can replace it later.

use std::fmt::Write as _;

use ols_line::jumper::JumperShape;
use ols_line::route::project;
use ols_line::solve::SectionShape;
use ols_line::{Line, Point, Route, Station};
use ols_terrain::Terrain;

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
    /// Half width of the plan view, m.
    pub plan_half_width: f64,
    /// Title lines (line name, weather case…).
    pub title: Vec<String>,
    /// Scale denominator of the plan across the line (default: the
    /// horizontal scale; along the line the plan always uses the horizontal
    /// scale to stay aligned with the profile).
    pub plan_offset_scale: Option<f64>,
    /// Logos for the title block of paged sheets.
    pub logos: Vec<Logo>,
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
    /// Ground (base) height.
    pub base: f64,
    /// Highest attachment height.
    pub top: f64,
    /// Attachment points: station, offset, z.
    pub attachments: Vec<[f64; 3]>,
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
    route: &Route,
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
                base: s.position[2],
                top: s
                    .attachments
                    .iter()
                    .map(|a| a.insulator_point[2])
                    .fold(s.position[2], f64::max),
                attachments: s
                    .attachments
                    .iter()
                    .map(|a| to_route(&a.insulator_point))
                    .collect(),
            }
        })
        .collect();
    // Spans of the route: between consecutive structures of the route.
    let consecutive = |a: &str, b: &str| {
        route
            .structures
            .windows(2)
            .any(|w| (w[0] == a && w[1] == b) || (w[0] == b && w[1] == a))
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
                    label: format!(
                        "{} span {k} ({} {})",
                        section.id, section.circuit, section.phase_label
                    ),
                    color: color.clone(),
                    points: span.points.iter().map(to_route).collect(),
                });
            }
        }
    }
    for j in jumpers {
        let Some(def) = line.jumpers.iter().find(|d| d.id == j.jumper) else {
            continue;
        };
        if route.structures.contains(&def.from.structure)
            || route.structures.contains(&def.to.structure)
        {
            let color = def.color.clone().unwrap_or_else(|| {
                line.sections
                    .iter()
                    .find(|s| s.points.contains(&def.from))
                    .map_or(ols_line::NEUTRAL_COLOR.to_owned(), |s| {
                        line.section_color(s)
                    })
            });
            wires.push(Trace {
                label: format!("jumper {}", j.jumper),
                color,
                points: j.points.iter().map(to_route).collect(),
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
    Ok(RouteDrawing {
        route: route.id.clone(),
        name: route.name.clone(),
        length: polyline_length(&centre),
        stations,
        structures,
        ground,
        sides,
        wires,
        markers,
    })
}

/// Station as `km+m`, e.g. `1+250.0`.
pub fn format_station(s: f64) -> String {
    let km = (s / 1000.0).floor();
    format!("{}+{:05.1}", km as i64, s - km * 1000.0)
}

fn nice_step(range: f64, target: f64) -> f64 {
    let raw = range / target;
    let p = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * p)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * p)
}

fn escape(t: &str) -> String {
    t.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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
                out.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
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

/// Where things go on a sheet (all mm, except stations and heights).
struct Frame {
    /// Station range drawn.
    s0: f64,
    s1: f64,
    /// Left edge of the drawing area.
    left: f64,
    /// Plan panel top and height.
    plan_top: f64,
    plan_h: f64,
    /// Profile panel top and height.
    prof_top: f64,
    prof_h: f64,
    /// Height range of the profile.
    z0: f64,
    z1: f64,
    /// mm per m: along the route, across in the plan, vertical.
    h: f64,
    plan_k: f64,
    v: f64,
}

impl Frame {
    fn x(&self, s: f64) -> f64 {
        self.left + (s - self.s0) * self.h
    }
    fn plan_y(&self, offset: f64) -> f64 {
        self.plan_top + self.plan_h / 2.0 + offset * self.plan_k
    }
    fn y(&self, z: f64) -> f64 {
        self.prof_top + (self.z1 - z) * self.v
    }
    fn width(&self) -> f64 {
        (self.s1 - self.s0) * self.h
    }
}

/// Height range of what lies between stations `s0` and `s1`, rounded to
/// grid steps (with one step of room on top).
fn height_range(d: &RouteDrawing, s0: f64, s1: f64) -> (f64, f64, f64) {
    let (mut z0, mut z1) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut see = |s: f64, z: f64| {
        if s >= s0 - 1e-6 && s <= s1 + 1e-6 {
            z0 = z0.min(z);
            z1 = z1.max(z);
        }
    };
    for m in &d.structures {
        see(m.station, m.base);
        see(m.station, m.top);
    }
    for w in &d.wires {
        for p in &w.points {
            see(p[0], p[2]);
        }
    }
    for run in d.ground.iter().chain(d.sides.iter().flat_map(|(_, p)| p)) {
        for p in run {
            see(p[0], p[1]);
        }
    }
    if !z0.is_finite() {
        (z0, z1) = (0.0, 10.0);
    }
    let step = nice_step((z1 - z0).max(1.0), 8.0);
    (
        (z0 / step).floor() * step,
        (z1 / step).ceil() * step + step,
        step,
    )
}

fn path(points: impl Iterator<Item = (f64, f64)>) -> String {
    let mut d = String::new();
    for (k, (px, py)) in points.enumerate() {
        let _ = write!(d, "{}{px:.2},{py:.2} ", if k == 0 { "M" } else { "L" });
    }
    d
}

/// Grids, panels and content of one frame (clipped to it).
fn draw(out: &mut String, d: &RouteDrawing, o: &Options, f: &Frame, z_step: f64, clip: &str) {
    let (s0, s1) = (f.s0, f.s1);
    let _ = writeln!(
        out,
        r#"<defs><clipPath id="{clip}-plan"><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"/></clipPath><clipPath id="{clip}-prof"><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"/></clipPath></defs>"#,
        f.left,
        f.plan_top,
        f.width(),
        f.plan_h,
        f.left,
        f.prof_top,
        f.width(),
        f.prof_h
    );
    // Station grid in both panels, labels under the profile.
    let s_step = nice_step((s1 - s0).max(1.0), (f.width() / 40.0).max(1.0));
    let mut s = (s0 / s_step).ceil() * s_step;
    while s <= s1 + 1e-9 {
        let _ = writeln!(
            out,
            r##"<line x1="{0:.2}" y1="{1:.2}" x2="{0:.2}" y2="{2:.2}" stroke="#ddd" stroke-width="0.2"/><line x1="{0:.2}" y1="{3:.2}" x2="{0:.2}" y2="{4:.2}" stroke="#ddd" stroke-width="0.2"/><text x="{0:.2}" y="{5:.2}" font-size="2.5" text-anchor="middle">{6}</text>"##,
            f.x(s),
            f.plan_top,
            f.plan_top + f.plan_h,
            f.prof_top,
            f.prof_top + f.prof_h,
            f.prof_top + f.prof_h + 4.0,
            format_station(s)
        );
        s += s_step;
    }
    let mut z = f.z0;
    while z <= f.z1 + 1e-9 {
        let _ = writeln!(
            out,
            r##"<line x1="{0:.2}" y1="{1:.2}" x2="{2:.2}" y2="{1:.2}" stroke="#ddd" stroke-width="0.2"/><text x="{3:.2}" y="{4:.2}" font-size="2.5" text-anchor="end">{5:.0}</text>"##,
            f.left,
            f.y(z),
            f.left + f.width(),
            f.left - 1.5,
            f.y(z) + 0.9,
            z
        );
        z += z_step;
    }
    let _ = writeln!(
        out,
        r##"<rect x="{0:.2}" y="{1:.2}" width="{2:.2}" height="{3:.2}" fill="none" stroke="#888" stroke-width="0.3"/><rect x="{0:.2}" y="{4:.2}" width="{2:.2}" height="{5:.2}" fill="none" stroke="#888" stroke-width="0.3"/><text x="{6:.2}" y="{7:.2}" font-size="3" font-weight="bold">Plan</text><text x="{6:.2}" y="{8:.2}" font-size="3" font-weight="bold">Profile</text>"##,
        f.left,
        f.plan_top,
        f.width(),
        f.plan_h,
        f.prof_top,
        f.prof_h,
        f.left + 1.0,
        f.plan_top + 3.5,
        f.prof_top + 3.5
    );

    // Plan.
    let _ = writeln!(out, r#"<g clip-path="url(#{clip}-plan)">"#);
    let _ = writeln!(
        out,
        r##"<line x1="{0:.2}" y1="{1:.2}" x2="{2:.2}" y2="{1:.2}" stroke="#999" stroke-width="0.25" stroke-dasharray="4 1 1 1"/>"##,
        f.x(0.0),
        f.plan_y(0.0),
        f.x(d.length)
    );
    for w in &d.wires {
        let _ = writeln!(
            out,
            r#"<path d="{}" fill="none" stroke="{}" stroke-width="0.35"><title>{}</title></path>"#,
            path(w.points.iter().map(|p| (f.x(p[0]), f.plan_y(p[1])))),
            w.color,
            escape(&w.label)
        );
    }
    for m in &d.structures {
        let (cx, cy) = (f.x(m.station), f.plan_y(m.offset));
        let _ = writeln!(
            out,
            r##"<rect x="{:.2}" y="{:.2}" width="2" height="2" fill="none" stroke="#333" stroke-width="0.3"/><text x="{cx:.2}" y="{:.2}" font-size="2.5" text-anchor="middle">{}</text>"##,
            cx - 1.0,
            cy - 1.0,
            f.plan_top + f.plan_h - 1.5,
            escape(&m.id)
        );
    }
    out.push_str("</g>\n");

    // Profile.
    let _ = writeln!(out, r#"<g clip-path="url(#{clip}-prof)">"#);
    for (offset, runs) in &d.sides {
        for run in runs {
            let _ = writeln!(
                out,
                r##"<path d="{}" fill="none" stroke="#b08850" stroke-width="0.25" stroke-dasharray="1.5 1"><title>ground at offset {offset:+.1} m</title></path>"##,
                path(run.iter().map(|p| (f.x(p[0]), f.y(p[1]))))
            );
        }
    }
    for run in &d.ground {
        let _ = writeln!(
            out,
            r##"<path d="{}" fill="none" stroke="#7a4a1a" stroke-width="0.45"><title>ground (centreline)</title></path>"##,
            path(run.iter().map(|p| (f.x(p[0]), f.y(p[1]))))
        );
        if let Some(c) = o.clearance {
            let _ = writeln!(
                out,
                r##"<path d="{}" fill="none" stroke="#d22" stroke-width="0.3" stroke-dasharray="3 1.5"><title>ground + {c} m</title></path>"##,
                path(run.iter().map(|p| (f.x(p[0]), f.y(p[1] + c))))
            );
        }
    }
    for m in &d.structures {
        let cx = f.x(m.station);
        let _ = writeln!(
            out,
            r##"<line x1="{cx:.2}" y1="{:.2}" x2="{cx:.2}" y2="{:.2}" stroke="#333" stroke-width="0.6"/><text x="{cx:.2}" y="{:.2}" font-size="2.5" text-anchor="middle">{}</text><text x="{cx:.2}" y="{:.2}" font-size="2" text-anchor="middle" fill="#555">{}</text>"##,
            f.y(m.base),
            f.y(m.top),
            f.y(m.top) - 4.0,
            escape(&m.id),
            f.y(m.top) - 1.5,
            format_station(m.station)
        );
        for a in &m.attachments {
            let _ = writeln!(
                out,
                r##"<circle cx="{:.2}" cy="{:.2}" r="0.4" fill="#333"/>"##,
                f.x(a[0]),
                f.y(a[2])
            );
        }
    }
    for w in &d.wires {
        let _ = writeln!(
            out,
            r#"<path d="{}" fill="none" stroke="{}" stroke-width="0.35"><title>{}</title></path>"#,
            path(w.points.iter().map(|p| (f.x(p[0]), f.y(p[2])))),
            w.color,
            escape(&w.label)
        );
    }
    for (p, label) in &d.markers {
        let _ = writeln!(
            out,
            r##"<circle cx="{:.2}" cy="{:.2}" r="1.2" fill="none" stroke="#d00" stroke-width="0.5"><title>{}</title></circle>"##,
            f.x(p[0]),
            f.y(p[2]),
            escape(label)
        );
    }
    out.push_str("</g>\n");
    // Markers in the plan (outside the profile clip group).
    let _ = writeln!(out, r#"<g clip-path="url(#{clip}-plan)">"#);
    for (p, label) in &d.markers {
        let _ = writeln!(
            out,
            r##"<circle cx="{:.2}" cy="{:.2}" r="1.2" fill="none" stroke="#d00" stroke-width="0.5"><title>{}</title></circle>"##,
            f.x(p[0]),
            f.plan_y(p[1]),
            escape(label)
        );
    }
    out.push_str("</g>\n");
}

fn svg_open(out: &mut String, width: f64, height: f64) {
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.1}mm" height="{height:.1}mm" viewBox="0 0 {width:.1} {height:.1}" font-family="Helvetica, Arial, sans-serif">"#
    );
    let _ = writeln!(out, r#"<rect width="100%" height="100%" fill="white"/>"#);
}

fn scales_text(o: &Options) -> String {
    let across = o.plan_offset_scale.unwrap_or(o.horizontal_scale);
    let mut t = format!(
        "horizontal 1:{} · vertical 1:{} (×{:.0})",
        o.horizontal_scale,
        o.vertical_scale,
        o.horizontal_scale / o.vertical_scale
    );
    if (across - o.horizontal_scale).abs() > 1e-9 {
        t.push_str(&format!(" · plan across 1:{across}"));
    }
    t
}

/// The whole route as one continuous plan and profile SVG (units mm at
/// the given scales).
pub fn svg(d: &RouteDrawing, o: &Options) -> String {
    let (mut s0, mut s1) = (0.0f64, d.length);
    for m in &d.structures {
        s0 = s0.min(m.station);
        s1 = s1.max(m.station);
    }
    let (z0, z1, z_step) = height_range(d, s0, s1);
    let (h, v) = (1000.0 / o.horizontal_scale, 1000.0 / o.vertical_scale);
    let plan_k = 1000.0 / o.plan_offset_scale.unwrap_or(o.horizontal_scale);
    let margin = 30.0;
    let title_h = 8.0 + 5.0 * o.title.len() as f64;
    let plan_h = 2.0 * o.plan_half_width * plan_k;
    let f = Frame {
        s0,
        s1,
        left: margin,
        plan_top: margin + title_h,
        plan_h,
        prof_top: margin + title_h + plan_h + 15.0,
        prof_h: (z1 - z0) * v,
        z0,
        z1,
        h,
        plan_k,
        v,
    };
    let width = 2.0 * margin + f.width();
    let height = f.prof_top + f.prof_h + margin;
    let mut out = String::new();
    svg_open(&mut out, width, height);
    let _ = writeln!(
        out,
        r#"<text x="{margin}" y="{:.1}" font-size="5" font-weight="bold">Route {} {} — plan and profile</text>"#,
        margin + 5.0,
        escape(&d.route),
        escape(&d.name)
    );
    for (k, t) in o.title.iter().enumerate() {
        let _ = writeln!(
            out,
            r#"<text x="{margin}" y="{:.1}" font-size="3">{}</text>"#,
            margin + 10.0 + 5.0 * k as f64,
            escape(t)
        );
    }
    let _ = writeln!(
        out,
        r#"<text x="{:.1}" y="{:.1}" font-size="3" text-anchor="end">{}</text>"#,
        width - margin,
        margin + 5.0,
        scales_text(o)
    );
    draw(&mut out, d, o, &f, z_step, "all");
    out.push_str("</svg>\n");
    out
}

/// One sheet of a paged drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    /// Sheet number (1-based).
    pub number: usize,
    /// Station range.
    pub from: f64,
    /// End station.
    pub to: f64,
    /// First and last structure on the sheet.
    pub structures: (String, String),
    /// The sheet.
    pub svg: String,
    /// Problems: a span longer than the sheet, a profile taller than it.
    pub warnings: Vec<String>,
}

const SHEET_MARGIN: f64 = 10.0;
const TITLE_BLOCK: f64 = 32.0;
const AXIS_ROOM: f64 = 14.0;

/// The route split over sheets of `page`, **at structures**: each sheet
/// holds as many whole spans as fit at the horizontal scale, and the
/// structure where it ends starts the next sheet (match line). Each sheet
/// has its own height range and a title block (title lines, route, sheet
/// n/N, station range, scales, logos).
pub fn sheets(d: &RouteDrawing, o: &Options, page: &Page) -> Vec<Sheet> {
    let (h, v) = (1000.0 / o.horizontal_scale, 1000.0 / o.vertical_scale);
    let plan_k = 1000.0 / o.plan_offset_scale.unwrap_or(o.horizontal_scale);
    let left = SHEET_MARGIN + AXIS_ROOM;
    let usable = page.width - left - SHEET_MARGIN;
    let reach = usable / h; // metres per sheet
    let mut stations: Vec<(f64, String)> = d
        .structures
        .iter()
        .map(|m| (m.station, m.id.clone()))
        .collect();
    stations.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Break points: greedy, whole spans per sheet.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i + 1 < stations.len() {
        let mut j = i + 1;
        while j + 1 < stations.len() && stations[j + 1].0 - stations[i].0 <= reach + 1e-9 {
            j += 1;
        }
        ranges.push((i, j));
        i = j;
    }
    if ranges.is_empty() && !stations.is_empty() {
        ranges.push((0, 0));
    }
    let n = ranges.len();
    let plan_h = 2.0 * o.plan_half_width * plan_k;
    let draw_top = SHEET_MARGIN + 6.0;
    let draw_bottom = page.height - SHEET_MARGIN - TITLE_BLOCK - 8.0;
    ranges
        .iter()
        .enumerate()
        .map(|(k, &(a, b))| {
            let (s0, s1) = (stations[a].0, stations[b].0);
            let mut warnings = Vec::new();
            if (s1 - s0) * h > usable + 1e-6 {
                warnings.push(format!(
                    "span {}–{} ({:.0} m) is longer than the sheet holds at 1:{} ({:.0} m)",
                    stations[a].1,
                    stations[b].1,
                    s1 - s0,
                    o.horizontal_scale,
                    reach
                ));
            }
            let (z0, z1, z_step) = height_range(d, s0, s1);
            let prof_top = draw_top + plan_h + 8.0;
            let available = draw_bottom - prof_top;
            let needed = (z1 - z0) * v;
            if needed > available + 1e-6 {
                warnings.push(format!(
                    "profile {}–{} needs {:.0} mm of height at 1:{}, the sheet has {:.0} mm",
                    stations[a].1, stations[b].1, needed, o.vertical_scale, available
                ));
            }
            let f = Frame {
                s0,
                s1: s0 + reach.max(s1 - s0),
                left,
                plan_top: draw_top,
                plan_h,
                prof_top,
                // The panel takes the height its range needs (the profile is
                // clipped to the sheet when it needs more: see warnings).
                prof_h: available.min(needed).max(10.0),
                z0,
                z1,
                h,
                plan_k,
                v,
            };
            let mut out = String::new();
            svg_open(&mut out, page.width, page.height);
            let _ = writeln!(
                out,
                r##"<rect x="{0:.1}" y="{0:.1}" width="{1:.1}" height="{2:.1}" fill="none" stroke="#000" stroke-width="0.5"/>"##,
                SHEET_MARGIN / 2.0,
                page.width - SHEET_MARGIN,
                page.height - SHEET_MARGIN
            );
            draw(&mut out, d, o, &f, z_step, &format!("s{}", k + 1));
            // Match lines at the sheet's end structures.
            for x in [f.x(s0), f.x(s1)] {
                let _ = writeln!(
                    out,
                    r##"<line x1="{x:.2}" y1="{:.2}" x2="{x:.2}" y2="{:.2}" stroke="#06c" stroke-width="0.3" stroke-dasharray="2 1"/>"##,
                    f.plan_top,
                    f.prof_top + f.prof_h
                );
            }
            // Title block across the bottom: logos, titles, sheet data.
            let tb_top = page.height - SHEET_MARGIN / 2.0 - TITLE_BLOCK;
            let tb_left = SHEET_MARGIN / 2.0;
            let tb_w = page.width - SHEET_MARGIN;
            let _ = writeln!(
                out,
                r##"<rect x="{tb_left:.1}" y="{tb_top:.1}" width="{tb_w:.1}" height="{TITLE_BLOCK:.1}" fill="none" stroke="#000" stroke-width="0.4"/>"##
            );
            let mut cursor = tb_left + 3.0;
            for logo in &o.logos {
                let _ = writeln!(
                    out,
                    r#"<image x="{cursor:.1}" y="{:.1}" width="{:.1}" height="{:.1}" href="{}" preserveAspectRatio="xMinYMid meet"/>"#,
                    tb_top + 3.0,
                    logo.width,
                    TITLE_BLOCK - 6.0,
                    logo.href
                );
                cursor += logo.width + 4.0;
            }
            let text_x = cursor + 2.0;
            let _ = writeln!(
                out,
                r#"<text x="{text_x:.1}" y="{:.1}" font-size="4.5" font-weight="bold">Route {} {} — plan and profile</text>"#,
                tb_top + 7.0,
                escape(&d.route),
                escape(&d.name)
            );
            for (t, line) in o.title.iter().enumerate().take(3) {
                let _ = writeln!(
                    out,
                    r#"<text x="{text_x:.1}" y="{:.1}" font-size="3">{}</text>"#,
                    tb_top + 13.0 + 5.0 * t as f64,
                    escape(line)
                );
            }
            let right = tb_left + tb_w - 3.0;
            for (t, line) in [
                format!("Sheet {} / {n}", k + 1),
                format!(
                    "{} – {}   ({} – {})",
                    format_station(s0),
                    format_station(s1),
                    stations[a].1,
                    stations[b].1
                ),
                scales_text(o),
                format!("Format {} ({:.0} × {:.0} mm)", page.name, page.width, page.height),
            ]
            .iter()
            .enumerate()
            {
                let _ = writeln!(
                    out,
                    r#"<text x="{right:.1}" y="{:.1}" font-size="{}" text-anchor="end"{}>{}</text>"#,
                    tb_top + 7.0 + 6.0 * t as f64,
                    if t == 0 { 4.5 } else { 3.0 },
                    if t == 0 { r#" font-weight="bold""# } else { "" },
                    escape(line)
                );
            }
            out.push_str("</svg>\n");
            Sheet {
                number: k + 1,
                from: s0,
                to: s1,
                structures: (stations[a].1.clone(), stations[b].1.clone()),
                svg: out,
                warnings,
            }
        })
        .collect()
}

/// Ground and wire profiles as CSV: `kind,label,station,offset,z`
/// (`ground`, `ground_side`, `wire`).
pub fn profile_csv(d: &RouteDrawing) -> String {
    let mut out = String::from("kind,label,station_m,offset_m,z_m\n");
    for run in &d.ground {
        for p in run {
            let _ = writeln!(out, "ground,centreline,{:.3},0,{:.3}", p[0], p[1]);
        }
    }
    for (offset, runs) in &d.sides {
        for run in runs {
            for p in run {
                let _ = writeln!(
                    out,
                    "ground_side,{offset:+.1},{:.3},{offset:.3},{:.3}",
                    p[0], p[1]
                );
            }
        }
    }
    for w in &d.wires {
        for p in &w.points {
            let _ = writeln!(
                out,
                "wire,{},{:.3},{:.3},{:.3}",
                w.label.replace(',', ";"),
                p[0],
                p[1],
                p[2]
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ols_core::units::Temperature;
    use ols_line::route::point_at;
    use ols_weather::WeatherCase;
    use std::path::Path;

    fn solved() -> (Line, Vec<SectionShape>, Vec<JumperShape>, Terrain) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let line = Line::load(&root.join("synthetic-line.ols.json")).unwrap();
        let library = ols_cables::Library::load(&root.join("../../libraries/cables")).unwrap();
        let case = WeatherCase::still("t", Temperature::from_celsius(15.0));
        let shapes: Vec<_> = ols_line::solve::solve_line(&line, &library, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let jumpers: Vec<_> = ols_line::jumper::solve_jumpers(&line, &library, &case, &shapes)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let terrain = Terrain::load(&root.join("synthetic-survey.csv")).unwrap();
        (line, shapes, jumpers, terrain)
    }

    #[test]
    fn route_drawings_hold_their_spans_and_ground() {
        let (line, shapes, jumpers, terrain) = solved();
        let options = Options {
            side_offsets: vec![-10.0, 10.0],
            clearance: Some(7.0),
            ..Options::default()
        };
        let routes = line.effective_routes();
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
        let csv = profile_csv(&main);
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
        let (line, shapes, jumpers, terrain) = solved();
        let routes = line.effective_routes();
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
        let d = build(
            &line,
            &routes[0],
            &shapes,
            &jumpers,
            Some(&terrain),
            &[],
            &options,
        )
        .unwrap();
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
        let first = &sheets[0].svg;
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
            ..fits
        };
        let s = super::sheets(&d, &big, &a4);
        assert_eq!(s.len(), 4);
        assert!(s[0].warnings[0].contains("longer than the sheet"));
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
