// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The bolt tools' minimum distances of every member end, bit for bit
//! (2026-11-18, third round: how the move of the bolts' force basis into
//! the design-code plugins was proved): for each member end of each
//! structure file (its joint details taken, the bolt libraries beside it),
//! in the member's leg and in two plates, without an analysis and with an
//! invented axial force — `file|member|end|part|force|end bits|spacing
//! bits|edge bits|force basis|clauses`. Two runs (before and after a
//! change) compared with `cmp`; a summary of the differences by
//! `OHL_COMPARE_WITH=<the other run's file>`.
//!
//! `cargo run -p ohl-member-check --example compare_bolt_rules -- METHOD OUT SECTIONS FILE...`
//! METHOD: a plugin's method (`asce10-97`, `sia263-2013`,
//! `plugin:<plugin>/<method>`), found as a structure's run finds it;
//! SECTIONS: a section library (file or folder).

use std::fmt::Write as _;
use std::path::Path;

use ohl_structures_fe::Model;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let method = &args[0];
    let out = &args[1];
    let sections = ohl_sections::SectionLibrary::load(Path::new(&args[2])).expect("sections");
    let mut text = String::new();
    let (mut files, mut ends) = (0usize, 0usize);
    for f in &args[3..] {
        let path = Path::new(f);
        let src = std::fs::read_to_string(path).expect("read");
        let bolts = ohl_structures::bolts::BoltLibrary::beside(path.parent().unwrap()).0;
        let mut model =
            match Model::from_structure_type_with_joint_details(&src, &sections, &bolts, None) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("{f}: {e}");
                    continue;
                }
            };
        model.capacity_plugin = Some(
            ohl_structures_fe::model::PluginMethod::parse(method)
                .unwrap()
                .unwrap(),
        );
        model = ohl_member_check::find(model, path.parent()).expect("the plugin");
        files += 1;
        let name = path.file_name().unwrap().to_string_lossy();
        for idx in 0..model.members.len() {
            for end in 0..2 {
                for (pname, part) in [
                    ("leg", None),
                    ("plate10", Some((0.010, Some(360e6)))),
                    ("plate8", Some((0.008, None))),
                ] {
                    for (fname, force) in [("none", None), ("50kN", Some(-50e3))] {
                        let r = ohl_member_check::joints::bolt_rules(&model, idx, end, force, part);
                        let row = match r {
                            None => "none".to_owned(),
                            Some(Err(e)) => format!("err|{e}"),
                            Some(Ok(r)) => format!(
                                "{:016x}|{:016x}|{:016x}|{}|{}",
                                r.end.to_bits(),
                                r.spacing.to_bits(),
                                r.edge.to_bits(),
                                match r.force {
                                    ohl_structures_fe::bolt_rules::Force::Analysis(p) =>
                                        format!("analysis {:016x}", p.to_bits()),
                                    ohl_structures_fe::bolt_rules::Force::Capacity(p) =>
                                        format!("capacity {:016x}", p.to_bits()),
                                    ohl_structures_fe::bolt_rules::Force::Unknown =>
                                        "unknown".to_owned(),
                                },
                                r.clauses.join(";")
                            ),
                        };
                        ends += 1;
                        let _ = writeln!(
                            text,
                            "{name}|{}|{end}|{pname}|{fname}|{row}",
                            model.members[idx].id
                        );
                    }
                }
            }
        }
    }
    std::fs::write(out, &text).expect("write");
    println!("{files} files, {ends} member-end rows");
    // The differences against another run: how many rows, which way and
    // the largest change of each distance (mm).
    if let Ok(other) = std::env::var("OHL_COMPARE_WITH") {
        let before = std::fs::read_to_string(other).expect("the other run");
        let (mut same, mut differ, mut larger, mut smaller) = (0usize, 0usize, 0usize, 0usize);
        let mut largest = [0.0f64; 3];
        let mut force_ratio: (f64, f64) = (f64::INFINITY, 0.0);
        for (a, b) in before.lines().zip(text.lines()) {
            if a == b {
                same += 1;
                continue;
            }
            differ += 1;
            let fa: Vec<&str> = a.split('|').collect();
            let fb: Vec<&str> = b.split('|').collect();
            if fa.len() < 9 || fb.len() < 9 || fa[5] == "err" || fb[5] == "err" {
                continue;
            }
            let v = |s: &str| f64::from_bits(u64::from_str_radix(s, 16).unwrap());
            let mut up = false;
            let mut down = false;
            for k in 0..3 {
                let d = v(fb[5 + k]) - v(fa[5 + k]);
                if d.abs() > largest[k].abs() {
                    largest[k] = d;
                }
                up |= d > 0.0;
                down |= d < 0.0;
            }
            larger += usize::from(up);
            smaller += usize::from(down);
            if let (Some(x), Some(y)) = (
                fa[8].strip_prefix("capacity "),
                fb[8].strip_prefix("capacity "),
            ) {
                let r = v(y) / v(x);
                force_ratio = (force_ratio.0.min(r), force_ratio.1.max(r));
            }
        }
        println!(
            "against the other run: {same} rows identical, {differ} differ; distances larger in \
             {larger}, smaller in {smaller}; largest change end {:+.1} mm, spacing {:+.1} mm, \
             edge {:+.1} mm; force basis (capacity) ratio {:.4} to {:.4}",
            largest[0] * 1000.0,
            largest[1] * 1000.0,
            largest[2] * 1000.0,
            force_ratio.0,
            force_ratio.1
        );
    }
}
