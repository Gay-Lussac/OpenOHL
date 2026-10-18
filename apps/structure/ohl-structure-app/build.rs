// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The build id `ohl-structure --version` prints (`OHL_BUILD_ID`): the
//! line application compares it with its own before it starts this one.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    build_id();
}

/// The build id (`OHL_BUILD_ID`): the git commit the program is built
/// from (`OHL_BUILD_ID` in the environment overrides it; `OLS_BUILD_ID`,
/// its name before 2026-10-18, too), else `unknown`.
/// The line application and the structure application compare theirs:
/// two programs of the same commit belong together. Built again when the
/// commit changes (the files `HEAD` points at). The same function is in
/// `crates/ohl-app/build.rs`.
fn build_id() {
    println!("cargo:rerun-if-env-changed=OHL_BUILD_ID");
    println!("cargo:rerun-if-env-changed=OLS_BUILD_ID");
    let dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let id = match std::env::var("OHL_BUILD_ID").or_else(|_| std::env::var("OLS_BUILD_ID")) {
        Ok(id) if !id.trim().is_empty() => id.trim().to_owned(),
        _ => match git(&["rev-parse", "--short=12", "HEAD"]) {
            Some(commit) => {
                let at = |p: String| {
                    let p = std::path::PathBuf::from(p);
                    if p.is_absolute() { p } else { dir.join(p) }
                };
                if let Some(d) = git(&["rev-parse", "--git-dir"]).map(at) {
                    println!("cargo:rerun-if-changed={}", d.join("HEAD").display());
                }
                if let Some(common) = git(&["rev-parse", "--git-common-dir"]).map(at) {
                    println!(
                        "cargo:rerun-if-changed={}",
                        common.join("packed-refs").display()
                    );
                    if let Some(r) = git(&["symbolic-ref", "-q", "HEAD"]) {
                        println!("cargo:rerun-if-changed={}", common.join(r).display());
                    }
                }
                commit
            }
            None => "unknown".to_owned(),
        },
    };
    println!("cargo:rustc-env=OHL_BUILD_ID={id}");
}
