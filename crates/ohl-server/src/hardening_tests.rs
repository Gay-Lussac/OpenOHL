// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The fixes of the security review of phase 13 (2026-11-20,
//! `docs/development/server-security-review.md`): long work after the
//! commands, limited; IPv6 addresses counted by their /64; large
//! messages within one budget; personal invitations reissued and
//! expiring; on a network (PostgreSQL): invitations, OIDC sign-in, the
//! tiles' budget, lists and a check's reach bounded.

use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::server::{Common, Gate, Host, Job, run};
use crate::{CodeRefusal, Keys};

/// An authority doing nothing but noting the order of its work.
struct Fake {
    common: Common,
    waiting: VecDeque<&'static str>,
    log: Arc<Mutex<Vec<String>>>,
    /// A job sent while the first long work runs (it arrives meanwhile).
    late: Option<std_mpsc::Sender<Job>>,
}

impl Host for Fake {
    fn common(&mut self) -> &mut Common {
        &mut self.common
    }

    fn handle(&mut self, job: Job) {
        if let Job::Leave { user, .. } = job {
            self.log.lock().unwrap().push(user);
        }
    }

    fn deferred(&self) -> bool {
        !self.waiting.is_empty()
    }

    fn run_deferred(&mut self) {
        let name = self.waiting.pop_front().unwrap();
        if let Some(tx) = self.late.take() {
            tx.send(Job::Leave {
                client: 9,
                user: "late".into(),
            })
            .unwrap();
        }
        self.log.lock().unwrap().push(format!("heavy {name}"));
    }

    fn stop(&mut self) {}
}

/// S1: long work (a check, an export, a bundle) is done only when no
/// other job waits: the commands sent before it go first, and a command
/// arriving while one runs goes before the next.
#[test]
fn long_work_waits_behind_the_commands() {
    let (tx, rx) = std_mpsc::channel();
    let log = Arc::new(Mutex::new(Vec::new()));
    let host = Fake {
        common: Common::new(Keys::random(), None, tokio::sync::broadcast::channel(16).0),
        waiting: VecDeque::from(["check", "export"]),
        log: log.clone(),
        late: Some(tx.clone()),
    };
    for user in ["a", "b"] {
        tx.send(Job::Leave {
            client: 1,
            user: user.into(),
        })
        .unwrap();
    }
    let t = std::thread::spawn(move || run(host, rx));
    let started = Instant::now();
    while log.lock().unwrap().len() < 5 {
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    tx.send(Job::Stop).unwrap();
    t.join().unwrap();
    assert_eq!(
        *log.lock().unwrap(),
        ["a", "b", "heavy check", "late", "heavy export"]
    );
}

/// S1: one long work per user waiting, at most so many in all.
#[cfg(feature = "network")]
#[test]
fn long_work_queue_limits() {
    use crate::network::heavy_refusal;
    assert_eq!(heavy_refusal(&[], 4, "ana"), None);
    assert_eq!(heavy_refusal(&["ben"], 4, "ana"), None);
    let mine = heavy_refusal(&["ben", "ana"], 4, "ana").unwrap();
    assert!(mine.contains("one at a time"), "{mine}");
    let full = heavy_refusal(&["b", "c", "d", "e"], 4, "ana").unwrap();
    assert!(full.contains("busy with 4"), "{full}");
    assert!(heavy_refusal(&[], 0, "ana").unwrap().contains("busy"));
}

/// S6: an IPv6 host's whole /64 is one address for the limits; IPv4 and
/// IPv4 mapped into IPv6 alike.
#[test]
fn ipv6_counted_by_its_64() {
    use crate::server::gate_key;
    let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
    let b: IpAddr = "2001:db8:1:2:ffff:1:2:3".parse().unwrap();
    let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
    assert_eq!(gate_key(a), gate_key(b));
    assert_ne!(gate_key(a), gate_key(other));
    let v4: IpAddr = "192.0.2.7".parse().unwrap();
    let mapped: IpAddr = "::ffff:192.0.2.7".parse().unwrap();
    assert_eq!(gate_key(mapped), v4);
    let gate = Arc::new(Gate::new(2));
    let p1 = gate.enter(a).unwrap();
    let p2 = gate.enter(b).unwrap();
    assert!(gate.enter("2001:db8:1:2::77".parse().unwrap()).is_none());
    assert!(gate.enter(other).is_some());
    // A wrong code from one address of the /64 makes all of it wait.
    gate.joined(a, &Err(crate::server::WRONG_CODE.into()));
    let e = gate.may_join(b).unwrap_err();
    assert!(e.contains("wrong access codes"), "{e}");
    drop((p1, p2));
}

/// S5: large messages share one budget: one waits while others hold it,
/// and is refused after its wait; small ones never wait.
#[test]
fn large_messages_share_a_budget() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    rt.block_on(async {
        let budget = Arc::new(tokio::sync::Semaphore::new(3 * 1024));
        let wait = Duration::from_millis(50);
        let first = crate::take_budget(2 << 20, &budget, wait).await.unwrap();
        let e = crate::take_budget(2 << 20, &budget, wait)
            .await
            .unwrap_err();
        assert!(e.contains("too many large messages"), "{e}");
        drop(first);
        let again = crate::take_budget(2 << 20, &budget, wait).await;
        assert!(again.is_ok());
    });
    const { assert!(crate::LARGE_MESSAGE < crate::MAX_WEB_MESSAGE) };
    const { assert!(crate::MAX_WEB_MESSAGE < crate::MAX_MESSAGE) };
}

