// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A fake identity provider on this computer, for tests and the user
//! guide's pictures only (no network beyond the loopback): plain HTTP on
//! 127.0.0.1, its discovery document, its keys (EC P-256), a sign-in page
//! that signs in a configured subject at once and redirects with a code,
//! a token endpoint checking the code, the redirect, the client id and
//! the PKCE verifier (S256), giving an ID token (with the nonce) and a
//! refresh token. [`FakeProvider::browse`] plays the user's browser.
//! The redirect may be a loopback port (OHL-Design) or the web viewer's
//! page on this computer (`https://127.0.0.1:PORT/`, 2026-11-21); the
//! origin given with [`FakeProvider::allow_origin`] is allowed (CORS) on
//! its discovery document, keys and token endpoint, as a provider must
//! for the web viewer.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

/// What a code was given for.
struct Given {
    client_id: String,
    redirect: String,
    challenge: String,
    nonce: Option<String>,
}

#[derive(Default)]
struct Book {
    codes: HashMap<String, Given>,
    refresh: HashMap<String, String>,
    subject: String,
    deny: bool,
    /// The origin allowed from a browser (CORS).
    origin: Option<String>,
}

/// The fake provider (see the module's notes).
pub struct FakeProvider {
    /// Its issuer (`http://127.0.0.1:PORT`).
    pub issuer: String,
    book: Arc<Mutex<Book>>,
}

