// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OHL-Server without a server: the start page offers both modes; a bad
//! invitation and an unreachable database are said, nothing else
//! happens; the user guide's page has a section for every page of the
//! app.

mod common;

use common::{click, harness, shot, shown, until};
use ohl_server_app::App;
use ohl_server_app::app::Page;

#[test]
fn the_start_page_offers_both_modes_and_says_what_is_wrong() {
    let mut h = harness(App::new());
    for l in [
        "Host a network",
        "Connect to a server",
        "Host",
        "Connect",
        "Sign in…",
    ] {
        assert!(shown(&h, l), "{l}");
    }
    shot(&mut h, "ohl-server-start.png");
    h.state_mut().connect_form.invitation = "not an invitation".into();
    h.state_mut().connect_form.user = "tester".into();
    click(&mut h, "Connect");
    let e = h.state().error.clone().unwrap();
    assert!(e.starts_with("the invitation"), "{e}");
    assert!(h.state().conn.is_none());
    // Host mode with nothing named: said, nothing started.
    click(&mut h, "Host");
    until(&mut h, "the host's refusal", |a| {
        a.error
            .as_deref()
            .is_some_and(|e| e.contains("did not start"))
    });
    assert!(h.state().hosted.is_none());
}

#[test]
fn the_user_guide_has_a_section_for_every_page() {
    let guide = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/user-guide/ohl-server.md"),
    )
    .unwrap();
    for p in Page::ALL {
        assert!(
            ohl_ui::tool_window::has_heading(&guide, &ohl_ui::tool_window::anchor(p.name())),
            "{}",
            p.name()
        );
    }
    for anchor in [
        "ohl-server-the-admin-app",
        "host-mode",
        "what-changes",
        "sessions",
        "users-and-groups",
        "libraries",
        "lines",
        "settings",
        "identities",
        "explain",
        "drawing-a-region",
        "background-from-your-own-map-server",
    ] {
        assert!(ohl_ui::tool_window::has_heading(&guide, anchor), "{anchor}");
    }
}