/// S7: personal codes of a generation and an end: the code of before
/// stays valid until reissued; a reissued generation refuses the older;
/// an end passed is said; nothing of it can be changed without the key.
#[test]
fn personal_codes_reissued_and_expiring() {
    assert_eq!(crate::ymd(0), 19_700_101);
    assert_eq!(crate::ymd(59), 19_700_301);
    assert_eq!(crate::ymd(10_957), 20_000_101);
    assert_eq!(crate::ymd(11_016), 20_000_229);
    assert_eq!(crate::ymd(-1), 19_691_231);
    let keys = Keys::new("general", [7; 32]);
    let today = 20_261_120;
    let old = keys.code(Some("bob"));
    assert_eq!(keys.personal_code("bob", 0, None), old);
    assert_eq!(keys.check_personal("bob", &old, 0, today), Ok(()));
    assert_eq!(
        keys.check_personal("bob", &old, 1, today),
        Err(CodeRefusal::Withdrawn)
    );
    let g1 = keys.personal_code("bob", 1, None);
    assert!(g1.starts_with("1.0."), "{g1}");
    assert_eq!(keys.check_personal("bob", &g1, 1, today), Ok(()));
    assert_eq!(
        keys.check_personal("bob", &g1, 2, today),
        Err(CodeRefusal::Withdrawn)
    );
    // Another user's code, a made-up one: wrong (nothing else said).
    assert_eq!(
        keys.check_personal("eve", &g1, 1, today),
        Err(CodeRefusal::Wrong)
    );
    assert_eq!(
        keys.check_personal("bob", "1.0.00000000000000000000000000000000", 1, today),
        Err(CodeRefusal::Wrong)
    );
    // An end: valid through that day, then expired.
    let until = keys.personal_code("bob", 1, Some(today));
    assert_eq!(keys.check_personal("bob", &until, 1, today), Ok(()));
    assert_eq!(
        keys.check_personal("bob", &until, 1, today + 1),
        Err(CodeRefusal::Expired(today))
    );
    // The end or the generation changed by hand: wrong.
    let forged = until.replacen(&today.to_string(), "20991231", 1);
    assert_eq!(
        keys.check_personal("bob", &forged, 1, today + 1),
        Err(CodeRefusal::Wrong)
    );
    let forged = g1.replacen("1.", "2.", 1);
    assert_eq!(
        keys.check_personal("bob", &forged, 2, today),
        Err(CodeRefusal::Wrong)
    );
    // The invitation's string carries it.
    let inv: crate::Invitation = format!("ohl://h:1/{until}/{}", "ab".repeat(32))
        .parse()
        .unwrap();
    assert_eq!(inv.code, until);
    assert!(crate::utc_day(1) > crate::utc_day(0));
}

