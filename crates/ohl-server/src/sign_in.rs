// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Signing in to a network, the client's side (2026-11-20: the sign-in
//! window of OHL-Design; ADR-0013, phase 13's OIDC option).
//!
//! - **What the server takes** ([`options`]): asked before joining with
//!   [`ClientMsg::SignInOptions`] — personal invitations always, and
//!   "sign in with your organisation" when the server checks OIDC tokens
//!   (its provider's issuer and the client id the tokens are for). A
//!   server of before closes the connection unanswered: invitations only.
//! - **An invitation's end** ([`invitation_end`]): a personal access code
//!   of phase 13 carries the last day it is valid (`G.UNTIL.MAC`), so the
//!   window says it before connecting; the server refuses it after that
//!   day, and a reissued one (an older generation) whatever its day.
//! - **Signing in with the organisation** ([`Flow`]): the authorization
//!   code flow with PKCE (RFC 7636, S256) and a loopback redirect (RFC
//!   8252: `http://127.0.0.1:PORT/callback`, a port chosen for each
//!   sign-in, listening on this computer only). The provider's endpoints
//!   come from its discovery document (HTTPS only; plain HTTP only for a
//!   provider on this computer — tests); the user signs in in their own
//!   browser, so OHL-Design never sees a password; the `state` and the
//!   `nonce` are checked; the code is exchanged for an ID token, which
//!   the join carries ([`crate::client::NetJoin::id_token`]) and the
//!   server checks (signature, issuer, audience, times). **Tokens are kept
//!   in memory only** (no keychain is used in the code base): never
//!   written, never logged, gone when OHL-Design closes. A refresh token,
//!   when the provider gives one, gets a new ID token for joining again
//!   ([`refresh`]).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ClientMsg, Invitation, PROTOCOL, ServerMsg};

/// How long a sign-in in the browser may take.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// What a server takes to join (its answer to
/// [`ClientMsg::SignInOptions`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInOptions {
    /// Sign-in with the organisation's identity provider (OIDC), when the
    /// server offers it. Personal invitations are always taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oidc: Option<OidcOffer>,
}

/// A server's OIDC sign-in: its provider and the client id its tokens are
/// for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcOffer {
    /// The provider's issuer (its discovery document is
    /// `ISSUER/.well-known/openid-configuration`).
    pub issuer: String,
    /// The client id (the tokens' audience).
    pub client_id: String,
}

impl OidcOffer {
    /// The provider's host, to name it in a window.
    pub fn host(&self) -> &str {
        let rest = self
            .issuer
            .split_once("://")
            .map_or(self.issuer.as_str(), |(_, r)| r);
        rest.split('/').next().unwrap_or(rest)
    }
}

/// What the server of `invitation` takes to join (asked before joining;
/// 20 s to connect, 10 s for the answer). A server of before 2026-11-20
/// answers nothing: invitations only.
pub fn options(invitation: &Invitation) -> Result<SignInOptions, String> {
    match crate::client::ask_once(invitation, &ClientMsg::SignInOptions { protocol: PROTOCOL })? {
        Some(ServerMsg::SignIn(o)) => Ok(o),
        Some(ServerMsg::Refused { reason }) => Err(format!("refused: {reason}")),
        Some(other) => Err(format!("unexpected answer {other:?}")),
        None => Ok(SignInOptions::default()),
    }
}

/// The last day personal access code `code` is valid (UTC, `YYYYMMDD`), as
/// the code itself says (phase 13: `G.UNTIL.MAC`); none: the code names
/// no end (a code of before, a general one, or one valid until reissued).
pub fn invitation_end(code: &str) -> Option<u32> {
    let mut parts = code.splitn(3, '.');
    let (Some(g), Some(u), Some(mac)) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    g.parse::<i32>().ok()?;
    if mac.is_empty() {
        return None;
    }
    u.parse::<u32>().ok().filter(|d| *d != 0)
}

/// Day `day` (`YYYYMMDD`) as `YYYY-MM-DD`.
pub fn day_text(day: u32) -> String {
    format!("{}-{:02}-{:02}", day / 10_000, day / 100 % 100, day % 100)
}

/// Today (UTC, `YYYYMMDD`).
pub fn today() -> u32 {
    crate::utc_day(0)
}