fn b64url(data: &[u8]) -> String {
    crate::base64::encode(data)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

fn dec(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut k = 0;
    let hex = |c: u8| (c as char).to_digit(16);
    while k < b.len() {
        match b[k] {
            b'%' if k + 2 < b.len() => {
                if let (Some(h), Some(l)) = (hex(b[k + 1]), hex(b[k + 2])) {
                    out.push((h * 16 + l) as u8);
                    k += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        k += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn form(text: &str) -> HashMap<String, String> {
    text.split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (dec(k), dec(v)))
        .collect()
}

impl FakeProvider {
    /// Starts it; it signs `subject` in.
    pub fn start(subject: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let issuer = format!("http://{}", listener.local_addr().expect("an address"));
        let key = rcgen::KeyPair::generate().expect("a key");
        let raw = key.public_key_raw();
        let jwks = serde_json::json!({"keys": [{
            "kty": "EC", "crv": "P-256", "use": "sig", "alg": "ES256", "kid": "fake-1",
            "x": b64url(&raw[1..33]), "y": b64url(&raw[33..]),
        }]})
        .to_string();
        let book = Arc::new(Mutex::new(Book {
            subject: subject.to_owned(),
            ..Book::default()
        }));
        let (iss, b) = (issuer.clone(), book.clone());
        let der = key.serialize_der();
        std::thread::spawn(move || {
            let sign = |sub: &str, aud: &str, nonce: Option<&str>| -> String {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
                header.kid = Some("fake-1".into());
                let mut claims = serde_json::json!({
                    "iss": iss, "sub": sub, "aud": aud, "iat": now, "exp": now + 600,
                });
                if let Some(n) = nonce {
                    claims["nonce"] = n.into();
                }
                let key = jsonwebtoken::EncodingKey::from_ec_der(&der);
                jsonwebtoken::encode(&header, &claims, &key).expect("signed")
            };
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut r = BufReader::new(s.try_clone().expect("a stream"));
                let mut line = String::new();
                if r.read_line(&mut line).is_err() {
                    continue;
                }
                let mut length = 0usize;
                let mut origin = None;
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).is_err() || h.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    if let Some((name, v)) = h.split_once(':')
                        && name.trim().eq_ignore_ascii_case("origin")
                    {
                        origin = Some(v.trim().to_owned());
                    }
                }
                // CORS: the allowed origin only, as a provider does.
                let cors = match (&origin, &b.lock().expect("the book").origin) {
                    (Some(o), Some(allowed)) if o == allowed => {
                        format!("Access-Control-Allow-Origin: {o}\r\nVary: Origin\r\n")
                    }
                    _ => String::new(),
                };
                let mut body = vec![0u8; length.min(1 << 16)];
                let _ = r.read_exact(&mut body);
                let target = line.split(' ').nth(1).unwrap_or_default().to_owned();
                let (path, query) = target.split_once('?').unwrap_or((&target, ""));
                let json = |status: &str, v: serde_json::Value| {
                    let body = v.to_string();
                    format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{cors}Content-Length: \
                         {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                let answer = match path {
                    "/.well-known/openid-configuration" => json(
                        "200 OK",
                        serde_json::json!({
                            "issuer": iss,
                            "jwks_uri": format!("{iss}/keys"),
                            "authorization_endpoint": format!("{iss}/authorize"),
                            "token_endpoint": format!("{iss}/token"),
                        }),
                    ),
                    "/keys" => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{cors}Content-Length: \
                         {}\r\nConnection: close\r\n\r\n{jwks}",
                        jwks.len()
                    ),
                    "/authorize" => {
                        let q = form(query);
                        let redirect = q.get("redirect_uri").cloned().unwrap_or_default();
                        let state = q.get("state").cloned().unwrap_or_default();
                        let mut book = b.lock().expect("the book");
                        let ok = q.get("response_type").map(String::as_str) == Some("code")
                            && q.get("code_challenge_method").map(String::as_str) == Some("S256")
                            && (redirect.starts_with("http://127.0.0.1:")
                                || redirect.starts_with("https://127.0.0.1:"));
                        let location = if book.deny || !ok {
                            format!(
                                "{redirect}?error=access_denied&error_description=not+allowed&state={state}"
                            )
                        } else {
                            let code = crate::random_code(24);
                            book.codes.insert(
                                code.clone(),
                                Given {
                                    client_id: q.get("client_id").cloned().unwrap_or_default(),
                                    redirect: redirect.clone(),
                                    challenge: q.get("code_challenge").cloned().unwrap_or_default(),
                                    nonce: q.get("nonce").cloned(),
                                },
                            );
                            format!("{redirect}?code={code}&state={state}")
                        };
                        format!(
                            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: \
                             0\r\nConnection: close\r\n\r\n"
                        )
                    }
                    "/token" => {
                        let f = form(&String::from_utf8_lossy(&body));
                        let mut book = b.lock().expect("the book");
                        let subject = book.subject.clone();
                        match f.get("grant_type").map(String::as_str) {
                            Some("authorization_code") => {
                                let given = f.get("code").and_then(|c| book.codes.remove(c));
                                let verifier = f.get("code_verifier").cloned().unwrap_or_default();
                                match given {
                                    Some(g)
                                        if Some(&g.client_id) == f.get("client_id")
                                            && Some(&g.redirect) == f.get("redirect_uri")
                                            && b64url(&Sha256::digest(verifier.as_bytes()))
                                                == g.challenge =>
                                    {
                                        let refresh = crate::random_code(24);
                                        book.refresh.insert(refresh.clone(), g.client_id.clone());
                                        json(
                                            "200 OK",
                                            serde_json::json!({
                                                "id_token": sign(&subject, &g.client_id, g.nonce.as_deref()),
                                                "access_token": "fake",
                                                "token_type": "Bearer",
                                                "refresh_token": refresh,
                                                "expires_in": 600,
                                            }),
                                        )
                                    }
                                    _ => json(
                                        "400 Bad Request",
                                        serde_json::json!({"error": "invalid_grant"}),
                                    ),
                                }
                            }
                            Some("refresh_token") => {
                                match f.get("refresh_token").and_then(|r| book.refresh.get(r)) {
                                    Some(client) if Some(client) == f.get("client_id") => json(
                                        "200 OK",
                                        serde_json::json!({
                                            "id_token": sign(&subject, client, None),
                                            "token_type": "Bearer",
                                            "expires_in": 600,
                                        }),
                                    ),
                                    _ => json(
                                        "400 Bad Request",
                                        serde_json::json!({"error": "invalid_grant"}),
                                    ),
                                }
                            }
                            _ => json(
                                "400 Bad Request",
                                serde_json::json!({"error": "unsupported_grant_type"}),
                            ),
                        }
                    }
                    _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_owned(),
                };
                let _ = s.write_all(answer.as_bytes());
            }
        });
        Self { issuer, book }
    }

    /// Allows `origin` (a web page's, `https://127.0.0.1:PORT`) from a
    /// browser: CORS on its discovery document, keys and token endpoint.
    pub fn allow_origin(&self, origin: &str) {
        self.book.lock().expect("the book").origin = Some(origin.to_owned());
    }

    /// From now on the sign-in page refuses (the user denies).
    pub fn deny(&self, deny: bool) {
        self.book.lock().expect("the book").deny = deny;
    }

    /// Plays the browser on sign-in page `url`: its redirect followed to
    /// the loopback, whose page is read.
    pub fn browse(url: &str) -> Result<String, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build()
            .into();
        let r = agent.get(url).call().map_err(|e| e.to_string())?;
        let location = r
            .headers()
            .get("location")
            .and_then(|l| l.to_str().ok())
            .ok_or("no redirect")?
            .to_owned();
        agent
            .get(&location)
            .call()
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())
    }
}