#[cfg(feature = "network")]
mod network {
    use std::time::Duration;

    use ohl_store_pg::Network;
    use ohl_store_pg::rights::RightsCommand as R;

    use crate::client::{Incoming, NetJoin, join_network};
    use crate::network::NetOptions;
    use crate::network_tests::{local, net_options, structure_of, test_network, tiles_around};
    use crate::oidc::tests::FakeIssuer;
    use crate::oidc::{Oidc, OidcConfig};
    use crate::rights_tests::{pump, pump_for};
    use crate::server::Server;
    use crate::{ClientMsg, HeldTile, ScopedCheck, ServerMsg};

    fn admin(url: &str, c: R) -> String {
        Network::connect(url)
            .unwrap()
            .rights_command("admin", &c)
            .unwrap()
    }

    fn join(server: &Server, user: &str, how: NetJoin) -> Result<crate::client::Joined, String> {
        join_network(&server.invitation_for(user), user, &how, &mut |_| {})
    }

    /// S7 on a network: the server gives each user's invitation of their
    /// generation; a reissue lets the connected user go and refuses the
    /// older invitation; an invitation with an end expires; refusals in
    /// the audit.
    #[test]
    fn invitations_reissued_and_expiring_on_a_network() {
        let Some(t) = test_network("invite", 60) else {
            return;
        };
        let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
        let old = server.invitation_for("bob");
        assert_eq!(old.code, server.keys().code(Some("bob")));
        let bob = join(&server, "bob", NetJoin::default()).unwrap();
        admin(&t.url, R::ReissueInvitation { name: "bob".into() });
        // Let go within a heartbeat.
        let mut log = Vec::new();
        let got = pump(
            &bob,
            &mut log,
            |m| matches!(m, ServerMsg::Notice { text } if text.contains("join again")),
        );
        assert!(!got.is_empty());
        drop(bob);
        let wait = std::time::Instant::now();
        while server.peers().iter().any(|p| p.user == "bob") {
            assert!(wait.elapsed() < Duration::from_secs(20));
            std::thread::sleep(Duration::from_millis(50));
        }
        let e = join_network(&old, "bob", &NetJoin::default(), &mut |_| {})
            .err()
            .unwrap();
        assert!(e.contains("reissued"), "{e}");
        let new = server.invitation_for("bob");
        assert!(new.code.starts_with("1.0."), "{}", new.code);
        drop(join_network(&new, "bob", &NetJoin::default(), &mut |_| {}).unwrap());
        // Valid to the end of today: joins; one that ended yesterday: no.
        let wait = std::time::Instant::now();
        while server.peers().iter().any(|p| p.user == "bob") {
            assert!(wait.elapsed() < Duration::from_secs(20));
            std::thread::sleep(Duration::from_millis(50));
        }
        let today = server.invitation_valid("bob", Some(0)).unwrap();
        drop(join_network(&today, "bob", &NetJoin::default(), &mut |_| {}).unwrap());
        let mut ended = new.clone();
        ended.code = server
            .keys()
            .personal_code("bob", 1, Some(crate::utc_day(-1)));
        let e = join_network(&ended, "carol", &NetJoin::default(), &mut |_| {})
            .err()
            .unwrap();
        assert!(e.contains("wrong access code"), "another user's: {e}");
        let wait = std::time::Instant::now();
        while server.peers().iter().any(|p| p.user == "bob") {
            assert!(wait.elapsed() < Duration::from_secs(20));
            std::thread::sleep(Duration::from_millis(50));
        }
        let e = join_network(&ended, "bob", &NetJoin::default(), &mut |_| {})
            .err()
            .unwrap();
        assert!(e.contains("expired on"), "{e}");
        assert!(server.invitation_valid("nobody", None).is_err());
        let audit = Network::connect(&t.url).unwrap().audit_rows().unwrap();
        assert!(
            audit
                .iter()
                .any(|a| a[1] == "bob" && a[2] == "refused" && a[4].contains("reissued"))
        );
        server.stop();
    }

