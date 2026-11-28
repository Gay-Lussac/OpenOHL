// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The admin messages of OHL-Server's stages C to F (2026-11-28) on a
//! network's server, on a throwaway PostgreSQL cluster: the shared towers
//! (who may edit one), twins; the Libraries page (a maintainer refuses
//! with a reason, grants, releases; Update library pins as a proposal,
//! then carried out); the Lines page (a line exported on the exports'
//! thread and fetched in pieces, uploaded in pieces and imported as a new
//! line, served at once with its twins listed); the audit paged and
//! filtered, the identities, "explain", a setting. A line admin and a
//! viewer are refused what is not theirs. Without PostgreSQL's binaries
//! the test says so and passes.

use std::time::{Duration, Instant};

use ohl_session::rights::{Permission, RightsCommand, Scope};
use ohl_store_pg::Network;
use ohl_store_pg::library::LibraryCommand as L;

use crate::admin::{AdminAsk, AdminReply, AuditFilter, CHUNK, LibraryAction};
use crate::client::{Incoming, Joined, NetJoin, join_network};
use crate::network_tests::{add_user, local, net_options, test_network};
use crate::server::Server;
use crate::ServerMsg;

/// Asks `ask` and waits for its answer (other messages dropped).
fn ask(j: &mut Joined, ask: AdminAsk) -> Result<AdminReply, String> {
    let id = j.link.admin(ask).unwrap();
    let started = Instant::now();
    loop {
        assert!(started.elapsed() < crate::tests::WAIT, "no answer to {id}");
        match j.link.recv_timeout(Duration::from_millis(20)) {
            Some(Incoming::Msg(ServerMsg::Admin { id: i, result })) if i == id => return result,
            Some(Incoming::Closed(why)) => panic!("closed: {why}"),
            _ => {}
        }
    }
}

fn join(server: &Server, user: &str) -> Joined {
    join_network(
        &server.invitation_for(user),
        user,
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap()
}

macro_rules! reply {
    ($pat:path, $e:expr) => {
        match $e {
            Ok($pat(x)) => x,
            other => panic!("{other:?}"),
        }
    };
}

fn cables() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../libraries/cables/acsr-examples.ohlw"),
    )
    .unwrap()
}

fn request(net: &Network, who: &str, id: &str) -> i64 {
    net.library_command(
        who,
        &L::Request {
            kind: "cables".into(),
            file: "acsr.ohlw".into(),
            text: format!("format = \"ohl-wire\"\n[[cable]]\nid = \"{id}\"\nname = \"N\"\n"),
            entry: None,
            reason: "a new wire".into(),
            line: None,
            branch: None,
            routes: Vec::new(),
            draft: false,
        },
    )
    .unwrap()
    .request
    .unwrap()
}

