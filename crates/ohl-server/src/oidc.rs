// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OIDC sign-in on a network (ADR-0013, phase 13; an option — personal
//! invitations stay the default).
//!
//! A client joins with an **ID token** of the owner's identity provider
//! (`ClientMsg::Join`'s `id_token`) instead of an access code. The server
//! checks it here, before the authority sees the join (off the authority
//! thread: fetching the provider's keys may take a moment):
//!
//! - **the signature**, with the provider's keys (its JWKS: from the
//!   discovery document `ISSUER/.well-known/openid-configuration`, or a
//!   URL or a file given), an asymmetric algorithm only (RS*, PS*, ES256,
//!   ES384, EdDSA; never HS* nor `none`), the key named by the token's
//!   `kid`, its algorithm matching the token's;
//! - **the issuer** (`iss`) the configured one, **the audience** (`aud`)
//!   holding the configured client id, **the times** (`exp` not passed,
//!   `nbf` reached, `iat` not in the future and not older than
//!   `max_age`, a minute of leeway), a subject (`sub`).
//!
//! The keys are kept (an hour), fetched again when a token names a key
//! not held (at most every 30 s: a stream of made-up `kid`s does not make
//! the server fetch). Fetched over HTTPS only (plain HTTP for an issuer on
//! this computer: tests), the provider's CA given or the Mozilla roots,
//! 1 MB at most, 10 s at most. The subject is mapped to a network user by
//! a network admin (`ohl network rights map-identity`, the `net_identity`
//! table): an unmapped subject is refused. A token is never logged nor
//! stored; the audit names the issuer and subject.
//!
//! Crates: `jsonwebtoken` (the token's checks, its pure-Rust crypto) and
//! `ureq` (HTTPS with rustls) — both widely used and maintained.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jsonwebtoken::jwk::{AlgorithmParameters, Jwk, JwkSet};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;

/// Longest ID token taken, bytes.
pub const MAX_TOKEN: usize = 8 << 10;

/// The keys are kept this long.
pub const KEYS_KEPT: Duration = Duration::from_secs(3600);

/// The keys are fetched again at most this often (a token naming a key
/// not held).
pub const REFETCH_EVERY: Duration = Duration::from_secs(30);

/// How the server checks ID tokens.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// The issuer, exactly as the provider names itself
    /// (`https://id.example.org/realms/grid`).
    pub issuer: String,
    /// The client id the tokens are for (`aud`).
    pub audience: String,
    /// The provider's keys' URL (default: its discovery document's
    /// `jwks_uri`).
    pub jwks_url: Option<String>,
    /// The provider's keys from a file instead (read again when it
    /// changes; no fetching).
    pub jwks_file: Option<PathBuf>,
    /// The provider's CA (PEM; default: the Mozilla roots).
    pub ca_file: Option<PathBuf>,
    /// The oldest `iat` taken.
    pub max_age: Duration,
    /// Leeway on the times, s.
    pub leeway: u64,
}

impl OidcConfig {
    /// For `issuer` and `audience`, the rest by default (an hour old at
    /// most, a minute of leeway).
    pub fn new(issuer: &str, audience: &str) -> Self {
        Self {
            issuer: issuer.to_owned(),
            audience: audience.to_owned(),
            jwks_url: None,
            jwks_file: None,
            ca_file: None,
            max_age: Duration::from_secs(3600),
            leeway: 60,
        }
    }
}

/// Who an ID token says signed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The issuer.
    pub issuer: String,
    /// The subject (`sub`).
    pub subject: String,
}

/// The keys as last read.
#[derive(Default)]
struct Keys {
    set: Option<JwkSet>,
    /// When read (for a file: its modification time too).
    read: Option<Instant>,
    file_time: Option<SystemTime>,
    /// The last fetch tried.
    tried: Option<Instant>,
    /// Fetches made (tests).
    fetches: usize,
}

/// The server's checker of ID tokens.
pub struct Oidc {
    config: OidcConfig,
    keys: Mutex<Keys>,
}

impl std::fmt::Debug for Oidc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Oidc")
            .field("issuer", &self.config.issuer)
            .field("audience", &self.config.audience)
            .finish_non_exhaustive()
    }
}

/// The algorithms taken: asymmetric ones only.
const ALGORITHMS: [Algorithm; 8] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
];

