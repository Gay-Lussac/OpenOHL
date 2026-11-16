// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl structures feet`: a placed structure's measured foot movements in
//! a line file (owner 2026-11-16; [`ohl_line::feet`]) — listed, set or
//! cleared. The structure is modelled from its plans; its feet's measured
//! movements move it (the reference state, the level-2 solves, the
//! structure checks).

use std::path::PathBuf;

use clap::Args;
use ohl_line::Line;
use ohl_line::feet::{FootMovement, FootMovements};

/// `ohl structures feet`.
#[derive(Args)]
pub struct FeetArgs {
    /// Line file (.ohl.json).
    file: PathBuf,
    /// The structure (none: every structure's foot movements listed).
    #[arg(long)]
    structure: Option<String>,
    /// A measured foot: "JOINT:across,ahead,up" (mm, structure axes: to
    /// the right of the line, ahead, up) or
    /// "JOINT:across,ahead,up:right,ahead" with its tilt (degrees, the
    /// axis leaning to the right and ahead). JOINT: a support joint of the
    /// structure's model, "base" for a pole, "all" for every foot alike.
    /// Repeat for each foot; the feet given replace the structure's.
    #[arg(long = "foot", requires = "structure")]
    feet: Vec<String>,
    /// Where they come from (who measured them, how).
    #[arg(long, requires = "structure")]
    source: Option<String>,
    /// When they were measured (by convention 2026-05-12).
    #[arg(long, requires = "structure")]
    date: Option<String>,
    /// Take the structure's foot movements away.
    #[arg(long, requires = "structure", conflicts_with_all = ["feet", "source", "date"])]
    clear: bool,
    /// The line file written (may be the one read); needed to change it.
    #[arg(long)]
    out: Option<PathBuf>,
}

/// `JOINT:across,ahead,up[:right,ahead]` (mm, degrees).
fn foot(text: &str) -> Result<FootMovement, String> {
    let bad = || {
        format!(
            "--foot \"{text}\": JOINT:across,ahead,up (mm) or JOINT:across,ahead,up:right,ahead \
             (mm, degrees)"
        )
    };
    let mut parts = text.split(':');
    let joint = parts
        .next()
        .map(str::trim)
        .filter(|j| !j.is_empty())
        .ok_or_else(bad)?;
    let numbers = |s: Option<&str>| -> Result<Vec<f64>, String> {
        s.map_or(Ok(Vec::new()), |s| {
            s.split(',')
                .map(|x| x.trim().parse::<f64>().map_err(|_| bad()))
                .collect()
        })
    };
    let t = numbers(parts.next())?;
    let tilt = numbers(parts.next())?;
    if parts.next().is_some() || t.len() != 3 || !(tilt.is_empty() || tilt.len() == 2) {
        return Err(bad());
    }
    Ok(FootMovement {
        joint: joint.to_owned(),
        translation: [t[0] / 1e3, t[1] / 1e3, t[2] / 1e3],
        tilt_deg: if tilt.is_empty() {
            [0.0; 2]
        } else {
            [tilt[0], tilt[1]]
        },
    })
}

/// One structure's foot movements in words, one line per foot.
fn words(id: &str, f: &FootMovements) -> String {
    let mut out = format!("{id}: {}", f.summary());
    for x in &f.feet {
        out.push_str(&format!(
            "\n  {}: across {:.1} mm, ahead {:.1} mm, up {:.1} mm{}",
            x.joint,
            x.translation[0] * 1e3,
            x.translation[1] * 1e3,
            x.translation[2] * 1e3,
            if x.tilt_deg == [0.0; 2] {
                String::new()
            } else {
                format!(
                    ", tilt {:.3}° to the right, {:.3}° ahead",
                    x.tilt_deg[0], x.tilt_deg[1]
                )
            }
        ));
    }
    out
}

/// Lists, sets or clears a structure's foot movements (written to
/// `--out`).
pub fn feet(args: &FeetArgs) -> Result<(), String> {
    if ohl_core::files::is_project_file(&args.file) {
        return Err(format!(
            "{}: a project file — change the structure in OHL-Design (its window: Foot \
             movements) or put it with `ohl project apply`",
            args.file.display()
        ));
    }
    let mut line = Line::load(&args.file).map_err(|e| e.to_string())?;
    let Some(id) = &args.structure else {
        let mut any = false;
        for s in &line.structures {
            if let Some(f) = s.foot_movements.as_ref().filter(|f| !f.is_empty()) {
                println!("{}", words(&s.id, f));
                any = true;
            }
        }
        if !any {
            println!("no structure has foot movements: every structure as designed");
        }
        return Ok(());
    };
    let changing =
        args.clear || !args.feet.is_empty() || args.source.is_some() || args.date.is_some();
    let s = line
        .structures
        .iter_mut()
        .find(|s| &s.id == id)
        .ok_or_else(|| format!("no structure {id}"))?;
    if changing {
        if args.clear {
            s.foot_movements = None;
        } else {
            let mut f = s.foot_movements.clone().unwrap_or_default();
            if !args.feet.is_empty() {
                f.feet = args
                    .feet
                    .iter()
                    .map(|x| foot(x))
                    .collect::<Result<_, _>>()?;
            }
            if let Some(x) = &args.source {
                f.source = x.trim().to_owned();
            }
            if let Some(x) = &args.date {
                f.date = x.trim().to_owned();
            }
            if let Some(p) = f.problem() {
                return Err(format!("{id}: {p}"));
            }
            s.foot_movements = (!f.is_empty()).then_some(f);
        }
    }
    match &s.foot_movements {
        Some(f) => println!("{}", words(id, f)),
        None => println!("{id}: no foot movements (as designed)"),
    }
    if !changing {
        return Ok(());
    }
    let out = args
        .out
        .as_ref()
        .ok_or("give --out FILE to write the line with them")?;
    line.validate().map_err(|e| e.to_string())?;
    std::fs::write(out, line.to_json()).map_err(|e| format!("{}: {e}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_foot_reads_in_mm_and_degrees() {
        let f = foot("F2:0,0,-12:0.1,0").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(f.joint, "F2");
        assert!((f.translation[2] + 0.012).abs() < 1e-15);
        assert_eq!(f.tilt_deg, [0.1, 0.0]);
        assert!(foot("all:1,2").is_err());
        assert!(foot(":1,2,3").is_err());
    }
}
