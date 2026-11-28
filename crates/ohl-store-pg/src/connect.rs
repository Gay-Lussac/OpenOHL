// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! How the network's database is reached (ADR-0013, phase 13): TLS to a
//! database on another host, the password kept off the command line.
//!
//! The connection string is PostgreSQL's own (`host=… port=… user=…
//! dbname=…` or `postgresql://…`), with libpq's TLS keys understood here:
//!
//! - `sslmode`: `disable` (no TLS), `require` (encrypted, the server's
//!   certificate not checked), `verify-ca` (its chain checked against
//!   `sslrootcert`), `verify-full` (the chain and the host name);
//!   `prefer`, `allow` or none: **TLS is required for a host that is not
//!   this computer** — the connection is refused, saying how to give TLS
//!   (`sslmode=verify-full sslrootcert=CA.pem`) — and not used for this
//!   computer (`localhost`, a loopback address, a Unix socket).
//!   `sslmode=disable` to another host is the admin's explicit choice (a
//!   network they trust): allowed, said on stderr once per process.
//! - `sslrootcert`: the CA file (PEM) for `verify-ca` and `verify-full`.
//!   No system or bundled roots are used: a country server's database has
//!   the owner's own CA.
//! - The password: in the string (`password=…`), else from the file named
//!   by `OHL_DATABASE_PASSWORD_FILE` (a Compose secret), never printed.
//!
//! TLS is rustls with ring (TLS 1.2 and 1.3), as the server's own.

use std::path::PathBuf;
use std::sync::Arc;

use postgres::config::{Host, SslMode as PgSslMode};
use postgres::{Client, Config, NoTls};

use crate::NetError;

/// The environment variable naming a file holding the database password.
pub const PASSWORD_FILE_ENV: &str = "OHL_DATABASE_PASSWORD_FILE";

/// How TLS is used to reach the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    /// Not given (or `prefer`, `allow`): none for this computer, refused
    /// for another host.
    Auto,
    /// No TLS.
    Disable,
    /// TLS, the server's certificate not checked.
    Require,
    /// TLS, the certificate's chain checked against the CA file.
    VerifyCa,
    /// TLS, the chain and the host name checked.
    VerifyFull,
}

impl SslMode {
    /// Parses libpq's words.
    pub fn parse(text: &str) -> Result<Self, NetError> {
        Ok(match text {
            "disable" => Self::Disable,
            "allow" | "prefer" => Self::Auto,
            "require" => Self::Require,
            "verify-ca" => Self::VerifyCa,
            "verify-full" => Self::VerifyFull,
            other => {
                return Err(NetError::Refused(format!(
                    "sslmode={other}: disable, require, verify-ca or verify-full"
                )));
            }
        })
    }

    /// libpq's word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "prefer",
            Self::Disable => "disable",
            Self::Require => "require",
            Self::VerifyCa => "verify-ca",
            Self::VerifyFull => "verify-full",
        }
    }
}

/// A connection string taken apart: PostgreSQL's settings, and the TLS
/// ones read here.
#[derive(Clone)]
pub struct Target {
    /// The settings (host, port, user, database, password, …).
    pub config: Config,
    /// TLS.
    pub mode: SslMode,
    /// The CA file.
    pub root: Option<PathBuf>,
}

impl std::fmt::Debug for Target {
    /// Without the password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Target")
            .field("hosts", &self.hosts())
            .field("dbname", &self.config.get_dbname())
            .field("user", &self.config.get_user())
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

/// One `key=value` of a connection string (key, value).
type Pair = (String, String);

/// A libpq key/value connection string's pairs (values quoted with `'`
/// and `\` escapes, or bare).
fn key_values(text: &str) -> Result<Vec<Pair>, NetError> {
    let bad = |why: &str| NetError::Refused(format!("the database's connection string: {why}"));
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    loop {
        while it.peek().is_some_and(|c| c.is_whitespace()) {
            it.next();
        }
        if it.peek().is_none() {
            return Ok(out);
        }
        let mut key = String::new();
        while let Some(&c) = it.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            it.next();
        }
        while it.peek().is_some_and(|c| c.is_whitespace()) {
            it.next();
        }
        if it.next() != Some('=') {
            return Err(bad(&format!("\"{key}\" without '='")));
        }
        while it.peek().is_some_and(|c| c.is_whitespace()) {
            it.next();
        }
        let mut value = String::new();
        if it.peek() == Some(&'\'') {
            it.next();
            loop {
                match it.next() {
                    Some('\\') => value.push(it.next().ok_or_else(|| bad("an open quote"))?),
                    Some('\'') => break,
                    Some(c) => value.push(c),
                    None => return Err(bad("an open quote")),
                }
            }
        } else {
            while let Some(&c) = it.peek() {
                if c.is_whitespace() {
                    break;
                }
                it.next();
                if c == '\\' {
                    value.push(it.next().ok_or_else(|| bad("a lone '\\'"))?);
                } else {
                    value.push(c);
                }
            }
        }
        out.push((key, value));
    }
}

