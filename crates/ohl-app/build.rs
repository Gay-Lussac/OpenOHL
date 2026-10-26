// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The build id (`OHL_BUILD_ID`, compared with the structure
//! application's before it is started) and, with the Python console, the
//! embedded interpreter's library directory as a run-time search path
//! (macOS and Linux), so the app finds libpython without environment
//! variables.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    build_id();
    #[cfg(feature = "python")]
    {
        let config = pyo3_build_config::get();
        if let Some(dir) = config.lib_dir()
            && std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os != "windows")
        {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
        }
    }
}

/// The build id (`OHL_BUILD_ID`): the git commit the program is built
/// from (`OHL_BUILD_ID` in the environment overrides it), else
/// `unknown`.
/// The line application and the structure application compare theirs:
/// two programs of the same commit belong together. Built again when the
/// commit changes (the files `HEAD` points at). The same function is in
/// `apps/structure/ohl-structure-app/build.rs`.
fn build_id() {
    println!("cargo:rerun-if-env-changed=OHL_BUILD_ID");
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
    let id = match std::env::var("OHL_BUILD_ID") {
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