/// A user signed in with the organisation: the ID token a join carries,
/// kept in memory only (see the module's notes). Its `Debug` names no
/// token.
#[derive(Clone)]
pub struct SignedIn {
    issuer: String,
    client_id: String,
    token_endpoint: String,
    id_token: String,
    refresh_token: Option<String>,
    subject: String,
    expires: Option<u64>,
    got: Instant,
}

impl std::fmt::Debug for SignedIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignedIn")
            .field("issuer", &self.issuer)
            .field("subject", &self.subject)
            .field("expires", &self.expires)
            .field("refreshable", &self.refresh_token.is_some())
            .finish_non_exhaustive()
    }
}

impl SignedIn {
    /// The ID token (for the join only).
    pub fn id_token(&self) -> &str {
        &self.id_token
    }

    /// Who the provider says signed in (`sub`).
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The provider.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// When the ID token expires (Unix seconds), if it says.
    pub fn expires(&self) -> Option<u64> {
        self.expires
    }

    /// Whether a new ID token can be got without the browser.
    pub fn refreshable(&self) -> bool {
        self.refresh_token.is_some()
    }

    /// How long ago the ID token was got.
    pub fn age(&self) -> Duration {
        self.got.elapsed()
    }

    /// Whether the ID token is too old to join again with: expired, or
    /// got more than 50 minutes ago (a server takes tokens issued within
    /// the hour).
    pub fn stale(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.expires.is_some_and(|e| e <= now + 30) || self.age() > Duration::from_secs(50 * 60)
    }
}

/// The provider's endpoints (its discovery document).
#[derive(Debug, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
}

/// Whether `url` is plain HTTP to this computer.
pub(crate) fn loopback_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', '?']).next().unwrap_or_default();
    let host = match host.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    };
    host == "localhost"
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn check_url(what: &str, url: &str) -> Result<(), String> {
    if url.starts_with("https://") || loopback_url(url) {
        Ok(())
    } else {
        Err(format!(
            "{what} {url}: HTTPS only (plain HTTP only on this computer)"
        ))
    }
}

/// An HTTPS client to the provider (plain HTTP only to this computer), 10 s
/// and 1 MB at most, no redirect followed.
fn agent(issuer: &str) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .https_only(!loopback_url(issuer))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

fn read_json(
    url: &str,
    mut r: ureq::http::Response<ureq::Body>,
) -> Result<serde_json::Value, String> {
    let status = r.status();
    let v: serde_json::Value = r
        .body_mut()
        .with_config()
        .limit(1 << 20)
        .read_json()
        .map_err(|e| format!("{url}: {e}"))?;
    if !status.is_success() {
        let said = v
            .get("error_description")
            .or_else(|| v.get("error"))
            .and_then(|e| e.as_str())
            .unwrap_or("refused");
        return Err(format!("the sign-in provider refused: {said} ({status})"));
    }
    Ok(v)
}

fn discover(issuer: &str) -> Result<Discovery, String> {
    check_url("the sign-in provider", issuer)?;
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let r = agent(issuer)
        .get(&url)
        .call()
        .map_err(|e| format!("the sign-in provider ({url}): {e}"))?;
    let d: Discovery = serde_json::from_value(read_json(&url, r)?)
        .map_err(|e| format!("the sign-in provider's discovery document: {e}"))?;
    if d.issuer != issuer {
        return Err(format!(
            "the sign-in provider names itself {}, not {issuer}",
            d.issuer
        ));
    }
    check_url("the provider's sign-in page", &d.authorization_endpoint)?;
    check_url("the provider's token endpoint", &d.token_endpoint)?;
    Ok(d)
}

