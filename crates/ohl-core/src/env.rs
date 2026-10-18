// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Environment variables of OpenOHL (`OHL_…`): each is read under its name
//! of before 2026-10-18 (`OLS_…`) too, when it is not set.

use std::ffi::OsString;

/// The name `name` (`OHL_…`) had before 2026-10-18 (`OLS_…`).
fn legacy(name: &str) -> Option<String> {
    name.strip_prefix("OHL_").map(|rest| format!("OLS_{rest}"))
}

/// The variable `name` (`OHL_…`), else its former `OLS_…`, as
/// [`std::env::var_os`].
pub fn var_os(name: &str) -> Option<OsString> {
    std::env::var_os(name).or_else(|| legacy(name).and_then(std::env::var_os))
}

/// The variable `name` (`OHL_…`), else its former `OLS_…`, as
/// [`std::env::var`].
pub fn var(name: &str) -> Result<String, std::env::VarError> {
    match std::env::var(name) {
        Err(std::env::VarError::NotPresent) => match legacy(name) {
            Some(old) => std::env::var(old),
            None => Err(std::env::VarError::NotPresent),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn former_names() {
        assert_eq!(legacy("OHL_SHOTS").as_deref(), Some("OLS_SHOTS"));
        assert_eq!(legacy("HOME"), None);
        // Neither set (names no one sets).
        assert!(var_os("OHL_NO_SUCH_VARIABLE_8231").is_none());
        assert!(var("OHL_NO_SUCH_VARIABLE_8231").is_err());
        assert!(var("PATH").is_ok());
    }
}
