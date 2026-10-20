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
    // Two circuits, 220 kV: 4 m.
    assert!(text.contains("Example 1: 220 kV — d 4.00 m"), "{text}");
    assert!(text.contains("Branch: 220 kV — d 4.00 m"), "{text}");
    for ext in ["dxf", "prj", "ifc", "kmz"] {
        let f = dir.join(format!("safety-volume.{ext}"));
        assert!(f.is_file(), "{} missing\n{text}", f.display());
    }
    let ifc = std::fs::read_to_string(dir.join("safety-volume.ifc")).unwrap();
    assert!(ifc.contains("IFCPROJECTEDCRS('EPSG:2056'"));
    assert_eq!(ifc.matches("IFCBUILDINGELEMENTPROXY(").count(), 2);
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
    std::fs::remove_dir_all(&dir).ok();
}