    /// OIDC sign-in on a network (a fake provider on this computer, its
    /// keys fetched over the loopback): a token of a subject a network
    /// admin mapped joins as that user, without an access code; an
    /// unmapped subject, another user's, an expired token refused; a
    /// server without OIDC refuses tokens.
    #[test]
    fn oidc_sign_in_on_a_network() {
        let Some(t) = test_network("oidc", 60) else {
            return;
        };
        let idp = FakeIssuer::start();
        admin(
            &t.url,
            R::MapIdentity {
                issuer: idp.issuer.clone(),
                subject: "00u-bob".into(),
                user: "bob".into(),
            },
        );
        let oidc = Arc::new(Oidc::new(OidcConfig::new(&idp.issuer, "ohl-server")).unwrap());
        let server = Server::start_network(
            local("abcd2345"),
            NetOptions {
                oidc: Some(oidc.clone()),
                ..net_options(&t.url)
            },
        )
        .unwrap();
        // The invitation's address and fingerprint, no code.
        let mut bare = server.invitation.clone();
        bare.code = "0".into();
        let with = |token: String| NetJoin {
            id_token: Some(token),
            ..NetJoin::default()
        };
        let bob = join_network(
            &bare,
            "bob",
            &with(idp.token("00u-bob", "ohl-server", 2, 300)),
            &mut |_| {},
        )
        .unwrap();
        drop(bob);
        for (user, token, why) in [
            (
                "carol",
                idp.token("00u-carol", "ohl-server", 2, 300),
                "not mapped",
            ),
            (
                "carol",
                idp.token("00u-bob", "ohl-server", 2, 300),
                "another user's",
            ),
            (
                "bob",
                idp.token("00u-bob", "ohl-server", 900, 300),
                "expired",
            ),
            (
                "bob",
                idp.token("00u-bob", "another-app", 2, 300),
                "another application",
            ),
        ] {
            let e = join_network(&bare, user, &with(token), &mut |_| {})
                .err()
                .unwrap();
            assert!(e.contains(why), "{user}: {e}");
        }
        assert_eq!(oidc.fetches(), 1, "the keys fetched once and kept");
        let audit = Network::connect(&t.url).unwrap().audit_rows().unwrap();
        assert!(audit.iter().any(|a| a[3].contains("00u-carol")));
        server.stop();
        // A server without OIDC: tokens refused.
        let plain = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
        let mut bare = plain.invitation.clone();
        bare.code = "0".into();
        let e = join_network(
            &bare,
            "bob",
            &with(idp.token("00u-bob", "ohl-server", 2, 300)),
            &mut |_| {},
        )
        .err()
        .unwrap();
        assert!(e.contains("no sign-in tokens"), "{e}");
        plain.stop();
    }

