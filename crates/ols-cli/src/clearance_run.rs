// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Clearance checks of `ols line`, for one or several weather cases.
//!
//! [`prepare`] runs once: rules (from a file, and `--clearance` as a
//! vertical ground rule), the class scheme, the ground surface (survey,
//! LiDAR tiles or a project's point cloud), the points of the classes the
//! rules need inside a corridor around the line's spans (kept in memory as
//! whole millimetres, exact for LiDAR), and the other lines. [`check`] then
//! runs per weather case: wires of this case against all of that.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ols_clearance::{
    PairResult, PointCheck, Rule, RuleSet, Target, Wire, WireResult, WireSet, wires_of,
};
use ols_line::Line;
use ols_line::export::ViewerExtras;
use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;
use ols_terrain::Terrain;
use ols_weather::WeatherCase;

/// Options of the clearance check.
pub struct Options<'a> {
    pub terrain: Option<&'a Path>,
    pub lidar: Option<&'a Path>,
    /// Project file with a point cloud, optionally `FILE#cloud-name`.
    pub pointcloud: Option<&'a str>,
    pub lidar_classes: Option<&'a Path>,
    pub ground_role: &'a str,
    pub corridor: f64,
    pub max_edge: f64,
    pub thin: Option<f64>,
    pub rules: Option<&'a Path>,
    pub clearance: Option<f64>,
    pub other_lines: &'a [PathBuf],
    pub structures: &'a Path,
    pub insulators: &'a Path,
}

/// What every weather case is checked against.
pub struct Prepared {
    rules: Vec<Rule>,
    scheme: ols_lidar::ClassScheme,
    /// Ground surface, if any.
    pub ground: Option<Terrain>,
    /// Points of the point rules' classes: millimetres from `origin`, class.
    points: Vec<([i32; 3], u8)>,
    origin: [f64; 3],
    /// Classes of each point rule (in rule order).
    point_classes: Vec<Vec<u8>>,
    other_lines: Vec<Line>,
}

/// Lines of the report for one result.
fn describe(r: &WireResult, rule: &Rule) -> String {
    let mut parts = Vec::new();
    if let Some(h) = r.vertical {
        parts.push(format!(
            "vertical {:.2} m{}",
            h.distance,
            match (rule.vertical, r.vertical_violations) {
                (Some(_), n) if n > 0 => format!(" ({n} below)"),
                _ => String::new(),
            }
        ));
    }
    if let Some(h) = r.radial {
        parts.push(format!(
            "radial {:.2} m{}",
            h.distance,
            match (rule.radial, r.radial_violations) {
                (Some(_), n) if n > 0 => format!(" ({n} below)"),
                _ => String::new(),
            }
        ));
    }
    parts.join(", ")
}

fn wire_name(w: &Wire) -> String {
    if w.section.starts_with("jumper:") {
        w.section.clone()
    } else {
        format!("{} span {}", w.section, w.span)
    }
}

/// One row per result and measure for the CSV.
fn csv_rows(out: &mut String, rule: &Rule, a: &Wire, b: Option<&Wire>, r: &WireResult) {
    for (kind, hit, required, count) in [
        ("vertical", r.vertical, rule.vertical, r.vertical_violations),
        ("radial", r.radial, rule.radial, r.radial_violations),
    ] {
        let Some(h) = hit else { continue };
        let _ = writeln!(
            out,
            "{},{kind},{},{},{},{},{:.3},{},{count},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{}",
            rule.name,
            a.section,
            a.span,
            b.map_or(String::new(), |w| w.section.clone()),
            b.map_or(String::new(), |w| w.span.to_string()),
            h.distance,
            required.map_or(String::new(), |v| format!("{v:.3}")),
            h.wire[0],
            h.wire[1],
            h.wire[2],
            h.target[0],
            h.target[1],
            h.target[2],
            h.class.map_or(String::new(), |c| c.to_string()),
        );
    }
}

/// Lowest vertical and radial over results.
fn lowest<'a>(results: impl Iterator<Item = &'a WireResult> + Clone) -> String {
    let min = |f: fn(&WireResult) -> Option<ols_clearance::Hit>| {
        results
            .clone()
            .filter_map(f)
            .min_by(|x, y| x.distance.total_cmp(&y.distance))
    };
    let mut parts = Vec::new();
    if let Some(h) = min(|r| r.vertical) {
        parts.push(format!(
            "lowest vertical {:.2} m at ({:.1}, {:.1})",
            h.distance, h.wire[0], h.wire[1]
        ));
    }
    if let Some(h) = min(|r| r.radial) {
        parts.push(format!(
            "lowest radial {:.2} m at ({:.1}, {:.1})",
            h.distance, h.wire[0], h.wire[1]
        ));
    }
    if parts.is_empty() {
        "nothing within reach".into()
    } else {
        parts.join("; ")
    }
}

