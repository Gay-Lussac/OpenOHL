// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The sign-in window's side of the protocol (2026-11-20), on a
//! throwaway PostgreSQL cluster and a fake identity provider on the
//! loopback ([`crate::fake_provider`]): what a server takes asked before
//! joining; signing in with the organisation (the authorization code
//! flow with PKCE and a loopback redirect), joining with the ID token,
//! renewing it; a sign-in refused, cancelled, answered for another
//! sign-in; invitations saying their end.

use std::sync::Arc;
use std::time::Duration;

use ohl_store_pg::Network;
use ohl_store_pg::rights::RightsCommand as R;

use crate::client::{NetJoin, join_network};
use crate::fake_provider::FakeProvider;
use crate::network::NetOptions;
use crate::network_tests::{local, net_options, test_network};
use crate::oidc::{Oidc, OidcConfig};
use crate::server::Server;
use crate::sign_in::{Flow, OidcOffer, SIGN_IN_TIMEOUT, invitation_end, options, refresh};

#[test]
fn signing_in_with_the_organisation() {
    let Some(t) = test_network("sign-in", 40) else {
        return;
    };
    let idp = FakeProvider::start("00u-bob");
    Network::connect(&t.url)
        .unwrap()
        .rights_command(
            "admin",
            &R::MapIdentity {
                issuer: idp.issuer.clone(),
                subject: "00u-bob".into(),
                user: "bob".into(),
            },
        )
        .unwrap();
    let oidc = Arc::new(Oidc::new(OidcConfig::new(&idp.issuer, "ohl-design")).unwrap());
    let server = Server::start_network(
        local("abcd2345"),
        NetOptions {
            oidc: Some(oidc),
            invite_days: Some(90),
            ..net_options(&t.url)
        },
    )
    .unwrap();
    // What it takes: its provider and client id.
    let offer = options(&server.invitation).unwrap().oidc.unwrap();
    assert_eq!(
        offer,
        OidcOffer {
            issuer: idp.issuer.clone(),
            client_id: "ohl-design".into()
        }
    );
    // Signed in in the "browser", the code exchanged on the loopback.
    let flow = Flow::start(&offer).unwrap();
    assert!(flow.url().contains("code_challenge_method=S256"));
    assert!(
        flow.url()
            .contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A")
    );
    let url = flow.url().to_owned();
    let browser = std::thread::spawn(move || FakeProvider::browse(&url));
    let signed = flow.finish(SIGN_IN_TIMEOUT, &|| false).unwrap();
    assert!(browser.join().unwrap().unwrap().contains("close this tab"));
    assert_eq!(signed.subject(), "00u-bob");
    assert!(signed.refreshable() && !signed.stale());
    assert!(
        !format!("{signed:?}").contains(signed.id_token()),
        "no token in Debug"
    );
    // Joined with it (the invitation's address and fingerprint; no code).
    let mut bare = server.invitation.clone();
    bare.code = "0".into();
    let with = |token: &str| NetJoin {
        id_token: Some(token.to_owned()),
        ..NetJoin::default()
    };
    let bob = join_network(&bare, "bob", &with(signed.id_token()), &mut |_| {}).unwrap();
    drop(bob);
    // Renewed without the browser, joined again.
    let again = refresh(&signed).unwrap();
    assert_eq!(again.subject(), "00u-bob");
    // Its first connection gone (one a user), waited for, not slept.
    let t0 = std::time::Instant::now();
    while server.peers().iter().any(|p| p.user == "bob") {
        assert!(t0.elapsed() < crate::tests::WAIT);
        std::thread::sleep(Duration::from_millis(20));
    }
    let bob = join_network(&bare, "bob", &with(again.id_token()), &mut |_| {}).unwrap();
    drop(bob);
    // The user refuses on the provider's page: said.
    idp.deny(true);
    let flow = Flow::start(&offer).unwrap();
    let url = flow.url().to_owned();
    let browser = std::thread::spawn(move || FakeProvider::browse(&url));
    let e = flow.finish(SIGN_IN_TIMEOUT, &|| false).unwrap_err();
    let _ = browser.join();
    assert!(e.contains("refused: not allowed"), "{e}");
    idp.deny(false);
    // An answer of another sign-in (its state): refused.
    let flow = Flow::start(&offer).unwrap();
    let forged = flow
        .url()
        .split("redirect_uri=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .replace("%3A", ":")
        .replace("%2F", "/");
    let browser = std::thread::spawn(move || {
        ureq::get(&format!("{forged}?code=x&state=another"))
            .call()
            .map(|_| ())
    });
    let e = flow.finish(SIGN_IN_TIMEOUT, &|| false).unwrap_err();
    let _ = browser.join();
    assert!(e.contains("not this sign-in's"), "{e}");
    // Cancelled (the window's Cancel or Escape): stops at once.
    let flow = Flow::start(&offer).unwrap();
    let e = flow.finish(Duration::from_secs(60), &|| true).unwrap_err();
    assert!(e.contains("cancelled"), "{e}");
    // A personal invitation of this server says its end (90 days).
    let code = server.invitation_for("bob").code;
    let end = invitation_end(&code).expect("an end");
    assert_eq!(end, crate::utc_day(90));
    server.stop();
    // A server without OIDC: invitations only.
    let plain = Server::start_network(local("abcd2345"), net_options(&t.url)).unwrap();
    assert_eq!(options(&plain.invitation).unwrap().oidc, None);
    plain.stop();
}
