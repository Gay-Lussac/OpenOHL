// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OHL-Server's pages of stages C to F against a network's server on a
//! throwaway PostgreSQL cluster (a synthetic country; without
//! PostgreSQL's binaries the tests say so and pass):
//!
//! - Shared towers: the list, who may edit one, a line's twins;
//! - the map: a region drawn (a crossing one refused) as an assignment's
//!   scope; the background asks nothing while its URL is empty, then one
//!   image of the view from the owner's own server (a local one here);
//! - Lines: one exported and fetched in pieces into a file, imported back
//!   as a new line with its report (the admin joined again after their own
//!   rights changed);
//! - Libraries: a refusal with its reason, Update library pins as a
//!   proposal then carried out;
//! - Audit: paged, exported to CSV; the identities; "explain"; a setting;
//! - a line admin sees no Audit page;
//! - (ignored, run by hand) the 12 000-pylon country: page open times, a
//!   line's export, an audit of 10 000 rows.
//!
//! With `OHL_RENDER_PNG` set, the user guide's pictures are made (as
//! USER=tester, on the synthetic country).

mod common;

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use common::{click, harness, network, shot, shown, shown_part, until};
use ohl_server::admin::{AuditFilter, LibraryAction};
use ohl_server::network::NetOptions;
use ohl_server::server::{Options, Server};
use ohl_server_app::App;
use ohl_server_app::app::{Page, Window};
use ohl_server_app::conn::JoinHow;
use ohl_session::rights::{Permission, RightsCommand, Scope, Subject};
use ohl_store_pg::Network;
use ohl_store_pg::library::LibraryCommand as L;

fn serve(url: &str) -> Server {
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
            ..NetOptions::default()
        },
    )
    .unwrap()
}

fn connected(server: &Server, user: &str) -> egui_kittest::Harness<'static, App> {
    let mut app = App::new();
    app.join(JoinHow {
        invitation: server.invitation_for(user),
        user: user.into(),
        id_token: None,
    });
    let mut h = harness(app);
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

fn conn<'a>(h: &'a mut egui_kittest::Harness<'static, App>) -> &'a mut ohl_server_app::conn::Conn {
    h.state_mut().conn.as_mut().expect("connected")
}

/// The country's libraries: a kind, released, a maintainer, two requests.
fn libraries(url: &str) -> (i64, i64) {
    let net = Network::connect(url).unwrap();
    net.rights_command(
        "admin",
        &RightsCommand::AddUser {
            name: "mia".into(),
            display: None,
        },
    )
    .unwrap();
    net.rights_command(
        "admin",
        &RightsCommand::Assign {
            subject: Subject::User("mia".into()),
            role: Some("library_maintainer".into()),
            deny: false,
            scope: Scope {
                library_kind: Some("cables".into()),
                ..Scope::default()
            },
            note: String::new(),
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/cables/acsr-examples.ohlw"),
    )
    .unwrap();
    net.library_command(
        "admin",
        &L::PutFile {
            kind: "cables".into(),
            file: "acsr.ohlw".into(),
            text,
            emergency: false,
        },
    )
    .unwrap();
    net.library_command(
        "mia",
        &L::Release {
            kind: "cables".into(),
            tag: "R1".into(),
            notes: String::new(),
        },
    )
    .unwrap();
    let request = |id: &str| {
        net.library_command(
            "bob",
            &L::Request {
                kind: "cables".into(),
                file: "acsr.ohlw".into(),
                text: format!("format = \"ohl-wire\"\n[[cable]]\nid = \"{id}\"\nname = \"N\"\n"),
                entry: None,
                reason: "a wire for the new line".into(),
                line: None,
                branch: None,
                routes: Vec::new(),
                draft: false,
            },
        )
        .unwrap()
        .request
        .unwrap()
    };
    (request("wire-one"), request("wire-two"))
}

/// A map server of this computer: answers every request with a picture
/// (a light grid) and keeps the request lines.
fn map_server() -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = seen.clone();
    let mut png = Vec::new();
    let img = image::RgbImage::from_fn(256, 256, |x, y| {
        if x % 32 == 0 || y % 32 == 0 {
            image::Rgb([200, 205, 215])
        } else {
            image::Rgb([236, 238, 242])
        }
    });
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    std::thread::spawn(move || {
        for s in listener.incoming() {
            let Ok(mut s) = s else { break };
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).to_string();
            kept.lock()
                .unwrap()
                .push(head.lines().next().unwrap_or_default().to_owned());
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: \
                 close\r\n\r\n",
                png.len()
            );
            let _ = s.write_all(&png);
        }
    });
    (
        format!(
            "http://127.0.0.1:{port}/wms?BBOX={{x0}},{{y0}},{{x1}},{{y1}}&WIDTH={{width}}&HEIGHT={{height}}&CRS={{crs}}"
        ),
        seen,
    )
}

