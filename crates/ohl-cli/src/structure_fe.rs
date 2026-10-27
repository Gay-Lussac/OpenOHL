// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl structures fe` and `ohl structures pole`: one structure model or
//! pole checked with the loads of a structure-loads CSV (`ohl line
//! --loads-csv`) or the model's own loads.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ohl_structures::LibraryItem;
use ohl_structures_fe::{Loads, Model, NodalLoad, Pole, PoleLoad};

/// Arguments of `ohl structures fe`.
#[derive(clap::Args)]
pub struct FeArgs {
    /// Structure model: a structure type holding its members (.ohls) or a
    /// model file (.ohlfe).
    model: PathBuf,
    /// Structure loads (CSV of `ohl line --loads-csv`): the factored loads
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
    /// Cross-section library (.ohlsec file or folder): a member's cross-section is
    /// looked up there first, then in the model's [[section]] tables.
    #[arg(long)]
    sections: Option<PathBuf>,
    /// Member wind: factored wind pressure, Pa, on the members whose
    /// cross-section comes from the cross-section library (their profiles' widths,
    /// the model's drag factor), added to every case.
    #[arg(long, default_value_t = 0.0)]
    wind: f64,
    /// Direction the member wind blows to, degrees from the structure's x
    /// axis (right of the line) towards y (ahead).
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    wind_dir: f64,
    /// The structure standing on its extension (by name): the
    /// extension's joints and members added, its leg extensions lowering
    /// their supports (a structure file only).
    #[arg(long)]
    extension: Option<String>,
}

