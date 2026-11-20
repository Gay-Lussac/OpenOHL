// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Scoped checks on a network's server (phase 10), on a throwaway
//! PostgreSQL cluster holding a synthetic country (each route a line,
//! towers shared by two routes twins): a check on a route with a shared
//! tower pulls in the other line's wires; a user who may see only that
//! route gets the same numbers — the hidden neighbour computed on the
//! server — without a hidden id or place; the loads on the shared tower
//! carry the other line's sections; the field says what it leaves out;
//! a change of what a result used makes it stale, told once, named only
//! as the user may see it.

use std::time::Duration;

use ohl_session::rights::Scope;
use ohl_session::working_set::{Pick, WorkingSetSpec};
use ohl_store_pg::Network;

use crate::client::{NetJoin, join_network};
use crate::network_tests::{add_user, local, moved, net_options, test_network};
use crate::rights_tests::{hidden_for, no_leak, pump};
use crate::server::Server;
use crate::{ClientMsg, ScopedCheck, ScopedReport, ServerMsg};

pub(crate) const RULES: &str = r#"
[[rule]]
name = "other lines"
target = "other_lines"
vertical = { by_voltage = [[150, 5.0], [420, 7.0]] }
radial = { by_voltage = [[150, 4.0], [420, 6.5]] }
[[rule]]
name = "phase to phase"
target = "same_line"
radial = "3 m"
"#;

/// A line with a tower shared with another line: (its code, its route,
/// the other line, the twin there, the tower's place).
pub(crate) fn shared(url: &str, codes: &[String]) -> (String, String, String, String, [f64; 3]) {
    let net = Network::connect(url).unwrap();
    for code in codes {
        let twins = ohl_store_pg::import::twins(&net, code, 0.5).unwrap();
        if let Some(t) = twins.first() {
            let s =
                ohl_session::Session::open_on(Box::new(net.store(code).unwrap()), code).unwrap();
            let route = s.line().routes[0].id.clone();
            let p = s.line().structure(&t.structure).unwrap().position;
            return (
                code.clone(),
                route,
                t.other_line.clone(),
                t.other.clone(),
                p,
            );
        }
    }
    panic!("no shared tower")
}

/// A square area of half side `r` around `p`.
pub(crate) fn area(p: [f64; 3], r: f64) -> Pick {
    Pick::Area {
        polygon: vec![
            [p[0] - r, p[1] - r],
            [p[0] + r, p[1] - r],
            [p[0] + r, p[1] + r],
            [p[0] - r, p[1] + r],
        ],
    }
}

fn ask(
    j: &crate::client::Joined,
    log: &mut Vec<ServerMsg>,
    id: u64,
    spec: &WorkingSetSpec,
    check: ScopedCheck,
) -> Result<ScopedReport, String> {
    j.link
        .send(ClientMsg::Check {
            id,
            working_set: Some(spec.clone()),
            check,
        })
        .unwrap();
    let got = pump(j, log, |m| {
        matches!(m, ServerMsg::CheckResult { id: i, .. } if *i == id)
            || matches!(m, ServerMsg::Notice { .. })
    });
    match got.last() {
        Some(ServerMsg::CheckResult { result, .. }) => result.clone().map(|r| *r),
        other => Err(format!("no result: {other:?}")),
    }
}

