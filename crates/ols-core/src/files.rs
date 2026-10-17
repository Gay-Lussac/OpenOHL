// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Files on disk: format headers, the libraries shipped with OLS CAD and
//! library folders.

use std::path::{Path, PathBuf};

/// Checks a file's `format` and `version` header — the one policy of every
/// OLS CAD format: both optional; the format, given, must be `expected`;
/// any version up to `supported` is read (older ones converted by the
/// reader), a newer one refused. `what` names the kind of file in the
/// message ("rule", "weather case", …).
pub fn check_header(
    format: Option<&str>,
    version: Option<u32>,
    expected: &str,
    supported: u32,
    what: &str,
) -> Result<(), String> {
    if let Some(f) = format.filter(|f| *f != expected) {
        return Err(format!("format \"{f}\" is not \"{expected}\""));
    }
    if version.is_some_and(|v| v > supported) {
        return Err(format!(
            "{what} format version {} newer than supported ({supported}): written by a newer OLS CAD",
            version.unwrap_or_default()
        ));
    }
    Ok(())
}

/// A folder of the libraries shipped with OLS CAD (`libraries/<kind>`):
/// in the current folder, else next to the program or above it (a build
/// in `target/release` of the source tree); the relative path if neither
/// exists.
pub fn shipped_library(kind: &str) -> PathBuf {
    let rel = Path::new("libraries").join(kind);
    if rel.is_dir() {
        return rel;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.ancestors()
                .skip(1)
                .map(|d| d.join(&rel))
                .find(|d| d.is_dir())
        })
        .unwrap_or(rel)
}

/// The files with `extension` in `dir`, sorted (none if it cannot be read).
pub fn files_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| {
            it.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == extension))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The folder of `file` (`.` for a bare file name).
pub fn dir_of(file: &Path) -> PathBuf {
    file.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers() {
        assert!(check_header(None, None, "ols-x", 1, "x").is_ok());
        assert!(check_header(Some("ols-x"), Some(1), "ols-x", 1, "x").is_ok());
        assert_eq!(
            check_header(Some("ols-y"), None, "ols-x", 1, "x").unwrap_err(),
            "format \"ols-y\" is not \"ols-x\""
        );
        assert_eq!(
            check_header(None, Some(2), "ols-x", 1, "x").unwrap_err(),
            "x format version 2 newer than supported (1): written by a newer OLS CAD"
        );
    }

    #[test]
    fn folders() {
        assert_eq!(dir_of(Path::new("a.ols")), Path::new("."));
        assert_eq!(dir_of(Path::new("d/a.ols")), Path::new("d"));
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let found = files_with_extension(&here.join("src"), "rs");
        assert!(found.windows(2).all(|w| w[0] < w[1]));
        assert!(found.iter().any(|p| p.ends_with("files.rs")));
        assert!(files_with_extension(&here.join("no-such-dir"), "rs").is_empty());
    }
}