fn allowed(alg: Algorithm) -> bool {
    ALGORITHMS.contains(&alg) || alg == Algorithm::EdDSA
}

/// Whether `url` is on this computer (plain HTTP allowed: tests).
fn is_loopback_url(url: &str) -> bool {
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
    if url.starts_with("https://") || is_loopback_url(url) {
        Ok(())
    } else {
        Err(format!(
            "{what} {url}: HTTPS only (plain HTTP only on this computer)"
        ))
    }
}

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    jwks_uri: String,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    iat: Option<u64>,
}

impl Oidc {
    /// A checker for `config` (refused: no issuer or audience, an issuer
    /// or keys' URL not HTTPS).
    pub fn new(config: OidcConfig) -> Result<Self, String> {
        if config.issuer.trim().is_empty() || config.audience.trim().is_empty() {
            return Err("OIDC: give the issuer and the audience (the client id)".into());
        }
        check_url("the OIDC issuer", &config.issuer)?;
        if let Some(u) = &config.jwks_url {
            check_url("the OIDC keys' URL", u)?;
        }
        Ok(Self {
            config,
            keys: Mutex::new(Keys::default()),
        })
    }

    /// The issuer.
    pub fn issuer(&self) -> &str {
        &self.config.issuer
    }

    /// The client id the tokens are for (their audience).
    pub fn audience(&self) -> &str {
        &self.config.audience
    }

