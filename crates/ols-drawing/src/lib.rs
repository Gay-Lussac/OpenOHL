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
//! [`build`] collects the drawing data; [`svg`] draws a sheet with
//! separate horizontal and vertical scales (e.g. 1:2000 / 1:200);
//! [`profile_csv`] writes the ground and wire profiles.

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

/// The plan and profile sheet as SVG (units mm at the given scales).
pub fn svg(d: &RouteDrawing, o: &Options) -> String {
    // Extent along the route: structures, wires and ground.
    let mut s0 = 0.0f64;
    let mut s1 = d.length;
    let mut z0 = f64::INFINITY;
    let mut z1 = f64::NEG_INFINITY;
    let mut see = |s: f64, z: f64| {
        s0 = s0.min(s);
        s1 = s1.max(s);
        z0 = z0.min(z);
        z1 = z1.max(z);
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
    let z_step = nice_step(z1 - z0, 8.0);
    let (z0, z1) = (
        (z0 / z_step).floor() * z_step,
        (z1 / z_step).ceil() * z_step + z_step,
    );
    let (h, v) = (1000.0 / o.horizontal_scale, 1000.0 / o.vertical_scale); // mm per m
    let margin = 30.0;
    let title_h = 8.0 + 5.0 * o.title.len() as f64;
    let plan_h = 2.0 * o.plan_half_width * h;
    let gap = 15.0;
    let profile_h = (z1 - z0) * v;
    let width = margin * 2.0 + (s1 - s0) * h;
    let height = title_h + plan_h + gap + profile_h + 2.0 * margin;
    let x = |s: f64| margin + (s - s0) * h;
    let plan_top = margin + title_h;
    let plan_y = |offset: f64| plan_top + o.plan_half_width * h + offset * h;
    let prof_top = plan_top + plan_h + gap;
    let y = |z: f64| prof_top + (z1 - z) * v;

    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.1}mm" height="{height:.1}mm" viewBox="0 0 {width:.1} {height:.1}" font-family="Helvetica, Arial, sans-serif">"#
    );
    let _ = writeln!(out, r#"<rect width="100%" height="100%" fill="white"/>"#);
    // Title.
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
        r#"<text x="{:.1}" y="{:.1}" font-size="3" text-anchor="end">horizontal 1:{} · vertical 1:{} (×{:.0})</text>"#,
        width - margin,
        margin + 5.0,
        o.horizontal_scale,
        o.vertical_scale,
        o.horizontal_scale / o.vertical_scale
    );

    // Grids: stations (both panels), elevations (profile).
    let s_step = nice_step(s1 - s0, ((s1 - s0) * h / 40.0).max(1.0));
    let mut s = (s0 / s_step).ceil() * s_step;
    while s <= s1 {
        let _ = writeln!(
            out,
            r##"<line x1="{0:.2}" y1="{1:.2}" x2="{0:.2}" y2="{2:.2}" stroke="#ddd" stroke-width="0.2"/><line x1="{0:.2}" y1="{3:.2}" x2="{0:.2}" y2="{4:.2}" stroke="#ddd" stroke-width="0.2"/><text x="{0:.2}" y="{5:.2}" font-size="2.5" text-anchor="middle">{6}</text>"##,
            x(s),
            plan_top,
            plan_top + plan_h,
            prof_top,
            prof_top + profile_h,
            prof_top + profile_h + 4.0,
            format_station(s)
        );
        s += s_step;
    }
    let mut z = z0;
    while z <= z1 + 1e-9 {
        let _ = writeln!(
            out,
            r##"<line x1="{0:.2}" y1="{1:.2}" x2="{2:.2}" y2="{1:.2}" stroke="#ddd" stroke-width="0.2"/><text x="{3:.2}" y="{4:.2}" font-size="2.5" text-anchor="end">{5:.0}</text>"##,
            x(s0),
            y(z),
            x(s1),
            x(s0) - 1.5,
            y(z) + 0.9,
            z
        );
        z += z_step;
    }
    let _ = writeln!(
        out,
        r##"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="none" stroke="#888" stroke-width="0.3"/><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="none" stroke="#888" stroke-width="0.3"/>"##,
        x(s0),
        plan_top,
        x(s1) - x(s0),
        plan_h,
        x(s0),
        prof_top,
        x(s1) - x(s0),
        profile_h
    );
    let _ = writeln!(
        out,
        r#"<text x="{:.2}" y="{:.2}" font-size="3" font-weight="bold">Plan</text><text x="{:.2}" y="{:.2}" font-size="3" font-weight="bold">Profile</text>"#,
        x(s0) + 1.0,
        plan_top + 3.5,
        x(s0) + 1.0,
        prof_top + 3.5
    );

    let path = |pts: &mut dyn Iterator<Item = (f64, f64)>| {
        let mut d = String::new();
        for (k, (px, py)) in pts.enumerate() {
            let _ = write!(d, "{}{px:.2},{py:.2} ", if k == 0 { "M" } else { "L" });
        }
        d
    };

    // Plan: centreline, wires, structures.
    let _ = writeln!(
        out,
        r##"<line x1="{0:.2}" y1="{1:.2}" x2="{2:.2}" y2="{1:.2}" stroke="#999" stroke-width="0.25" stroke-dasharray="4 1 1 1"/>"##,
        x(0.0),
        plan_y(0.0),
        x(d.length)
    );
    for w in &d.wires {
        let _ = writeln!(
            out,
            r#"<path d="{}" fill="none" stroke="{}" stroke-width="0.35"><title>{}</title></path>"#,
            path(&mut w.points.iter().map(|p| (x(p[0]), plan_y(p[1])))),
            w.color,
            escape(&w.label)
        );
    }
    for m in &d.structures {
        let (cx, cy) = (x(m.station), plan_y(m.offset));
        let _ = writeln!(
            out,
            r##"<rect x="{:.2}" y="{:.2}" width="2" height="2" fill="none" stroke="#333" stroke-width="0.3"/><text x="{cx:.2}" y="{:.2}" font-size="2.5" text-anchor="middle">{}</text>"##,
            cx - 1.0,
            cy - 1.0,
            plan_top + plan_h - 1.5,
            escape(&m.id)
        );
    }

    // Profile: side ground, ground, clearance line, structures, wires.
    for (offset, runs) in &d.sides {
        for run in runs {
            let _ = writeln!(
                out,
                r##"<path d="{}" fill="none" stroke="#b08850" stroke-width="0.25" stroke-dasharray="1.5 1"><title>ground at offset {offset:+.1} m</title></path>"##,
                path(&mut run.iter().map(|p| (x(p[0]), y(p[1]))))
            );
        }
    }
    for run in &d.ground {
        let _ = writeln!(
            out,
            r##"<path d="{}" fill="none" stroke="#7a4a1a" stroke-width="0.45"><title>ground (centreline)</title></path>"##,
            path(&mut run.iter().map(|p| (x(p[0]), y(p[1]))))
        );
        if let Some(c) = o.clearance {
            let _ = writeln!(
                out,
                r##"<path d="{}" fill="none" stroke="#d22" stroke-width="0.3" stroke-dasharray="3 1.5"><title>ground + {c} m</title></path>"##,
                path(&mut run.iter().map(|p| (x(p[0]), y(p[1] + c))))
            );
        }
    }
    for m in &d.structures {
        let cx = x(m.station);
        let _ = writeln!(
            out,
            r##"<line x1="{cx:.2}" y1="{:.2}" x2="{cx:.2}" y2="{:.2}" stroke="#333" stroke-width="0.6"/><text x="{cx:.2}" y="{:.2}" font-size="2.5" text-anchor="middle">{}</text><text x="{cx:.2}" y="{:.2}" font-size="2" text-anchor="middle" fill="#555">{}</text>"##,
            y(m.base),
            y(m.top),
            y(m.top) - 4.0,
            escape(&m.id),
            y(m.top) - 1.5,
            format_station(m.station)
        );
        for a in &m.attachments {
            let _ = writeln!(
                out,
                r##"<circle cx="{:.2}" cy="{:.2}" r="0.4" fill="#333"/>"##,
                x(a[0]),
                y(a[2])
            );
        }
    }
    for w in &d.wires {
        let _ = writeln!(
            out,
            r#"<path d="{}" fill="none" stroke="{}" stroke-width="0.35"><title>{}</title></path>"#,
            path(&mut w.points.iter().map(|p| (x(p[0]), y(p[2])))),
            w.color,
            escape(&w.label)
        );
    }
    for (p, label) in &d.markers {
        let _ = writeln!(
            out,
            r##"<circle cx="{:.2}" cy="{:.2}" r="1.2" fill="none" stroke="#d00" stroke-width="0.5"><title>{}</title></circle><circle cx="{:.2}" cy="{:.2}" r="1.2" fill="none" stroke="#d00" stroke-width="0.5"><title>{}</title></circle>"##,
            x(p[0]),
            y(p[2]),
            escape(label),
            x(p[0]),
            plan_y(p[1]),
            escape(label)
        );
    }
    out.push_str("</svg>\n");
    out
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
    fn stations_format() {
        assert_eq!(format_station(0.0), "0+000.0");
        assert_eq!(format_station(1250.58), "1+250.6");
        assert_eq!(nice_step(1300.0, 13.0), 100.0);
    }
}
