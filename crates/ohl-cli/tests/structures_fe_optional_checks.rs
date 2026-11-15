// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl structures fe` with the optional checks of 2026-11-15 (owner):
//! `--redundant-force` / `[analysis] redundant_force` and
//! `--climbing-load` / `[analysis] climbing_load`, both off by default, on
//! an invented column braced by two redundant members.

use std::path::PathBuf;
use std::process::Command;

/// A column A–M–B (2 × 2 m, pinned at A, B held across) loaded by 100 kN
/// down at B, braced at M by two horizontal redundant members.
const MODEL: &str = r#"format = "ohl-structure-model"
version = 7
name = "Braced column (invented)"

[analysis]
capacity = "simple"
ANALYSIS

[[material]]
id = "steel"
elasticity = "210 GPa"
yield = "355 MPa"

[[section]]
id = "leg"
material = "steel"
area = "40 cm2"
i_min = "400 cm4"

[[section]]
id = "small"
material = "steel"
area = "1 cm2"
iy = "1 cm4"
iz = "1 cm4"
wy = "1 cm3"
wz = "1 cm3"

[[node]]
id = "A"
point = [0, 0, 0]
support = "pinned"

[[node]]
id = "M"
point = [0, 0, 2]

[[node]]
id = "B"
point = [0, 0, 4]
support = ["x", "y"]

[[node]]
id = "S1"
point = [2, 0, 2]
support = "pinned"

[[node]]
id = "S2"
point = [0, 2, 2]
support = "pinned"

[[member]]
id = "AM"
nodes = ["A", "M"]
section = "leg"
design = { class = "leg" }

[[member]]
id = "MB"
nodes = ["M", "B"]
section = "leg"
design = { class = "leg" }

[[member]]
id = "R1"
nodes = ["M", "S1"]
section = "small"
design = { class = "redundant" }

[[member]]
id = "R2"
nodes = ["M", "S2"]
section = "small"
design = { class = "redundant" }

[[load]]
node = "B"
force = ["0 kN", "0 kN", "-100 kN"]
"#;

fn write(name: &str, analysis: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-cli-optional-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("column.ohlfe");
    std::fs::write(&p, MODEL.replace("ANALYSIS", analysis)).unwrap();
    p
}

fn ohl(args: &[&std::ffi::OsStr]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_optional_checks_from_the_file_and_the_command_line() {
    // Off by default: no nominal force, no climbing list, the CSV as
    // before.
    let plain = write("plain", "");
    let csv = plain.with_extension("csv");
    let (ok, out, err) = ohl(&[
        "structures".as_ref(),
        "fe".as_ref(),
        plain.as_os_str(),
        "--csv".as_ref(),
        csv.as_os_str(),
    ]);
    assert!(ok, "{out}\n{err}");
    assert!(
        !out.contains("[redundant force:") && !out.contains("climbing"),
        "{out}"
    );
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(
        text.starts_with("case,member,kind,section,axial_n,usage,governs\n"),
        "{text}"
    );
    // From the file: 2.5 % of 100 kN on R1, R2 (5.18 kN Euler: 48 %);
    // 1100 N on the two flat redundants (550 N m on 1 cm³ × 355 MPa).
    let file = write("file", "redundant_force = 2.5\nclimbing_load = \"1100 N\"");
    let csv = file.with_extension("csv");
    let (ok, out, err) = ohl(&[
        "structures".as_ref(),
        "fe".as_ref(),
        file.as_os_str(),
        "--csv".as_ref(),
        csv.as_os_str(),
    ]);
    assert!(ok, "{out}\n{err}");
    assert!(
        out.contains(
            "[redundant force: 2.5 % of 100.00 kN, the compression of AM at M (the member it \
             braces): 2.50 kN — governs; own force 0 %]"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "climbing: climbing load (alone, not a load case; ASCE 10-97 commentary C2.2)"
        ),
        "{out}"
    );
    assert!(
        out.contains("2 OVER first yield: R1 155 %, R2 155 %"),
        "{out}"
    );
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(
        text.starts_with("case,member,kind,section,axial_n,usage,governs,climbing\n"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("model,R1,") && l.ends_with(",154.9")),
        "{text}"
    );
    // The command line wins: off (0), or another share.
    let (ok, out, err) = ohl(&[
        "structures".as_ref(),
        "fe".as_ref(),
        file.as_os_str(),
        "--redundant-force".as_ref(),
        "0".as_ref(),
        "--climbing-load".as_ref(),
        "0 N".as_ref(),
    ]);
    assert!(ok, "{out}\n{err}");
    assert!(
        !out.contains("[redundant force:") && !out.contains("climbing"),
        "{out}"
    );
    let (ok, out, err) = ohl(&[
        "structures".as_ref(),
        "fe".as_ref(),
        plain.as_os_str(),
        "--redundant-force".as_ref(),
        "1".as_ref(),
    ]);
    assert!(ok, "{out}\n{err}");
    assert!(out.contains("[redundant force: 1 % of 100.00 kN"), "{out}");
    let (ok, _, err) = ohl(&[
        "structures".as_ref(),
        "fe".as_ref(),
        plain.as_os_str(),
        "--redundant-force".as_ref(),
        "250".as_ref(),
    ]);
    assert!(!ok && err.contains("redundant_force"), "{err}");
}
