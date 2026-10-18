// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols-structure`: the former name of `ohl-structure`, kept for a transition period
//! (the project is now OpenOHL). It says so on the error output and runs
//! `ohl-structure` from its own folder with the same arguments; the exit code is
//! `ohl-structure`'s.

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    eprintln!(
        "note: `ols-structure` is now called `ohl-structure`; the old name will be removed in a later version"
    );
    let file = format!("ohl-structure{}", std::env::consts::EXE_SUFFIX);
    let Some(program) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&file)))
        .filter(|p| p.is_file())
    else {
        eprintln!(
            "error: {file} is not next to ols-structure: build it with  cargo build --release -p ohl-structure-app"
        );
        return ExitCode::FAILURE;
    };
    let mut command = Command::new(&program);
    command.args(std::env::args_os().skip(1));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Replaces this process: signals and the exit code are `ohl-structure`'s.
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