#[test]
fn scoped_checks_on_the_server() {
    let Some(t) = test_network("scoped", 300) else {
        return;
    };
    let (code, route, other, twin, at) = shared(&t.url, &t.codes);
    add_user(
        &Network::connect(&t.url).unwrap(),
        "pat",
        "designer",
        Scope {
            route: Some(route.clone()),
            ..Scope::default()
        },
    );
    let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    // The sections within 400 m of the shared tower (snapped out to whole
    // tension sections).
    let spec = WorkingSetSpec {
        name: String::new(),
        model: code.clone(),
        picks: vec![area(at, 400.0)],
    };
    let clearances = ScopedCheck::Clearances {
        case: "10 °C".into(),
        rules: RULES.into(),
    };
    let mut log = Vec::new();
    let admin = join_network(
        &server.invitation_for("admin"),
        "admin",
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    let full = ask(&admin, &mut log, 1, &spec, clearances.clone()).unwrap();
    assert_eq!(full.status, "every part checked", "{full:?}");
    assert!(full.pulled.lines.contains(&other), "{:?}", full.pulled);
    assert_eq!(full.pulled.hidden, 0);
    assert!(full.violations > 0, "{full:?}");
    assert!(full.reach.starts_with("wires within"), "{}", full.reach);
    assert!(
        full.findings.iter().any(|f| f
            .against
            .as_deref()
            .is_some_and(|a| a.starts_with(&format!("line {other}:")))),
        "{:?}",
        full.findings
    );
    // Pat may see the route only: the same numbers, the other line hidden.
    let before = hidden_for(&t, "pat");
    let mut pat_log = Vec::new();
    let pat = join_network(
        &server.invitation_for("pat"),
        "pat",
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    let mine = ask(&pat, &mut pat_log, 2, &spec, clearances.clone()).unwrap();
    assert_eq!(mine.results, full.results);
    assert_eq!(mine.violations, full.violations);
    assert!(mine.pulled.hidden > 0, "{:?}", mine.pulled);
    assert!(!mine.pulled.lines.contains(&other));
    let hidden: Vec<_> = mine.findings.iter().filter(|f| f.hidden).collect();
    assert!(!hidden.is_empty());
    for f in &hidden {
        assert_eq!(f.at, None);
        assert_eq!(
            f.against.as_deref(),
            Some("a hidden object of another line")
        );
        if f.violates {
            assert!(
                f.on.contains("limited by an object you may not see"),
                "{f:?}"
            );
        }
    }
    // The loads on the shared tower: the other line's sections in them.
    let loads = ScopedCheck::StructureLoads {
        structures: Vec::new(),
    };
    let l = ask(&admin, &mut log, 3, &spec, loads.clone()).unwrap();
    assert!(l.results > 0, "{l:?}");
    assert!(
        l.notes.iter().any(|n| n.contains("twin(s) of other lines")),
        "{:?}",
        l.notes
    );
    assert!(
        l.pulled.objects.iter().any(|o| o.contains(&twin)),
        "{:?}",
        l.pulled
    );
    let lp = ask(&pat, &mut pat_log, 4, &spec, loads).unwrap();
    assert_eq!(lp.results, l.results);
    assert!(lp.pulled.hidden > 0);
    assert!(
        lp.notes.iter().any(|n| n.contains("you may not see")),
        "{:?}",
        lp.notes
    );
    // The field says what it leaves out.
    let f = ask(
        &pat,
        &mut pat_log,
        5,
        &spec,
        ScopedCheck::Fields {
            case: "40 °C".into(),
            distance: None,
            amps: Some(500.0),
            limit_ut: Some(1.0),
        },
    )
    .unwrap();
    assert!(
        f.notes
            .iter()
            .any(|n| n.starts_with("NOT ON THE SAFE SIDE")),
        "{:?}",
        f.notes
    );
    assert!(f.findings.iter().all(|x| x.measure == "B (µT)"));
    // Bob moves the twin: the results using it are stale, once.
    let bob = join_network(
        &server.invitation_for("bob"),
        "bob",
        &NetJoin::default(),
        &mut |_| {},
    )
    .unwrap();
    let net = Network::connect(&t.url).unwrap();
    let s = ohl_session::Session::open_on(Box::new(net.store(&other).unwrap()), &other).unwrap();
    let p = s.line().structure(&twin).unwrap().position;
    let mut bob_log = Vec::new();
    bob.link
        .send(ClientMsg::RequestOn {
            id: 9,
            model: other.clone(),
            op: moved(&twin, [p[0] + 0.2, p[1], p[2]]),
            line: "main".into(),
        })
        .unwrap();
    pump(&bob, &mut bob_log, |m| {
        matches!(m, ServerMsg::Response { id: 9, .. })
    });
    let got = pump(&admin, &mut log, |m| {
        matches!(m, ServerMsg::CheckStale { id: 1, .. })
    });
    let Some(ServerMsg::CheckStale { reason, .. }) = got.last() else {
        panic!()
    };
    assert!(reason.contains("changed (by bob)"), "{reason}");
    let got = pump(&pat, &mut pat_log, |m| {
        matches!(m, ServerMsg::CheckStale { id: 2, .. })
    });
    let Some(ServerMsg::CheckStale { reason, .. }) = got.last() else {
        panic!()
    };
    assert!(reason.contains("you may not see"), "{reason}");
    // Told once: a second move sends nothing more for these.
    bob.link
        .send(ClientMsg::RequestOn {
            id: 10,
            model: other.clone(),
            op: moved(&twin, p),
            line: "main".into(),
        })
        .unwrap();
    pump(&bob, &mut bob_log, |m| {
        matches!(m, ServerMsg::Response { id: 10, .. })
    });
    let until = std::time::Instant::now() + Duration::from_millis(400);
    let more = pump(&pat, &mut pat_log, |_| std::time::Instant::now() >= until);
    assert!(
        !more
            .iter()
            .any(|m| matches!(m, ServerMsg::CheckStale { id: 2, .. })),
        "{more:?}"
    );
    // Nothing pat was sent holds a hidden id or place.
    let after = hidden_for(&t, "pat");
    no_leak(&pat_log, &before, &after, "scoped checks");
    drop((admin, pat, bob));
    server.stop();
}
