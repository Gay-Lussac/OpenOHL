// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the app's tests share: a network of a synthetic country on a
//! throwaway PostgreSQL cluster (`ohl_store_pg::testing`, as the server's
//! own tests), the app in a test window, waiting for the server's
//! answers, pictures for the user guide (`OHL_RENDER_PNG`, a GPU
//! rendering).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_server_app::App;
use ohl_session::rights::{RightsCommand as R, Scope, Subject};
use ohl_store_pg::Network;
use ohl_store_pg::import::{ImportOptions, import};
use ohl_store_pg::testing::Cluster;

/// A network: its cluster (kept while the test runs), its connection
/// string, its lines, a scratch folder.
pub struct TestNet {
    pub cluster: Cluster,
    pub url: String,
    pub codes: Vec<String>,
    pub dir: PathBuf,
}

/// A network of a synthetic country of `pylons`, one line per route:
/// `admin` its network admin, `bob` a designer of everything, `lena` the
/// line admin of the first line, `vic` a viewer. None without
/// PostgreSQL's binaries (said on stderr).
pub fn network(name: &str, pylons: usize) -> Option<TestNet> {
    let cluster = Cluster::start(name)?;
    cluster.create_db("net");
    let url = cluster.url("net");
    let dir = cluster.scratch();
    let country = ohl_synth::country(&ohl_synth::Settings {
        pylons,
        ..Default::default()
    })
    .unwrap();
    let files =
        ohl_synth::project::write_line_projects(&country, &dir.join("lines"), "admin").unwrap();
    let net = Network::connect_writer(&url).unwrap();
    let mut codes = Vec::new();
    for (code, path, _) in files {
        let mut o = ImportOptions::new(&code, "admin");
        o.revision = false;
        import(&net, &path, &o).unwrap();
        codes.push(code);
    }
    net.rights_command(
        "admin",
        &R::Bootstrap {
            user: "admin".into(),
        },
    )
    .unwrap();
    for (u, role, scope) in [
        ("bob", "designer", Scope::network()),
        ("lena", "line_admin", Scope::line(&codes[0])),
        ("vic", "viewer", Scope::network()),
    ] {
        net.rights_command(
            "admin",
            &R::AddUser {
                name: u.into(),
                display: None,
            },
        )
        .unwrap();
        net.rights_command(
            "admin",
            &R::Assign {
                subject: Subject::User(u.into()),
                role: Some(role.into()),
                deny: false,
                scope,
                note: String::new(),
            },
        )
        .unwrap();
    }
    net.rights_command(
        "admin",
        &R::AddGroup {
            name: "east".into(),
            note: String::new(),
        },
    )
    .unwrap();
    drop(net);
    cluster.wait_for_locks_gone("net");
    Some(TestNet {
        cluster,
        url,
        codes,
        dir,
    })
}

/// The app in a test window (rendered on the GPU when there is one).
pub fn harness(app: App) -> Harness<'static, App> {
    ohl_server_app::help::DRY.store(true, std::sync::atomic::Ordering::SeqCst);
    let gpu = ohl_render::headless_device().is_ok();
    let mut builder = Harness::builder().with_size(egui::vec2(1280.0, 860.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut app = Some(app);
    let mut h = builder.build_eframe(move |_| app.take().expect("built once"));
    h.run_steps(2);
    h
}

/// Runs frames until `done` holds (60 s at most).
pub fn until(h: &mut Harness<'static, App>, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(h.state()) {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "waited 60 s for {what}; said: {:?}, busy {:?}, closed {:?}, error: {:?}",
            h.state().conn.as_ref().map(|c| c.data.said.clone()),
            h.state().conn.as_ref().map(|c| c.data.busy.clone()),
            h.state().conn.as_ref().map(|c| c.closed.clone()),
            h.state().error
        );
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(15));
    }
    h.run_steps(3);
}

/// Clicks the widget labelled `label`.
pub fn click(h: &mut Harness<'static, App>, label: &str) {
    h.query_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("no {label}"))
        .click();
    h.run_steps(3);
}

/// Whether something labelled `label` is shown.
pub fn shown(h: &Harness<'static, App>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

/// Whether some text holding `part` is shown.
pub fn shown_part(h: &Harness<'static, App>, part: &str) -> bool {
    h.query_all_by_label_contains(part).next().is_some()
}

/// Saves the window as `name` in `OHL_RENDER_PNG` (a GPU rendering; the
/// user guide's pictures, made as USER=tester on the synthetic country).
pub fn shot(h: &mut Harness<'static, App>, name: &str) {
    if ohl_render::headless_device().is_ok()
        && let Some(dir) = std::env::var_os("OHL_RENDER_PNG")
    {
        h.run_steps(2);
        let image = h.render().unwrap();
        image.save(Path::new(&dir).join(name)).unwrap();
    }
}
