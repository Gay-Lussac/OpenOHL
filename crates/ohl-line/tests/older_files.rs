// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
