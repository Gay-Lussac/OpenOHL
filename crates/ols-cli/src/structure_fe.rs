// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols structures fe` and `ols structures pole`: one structure model or
//! pole checked with the loads of a structure-loads CSV (`ols line
//! --loads-csv`) or the model's own loads.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ols_structures::LibraryItem;
use ols_structures_fe::{Loads, Model, NodalLoad, Pole, PoleLoad};

/// Arguments of `ols structures fe`.
#[derive(clap::Args)]
pub struct FeArgs {
    /// Structure model (.olsfe).
    model: PathBuf,
    /// Structure loads (CSV of `ols line --loads-csv`): the factored loads
    /// at the attachments, applied at the model's attachment nodes
    /// (default: the model's own [[load]]s).
    #[arg(long, requires = "structure")]
    loads: Option<PathBuf>,
    /// The structure whose loads to take from the CSV.
    #[arg(long)]
    structure: Option<String>,
    /// Load case to run, by name (repeatable; default: every case).
    #[arg(long)]
    case: Vec<String>,
    /// Strength factor on the capacities.
    #[arg(long, default_value_t = 1.0)]
    strength: f64,
    /// Factor on the members' self-weight (0: none).
    #[arg(long, default_value_t = 1.0)]
    weight_factor: f64,
    /// First order (no P-Delta), whatever the model says.
    #[arg(long)]
    linear: bool,
    /// Members printed per case, worst first.
    #[arg(long, default_value_t = 10)]
    top: usize,
    /// Write every member of every case as CSV.
    #[arg(long)]
    csv: Option<PathBuf>,
}

/// Arguments of `ols structures pole`.
#[derive(clap::Args)]
pub struct PoleArgs {
    /// Pole (.olsp).
    pole: PathBuf,
    /// Structure type (.olss): where the attachments are.
    #[arg(long = "type")]
    structure_type: PathBuf,
    /// Structure loads (CSV of `ols line --loads-csv`).
    #[arg(long)]
    loads: PathBuf,
    /// The structure whose loads to take.
    #[arg(long)]
    structure: String,
    /// Load case to run, by name (repeatable; default: every case).
    #[arg(long)]
    case: Vec<String>,
    /// Factored wind pressure on the pole, Pa.
    #[arg(long, default_value_t = 0.0)]
    wind_pressure: f64,
    /// Side the wind comes from, looking ahead: left or right.
    #[arg(long, default_value = "right")]
    wind_from: String,
    /// Strength factor on the capacity.
    #[arg(long, default_value_t = 1.0)]
    strength: f64,
    /// No P-Delta amplification.
    #[arg(long)]
    no_p_delta: bool,
}

/// Splits one CSV record (fields may be quoted).
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                chars.next();
                fields.last_mut().expect("one field").push('"');
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(String::new()),
            (c, _) => fields.last_mut().expect("one field").push(c),
        }
    }
    fields
}

/// Factored loads of one structure by case: (set, phase, [transverse,
/// longitudinal, vertical down]) summed over the supports, in file order.
type CaseLoads = Vec<(String, Vec<(u32, u32, [f64; 3])>)>;