/// `value` quoted for a key/value connection string.
fn quoted(value: &str) -> String {
    let mut s = String::from("'");
    for c in value.chars() {
        if c == '\'' || c == '\\' {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('\'');
    s
}

/// Percent-decoding of a URL's query value.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut k = 0;
    let hex = |b: u8| (b as char).to_digit(16);
    while k < bytes.len() {
        if bytes[k] == b'%'
            && k + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[k + 1]), hex(bytes[k + 2]))
        {
            out.push((h * 16 + l) as u8);
            k += 3;
            continue;
        }
        out.push(bytes[k]);
        k += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Target {
    /// Takes `url` apart: `sslmode` and `sslrootcert` read here (the
    /// rest left to PostgreSQL's client), the password from
    /// [`PASSWORD_FILE_ENV`] when the string has none.
    pub fn parse(url: &str) -> Result<Self, NetError> {
        let mut mode = SslMode::Auto;
        let mut root = None;
        let rest = if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            let (base, query) = url.split_once('?').unwrap_or((url, ""));
            let mut kept = Vec::new();
            for pair in query.split('&').filter(|p| !p.is_empty()) {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                match k {
                    "sslmode" => mode = SslMode::parse(&percent_decode(v))?,
                    "sslrootcert" => root = Some(PathBuf::from(percent_decode(v))),
                    _ => kept.push(pair),
                }
            }
            if kept.is_empty() {
                base.to_owned()
            } else {
                format!("{base}?{}", kept.join("&"))
            }
        } else {
            let mut kept = Vec::new();
            for (k, v) in key_values(url)? {
                match k.as_str() {
                    "sslmode" => mode = SslMode::parse(&v)?,
                    "sslrootcert" => root = Some(PathBuf::from(v)),
                    _ => kept.push(format!("{k}={}", quoted(&v))),
                }
            }
            kept.join(" ")
        };
        let mut config: Config = rest.parse().map_err(|e: postgres::Error| {
            NetError::Refused(format!("the database's connection string: {e}"))
        })?;
        if config.get_password().is_none()
            && let Some(file) = std::env::var_os(PASSWORD_FILE_ENV)
        {
            let text = std::fs::read_to_string(&file).map_err(|e| {
                NetError::Refused(format!(
                    "{PASSWORD_FILE_ENV} ({}): {e}",
                    PathBuf::from(&file).display()
                ))
            })?;
            config.password(text.trim_end_matches(['\n', '\r']));
        }
        Ok(Self { config, mode, root })
    }

    /// The hosts named (host names, addresses or socket folders).
    pub fn hosts(&self) -> Vec<String> {
        self.config
            .get_hosts()
            .iter()
            .map(|h| match h {
                Host::Tcp(n) => n.clone(),
                #[cfg(unix)]
                Host::Unix(p) => p.display().to_string(),
            })
            .collect()
    }

    /// Whether every host is this computer: `localhost`, a loopback
    /// address or a Unix socket (none given: PostgreSQL's default, this
    /// computer).
    pub fn is_local(&self) -> bool {
        let addrs = self.config.get_hostaddrs();
        if !addrs.is_empty() {
            return addrs.iter().all(|a| a.is_loopback());
        }
        self.config.get_hosts().iter().all(|h| match h {
            Host::Tcp(n) => {
                n.eq_ignore_ascii_case("localhost")
                    || n.trim_start_matches('[')
                        .trim_end_matches(']')
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            }
            #[cfg(unix)]
            Host::Unix(_) => true,
        })
    }

    /// What TLS the connection uses (the refusal of a host that is not
    /// this computer without TLS).
    pub fn effective_mode(&self) -> Result<SslMode, NetError> {
        match self.mode {
            SslMode::Auto if self.is_local() => Ok(SslMode::Disable),
            SslMode::Auto => Err(NetError::Refused(format!(
                "the database on {} is not on this computer: connect with TLS — sslmode=verify-full \
                 sslrootcert=CA.pem (or sslmode=require: encrypted, its certificate not checked); \
                 sslmode=disable only on a network you trust",
                self.hosts().join(", ")
            ))),
            SslMode::VerifyCa | SslMode::VerifyFull if self.root.is_none() => {
                Err(NetError::Refused(format!(
                    "sslmode={} needs the CA file: sslrootcert=CA.pem",
                    self.mode.as_str()
                )))
            }
            m => Ok(m),
        }
    }

    /// libpq's environment for the PostgreSQL tools (`pg_dump`,
    /// `pg_restore`): the TLS settings and the password, never on their
    /// command line.
    pub fn libpq_env(&self) -> Result<Vec<(String, String)>, NetError> {
        let mode = self.effective_mode()?;
        let mut env = vec![("PGSSLMODE".to_owned(), mode.as_str().to_owned())];
        if let Some(r) = &self.root {
            env.push(("PGSSLROOTCERT".into(), r.display().to_string()));
        }
        if let Some(p) = self.config.get_password() {
            env.push(("PGPASSWORD".into(), String::from_utf8_lossy(p).into_owned()));
        }
        Ok(env)
    }

    /// A key/value connection string for the PostgreSQL tools without the
    /// password and the TLS keys (given by [`Target::libpq_env`]): hosts,
    /// ports, user, database, application name.
    pub fn libpq_string(&self) -> String {
        let mut parts = Vec::new();
        let hosts = self.hosts();
        if !hosts.is_empty() {
            parts.push(format!("host={}", quoted(&hosts.join(","))));
        }
        let ports: Vec<String> = self
            .config
            .get_ports()
            .iter()
            .map(|p| p.to_string())
            .collect();
        if !ports.is_empty() {
            parts.push(format!("port={}", quoted(&ports.join(","))));
        }
        if let Some(u) = self.config.get_user() {
            parts.push(format!("user={}", quoted(u)));
        }
        if let Some(d) = self.config.get_dbname() {
            parts.push(format!("dbname={}", quoted(d)));
        }
        if let Some(a) = self.config.get_application_name() {
            parts.push(format!("application_name={}", quoted(a)));
        }
        parts.join(" ")
    }

    /// Connects.
    pub fn connect(&self) -> Result<Client, NetError> {
        let mode = self.effective_mode()?;
        let mut config = self.config.clone();
        if mode == SslMode::Disable {
            if !self.is_local() {
                warn_plain(&self.hosts().join(", "));
            }
            config.ssl_mode(PgSslMode::Disable);
            return Ok(config.connect(NoTls)?);
        }
        config.ssl_mode(PgSslMode::Require);
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(client_config(
            mode,
            self.root.as_deref(),
        )?);
        Ok(config.connect(tls)?)
    }
}

