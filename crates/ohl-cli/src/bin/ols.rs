// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ols`: the former name of `ohl`, kept for a transition period
//! (the project is now OpenOHL). It says so on the error output and runs
//! `ohl` from its own folder with the same arguments; the exit code is
//! `ohl`'s.

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    eprintln!("note: `ols` is now called `ohl`; the old name will be removed in a later version");
    let file = format!("ohl{}", std::env::consts::EXE_SUFFIX);
    let Some(program) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&file)))
        .filter(|p| p.is_file())
    else {
        eprintln!(
            "error: {file} is not next to ols: build it with  cargo build --release -p ohl-cli"
        );
        return ExitCode::FAILURE;
    };
    let mut command = Command::new(&program);
    command.args(std::env::args_os().skip(1));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Replaces this process: signals and the exit code are `ohl`'s.
        let e = command.exec();
        eprintln!("error: {}: {e}", program.display());
        ExitCode::FAILURE
    }
    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => status
            .code()
            .and_then(|c| u8::try_from(c).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(e) => {
            eprintln!("error: {}: {e}", program.display());
            ExitCode::FAILURE
        }
    }
}