/// `text` percent-encoded (all but the unreserved signs).
fn enc(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `text` percent-decoded (`+` a space, as a form's).
fn dec(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut k = 0;
    while k < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match b[k] {
            b'%' if k + 2 < b.len() => match (hex(b[k + 1]), hex(b[k + 2])) {
                (Some(h), Some(l)) => {
                    out.push((h * 16 + l) as u8);
                    k += 3;
                    continue;
                }
                _ => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        k += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Base64url without padding.
fn b64url(data: &[u8]) -> String {
    crate::base64::encode(data)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

/// The claims of a JWT (not checked: the server checks the signature).
fn claims(token: &str) -> Result<serde_json::Value, String> {
    let part = token.split('.').nth(1).ok_or("not a token")?;
    let mut b64 = part.replace('-', "+").replace('_', "/");
    while !b64.len().is_multiple_of(4) {
        b64.push('=');
    }
    let bytes = crate::base64::decode(&b64)?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

/// The ID token of a token answer, its claims looked at: for `client_id`,
/// from `issuer`, with `nonce` when one was sent.
fn signed_in(
    v: &serde_json::Value,
    issuer: &str,
    client_id: &str,
    token_endpoint: &str,
    nonce: Option<&str>,
    refresh_before: Option<String>,
) -> Result<SignedIn, String> {
    let id_token = v
        .get("id_token")
        .and_then(|t| t.as_str())
        .ok_or("the sign-in provider gave no ID token (ask for the scope openid)")?
        .to_owned();
    if id_token.len() > 8 << 10 {
        return Err("the ID token is too long".into());
    }
    let c = claims(&id_token).map_err(|e| format!("the ID token: {e}"))?;
    if c.get("iss").and_then(|i| i.as_str()) != Some(issuer) {
        return Err("the ID token is of another issuer".into());
    }
    let aud_ok = match c.get("aud") {
        Some(serde_json::Value::String(a)) => a == client_id,
        Some(serde_json::Value::Array(a)) => a.iter().any(|x| x.as_str() == Some(client_id)),
        _ => false,
    };
    if !aud_ok {
        return Err("the ID token is for another application".into());
    }
    if let Some(n) = nonce
        && c.get("nonce").and_then(|x| x.as_str()) != Some(n)
    {
        return Err("the ID token is not the answer to this sign-in (nonce)".into());
    }
    let subject = c
        .get("sub")
        .and_then(|s| s.as_str())
        .ok_or("the ID token names no subject")?
        .to_owned();
    Ok(SignedIn {
        issuer: issuer.to_owned(),
        client_id: client_id.to_owned(),
        token_endpoint: token_endpoint.to_owned(),
        id_token,
        refresh_token: v
            .get("refresh_token")
            .and_then(|t| t.as_str())
            .map(str::to_owned)
            .or(refresh_before),
        subject,
        expires: c.get("exp").and_then(serde_json::Value::as_u64),
        got: Instant::now(),
    })
}

/// A sign-in with the organisation under way (see the module's notes):
/// [`Flow::url`] opened in the browser, then [`Flow::finish`] waits for
/// the provider's answer on the loopback.
pub struct Flow {
    listener: TcpListener,
    redirect: String,
    state: String,
    nonce: String,
    verifier: String,
    issuer: String,
    client_id: String,
    token_endpoint: String,
    url: String,
}

impl std::fmt::Debug for Flow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flow")
            .field("issuer", &self.issuer)
            .field("redirect", &self.redirect)
            .finish_non_exhaustive()
    }
}

impl Flow {
    /// Starts signing in with `offer`'s provider: its endpoints read, a
    /// loopback port opened, the sign-in page's address made (with the
    /// PKCE challenge, a state and a nonce).
    pub fn start(offer: &OidcOffer) -> Result<Self, String> {
        let d = discover(&offer.issuer)?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let redirect = format!("http://127.0.0.1:{port}/callback");
        let (state, nonce) = (crate::random_code(32), crate::random_code(32));
        let verifier = crate::random_code(64);
        let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
        let sep = if d.authorization_endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        let url = format!(
            "{}{sep}response_type=code&client_id={}&redirect_uri={}&scope=openid&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
            d.authorization_endpoint,
            enc(&offer.client_id),
            enc(&redirect),
            enc(&state),
            enc(&nonce),
            enc(&challenge)
        );
        Ok(Self {
            listener,
            redirect,
            state,
            nonce,
            verifier,
            issuer: offer.issuer.clone(),
            client_id: offer.client_id.clone(),
            token_endpoint: d.token_endpoint,
            url,
        })
    }

    /// The provider's sign-in page (to open in the browser).
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Waits for the provider's answer on the loopback (at most `timeout`,
    /// or until `cancelled` says so), then exchanges its code for an ID
    /// token. The browser's tab is told it may be closed.
    pub fn finish(
        self,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<SignedIn, String> {
        self.listener
            .set_nonblocking(true)
            .map_err(|e| e.to_string())?;
        let started = Instant::now();
        let query = loop {
            if cancelled() {
                return Err("the sign-in was cancelled".into());
            }
            if started.elapsed() > timeout {
                return Err(format!(
                    "no answer from the sign-in page in {} minutes: sign in again",
                    timeout.as_secs() / 60
                ));
            }
            let mut stream = match self.listener.accept() {
                Ok((s, _)) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            };
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut line = String::new();
            let mut r = BufReader::new((&stream).take(16 << 10));
            if r.read_line(&mut line).is_err() {
                continue;
            }
            // The headers read and dropped.
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).map_or(true, |n| n == 0) || h.trim().is_empty() {
                    break;
                }
            }
            let target = line.split(' ').nth(1).unwrap_or_default().to_owned();
            let (path, query) = target.split_once('?').unwrap_or((&target, ""));
            if path != "/callback" {
                let _ = write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                continue;
            }
            let page = "<!doctype html><meta charset=\"utf-8\"><title>OHL-Design</title>\
                        <p>Back to OHL-Design: you may close this tab.</p>";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: \
                 {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{page}",
                page.len()
            );
            break query.to_owned();
        };
        let param = |name: &str| -> Option<String> {
            query
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .find(|(k, _)| *k == name)
                .map(|(_, v)| dec(v))
        };
        if let Some(e) = param("error") {
            let said = param("error_description").unwrap_or(e);
            return Err(format!("the sign-in provider refused: {said}"));
        }
        if param("state").as_deref() != Some(self.state.as_str()) {
            return Err("the sign-in page's answer is not this sign-in's (state)".into());
        }
        let code = param("code").ok_or("the sign-in page gave no code")?;
        let r = agent(&self.issuer)
            .post(&self.token_endpoint)
            .send_form([
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", self.redirect.as_str()),
                ("client_id", self.client_id.as_str()),
                ("code_verifier", self.verifier.as_str()),
            ])
            .map_err(|e| format!("the sign-in provider ({}): {e}", self.token_endpoint))?;
        let v = read_json(&self.token_endpoint, r)?;
        signed_in(
            &v,
            &self.issuer,
            &self.client_id,
            &self.token_endpoint,
            Some(&self.nonce),
            None,
        )
    }
}

/// A new ID token for `s` with its refresh token (to join again after the
/// first one aged), without the browser.
pub fn refresh(s: &SignedIn) -> Result<SignedIn, String> {
    let token = s
        .refresh_token
        .as_deref()
        .ok_or("the sign-in cannot be renewed: sign in again")?;
    let r = agent(&s.issuer)
        .post(&s.token_endpoint)
        .send_form([
            ("grant_type", "refresh_token"),
            ("refresh_token", token),
            ("client_id", s.client_id.as_str()),
        ])
        .map_err(|e| format!("the sign-in provider ({}): {e}", s.token_endpoint))?;
    let v = read_json(&s.token_endpoint, r)?;
    signed_in(
        &v,
        &s.issuer,
        &s.client_id,
        &s.token_endpoint,
        None,
        s.refresh_token.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invitations_say_their_end() {
        assert_eq!(invitation_end("2.20261231.0123abcd"), Some(20261231));
        assert_eq!(invitation_end("1.0.0123abcd"), None, "until reissued");
        assert_eq!(invitation_end("abcd2345"), None, "a code of before");
        assert_eq!(invitation_end("x.20261231.ab"), None);
        assert_eq!(day_text(20261231), "2026-12-31");
        assert_eq!(
            OidcOffer {
                issuer: "https://id.example.org/realms/grid".into(),
                client_id: "ohl".into()
            }
            .host(),
            "id.example.org"
        );
    }

    #[test]
    fn urls_coded_and_decoded() {
        assert_eq!(
            enc("http://127.0.0.1:5/cb"),
            "http%3A%2F%2F127.0.0.1%3A5%2Fcb"
        );
        assert_eq!(dec("a%20b+c%2Fd"), "a b c/d");
        assert_eq!(dec("50%"), "50%");
        assert!(check_url("x", "http://id.example.org").is_err());
        assert!(check_url("x", "http://127.0.0.1:9/a").is_ok());
        assert!(check_url("x", "https://id.example.org").is_ok());
    }
}
