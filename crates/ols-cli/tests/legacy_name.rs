// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The former program name `ols` still works: it says it is now `ohl`
//! and runs it with the same arguments (same output, same exit code).

use std::process::Command;

#[test]
fn the_former_name_runs_the_new_program() {
    let new = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("--version")
        .output()
        .unwrap();
    let old = Command::new(env!("CARGO_BIN_EXE_ols"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(new.status.success() && old.status.success());
    let text = String::from_utf8_lossy(&new.stdout);
    assert!(text.starts_with("ohl "), "{text}");
    assert_eq!(old.stdout, new.stdout);
    let note = String::from_utf8_lossy(&old.stderr);
    assert!(note.contains("now called `ohl`"), "{note}");
    // An error: the same exit code as the program it runs.
    let bad_new = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("no-such-command")
        .output()
        .unwrap();
    let bad_old = Command::new(env!("CARGO_BIN_EXE_ols"))
        .arg("no-such-command")
        .output()
        .unwrap();
    assert!(!bad_new.status.success());
    assert_eq!(bad_old.status.code(), bad_new.status.code());
}