fn read_loads(path: &Path, structure: &str, cases: &[String]) -> Result<CaseLoads, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines = text.lines();
    let header = csv_fields(lines.next().unwrap_or_default());
    let col = |name: &str| {
        header
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| format!("{}: no column {name}", path.display()))
    };
    let (c_case, c_structure, c_set, c_phase) =
        (col("case")?, col("structure")?, col("set")?, col("phase")?);
    let forces = [
        col("factored_transverse")?,
        col("factored_longitudinal")?,
        col("factored_vertical")?,
    ];
    let mut out: CaseLoads = Vec::new();
    let mut known = std::collections::BTreeSet::new();
    for (k, line) in lines.enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let f = csv_fields(line);
        let get = |c: usize| f.get(c).map(String::as_str).unwrap_or_default();
        let bad = |what: &str| format!("{} line {}: {what}", path.display(), k + 2);
        known.insert(get(c_structure).to_owned());
        if get(c_structure) != structure {
            continue;
        }
        let case = get(c_case).to_owned();
        if !cases.is_empty() && !cases.iter().any(|c| ols_weather::name_matches(c, &case)) {
            continue;
        }
        let set: u32 = get(c_set).parse().map_err(|_| bad("set"))?;
        let phase: u32 = get(c_phase).parse().map_err(|_| bad("phase"))?;
        let mut v = [0.0; 3];
        for (x, c) in v.iter_mut().zip(forces) {
            *x = get(c).parse().map_err(|_| bad("force"))?;
        }
        let pos = match out.iter().position(|(c, _)| *c == case) {
            Some(p) => p,
            None => {
                out.push((case, Vec::new()));
                out.len() - 1
            }
        };
        let list = &mut out[pos].1;
        match list.iter_mut().find(|(s, p, _)| *s == set && *p == phase) {
            Some((_, _, sum)) => {
                for (a, b) in sum.iter_mut().zip(v) {
                    *a += b;
                }
            }
            None => list.push((set, phase, v)),
        }
    }
    if out.is_empty() {
        return Err(format!(
            "{}: no loads of structure {structure}{} (structures: {})",
            path.display(),
            if cases.is_empty() {
                String::new()
            } else {
                format!(" in cases {}", cases.join(", "))
            },
            known.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(out)
}

/// `ols structures fe`.
pub fn fe(args: &FeArgs) -> Result<(), String> {
    let mut model = Model::load(&args.model)?;
    if args.linear {
        model.analysis.nonlinear = false;
    }
    let weight = if model.analysis.self_weight {
        args.weight_factor
    } else {
        0.0
    };
    println!(
        "model {}: {} nodes, {} members, {:.1} kN of members; {}",
        if model.name.is_empty() {
            args.model.display().to_string()
        } else {
            model.name.clone()
        },
        model.nodes.len(),
        model.members.len(),
        model.weight() / 1000.0,
        if model.analysis.nonlinear {
            "P-Delta"
        } else {
            "first order"
        }
    );
    let runs: Vec<(String, Vec<NodalLoad>)> = match (&args.loads, &args.structure) {
        (Some(path), Some(structure)) => read_loads(path, structure, &args.case)?
            .into_iter()
            .map(|(case, at)| {
                let mut nodal = Vec::new();
                for (set, phase, [t, l, v]) in at {
                    let node = model.attachment_node(set, phase).ok_or_else(|| {
                        format!("{case}: the model has no node for attachment {set}:{phase}")
                    })?;
                    nodal.push(NodalLoad {
                        node,
                        force: [t, l, -v],
                        moment: [0.0; 3],
                    });
                }
                Ok((case, nodal))
            })
            .collect::<Result<_, String>>()?,
        _ => {
            if model.loads.is_empty() && weight == 0.0 {
                return Err("the model has no [[load]]; give --loads and --structure".into());
            }
            vec![("model".into(), model.loads.clone())]
        }
    };
    let mut csv = String::from("case,member,kind,section,axial_n,usage,governs\n");
    for (case, nodal) in runs {
        let started = std::time::Instant::now();
        let loads = Loads {
            nodal,
            self_weight: weight,
        };
        let (s, usage) = ols_structures_fe::analyse(
            &model,
            &loads,
            args.strength,
            &ols_structures_fe::default_budget(),
        )
        .map_err(|e| format!("{case}: {e}"))?;
        let (worst_node, worst_d) = s
            .displacements
            .iter()
            .enumerate()
            .map(|(k, d)| (k, (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((0, 0.0));
        let reaction: [f64; 3] = s.reactions.iter().fold([0.0; 3], |a, r| {
            [a[0] + r.force[0], a[1] + r.force[1], a[2] + r.force[2]]
        });
        println!(
            "{case}: {} iterations in {:.1?}, largest displacement {:.1} mm at {}, \
             reactions {:.1} / {:.1} / {:.1} kN",
            s.iterations,
            started.elapsed(),
            worst_d * 1000.0,
            model.nodes[worst_node].id,
            reaction[0] / 1000.0,
            reaction[1] / 1000.0,
            reaction[2] / 1000.0
        );
        let mut sorted: Vec<&ols_structures_fe::MemberUsage> =
            usage.iter().filter(|u| u.usage.is_some()).collect();
        sorted.sort_by(|a, b| b.usage.unwrap_or(0.0).total_cmp(&a.usage.unwrap_or(0.0)));
        if sorted.is_empty() {
            println!("  no member has a capacity");
        }
        for u in sorted.iter().take(args.top) {
            let m = &model.members[u.member];
            println!(
                "  {:<12} {:>7.1} %  {:<6} {}",
                m.id,
                u.usage.unwrap_or(0.0) * 100.0,
                m.kind.as_str(),
                u.governs
            );
        }
        let over = sorted.iter().filter(|u| u.usage > Some(1.0)).count();
        if over > 0 {
            println!("  {over} members over 100 %");
        }
        for u in &usage {
            let m = &model.members[u.member];
            let _ = writeln!(
                csv,
                "{},{},{},{},{:.1},{},{}",
                ols_core::text::csv_field(&case),
                ols_core::text::csv_field(&m.id),
                m.kind.as_str(),
                ols_core::text::csv_field(&model.sections[m.section].id),
                s.members[u.member].axial,
                u.usage
                    .map_or(String::new(), |x| format!("{:.1}", x * 100.0)),
                ols_core::text::csv_field(&u.governs)
            );
        }
    }
    if let Some(out) = &args.csv {
        std::fs::write(out, csv).map_err(|e| format!("{}: {e}", out.display()))?;
        println!("written to {}", out.display());
    }
    Ok(())
}

/// `ols structures pole`.
pub fn pole(args: &PoleArgs) -> Result<(), String> {
    let pole = Pole::load(&args.pole)?;
    let text = std::fs::read_to_string(&args.structure_type)
        .map_err(|e| format!("{}: {e}", args.structure_type.display()))?;
    let t = ols_structures::StructureType::parse(&args.structure_type, &text)
        .map_err(|e| e.to_string())?;
    let side = match args.wind_from.as_str() {
        "right" => -1.0,
        "left" => 1.0,
        other => return Err(format!("--wind-from {other}: write left or right")),
    };
    let wind = [side * args.wind_pressure, 0.0];
    println!(
        "pole {}: {:.1} m above ground, diameter {:.0} → {:.0} mm",
        pole.id,
        pole.height(),
        pole.ground_diameter * 1000.0,
        pole.top_diameter * 1000.0
    );
    let mut by_case: Vec<(String, Vec<PoleLoad>)> = Vec::new();
    for (case, at) in read_loads(&args.loads, &args.structure, &args.case)? {
        let mut loads = Vec::new();
        for (set, phase, [tr, lo, ve]) in at {
            let a = t
                .attachments
                .iter()
                .find(|a| a.set == set && a.phase == phase)
                .ok_or_else(|| format!("type {} has no attachment {set}:{phase}", t.id))?;
            loads.push(PoleLoad {
                point: a.point.map(|x| x.si()),
                force: [tr, lo, -ve],
            });
        }
        by_case.push((case, loads));
    }
    for (case, loads) in &by_case {
        let c = ols_structures_fe::check_pole(
            &pole,
            loads,
            wind,
            args.strength,
            !args.no_p_delta,
            &ols_structures_fe::default_budget(),
        )?;
        println!(
            "{case}: {:.1} %  {}; ground line {:.1} kN m (wind on the pole {:.1})",
            c.usage * 100.0,
            c.governs,
            c.ground_moment / 1000.0,
            c.wind_moment / 1000.0
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_csv_is_read_per_case_and_attachment() {
        let dir = std::env::temp_dir().join(format!("ols-fe-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("loads.csv");
        std::fs::write(
            &path,
            "case,structure,set,phase,label,support,x,y,z,transverse,longitudinal,vertical,\
             factored_transverse,factored_longitudinal,factored_vertical\n\
             wind,T2,1,1,\"a, b\",0,0,0,0,1,2,3,10,20,30\n\
             wind,T2,1,1,,1,0,0,0,1,2,3,1,2,3\n\
             wind,T3,1,1,,0,0,0,0,1,2,3,5,5,5\n\
             ice,T2,1,2,,0,0,0,0,1,2,3,4,5,6\n",
        )
        .unwrap();
        let r = read_loads(&path, "T2", &[]).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].0, "wind");
        assert_eq!(r[0].1, vec![(1, 1, [11.0, 22.0, 33.0])]);
        assert_eq!(r[1].1, vec![(1, 2, [4.0, 5.0, 6.0])]);
        assert_eq!(read_loads(&path, "T2", &["ice".into()]).unwrap().len(), 1);
        let e = read_loads(&path, "T9", &[]).unwrap_err();
        assert!(e.contains("T2, T3"), "{e}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