/// Arguments of `ohl structures pole`.
#[derive(clap::Args)]
pub struct PoleArgs {
    /// Pole (.ohlp); none: the structure type's own pole (its `[pole]`
    /// table, or the `.ohlp` it names).
    pole: Option<PathBuf>,
    /// Structure type (.ohls): where the attachments are.
    #[arg(long = "type")]
    structure_type: PathBuf,
    /// Cross-section library (.ohlsec file or folder): a `[pole]`'s cross-section.
    #[arg(long)]
    sections: Option<PathBuf>,
    /// Structure loads (CSV of `ohl line --loads-csv`).
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

/// Factored loads of one structure by case: (point label, [transverse,
/// longitudinal, vertical down]), in file order (the wind on the structure
/// body, at no point, left out).
type CaseLoads = Vec<(String, Vec<(String, [f64; 3])>)>;

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
    let (c_case, c_structure, c_point) = (col("case")?, col("structure")?, col("point")?);
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
        if !cases.is_empty() && !cases.iter().any(|c| ohl_weather::name_matches(c, &case)) {
            continue;
        }
        let point = get(c_point).to_owned();
        if point.is_empty() {
            continue;
        }
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
        match list.iter_mut().find(|(p, _)| *p == point) {
            Some((_, sum)) => {
                for (a, b) in sum.iter_mut().zip(v) {
                    *a += b;
                }
            }
            None => list.push((point, v)),
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

/// `ohl structures fe`.
pub fn fe(args: &FeArgs) -> Result<(), String> {
    let sections = match &args.sections {
        Some(p) => ohl_sections::SectionLibrary::load(p)?,
        None => ohl_sections::SectionLibrary::default(),
    };
    let mut model = Model::load_extended(&args.model, &sections, args.extension.as_deref())?;
    if args.linear {
        model.analysis.nonlinear = false;
    }
    let weight = if model.analysis.self_weight {
        args.weight_factor
    } else {
        0.0
    };
    println!(
        "model {}: {} nodes, {} members, {:.1} kN of members{}; {}",
        if model.name.is_empty() {
            args.model.display().to_string()
        } else {
            model.name.clone()
        },
        model.nodes.len(),
        model.members.len(),
        model.steel_weight() / 1000.0,
        if model.weight() == model.steel_weight() {
            String::new()
        } else {
            format!(
                " ({:.1} kN with the dead-load factors)",
                model.weight() / 1000.0
            )
        },
        if model.analysis.nonlinear {
            "P-Delta"
        } else {
            "first order"
        }
    );
    let a = args.wind_dir.to_radians();
    // Drag and shielding factors from the cross-section library's [wind] when
    // given; else the model's generic drag and no shielding (said).
    let member_wind =
        model.member_wind_with(args.wind, [a.cos(), a.sin(), 0.0], sections.wind.as_ref());
    if args.wind != 0.0 {
        if !model.has_profiles() {
            return Err(
                "--wind: no member's cross-section comes from a cross-section library (give --sections)".into(),
            );
        }
        let total = member_wind.total();
        println!(
            "member wind {:.0} Pa to {:.0}°: {:.1} / {:.1} / {:.1} kN",
            args.wind,
            args.wind_dir,
            total[0] / 1000.0,
            total[1] / 1000.0,
            total[2] / 1000.0,
        );
        for n in &member_wind.notes {
            println!("  {n}");
        }
    }
    let wind = member_wind.loads;
    let runs: Vec<(String, Vec<NodalLoad>)> = match (&args.loads, &args.structure) {
        (Some(path), Some(structure)) => read_loads(path, structure, &args.case)?
            .into_iter()
            .map(|(case, at)| {
                let mut nodal = Vec::new();
                for (point, [t, l, v]) in at {
                    let at = model.point_load(&point, [t, l, -v]).ok_or_else(|| {
                        format!("{case}: the model has no node for point {point}")
                    })?;
                    if let Some(n) = &at.note {
                        println!("  {case}: {n}");
                    }
                    nodal.extend(at.loads);
                }
                Ok((case, nodal))
            })
            .collect::<Result<_, String>>()?,
        _ => {
            if model.loads.is_empty() && weight == 0.0 && wind.is_empty() {
                return Err("the model has no [[load]]; give --loads and --structure".into());
            }
            vec![("model".into(), model.loads.clone())]
        }
    };
    let mut csv = String::from("case,member,kind,section,axial_n,usage,governs\n");
    for (case, mut nodal) in runs {
        nodal.extend(wind.iter().copied());
        let started = std::time::Instant::now();
        let loads = Loads {
            nodal,
            self_weight: weight,
        };
        let (s, usage) = ohl_structures_fe::analyse(
            &model,
            &loads,
            args.strength,
            &ohl_structures_fe::default_budget(),
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
        if let Some(note) = ohl_structures_fe::stabilisation_note(&model, &s) {
            println!("  {note}");
        }
        if let Some(note) = ohl_structures_fe::tension_only_note(&model, &s) {
            println!("  {note}");
        }
        if let Some(note) = ohl_structures_fe::kept_active_note(&model, &s) {
            println!("  {note}");
        }
        if let Some(note) = ohl_structures_fe::convergence_note(&s) {
            println!("  {note}");
        }
        let mut sorted: Vec<&ohl_structures_fe::MemberUsage> =
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
                ohl_core::text::csv_field(&case),
                ohl_core::text::csv_field(&m.id),
                m.kind.as_str(),
                ohl_core::text::csv_field(&model.sections[m.section].id),
                s.members[u.member].axial,
                u.usage
                    .map_or(String::new(), |x| format!("{:.1}", x * 100.0)),
                ohl_core::text::csv_field(&u.governs)
            );
        }
    }
    if let Some(out) = &args.csv {
        std::fs::write(out, csv).map_err(|e| format!("{}: {e}", out.display()))?;
        println!("written to {}", out.display());
    }
    Ok(())
}

/// `ohl structures pole`.
pub fn pole(args: &PoleArgs) -> Result<(), String> {
    let text = std::fs::read_to_string(&args.structure_type)
        .map_err(|e| format!("{}: {e}", args.structure_type.display()))?;
    let t = ohl_structures::StructureType::parse(&args.structure_type, &text)
        .map_err(|e| e.to_string())?;
    let pole = match &args.pole {
        Some(p) => Pole::load(p)?,
        None => {
            let sections = match &args.sections {
                Some(p) => ohl_sections::SectionLibrary::load(p)?,
                None => ohl_sections::SectionLibrary::default(),
            };
            let dir = args
                .structure_type
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf();
            ohl_structures_fe::pole::structure_pole(&t, &dir, &sections)
                .ok_or_else(|| format!("type {} has no pole: give a .ohlp file", t.id))??
        }
    };
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
        for (label, [tr, lo, ve]) in at {
            let point = t
                .point_local(&label, None)
                .ok_or_else(|| format!("type {} has no point {label}", t.id))?;
            loads.push(PoleLoad {
                point,
                force: [tr, lo, -ve],
            });
        }
        by_case.push((case, loads));
    }
    for (case, loads) in &by_case {
        let c = ohl_structures_fe::check_pole(
            &pole,
            loads,
            wind,
            args.strength,
            !args.no_p_delta,
            &ohl_structures_fe::default_budget(),
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
        let dir = std::env::temp_dir().join(format!("ohl-fe-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("loads.csv");
        std::fs::write(
            &path,
            "case,structure,point,label,sets,x,y,z,transverse,longitudinal,vertical,\
             factored_transverse,factored_longitudinal,factored_vertical\n\
             wind,T2,A1-1,\"a, b\",1,0,0,0,1,2,3,10,20,30\n\
             wind,T2,A1-1,,1,0,0,0,1,2,3,1,2,3\n\
             wind,T2,,wind on the structure,,0,0,0,1,2,3,7,7,7\n\
             wind,T3,A1-1,,1,0,0,0,1,2,3,5,5,5\n\
             ice,T2,A1-2,,3001,0,0,0,1,2,3,4,5,6\n",
        )
        .unwrap();
        let r = read_loads(&path, "T2", &[]).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].0, "wind");
        assert_eq!(r[0].1, vec![("A1-1".to_owned(), [11.0, 22.0, 33.0])]);
        assert_eq!(r[1].1, vec![("A1-2".to_owned(), [4.0, 5.0, 6.0])]);
        assert_eq!(read_loads(&path, "T2", &["ice".into()]).unwrap().len(), 1);
        let e = read_loads(&path, "T9", &[]).unwrap_err();
        assert!(e.contains("T2, T3"), "{e}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