fn marker(extras: &mut ViewerExtras, rule: &Rule, label: String, r: &WireResult) {
    let worst = [r.vertical, r.radial]
        .into_iter()
        .flatten()
        .min_by(|x, y| x.distance.total_cmp(&y.distance));
    if let Some(h) = worst {
        extras
            .markers
            .push((h.wire, format!("{}: {label}", rule.name)));
    }
}

/// Loads everything the checks need, once.
pub fn prepare(line: &Line, o: &Options) -> Result<Prepared, String> {
    let mut rules = match o.rules {
        Some(path) => RuleSet::load(path)?.rules,
        None => Vec::new(),
    };
    if let Some(v) = o.clearance {
        rules.extend(RuleSet::ground(v).rules);
    }
    if rules.is_empty() && (o.terrain.is_some() || o.lidar.is_some() || o.pointcloud.is_some()) {
        // Ground only: still report the lowest ground clearance.
        let mut r = RuleSet::ground(0.0).rules;
        r[0].vertical = None;
        rules = r;
    }
    // Points: a project's point cloud or LAS/LAZ tiles, one interface.
    let source: Option<Box<dyn ols_pointcloud::PointSource>> = match (o.pointcloud, o.lidar) {
        (Some(spec), _) => {
            let (file, name) = match spec.split_once('#') {
                Some((f, n)) => (f, Some(n)),
                None => (spec, None),
            };
            Some(Box::new(
                ols_pointcloud::CloudSource::open(Path::new(file), name)
                    .map_err(|e| e.to_string())?,
            ))
        }
        (None, Some(dir)) => Some(Box::new(ols_pointcloud::TileSource {
            tiles: ols_lidar::tiles_in(dir).map_err(|e| e.to_string())?,
            scheme: None,
        })),
        (None, None) => None,
    };
    // Class scheme: given, else the one stored with the points, else ASPRS.
    let scheme = match (
        o.lidar_classes,
        source.as_ref().and_then(|s| s.class_scheme()),
    ) {
        (Some(p), _) => ols_lidar::ClassScheme::load(p)?,
        (None, Some(text)) => ols_lidar::ClassScheme::parse(&text)?,
        (None, None) => ols_lidar::ClassScheme::asprs(),
    };
    let mut point_classes = Vec::new();
    for rule in rules
        .iter()
        .filter(|r| matches!(r.target, Target::Points { .. }))
    {
        let classes = rule.point_classes(|role| scheme.codes_with_role(role));
        if classes.is_empty() {
            return Err(format!(
                "rule \"{}\": no class of \"{}\" matches",
                rule.name, scheme.name
            ));
        }
        point_classes.push(classes);
    }
    if !point_classes.is_empty() && source.is_none() {
        return Err("point rules need --pointcloud or --lidar".into());
    }

    let origin = line.structures.first().map_or([0.0; 3], |s| s.position);
    let mut points: Vec<([i32; 3], u8)> = Vec::new();
    let started = std::time::Instant::now();
    let ground = match (o.terrain, &source) {
        (Some(path), _) => Some(Terrain::load(path).map_err(|e| e.to_string())?),
        (None, Some(source)) => {
            let ground_classes = scheme.codes_with_role(o.ground_role);
            if ground_classes.is_empty() {
                return Err(format!(
                    "no class of \"{}\" has the role \"{}\"",
                    scheme.name, o.ground_role
                ));
            }
            let (mut is_ground, mut is_needed) = ([false; 256], [false; 256]);
            for &c in &ground_classes {
                is_ground[c as usize] = true;
            }
            for &c in point_classes.iter().flatten() {
                is_needed[c as usize] = true;
            }
            let mut wanted: Vec<u8> = (0..=255u8)
                .filter(|&c| is_ground[c as usize] || is_needed[c as usize])
                .collect();
            wanted.dedup();
            // Corridor around the spans' chords (attachment to attachment):
            // the same for every weather case.
            let polylines: Vec<Vec<[f64; 2]>> = line
                .sections
                .iter()
                .map(|s| {
                    s.points
                        .iter()
                        .filter_map(|p| line.attachment(p))
                        .map(|a| [a.wire_point[0], a.wire_point[1]])
                        .collect()
                })
                .collect();
            let corridor = ols_lidar::Corridor::new(&polylines, o.corridor);
            // Exit clause: ~1 min per 10⁸ points from LAZ; 30 min is far beyond.
            let budget =
                ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(1800));
            let mut ground_points: Vec<[f64; 3]> = Vec::new();
            let mut cells: std::collections::HashMap<(i64, i64), usize> =
                std::collections::HashMap::new();
            let mm = |v: f64, k: usize| ((v - origin[k]) * 1000.0).round() as i32;
            let stats = source.visit(
                Some(&corridor),
                Some(&wanted),
                &budget,
                &mut |x, y, z, c| {
                    if is_ground[c as usize] {
                        match o.thin {
                            None => ground_points.push([x, y, z]),
                            Some(cell) => {
                                let key = ((x / cell).floor() as i64, (y / cell).floor() as i64);
                                match cells.get(&key) {
                                    Some(&k) if ground_points[k][2] <= z => {}
                                    Some(&k) => ground_points[k] = [x, y, z],
                                    None => {
                                        cells.insert(key, ground_points.len());
                                        ground_points.push([x, y, z]);
                                    }
                                }
                            }
                        }
                    }
                    if is_needed[c as usize] {
                        points.push(([mm(x, 0), mm(y, 1), mm(z, 2)], c));
                    }
                },
            )?;
            println!(
                "points: {} — {} read, {} skipped (other classes or outside the ±{} m corridor); {} decoded, {} used; classes: {}; ground {ground_classes:?}: {} points; for point rules: {} points",
                source.describe(),
                stats.read,
                stats.skipped,
                o.corridor,
                stats.points_read,
                stats.points_visited,
                scheme.name,
                ground_points.len(),
                points.len()
            );
            Some(Terrain::from_xyz(&ground_points, Some(o.max_edge)).map_err(|e| e.to_string())?)
        }
        (None, None) => None,
    };
    if let Some(g) = &ground {
        println!(
            "ground: {} points, {} triangles ({:.1?})",
            g.num_points(),
            g.num_triangles(),
            started.elapsed()
        );
    }
    let mut other_lines = Vec::new();
    for path in o.other_lines {
        let mut other = crate::project::load_line(path)?;
        if other.structures.iter().any(|s| s.structure_type.is_some()) {
            let structures = ols_structures::StructureLibrary::load_dir(o.structures)
                .map_err(|e| e.to_string())?;
            let insulators = ols_structures::InsulatorLibrary::load_dir(o.insulators)
                .map_err(|e| e.to_string())?;
            other = other
                .resolve(&structures, &insulators)
                .map_err(|e| e.to_string())?;
        }
        other_lines.push(other);
    }
    Ok(Prepared {
        rules,
        scheme,
        ground,
        points,
        origin,
        point_classes,
        other_lines,
    })
}