#[test]
fn the_pages_of_stages_c_to_f() {
    let Some(t) = network("app-cf", 240) else {
        return;
    };
    let (req1, _req2) = libraries(&t.url);
    let server = serve(&t.url);
    let mut h = connected(&server, "admin");
    for p in Page::ALL {
        assert!(shown(&h, p.name()), "{}", p.name());
    }

    // Shared towers: the list, who may edit one, a line's twins.
    open_page(&mut h, Page::Shared);
    until(&mut h, "the shared towers", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.shared.is_some())
    });
    let first = conn(&mut h)
        .data
        .shared
        .as_ref()
        .unwrap()
        .rows
        .first()
        .cloned();
    if let Some(r) = &first {
        conn(&mut h).ask_shared_tower(&r.model, &r.id);
        until(&mut h, "who may edit", |a| {
            a.conn
                .as_ref()
                .is_some_and(|c| c.data.shared_tower.is_some())
        });
        assert!(shown_part(&h, "Who may edit it:"));
    }
    conn(&mut h).ask_twins(&t.codes[0], Some(0.5));
    until(&mut h, "the twins", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.twins.is_some())
    });

    // The map: no background request while its URL is empty.
    open_page(&mut h, Page::Map);
    until(&mut h, "the map", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.map.is_some())
    });
    h.state_mut().map.background.on = true;
    for _ in 0..40 {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(15));
    }
    assert_eq!(h.state().map.background_requests(), 0, "no URL, no request");
    // A region drawn: a crossing one refused, then one taken.
    let b = conn(&mut h).data.map.as_ref().unwrap().bounds.unwrap();
    let (cx, cy, w, hh) = (
        (b[0] + b[2]) / 2.0,
        (b[1] + b[3]) / 2.0,
        (b[2] - b[0]) / 4.0,
        (b[3] - b[1]) / 4.0,
    );
    h.state_mut().map.drawing = Some(vec![
        [cx - w, cy - hh],
        [cx + w, cy + hh],
        [cx + w, cy - hh],
        [cx - w, cy + hh],
    ]);
    h.run_steps(2);
    h.state_mut().finish_region();
    h.run_steps(3);
    assert!(
        h.state().map.drawing.is_some(),
        "a crossing region is kept to mend"
    );
    assert!(shown_part(&h, "edges cross"));
    h.state_mut().map.drawing = Some(vec![
        [cx - w, cy - hh],
        [cx + w, cy - hh],
        [cx + w * 0.2, cy],
        [cx + w, cy + hh],
        [cx - w, cy + hh],
    ]);
    h.run_steps(3);
    shot(&mut h, "ohl-server-draw-region.png");
    h.state_mut().finish_region();
    assert_eq!(h.state().page, Page::Assignments);
    assert_eq!(h.state().pages.scope.region.as_ref().map(Vec::len), Some(5));
    // The owner's own map server: one image of the view once it rests.
    let (url, seen) = map_server();
    open_page(&mut h, Page::Map);
    h.state_mut().map.background.url = url;
    until(&mut h, "the background", |a| a.map.background_shown());
    assert_eq!(h.state().map.background_requests(), 1);
    let line = seen.lock().unwrap()[0].clone();
    assert!(line.starts_with("GET /wms?BBOX="), "{line}");
    shot(&mut h, "ohl-server-background.png");
    h.state_mut().map.background = Default::default();

    // Lines: one exported into a file, imported back as a new line.
    open_page(&mut h, Page::Lines);
    until(&mut h, "the lines", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.lines.is_some())
    });
    shot(&mut h, "ohl-server-lines.png");
    let out = t.dir.join("exported.ohl");
    conn(&mut h).export_line(&t.codes[1], out.clone());
    until(&mut h, "the export fetched", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.download.is_none()) && out.is_file()
    });
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"SQLite format 3"));
    assert!(shown_part(&h, &format!("line {} written to", t.codes[1])));
    conn(&mut h).import_line(out.clone(), "COPY1", Some("a copy".into()), Some(0.5));
    until(&mut h, "the import's report", |a| {
        a.pages.imported.is_some()
    });
    // The file's users joined the line, the admin too: let go, joined
    // again at once.
    until(&mut h, "joined again", |a| {
        a.conn.as_ref().is_some_and(|c| {
            c.closed.is_none()
                && c.data
                    .lines
                    .as_ref()
                    .is_some_and(|l| l.len() == t.codes.len() + 1)
        })
    });
    assert_eq!(h.state().page, Page::Lines);
    assert!(shown_part(&h, "Line COPY1 imported from exported.ohl"));
    shot(&mut h, "ohl-server-import.png");
    // Its towers are the exported line's twins: shown, never merged.
    open_page(&mut h, Page::Shared);
    h.state_mut().pages.twins_line = "COPY1".into();
    conn(&mut h).ask_twins("COPY1", Some(0.5));
    until(&mut h, "the twins", |a| {
        a.conn
            .as_ref()
            .is_some_and(|c| c.data.twins.as_ref().is_some_and(|t| !t.1.is_empty()))
    });
    assert!(shown_part(&h, "Line COPY1:"));
    shot(&mut h, "ohl-server-shared.png");

    // Libraries: a refusal says why; Update library pins proposed first.
    open_page(&mut h, Page::Libraries);
    until(&mut h, "the libraries", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.library.is_some())
    });
    assert!(shown_part(&h, "user:mia"));
    shot(&mut h, "ohl-server-libraries.png");
    h.state_mut().window = Some(Window::Library(LibraryAction::Refuse {
        id: req1,
        note: String::new(),
    }));
    h.run_steps(3);
    shot(&mut h, "ohl-server-refuse.png");
    h.key_press(eframe::egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().window.is_none());
    h.state_mut().window = Some(Window::Pins(t.codes[0].clone()));
    until(&mut h, "the proposal", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.pins.is_some())
    });
    assert!(shown_part(&h, "cables"));
    shot(&mut h, "ohl-server-pins.png");
    click(&mut h, "Update the pins");
    until(&mut h, "the pins updated", |a| {
        a.conn.as_ref().is_some_and(|c| {
            c.data
                .said
                .iter()
                .any(|(s, e)| !e && s.contains("library pins"))
        })
    });

    // Audit: paged, to CSV.
    open_page(&mut h, Page::Audit);
    until(&mut h, "the audit", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.audit.is_some())
    });
    assert!(shown_part(&h, "row(s) match"));
    shot(&mut h, "ohl-server-audit.png");
    let csv = t.dir.join("audit.csv");
    conn(&mut h).export_audit(&AuditFilter::default(), csv.clone());
    until(&mut h, "the CSV written", |a| {
        a.conn
            .as_ref()
            .is_some_and(|c| c.data.audit_export.is_none())
    });
    let total = conn(&mut h).data.audit.as_ref().unwrap().total;
    let lines = std::fs::read_to_string(&csv).unwrap().lines().count() as u64;
    assert!(lines > total, "a header and every row: {lines} for {total}");

    // The identities, "explain", a setting.
    open_page(&mut h, Page::Users);
    h.state_mut().window = Some(Window::Change(RightsCommand::MapIdentity {
        issuer: "https://id.example".into(),
        subject: "subject-1".into(),
        user: "bob".into(),
    }));
    until(&mut h, "what changes", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.preview.is_some())
    });
    click(&mut h, "Save");
    until(&mut h, "the identity", |a| {
        a.conn.as_ref().is_some_and(|c| {
            c.data
                .identities
                .as_ref()
                .is_some_and(|i| i.iter().any(|i| i.subject == "subject-1"))
        })
    });
    open_page(&mut h, Page::SeeAs);
    h.state_mut().pages.see_as = "vic".into();
    conn(&mut h).ask_explain(
        "vic",
        Permission::Edit,
        Some(t.codes[0].clone()),
        None,
        None,
    );
    until(&mut h, "explain", |a| {
        a.conn.as_ref().is_some_and(|c| c.data.explain.is_some())
    });
    assert!(shown_part(&h, "vic may not edit on line"));
    shot(&mut h, "ohl-server-explain.png");
    conn(&mut h).set_setting("backup_max_age_hours", "48");
    until(&mut h, "the setting", |a| {
        a.conn.as_ref().is_some_and(|c| {
            c.data
                .said
                .iter()
                .any(|(s, e)| !e && s.contains("backup_max_age_hours = 48"))
        })
    });
    drop(h);

    // A line admin: no Audit page.
    let mut h = connected(&server, "lena");
    assert!(!shown(&h, "Audit"));
    assert!(shown(&h, "Lines") && shown(&h, "Libraries"));
    h.state_mut().page = Page::Audit;
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Overview);
    drop(h);
    server.stop();
}

