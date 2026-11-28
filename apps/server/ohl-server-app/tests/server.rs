// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OHL-Server against a network's server on a throwaway PostgreSQL
//! cluster (a synthetic country; without PostgreSQL's binaries the tests
//! say so and pass):
//!
//! - connect mode: a network admin sees every page; a change is saved only
//!   after "what changes" is shown, and Escape cancels it; an invitation
//!   is made and shown; the Operations tabs answer;
//! - a line admin has no Operations page, and sees their line only;
//! - host mode: the server starts inside the app, the admin signs in to
//!   it, and closing asks first, naming who is connected;
//! - (ignored, run by hand) the 12 000-pylon country: open and frame
//!   times.
//!
//! With `OHL_RENDER_PNG` set, the user guide's pictures are made (as
//! USER=tester, on the synthetic country).

mod common;

use std::time::{Duration, Instant};

use common::{click, harness, network, shot, shown, shown_part, until};
use ohl_server::network::NetOptions;
use ohl_server::server::{Options, Server};
use ohl_server_app::App;
use ohl_server_app::app::{OpTab, Page, Window};
use ohl_server_app::conn::JoinHow;
use ohl_session::rights::{RightsCommand, Scope, Subject};

fn serve(url: &str, backups: Option<std::path::PathBuf>) -> Server {
    Server::start_network(
        Options {
            bind: "127.0.0.1:0".parse().unwrap(),
            newcomers: None,
            ..Options::default()
        },
        NetOptions {
            database: url.into(),
            heartbeat: Duration::from_millis(200),
            packagers: 1,
            readers: 2,
            idle_solve: false,
            backup_dir: backups,
            ..NetOptions::default()
        },
    )
    .unwrap()
}

fn connected(server: &Server, user: &str) -> egui_kittest::Harness<'static, App> {
    let mut app = App::new();
    app.connect_form.invitation = server.invitation_for(user).to_string();
    app.connect_form.user = user.into();
    let mut h = harness(app);
    click(&mut h, "Connect");
    until(&mut h, "the overview", |a| {
        a.conn
            .as_ref()
            .is_some_and(|c| c.data.overview.is_some() && c.data.rights.is_some())
    });
    h
}

fn open_page(h: &mut egui_kittest::Harness<'static, App>, page: Page) {
    h.state_mut().page = page;
    h.run_steps(3);
}