    /// Fetches made so far (the keys and the discovery document count as
    /// one; tests).
    pub fn fetches(&self) -> usize {
        self.lock().fetches
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Keys> {
        self.keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn agent(&self) -> Result<ureq::Agent, String> {
        use ureq::tls::{RootCerts, TlsConfig};
        let mut tls = TlsConfig::builder();
        if let Some(ca) = &self.config.ca_file {
            let pem = std::fs::read(ca).map_err(|e| format!("{}: {e}", ca.display()))?;
            let mut certs = Vec::new();
            for item in ureq::tls::parse_pem(&pem) {
                if let ureq::tls::PemItem::Certificate(c) =
                    item.map_err(|e| format!("{}: {e}", ca.display()))?
                {
                    certs.push(c.to_owned());
                }
            }
            if certs.is_empty() {
                return Err(format!("{}: no certificate", ca.display()));
            }
            tls = tls.root_certs(RootCerts::new_with_certs(&certs));
        }
        let loopback = is_loopback_url(&self.config.issuer);
        Ok(ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .https_only(!loopback)
            .max_redirects(0)
            .tls_config(tls.build())
            .build()
            .into())
    }

    fn get_json<T: serde::de::DeserializeOwned>(
        agent: &ureq::Agent,
        url: &str,
    ) -> Result<T, String> {
        agent
            .get(url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?
            .body_mut()
            .with_config()
            .limit(1 << 20)
            .read_json::<T>()
            .map_err(|e| format!("{url}: {e}"))
    }

    /// The provider's keys, read (a file) or fetched now.
    fn fetch(&self) -> Result<(JwkSet, Option<SystemTime>), String> {
        if let Some(file) = &self.config.jwks_file {
            let time = std::fs::metadata(file).and_then(|m| m.modified()).ok();
            let text =
                std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
            let set: JwkSet =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
            return Ok((set, time));
        }
        let agent = self.agent()?;
        let url = match &self.config.jwks_url {
            Some(u) => u.clone(),
            None => {
                let d: Discovery = Self::get_json(
                    &agent,
                    &format!(
                        "{}/.well-known/openid-configuration",
                        self.config.issuer.trim_end_matches('/')
                    ),
                )?;
                if d.issuer != self.config.issuer {
                    return Err(format!(
                        "the provider names itself {}, not {}",
                        d.issuer, self.config.issuer
                    ));
                }
                check_url("the OIDC keys' URL", &d.jwks_uri)?;
                d.jwks_uri
            }
        };
        Ok((Self::get_json(&agent, &url)?, None))
    }

    /// The key `kid` names (none: the only key), fetched again when not
    /// held or the keys are old (at most every [`REFETCH_EVERY`]).
    fn key(&self, kid: Option<&str>) -> Result<Jwk, String> {
        let find = |set: &JwkSet| -> Option<Jwk> {
            match kid {
                Some(k) => set.find(k).cloned(),
                None if set.keys.len() == 1 => set.keys.first().cloned(),
                None => None,
            }
        };
        let mut keys = self.lock();
        let file_changed = self.config.jwks_file.as_ref().is_some_and(|f| {
            std::fs::metadata(f).and_then(|m| m.modified()).ok() != keys.file_time
        });
        let fresh = keys.read.is_some_and(|t| t.elapsed() < KEYS_KEPT) && !file_changed;
        if fresh && let Some(k) = keys.set.as_ref().and_then(find) {
            return Ok(k);
        }
        let may_try = keys.set.is_none()
            || file_changed
            || !fresh
            || keys.tried.is_none_or(|t| t.elapsed() >= REFETCH_EVERY);
        if may_try {
            keys.tried = Some(Instant::now());
            keys.fetches += 1;
            match self.fetch() {
                Ok((set, time)) => {
                    keys.set = Some(set);
                    keys.read = Some(Instant::now());
                    keys.file_time = time;
                }
                Err(e) => {
                    crate::server::log(&format!("OIDC: the provider's keys: {e}"));
                    if keys.set.is_none() {
                        return Err("the sign-in provider's keys could not be read: try again \
                                    later"
                            .into());
                    }
                }
            }
        }
        keys.set
            .as_ref()
            .and_then(find)
            .ok_or_else(|| "the sign-in token's key is not the provider's".to_owned())
    }

    /// Checks `token` now: who signed in, or why not (never the token).
    pub fn verify(&self, token: &str) -> Result<Identity, String> {
        self.verify_at(token, SystemTime::now())
    }

    fn verify_at(&self, token: &str, now: SystemTime) -> Result<Identity, String> {
        let bad = |why: &str| format!("the sign-in token: {why}");
        if token.len() > MAX_TOKEN {
            return Err(bad("too long"));
        }
        let header = jsonwebtoken::decode_header(token).map_err(|_| bad("not a token"))?;
        if !allowed(header.alg) {
            return Err(bad(
                "not signed with a provider's key (HS* and none refused)",
            ));
        }
        let jwk = self.key(header.kid.as_deref())?;
        // The key's own algorithm and kind, when it says them, match the
        // token's; a shared secret is never a key.
        if matches!(jwk.algorithm, AlgorithmParameters::OctetKey(_)) {
            return Err(bad("the provider's key is a shared secret: refused"));
        }
        if let Some(ka) = jwk.common.key_algorithm
            && format!("{ka:?}") != format!("{:?}", header.alg)
        {
            return Err(bad("signed with another algorithm than its key's"));
        }
        let key = DecodingKey::from_jwk(&jwk).map_err(|_| bad("the provider's key is unusable"))?;
        let mut v = Validation::new(header.alg);
        v.set_issuer(&[&self.config.issuer]);
        v.set_audience(&[&self.config.audience]);
        v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        v.leeway = self.config.leeway;
        v.validate_nbf = true;
        let data = jsonwebtoken::decode::<Claims>(token, &key, &v).map_err(|e| {
            use jsonwebtoken::errors::ErrorKind as K;
            bad(match e.kind() {
                K::ExpiredSignature => "expired: sign in again",
                K::ImmatureSignature => "not valid yet",
                K::InvalidIssuer => "of another issuer",
                K::InvalidAudience => "for another application",
                K::InvalidSignature => "its signature does not hold",
                K::MissingRequiredClaim(_) => "a claim is missing",
                _ => "refused",
            })
        })?;
        let now = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        if let Some(iat) = data.claims.iat {
            if iat > now + self.config.leeway {
                return Err(bad("issued in the future"));
            }
            if now.saturating_sub(iat) > self.config.max_age.as_secs() + self.config.leeway {
                return Err(bad("issued too long ago: sign in again"));
            }
        } else {
            return Err(bad("a claim is missing"));
        }
        if data.claims.sub.is_empty() || data.claims.sub.len() > 255 {
            return Err(bad("no subject"));
        }
        Ok(Identity {
            issuer: self.config.issuer.clone(),
            subject: data.claims.sub,
        })
    }
}

/// The kinds of key a JWKS may hold that the server takes (tests,
/// diagnostics).
pub fn usable_keys(set: &JwkSet) -> HashSet<String> {
    set.keys
        .iter()
        .filter(|k| !matches!(k.algorithm, AlgorithmParameters::OctetKey(_)))
        .filter_map(|k| k.common.key_id.clone())
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Base64url without padding.
    pub(crate) fn b64url(data: &[u8]) -> String {
        crate::base64::encode(data)
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_")
    }

    /// A fake identity provider on this computer: its discovery document
    /// and its keys over plain HTTP (no network beyond the loopback), the
    /// requests counted; signs tokens with its EC P-256 key.
    pub(crate) struct FakeIssuer {
        pub(crate) issuer: String,
        key: rcgen::KeyPair,
        pub(crate) kid: String,
        pub(crate) requests: Arc<AtomicUsize>,
        jwks: Arc<Mutex<String>>,
    }

    impl FakeIssuer {
        pub(crate) fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let issuer = format!("http://{}", listener.local_addr().unwrap());
            let key = rcgen::KeyPair::generate().unwrap();
            let kid = "key-1".to_owned();
            let jwks = Arc::new(Mutex::new(Self::jwks_of(&key, &kid)));
            let requests = Arc::new(AtomicUsize::new(0));
            let (iss, j, n) = (issuer.clone(), jwks.clone(), requests.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut s) = stream else { continue };
                    let mut line = String::new();
                    let mut r = BufReader::new(s.try_clone().unwrap());
                    if r.read_line(&mut line).is_err() {
                        continue;
                    }
                    loop {
                        let mut h = String::new();
                        if r.read_line(&mut h).is_err() || h.trim().is_empty() {
                            break;
                        }
                    }
                    n.fetch_add(1, Ordering::SeqCst);
                    let path = line.split(' ').nth(1).unwrap_or_default();
                    let body = match path {
                        "/.well-known/openid-configuration" => serde_json::json!({
                            "issuer": iss, "jwks_uri": format!("{iss}/keys"),
                        })
                        .to_string(),
                        "/keys" => j.lock().unwrap().clone(),
                        _ => String::new(),
                    };
                    let status = if body.is_empty() {
                        "404 Not Found"
                    } else {
                        "200 OK"
                    };
                    let _ = write!(
                        s,
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: \
                         {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            });
            Self {
                issuer,
                key,
                kid,
                requests,
                jwks,
            }
        }

        fn jwks_of(key: &rcgen::KeyPair, kid: &str) -> String {
            let raw = key.public_key_raw();
            assert_eq!(raw.len(), 65);
            serde_json::json!({"keys": [{
                "kty": "EC", "crv": "P-256", "use": "sig", "alg": "ES256", "kid": kid,
                "x": b64url(&raw[1..33]), "y": b64url(&raw[33..]),
            }]})
            .to_string()
        }

        /// The provider's keys turned over (a new key, a new kid).
        pub(crate) fn rotate(&mut self) {
            self.key = rcgen::KeyPair::generate().unwrap();
            self.kid = format!("{}-next", self.kid);
            *self.jwks.lock().unwrap() = Self::jwks_of(&self.key, &self.kid);
        }

        /// A token of `sub` for `aud`, issued `age` s ago, valid `life` s.
        pub(crate) fn token(&self, sub: &str, aud: &str, age: i64, life: i64) -> String {
            self.token_with(
                serde_json::json!({}),
                &self.issuer.clone(),
                sub,
                aud,
                age,
                life,
            )
        }

        pub(crate) fn token_with(
            &self,
            header_extra: serde_json::Value,
            iss: &str,
            sub: &str,
            aud: &str,
            age: i64,
            life: i64,
        ) -> String {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            let mut header = jsonwebtoken::Header::new(Algorithm::ES256);
            header.kid = Some(self.kid.clone());
            if let Some(k) = header_extra.get("kid").and_then(|k| k.as_str()) {
                header.kid = Some(k.to_owned());
            }
            let claims = serde_json::json!({
                "iss": iss, "sub": sub, "aud": aud,
                "iat": now - age, "exp": now - age + life,
            });
            let key = jsonwebtoken::EncodingKey::from_ec_der(&self.key.serialize_der());
            jsonwebtoken::encode(&header, &claims, &key).unwrap()
        }
    }

    #[test]
    fn id_tokens_checked_keys_fetched_and_kept() {
        let mut idp = FakeIssuer::start();
        let oidc = Oidc::new(OidcConfig::new(&idp.issuer, "ohl-server")).unwrap();
        let ok = oidc
            .verify(&idp.token("00u1", "ohl-server", 5, 300))
            .unwrap();
        assert_eq!(
            ok,
            Identity {
                issuer: idp.issuer.clone(),
                subject: "00u1".into()
            }
        );
        // Discovery and keys fetched once, then kept.
        assert_eq!(idp.requests.load(Ordering::SeqCst), 2);
        for _ in 0..5 {
            oidc.verify(&idp.token("00u2", "ohl-server", 1, 300))
                .unwrap();
        }
        assert_eq!(oidc.fetches(), 1);
        let refused = |t: &str| oidc.verify(t).unwrap_err();
        // Expired, not for this server, another issuer, too old, from the
        // future, damaged, unsigned, a made-up key.
        assert!(refused(&idp.token("u", "ohl-server", 600, 300)).contains("expired"));
        assert!(refused(&idp.token("u", "another-app", 1, 300)).contains("another application"));
        let other_iss = idp.token_with(
            serde_json::json!({}),
            "https://evil.example.org",
            "u",
            "ohl-server",
            1,
            300,
        );
        assert!(refused(&other_iss).contains("another issuer"));
        assert!(refused(&idp.token("u", "ohl-server", 7200, 9000)).contains("too long ago"));
        assert!(refused(&idp.token("u", "ohl-server", -600, 9000)).contains("future"));
        let good = idp.token("u", "ohl-server", 1, 300);
        let mut parts: Vec<String> = good.split('.').map(str::to_owned).collect();
        let claims = serde_json::json!({
            "iss": idp.issuer, "sub": "admin", "aud": "ohl-server",
            "iat": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            "exp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 300,
        });
        parts[1] = b64url(claims.to_string().as_bytes());
        assert!(refused(&parts.join(".")).contains("signature"));
        let none = format!("{}.{}.", b64url(br#"{"alg":"none","typ":"JWT"}"#), parts[1]);
        assert!(refused(&none).contains("not a token") || refused(&none).contains("HS*"));
        let hs = format!(
            "{}.{}.{}",
            b64url(br#"{"alg":"HS256","typ":"JWT","kid":"key-1"}"#),
            parts[1],
            b64url(b"sig")
        );
        assert!(refused(&hs).contains("HS*"));
        let before = oidc.fetches();
        let made_up = idp.token_with(
            serde_json::json!({"kid": "made-up"}),
            &idp.issuer.clone(),
            "u",
            "ohl-server",
            1,
            300,
        );
        // A key not held: fetched again once, then not for 30 s.
        assert!(refused(&made_up).contains("not the provider's"));
        assert!(refused(&made_up).contains("not the provider's"));
        assert_eq!(oidc.fetches(), before);
        assert!(refused(&"x".repeat(MAX_TOKEN + 1)).contains("too long"));
        // The provider turns its keys over: a fresh checker fetches them.
        idp.rotate();
        let next = idp.token("00u1", "ohl-server", 1, 300);
        let fresh = Oidc::new(OidcConfig::new(&idp.issuer, "ohl-server")).unwrap();
        assert!(fresh.verify(&next).is_ok());
        // Keys from a file (no fetching), read again when it changes.
        let dir = std::env::temp_dir().join(format!("ohl-oidc-{}", crate::random_code(8)));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("jwks.json");
        std::fs::write(&file, idp.jwks.lock().unwrap().as_bytes()).unwrap();
        let mut c = OidcConfig::new("https://id.example.org", "ohl-server");
        c.jwks_file = Some(file.clone());
        let from_file = Oidc::new(c).unwrap();
        let t = idp.token_with(
            serde_json::json!({}),
            "https://id.example.org",
            "00u9",
            "ohl-server",
            1,
            300,
        );
        assert_eq!(from_file.verify(&t).unwrap().subject, "00u9");
        let _ = std::fs::remove_dir_all(&dir);
        // Plain HTTP only on this computer.
        assert!(Oidc::new(OidcConfig::new("http://id.example.org", "a")).is_err());
        assert!(Oidc::new(OidcConfig::new("https://id.example.org", "")).is_err());
        assert!(format!("{oidc:?}").contains("ohl-server"));
    }
}