/// Said once per process: a connection to another host without TLS.
fn warn_plain(hosts: &str) {
    static SAID: std::sync::Once = std::sync::Once::new();
    SAID.call_once(|| {
        eprintln!(
            "ohl: the connection to the database on {hosts} is not encrypted (sslmode=disable)"
        );
    });
}

/// The CA certificates of a PEM file.
fn roots(path: &std::path::Path) -> Result<rustls::RootCertStore, NetError> {
    use rustls_pki_types::CertificateDer;
    use rustls_pki_types::pem::PemObject;
    let bad = |e: String| NetError::Refused(format!("sslrootcert {}: {e}", path.display()));
    let mut store = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_file_iter(path).map_err(|e| bad(e.to_string()))? {
        let cert = cert.map_err(|e| bad(e.to_string()))?;
        store.add(cert).map_err(|e| bad(e.to_string()))?;
    }
    if store.is_empty() {
        return Err(bad("no certificate in it".into()));
    }
    Ok(store)
}

/// rustls's settings for `mode`.
fn client_config(
    mode: SslMode,
    root: Option<&std::path::Path>,
) -> Result<rustls::ClientConfig, NetError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| NetError::Refused(e.to_string()))?;
    let config = match mode {
        SslMode::VerifyFull => builder
            .with_root_certificates(roots(root.expect("checked"))?)
            .with_no_client_auth(),
        SslMode::VerifyCa => {
            let inner = rustls::client::WebPkiServerVerifier::builder_with_provider(
                Arc::new(roots(root.expect("checked"))?),
                provider,
            )
            .build()
            .map_err(|e| NetError::Refused(e.to_string()))?;
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(ChainOnly(inner)))
                .with_no_client_auth()
        }
        _ => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Unchecked(provider)))
            .with_no_client_auth(),
    };
    Ok(config)
}

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{DigitallySignedStruct, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};