    /// S3 and S4: a client's new tiles within its budget (the rest not
    /// made, said), lists bounded (routes, picks), a check's parameters and
    /// reach bounded, long work refused when the queue is full.
    #[test]
    fn tiles_lists_checks_and_queue_bounded() {
        let Some(t) = test_network("bounds", 120) else {
            return;
        };
        let server = Server::start_network(
            local("abcd2345"),
            NetOptions {
                tiles_per_second: 0.01,
                tiles_burst: 3.0,
                max_heavy_queue: 0,
                ..net_options(&t.url)
            },
        )
        .unwrap();
        let (_, p) = structure_of(&t, &t.codes[0], 3);
        let tiles: Vec<HeldTile> = tiles_around(p, 2)
            .into_iter()
            .map(|tile| HeldTile { tile, rev: None })
            .collect();
        let vic = join_network(
            &server.invitation_for("vic"),
            "vic",
            &NetJoin {
                tiles,
                ..NetJoin::default()
            },
            &mut |_| {},
        )
        .unwrap();
        let mut log = Vec::new();
        let got = pump(
            &vic,
            &mut log,
            |m| matches!(m, ServerMsg::Notice { text } if text.contains("not made now")),
        );
        let made = got
            .iter()
            .filter(|m| matches!(m, ServerMsg::Tile(_)))
            .count();
        assert!(made <= 3, "{made} tiles made with a budget of 3");
        // Lists bounded.
        vic.link
            .send(ClientMsg::Interest {
                routes: (0..5000).map(|k| format!("r{k}")).collect(),
                tiles: Vec::new(),
            })
            .unwrap();
        pump(
            &vic,
            &mut log,
            |m| matches!(m, ServerMsg::Notice { text } if text.contains("at most 4096")),
        );
        vic.link
            .send(ClientMsg::Pick {
                id: 1,
                model: t.codes[0].clone(),
                picks: (0..300)
                    .map(|_| ohl_session::working_set::Pick::Route { route: "x".into() })
                    .collect(),
            })
            .unwrap();
        pump(
            &vic,
            &mut log,
            |m| matches!(m, ServerMsg::Notice { text } if text.contains("at most 256")),
        );
        // A check's parameters: refused before it waits.
        let ws = ohl_session::working_set::WorkingSetSpec {
            name: String::new(),
            model: t.codes[0].clone(),
            picks: vec![crate::scoped_tests::area(p, 300.0)],
        };
        for (check, why) in [
            (
                ScopedCheck::Fields {
                    case: "10 °C".into(),
                    distance: Some(1e9),
                    amps: None,
                    limit_ut: None,
                },
                "field distance",
            ),
            (
                ScopedCheck::Clearances {
                    case: "10 °C".into(),
                    rules: "#".repeat(300 << 10),
                },
                "clearance rules of",
            ),
            (
                ScopedCheck::StructureLoads {
                    structures: vec!["s".into(); 10_001],
                },
                "at most 10000",
            ),
            // Within the parameters, but the queue holds none.
            (
                ScopedCheck::StructureLoads {
                    structures: Vec::new(),
                },
                "busy",
            ),
        ] {
            vic.link
                .send(ClientMsg::Check {
                    id: 2,
                    working_set: Some(ws.clone()),
                    check,
                })
                .unwrap();
            pump(
                &vic,
                &mut log,
                |m| matches!(m, ServerMsg::Notice { text } if text.contains(why)),
            );
        }
        drop(vic);
        server.stop();
        // A check whose rules reach farther than 10 km: refused after its
        // reading was cut.
        let server = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
        let bob = join_network(
            &server.invitation_for("bob"),
            "bob",
            &NetJoin::default(),
            &mut |_| {},
        )
        .unwrap();
        let far = r#"
[[rule]]
name = "far"
target = "other_lines"
radial = "50000 m"
"#;
        bob.link
            .send(ClientMsg::Check {
                id: 3,
                working_set: Some(ws),
                check: ScopedCheck::Clearances {
                    case: "10 °C".into(),
                    rules: far.into(),
                },
            })
            .unwrap();
        let got = pump_for(&bob, &mut log, Duration::from_secs(120), |m| {
            matches!(m, ServerMsg::CheckResult { id: 3, .. })
                || matches!(m, ServerMsg::Notice { .. })
        });
        match got.last() {
            Some(ServerMsg::CheckResult { result: Err(e), .. }) => {
                assert!(e.contains("more than 10000 m"), "{e}")
            }
            other => panic!("{other:?}"),
        }
        drop(bob);
        server.stop();
        let _ = Incoming::Closed(String::new());
    }

    use std::sync::Arc;
}
