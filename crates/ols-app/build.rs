// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! With the Python console: the embedded interpreter's library directory
//! as a run-time search path (macOS and Linux), so the app finds libpython
//! without environment variables.

fn main() {
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