/// The 12 000-pylon synthetic country: the Libraries, Lines and Audit
/// pages' open times, a line's export (written and fetched), an audit of
/// 10 000 rows (a page of 1000, and all of it to CSV). By hand:
/// `cargo test -p ohl-server-app --release --test stages_cf -- --ignored --nocapture`.
#[test]
#[ignore]
fn measure_stages_c_to_f_on_the_twelve_thousand_pylon_country() {
    let Some(t) = network("app-cf-12k", 12_000) else {
        return;
    };
    libraries(&t.url);
    {
        let net = Network::connect(&t.url).unwrap();
        let s = Instant::now();
        for i in 0..10_000 {
            net.audit(
                "admin",
                "measure",
                &format!("line L{:03}", i % 100),
                "a row",
            )
            .unwrap();
        }
        eprintln!(
            "12k: 10 000 audit rows written in {:.1} s",
            s.elapsed().as_secs_f64()
        );
    }
    let server = serve(&t.url);
    let mut h = connected(&server, "admin");
    let time = |h: &mut egui_kittest::Harness<'static, App>,
                page: Page,
                what: &str,
                ask: &dyn Fn(&mut ohl_server_app::conn::Conn),
                done: &dyn Fn(&App) -> bool| {
        open_page(h, page);
        ask(conn(h));
        let s = Instant::now();
        until(h, what, done);
        let ms = s.elapsed().as_secs_f64() * 1e3;
        eprintln!("12k: {what} in {ms:.0} ms");
        ms
    };
    time(
        &mut h,
        Page::Libraries,
        "the Libraries page",
        &|c| {
            c.data.library = None;
            c.ask_library_page();
        },
        &|a| a.conn.as_ref().is_some_and(|c| c.data.library.is_some()),
    );
    time(
        &mut h,
        Page::Lines,
        "the Lines page",
        &|c| {
            c.data.lines = None;
            c.ask_lines();
        },
        &|a| a.conn.as_ref().is_some_and(|c| c.data.lines.is_some()),
    );
    time(
        &mut h,
        Page::Shared,
        "the Shared towers page",
        &|c| {
            c.data.shared = None;
            c.ask_shared();
        },
        &|a| a.conn.as_ref().is_some_and(|c| c.data.shared.is_some()),
    );
    time(
        &mut h,
        Page::Audit,
        "the Audit page (200 rows of 10 000+)",
        &|c| {
            c.data.audit = None;
            c.ask_audit(&AuditFilter::default(), None, 200);
        },
        &|a| a.conn.as_ref().is_some_and(|c| c.data.audit.is_some()),
    );
    time(
        &mut h,
        Page::Audit,
        "an audit page of 1000 rows (filtered by scope)",
        &|c| {
            c.data.audit = None;
            c.ask_audit(
                &AuditFilter {
                    target: "line L00".into(),
                    ..AuditFilter::default()
                },
                None,
                1000,
            );
        },
        &|a| a.conn.as_ref().is_some_and(|c| c.data.audit.is_some()),
    );
    let csv = t.dir.join("audit.csv");
    let csv2 = csv.clone();
    time(
        &mut h,
        Page::Audit,
        "the whole audit to CSV",
        &move |c| c.export_audit(&AuditFilter::default(), csv2.clone()),
        &|a| {
            a.conn
                .as_ref()
                .is_some_and(|c| c.data.audit_export.is_none())
        },
    );
    eprintln!(
        "12k: CSV {} lines",
        std::fs::read_to_string(&csv).unwrap().lines().count()
    );
    let lines = conn(&mut h).data.lines.clone().unwrap();
    let big = lines.iter().max_by_key(|l| l.structures).unwrap().clone();
    let out = t.dir.join("big.ohl");
    let out2 = out.clone();
    let code = big.code.clone();
    time(
        &mut h,
        Page::Lines,
        &format!(
            "line {} ({} structures) exported and fetched",
            big.code, big.structures
        ),
        &move |c| c.export_line(&code, out2.clone()),
        &|a| a.conn.as_ref().is_some_and(|c| c.data.download.is_none()),
    );
    eprintln!(
        "12k: the export {:.1} MB",
        std::fs::metadata(&out).unwrap().len() as f64 / 1e6
    );
    drop(h);
    server.stop();
}
