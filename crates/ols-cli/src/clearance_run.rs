// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Clearance checks of `ols line`: ground from a survey or LiDAR, rules
//! from a file (or `--clearance` as a vertical ground rule), point rules
//! fed from the same LiDAR pass, other lines solved in the same weather.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ols_clearance::{
    PairResult, PointCheck, Rule, RuleSet, Target, Wire, WireResult, WireSet, wires_of,
};
use ols_line::Line;
use ols_line::export::ViewerExtras;
use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;
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
    pub clearance_csv: Option<&'a Path>,
    pub other_lines: &'a [PathBuf],
    pub structures: &'a Path,
    pub insulators: &'a Path,
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

/// Runs the clearance checks and prints the report; returns the ground
/// surface used, if any (for drawings).
pub fn run(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    shapes: &[SectionShape],
    jumpers: &[JumperShape],
    o: &Options,
    extras: &mut ViewerExtras,
) -> Result<Option<ols_terrain::Terrain>, String> {
    let mut rules = match o.rules {
        Some(path) => RuleSet::load(path)?.rules,
        None => Vec::new(),
    };
    if let Some(v) = o.clearance {
        rules.extend(RuleSet::ground(v).rules);
    }
    if rules.is_empty() {
        if o.clearance_csv.is_some() {
            return Err("--clearance-csv needs --rules or --clearance".into());
        }
        // Ground only: still report the lowest ground clearance.
        if o.terrain.is_none() && o.lidar.is_none() {
            return Ok(None);
        }
        rules = RuleSet::ground(0.0).rules;
        rules[0].vertical = None;
        rules[0].radial = None;
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
    let wires = wires_of(line, shapes, jumpers);
    let reach = rules.iter().map(Rule::reach).fold(0.0, f64::max);
    let set = WireSet::new(wires.clone(), reach);

    // Point rules: their classes from the scheme.
    let mut point_checks: Vec<PointCheck> = Vec::new();
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
        point_checks.push(PointCheck::new(rule, &classes, &set));
    }
    if !point_checks.is_empty() && source.is_none() {
        return Err("point rules need --pointcloud or --lidar".into());
    }

    // Ground surface, and one pass over the points for ground and point
    // rules (only the classes needed).
    let started = std::time::Instant::now();
    let ground = match (o.terrain, &source) {
        (Some(path), _) => Some(ols_terrain::Terrain::load(path).map_err(|e| e.to_string())?),
        (None, Some(source)) => {
            let ground_classes = scheme.codes_with_role(o.ground_role);
            if ground_classes.is_empty() {
                return Err(format!(
                    "no class of \"{}\" has the role \"{}\"",
                    scheme.name, o.ground_role
                ));
            }
            let mut is_ground = [false; 256];
            for &c in &ground_classes {
                is_ground[c as usize] = true;
            }
            let polylines: Vec<Vec<[f64; 2]>> = wires
                .iter()
                .map(|w| w.points.iter().map(|p| [p[0], p[1]]).collect())
                .collect();
            let corridor = ols_lidar::Corridor::new(&polylines, o.corridor);
            let mut needed: Vec<u8> = ground_classes.clone();
            for rule in rules
                .iter()
                .filter(|r| matches!(r.target, Target::Points { .. }))
            {
                needed.extend(rule.point_classes(|role| scheme.codes_with_role(role)));
            }
            needed.sort_unstable();
            needed.dedup();
            // Exit clause: ~1 min per 10⁸ points from LAZ; 30 min is far beyond.
            let budget =
                ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(1800));
            let mut ground_points: Vec<[f64; 3]> = Vec::new();
            let mut cells: std::collections::HashMap<(i64, i64), usize> =
                std::collections::HashMap::new();
            let stats = source.visit(
                Some(&corridor),
                Some(&needed),
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
                    for check in &mut point_checks {
                        check.visit(&set, x, y, z, c);
                    }
                },
            )?;
            println!(
                "points: {} — {} read, {} skipped (other classes or outside the ±{} m corridor); {} points decoded, {} used; classes: {}; ground classes {ground_classes:?}: {} points ({:.1?})",
                source.describe(),
                stats.read,
                stats.skipped,
                o.corridor,
                stats.points_read,
                stats.points_visited,
                scheme.name,
                ground_points.len(),
                started.elapsed()
            );
            Some(
                ols_terrain::Terrain::from_xyz(&ground_points, Some(o.max_edge))
                    .map_err(|e| e.to_string())?,
            )
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

    // Other lines, solved in the same weather.
    let mut other_wires: Vec<Wire> = Vec::new();
    for path in o.other_lines {
        let mut other = Line::load(path).map_err(|e| e.to_string())?;
        if other.structures.iter().any(|s| s.structure_type.is_some()) {
            let structures = ols_structures::StructureLibrary::load_dir(o.structures)
                .map_err(|e| e.to_string())?;
            let insulators = ols_structures::InsulatorLibrary::load_dir(o.insulators)
                .map_err(|e| e.to_string())?;
            other = other
                .resolve(&structures, &insulators)
                .map_err(|e| e.to_string())?;
        }
        let shapes: Vec<SectionShape> = ols_line::solve::solve_line(&other, library, case)
            .into_iter()
            .collect::<Result<_, _>>()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let jumpers: Vec<JumperShape> =
            ols_line::jumper::solve_jumpers(&other, library, case, &shapes)
                .into_iter()
                .collect::<Result<_, _>>()
                .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut w = wires_of(&other, &shapes, &jumpers);
        for wire in &mut w {
            wire.section = format!("{}:{}", other.name, wire.section);
        }
        println!("other line {}: {} wires", other.name, w.len());
        other_wires.extend(w);
    }
    let other_set = (!other_wires.is_empty()).then(|| WireSet::new(other_wires, reach));

    let mut csv = String::from(
        "rule,measure,section,span,other_section,other_span,clearance_m,required_m,violations,wire_x,wire_y,wire_z,target_x,target_y,target_z,class\n",
    );
    let mut point_checks = point_checks.into_iter();
    for rule in &rules {
        let required = |r: Option<f64>| r.map_or("-".to_owned(), |v| format!("{v:.2} m"));
        println!(
            "rule \"{}\" ({}): required vertical {}, radial {}",
            rule.name,
            match &rule.target {
                Target::Ground => "ground".to_owned(),
                Target::Points { .. } => format!(
                    "points of classes {:?}",
                    rule.point_classes(|role| scheme.codes_with_role(role))
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
                    Target::Ground => match &ground {
                        Some(g) => ols_clearance::checks::ground_check(rule, &wires, g),
                        None => {
                            println!("  no ground: give --terrain or --lidar");
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
                    marker(extras, rule, wire_name(&wires[*w]), r);
                }
                for (w, r) in results.iter().enumerate() {
                    csv_rows(&mut csv, rule, &wires[w], None, r);
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
                for p in bad.iter().take(20) {
                    println!(
                        "    {:<24} – {:<24} {}",
                        wire_name(&wires[p.a]),
                        wire_name(&b_wires[p.b]),
                        describe(&p.result, rule)
                    );
                }
                for p in &bad {
                    marker(
                        extras,
                        rule,
                        format!("{} – {}", wire_name(&wires[p.a]), wire_name(&b_wires[p.b])),
                        &p.result,
                    );
                }
                for p in &pairs {
                    csv_rows(&mut csv, rule, &wires[p.a], Some(&b_wires[p.b]), &p.result);
                }
            }
        }
    }
    if let Some(path) = o.clearance_csv {
        std::fs::write(path, csv).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if let Some(g) = ground
        .as_ref()
        .filter(|g| g.num_triangles() <= ols_line::export::MAX_VIEWER_TRIANGLES)
    {
        extras.ground = Some(g.mesh());
    }
    Ok(ground)
}
