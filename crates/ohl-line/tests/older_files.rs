// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Files of older versions read as the current one: the example lines as
//! they were written in version 2 (`tests/data/*-v2.ols.json`) give the
//! same line as the examples, written in the current version.

use std::path::Path;

#[test]
fn version_2_examples_read_as_the_current_ones() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    for f in ["synthetic-line", "synthetic-line-typed", "crossing-line"] {
        let old = ohl_line::Line::load(&here.join(format!("tests/data/{f}-v2.ols.json"))).unwrap();
        let new =
            ohl_line::Line::load(&here.join(format!("../../examples/synthetic-line/{f}.ohl.json")))
                .unwrap();
        assert_eq!(new.version, ohl_line::LINE_FILE_VERSION, "{f}");
        assert_eq!(old, new, "{f}");
    }
}