/// `verify-ca`: the chain checked, not the name.
#[derive(Debug)]
struct ChainOnly(Arc<rustls::client::WebPkiServerVerifier>);

impl ServerCertVerifier for ChainOnly {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self
            .0
            .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
        {
            Err(rustls::Error::InvalidCertificate(rustls::CertificateError::NotValidForName))
            | Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::NotValidForNameContext { .. },
            )) => Ok(ServerCertVerified::assertion()),
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_verify_schemes()
    }
}

/// `require`: encrypted, the certificate not checked (libpq's meaning);
/// the handshake's signatures still are.
#[derive(Debug)]
struct Unchecked(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for Unchecked {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// `url` with the password read from `file` added (2026-11-28: OHL-Server's
/// host mode names the password file, as [`PASSWORD_FILE_ENV`] does for the
/// command line, without setting the program's environment): unchanged when
/// the string has a password already. The string returned holds the
/// password: never show or log it.
pub fn with_password_file(url: &str, file: &std::path::Path) -> Result<String, NetError> {
    if Target::parse(url)?.config.get_password().is_some() {
        return Ok(url.to_owned());
    }
    let text = std::fs::read_to_string(file)
        .map_err(|e| NetError::Refused(format!("the password file ({}): {e}", file.display())))?;
    let password = text.trim_end_matches(['\n', '\r']);
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        let mut s = url.to_owned();
        s.push(if s.contains('?') { '&' } else { '?' });
        s.push_str("password=");
        for b in password.bytes() {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                s.push(b as char);
            } else {
                s.push_str(&format!("%{b:02X}"));
            }
        }
        Ok(s)
    } else {
        Ok(format!("{url} password={}", quoted(password)))
    }
}