/// The CSV header of [`check`]'s rows.
pub const CSV_HEADER: &str = "case,rule,measure,section,span,other_section,other_span,clearance_m,required_m,violations,wire_x,wire_y,wire_z,target_x,target_y,target_z,class\n";

/// Checks one weather case: prints the report, appends CSV rows, marks
/// violations for the viewer.
#[allow(clippy::too_many_arguments)]
pub fn check(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    shapes: &[SectionShape],
    jumpers: &[JumperShape],
    p: &Prepared,
    csv: &mut String,
    extras: &mut ViewerExtras,
) -> Result<(), String> {
    if p.rules.is_empty() {
        return Ok(());
    }
    let rules = &p.rules;
    let wires = wires_of(line, shapes, jumpers);
    let reach = rules.iter().map(Rule::reach).fold(0.0, f64::max);
    let set = WireSet::new(wires.clone(), reach);
    // Point rules from the points in memory.
    let mut point_checks: Vec<PointCheck> = rules
        .iter()
        .filter(|r| matches!(r.target, Target::Points { .. }))
        .zip(&p.point_classes)
        .map(|(rule, classes)| PointCheck::new(rule, classes, &set))
        .collect();
    for (q, c) in &p.points {
        let (x, y, z) = (
            p.origin[0] + q[0] as f64 / 1000.0,
            p.origin[1] + q[1] as f64 / 1000.0,
            p.origin[2] + q[2] as f64 / 1000.0,
        );
        for check in &mut point_checks {
            check.visit(&set, x, y, z, *c);
        }
    }
    // Other lines in this weather case.
    let mut other_wires: Vec<Wire> = Vec::new();
    for other in &p.other_lines {
        let shapes: Vec<SectionShape> = ols_line::solve::solve_line(other, library, case)
            .into_iter()
            .collect::<Result<_, _>>()
            .map_err(|e| format!("{}: {e}", other.name))?;
        let jumpers: Vec<JumperShape> =
            ols_line::jumper::solve_jumpers(other, library, case, &shapes)
                .into_iter()
                .collect::<Result<_, _>>()
                .map_err(|e| format!("{}: {e}", other.name))?;
        let mut w = wires_of(other, &shapes, &jumpers);
        for wire in &mut w {
            wire.section = format!("{}:{}", other.name, wire.section);
        }
        other_wires.extend(w);
    }
    let other_set = (!other_wires.is_empty()).then(|| WireSet::new(other_wires, reach));

    let mut point_checks = point_checks.into_iter();
    let mut rows = String::new();
    for rule in rules {
        let required = |r: Option<f64>| r.map_or("-".to_owned(), |v| format!("{v:.2} m"));
        println!(
            "rule \"{}\" ({}): required vertical {}, radial {}",
            rule.name,
            match &rule.target {
                Target::Ground => "ground".to_owned(),
                Target::Points { .. } => format!(
                    "points of classes {:?}",
                    rule.point_classes(|role| p.scheme.codes_with_role(role))
                ),
                Target::SameLine { .. } => "other wires of this line".to_owned(),
                Target::OtherLines => "wires of other lines".to_owned(),
            },
            required(rule.vertical),
            required(rule.radial)
        );
        match &rule.target {
            Target::Ground | Target::Points { .. } => {
                let results: Vec<WireResult> = match &rule.target {
                    Target::Ground => match &p.ground {
                        Some(g) => ols_clearance::checks::ground_check(rule, &wires, g),
                        None => {
                            println!("  no ground: give --terrain, --lidar or --pointcloud");
                            continue;
                        }
                    },
                    _ => {
                        point_checks
                            .next()
                            .expect("one check per point rule")
                            .results
                    }
                };
                println!("  {}", lowest(results.iter()));
                let mut bad: Vec<(usize, &WireResult)> = results
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.violates(rule))
                    .collect();
                bad.sort_by(|x, y| {
                    let key = |r: &WireResult| {
                        [r.vertical, r.radial]
                            .into_iter()
                            .flatten()
                            .map(|h| h.distance)
                            .fold(f64::INFINITY, f64::min)
                    };
                    key(x.1).total_cmp(&key(y.1))
                });
                println!("  wires below the requirement: {}", bad.len());
                for (w, r) in bad.iter().take(20) {
                    println!("    {:<24} {}", wire_name(&wires[*w]), describe(r, rule));
                }
                for (w, r) in &bad {
                    marker(
                        extras,
                        rule,
                        format!("{} ({})", wire_name(&wires[*w]), case.name),
                        r,
                    );
                }
                for (w, r) in results.iter().enumerate() {
                    csv_rows(&mut rows, rule, &wires[w], None, r);
                }
            }
            Target::SameLine { .. } | Target::OtherLines => {
                let (pairs, b_wires): (Vec<PairResult>, &[Wire]) = match &rule.target {
                    Target::SameLine { include_same_phase } => (
                        ols_clearance::checks::wire_pairs(
                            rule,
                            &wires,
                            &set,
                            true,
                            *include_same_phase,
                        ),
                        &set.wires,
                    ),
                    _ => match &other_set {
                        Some(os) => (
                            ols_clearance::checks::wire_pairs(rule, &wires, os, false, false),
                            &os.wires,
                        ),
                        None => {
                            println!("  no other line: give --other-line FILE");
                            continue;
                        }
                    },
                };
                println!(
                    "  {} wire pairs within reach; {}",
                    pairs.len(),
                    lowest(pairs.iter().map(|p| &p.result))
                );
                let bad: Vec<&PairResult> =
                    pairs.iter().filter(|p| p.result.violates(rule)).collect();
                println!("  pairs below the requirement: {}", bad.len());
                for pr in bad.iter().take(20) {
                    println!(
                        "    {:<24} – {:<24} {}",
                        wire_name(&wires[pr.a]),
                        wire_name(&b_wires[pr.b]),
                        describe(&pr.result, rule)
                    );
                }
                for pr in &bad {
                    marker(
                        extras,
                        rule,
                        format!(
                            "{} – {} ({})",
                            wire_name(&wires[pr.a]),
                            wire_name(&b_wires[pr.b]),
                            case.name
                        ),
                        &pr.result,
                    );
                }
                for pr in &pairs {
                    csv_rows(
                        &mut rows,
                        rule,
                        &wires[pr.a],
                        Some(&b_wires[pr.b]),
                        &pr.result,
                    );
                }
            }
        }
    }
    // Prefix the rows with the case name.
    let name = case.name.replace(',', ";");
    for row in rows.lines() {
        let _ = writeln!(csv, "{name},{row}");
    }
    Ok(())
}
