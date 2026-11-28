// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The admin messages (2026-11-28, OHL-Server stages A and B) on a
//! network's server, on a throwaway PostgreSQL cluster: a network admin
//! sees the server at a glance, its sessions, log, settings, backups (one
//! made and drilled), the map and the rights, changes them (audited, with
//! "what changes" before), invites a user and lets a connection go; a line
//! admin sees only their line's rights and routes and none of the server's
//! own pages; a viewer is refused (audited). Without PostgreSQL's binaries
//! the tests say so and pass.

use std::time::{Duration, Instant};

use ohl_session::rights::{RightsCommand, Scope, Subject};
use ohl_store_pg::Network;

use crate::admin::{AdminAsk, AdminReply};
use crate::client::{Incoming, Joined, NetJoin, join_network};
use crate::network_tests::{add_user, local, net_options, test_network};
use crate::server::Server;
use crate::{Invitation, ServerMsg};

/// Asks `ask` and waits for its answer (other messages dropped).
fn ask(j: &mut Joined, ask: AdminAsk) -> Result<AdminReply, String> {
    let id = j.link.admin(ask).unwrap();
    let started = Instant::now();
    loop {
        assert!(started.elapsed() < crate::tests::WAIT, "no answer to {id}");
        match j.link.recv_timeout(Duration::from_millis(50)) {
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

#[test]
fn a_network_admin_runs_the_server_from_the_admin_messages() {
    let Some(t) = test_network("admin-msgs", 240) else {
        return;
    };
    {
        let net = Network::connect(&t.url).unwrap();
        add_user(&net, "lena", "line_admin", Scope::line(&t.codes[0]));
    }
    let backups = t.dir.join("backups");
    std::fs::create_dir_all(&backups).unwrap();
    let mut o = net_options(&t.url);
    o.backup_dir = Some(backups.clone());
    let server = Server::start_network(local("admin-msgs"), o).unwrap();
    let mut admin = join(&server, "admin");

    // The Overview: everything, and no backup yet.
    let ov = reply!(AdminReply::Overview, ask(&mut admin, AdminAsk::Overview));
    assert!(ov.full);
    assert_eq!(ov.models, t.codes.len());
    assert!(ov.structures > 0 && ov.routes >= t.codes.len());
    assert_eq!(ov.protocol, crate::PROTOCOL);
    assert!(ov.connected.contains(&"admin".to_owned()));
    assert!(
        ov.warnings.iter().any(|w| w.contains("no backup")),
        "{:?}",
        ov.warnings
    );

    // The map and its dots.
    let map = reply!(AdminReply::Map, ask(&mut admin, AdminAsk::Map));
    let models: std::collections::BTreeSet<&String> = map.routes.iter().map(|r| &r.model).collect();
    assert_eq!(models.len(), t.codes.len());
    assert!(
        map.routes
            .iter()
            .all(|r| r.voltage_kv.is_some() && !r.lines.is_empty())
    );
    let b = map.bounds.unwrap();
    let dots = reply!(
        AdminReply::Structures,
        ask(&mut admin, AdminAsk::Structures { bounds: b })
    );
    assert_eq!(dots.len(), ov.structures);
    assert_eq!(
        dots.iter().filter(|d| d.shared).count(),
        map.shared.len(),
        "shared towers on the map and as dots"
    );
    let r0 = map.routes[0].clone();
    let along = reply!(
        AdminReply::Structures,
        ask(
            &mut admin,
            AdminAsk::RouteStructures {
                model: r0.model.clone(),
                route: r0.id.clone(),
            },
        )
    );
    assert_eq!(along.len(), r0.structures);

    // "What changes", then the change (audited), then an invitation.
    let assign = RightsCommand::Assign {
        subject: Subject::User("wendy".into()),
        role: Some("designer".into()),
        deny: false,
        scope: Scope::line(&t.codes[1]),
        note: "east".into(),
    };
    let w = reply!(
        AdminReply::Preview,
        ask(
            &mut admin,
            AdminAsk::Preview {
                command: assign.clone(),
            },
        )
    );
    assert_eq!((w.gaining, w.losing), (1, 0));
    assert_eq!(w.users[0].user, "wendy");
    let done = reply!(
        AdminReply::Done,
        ask(&mut admin, AdminAsk::Change { command: assign })
    );
    assert!(!done.is_empty());
    let inv = reply!(
        AdminReply::Invitation,
        ask(
            &mut admin,
            AdminAsk::Invite {
                name: "dora".into(),
                display: None,
                days: Some(7),
            },
        )
    );
    assert!(inv.until.is_some());
    let dora_inv = Invitation {
        code: inv.code.clone(),
        ..server.invitation.clone()
    };
    // Dora joins with it (no rights yet: she follows nothing).
    let dora = join_network(&dora_inv, "dora", &NetJoin::default(), &mut |_| {}).unwrap();

    // Sessions; Dora let go with a reason.
    let rows = reply!(AdminReply::Sessions, ask(&mut admin, AdminAsk::Sessions));
    let d = rows
        .iter()
        .find(|r| r.user == "dora")
        .expect("dora connected")
        .clone();
    let me = rows.iter().find(|r| r.user == "admin").unwrap().client;
    reply!(
        AdminReply::Done,
        ask(
            &mut admin,
            AdminAsk::Disconnect {
                client: d.client,
                reason: "maintenance".into(),
            },
        )
    );
    let started = Instant::now();
    let mut told = false;
    while started.elapsed() < Duration::from_secs(20) && !told {
        match dora.link.recv_timeout(Duration::from_millis(50)) {
            Some(Incoming::Msg(ServerMsg::Notice { text })) => {
                told = text.contains("maintenance");
            }
            Some(Incoming::Closed(_)) => break,
            _ => {}
        }
    }
    assert!(told, "dora is told why");
    // A reason is needed; one's own connection is not let go.
    let no_reason = AdminAsk::Disconnect {
        client: d.client,
        reason: " ".into(),
    };
    assert!(ask(&mut admin, no_reason).is_err());
    let own = AdminAsk::Disconnect {
        client: me,
        reason: "x".into(),
    };
    assert!(ask(&mut admin, own).is_err());

    // The log and the settings.
    let log = reply!(
        AdminReply::Log,
        ask(
            &mut admin,
            AdminAsk::Log {
                after: 0,
                filter: "changed the rights".into(),
            },
        )
    );
    assert!(!log.lines.is_empty());
    let settings = reply!(AdminReply::Settings, ask(&mut admin, AdminAsk::Settings));
    let db = settings.iter().find(|s| s.key == "database").unwrap();
    assert!(!db.value.contains("password"), "{}", db.value);
    assert!(settings.iter().any(|s| s.source == "network"));

    // A backup, listed, drilled (without a drill database: the dump
    // against its manifest); then the Overview's warning gone.
    if ohl_store_pg::backup::pg_tool("pg_dump").is_ok() {
        let b = reply!(AdminReply::Done, ask(&mut admin, AdminAsk::BackupNow));
        assert!(b.starts_with("backup "), "{b}");
        let list = reply!(AdminReply::Backups, ask(&mut admin, AdminAsk::Backups));
        assert_eq!(list.rows.len(), 1);
        assert!(list.folder && !list.drill_database && list.age_h.is_some());
        let drill = reply!(
            AdminReply::Done,
            ask(
                &mut admin,
                AdminAsk::RestoreDrill {
                    backup: list.rows[0].name.clone(),
                },
            )
        );
        assert!(drill.contains("matches its manifest"), "{drill}");
        let outside = AdminAsk::RestoreDrill {
            backup: "../elsewhere".into(),
        };
        assert!(ask(&mut admin, outside).is_err());
        let ov = reply!(AdminReply::Overview, ask(&mut admin, AdminAsk::Overview));
        assert!(
            !ov.warnings.iter().any(|w| w.contains("backup")),
            "{:?}",
            ov.warnings
        );
    }

    // A line admin: their line's rights and routes, none of the server's
    // own pages.
    let mut lena = join(&server, "lena");
    let rv = reply!(AdminReply::Rights, ask(&mut lena, AdminAsk::Rights));
    assert!(!rv.network_admin);
    assert_eq!(rv.manages_lines, vec![t.codes[0].clone()]);
    assert!(
        rv.assignments
            .iter()
            .all(|a| a.scope.line.as_deref() == Some(t.codes[0].as_str())),
        "{:?}",
        rv.assignments
    );
    let m = reply!(AdminReply::Map, ask(&mut lena, AdminAsk::Map));
    assert!(!m.routes.is_empty() && m.routes.iter().all(|r| r.model == t.codes[0]));
    assert!(ask(&mut lena, AdminAsk::Sessions).is_err());
    let lo = reply!(AdminReply::Overview, ask(&mut lena, AdminAsk::Overview));
    assert!(!lo.full && lo.connected.is_empty() && lo.models == 1);
    let p = reply!(
        AdminReply::Preview,
        ask(
            &mut lena,
            AdminAsk::Preview {
                command: RightsCommand::Assign {
                    subject: Subject::User("vic".into()),
                    role: Some("designer".into()),
                    deny: false,
                    scope: Scope::line(&t.codes[1]),
                    note: String::new(),
                },
            },
        )
    );
    assert!(p.notes.iter().any(|n| n.contains("will be refused")));
    // See as: what both Lena and Vic may (Vic sees the whole network,
    // Lena one line).
    let sa = reply!(
        AdminReply::SeeAs,
        ask(&mut lena, AdminAsk::SeeAs { user: "vic".into() })
    );
    assert_eq!(sa.lines.len(), 1);

    // A viewer is refused, and the refusal is in the audit.
    let mut vic = join(&server, "vic");
    let e = ask(&mut vic, AdminAsk::Overview).unwrap_err();
    assert!(e.contains("manages no rights"), "{e}");
    server.stop();
    let net = Network::connect(&t.url).unwrap();
    let audit = net.audit_rows().unwrap();
    for action in ["admin.disconnect", "admin.invite", "see_as", "refused"] {
        assert!(audit.iter().any(|r| r[2] == action), "{action} audited");
    }
}
