// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl safety-volume` on the synthetic example line (invented values):
//! high wind with the swing arc, a distance by voltage, written as DXF
//! (with its .prj), IFC4 and KMZ in an EPSG system given on the command
//! line.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn safety_volumes_are_written_as_dxf_ifc_and_kmz() {
    let dir = std::env::temp_dir().join(format!("ohl-safety-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let ex = root().join("examples/synthetic-line");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("safety-volume")
        .arg(ex.join("synthetic-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .args(["--cases", "high wind", "--wind", "arc"])
        .args(["--distance", "{ by_voltage = [[0, 2], [220, 4]] }"])
        .args([
            "--tolerance",
            "0.2",
            "--per",
            "circuit",
            "--crs",
            "EPSG:2056",
        ])
        .arg("--out")
        .arg(&dir)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Two circuits, 220 kV: 4 m; the band's margin by default.
    assert!(text.contains("Example 1: 220 kV — d 4.00 m"), "{text}");
    assert!(text.contains("swing arc (band margin 0.15 m)"), "{text}");
    assert!(text.contains("Branch: 220 kV — d 4.00 m"), "{text}");
    for ext in ["dxf", "prj", "ifc", "kmz"] {
        let f = dir.join(format!("safety-volume.{ext}"));
        assert!(f.is_file(), "{} missing\n{text}", f.display());
    }
    let ifc = std::fs::read_to_string(dir.join("safety-volume.ifc")).unwrap();
    assert!(ifc.contains("IFCPROJECTEDCRS('EPSG:2056'"));
    // Each volume a spatial zone (2026-10-21), referenced in the site.
    assert_eq!(ifc.matches("IFCSPATIALZONE(").count(), 2);
    assert_eq!(ifc.matches("IFCBUILDINGELEMENTPROXY(").count(), 0);
    assert_eq!(
        ifc.matches("IFCRELREFERENCEDINSPATIALSTRUCTURE(").count(),
        1
    );
    // No distance: refused, none is built in.
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("safety-volume")
        .arg(ex.join("synthetic-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .arg("--out")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--distance"));
    // A negative band margin: refused.
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("safety-volume")
        .arg(ex.join("synthetic-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .args(["--cases", "high wind", "--wind", "arc", "--distance", "3"])
        .args(["--band-margin=-0.1"])
        .arg("--out")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--band-margin"));
    std::fs::remove_dir_all(&dir).ok();
}

/// `--system` (by name) and `--per system`: the example line's one system
/// holds both circuits, so one volume, named after the system; an
/// unknown system refused.
#[test]
fn safety_volumes_by_system() {
    let dir = std::env::temp_dir().join(format!("ohl-safety-system-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let ex = root().join("examples/synthetic-line");
    let run = |system: &str| {
        Command::new(env!("CARGO_BIN_EXE_ohl"))
            .arg("safety-volume")
            .arg(ex.join("synthetic-line.ohl.json"))
            .arg("--weather")
            .arg(ex.join("weather-cases.ohlwc"))
            .args(["--cases", "max temperature", "--distance", "3"])
            .args(["--tolerance", "0.3", "--per", "system", "--format", "dxf"])
            .args(["--system", system])
            .arg("--out")
            .arg(&dir)
            .output()
            .unwrap()
    };
    let out = run("220 kV example system");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("220 kV example system: 220 kV — d 3.00 m"),
        "{text}"
    );
    assert_eq!(text.matches(" triangles, ").count(), 1, "{text}");
    let out = run("nope");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no system \"nope\""));
    // `none`: the wires without a system — the example line has none
    // (no earth wire), said so (2026-10-21).
    let out = run("none");
    assert!(!out.status.success());
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(e.contains("no wire of the systems (no system)"), "{e}");
    // Two spans of S1 and the branch's first span, per wire: the wires
    // S1 and S3 only (S2 untouched).
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("safety-volume")
        .arg(ex.join("synthetic-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .args(["--cases", "max temperature", "--distance", "3"])
        .args(["--tolerance", "0.3", "--format", "dxf"])
        .args(["--span", "S1:1", "--span", "S1:2", "--span", "S3:1"])
        .arg("--out")
        .arg(&dir)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(text.matches(" triangles, ").count(), 2, "{text}");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("safety-volume")
        .arg(ex.join("synthetic-line.ohl.json"))
        .args([
            "--distance",
            "3",
            "--span",
            "S1:9",
            "--cases",
            "max temperature",
        ])
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .arg("--out")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no span 9"));
    std::fs::remove_dir_all(&dir).ok();
}