#[test]
fn stages_c_to_f_on_the_server() {
    let Some(t) = test_network("admin-cf", 240) else {
        return;
    };
    let (req1, req2) = {
        let net = Network::connect(&t.url).unwrap();
        add_user(&net, "lena", "line_admin", Scope::line(&t.codes[0]));
        add_user(
            &net,
            "mia",
            "library_maintainer",
            Scope {
                library_kind: Some("cables".into()),
                ..Scope::default()
            },
        );
        for c in [
            L::PutFile {
                kind: "cables".into(),
                file: "acsr.ohlw".into(),
                text: cables(),
                emergency: false,
            },
            L::Release {
                kind: "cables".into(),
                tag: "R1".into(),
                notes: String::new(),
            },
        ] {
            let who = if matches!(c, L::Release { .. }) {
                "mia"
            } else {
                "admin"
            };
            net.library_command(who, &c).unwrap();
        }
        (request(&net, "bob", "wire-one"), request(&net, "bob", "wire-two"))
    };
    let server = Server::start_network(local("admin-cf"), net_options(&t.url)).unwrap();
    let mut admin = join(&server, "admin");
    let mut mia = join(&server, "mia");
    let mut lena = join(&server, "lena");
    let mut vic = join(&server, "vic");

    // C: the shared towers, as the map marks them; who may edit one.
    let map = reply!(AdminReply::Map, ask(&mut admin, AdminAsk::Map));
    let shared = reply!(
        AdminReply::SharedTowers,
        ask(&mut admin, AdminAsk::SharedTowers)
    );
    assert_eq!(shared.total, map.shared.len());
    if let Some(r) = shared.rows.first() {
        assert!(r.routes.len() >= 2);
        let d = reply!(
            AdminReply::SharedTower,
            ask(
                &mut admin,
                AdminAsk::SharedTower {
                    model: r.model.clone(),
                    id: r.id.clone(),
                }
            )
        );
        let editors = d.editors.unwrap();
        assert!(editors.contains(&"bob".to_owned()), "{editors:?}");
        assert!(!editors.contains(&"vic".to_owned()), "{editors:?}");
    }
    let theirs = reply!(
        AdminReply::SharedTowers,
        ask(&mut lena, AdminAsk::SharedTowers)
    );
    assert!(theirs.rows.iter().all(|r| r.model == t.codes[0]));
    reply!(
        AdminReply::Twins,
        ask(
            &mut admin,
            AdminAsk::Twins {
                model: t.codes[0].clone(),
                tolerance: None,
            }
        )
    );
    assert!(
        ask(
            &mut lena,
            AdminAsk::Twins {
                model: t.codes[1].clone(),
                tolerance: None,
            }
        )
        .is_err()
    );

    // D: the Libraries page.
    let page = reply!(
        AdminReply::LibraryPage,
        ask(&mut admin, AdminAsk::LibraryPage)
    );
    let k = page.kinds.iter().find(|k| k.kind == "cables").unwrap();
    assert_eq!(k.releases, vec!["R1".to_owned()]);
    assert!(k.maintainers.contains(&"user:mia".to_owned()), "{k:?}");
    assert!(k.maintain && !k.files.is_empty());
    assert!(page.requests.iter().any(|r| r.id == req1 && r.state == "requested"));
    assert_eq!(page.lines.len(), t.codes.len());
    let theirs = reply!(
        AdminReply::LibraryPage,
        ask(&mut mia, AdminAsk::LibraryPage)
    );
    assert!(theirs.kinds.iter().any(|k| k.kind == "cables" && k.maintain));
    // A refusal needs a reason; a line admin grants nothing.
    let refuse = |note: &str| AdminAsk::Library {
        action: LibraryAction::Refuse {
            id: req1,
            note: note.into(),
        },
    };
    assert!(ask(&mut mia, refuse(" ")).is_err());
    reply!(AdminReply::Done, ask(&mut mia, refuse("not needed")));
    let grant = AdminAsk::Library {
        action: LibraryAction::Grant {
            id: req2,
            note: String::new(),
        },
    };
    assert!(ask(&mut lena, grant.clone()).is_err());
    reply!(AdminReply::Done, ask(&mut mia, grant));
    reply!(
        AdminReply::Done,
        ask(
            &mut mia,
            AdminAsk::Library {
                action: LibraryAction::Release {
                    kind: "cables".into(),
                    tag: "R2".into(),
                    notes: "wire-two".into(),
                },
            }
        )
    );
    reply!(
        AdminReply::Uses,
        ask(
            &mut admin,
            AdminAsk::Uses {
                kind: "cables".into(),
                entry: "wire-two".into(),
            }
        )
    );
    // Update library pins: a proposal, then carried out.
    let plan = AdminAsk::PinsPlan {
        line: t.codes[0].clone(),
        kinds: Vec::new(),
    };
    let p = reply!(AdminReply::Pins, ask(&mut admin, plan.clone()));
    assert!(p.moves && p.refusal.is_none(), "{p:?}");
    assert!(p.kinds.iter().any(|k| k.kind == "cables" && k.to == "R2"));
    assert!(ask(&mut vic, plan.clone()).is_err(), "a viewer opens no admin app");
    reply!(
        AdminReply::Done,
        ask(
            &mut admin,
            AdminAsk::UpdatePins {
                line: t.codes[0].clone(),
                kinds: Vec::new(),
            }
        )
    );
    let p = reply!(AdminReply::Pins, ask(&mut admin, plan));
    assert!(!p.moves, "{}", p.text);

    // E: the Lines page.
    let lines = reply!(AdminReply::Lines, ask(&mut admin, AdminAsk::Lines));
    assert_eq!(lines.len(), t.codes.len());
    assert!(lines.iter().all(|l| l.may_export && l.whole));
    let theirs = reply!(AdminReply::Lines, ask(&mut lena, AdminAsk::Lines));
    assert!(theirs.iter().all(|l| l.code == t.codes[0]));
    let export = |m: &str| AdminAsk::ExportLine { model: m.into() };
    assert!(ask(&mut lena, export(&t.codes[1])).is_err());
    let tr = reply!(AdminReply::Transfer, ask(&mut admin, export(&t.codes[1])));
    assert!(tr.bytes > 0 && tr.name.ends_with(".ohl"), "{tr:?}");
    let mut file = Vec::new();
    loop {
        let c = reply!(
            AdminReply::Chunk,
            ask(
                &mut admin,
                AdminAsk::Download {
                    token: tr.token.clone(),
                    offset: file.len() as u64,
                }
            )
        );
        assert_eq!(c.offset, file.len() as u64);
        file.extend(crate::base64::decode(&c.data).unwrap());
        if c.last {
            break;
        }
    }
    assert_eq!(file.len() as u64, tr.bytes);
    assert!(file.starts_with(b"SQLite format 3"));
    // Another connection may not fetch it.
    assert!(
        ask(
            &mut lena,
            AdminAsk::Download {
                token: tr.token.clone(),
                offset: 0,
            }
        )
        .is_err()
    );
    reply!(
        AdminReply::Done,
        ask(
            &mut admin,
            AdminAsk::TransferDone {
                token: tr.token.clone(),
            }
        )
    );
    // Uploaded in pieces (network admins only), imported as a new line.
    let start = |name: &str| AdminAsk::UploadStart {
        name: name.into(),
        bytes: file.len() as u64,
    };
    assert!(ask(&mut lena, start("copy.ohl")).is_err());
    assert!(ask(&mut admin, start("../copy.txt")).is_err());
    let up = reply!(AdminReply::Transfer, ask(&mut admin, start("../copy.ohl")));
    assert_eq!(up.name, "copy.ohl");
    let piece = |offset: usize| AdminAsk::Upload {
        token: up.token.clone(),
        offset: offset as u64,
        data: crate::base64::encode(&file[offset..(offset + CHUNK).min(file.len())]),
    };
    assert!(ask(&mut admin, piece(CHUNK.min(file.len() / 2))).is_err(), "in order");
    let mut sent = 0;
    while sent < file.len() {
        let got = reply!(AdminReply::Received, ask(&mut admin, piece(sent)));
        sent = (sent + CHUNK).min(file.len());
        assert_eq!(got, sent as u64);
    }
    let imported = reply!(
        AdminReply::Imported,
        ask(
            &mut admin,
            AdminAsk::ImportLine {
                token: up.token.clone(),
                code: "COPY1".into(),
                name: Some("a copy".into()),
                tolerance: None,
            }
        )
    );
    assert_eq!(imported.line, "COPY1");
    assert_eq!(imported.file, "copy.ohl");
    assert!(!imported.renamed.is_empty(), "its ids are another line's");
    assert!(!imported.twins.is_empty(), "every tower is a twin");
    // The file's users joined the line (the admin too): their connections
    // are let go, as after any change of their rights.
    drop(admin);
    let mut admin = join(&server, "admin");
    let lines = reply!(AdminReply::Lines, ask(&mut admin, AdminAsk::Lines));
    assert!(lines.iter().any(|l| l.code == "COPY1" && l.name == "a copy"));
    let map = reply!(AdminReply::Map, ask(&mut admin, AdminAsk::Map));
    assert!(map.routes.iter().any(|r| r.model == "COPY1"));

    // F: the audit, paged and filtered (network admins only).
    let audit = |f: AuditFilter, before: Option<i64>, limit: usize| AdminAsk::Audit {
        filter: f,
        before,
        limit,
    };
    let a = reply!(
        AdminReply::Audit,
        ask(&mut admin, audit(AuditFilter::default(), None, 5))
    );
    assert_eq!(a.rows.len(), 5);
    assert!(a.more && a.total > 5);
    assert!(a.rows.windows(2).all(|w| w[0].id > w[1].id), "newest first");
    let b = reply!(
        AdminReply::Audit,
        ask(
            &mut admin,
            audit(AuditFilter::default(), Some(a.rows[4].id), 5)
        )
    );
    assert!(b.rows.iter().all(|r| r.id < a.rows[4].id));
    let ours = reply!(
        AdminReply::Audit,
        ask(
            &mut admin,
            audit(
                AuditFilter {
                    action: "admin.".into(),
                    ..AuditFilter::default()
                },
                None,
                100
            )
        )
    );
    for action in [
        "admin.export_line",
        "admin.import_line",
        "admin.update_pins",
    ] {
        assert!(ours.rows.iter().any(|r| r.action == action), "{action}");
    }
    assert!(ours.rows.iter().all(|r| r.action.starts_with("admin.")));
    let hers = reply!(
        AdminReply::Audit,
        ask(
            &mut admin,
            audit(
                AuditFilter {
                    user: "mia".into(),
                    ..AuditFilter::default()
                },
                None,
                100
            )
        )
    );
    assert!(hers.rows.iter().all(|r| r.user == "mia") && !hers.rows.is_empty());
    let none = reply!(
        AdminReply::Audit,
        ask(
            &mut admin,
            audit(
                AuditFilter {
                    from: "2000-01-01".into(),
                    until: "2000-01-02".into(),
                    ..AuditFilter::default()
                },
                None,
                10
            )
        )
    );
    assert_eq!(none.total, 0);
    assert!(
        ask(
            &mut admin,
            audit(
                AuditFilter {
                    from: "yesterday".into(),
                    ..AuditFilter::default()
                },
                None,
                10
            )
        )
        .is_err()
    );
    assert!(ask(&mut lena, audit(AuditFilter::default(), None, 10)).is_err());
    let csv = crate::admin::audit_csv(&a.rows, true);
    assert_eq!(csv.lines().count(), 6);

    // The identities, "explain", a setting.
    reply!(
        AdminReply::Done,
        ask(
            &mut admin,
            AdminAsk::Change {
                command: RightsCommand::MapIdentity {
                    issuer: "https://id.example".into(),
                    subject: "s-1".into(),
                    user: "bob".into(),
                },
            }
        )
    );
    let ids = reply!(
        AdminReply::Identities,
        ask(&mut admin, AdminAsk::Identities)
    );
    assert!(ids.iter().any(|i| i.subject == "s-1" && i.user == "bob"));
    assert!(ask(&mut lena, AdminAsk::Identities).is_err());
    let explain = |who: &str| AdminAsk::Explain {
        user: who.into(),
        permission: Permission::Edit,
        line: Some(t.codes[0].clone()),
        kind: None,
        id: None,
    };
    let e = reply!(AdminReply::Explain, ask(&mut admin, explain("vic")));
    assert!(!e.may);
    let e = reply!(AdminReply::Explain, ask(&mut admin, explain("bob")));
    assert!(e.may && !e.granted_by.is_empty(), "{e:?}");
    let set = |k: &str, v: &str| AdminAsk::SetSetting {
        key: k.into(),
        value: v.into(),
    };
    reply!(
        AdminReply::Done,
        ask(&mut admin, set("backup_max_age_hours", "48"))
    );
    assert!(ask(&mut admin, set("backup_max_age_hours", "-1")).is_err());
    assert!(ask(&mut lena, set("backup_max_age_hours", "12")).is_err());
    drop((admin, mia, lena, vic));
    server.stop();
}