#[test]
fn a_network_admin_uses_every_page_and_saves_after_what_changes() {
    let Some(t) = network("app-admin", 240) else {
        return;
    };
    let backups = t.dir.join("backups");
    std::fs::create_dir_all(&backups).unwrap();
    let server = serve(&t.url, Some(backups));
    let mut h = connected(&server, "admin");
    for p in Page::ALL {
        assert!(shown(&h, p.name()), "{}", p.name());
    }
    assert!(shown_part(&h, "no backup is recorded"));
    shot(&mut h, "ohl-server-overview.png");

    // The map: every line, then structures once zoomed in.
    open_page(&mut h, Page::Map);
    until(&mut h, "the map", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.map.is_some())
    });
    let routes = h
        .state()
        .conn
        .as_ref()
        .unwrap()
        .data
        .map
        .as_ref()
        .unwrap()
        .routes
        .len();
    assert!(routes >= t.codes.len());
    shot(&mut h, "ohl-server-map.png");
    let b = h
        .state()
        .conn
        .as_ref()
        .unwrap()
        .data
        .map
        .as_ref()
        .unwrap()
        .bounds
        .unwrap();
    {
        let a = h.state_mut();
        a.map.centre = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        a.map.scale = 0.1;
    }
    until(&mut h, "the structures in view", |a| {
        a.conn.as_ref().is_some_and(|c| !c.data.dots.is_empty())
    });
    shot(&mut h, "ohl-server-map-structures.png");

    // Users and groups: an invitation made and shown.
    open_page(&mut h, Page::Users);
    shot(&mut h, "ohl-server-users.png");
    click(&mut h, "Invite a user…");
    assert!(shown(&h, "Invite a user"));
    h.state_mut().window = Some(Window::Invite {
        name: "dora".into(),
        display: String::new(),
        days: 30,
    });
    h.run_steps(2);
    click(&mut h, "Invite");
    until(&mut h, "the invitation", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.invitation.is_some())
    });
    assert!(shown(&h, "Invitation"));
    assert!(shown_part(&h, "Valid until"));
    shot(&mut h, "ohl-server-invitation.png");
    click(&mut h, "Close");
    assert!(h.state().conn.as_ref().unwrap().data.invitation.is_none());

    // Roles: the matrix, shipped roles read-only.
    open_page(&mut h, Page::Roles);
    assert!(shown(&h, "network_admin (shipped)"));
    shot(&mut h, "ohl-server-roles.png");

    // Assignments: a change shown with what changes; Escape cancels it.
    open_page(&mut h, Page::Assignments);
    until(&mut h, "the libraries", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.libraries.is_some())
    });
    let assign = RightsCommand::Assign {
        subject: Subject::User("vic".into()),
        role: Some("designer".into()),
        deny: false,
        scope: Scope::line(&t.codes[1]),
        note: "the east".into(),
    };
    h.state_mut().window = Some(Window::Change(assign.clone()));
    until(&mut h, "what changes", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.preview.is_some())
    });
    assert!(shown_part(&h, "1 user(s) gain"));
    shot(&mut h, "ohl-server-what-changes.png");
    h.key_press(eframe::egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().window.is_none(), "Escape cancels");
    let before = h
        .state()
        .conn
        .as_ref()
        .unwrap()
        .data
        .rights
        .as_ref()
        .unwrap()
        .assignments
        .len();
    h.state_mut().window = Some(Window::Change(assign));
    until(&mut h, "what changes", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.preview.is_some())
    });
    click(&mut h, "Save");
    until(&mut h, "the assignment saved", |a| {
        a.conn
            .as_ref()
            .and_then(|c| c.data.rights.as_ref())
            .is_some_and(|r| r.assignments.len() == before + 1)
    });
    shot(&mut h, "ohl-server-assignments.png");

    // See as user.
    open_page(&mut h, Page::SeeAs);
    h.state_mut().conn.as_mut().unwrap().ask_see_as("lena");
    until(&mut h, "see as", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.see_as.is_some())
    });
    assert!(shown_part(&h, "lena sees 1 line(s)"));
    shot(&mut h, "ohl-server-see-as.png");

    // Operations: every tab answers.
    open_page(&mut h, Page::Operations);
    for (tab, wait, pic) in [
        (OpTab::Sessions, "admin", "ohl-server-sessions.png"),
        (OpTab::Settings, "database", "ohl-server-settings.png"),
        (OpTab::Log, "changed the rights", "ohl-server-log.png"),
        (OpTab::Backups, "Back up now", "ohl-server-backups.png"),
        (
            OpTab::Migrations,
            "No migration pending",
            "ohl-server-migrations.png",
        ),
    ] {
        h.state_mut().op_tab = tab;
        let started = Instant::now();
        while !shown_part(&h, wait) {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "{tab:?}: {wait}"
            );
            h.run_steps(1);
            std::thread::sleep(Duration::from_millis(20));
        }
        shot(&mut h, pic);
    }
    drop(h);
    server.stop();
}

#[test]
fn a_line_admin_has_no_operations_and_sees_their_line() {
    let Some(t) = network("app-line-admin", 200) else {
        return;
    };
    let server = serve(&t.url, None);
    let mut h = connected(&server, "lena");
    assert!(!shown(&h, "Operations"));
    assert!(shown(&h, "Assignments"));
    open_page(&mut h, Page::Map);
    until(&mut h, "the map", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.map.is_some())
    });
    let map = h.state().conn.as_ref().unwrap().data.map.clone().unwrap();
    assert!(map.routes.iter().all(|r| r.model == t.codes[0]));
    // Asked anyway: refused by the server.
    h.state_mut().page = Page::Operations;
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Overview);
    drop(h);
    server.stop();
}

