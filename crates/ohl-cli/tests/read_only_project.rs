// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Commands that only read or compute open a project file read-only
//! (owner 2026-11-15): its bytes and modification time stay as they were
//! and no `-wal` or `-shm` file is made beside it; a command that writes
//! still writes.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line")
        .join(name)
}

fn ohl(args: &[&OsStr]) -> (bool, String, String) {
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

fn sidecars(path: &Path) -> Vec<&'static str> {
    ["-wal", "-shm", "-journal"]
        .into_iter()
        .filter(|s| {
            let mut p = path.as_os_str().to_owned();
            p.push(s);
            Path::new(&p).exists()
        })
        .collect()
}

#[test]
fn computing_commands_leave_the_project_file_unchanged() {
    let d = std::env::temp_dir().join(format!("ohl-cli-read-only-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    let project = d.join("p.ohl");
    let (ok, out, err) = ohl(&[
        "project".as_ref(),
        "import".as_ref(),
        example("synthetic-line.ohl.json").as_os_str(),
        "--into".as_ref(),
        project.as_os_str(),
        "--user".as_ref(),
        "ana".as_ref(),
    ]);
    assert!(ok, "{out}\n{err}");
    assert!(sidecars(&project).is_empty(), "{:?}", sidecars(&project));
    let bytes = std::fs::read(&project).unwrap();
    let modified = std::fs::metadata(&project).unwrap().modified().unwrap();
    let csv = d.join("loads.csv");
    let line = d.join("line.ohl.json");
    let weather = example("weather-cases.ohlwc");
    let cases = example("load-cases.ohllc");
    let runs: [Vec<&OsStr>; 4] = [
        vec![
            "line".as_ref(),
            project.as_os_str(),
            "--weather".as_ref(),
            weather.as_os_str(),
            "--load-cases".as_ref(),
            cases.as_os_str(),
            "--loads-csv".as_ref(),
            csv.as_os_str(),
        ],
        vec![
            "project".as_ref(),
            "export".as_ref(),
            project.as_os_str(),
            "--out".as_ref(),
            line.as_os_str(),
        ],
        vec!["project".as_ref(), "log".as_ref(), project.as_os_str()],
        vec!["project".as_ref(), "status".as_ref(), project.as_os_str()],
    ];
    for args in &runs {
        let (ok, out, err) = ohl(args);
        assert!(ok, "{args:?}\n{out}\n{err}");
        assert_eq!(std::fs::read(&project).unwrap(), bytes, "{args:?}");
        assert_eq!(
            std::fs::metadata(&project).unwrap().modified().unwrap(),
            modified,
            "{args:?}"
        );
        assert!(
            sidecars(&project).is_empty(),
            "{args:?}: {:?}",
            sidecars(&project)
        );
    }
    assert!(std::fs::metadata(&csv).unwrap().len() > 0);
    // A writing command writes as before.
    let (ok, out, err) = ohl(&[
        "project".as_ref(),
        "apply".as_ref(),
        project.as_os_str(),
        "--user".as_ref(),
        "ana".as_ref(),
        r#"{"op": "set_wind_factor", "section": "S1", "spans": [1, 2], "factor": 1.3}"#.as_ref(),
    ]);
    assert!(ok, "{out}\n{err}");
    assert_ne!(std::fs::read(&project).unwrap(), bytes);
    std::fs::remove_dir_all(&d).ok();
}
