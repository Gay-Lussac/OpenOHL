// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The comparison of a design code in the core and as a plugin, bit for
//! bit (2026-11-18: how the moves of SIA 263:2013 and then of ASCE 10-97
//! were proved; since then the core's only own method is `simple`):
//! the members' usage of structure files in four load cases (loads at every
//! attachment point) and the variants as given, with the redundant-force
//! check, with single angles' moments, with tension-only members carrying
//! no compression and with the joint details taken (a file with
//! connections) — every member of every case as `file|variant|case|member|
//! usage bits|governs`. Two runs (before and after a move) compared with
//! `cmp`. It prints the slowest plugin call (the measure of a bundled
//! plugin's time limit); `OHL_COMPARE_CASES=N` checks N load cases instead
//! of four (the four again, their loads scaled) for timing it.
//!
//! `cargo run -p ohl-member-check --example compare_codes -- MODE OUT SECTIONS FILE...`
//! MODE: `core:<method>` (the core's method, `core:simple`; it was
//! `core:asce10-97` before the move) or a plugin's method (`asce10-97`,
//! `sia263-2013`, `plugin:<plugin>/<method>`: found as a
//! structure's run finds it); SECTIONS: a section library (file or
//! folder).

use std::fmt::Write as _;
use std::path::Path;

use ohl_structures_fe::{Loads, Model};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = &args[0];
    let out = &args[1];
    let sections = ohl_sections::SectionLibrary::load(Path::new(&args[2])).expect("sections");
    let mut text = String::new();
    let (mut members, mut cases, mut files) = (0usize, 0usize, 0usize);
    // The slowest plugin call (seconds, which): the measure of a bundled
    // plugin's time limit (2026-11-18).
    let mut slowest = (0.0f64, String::new());
    for f in &args[3..] {
        let path = Path::new(f);
        let src = std::fs::read_to_string(path).expect("read");
        for variant in ["base", "redundant", "angles", "no-compression", "joints"] {
            let mut model = match variant {
                "joints" => {
                    if !src.contains("[[connection]]") {
                        continue;
                    }
                    let bolts =
                        ohl_structures::bolts::BoltLibrary::beside(path.parent().unwrap()).0;
                    match Model::from_structure_type_with_joint_details(
                        &src, &sections, &bolts, None,
                    ) {
                        Ok(m) => m,
                        Err(_) => continue,
                    }
                }
                _ => match Model::load_with(path, &sections) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("{f}: {e}");
                        break;
                    }
                },
            };
            match variant {
                "redundant" => model.analysis.redundant_force = Some(0.025),
                "angles" => model.analysis.single_angle_moments = true,
                "no-compression" => {
                    if !model
                        .members
                        .iter()
                        .any(|m| m.kind == ohl_structures_fe::MemberKind::Tension)
                    {
                        continue;
                    }
                    model.analysis.tension_compression = ohl_structures_fe::TensionCompression::None
                }
                _ => {}
            }
            if let Some(m) = mode.strip_prefix("core:") {
                model.analysis.capacity = ohl_structures_fe::CapacityMethod::parse(m).unwrap();
                model.capacity_plugin = None;
            } else {
                model.capacity_plugin = Some(
                    ohl_structures_fe::model::PluginMethod::parse(mode)
                        .unwrap()
                        .unwrap(),
                );
                model = match ohl_member_check::find(model, path.parent()) {
                    Ok(m) => m,
                    Err(e) => panic!("{e}"),
                };
            }
            let mut runs: Vec<(String, [f64; 3], f64)> = vec![
                ("A".into(), [6e3, 0.0, 12e3], 1.0),
                ("B".into(), [0.0, 8e3, 10e3], 1.0),
                ("C".into(), [-15e3, -3e3, 20e3], 0.9),
                ("D".into(), [40e3, 10e3, 60e3], 1.0),
            ];
            // More cases for timing a plugin call (OHL_COMPARE_CASES, e.g.
            // 100: the four again with their loads scaled; 2026-11-18).
            let wanted: usize = std::env::var("OHL_COMPARE_CASES")
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(4);
            for k in 4..wanted {
                let (name, [t, l, v], s) = runs[k % 4].clone();
                let f = 1.0 + 0.05 * (k / 4) as f64;
                runs.push((format!("{name}{k}"), [t * f, l * f, v * f], s));
            }
            let mut solved = Vec::new();
            for (name, [t, l, v], strength) in &runs {
                let mut nodal = Vec::new();
                let labels: Vec<String> =
                    model.joint_points.iter().map(|p| p.label.clone()).collect();
                if labels.is_empty() {
                    nodal.extend(model.loads.iter().cloned());
                }
                for label in &labels {
                    if let Some(at) = model.point_load(label, [*t, *l, -*v]) {
                        nodal.extend(at.loads);
                    }
                }
                let loads = Loads {
                    nodal,
                    member: Vec::new(),
                    self_weight: 1.0,
                    prescribed: Vec::new(),
                };
                match ohl_structures_fe::analyse(
                    &model,
                    &loads,
                    *strength,
                    &ohl_structures_fe::default_budget(),
                ) {
                    Ok((s, u)) => solved.push((name.as_str(), *strength, s, u)),
                    Err(e) => {
                        let _ = writeln!(text, "{f}|{variant}|{name}|refused|{e:?}");
                    }
                }
            }
            if model.capacity_plugin.is_some() {
                let list: Vec<ohl_member_check::Solved> = solved
                    .iter()
                    .map(|(n, st, s, _)| ohl_member_check::Solved {
                        name: n,
                        strength: *st,
                        self_weight: 1.0,
                        solution: s,
                        moved: None,
                    })
                    .collect();
                let t0 = std::time::Instant::now();
                let c = ohl_member_check::check(&model, &list)
                    .expect("a plugin")
                    .expect("checked");
                let took = t0.elapsed().as_secs_f64();
                if took > slowest.0 {
                    slowest = (
                        took,
                        format!(
                            "{} ({variant}, {} members, {} cases)",
                            path.file_name().unwrap().to_string_lossy(),
                            model.members.len(),
                            list.len()
                        ),
                    );
                }
                for (run, u) in solved.iter_mut().zip(c.usage) {
                    run.3 = u;
                }
            }
            files += 1;
            for (name, _, _, usage) in &solved {
                cases += 1;
                for u in usage {
                    members += 1;
                    let _ = writeln!(
                        text,
                        "{}|{variant}|{name}|{}|{}|{}",
                        path.file_name().unwrap().to_string_lossy(),
                        model.members[u.member].id,
                        u.usage
                            .map_or("none".to_owned(), |x| format!("{:016x}", x.to_bits())),
                        u.governs
                    );
                }
            }
        }
    }
    std::fs::write(out, text).expect("write");
    if slowest.0 > 0.0 {
        println!("slowest plugin call: {:.2} s, {}", slowest.0, slowest.1);
    }
    println!("{files} file variants, {cases} cases, {members} member results");
}