#[test]
fn host_mode_starts_the_server_and_closing_asks_first() {
    let Some(t) = network("app-host", 200) else {
        return;
    };
    let mut app = App::new();
    app.host_form.database = t.url.clone();
    app.host_form.bind = "127.0.0.1:0".into();
    app.host_form.user = "admin".into();
    let mut h = harness(app);
    click(&mut h, "Look at the database");
    assert!(shown_part(&h, "network admins: admin"));
    shot(&mut h, "ohl-server-host-form.png");
    click(&mut h, "Host");
    until(&mut h, "the hosted server's overview", |a| {
        a.hosted.is_some() && a.conn.as_ref().is_some_and(|c| c.data.overview.is_some())
    });
    // Bob joins the app's server.
    let inv = h
        .state()
        .hosted
        .as_ref()
        .unwrap()
        .server
        .invitation_for("bob");
    let bob = ohl_server::client::join_network(
        &inv,
        "bob",
        &ohl_server::client::NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    h.state_mut().request_close();
    until(&mut h, "who is connected", |a| {
        a.conn
            .as_ref()
            .is_some_and(|c| c.data.sessions.iter().any(|s| s.user == "bob"))
    });
    assert!(shown(&h, "Stop the server"));
    assert!(shown_part(&h, "bob ("));
    shot(&mut h, "ohl-server-closing.png");
    click(&mut h, "Cancel");
    assert!(!h.state().closing && h.state().hosted.is_some());
    h.state_mut().request_close();
    h.run_steps(3);
    click(&mut h, "Stop the server and close");
    assert!(h.state().hosted.is_none());
    drop(bob);
}

/// The 12 000-pylon synthetic country (the owner's size): the time from
/// Connect to the map drawn, and the map's frame time (whole country, and
/// zoomed in with its structures). By hand:
/// `cargo test -p ohl-server-app --release --test server -- --ignored --nocapture`.
#[test]
#[ignore]
fn measure_the_twelve_thousand_pylon_country() {
    let started = Instant::now();
    let Some(t) = network("app-12k", 12_000) else {
        return;
    };
    eprintln!(
        "12k: {} lines imported in {:.1} s",
        t.codes.len(),
        started.elapsed().as_secs_f64()
    );
    let s = Instant::now();
    let server = serve(&t.url, None);
    eprintln!("12k: server read in {:.1} s", s.elapsed().as_secs_f64());
    let mut app = App::new();
    app.join(JoinHow {
        invitation: server.invitation_for("admin"),
        user: "admin".into(),
        id_token: None,
    });
    app.page = Page::Map;
    let mut h = harness(app);
    let open = Instant::now();
    until(&mut h, "the map", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.map.is_some())
    });
    let open_s = open.elapsed().as_secs_f64();
    let frames = |h: &mut egui_kittest::Harness<'static, App>| {
        let mut ms = Vec::new();
        let mut draw = Vec::new();
        for _ in 0..30 {
            let t = Instant::now();
            h.run_steps(1);
            ms.push(t.elapsed().as_secs_f64() * 1e3);
            draw.push(h.state().map.draw_ms);
        }
        ms.sort_by(f64::total_cmp);
        draw.sort_by(f64::total_cmp);
        (ms[15], ms[28], draw[15])
    };
    let whole = frames(&mut h);
    let b = h
        .state()
        .conn
        .as_ref()
        .unwrap()
        .data
        .map
        .as_ref()
        .unwrap()
        .bounds
        .unwrap();
    {
        let a = h.state_mut();
        a.map.centre = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        a.map.scale = 0.04;
    }
    let z = Instant::now();
    until(&mut h, "the structures in view", |a| {
        a.conn.as_ref().is_some_and(|c| !c.data.dots.is_empty())
    });
    let dots_s = z.elapsed().as_secs_f64();
    let zoomed = frames(&mut h);
    let c = h.state().conn.as_ref().unwrap();
    eprintln!(
        "12k: {} routes, {} shared towers; open (connect → map) {open_s:.2} s; whole country: \
         frame median {:.1} ms (p95 {:.1}), map drawing {:.1} ms; zoomed (25 km): {} \
         structures in {dots_s:.2} s, frame median {:.1} ms (p95 {:.1}), map drawing {:.1} ms",
        c.data.map.as_ref().unwrap().routes.len(),
        c.data.map.as_ref().unwrap().shared.len(),
        whole.0,
        whole.1,
        whole.2,
        c.data.dots.len(),
        zoomed.0,
        zoomed.1,
        zoomed.2
    );
    drop(h);
    server.stop();
}
