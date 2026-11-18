// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The method's parameters: none. ASCE 10-97's design strengths carry no
//! resistance factor to set (the load factors are the loading's, not the
//! member check's), and the core had no setting for its ASCE 10-97 check
//! before the move (2026-11-18). A parameter file for this plugin
//! (`Plugins/asce10.toml` beside a structure, the design-code plugins'
//! mechanism) must therefore be empty: one with a value in it is refused,
//! so that a value meant for the check is never silently left out.

/// What a parameter file for this method says (`text`, "" none). Err: it
/// holds a value (the method takes none) or does not read.
pub fn parse(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let v: toml::Table = text.parse().map_err(|e| format!("parameter file: {e}"))?;
    if v.is_empty() {
        return Ok(());
    }
    let keys: Vec<String> = v.keys().cloned().collect();
    Err(format!(
        "parameter file: ASCE 10-97 (method asce10-97) takes no parameters ({} given); remove \
         the file Plugins/asce10.toml or its values",
        keys.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_parameters_and_values_refused() {
        assert_eq!(parse(""), Ok(()));
        assert_eq!(parse("# a comment only\n"), Ok(()));
        assert!(parse("[resistance]\ngamma_m1 = 1.1\n").is_err());
        assert!(parse("not toml = = 1").is_err());
    }
}
