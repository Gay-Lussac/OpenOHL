// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Environment variables of OpenOHL (`OHL_…`). Their names of before
//! 2026-10-18 (`OLS_…`) are no longer read (removed 2026-10-26).

use std::ffi::OsString;

/// The variable `name` (`OHL_…`), as [`std::env::var_os`].
pub fn var_os(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

/// The variable `name` (`OHL_…`), as [`std::env::var`].
pub fn var(name: &str) -> Result<String, std::env::VarError> {
    std::env::var(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read() {
        // Not set (a name no one sets).
        assert!(var_os("OHL_NO_SUCH_VARIABLE_8231").is_none());
        assert!(var("OHL_NO_SUCH_VARIABLE_8231").is_err());
        assert!(var("PATH").is_ok());
    }
}