/// `url` with TLS settings added (the command line's `--db-sslmode` and
/// `--db-ca`): a key/value string gets `sslmode=…` and `sslrootcert=…`,
/// a URL the same in its query.
pub fn with_tls(url: &str, mode: Option<&str>, ca: Option<&std::path::Path>) -> String {
    let mut pairs = Vec::new();
    if let Some(m) = mode {
        pairs.push(("sslmode", m.to_owned()));
    }
    if let Some(c) = ca {
        pairs.push(("sslrootcert", c.display().to_string()));
    }
    if pairs.is_empty() {
        return url.to_owned();
    }
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        let mut s = url.to_owned();
        for (k, v) in pairs {
            s.push(if s.contains('?') { '&' } else { '?' });
            s.push_str(k);
            s.push('=');
            for b in v.bytes() {
                if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
                    s.push(b as char);
                } else {
                    s.push_str(&format!("%{b:02X}"));
                }
            }
        }
        s
    } else {
        let mut s = url.to_owned();
        for (k, v) in pairs {
            s.push(' ');
            s.push_str(k);
            s.push('=');
            s.push_str(&quoted(&v));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_file_goes_into_the_string_only_when_it_has_none() {
        let dir = std::env::temp_dir().join(format!("ohl-pw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("pw");
        std::fs::write(&f, "s'e cr&t\n").unwrap();
        let kv = with_password_file("host=127.0.0.1 user=ohl dbname=net", &f).unwrap();
        assert_eq!(
            Target::parse(&kv).unwrap().config.get_password(),
            Some(&b"s'e cr&t"[..])
        );
        let url = with_password_file("postgresql://ohl@127.0.0.1/net", &f).unwrap();
        assert_eq!(
            Target::parse(&url).unwrap().config.get_password(),
            Some(&b"s'e cr&t"[..])
        );
        let kept = "host=127.0.0.1 user=ohl dbname=net password=x";
        assert_eq!(with_password_file(kept, &f).unwrap(), kept);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tls_keys_read_and_a_remote_host_refused_without_tls() {
        let t = Target::parse("host=127.0.0.1 port=5432 user=ohl dbname=net").unwrap();
        assert!(t.is_local());
        assert_eq!(t.effective_mode().unwrap(), SslMode::Disable);
        for local in [
            "host=localhost dbname=n",
            "host=::1 dbname=n",
            "host=/tmp dbname=n",
            "postgresql://ohl@127.0.0.1:5432/net",
        ] {
            assert!(Target::parse(local).unwrap().is_local(), "{local}");
        }
        // Another host: refused without TLS, saying how.
        let remote = Target::parse("host=db.example.org user=ohl dbname=net").unwrap();
        assert!(!remote.is_local());
        let e = remote.effective_mode().unwrap_err().to_string();
        assert!(
            e.contains("not on this computer") && e.contains("verify-full"),
            "{e}"
        );
        let e = remote.connect().err().unwrap().to_string();
        assert!(e.contains("not on this computer"), "{e}");
        // `prefer` is libpq's default: the same.
        assert!(
            Target::parse("host=10.0.0.5 sslmode=prefer dbname=n")
                .unwrap()
                .effective_mode()
                .is_err()
        );
        // verify-full needs its CA file.
        let e = Target::parse("host=db.example.org sslmode=verify-full dbname=n")
            .unwrap()
            .effective_mode()
            .unwrap_err()
            .to_string();
        assert!(e.contains("sslrootcert"), "{e}");
        let t = Target::parse(
            "host=db.example.org sslmode=verify-full sslrootcert='/etc/ohl/ca file.pem' dbname=n",
        )
        .unwrap();
        assert_eq!(t.effective_mode().unwrap(), SslMode::VerifyFull);
        assert_eq!(
            t.root.as_deref(),
            Some(std::path::Path::new("/etc/ohl/ca file.pem"))
        );
        // In a URL's query too; other keys kept.
        let t = Target::parse(
            "postgresql://ohl:secret@db.example.org/net?sslmode=require&connect_timeout=5",
        )
        .unwrap();
        assert_eq!(t.effective_mode().unwrap(), SslMode::Require);
        assert_eq!(
            t.config.get_connect_timeout(),
            Some(&std::time::Duration::from_secs(5))
        );
        // The explicit choice of no TLS to another host: allowed.
        assert_eq!(
            Target::parse("host=10.0.0.5 sslmode=disable dbname=n")
                .unwrap()
                .effective_mode()
                .unwrap(),
            SslMode::Disable
        );
        assert!(Target::parse("host=x sslmode=sometimes").is_err());
    }

    #[test]
    fn the_password_never_shown_nor_given_on_a_command_line() {
        let t =
            Target::parse("host=db.example.org user=ohl password='s3 cr\\'et' dbname=net").unwrap();
        assert_eq!(t.config.get_password(), Some(&b"s3 cr'et"[..]));
        let shown = format!("{t:?}");
        assert!(!shown.contains("s3"), "{shown}");
        let s = t.libpq_string();
        assert!(
            !s.contains("s3") && s.contains("host='db.example.org'"),
            "{s}"
        );
        assert!(Target::parse(&s).is_ok());
        let t = Target::parse(&with_tls(
            "host=db.example.org user=ohl password=pw dbname=net",
            Some("verify-full"),
            Some(std::path::Path::new("/ca.pem")),
        ))
        .unwrap();
        let env = t.libpq_env().unwrap();
        assert!(env.contains(&("PGPASSWORD".into(), "pw".into())));
        assert!(env.contains(&("PGSSLMODE".into(), "verify-full".into())));
        assert!(env.contains(&("PGSSLROOTCERT".into(), "/ca.pem".into())));
        let u = with_tls(
            "postgresql://ohl@db.example.org/net",
            Some("verify-full"),
            Some(std::path::Path::new("/a b/ca.pem")),
        );
        let t = Target::parse(&u).unwrap();
        assert_eq!(t.root.as_deref(), Some(std::path::Path::new("/a b/ca.pem")));
    }
}
